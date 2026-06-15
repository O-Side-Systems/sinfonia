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
use tracing::{debug, info, info_span, warn, Instrument};

use crate::telemetry::spans;

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
    /// Phase 3 §7.2 — secondary fan-out for `AgentEvent`s the subscriber
    /// emitter task consumes. `forward_events` writes to both this and
    /// `agent_tx`; the emitter task filters for `SessionCompleted` and
    /// POSTs to every registered subscriber. Kept as a `RwLock<Option<_>>`
    /// so `main.rs` can install the sender after spawning the emitter
    /// task without making it mandatory for tests that don't exercise
    /// the event channel.
    pub(crate) subscribers_tx: parking_lot::RwLock<
        Option<mpsc::UnboundedSender<(String, AgentEvent)>>,
    >,
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

/// Per-issue outcome of a dispatch attempt. `Dispatched` means a worker
/// future was spawned; `Skipped` covers ineligibility / already-running /
/// state-claimed cases (the tick loop keeps going); `NoSlot` means the
/// orchestrator's concurrency budget is full and the caller should stop
/// trying further candidates this tick.
///
/// `bool::from` flattens to "should the dispatch loop keep going?" which
/// matches the pre-Phase-3 boolean contract: `Dispatched` and `Skipped`
/// both keep the loop going; `NoSlot` breaks. `retries::tick_retries` only
/// cares about the binary "did it dispatch?" — `is_dispatched()` answers
/// that without exposing the enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchOutcome {
    Dispatched,
    Skipped,
    NoSlot,
}

impl DispatchOutcome {
    pub(crate) fn continue_loop(self) -> bool {
        !matches!(self, Self::NoSlot)
    }
    pub(crate) fn is_dispatched(self) -> bool {
        matches!(self, Self::Dispatched)
    }
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
            subscribers_tx: parking_lot::RwLock::new(None),
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
        self.warn_permissive_posture();
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

    /// Make a permissive agent posture visible at startup (Proposal 0004 §4.2).
    /// CLI backends are commonly run with their own permission systems disabled
    /// (e.g. `--dangerously-skip-permissions` for Claude Code, `codex exec`),
    /// which is what unattended autonomous operation needs — but it means the
    /// agent can run arbitrary commands with no per-action approval. Surfacing
    /// it in the log (rather than burying it in a default command string) is the
    /// §4.2 control; the operational mitigation is to run Sinfonia inside an
    /// isolated environment (container/VM, scoped credentials, restricted
    /// egress) — see `SECURITY.md`.
    fn warn_permissive_posture(&self) {
        let cfg = self.config();
        // Collect the effective command line for the default lane plus every
        // configured state override, de-duplicated.
        let mut commands: Vec<String> = vec![cfg.llm.command.clone()];
        for state in cfg.tracker.active_states.iter() {
            commands.push(cfg.effective_llm_for_state(state).command);
        }
        commands.sort();
        commands.dedup();
        let permissive: Vec<&String> = commands
            .iter()
            .filter(|c| {
                let lc = c.to_lowercase();
                lc.contains("--dangerously-skip-permissions")
                    || lc.contains("dangerously_skip_permissions")
                    || lc.contains("codex exec")
            })
            .collect();
        if !permissive.is_empty() {
            warn!(
                target: "orchestrator",
                commands = ?permissive,
                "agent runs with per-action approval disabled (autonomous mode). \
                 This grants the agent unrestricted shell in its workspace and the \
                 daemon's environment. Run Sinfonia in an isolated container/VM with \
                 scoped credentials and restricted egress — see SECURITY.md."
            );
        }
    }

    /// One poll cycle (§8.1 tick sequence). Wrapped in the
    /// `orchestrator.tick` span (plan §4) so per-tick telemetry surfaces in
    /// OTel with `tenant_id`, candidate count, dispatched count, and tick
    /// duration as structured attributes.
    pub async fn tick(&self) {
        let cfg = self.config();
        let span = info_span!(
            target: "orchestrator",
            spans::ORCHESTRATOR_TICK,
            { spans::ATTR_TENANT_ID } = %cfg.telemetry.tenant_id,
            { spans::ATTR_CANDIDATES_COUNT } = tracing::field::Empty,
            { spans::ATTR_DISPATCHED_COUNT } = tracing::field::Empty,
            { spans::ATTR_TICK_DURATION_MS } = tracing::field::Empty,
        );
        self.tick_body(cfg).instrument(span).await;
    }

    async fn tick_body(&self, cfg: Arc<ServiceConfig>) {
        let started = std::time::Instant::now();
        debug!(target: "orchestrator", "tick start");
        self.reconcile_running_issues().await;

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
        let candidates_count = sorted.len();
        let mut dispatched_count: u32 = 0;
        for issue in sorted {
            let outcome = self.dispatch_one(issue, None).await;
            if outcome.is_dispatched() {
                dispatched_count += 1;
            }
            if !outcome.continue_loop() {
                break; // no slots
            }
        }

        let current = tracing::Span::current();
        current.record(spans::ATTR_CANDIDATES_COUNT, candidates_count as i64);
        current.record(spans::ATTR_DISPATCHED_COUNT, dispatched_count as i64);
        current.record(
            spans::ATTR_TICK_DURATION_MS,
            started.elapsed().as_millis() as i64,
        );
        debug!(target: "orchestrator", "tick end");
    }

