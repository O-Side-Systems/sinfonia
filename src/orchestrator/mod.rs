//! Sinfonia orchestrator (Symphony spec §7, §8, §16).
//!
//! The orchestrator is the single authority over scheduling state. Worker tasks
//! report outcomes back through channels; mutation of the running/claimed/retry
//! maps happens behind a Tokio mutex from the control loop and the worker
//! callbacks.

mod dispatch;
mod retries;
mod runner;
mod state;

pub use state::{RunningRow, SnapshotView};

/// Sort issues by dispatch order. Public so integration tests in `tests/` can call it.
pub fn dispatch_for_test(issues: Vec<crate::domain::Issue>) -> Vec<crate::domain::Issue> {
    dispatch::sort_for_dispatch(issues)
}

/// Public for integration tests: eligibility check (no claim/slot tracking).
pub fn is_eligible_for_test(
    issue: &crate::domain::Issue,
    cfg: &crate::config::ServiceConfig,
) -> bool {
    dispatch::is_dispatch_eligible(issue, cfg)
}

/// Public for integration tests: exponential backoff math.
pub fn backoff_for_test(attempt: u32, max_backoff: u64) -> u64 {
    retries::backoff_ms(attempt, max_backoff)
}

use crate::agent::{event_channel, AgentEvent};
use crate::config::{ServiceConfig, WorkflowDefinition};
use crate::domain::{Issue, OrchestratorState, RetryEntry, RunningEntry};
use crate::errors::{Error, Result};
use crate::tracker::IssueTracker;
use crate::workspace::WorkspaceManager;
use chrono::{Duration as ChronoDuration, Utc};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Notify};
use tracing::{debug, info, warn};

/// Compact, cloneable handle to the running orchestrator.
#[derive(Clone)]
pub struct Orchestrator {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) state: Mutex<OrchestratorState>,
    config: RwLock<Arc<ServiceConfig>>,
    workflow: RwLock<Arc<WorkflowDefinition>>,
    tracker: RwLock<Arc<dyn IssueTracker>>,
    workspace: RwLock<Arc<WorkspaceManager>>,
    refresh: Notify,
    pub(crate) worker_tx: mpsc::UnboundedSender<WorkerReport>,
    agent_tx: mpsc::UnboundedSender<(String, AgentEvent)>,
}

#[derive(Debug)]
pub(crate) enum WorkerReport {
    Exited {
        issue_id: String,
        identifier: String,
        normal: bool,
        error: Option<String>,
        finished_at: chrono::DateTime<Utc>,
        started_at: chrono::DateTime<Utc>,
        attempt: Option<u32>,
    },
}

impl Orchestrator {
    pub async fn new(
        workflow: WorkflowDefinition,
        config: ServiceConfig,
        tracker: Arc<dyn IssueTracker>,
        workspace: Arc<WorkspaceManager>,
    ) -> Result<Self> {
        let s = OrchestratorState {
            poll_interval_ms: config.polling.interval_ms,
            max_concurrent_agents: config.agent.max_concurrent_agents,
            ..Default::default()
        };

        let (worker_tx, worker_rx) = mpsc::unbounded_channel::<WorkerReport>();
        let (agent_tx, agent_rx) = mpsc::unbounded_channel::<(String, AgentEvent)>();

        let inner = Arc::new(Inner {
            state: Mutex::new(s),
            config: RwLock::new(Arc::new(config)),
            workflow: RwLock::new(Arc::new(workflow)),
            tracker: RwLock::new(tracker),
            workspace: RwLock::new(workspace),
            refresh: Notify::new(),
            worker_tx,
            agent_tx,
        });

        let orch = Orchestrator { inner: inner.clone() };

        let oc = orch.clone();
        tokio::spawn(async move {
            oc.consume_worker_reports(worker_rx).await;
        });
        let oc = orch.clone();
        tokio::spawn(async move {
            oc.consume_agent_events(agent_rx).await;
        });

        Ok(orch)
    }

    pub fn config(&self) -> Arc<ServiceConfig> {
        self.inner.config.read().clone()
    }
    pub fn workflow(&self) -> Arc<WorkflowDefinition> {
        self.inner.workflow.read().clone()
    }
    pub fn tracker(&self) -> Arc<dyn IssueTracker> {
        self.inner.tracker.read().clone()
    }
    pub fn workspace_manager(&self) -> Arc<WorkspaceManager> {
        self.inner.workspace.read().clone()
    }

