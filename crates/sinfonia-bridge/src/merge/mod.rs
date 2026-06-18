//! Sinfonia-native merge coordinator (Proposal 0005, Phase 3).
//!
//! The coordinator turns "this PR is approved and was green" into "this PR is
//! green **against the `main` it will actually land on**, and is now merged" —
//! the guarantee a GitHub native merge queue gives, but available on any
//! GitHub tier (the native queue is Enterprise-Cloud-only for private repos).
//!
//! It lives in the bridge, not the daemon: the daemon holds zero GitHub
//! credentials by invariant (SPEC §11.6.1 / §15.1), and merging needs them.
//! Because the bridge is a thin HTTP client with **no git checkout**, the
//! base-sync step is GitHub's `update-branch` (a merge-from-base), not a true
//! rebase; the final merge still uses the configured method (default
//! `rebase`) so `main` stays linear (§8.2 / §8.3).
//!
//! ## Shape — event-driven, serialized, crash-safe
//!
//! There is no background loop. Every step is driven by a webhook and the
//! landing's durable row in [`Store`]'s `landing_queue`:
//!
//! - **Enqueue** ([`enqueue_on_approval`]) — a `pull_request_review`
//!   *submitted/approved* event for a tracker-linked PR inserts a landing row
//!   (idempotently). The row's existence **is** the "human approved" marker —
//!   the coordinator never self-approves (§8.6).
//! - **Advance** ([`kick`] → [`advance_one`]) — re-reads the PR from GitHub and
//!   takes exactly one action: `update-branch` if `BEHIND`, merge if green +
//!   mergeable, park if `DIRTY`/over-budget, or wait. Re-invoked on the green
//!   CI webhook ([`on_ci_green`]) for the new head after each `update-branch`.
//! - **Serialization** — only the oldest landing per repo (`head_of_queue`)
//!   ever acts; v1 lands one PR at a time, matching the serial-foundation
//!   invariant. When the head terminates, [`kick`] advances the next one.
//! - **Park** ([`park`]) — on conflict, exhausted update cycles, or a closed
//!   PR, the ticket is transitioned to the configured needs-fixes state via the
//!   same tracker write the red-CI path uses, then the row is dequeued so a
//!   future re-approval re-enqueues cleanly (§8.5).
//! - **Boot reconcile** ([`reconcile_on_boot`]) — before the webhook server
//!   binds, every in-flight row is re-checked against GitHub's actual state so
//!   an out-of-band human merge can't cause a double-merge (§8.4).
//!
//! Every entry point is a no-op when `feedback_loop.merge_coordinator.enabled`
//! is `false` (the default), so the whole feature ships dark.

use crate::config::BridgeConfig;
use crate::github::{GhOps, MergeOutcome, PrLanding, UpdateBranchOutcome};
use crate::labels::{BridgeLabel, LabelManager};
use crate::storage::{LandingRow, LandingStatus, Store};
use crate::{Error, Result};
use sinfonia_tracker::{CustomFieldValue, IssueTracker};
use std::collections::BTreeSet;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Per-call dependencies the coordinator needs. Mirrors
/// [`crate::feedback::EvaluateContext`] so the green/red feedback hooks can
/// hand their borrow straight through, while the webhook + boot paths build
/// one from `AppState`.
pub struct Ctx<'a> {
    pub config: &'a BridgeConfig,
    pub store: &'a Store,
    pub tracker: &'a dyn IssueTracker,
    pub gh: &'a Arc<dyn GhOps>,
    pub labels: &'a LabelManager,
}

impl Ctx<'_> {
    fn enabled(&self) -> bool {
        self.config.feedback_loop.merge_coordinator.enabled
    }
}