    pub(crate) async fn dispatch_one(
        &self,
        issue: Issue,
        attempt: Option<u32>,
    ) -> DispatchOutcome {
        let cfg = self.config();
        let span = info_span!(
            target: "orchestrator",
            spans::ORCHESTRATOR_DISPATCH,
            { spans::ATTR_TENANT_ID } = %cfg.telemetry.tenant_id,
            { spans::ATTR_ISSUE_ID } = %issue.id,
            { spans::ATTR_ISSUE_IDENTIFIER } = %issue.identifier,
            { spans::ATTR_STATE } = %issue.state,
            { spans::ATTR_PROVIDER } = tracing::field::Empty,
            { spans::ATTR_MODEL } = tracing::field::Empty,
        );
        async move {
            if !dispatch::is_dispatch_eligible(&issue, &cfg) {
                return DispatchOutcome::Skipped;
            }
            // Record the resolved provider/model so a per-state routing dashboard
            // can filter on them without re-reading the config from the span.
            let eff_llm = cfg.effective_llm_for_state(&issue.state);
            tracing::Span::current()
                .record(spans::ATTR_PROVIDER, format!("{:?}", eff_llm.provider).as_str());
            tracing::Span::current().record(spans::ATTR_MODEL, eff_llm.model.as_str());

            let mut state = self.inner.state.lock().await;
            if state.running.contains_key(&issue.id) {
                return DispatchOutcome::Skipped;
            }
            // `attempt.is_some()` means this came from the retry queue (the
            // issue is already claimed and a backoff timer fired). A fresh
            // poll-tick dispatch passes `None`.
            let is_retry = attempt.is_some();
            if state.claimed.contains(&issue.id) {
                // The issue is already owned by the orchestrator. If this is a
                // fresh poll-tick dispatch, a retry is pending for it — skip,
                // and let the backoff schedule drive it. Re-dispatching here
                // would bypass backoff and reset the attempt counter, turning a
                // failing issue into a tight crash loop (it also clobbers the
                // pending RetryEntry at the `retry_attempts.remove` below).
                if !is_retry {
                    return DispatchOutcome::Skipped;
                }
                // Retry path: claim already held; just confirm a slot is free.
                if !dispatch::has_slot(&state, &issue, &cfg) {
                    return DispatchOutcome::NoSlot;
                }
            } else {
                if !dispatch::has_slot(&state, &issue, &cfg) {
                    return DispatchOutcome::NoSlot;
                }
                state.claimed.insert(issue.id.clone());
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
            DispatchOutcome::Dispatched
        }
        .instrument(span)
        .await
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
        let subscribers_tx = self.inner.subscribers_tx.read().clone();
        tokio::spawn(forward_events(
            issue_id.clone(),
            event_rx,
            agent_tx,
            subscribers_tx,
        ));

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
            warn!(
                target: "orchestrator",
                issue_identifier=%identifier,
                issue_id=%issue_id,
                attempt=next_attempt,
                backoff_ms=backoff,
                error=%error.as_deref().unwrap_or("(none)"),
                "worker exit (abnormal); retry scheduled"
            );
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

    /// Phase 3 §7.2 — install the subscriber-emitter channel. `main.rs`
    /// constructs the (tx, rx) pair, hands the rx to the emitter task,
    /// and registers the tx here so subsequent worker spawns fan out
    /// their AgentEvents to subscribers. Idempotent — last writer wins.
    pub fn install_subscribers_tx(
        &self,
        tx: mpsc::UnboundedSender<(String, AgentEvent)>,
    ) {
        *self.inner.subscribers_tx.write() = Some(tx);
    }
}

async fn forward_events(
    issue_id: String,
    mut rx: mpsc::UnboundedReceiver<AgentEvent>,
    tx: mpsc::UnboundedSender<(String, AgentEvent)>,
    subscribers_tx: Option<mpsc::UnboundedSender<(String, AgentEvent)>>,
) {
    while let Some(ev) = rx.recv().await {
        // Fan-out to the subscriber emitter task (Phase 3 §7.2). The send
        // is best-effort: when no subscribers are configured the optional
        // sender is `None` and the broadcast is skipped. The dashboard
        // channel (`tx`) remains the source of truth for the live
        // `/api/v1/state` view.
        if let Some(stx) = subscribers_tx.as_ref() {
            let _ = stx.send((issue_id.clone(), ev.clone()));
        }
        if tx.send((issue_id.clone(), ev)).is_err() {
            break;
        }
    }
}
