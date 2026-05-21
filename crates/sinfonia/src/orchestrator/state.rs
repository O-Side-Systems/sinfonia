//! Snapshot rendering for the HTTP API (spec §13.3, §13.7.2).

use crate::config::ServiceConfig;
use crate::domain::OrchestratorState;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotView {
    pub generated_at: DateTime<Utc>,
    pub counts: SnapshotCounts,
    pub running: Vec<RunningRow>,
    pub retrying: Vec<RetryRow>,
    pub codex_totals: CodexTotalsView,
    pub rate_limits: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotCounts {
    pub running: usize,
    pub retrying: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunningRow {
    pub issue_id: String,
    pub issue_identifier: String,
    pub state: String,
    pub session_id: Option<String>,
    pub turn_count: u32,
    pub last_event: Option<String>,
    pub last_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_event_at: Option<DateTime<Utc>>,
    pub tokens: TokenView,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenView {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetryRow {
    pub issue_id: String,
    pub issue_identifier: String,
    pub attempt: u32,
    pub due_at: DateTime<Utc>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexTotalsView {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub seconds_running: f64,
}

pub(crate) fn build_snapshot(state: &OrchestratorState, _cfg: &ServiceConfig) -> SnapshotView {
    let now = Utc::now();
    let running: Vec<RunningRow> = state
        .running
        .values()
        .map(|r| RunningRow {
            issue_id: r.issue_id.clone(),
            issue_identifier: r.identifier.clone(),
            state: r.issue.state.clone(),
            session_id: r
                .session
                .thread_id
                .as_ref()
                .and_then(|t| r.session.turn_id.as_ref().map(|u| format!("{t}-{u}")))
                .or_else(|| r.session.thread_id.clone()),
            turn_count: r.session.turn_count,
            last_event: r.session.last_codex_event.clone(),
            last_message: r.session.last_codex_message.clone(),
            started_at: r.started_at,
            last_event_at: r.session.last_codex_timestamp,
            tokens: TokenView {
                input_tokens: r.session.codex_input_tokens,
                output_tokens: r.session.codex_output_tokens,
                total_tokens: r.session.codex_total_tokens,
            },
        })
        .collect();

    let retrying: Vec<RetryRow> = state
        .retry_attempts
        .values()
        .map(|r| RetryRow {
            issue_id: r.issue_id.clone(),
            issue_identifier: r.identifier.clone(),
            attempt: r.attempt,
            due_at: r.due_at,
            error: r.error.clone(),
        })
        .collect();

    // Live runtime aggregate (§13.5): cumulative ended-session seconds + active sessions' elapsed.
    let mut seconds_running = state.codex_totals.seconds_running;
    for r in state.running.values() {
        let elapsed = (now - r.started_at).num_milliseconds().max(0) as f64 / 1000.0;
        seconds_running += elapsed;
    }

    SnapshotView {
        generated_at: now,
        counts: SnapshotCounts {
            running: state.running.len(),
            retrying: state.retry_attempts.len(),
        },
        running,
        retrying,
        codex_totals: CodexTotalsView {
            input_tokens: state.codex_totals.input_tokens,
            output_tokens: state.codex_totals.output_tokens,
            total_tokens: state.codex_totals.total_tokens,
            seconds_running,
        },
        rate_limits: state.codex_rate_limits.as_ref().map(|v| v.0.clone()),
    }
}

pub(crate) fn build_issue_view(
    state: &OrchestratorState,
    _cfg: &ServiceConfig,
    identifier: &str,
) -> Option<Value> {
    let entry = state.running.values().find(|r| r.identifier == identifier);
    let retry = state
        .retry_attempts
        .values()
        .find(|r| r.identifier == identifier);

    if entry.is_none() && retry.is_none() {
        return None;
    }

    let running_json = entry.map(|r| {
        json!({
            "session_id": r.session.thread_id.clone(),
            "turn_count": r.session.turn_count,
            "state": r.issue.state,
            "started_at": r.started_at,
            "last_event": r.session.last_codex_event,
            "last_message": r.session.last_codex_message,
            "last_event_at": r.session.last_codex_timestamp,
            "tokens": {
                "input_tokens": r.session.codex_input_tokens,
                "output_tokens": r.session.codex_output_tokens,
                "total_tokens": r.session.codex_total_tokens,
            }
        })
    });

    let retry_json = retry.map(|r| {
        json!({
            "attempt": r.attempt,
            "due_at": r.due_at,
            "error": r.error,
        })
    });

    let status = if entry.is_some() { "running" } else { "retrying" };
    let issue_id = entry
        .map(|r| r.issue_id.clone())
        .or_else(|| retry.map(|r| r.issue_id.clone()))
        .unwrap_or_default();

    Some(json!({
        "issue_identifier": identifier,
        "issue_id": issue_id,
        "status": status,
        "running": running_json,
        "retry": retry_json,
        "last_error": retry.and_then(|r| r.error.clone()),
        "tracked": {}
    }))
}