    /// Replace the live workflow/config (and the dependent objects) after a successful reload.
    pub async fn apply_reload(
        &self,
        workflow: WorkflowDefinition,
        config: ServiceConfig,
        tracker: Arc<dyn IssueTracker>,
        workspace: Arc<WorkspaceManager>,
    ) {
        *self.inner.workflow.write() = Arc::new(workflow);
        *self.inner.config.write() = Arc::new(config.clone());
        *self.inner.tracker.write() = tracker;
        *self.inner.workspace.write() = workspace;
        {
            let mut s = self.inner.state.lock().await;
            s.poll_interval_ms = config.polling.interval_ms;
            s.max_concurrent_agents = config.agent.max_concurrent_agents;
        }
        info!(target: "orchestrator", "workflow reload applied");
        self.request_refresh();
    }

    /// Wake the poll loop immediately.
    pub fn request_refresh(&self) {
        self.inner.refresh.notify_one();
    }

    /// Spec §16.1: startup terminal workspace cleanup + first tick + main poll loop.
    pub async fn run(&self) -> Result<()> {
        if let Err(e) = self.startup_terminal_cleanup().await {
            warn!(target: "orchestrator", error=%e, "startup terminal cleanup failed");
        }
        self.tick().await;

        loop {
            let interval_ms = self.config().polling.interval_ms.max(50);
            let sleep = tokio::time::sleep(std::time::Duration::from_millis(interval_ms));
            tokio::select! {
                _ = sleep => self.tick().await,
                _ = self.inner.refresh.notified() => self.tick().await,
            }
        }
    }

    /// One poll cycle (§8.1 tick sequence).
    pub async fn tick(&self) {
        debug!(target: "orchestrator", "tick start");
        self.reconcile_running_issues().await;

        let cfg = self.config();
        if let Err(e) = cfg.validate_for_dispatch() {
            warn!(target: "orchestrator", error=%e, "preflight validation failed; skipping dispatch");
            return;
        }

        let tracker = self.tracker();
        let issues = match tracker.fetch_candidate_issues().await {
            Ok(v) => v,
            Err(e) => {
                warn!(target: "orchestrator", error=%e, "candidate fetch failed; skipping dispatch");
                return;
            }
        };

        let sorted = dispatch::sort_for_dispatch(issues);
        for issue in sorted {
            if !self.dispatch_one(issue, None).await {
                break; // no slots
            }
        }
        debug!(target: "orchestrator", "tick end");
    }

    pub(crate) async fn dispatch_one(&self, issue: Issue, attempt: Option<u32>) -> bool {
        let cfg = self.config();
        if !dispatch::is_dispatch_eligible(&issue, &cfg) {
            return true;
        }
        let mut state = self.inner.state.lock().await;
        if state.running.contains_key(&issue.id) {
            return true;
        }
        // For retries the issue is already claimed; for fresh dispatch we add it now.
        if !state.claimed.contains(&issue.id) {
            // Claim it preemptively to avoid races with the next tick.
            if !dispatch::has_slot(&state, &issue, &cfg) {
                return false;
            }
            state.claimed.insert(issue.id.clone());
        } else if !dispatch::has_slot(&state, &issue, &cfg) {
            // Slot exhausted on retry — caller should requeue.
            return false;
        }
        let started_at = Utc::now();
        let workspace_path = self
            .workspace_manager()
            .workspace_path_for(&issue.identifier)
            .display()
            .to_string();
        let entry = RunningEntry {
            issue_id: issue.id.clone(),
            identifier: issue.identifier.clone(),
            issue: issue.clone(),
            workspace_path,
            session: Default::default(),
            retry_attempt: attempt,
            started_at,
        };
        state.running.insert(issue.id.clone(), entry);
        state.retry_attempts.remove(&issue.id);
        drop(state);

        let orch = self.clone();
        tokio::spawn(async move {
            orch.run_worker(issue, attempt, started_at).await;
        });
        true
    }

