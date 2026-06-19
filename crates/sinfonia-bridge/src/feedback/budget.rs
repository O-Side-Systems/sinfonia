//! Budget enforcement + 30 s flush debounce (Phase 3 §7.3 + §7.4).
//!
//! ## Why a debounce?
//!
//! Linear's "custom field" is a single bot-owned comment per issue; every
//! write to it is a GraphQL read-modify-write of that one comment. A busy
//! ticket emits 3-5 `runner.session.completed` events per minute. Writing
//! the comment on every event burns Linear API budget on values nobody
//! reads until the next agent dispatch. Coalescing to a 30 s idle window
//! drops the write rate by ~10× with no observable user impact (the
//! cost-cap dashboards work off span attributes, not the comment).
//!
//! ## Cap-crossing path
//!
//! When a session's accumulation pushes a ticket over `max_tokens_per_ticket`
//! or `max_cost_per_ticket_usd`, the debounce is BYPASSED — we flush
//! immediately and transition the ticket to `budget_exceeded_state`. This
//! is the only path where the bridge writes per-session to the tracker.
//!
//! ## State durability
//!
//! The accumulator is in-process state. It does NOT survive bridge
//! restart. On restart the bridge re-reads the last persisted totals
//! from the tracker (whatever was on disk at the last flush) and starts
//! a fresh accumulator from there. Any in-flight deltas not yet flushed
//! are lost. Acceptable per plan §7.3: budget caps are an SLO, not a
//! billing system.

use crate::config::{BridgeConfig, CustomFieldsSection, FeedbackLoopSection};
use crate::feedback::cost::CostTable;
use crate::telemetry::spans;
use crate::Result;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rust_decimal::Decimal;
use sinfonia_tracker::{CustomFieldValue, IssueTracker};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, info_span, warn};

/// Per-ticket in-process accumulator. All fields are local-only —
/// nothing here is persisted across restart.
#[derive(Debug, Clone, Default)]
pub struct TicketAccumulator {
    pub issue_id: String,
    /// Cumulative since last tracker flush.
    pub pending_input_tokens: u64,
    pub pending_output_tokens: u64,
    pub pending_cost_usd: Decimal,
    /// Total seen since process start (NOT since last flush).
    pub running_total_tokens: u64,
    pub running_total_cost_usd: Decimal,
    /// Last time a session.completed event mutated this row.
    pub last_event_at: Option<DateTime<Utc>>,
    /// Last time the row was flushed to the tracker.
    pub last_flush_at: Option<DateTime<Utc>>,
}

/// Outcome of feeding one `session.completed` event into the accumulator.
/// The `CapHit` variant is the cap-crossing flush path; everything else
/// stays in-memory and waits for the 30 s debounce to flush.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionApplyOutcome {
    /// Accumulated; will flush at the next debounce tick.
    Accumulated,
    /// Crossed a cap. Caller should flush immediately and transition
    /// the ticket to `budget_exceeded_state`. `kind` distinguishes
    /// the failing cap so dashboards know whether it was tokens or
    /// cost that fired.
    CapHit { kind: CapKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapKind {
    Tokens,
    Cost,
}

impl CapKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CapKind::Tokens => spans::CAP_KIND_TOKENS,
            CapKind::Cost => spans::CAP_KIND_COST,
        }
    }
}

/// Process-wide budget manager. Wraps the per-ticket accumulator map
/// + the cost table + the bridge config. Cheap to clone; the inner
/// state is `Arc<Mutex<...>>`.
#[derive(Clone)]
pub struct BudgetManager {
    state: Arc<Mutex<BudgetState>>,
    cost_table: Arc<CostTable>,
    feedback: Arc<FeedbackLoopSection>,
    custom_fields: Arc<CustomFieldsSection>,
    tracker: Arc<dyn IssueTracker>,
    /// Today's date at process start. Used to gate cost-cap acceptance
    /// against `CostTable::accepts_cost_caps` (M-2 fix). Re-resolved at
    /// startup, not per-event — caps don't drift mid-run.
    accepts_cost_caps_today: bool,
}

#[derive(Default)]
struct BudgetState {
    tickets: HashMap<String, TicketAccumulator>,
}

