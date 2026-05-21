//! CI-result-→-tracker-state feedback loop (plan §5.2).
//!
//! [`evaluate_ci`] is the front door. It's called from the
//! `check_suite` / `workflow_run` webhook handlers with the parsed
//! event payload. It:
//!
//! 1. Pulls the repo + head SHA + candidate PR numbers from the
//!    payload.
//! 2. For each PR, looks up the linked ticket via the P1-E
//!    `pr_ticket_map`. PRs without a mapping are silently skipped.
//! 3. Asks the github client for the aggregated check-run summary on
//!    that head SHA.
//! 4. Dispatches to one of three transition paths in
//!    [`transition`]: green, red-below-cap, red-at-cap.
//!
//! The orchestrator is intentionally JSON-shaped — both `check_suite`
//! and `workflow_run` payloads land here with the same structural keys
//! (`pull_requests`, `head_sha`, `repository.full_name`). When P1-G
//! adds the App-mode github client, this layer doesn't change.

pub mod attempts;
pub mod categorize;
pub mod transition;

use crate::github::{CheckRunSummary, GhOps};
use crate::labels::LabelManager;
use crate::storage::Store;
use crate::{BridgeConfig, Result};
use liquid::ParserBuilder;
use serde_json::Value;
use sinfonia_tracker::IssueTracker;
use std::sync::Arc;
use tracing::{debug, info, warn};

use attempts::{read_and_decide, AttemptDecision};
use transition::RedContext;

/// Top-level outcome of one [`evaluate_ci`] call. Returned to the
/// webhook handler so it can pick a status code + JSON body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiOutcome {
    /// No PR with a matching tracker mapping was found on this event.
    /// 200 OK; no further action.
    NoMappedPr,
    /// At least one check is still running. 200 OK; the next
    /// `check_suite` redelivery will re-evaluate.
    Pending,
    /// CI completed green. The `awaiting-review` label was applied;
    /// no tracker writes. 202 OK.
    Green,
    /// CI completed red below the cap. Counter incremented, ticket
    /// transitioned to the category target state, labels updated, PR
    /// commented. 202 OK.
    Red {
        category: String,
        next_attempt: u32,
        max_attempts: u32,
        target_state: String,
    },
    /// CI red and the cap was hit. Ticket transitioned to
    /// `blocked_state`; counter NOT incremented past the cap. 202 OK.
    CapHit { stayed_at: u32, max: u32 },
}

/// Per-event dependencies the orchestrator needs.
///
/// Pulled out of `AppState` so the orchestrator's signature is
/// uniform across `check_suite` / `workflow_run` and so future tests
/// can construct it without going through axum.
pub struct EvaluateContext<'a> {
    pub config: &'a BridgeConfig,
    pub store: &'a Store,
    pub tracker: &'a dyn IssueTracker,
    pub gh: &'a Arc<dyn GhOps>,
    pub labels: &'a LabelManager,
}