/// What the PR's current `mergeable_state` means for the coordinator's next
/// action. The REST strings are the lowercase of GraphQL `mergeStateStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mergeability {
    /// `clean` / `has_hooks` / `blocked` / `unstable` — eligible to merge once
    /// the harness gate is re-confirmed green against this head. `blocked` and
    /// `unstable` are §7.4's "ready-for-human" states; the compare-and-set
    /// `merge_pr` + the green re-check are the real guards.
    Ready,
    /// `behind` — needs a base-sync (`update-branch`) before it can land.
    Behind,
    /// `dirty` — merge conflict with the base; park to the agent loop.
    Dirty,
    /// `draft` / `unknown` — GitHub is still computing mergeability (or the PR
    /// is a draft); wait for the next webhook rather than guessing.
    Wait,
}

fn classify(mergeable_state: &str) -> Mergeability {
    match mergeable_state {
        "behind" => Mergeability::Behind,
        "dirty" => Mergeability::Dirty,
        "draft" | "unknown" => Mergeability::Wait,
        _ => Mergeability::Ready,
    }
}

/// Enqueue a PR for landing in response to a human approval (§8.6). Idempotent
/// — a re-fired approval does not restart an in-flight landing. After
/// enqueuing, attempts to make immediate progress on the repo's queue head.
pub async fn enqueue_on_approval(ctx: &Ctx<'_>, repo: &str, pr_number: u64) -> Result<()> {
    if !ctx.enabled() {
        return Ok(());
    }
    let Some(ticket_id) = ctx.store.lookup_pr_ticket(repo, pr_number).await? else {
        debug!(target: "merge", repo, pr_number, "approved PR has no tracker mapping; ignoring");
        return Ok(());
    };
    let pr = ctx.gh.get_pull_request(repo, pr_number).await?;
    if pr.merged {
        debug!(target: "merge", repo, pr_number, "approved PR is already merged; not enqueuing");
        return Ok(());
    }
    ctx.store
        .enqueue_landing(repo, pr_number, &ticket_id, &pr.head_sha)
        .await?;
    info!(
        target: "merge",
        repo, pr_number, ticket = %ticket_id, head = %pr.head_sha,
        "landing enqueued on approval"
    );
    kick(ctx, repo).await;
    Ok(())
}

/// Green-CI hook (called from the feedback loop's `all_passed` branch). Only
/// acts when the PR already has a landing row — i.e. it was approved — so a
/// green PR that was never approved stays exactly today's no-op. Drives the
/// repo's queue head forward (the green head may now be mergeable, or the
/// update-branch re-test may have just completed).
pub async fn on_ci_green(ctx: &Ctx<'_>, repo: &str, pr_number: u64) -> Result<()> {
    if !ctx.enabled() {
        return Ok(());
    }
    if ctx.store.get_landing(repo, pr_number).await?.is_none() {
        return Ok(());
    }
    kick(ctx, repo).await;
    Ok(())
}

/// Red-CI hook (called from the feedback loop's red / cap-hit branches). The
/// red path already transitioned the ticket to needs-fixes; the coordinator
/// just relinquishes the landing so a future re-approval re-enqueues, then
/// lets the next queued PR proceed if this one was the head.
pub async fn on_ci_red(ctx: &Ctx<'_>, repo: &str, pr_number: u64) -> Result<()> {
    if !ctx.enabled() {
        return Ok(());
    }
    if ctx.store.get_landing(repo, pr_number).await?.is_some() {
        info!(
            target: "merge",
            repo, pr_number,
            "red CI on a queued PR; dequeuing landing (agent loop owns the fix now)"
        );
        ctx.store.dequeue_landing(repo, pr_number).await?;
    }
    kick(ctx, repo).await;
    Ok(())
}

