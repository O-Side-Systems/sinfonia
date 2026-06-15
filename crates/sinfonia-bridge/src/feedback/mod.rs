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
pub mod budget;
pub mod categorize;
pub mod cost;
pub mod manifest;
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

impl<'a> EvaluateContext<'a> {
    /// Borrow these same dependencies as a merge-coordinator [`Ctx`]
    /// (Proposal 0005). Lets the green/red feedback branches hand the
    /// coordinator its context without re-plumbing `AppState`.
    fn merge_ctx(&self) -> crate::merge::Ctx<'_> {
        crate::merge::Ctx {
            config: self.config,
            store: self.store,
            tracker: self.tracker,
            gh: self.gh,
            labels: self.labels,
        }
    }
}

/// Entry point. Returns the per-PR outcomes (most events touch a
/// single PR, but check_suite/workflow_run can list multiple).
pub async fn evaluate_ci(
    ctx: EvaluateContext<'_>,
    event: &str,
    payload: &Value,
) -> Result<Vec<CiOutcome>> {
    let EventTargets {
        repo,
        head_sha,
        pr_numbers,
        run_id,
    } = extract_targets(event, payload);

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
        let outcome =
            evaluate_one_pr(&ctx, &repo, &head_sha, pr_number, &summary, run_id).await?;
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
    run_id: Option<u64>,
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
        // Merge coordinator (Proposal 0005): if this PR is an approved,
        // in-flight landing, a green head may now be mergeable (or the
        // update-branch re-test just completed). No-op unless enabled + queued;
        // failures here must not fail the CI webhook, so they are logged only.
        if ctx.config.feedback_loop.merge_coordinator.enabled {
            if let Err(e) = crate::merge::on_ci_green(&ctx.merge_ctx(), repo, pr_number).await {
                warn!(target: "feedback", error = %e, repo, pr_number, "merge coordinator on_ci_green failed (continuing)");
            }
        }
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

    // Default (check-name) feedback content — the degradation floor. The
    // field carries the comma-joined check names; the comment excerpt is a
    // placeholder pointing at the PR's checks tab. (Generic check-run log
    // fetching, the original P1-F TODO, remains out of scope — see
    // Proposal 0001 §3.2/§7.)
    let mut failure_summary = render_failure_summary(&summary.failed);
    let mut failure_log_excerpt =
        format!("(log excerpt not yet fetched; see PR {pr_url}/checks for full logs)");

    // Optional enrichment (Proposal 0001): when harness-manifest ingestion
    // is enabled and a `workflow_run` id is available, fold the structured
    // `bridge.json` digest into BOTH the `sinfonia_last_ci_failure` field
    // (the retry-turn diagnostic channel, §12) and the comment excerpt.
    // Degrade-only: any miss keeps the floor above.
    let hm = &ctx.config.feedback_loop.harness_manifest;
    if hm.ingest {
        if let Some(run_id) = run_id {
            if let Some(m) =
                manifest::try_fetch_manifest(ctx.gh.as_ref(), repo, run_id, hm).await
            {
                let run_ref = m.run_url.clone().unwrap_or_else(|| pr_url.clone());
                let digest = manifest::build_failure_digest(&m, &run_ref, hm);
                info!(
                    target: "feedback",
                    repo, pr_number, run_id,
                    scenarios = m.total_failures,
                    "harness manifest ingested; folding structured digest into sinfonia_last_ci_failure"
                );
                failure_summary = digest.clone();
                failure_log_excerpt = digest;
            }
        }
    }

    let red_ctx = RedContext {
        repo,
        pr_number,
        pr_url: &pr_url,
        ticket_id: &ticket_id,
        ticket_identifier: &ticket_id,
        failed_checks: &summary.failed,
        failure_log_excerpt: &failure_log_excerpt,
    };

    let outcome = match decision {
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
            CiOutcome::Red {
                category: category.name.clone(),
                next_attempt: next,
                max_attempts,
                target_state: category.target_state.clone(),
            }
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
            CiOutcome::CapHit { stayed_at, max }
        }
    };

    // Merge coordinator (Proposal 0005): red CI means the agent loop owns the
    // fix now, so relinquish any in-flight landing for this PR. No-op unless
    // enabled + queued; failures are logged, never propagated.
    if ctx.config.feedback_loop.merge_coordinator.enabled {
        if let Err(e) = crate::merge::on_ci_red(&ctx.merge_ctx(), repo, pr_number).await {
            warn!(target: "feedback", error = %e, repo, pr_number, "merge coordinator on_ci_red failed (continuing)");
        }
    }

    Ok(outcome)
}

/// The targets the feedback loop pulls out of a `check_suite` or
/// `workflow_run` payload before evaluating it.
///
/// `run_id` is the one field that differs by event: the GitHub Actions
/// artifacts API is keyed by *workflow run id*, which is present on a
/// `workflow_run` payload (`workflow_run.id`) but not on `check_suite`.
/// It is therefore `Some` only for `workflow_run` events and gates the
/// harness-manifest ingestion path (Proposal 0001 §4.1).
#[derive(Debug, Default, PartialEq, Eq)]
struct EventTargets {
    repo: Option<String>,
    head_sha: Option<String>,
    pr_numbers: Vec<u64>,
    run_id: Option<u64>,
}