/// Entry point. Returns the per-PR outcomes (most events touch a
/// single PR, but check_suite/workflow_run can list multiple).
pub async fn evaluate_ci(
    ctx: EvaluateContext<'_>,
    event: &str,
    payload: &Value,
) -> Result<Vec<CiOutcome>> {
    let (repo, head_sha, pr_numbers) = extract_targets(event, payload);

    let Some(repo) = repo else {
        debug!(target: "feedback", event, "payload missing repository.full_name");
        return Ok(vec![CiOutcome::NoMappedPr]);
    };
    let Some(head_sha) = head_sha else {
        debug!(target: "feedback", event, repo, "payload missing head_sha");
        return Ok(vec![CiOutcome::NoMappedPr]);
    };
    if pr_numbers.is_empty() {
        debug!(target: "feedback", event, repo, head_sha, "payload has no pull_requests; nothing to do");
        return Ok(vec![CiOutcome::NoMappedPr]);
    }

    // Fetch the summary once; it's the same for every PR on this SHA.
    let summary = match ctx.gh.list_check_run_summary(&repo, &head_sha).await {
        Ok(s) => s,
        Err(e) => {
            warn!(target: "feedback", error = %e, repo, head_sha, "list_check_run_summary failed");
            return Err(e);
        }
    };

    if summary.any_pending {
        debug!(target: "feedback", repo, head_sha, "checks still pending; waiting for next event");
        return Ok(vec![CiOutcome::Pending]);
    }

    // Ensure base labels exist for this repo lazily — first event per
    // repo. The LabelManager short-circuits when `manage_labels: false`.
    if let Err(e) = ctx.labels.ensure_base_set(&repo).await {
        warn!(target: "feedback", error = %e, repo, "ensure_base_set failed (continuing)");
    }

    let mut outcomes = Vec::with_capacity(pr_numbers.len());
    for pr_number in pr_numbers {
        let outcome = evaluate_one_pr(&ctx, &repo, &head_sha, pr_number, &summary).await?;
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

async fn evaluate_one_pr(
    ctx: &EvaluateContext<'_>,
    repo: &str,
    _head_sha: &str,
    pr_number: u64,
    summary: &CheckRunSummary,
) -> Result<CiOutcome> {
    let ticket_id = match ctx.store.lookup_pr_ticket(repo, pr_number).await? {
        Some(id) => id,
        None => {
            debug!(target: "feedback", repo, pr_number, "no tracker mapping; skipping");
            return Ok(CiOutcome::NoMappedPr);
        }
    };

    if summary.all_passed() {
        transition::apply_green(ctx.labels, repo, pr_number).await?;
        return Ok(CiOutcome::Green);
    }

    if !summary.has_failed() {
        // No failed checks AND not all_passed means: zero completed
        // checks (or only "skipped" without explicit pass). Treat as
        // pending rather than green — we don't want to declare victory
        // on an empty suite.
        debug!(target: "feedback", repo, pr_number, "no failed and no passed runs; treating as pending");
        return Ok(CiOutcome::Pending);
    }

    // ---- Red path ----------------------------------------------------
    let (_prior, max_attempts, decision) = read_and_decide(
        ctx.tracker,
        &ticket_id,
        &ctx.config.custom_fields.attempt_count,
        &ctx.config.custom_fields.max_attempts_override,
        ctx.config.feedback_loop.max_attempts,
    )
    .await?;

    let category =
        categorize::categorize(&summary.failed, &ctx.config.feedback_loop.failure_categories)
            .clone();

    let pr_url = format!("https://github.com/{repo}/pull/{pr_number}");
    let failure_summary = render_failure_summary(&summary.failed);
    // Phase 1 limitation: we don't fetch check-run logs. The template
    // sees a placeholder; plan §11.6 question 6 deferred the log-size
    // knob, and the fetcher itself is documented in `01-bridge-mvp.md`
    // §5.4 as P1-F scope — left as a follow-up alongside the rest of
    // the budget/telemetry work in Phase 3.
    let failure_log_excerpt = format!(
        "(log excerpt not yet fetched; see PR {pr_url}/checks for full logs)"
    );

    let red_ctx = RedContext {
        repo,
        pr_number,
        pr_url: &pr_url,
        ticket_id: &ticket_id,
        ticket_identifier: &ticket_id,
        failed_checks: &summary.failed,
        failure_log_excerpt: &failure_log_excerpt,
    };

    match decision {
        AttemptDecision::Continue { next } => {
            let rendered = render_failure_comment(
                &ctx.config.feedback_loop.failure_comment_template,
                next,
                max_attempts,
                &category.name,
                &summary.failed,
                &failure_log_excerpt,
                &pr_url,
                &ticket_id,
            );
            transition::apply_red_below_cap(
                ctx.tracker,
                ctx.labels,
                ctx.gh.as_ref(),
                &red_ctx,
                &category,
                next,
                max_attempts,
                &ctx.config.custom_fields,
                &rendered,
                &failure_summary,
            )
            .await?;
            info!(
                target: "feedback",
                repo,
                pr_number,
                ticket = %ticket_id,
                category = %category.name,
                next,
                max = max_attempts,
                "red below cap"
            );
            Ok(CiOutcome::Red {
                category: category.name.clone(),
                next_attempt: next,
                max_attempts,
                target_state: category.target_state.clone(),
            })
        }
        AttemptDecision::CapHit { stayed_at, max } => {
            let rendered = render_cap_hit_comment(
                stayed_at,
                max,
                &category.name,
                &summary.failed,
                &pr_url,
                &ticket_id,
            );
            transition::apply_cap_hit(
                ctx.tracker,
                ctx.labels,
                ctx.gh.as_ref(),
                &red_ctx,
                &decision,
                &ctx.config.feedback_loop.blocked_state,
                &rendered,
            )
            .await?;
            Ok(CiOutcome::CapHit { stayed_at, max })
        }
    }
}

/// Pull `(repo, head_sha, pr_numbers)` out of a `check_suite` or
/// `workflow_run` payload. Both events nest the same shape under
/// different keys.
fn extract_targets(event: &str, payload: &Value) -> (Option<String>, Option<String>, Vec<u64>) {
    let key = match event {
        "check_suite" => "check_suite",
        "workflow_run" => "workflow_run",
        _ => return (None, None, vec![]),
    };
    let envelope = payload.get(key);
    let head_sha = envelope
        .and_then(|e| e.get("head_sha"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let repo = payload
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let pr_numbers: Vec<u64> = envelope
        .and_then(|e| e.get("pull_requests"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|pr| pr.get("number").and_then(|n| n.as_u64()))
                .collect()
        })
        .unwrap_or_default();
    (repo, head_sha, pr_numbers)
}

fn render_failure_summary(failed: &[String]) -> String {
    if failed.is_empty() {
        "(no failed checks)".into()
    } else {
        failed.join(", ")
    }
}

#[allow(clippy::too_many_arguments)]
fn render_failure_comment(
    template_src: &str,
    attempt: u32,
    max_attempts: u32,
    failure_category: &str,
    failed_checks: &[String],
    failure_log_excerpt: &str,
    pr_url: &str,
    ticket_identifier: &str,
) -> String {
    render_template_or_fallback(template_src, |obj| {
        obj.insert("attempt".into(), liquid::model::Value::scalar(attempt as i64));
        obj.insert(
            "max_attempts".into(),
            liquid::model::Value::scalar(max_attempts as i64),
        );
        obj.insert(
            "failed_checks".into(),
            liquid::model::Value::scalar(failed_checks.join(", ")),
        );
        obj.insert(
            "failure_log_excerpt".into(),
            liquid::model::Value::scalar(failure_log_excerpt.to_string()),
        );
        obj.insert(
            "failure_category".into(),
            liquid::model::Value::scalar(failure_category.to_string()),
        );
        obj.insert(
            "pr_url".into(),
            liquid::model::Value::scalar(pr_url.to_string()),
        );
        obj.insert(
            "ticket_identifier".into(),
            liquid::model::Value::scalar(ticket_identifier.to_string()),
        );
    })
}

fn render_cap_hit_comment(
    stayed_at: u32,
    max: u32,
    failure_category: &str,
    failed_checks: &[String],
    pr_url: &str,
    ticket_identifier: &str,
) -> String {
    format!(
        "Sinfonia has hit the configured retry cap ({stayed_at}/{max}) for {ticket_identifier} \
         while attempting CI on {pr_url}. Last failure category: `{failure_category}`. \
         Failed checks: {}. The ticket has been moved to the bridge's blocked state for \
         human review.",
        failed_checks.join(", ")
    )
}

fn render_template_or_fallback<F>(template_src: &str, populate: F) -> String
where
    F: FnOnce(&mut liquid::Object),
{
    let parser = match ParserBuilder::with_stdlib().build() {
        Ok(p) => p,
        Err(e) => {
            warn!(target: "feedback", error = %e, "liquid parser build failed; using fallback");
            return template_src.to_string();
        }
    };
    let tpl = match parser.parse(template_src) {
        Ok(t) => t,
        Err(e) => {
            warn!(target: "feedback", error = %e, "failure_comment_template parse failed; using raw template");
            return template_src.to_string();
        }
    };
    let mut obj = liquid::Object::new();
    populate(&mut obj);
    match tpl.render(&obj) {
        Ok(s) => s,
        Err(e) => {
            warn!(target: "feedback", error = %e, "failure_comment_template render failed; using raw template");
            template_src.to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_targets_pulls_check_suite_fields() {
        let payload = json!({
            "action": "completed",
            "check_suite": {
                "head_sha": "abc123",
                "pull_requests": [{"number": 7}, {"number": 8}],
            },
            "repository": {"full_name": "acme/widgets"},
        });
        let (repo, sha, prs) = extract_targets("check_suite", &payload);
        assert_eq!(repo.as_deref(), Some("acme/widgets"));
        assert_eq!(sha.as_deref(), Some("abc123"));
        assert_eq!(prs, vec![7, 8]);
    }

    #[test]
    fn extract_targets_pulls_workflow_run_fields() {
        let payload = json!({
            "action": "completed",
            "workflow_run": {
                "head_sha": "deadbeef",
                "pull_requests": [{"number": 42}],
            },
            "repository": {"full_name": "acme/widgets"},
        });
        let (repo, sha, prs) = extract_targets("workflow_run", &payload);
        assert_eq!(repo.as_deref(), Some("acme/widgets"));
        assert_eq!(sha.as_deref(), Some("deadbeef"));
        assert_eq!(prs, vec![42]);
    }

    #[test]
    fn extract_targets_handles_missing_pull_requests() {
        let payload = json!({
            "check_suite": {"head_sha": "x"},
            "repository": {"full_name": "a/b"},
        });
        let (_, _, prs) = extract_targets("check_suite", &payload);
        assert!(prs.is_empty());
    }

    #[test]
    fn render_failure_comment_substitutes_variables() {
        let template = "CI failed on attempt {{ attempt }} of {{ max_attempts }}. \
                        Category: {{ failure_category }}. Failed: {{ failed_checks }}. \
                        PR: {{ pr_url }}. Ticket: {{ ticket_identifier }}.";
        let rendered = render_failure_comment(
            template,
            2,
            5,
            "lint",
            &["unit/lint".to_string(), "unit/clippy".to_string()],
            "(no log)",
            "https://github.com/acme/widgets/pull/42",
            "ENG-7",
        );
        assert!(rendered.contains("attempt 2 of 5"));
        assert!(rendered.contains("Category: lint"));
        assert!(rendered.contains("unit/lint, unit/clippy"));
        assert!(rendered.contains("https://github.com/acme/widgets/pull/42"));
        assert!(rendered.contains("ENG-7"));
    }
}
