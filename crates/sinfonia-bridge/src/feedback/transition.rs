//! State-transition execution (plan §5.2).
//!
//! Each of the three top-level outcomes — green, red-below-cap,
//! red-at-cap — is one async function here. They take everything they
//! need by `&` parameter (no orchestrator state mutation), perform the
//! tracker writes + label updates + PR comment, and return.
//!
//! The functions are deliberately verbose about the order of side
//! effects: tracker writes happen BEFORE label/comment writes, so a
//! mid-call failure leaves the tracker correctly state-machined even
//! if the PR comment fails. (The reverse — labels first, transition
//! second — could orphan a "needs-fixes" label on a green PR if the
//! transition fails.)

use crate::config::FailureCategory;
use crate::feedback::attempts::AttemptDecision;
use crate::github::GhOps;
use crate::labels::{BridgeLabel, LabelManager};
use crate::telemetry::spans;
use crate::{Error, Result};
use sinfonia_tracker::{CustomFieldValue, IssueTracker};
use tracing::{info, info_span, warn, Instrument};

/// Inputs the red paths need that aren't already obvious from the
/// signature.
pub struct RedContext<'a> {
    pub repo: &'a str,
    pub pr_number: u64,
    pub pr_url: &'a str,
    pub ticket_id: &'a str,
    pub ticket_identifier: &'a str,
    pub failed_checks: &'a [String],
    pub failure_log_excerpt: &'a str,
}

/// Apply the green-CI transitions: optionally promote the ticket into the
/// configured review state, then tag the PR as `awaiting-review` and clear
/// `in-progress` / `needs-fixes`.
///
/// When `awaiting_review_state` is `Some`, the bridge is the authoritative
/// gate that moves a ticket into human review — it does so ONLY here, after
/// the expected checks are confirmed green (see `evaluate_one_pr`). The
/// tracker write happens BEFORE labels (mirroring the red path): a green PR
/// must never be left un-promoted just because a label call hiccuped. When
/// `None`, this is label-only — the legacy behaviour, where the agent owns
/// the review-state transition.
pub async fn apply_green(
    tracker: &dyn IssueTracker,
    labels: &LabelManager,
    repo: &str,
    pr_number: u64,
    ticket_id: &str,
    awaiting_review_state: Option<&str>,
) -> Result<()> {
    if let Some(state) = awaiting_review_state {
        let span = info_span!(
            target: "feedback",
            spans::BRIDGE_STATE_TRANSITION,
            { spans::ATTR_TICKET_ID } = ticket_id,
            { spans::ATTR_TO_STATE } = state,
            { spans::ATTR_REASON } = spans::REASON_CI_GREEN,
        );
        async {
            tracker
                .transition_issue(ticket_id, state)
                .await
                .map_err(Error::Tracker)
        }
        .instrument(span)
        .await?;
        info!(
            target: "feedback",
            repo, pr_number, ticket_id, to_state = state,
            "green: ticket promoted to review state"
        );
    }

    labels
        .apply(repo, pr_number, &BridgeLabel::AwaitingReview)
        .await?;
    labels
        .remove(repo, pr_number, &BridgeLabel::InProgress)
        .await?;
    labels
        .remove(repo, pr_number, &BridgeLabel::NeedsFixes)
        .await?;
    info!(target: "feedback", repo, pr_number, "green: awaiting-review applied");
    Ok(())
}