    async fn run_worker(
        &self,
        issue: Issue,
        attempt: Option<u32>,
        started_at: chrono::DateTime<Utc>,
    ) {
        let cfg = self.config();
        let workflow = self.workflow();
        let workspace_mgr = self.workspace_manager();
        let tracker = self.tracker();

        let (event_tx, event_rx) = event_channel();
        let issue_id = issue.id.clone();
        let agent_tx = self.inner.agent_tx.clone();
        tokio::spawn(forward_events(issue_id.clone(), event_rx, agent_tx));

        let outcome = runner::run_agent_attempt(
            issue.clone(),
            attempt,
            &cfg,
            &workflow,
            tracker.clone(),
            workspace_mgr.clone(),
            event_tx,
        )
        .await;

        let report = match outcome {
            Ok(()) => WorkerReport::Exited {
                issue_id: issue.id.clone(),
                identifier: issue.identifier.clone(),
                normal: true,
                error: None,
                finished_at: Utc::now(),
                started_at,
                attempt,
            },
            Err(e) => WorkerReport::Exited {
                issue_id: issue.id.clone(),
                identifier: issue.identifier.clone(),
                normal: false,
                error: Some(e.to_string()),
                finished_at: Utc::now(),
                started_at,
                attempt,
            },
        };
        let _ = self.inner.worker_tx.send(report);
    }

    async fn consume_worker_reports(&self, mut rx: mpsc::UnboundedReceiver<WorkerReport>) {
        while let Some(rep) = rx.recv().await {
            match rep {
                WorkerReport::Exited {
                    issue_id,
                    identifier,
                    normal,
                    error,
                    finished_at,
                    started_at,
                    attempt,
                } => self
                    .on_worker_exit(
                        issue_id, identifier, normal, error, finished_at, started_at, attempt,
                    )
                    .await,
            }
        }
    }

    async fn consume_agent_events(
        &self,
        mut rx: mpsc::UnboundedReceiver<(String, AgentEvent)>,
    ) {
        while let Some((issue_id, ev)) = rx.recv().await {
            let mut state = self.inner.state.lock().await;
            let usage_owned = ev.usage().cloned();
            let summary = ev.summary_message().map(|s| s.to_string());
            let name = ev.event_name().to_string();
            let ts = ev.timestamp();
            let is_completed = matches!(ev, AgentEvent::TurnCompleted { .. });
            // Capture thread/turn IDs as they appear.
            let (new_thread, new_turn) = match &ev {
                AgentEvent::SessionStarted { thread_id, .. } => (Some(thread_id.clone()), None),
                AgentEvent::TurnStarted {
                    thread_id, turn_id, ..
                }
                | AgentEvent::TurnCompleted {
                    thread_id, turn_id, ..
                }
                | AgentEvent::TurnProgress {
                    thread_id, turn_id, ..
                }
                | AgentEvent::TurnFailed {
                    thread_id, turn_id, ..
                } => (Some(thread_id.clone()), Some(turn_id.clone())),
                _ => (None, None),
            };
            if let Some(running) = state.running.get_mut(&issue_id) {
                running.session.last_codex_event = Some(name);
                running.session.last_codex_timestamp = Some(ts);
                if new_thread.is_some() {
                    running.session.thread_id = new_thread;
                }
                if new_turn.is_some() {
                    running.session.turn_id = new_turn;
                }
                if running.session.thread_id.is_some() && running.session.turn_id.is_some() {
                    running.session.session_id = Some(format!(
                        "{}-{}",
                        running.session.thread_id.as_deref().unwrap_or(""),
                        running.session.turn_id.as_deref().unwrap_or("")
                    ));
                }
                if let Some(msg) = summary {
                    running.session.last_codex_message =
                        Some(msg.chars().take(2000).collect());
                }
                // Reset the per-turn token baseline at TurnStarted so a new
                // turn's streaming progress events aren't suppressed by the
                // previous turn's final counts via saturating_sub deltas.
                if matches!(ev, AgentEvent::TurnStarted { .. }) {
                    running.session.last_reported_input_tokens = 0;
                    running.session.last_reported_output_tokens = 0;
                    running.session.last_reported_total_tokens = 0;
                }
                if let Some(usage) = usage_owned.as_ref() {
                    let new_in = usage.input_tokens;
                    let new_out = usage.output_tokens;
                    let new_total = usage.total_tokens;
                    let din = new_in.saturating_sub(running.session.last_reported_input_tokens);
                    let dout = new_out.saturating_sub(running.session.last_reported_output_tokens);
                    let dtot = new_total.saturating_sub(running.session.last_reported_total_tokens);
                    running.session.codex_input_tokens =
                        running.session.codex_input_tokens.saturating_add(din);
                    running.session.codex_output_tokens =
                        running.session.codex_output_tokens.saturating_add(dout);
                    running.session.codex_total_tokens =
                        running.session.codex_total_tokens.saturating_add(dtot);
                    running.session.last_reported_input_tokens = new_in;
                    running.session.last_reported_output_tokens = new_out;
                    running.session.last_reported_total_tokens = new_total;
                    state.codex_totals.input_tokens =
                        state.codex_totals.input_tokens.saturating_add(din);
                    state.codex_totals.output_tokens =
                        state.codex_totals.output_tokens.saturating_add(dout);
                    state.codex_totals.total_tokens =
                        state.codex_totals.total_tokens.saturating_add(dtot);
                }
                if is_completed {
                    if let Some(r) = state.running.get_mut(&issue_id) {
                        r.session.turn_count = r.session.turn_count.saturating_add(1);
                    }
                }
            }
        }
    }