/// Reconcile every in-flight landing against GitHub's actual state, then make
/// progress on each repo's head. Run once at boot, before the webhook server
/// binds, so an out-of-band human merge that happened while the bridge was
/// down cannot lead to a double-merge (§8.4).
pub async fn reconcile_on_boot(ctx: &Ctx<'_>) -> Result<()> {
    if !ctx.enabled() {
        return Ok(());
    }
    let landings = ctx.store.list_landings().await?;
    if landings.is_empty() {
        debug!(target: "merge", "boot reconcile: landing queue empty");
        return Ok(());
    }
    info!(target: "merge", count = landings.len(), "boot reconcile: re-checking in-flight landings");

    let mut repos: BTreeSet<String> = BTreeSet::new();
    for l in &landings {
        repos.insert(l.repo.clone());
        match ctx.gh.get_pull_request(&l.repo, l.pr_number).await {
            Ok(pr) => {
                if pr.merged || pr.state == "closed" {
                    info!(
                        target: "merge",
                        repo = %l.repo, pr_number = l.pr_number, merged = pr.merged,
                        "boot reconcile: PR is merged/closed out-of-band; dequeuing"
                    );
                    ctx.store.dequeue_landing(&l.repo, l.pr_number).await?;
                } else if pr.head_sha != l.head_sha {
                    info!(
                        target: "merge",
                        repo = %l.repo, pr_number = l.pr_number,
                        old = %l.head_sha, new = %pr.head_sha,
                        "boot reconcile: head moved under us; re-arming awaiting_ci"
                    );
                    ctx.store
                        .advance_landing(
                            &l.repo,
                            l.pr_number,
                            LandingStatus::AwaitingCi,
                            &pr.head_sha,
                            l.attempt,
                        )
                        .await?;
                }
                // else: head unchanged and still open — resume as stored.
            }
            Err(e) => {
                // Leave the row for the runtime path to retry; a transient API
                // error at boot must not silently drop a landing.
                warn!(
                    target: "merge",
                    repo = %l.repo, pr_number = l.pr_number, error = %e,
                    "boot reconcile: get_pull_request failed; leaving row for runtime"
                );
            }
        }
    }

    for repo in repos {
        kick(ctx, &repo).await;
    }
    Ok(())
}

/// Drive the repo's queue head forward, one landing at a time, until the head
/// blocks waiting on an external signal (a re-test, GitHub still computing
/// mergeability) or the queue is empty. When a head terminates (merged or
/// parked → dequeued), the next-oldest becomes head and the loop continues —
/// this is the serialization seam (§3: "one in-flight at a time"). Errors are
/// logged and stop the loop; the next webhook re-enters.
pub async fn kick(ctx: &Ctx<'_>, repo: &str) {
    loop {
        let head = match head_of_queue(ctx, repo).await {
            Ok(Some(h)) => h,
            Ok(None) => break,
            Err(e) => {
                warn!(target: "merge", repo, error = %e, "kick: head_of_queue failed");
                break;
            }
        };
        match advance_one(ctx, &head).await {
            Ok(true) => continue, // head terminated; advance the next one
            Ok(false) => break,   // head is waiting on an external signal
            Err(e) => {
                warn!(
                    target: "merge",
                    repo, pr_number = head.pr_number, error = %e,
                    "kick: advance_one failed; will retry on the next webhook"
                );
                break;
            }
        }
    }
}

/// The oldest landing in `repo` (FIFO by `updated_at`), or `None` if the repo
/// has no rows. `list_landings` is already globally oldest-first.
async fn head_of_queue(ctx: &Ctx<'_>, repo: &str) -> Result<Option<LandingRow>> {
    Ok(ctx
        .store
        .list_landings()
        .await?
        .into_iter()
        .find(|l| l.repo == repo))
}