/// Apply the red-below-cap transitions:
///
/// 1. Write `sinfonia_last_ci_failure`, `sinfonia_failure_category`,
///    `sinfonia_attempt_count` on the ticket.
/// 2. Transition the ticket to the category's `target_state` (or to
///    the configured `needs_fixes_state` if `default` matched).
/// 3. Apply `needs-fixes` + `failure:<category>` labels; remove
///    `in-progress`.
/// 4. Post the rendered `failure_comment_template` to the PR.
#[allow(clippy::too_many_arguments)]
pub async fn apply_red_below_cap(
    tracker: &dyn IssueTracker,
    labels: &LabelManager,
    gh: &dyn GhOps,
    ctx: &RedContext<'_>,
    category: &FailureCategory,
    next_attempt: u32,
    max_attempts: u32,
    custom_fields: &crate::config::CustomFieldsSection,
    rendered_comment: &str,
    failure_summary: &str,
) -> Result<()> {
    let span = info_span!(
        target: "feedback",
        spans::BRIDGE_STATE_TRANSITION,
        { spans::ATTR_TICKET_ID } = ctx.ticket_id,
        { spans::ATTR_TO_STATE } = %category.target_state,
        { spans::ATTR_REASON } = spans::REASON_CI_FAILURE,
        { spans::ATTR_ATTEMPT_COUNT } = next_attempt,
    );
    apply_red_below_cap_inner(
        tracker,
        labels,
        gh,
        ctx,
        category,
        next_attempt,
        max_attempts,
        custom_fields,
        rendered_comment,
        failure_summary,
    )
    .instrument(span)
    .await
}

#[allow(clippy::too_many_arguments)]
async fn apply_red_below_cap_inner(
    tracker: &dyn IssueTracker,
    labels: &LabelManager,
    gh: &dyn GhOps,
    ctx: &RedContext<'_>,
    category: &FailureCategory,
    next_attempt: u32,
    max_attempts: u32,
    custom_fields: &crate::config::CustomFieldsSection,
    rendered_comment: &str,
    failure_summary: &str,
) -> Result<()> {
    // 1. Tracker writes — order matters: write the failure context
    //    BEFORE we transition the ticket so Sinfonia, polling the next
    //    `Needs Fixes` state, always sees a populated context.
    tracker
        .write_custom_field(
            ctx.ticket_id,
            &custom_fields.last_failure_log,
            CustomFieldValue::String(failure_summary.to_string()),
        )
        .await
        .map_err(|e| Error::Tracker(e))?;
    tracker
        .write_custom_field(
            ctx.ticket_id,
            &custom_fields.failure_category,
            CustomFieldValue::String(category.name.clone()),
        )
        .await
        .map_err(|e| Error::Tracker(e))?;
    tracker
        .write_custom_field(
            ctx.ticket_id,
            &custom_fields.attempt_count,
            CustomFieldValue::Number(next_attempt as f64),
        )
        .await
        .map_err(|e| Error::Tracker(e))?;
    tracker
        .transition_issue(ctx.ticket_id, &category.target_state)
        .await
        .map_err(|e| Error::Tracker(e))?;

    // 2. Labels. We tolerate label failures so a transient GitHub hiccup
    //    doesn't undo the successful tracker transition.
    if let Err(e) = labels
        .apply(ctx.repo, ctx.pr_number, &BridgeLabel::NeedsFixes)
        .await
    {
        warn!(target: "feedback", error = %e, "needs-fixes label apply failed (continuing)");
    }
    if let Err(e) = labels
        .apply(
            ctx.repo,
            ctx.pr_number,
            &BridgeLabel::Failure(category.name.clone()),
        )
        .await
    {
        warn!(target: "feedback", error = %e, "failure-category label apply failed (continuing)");
    }
    if let Err(e) = labels
        .remove(ctx.repo, ctx.pr_number, &BridgeLabel::InProgress)
        .await
    {
        warn!(target: "feedback", error = %e, "in-progress label remove failed (continuing)");
    }
    if let Err(e) = labels
        .remove(ctx.repo, ctx.pr_number, &BridgeLabel::AwaitingReview)
        .await
    {
        warn!(target: "feedback", error = %e, "awaiting-review label remove failed (continuing)");
    }

    // 3. PR comment.
    if let Err(e) = gh
        .post_pr_comment(ctx.repo, ctx.pr_number, rendered_comment)
        .await
    {
        warn!(target: "feedback", error = %e, "failure comment post failed (continuing)");
    }

    info!(
        target: "feedback",
        repo = ctx.repo,
        pr = ctx.pr_number,
        ticket = ctx.ticket_id,
        category = %category.name,
        attempt = next_attempt,
        max = max_attempts,
        target_state = %category.target_state,
        "red: transitioned to needs-fixes state"
    );
    Ok(())
}