/// Pull the [`EventTargets`] out of a `check_suite` or `workflow_run`
/// payload. Both events nest the same shape under different keys.
fn extract_targets(event: &str, payload: &Value) -> EventTargets {
    let key = match event {
        "check_suite" => "check_suite",
        "workflow_run" => "workflow_run",
        _ => return EventTargets::default(),
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
    // Only `workflow_run` carries a run id; `check_suite` leaves it `None`
    // and the caller degrades to the check-name path.
    let run_id = match event {
        "workflow_run" => envelope.and_then(|e| e.get("id")).and_then(|v| v.as_u64()),
        _ => None,
    };
    EventTargets {
        repo,
        head_sha,
        pr_numbers,
        run_id,
    }
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
        let t = extract_targets("check_suite", &payload);
        assert_eq!(t.repo.as_deref(), Some("acme/widgets"));
        assert_eq!(t.head_sha.as_deref(), Some("abc123"));
        assert_eq!(t.pr_numbers, vec![7, 8]);
        // check_suite carries no run id — the manifest path stays off.
        assert_eq!(t.run_id, None);
    }

    #[test]
    fn extract_targets_pulls_workflow_run_fields() {
        let payload = json!({
            "action": "completed",
            "workflow_run": {
                "id": 1820934,
                "head_sha": "deadbeef",
                "pull_requests": [{"number": 42}],
            },
            "repository": {"full_name": "acme/widgets"},
        });
        let t = extract_targets("workflow_run", &payload);
        assert_eq!(t.repo.as_deref(), Some("acme/widgets"));
        assert_eq!(t.head_sha.as_deref(), Some("deadbeef"));
        assert_eq!(t.pr_numbers, vec![42]);
        // workflow_run carries the run id used to fetch artifacts.
        assert_eq!(t.run_id, Some(1820934));
    }

    #[test]
    fn extract_targets_workflow_run_missing_id_tolerated() {
        // A malformed workflow_run with no `id` must not panic; run_id
        // simply stays None and the caller degrades.
        let payload = json!({
            "action": "completed",
            "workflow_run": {
                "head_sha": "deadbeef",
                "pull_requests": [{"number": 42}],
            },
            "repository": {"full_name": "acme/widgets"},
        });
        let t = extract_targets("workflow_run", &payload);
        assert_eq!(t.run_id, None);
        assert_eq!(t.pr_numbers, vec![42]);
    }

    #[test]
    fn extract_targets_handles_missing_pull_requests() {
        let payload = json!({
            "check_suite": {"head_sha": "x"},
            "repository": {"full_name": "a/b"},
        });
        let t = extract_targets("check_suite", &payload);
        assert!(t.pr_numbers.is_empty());
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

    #[test]
    fn manifest_digest_is_not_evaluated_as_a_template() {
        // A fork-controlled `assertion` containing Liquid and Markdown must
        // render LITERALLY: the digest enters the failure comment as a
        // scalar value, never as template source (Proposal 0001 §5).
        use crate::config::HarnessManifestSection;
        use manifest::{ArtifactUrls, Failure, Manifest};

        let hostile = Failure {
            scenario: "Injection attempt".into(),
            feature_file: None,
            step: Some("When the attacker controls the assertion".into()),
            // Liquid expression + a Markdown link with a javascript: scheme.
            assertion: Some("{{ 7*7 }} and [click](javascript:alert(1))".into()),
            artifact_urls: Some(ArtifactUrls::default()),
        };
        let m = Manifest {
            schema_version: 2,
            run_url: Some("https://example/run".into()),
            artifact_bundle_name: None,
            failures: vec![hostile],
            total_failures: 1,
        };
        let digest = manifest::build_failure_digest(
            &m,
            "https://example/run",
            &HarnessManifestSection::default(),
        );

        // The digest goes into the comment as the failure_log_excerpt
        // scalar; the template references it via {{ failure_log_excerpt }}.
        let template = "Failure:\n{{ failure_log_excerpt }}";
        let rendered = render_failure_comment(
            template,
            1,
            5,
            "e2e",
            &["e2e/login".to_string()],
            &digest,
            "https://github.com/acme/widgets/pull/42",
            "ENG-7",
        );

        // The Liquid is present verbatim and was NOT evaluated to 49.
        assert!(
            rendered.contains("{{ 7*7 }}"),
            "Liquid must render literally; got: {rendered}"
        );
        assert!(
            !rendered.contains("49"),
            "Liquid must not be evaluated; got: {rendered}"
        );
        // The Markdown link text survives verbatim (no comment-escape).
        assert!(rendered.contains("[click](javascript:alert(1))"));
    }
}