    async fn on_worker_exit(
        &self,
        issue_id: String,
        identifier: String,
        normal: bool,
        error: Option<String>,
        finished_at: chrono::DateTime<Utc>,
        started_at: chrono::DateTime<Utc>,
        attempt: Option<u32>,
    ) {
        let mut state = self.inner.state.lock().await;
        let _ = state.running.remove(&issue_id);
        let elapsed = (finished_at - started_at).num_milliseconds().max(0) as f64 / 1000.0;
        state.codex_totals.seconds_running += elapsed;

        let cfg = self.config();
        let max_backoff = cfg.agent.max_retry_backoff_ms;
        if normal {
            state.completed.insert(issue_id.clone());
            let next_attempt = attempt.unwrap_or(0) + 1;
            let due_at = Utc::now() + ChronoDuration::milliseconds(1000);
            retries::schedule(
                &mut state,
                &self.inner,
                RetryEntry {
                    issue_id: issue_id.clone(),
                    identifier: identifier.clone(),
                    attempt: next_attempt,
                    due_at,
                    error: None,
                },
            );
            info!(target: "orchestrator", issue_identifier=%identifier, issue_id=%issue_id, "worker exit (normal)");
        } else {
            let next_attempt = attempt.unwrap_or(0) + 1;
            let backoff = retries::backoff_ms(next_attempt, max_backoff);
            let due_at = Utc::now() + ChronoDuration::milliseconds(backoff as i64);
            retries::schedule(
                &mut state,
                &self.inner,
                RetryEntry {
                    issue_id: issue_id.clone(),
                    identifier: identifier.clone(),
                    attempt: next_attempt,
                    due_at,
                    error,
                },
            );
            warn!(target: "orchestrator", issue_identifier=%identifier, issue_id=%issue_id, "worker exit (abnormal); retry scheduled");
        }
    }

