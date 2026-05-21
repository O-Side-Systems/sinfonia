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
use crate::{Error, Result};
use sinfonia_tracker::{CustomFieldValue, IssueTracker};
use tracing::{info, warn};

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

/// Apply the green-CI transitions: tag the PR as `awaiting-review`
/// and clear `in-progress` / `needs-fixes`. No tracker writes.
pub async fn apply_green(labels: &LabelManager, repo: &str, pr_number: u64) -> Result<()> {
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
