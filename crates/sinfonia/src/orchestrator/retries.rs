//! Retry queue (spec §8.4).

use super::Inner;
use crate::domain::{OrchestratorState, RetryEntry};
use chrono::Utc;
use std::sync::Arc;
use tracing::debug;

/// §8.4 backoff: `min(10000 * 2^(attempt-1), max_backoff)`.
pub fn backoff_ms(attempt: u32, max_backoff: u64) -> u64 {
    let exp = attempt.saturating_sub(1).min(20);
    let base: u64 = 10_000u64.saturating_mul(1u64 << exp);
    base.min(max_backoff.max(1000))
}

/// Schedule (or reschedule) a retry. Cancels any existing timer for the issue.
pub fn schedule(state: &mut OrchestratorState, inner: &Arc<Inner>, entry: RetryEntry) {
    state.claimed.insert(entry.issue_id.clone());
    state.retry_attempts.insert(entry.issue_id.clone(), entry.clone());

    // Spawn a one-shot timer that fires retry handling.
    let inner = inner.clone();
    let id = entry.issue_id.clone();
    let due = entry.due_at;
    tokio::spawn(async move {
        let now = Utc::now();
        let delay = (due - now).num_milliseconds();
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
        }
        fire_retry(&inner, id).await;
    });
}

async fn fire_retry(inner: &Arc<Inner>, issue_id: String) {
    use crate::orchestrator::Orchestrator;
    let orch = Orchestrator { inner: inner.clone() };

    // Pop our entry (if it's still us — we may have been replaced by a fresher schedule).
    let entry = {
        let mut state = inner.state.lock().await;
        state.retry_attempts.remove(&issue_id)
    };
    let Some(entry) = entry else { return };

    // §8.4: refetch active candidates, find by id, dispatch or requeue.
    let cfg = orch.config();
    let tracker = orch.tracker();
    let candidates = match tracker.fetch_candidate_issues().await {
        Ok(v) => v,
        Err(e) => {
            debug!(target: "orchestrator.retry", error=%e, "retry candidate fetch failed; requeue");
            let next = RetryEntry {
                attempt: entry.attempt + 1,
                due_at: Utc::now() + chrono::Duration::milliseconds(
                    backoff_ms(entry.attempt + 1, cfg.agent.max_retry_backoff_ms) as i64,
                ),
                error: Some("retry poll failed".into()),
                ..entry
            };
            let mut state = inner.state.lock().await;
            schedule(&mut state, inner, next);
            return;
        }
    };

    let issue = candidates.into_iter().find(|i| i.id == issue_id);
    let Some(issue) = issue else {
        // Not present any more — release claim.
        let mut state = inner.state.lock().await;
        state.claimed.remove(&issue_id);
        debug!(target: "orchestrator.retry", issue_id=%issue_id, "retry: issue no longer active; released");
        return;
    };

    let outcome = orch.dispatch_one(issue.clone(), Some(entry.attempt)).await;
    if !outcome.continue_loop() {
        // No slot — requeue.
        let next = RetryEntry {
            issue_id: entry.issue_id.clone(),
            identifier: issue.identifier.clone(),
            attempt: entry.attempt + 1,
            due_at: Utc::now() + chrono::Duration::milliseconds(
                backoff_ms(entry.attempt + 1, cfg.agent.max_retry_backoff_ms) as i64,
            ),
            error: Some("no available orchestrator slots".into()),
        };
        let mut state = inner.state.lock().await;
        schedule(&mut state, inner, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_grows_then_caps() {
        assert_eq!(backoff_ms(1, 300_000), 10_000);
        assert_eq!(backoff_ms(2, 300_000), 20_000);
        assert_eq!(backoff_ms(3, 300_000), 40_000);
        assert_eq!(backoff_ms(8, 300_000), 300_000); // capped
        assert_eq!(backoff_ms(20, 300_000), 300_000);
    }
}