/// Apply the cap-hit transitions:
///
/// 1. Transition the ticket to `blocked_state` (no counter increment —
///    `decision` is `AttemptDecision::CapHit { stayed_at, max }`).
/// 2. Apply `cap-hit`; remove `in-progress` and `needs-fixes`.
/// 3. Post the rendered cap-explanation comment to the PR.
pub async fn apply_cap_hit(
    tracker: &dyn IssueTracker,
    labels: &LabelManager,
    gh: &dyn GhOps,
    ctx: &RedContext<'_>,
    decision: &AttemptDecision,
    blocked_state: &str,
    rendered_comment: &str,
) -> Result<()> {
    let (stayed_at, max) = match decision {
        AttemptDecision::CapHit { stayed_at, max } => (*stayed_at, *max),
        _ => (0, 0), // re-checked below; placeholder so the spans see real values
    };
    let cap_span = info_span!(
        target: "feedback",
        spans::BRIDGE_CAP_HIT,
        { spans::ATTR_TICKET_ID } = ctx.ticket_id,
        { spans::ATTR_CAP_KIND } = spans::CAP_KIND_ATTEMPTS,
        { spans::ATTR_FINAL_ATTEMPT_COUNT } = stayed_at,
    );
    let transition_span = info_span!(
        target: "feedback",
        spans::BRIDGE_STATE_TRANSITION,
        { spans::ATTR_TICKET_ID } = ctx.ticket_id,
        { spans::ATTR_TO_STATE } = blocked_state,
        { spans::ATTR_REASON } = spans::REASON_CAP_HIT,
        { spans::ATTR_FINAL_ATTEMPT_COUNT } = stayed_at,
    );
    let _ = max; // claimed in trace below
    apply_cap_hit_inner(tracker, labels, gh, ctx, decision, blocked_state, rendered_comment)
        .instrument(cap_span.clone())
        .instrument(transition_span)
        .await
}