/// Take exactly one action on a single landing. Returns `Ok(true)` when the
/// landing row was removed from the queue (merged, abandoned, or parked) so the
/// caller advances the next one; `Ok(false)` when the row remains and the
/// landing is now waiting on an external signal.
async fn advance_one(ctx: &Ctx<'_>, landing: &LandingRow) -> Result<bool> {
    let repo = landing.repo.as_str();
    let pr_number = landing.pr_number;

    let pr = ctx.gh.get_pull_request(repo, pr_number).await?;

    if pr.merged {
        info!(target: "merge", repo, pr_number, "PR already merged (out-of-band); dequeuing");
        ctx.store.dequeue_landing(repo, pr_number).await?;
        return Ok(true);
    }
    if pr.state == "closed" {
        info!(target: "merge", repo, pr_number, "PR closed without merge; dequeuing");
        ctx.store.dequeue_landing(repo, pr_number).await?;
        return Ok(true);
    }

    match classify(&pr.mergeable_state) {
        Mergeability::Dirty => {
            park(ctx, landing, "merge conflict with the base branch (DIRTY)").await?;
            Ok(true)
        }
        Mergeability::Behind => advance_behind(ctx, landing, &pr).await,
        Mergeability::Wait => {
            debug!(
                target: "merge",
                repo, pr_number, state = %pr.mergeable_state,
                "mergeability still computing; waiting"
            );
            ctx.store
                .advance_landing(
                    repo,
                    pr_number,
                    LandingStatus::AwaitingCi,
                    &pr.head_sha,
                    landing.attempt,
                )
                .await?;
            Ok(false)
        }
        Mergeability::Ready => advance_ready(ctx, landing, &pr).await,
    }
}

/// `BEHIND`: integrate the latest base via `update-branch`, bounded by
/// `max_update_cycles`. The new head arrives via the `pull_request synchronize`
/// → `check_suite` webhook, which re-enters [`on_ci_green`] → [`kick`].
async fn advance_behind(ctx: &Ctx<'_>, landing: &LandingRow, pr: &PrLanding) -> Result<bool> {
    let repo = landing.repo.as_str();
    let pr_number = landing.pr_number;
    let max_cycles = ctx.config.feedback_loop.merge_coordinator.max_update_cycles;

    if landing.attempt >= max_cycles {
        park(
            ctx,
            landing,
            &format!("exceeded max_update_cycles ({max_cycles}) integrating the base branch"),
        )
        .await?;
        return Ok(true);
    }

    match ctx.gh.update_pr_branch(repo, pr_number, &pr.head_sha).await? {
        UpdateBranchOutcome::Accepted => {
            // The new head SHA arrives via webhook — we never guess it.
            ctx.store
                .advance_landing(
                    repo,
                    pr_number,
                    LandingStatus::Updating,
                    &pr.head_sha,
                    landing.attempt + 1,
                )
                .await?;
            info!(
                target: "merge",
                repo, pr_number, cycle = landing.attempt + 1,
                "update-branch accepted; awaiting CI on the integrated head"
            );
            Ok(false)
        }
        UpdateBranchOutcome::Stale => {
            // The head moved under us; re-read on the next webhook (no cycle
            // is consumed — nothing was integrated).
            debug!(target: "merge", repo, pr_number, "update-branch stale; will re-read");
            ctx.store
                .advance_landing(
                    repo,
                    pr_number,
                    LandingStatus::AwaitingCi,
                    &pr.head_sha,
                    landing.attempt,
                )
                .await?;
            Ok(false)
        }
        UpdateBranchOutcome::Conflict => {
            park(ctx, landing, "branch could not be updated against the base (conflict)").await?;
            Ok(true)
        }
    }
}