    async fn reconcile_running_issues(&self) {
        let cfg = self.config();
        let stall_ms = cfg.llm.stall_timeout_ms;
        let mut to_cancel_stalled: Vec<String> = Vec::new();
        let running_ids: Vec<String> = {
            let state = self.inner.state.lock().await;
            if stall_ms > 0 {
                let stall_threshold = ChronoDuration::milliseconds(stall_ms);
                let now = Utc::now();
                for (id, r) in state.running.iter() {
                    let last = r.session.last_codex_timestamp.unwrap_or(r.started_at);
                    if now - last > stall_threshold {
                        to_cancel_stalled.push(id.clone());
                    }
                }
            }
            state.running.keys().cloned().collect()
        };
        for id in to_cancel_stalled {
            warn!(target: "orchestrator", issue_id=%id, "stalled run; scheduling retry");
            let mut state = self.inner.state.lock().await;
            if let Some(entry) = state.running.remove(&id) {
                let now = Utc::now();
                let elapsed = (now - entry.started_at).num_milliseconds().max(0) as f64 / 1000.0;
                state.codex_totals.seconds_running += elapsed;
                let next_attempt = entry.retry_attempt.unwrap_or(0) + 1;
                let backoff = retries::backoff_ms(next_attempt, cfg.agent.max_retry_backoff_ms);
                let due_at = now + ChronoDuration::milliseconds(backoff as i64);
                retries::schedule(
                    &mut state,
                    &self.inner,
                    RetryEntry {
                        issue_id: entry.issue_id.clone(),
                        identifier: entry.identifier.clone(),
                        attempt: next_attempt,
                        due_at,
                        error: Some("stalled".into()),
                    },
                );
            }
        }

        if running_ids.is_empty() {
            return;
        }
        let tracker = self.tracker();
        let refreshed = match tracker.fetch_issue_states_by_ids(&running_ids).await {
            Ok(v) => v,
            Err(e) => {
                debug!(target: "orchestrator", error=%e, "running state refresh failed; keeping workers");
                return;
            }
        };
        let active_states: Vec<String> = cfg
            .tracker
            .active_states
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let terminal_states: Vec<String> = cfg
            .tracker
            .terminal_states
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        let mut seen = std::collections::HashSet::new();
        for s in refreshed {
            seen.insert(s.id.clone());
            let state_l = s.state.to_lowercase();
            if terminal_states.iter().any(|t| t == &state_l) {
                self.terminate_running_issue(&s.id, true).await;
            } else if active_states.iter().any(|a| a == &state_l) {
                let mut state = self.inner.state.lock().await;
                if let Some(r) = state.running.get_mut(&s.id) {
                    r.issue.state = s.state.clone();
                }
            } else {
                self.terminate_running_issue(&s.id, false).await;
            }
        }
        for id in running_ids {
            if !seen.contains(&id) {
                self.terminate_running_issue(&id, false).await;
            }
        }
    }

    async fn terminate_running_issue(&self, issue_id: &str, cleanup: bool) {
        let mut state = self.inner.state.lock().await;
        if let Some(entry) = state.running.remove(issue_id) {
            let elapsed =
                (Utc::now() - entry.started_at).num_milliseconds().max(0) as f64 / 1000.0;
            state.codex_totals.seconds_running += elapsed;
            state.claimed.remove(issue_id);
            state.retry_attempts.remove(issue_id);
            drop(state);
            if cleanup {
                let mgr = self.workspace_manager();
                if let Err(e) = mgr.remove(&entry.identifier) {
                    debug!(target: "orchestrator", error=%e, "workspace cleanup error");
                }
            }
            info!(target: "orchestrator", issue_identifier=%entry.identifier, cleanup, "terminated run");
        }
    }

    async fn startup_terminal_cleanup(&self) -> Result<()> {
        let cfg = self.config();
        let terminal_states = cfg.tracker.terminal_states.clone();
        let tracker = self.tracker();
        let issues = tracker
            .fetch_issues_by_states(&terminal_states)
            .await
            .map_err(|e| Error::Other(format!("startup cleanup: {e}")))?;
        let mgr = self.workspace_manager();
        for issue in issues {
            if let Err(e) = mgr.remove(&issue.identifier) {
                debug!(target: "orchestrator", error=%e, ident=%issue.identifier, "cleanup remove failed");
            }
        }
        Ok(())
    }

    /// Render an immutable snapshot for the HTTP `/api/v1/state` endpoint (§13.7.2).
    pub async fn snapshot(&self) -> SnapshotView {
        let state = self.inner.state.lock().await;
        let cfg = self.config();
        state::build_snapshot(&state, &cfg)
    }

    /// Render the per-issue debug view for `/api/v1/<issue_identifier>`.
    pub async fn issue_view(&self, identifier: &str) -> Option<serde_json::Value> {
        let state = self.inner.state.lock().await;
        let cfg = self.config();
        state::build_issue_view(&state, &cfg, identifier)
    }
}

async fn forward_events(
    issue_id: String,
    mut rx: mpsc::UnboundedReceiver<AgentEvent>,
    tx: mpsc::UnboundedSender<(String, AgentEvent)>,
) {
    while let Some(ev) = rx.recv().await {
        if tx.send((issue_id.clone(), ev)).is_err() {
            break;
        }
    }
}