async fn apply_cap_hit_inner(
    tracker: &dyn IssueTracker,
    labels: &LabelManager,
    gh: &dyn GhOps,
    ctx: &RedContext<'_>,
    decision: &AttemptDecision,
    blocked_state: &str,
    rendered_comment: &str,
) -> Result<()> {
    let (stayed_at, max) = match decision {
        AttemptDecision::CapHit { stayed_at, max } => (*stayed_at, *max),
        AttemptDecision::Continue { .. } => {
            // Caller bug — apply_cap_hit MUST be called with a CapHit
            // decision. Refuse to write anything rather than papering
            // over it.
            return Err(Error::Other(
                "apply_cap_hit called with Continue decision (caller bug)".into(),
            ));
        }
    };

    tracker
        .transition_issue(ctx.ticket_id, blocked_state)
        .await
        .map_err(|e| Error::Tracker(e))?;

    if let Err(e) = labels
        .apply(ctx.repo, ctx.pr_number, &BridgeLabel::CapHit)
        .await
    {
        warn!(target: "feedback", error = %e, "cap-hit label apply failed (continuing)");
    }
    if let Err(e) = labels
        .remove(ctx.repo, ctx.pr_number, &BridgeLabel::InProgress)
        .await
    {
        warn!(target: "feedback", error = %e, "in-progress label remove failed (continuing)");
    }
    if let Err(e) = labels
        .remove(ctx.repo, ctx.pr_number, &BridgeLabel::NeedsFixes)
        .await
    {
        warn!(target: "feedback", error = %e, "needs-fixes label remove failed (continuing)");
    }
    if let Err(e) = gh
        .post_pr_comment(ctx.repo, ctx.pr_number, rendered_comment)
        .await
    {
        warn!(target: "feedback", error = %e, "cap-hit comment post failed (continuing)");
    }

    info!(
        target: "feedback",
        repo = ctx.repo,
        pr = ctx.pr_number,
        ticket = ctx.ticket_id,
        stayed_at,
        max,
        target_state = blocked_state,
        "cap-hit: transitioned to blocked_state; counter not advanced"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::apply_green;
    use crate::config::LabelAliases;
    use crate::github::{ArtifactMeta, CheckRunSummary, GhOps};
    use crate::labels::LabelManager;
    use crate::Result as BridgeResult;
    use async_trait::async_trait;
    use sinfonia_tracker::{
        CustomFieldSchema, CustomFieldValue, Issue, IssueState, IssueTracker,
        Result as TrackerResult,
    };
    use std::sync::{Arc, Mutex};

    /// Tracker that records every `transition_issue(id, target)` call.
    #[derive(Default)]
    struct RecordingTracker {
        transitions: Mutex<Vec<(String, String)>>,
    }

    #[async_trait]
    impl IssueTracker for RecordingTracker {
        async fn fetch_candidate_issues(&self) -> TrackerResult<Vec<Issue>> {
            Ok(vec![])
        }
        async fn fetch_issues_by_states(&self, _: &[String]) -> TrackerResult<Vec<Issue>> {
            Ok(vec![])
        }
        async fn fetch_issue_states_by_ids(&self, _: &[String]) -> TrackerResult<Vec<IssueState>> {
            Ok(vec![])
        }
        async fn read_custom_field(&self, _: &str, _: &str) -> TrackerResult<CustomFieldValue> {
            Ok(CustomFieldValue::Null)
        }
        async fn write_custom_field(
            &self,
            _: &str,
            _: &str,
            _: CustomFieldValue,
        ) -> TrackerResult<()> {
            Ok(())
        }
        async fn ensure_custom_field(&self, _: &CustomFieldSchema) -> TrackerResult<()> {
            Ok(())
        }
        async fn post_comment(&self, _: &str, _: &str) -> TrackerResult<()> {
            Ok(())
        }
        async fn transition_issue(&self, id: &str, target: &str) -> TrackerResult<()> {
            self.transitions
                .lock()
                .unwrap()
                .push((id.to_string(), target.to_string()));
            Ok(())
        }
    }

    /// GhOps that does nothing — the LabelManager below is disabled, so its
    /// label calls short-circuit before reaching this.
    struct NoopGh;

    #[async_trait]
    impl GhOps for NoopGh {
        async fn ensure_label(&self, _: &str, _: &str, _: &str, _: &str) -> BridgeResult<()> {
            Ok(())
        }
        async fn apply_label_to_pr(&self, _: &str, _: u64, _: &str) -> BridgeResult<()> {
            Ok(())
        }
        async fn remove_label_from_pr(&self, _: &str, _: u64, _: &str) -> BridgeResult<()> {
            Ok(())
        }
        async fn post_pr_comment(&self, _: &str, _: u64, _: &str) -> BridgeResult<()> {
            Ok(())
        }
        async fn list_check_run_summary(&self, _: &str, _: &str) -> BridgeResult<CheckRunSummary> {
            Ok(CheckRunSummary::default())
        }
        async fn whoami(&self) -> BridgeResult<String> {
            Ok("noop".into())
        }
        async fn list_run_artifacts(&self, _: &str, _: u64) -> BridgeResult<Vec<ArtifactMeta>> {
            Ok(vec![])
        }
        async fn download_artifact(&self, _: &str, _: u64, _: u64) -> BridgeResult<Vec<u8>> {
            Ok(vec![])
        }
    }

    fn disabled_labels() -> LabelManager {
        LabelManager::new(Arc::new(NoopGh), false, "sinfonia:", LabelAliases::default())
    }

    #[tokio::test]
    async fn apply_green_transitions_when_review_state_configured() {
        let tracker = RecordingTracker::default();
        let labels = disabled_labels();
        apply_green(&tracker, &labels, "acme/widgets", 7, "TICKET-1", Some("In Review"))
            .await
            .expect("apply_green ok");
        assert_eq!(
            tracker.transitions.lock().unwrap().as_slice(),
            &[("TICKET-1".to_string(), "In Review".to_string())],
            "green must promote the ticket into the configured review state"
        );
    }

    #[tokio::test]
    async fn apply_green_is_label_only_when_no_review_state() {
        let tracker = RecordingTracker::default();
        let labels = disabled_labels();
        apply_green(&tracker, &labels, "acme/widgets", 7, "TICKET-1", None)
            .await
            .expect("apply_green ok");
        assert!(
            tracker.transitions.lock().unwrap().is_empty(),
            "legacy behaviour: no tracker write when awaiting_review_state is unset"
        );
    }
}