/// `Ready`: re-confirm the harness gate is green against this exact head (§1 —
/// "green against the `main` it will actually land on"), then merge with a
/// `head_sha` compare-and-set.
async fn advance_ready(ctx: &Ctx<'_>, landing: &LandingRow, pr: &PrLanding) -> Result<bool> {
    let repo = landing.repo.as_str();
    let pr_number = landing.pr_number;

    let summary = ctx.gh.list_check_run_summary(repo, &pr.head_sha).await?;
    if summary.any_pending {
        debug!(target: "merge", repo, pr_number, "checks still pending on head; waiting");
        ctx.store
            .advance_landing(
                repo,
                pr_number,
                LandingStatus::AwaitingCi,
                &pr.head_sha,
                landing.attempt,
            )
            .await?;
        return Ok(false);
    }
    if !summary.all_passed() {
        // Red against the integrated base. The feedback loop's own red path
        // owns the needs-fixes transition; relinquish so re-approval re-enqueues.
        info!(
            target: "merge",
            repo, pr_number,
            "re-test on the integrated head is not green; relinquishing to the agent loop"
        );
        ctx.store.dequeue_landing(repo, pr_number).await?;
        return Ok(true);
    }

    let method = ctx
        .config
        .feedback_loop
        .merge_coordinator
        .merge_method
        .as_str();
    ctx.store
        .advance_landing(
            repo,
            pr_number,
            LandingStatus::Merging,
            &pr.head_sha,
            landing.attempt,
        )
        .await?;
    match ctx.gh.merge_pr(repo, pr_number, method, &pr.head_sha).await? {
        MergeOutcome::Merged { sha } => {
            info!(target: "merge", repo, pr_number, %sha, method, "PR landed");
            ctx.store.dequeue_landing(repo, pr_number).await?;
            Ok(true)
        }
        MergeOutcome::NotMergeable => {
            // 405: not mergeable right now (e.g. a required check flipped).
            // Step back to awaiting; a later webhook re-enters.
            warn!(target: "merge", repo, pr_number, "merge returned not-mergeable (405); awaiting re-check");
            ctx.store
                .advance_landing(
                    repo,
                    pr_number,
                    LandingStatus::AwaitingCi,
                    &pr.head_sha,
                    landing.attempt,
                )
                .await?;
            Ok(false)
        }
        MergeOutcome::ShaMismatch => {
            // 409: the head moved between our read and the merge; the
            // synchronize webhook for the new head will re-enter.
            debug!(target: "merge", repo, pr_number, "merge sha mismatch (409); awaiting new head");
            ctx.store
                .advance_landing(
                    repo,
                    pr_number,
                    LandingStatus::AwaitingCi,
                    &pr.head_sha,
                    landing.attempt,
                )
                .await?;
            Ok(false)
        }
    }
}

/// Park a landing back to the agent loop: write the reason into the failure
/// context custom field, transition the ticket to the configured needs-fixes
/// state, best-effort label + comment, and dequeue the row so a future
/// re-approval re-enqueues cleanly. Tracker writes happen before label/comment
/// writes, mirroring [`crate::feedback::transition`]'s ordering so a mid-call
/// failure still leaves the tracker correctly state-machined.
async fn park(ctx: &Ctx<'_>, landing: &LandingRow, reason: &str) -> Result<()> {
    let repo = landing.repo.as_str();
    let pr_number = landing.pr_number;
    let ticket_id = landing.ticket_id.as_str();
    let needs_fixes_state = &ctx.config.feedback_loop.needs_fixes_state;

    ctx.tracker
        .write_custom_field(
            ticket_id,
            &ctx.config.custom_fields.last_failure_log,
            CustomFieldValue::String(format!("Merge coordinator parked this PR: {reason}")),
        )
        .await
        .map_err(Error::Tracker)?;
    ctx.tracker
        .transition_issue(ticket_id, needs_fixes_state)
        .await
        .map_err(Error::Tracker)?;

    let comment = format!(
        "Sinfonia's merge coordinator could not land this PR: {reason}. The ticket has been \
         moved to **{needs_fixes_state}** so the agent loop can pick it back up; re-approval will \
         re-enqueue it once the issue is resolved."
    );
    if let Err(e) = ctx
        .labels
        .apply(repo, pr_number, &BridgeLabel::NeedsFixes)
        .await
    {
        warn!(target: "merge", error = %e, "park: needs-fixes label apply failed (continuing)");
    }
    if let Err(e) = ctx
        .labels
        .remove(repo, pr_number, &BridgeLabel::AwaitingReview)
        .await
    {
        warn!(target: "merge", error = %e, "park: awaiting-review label remove failed (continuing)");
    }
    if let Err(e) = ctx.gh.post_pr_comment(repo, pr_number, &comment).await {
        warn!(target: "merge", error = %e, "park: comment post failed (continuing)");
    }

    ctx.store.dequeue_landing(repo, pr_number).await?;
    info!(
        target: "merge",
        repo, pr_number, ticket = %ticket_id, state = %needs_fixes_state,
        reason, "landing parked to the agent loop"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