impl BudgetManager {
    pub fn new(
        cost_table: CostTable,
        config: &BridgeConfig,
        tracker: Arc<dyn IssueTracker>,
    ) -> Self {
        let today = Utc::now().date_naive();
        let accepts_cost_caps_today = cost_table.accepts_cost_caps(today);
        if !accepts_cost_caps_today && config.feedback_loop.max_cost_per_ticket_usd.is_some() {
            warn!(
                target: "budget",
                "cost table is older than the M-2 block window; cost caps WILL NOT be enforced this run (token caps still apply)"
            );
        }
        Self {
            state: Arc::new(Mutex::new(BudgetState::default())),
            cost_table: Arc::new(cost_table),
            feedback: Arc::new(config.feedback_loop.clone()),
            custom_fields: Arc::new(config.custom_fields.clone()),
            tracker,
            accepts_cost_caps_today,
        }
    }

    /// Feed one `session.completed` event into the accumulator. Returns
    /// the outcome — the caller decides whether to flush immediately
    /// (CapHit) or let the debounce reconciler pick it up later
    /// (Accumulated).
    pub fn apply_session(
        &self,
        issue_id: &str,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> SessionApplyOutcome {
        let session_cost = self
            .cost_table
            .compute_cost(provider, model, input_tokens, output_tokens);
        let now = Utc::now();
        let mut state = self.state.lock();
        let acc = state
            .tickets
            .entry(issue_id.to_string())
            .or_insert_with(|| TicketAccumulator {
                issue_id: issue_id.to_string(),
                ..Default::default()
            });
        acc.pending_input_tokens = acc.pending_input_tokens.saturating_add(input_tokens);
        acc.pending_output_tokens = acc.pending_output_tokens.saturating_add(output_tokens);
        acc.pending_cost_usd += session_cost;
        acc.running_total_tokens = acc
            .running_total_tokens
            .saturating_add(input_tokens.saturating_add(output_tokens));
        acc.running_total_cost_usd += session_cost;
        acc.last_event_at = Some(now);

        // Cap detection runs against running totals (process-lifetime).
        if let Some(max_tokens) = self.feedback.max_tokens_per_ticket {
            if acc.running_total_tokens >= max_tokens {
                return SessionApplyOutcome::CapHit { kind: CapKind::Tokens };
            }
        }
        if self.accepts_cost_caps_today {
            if let Some(max_cost) = self.feedback.max_cost_per_ticket_usd {
                let max_cost_dec = Decimal::try_from(max_cost).unwrap_or(Decimal::ZERO);
                if acc.running_total_cost_usd >= max_cost_dec {
                    return SessionApplyOutcome::CapHit { kind: CapKind::Cost };
                }
            }
        }
        SessionApplyOutcome::Accumulated
    }

    /// Flush a ticket's accumulator to the tracker — writes
    /// `tokens_consumed` and `cost_consumed_usd` custom fields. Clears
    /// the pending counters and stamps `last_flush_at`. Returns the
    /// running totals that were written so the caller can emit
    /// `bridge.cost_update` with the right values.
    pub async fn flush_ticket(&self, issue_id: &str) -> Result<Option<FlushedTotals>> {
        let snapshot = {
            let mut state = self.state.lock();
            let Some(acc) = state.tickets.get_mut(issue_id) else {
                return Ok(None);
            };
            if acc.pending_input_tokens == 0
                && acc.pending_output_tokens == 0
                && acc.pending_cost_usd == Decimal::ZERO
            {
                return Ok(None);
            }
            let snap = FlushedTotals {
                tokens_delta: acc.pending_input_tokens + acc.pending_output_tokens,
                cost_delta_usd: acc.pending_cost_usd,
                tokens_total: acc.running_total_tokens,
                cost_total_usd: acc.running_total_cost_usd,
            };
            acc.pending_input_tokens = 0;
            acc.pending_output_tokens = 0;
            acc.pending_cost_usd = Decimal::ZERO;
            acc.last_flush_at = Some(Utc::now());
            snap
        };

        let span = info_span!(
            target: "feedback",
            spans::BRIDGE_COST_UPDATE,
            { spans::ATTR_TICKET_ID } = issue_id,
            { spans::ATTR_TOKENS_DELTA } = snapshot.tokens_delta,
            { spans::ATTR_TOKENS_TOTAL } = snapshot.tokens_total,
            { spans::ATTR_COST_DELTA_USD } =
                crate::feedback::cost::cost_to_string(snapshot.cost_delta_usd).as_str(),
            { spans::ATTR_COST_TOTAL_USD } =
                crate::feedback::cost::cost_to_string(snapshot.cost_total_usd).as_str(),
        );
        let _enter = span.enter();

        // Tracker writes — strings per STATUS §5.1 (never f64 for money).
        self.tracker
            .write_custom_field(
                issue_id,
                &self.custom_fields.tokens_consumed,
                CustomFieldValue::Number(snapshot.tokens_total as f64),
            )
            .await
            .map_err(crate::Error::Tracker)?;
        self.tracker
            .write_custom_field(
                issue_id,
                &self.custom_fields.cost_consumed_usd,
                CustomFieldValue::String(crate::feedback::cost::cost_to_string(
                    snapshot.cost_total_usd,
                )),
            )
            .await
            .map_err(crate::Error::Tracker)?;

        info!(
            target: "feedback",
            issue_id,
            tokens_total = snapshot.tokens_total,
            cost_total_usd = %crate::feedback::cost::cost_to_string(snapshot.cost_total_usd),
            "flushed budget accumulator to tracker"
        );
        Ok(Some(snapshot))
    }

    /// Snapshot of the in-process accumulator state (test + diagnostic).
    pub fn snapshot(&self) -> Vec<TicketAccumulator> {
        self.state.lock().tickets.values().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub struct FlushedTotals {
    pub tokens_delta: u64,
    pub cost_delta_usd: Decimal,
    pub tokens_total: u64,
    pub cost_total_usd: Decimal,
}

/// Background reconciler — wakes every 5 s and flushes any ticket that
/// hasn't seen a session event in >= 30 s. Per §7.3.
pub fn spawn_debounce_reconciler(manager: BudgetManager) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let now = Utc::now();
            let candidates: Vec<String> = {
                let state = manager.state.lock();
                state
                    .tickets
                    .iter()
                    .filter_map(|(id, acc)| {
                        let has_pending = acc.pending_input_tokens > 0
                            || acc.pending_output_tokens > 0
                            || acc.pending_cost_usd != Decimal::ZERO;
                        let idle_long_enough = match acc.last_event_at {
                            Some(t) => (now - t).num_seconds() >= 30,
                            None => false,
                        };
                        if has_pending && idle_long_enough {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            for id in candidates {
                if let Err(e) = manager.flush_ticket(&id).await {
                    warn!(target: "feedback", issue_id = %id, error = %e, "debounce flush failed");
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Tests — focus on cap detection + accumulator math. The tracker-write
// path is exercised by the existing `bridge_e2e.rs` integration suite.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FailureCategory;
    use async_trait::async_trait;
    use regex::Regex;
    use sinfonia_tracker::{Issue, IssueState, Result as TrackerResult};

    // A stub tracker that records every write_custom_field call. Other
    // trait methods use default impls (which return `Other` errors —
    // the budget tests never call them).
    struct StubTracker {
        writes: Mutex<Vec<(String, String, CustomFieldValue)>>,
    }
    impl StubTracker {
        fn new() -> Self {
            Self {
                writes: Mutex::new(vec![]),
            }
        }
    }
    #[async_trait]
    impl IssueTracker for StubTracker {
        async fn fetch_candidate_issues(&self) -> TrackerResult<Vec<Issue>> {
            Ok(vec![])
        }
        async fn fetch_issues_by_states(&self, _s: &[String]) -> TrackerResult<Vec<Issue>> {
            Ok(vec![])
        }
        async fn fetch_issue_states_by_ids(
            &self,
            _ids: &[String],
        ) -> TrackerResult<Vec<IssueState>> {
            Ok(vec![])
        }
        async fn write_custom_field(
            &self,
            id: &str,
            key: &str,
            value: CustomFieldValue,
        ) -> TrackerResult<()> {
            self.writes.lock().push((id.into(), key.into(), value));
            Ok(())
        }
    }

    fn make_config(max_tokens: Option<u64>, max_cost: Option<f64>) -> BridgeConfig {
        BridgeConfig {
            tracker: crate::config::TrackerSection {
                kind: sinfonia_tracker::TrackerKind::Linear,
                endpoint: "x".into(),
                api_key: Some("x".into()),
                project_slug: Some("p".into()),
                active_states: vec!["Todo".into()],
                terminal_states: vec!["Done".into()],
                jira_email: None,
            },
            github: crate::config::GitHubSection {
                webhook_secret: Some("s".into()),
                pat: Some("x".into()),
                app_id: None,
                private_key: None,
                manage_labels: true,
                label_prefix: "sinfonia:".into(),
                label_aliases: crate::config::LabelAliases::default(),
            },
            feedback_loop: FeedbackLoopSection {
                max_attempts: 3,
                needs_fixes_state: "Needs Fixes".into(),
                blocked_state: "Blocked".into(),
                awaiting_review_state: None,
                pr_link_pattern: Regex::new(r".*").unwrap(),
                required_checks: vec![],
                max_tokens_per_ticket: max_tokens,
                max_cost_per_ticket_usd: max_cost,
                budget_exceeded_state: "Budget Exceeded".into(),
                failure_comment_template: "x".into(),
                failure_categories: vec![FailureCategory {
                    name: "default".into(),
                    check_pattern: None,
                    target_state: "Needs Fixes".into(),
                    priority: 0,
                }],
                harness_manifest: crate::config::HarnessManifestSection::default(),
                merge_coordinator: crate::config::MergeCoordinatorSection::default(),
            },
            custom_fields: CustomFieldsSection {
                attempt_count: "sinfonia_attempt_count".into(),
                last_failure_log: "sinfonia_last_ci_failure".into(),
                max_attempts_override: "sinfonia_max_attempts".into(),
                failure_category: "sinfonia_failure_category".into(),
                tokens_consumed: "sinfonia_tokens_consumed".into(),
                cost_consumed_usd: "sinfonia_cost_consumed_usd".into(),
                max_cost_override_usd: "sinfonia_max_cost_usd".into(),
            },
            server: crate::config::ServerSection {
                bind: "127.0.0.1".into(),
                port: 8081,
                public_url: None,
            },
            storage: crate::config::StorageSection {
                state_db_path: std::path::PathBuf::from(":memory:"),
            },
            telemetry: crate::config::TelemetrySection::default(),
            source_path: std::path::PathBuf::from("/dev/null"),
        }
    }

    #[tokio::test]
    async fn accumulates_under_cap() {
        let cfg = make_config(Some(1_000_000), None);
        let tracker: Arc<dyn IssueTracker> = Arc::new(StubTracker::new());
        let mgr = BudgetManager::new(CostTable::embedded_default(), &cfg, tracker);
        let r = mgr.apply_session("iss_1", "anthropic", "claude-sonnet-4-6", 100_000, 50_000);
        assert_eq!(r, SessionApplyOutcome::Accumulated);
    }

    #[tokio::test]
    async fn token_cap_hit_returns_cap_hit_outcome() {
        let cfg = make_config(Some(150_000), None);
        let tracker: Arc<dyn IssueTracker> = Arc::new(StubTracker::new());
        let mgr = BudgetManager::new(CostTable::embedded_default(), &cfg, tracker);
        let r = mgr.apply_session("iss_1", "anthropic", "claude-sonnet-4-6", 100_000, 50_000);
        assert_eq!(r, SessionApplyOutcome::CapHit { kind: CapKind::Tokens });
    }

    #[tokio::test]
    async fn cost_cap_hit_when_cost_caps_accepted() {
        // claude-sonnet-4-6 @ 100k input + 50k output = 1.05 USD
        let cfg = make_config(None, Some(1.00));
        let tracker: Arc<dyn IssueTracker> = Arc::new(StubTracker::new());
        let mgr = BudgetManager::new(CostTable::embedded_default(), &cfg, tracker);
        let r = mgr.apply_session("iss_1", "anthropic", "claude-sonnet-4-6", 100_000, 50_000);
        assert_eq!(r, SessionApplyOutcome::CapHit { kind: CapKind::Cost });
    }

    #[tokio::test]
    async fn flush_writes_to_tracker_and_clears_pending() {
        let cfg = make_config(Some(1_000_000), None);
        let stub = Arc::new(StubTracker::new());
        let tracker: Arc<dyn IssueTracker> = stub.clone();
        let mgr = BudgetManager::new(CostTable::embedded_default(), &cfg, tracker);
        mgr.apply_session("iss_1", "anthropic", "claude-sonnet-4-6", 100_000, 50_000);
        let totals = mgr.flush_ticket("iss_1").await.unwrap().unwrap();
        assert_eq!(totals.tokens_total, 150_000);

        let writes = stub.writes.lock().clone();
        assert_eq!(writes.len(), 2);
        // tokens_consumed write — Number type since it's a counter
        assert_eq!(writes[0].1, "sinfonia_tokens_consumed");
        // cost_consumed_usd write — must be a String per STATUS §5.1 so
        // tracker round-trip preserves precision (Linear's marker-comment
        // carries everything as text; Jira's customfield is text-typed
        // when the source-of-truth is a Decimal).
        assert_eq!(writes[1].1, "sinfonia_cost_consumed_usd");
        match &writes[1].2 {
            CustomFieldValue::String(s) => assert_eq!(s, "1.05"),
            other => panic!("cost must write as String, got {:?}", other),
        }

        // Second flush with no new event is a no-op (pending cleared).
        let second = mgr.flush_ticket("iss_1").await.unwrap();
        assert!(second.is_none());
    }
}
