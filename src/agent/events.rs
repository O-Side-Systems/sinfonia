//! Agent → orchestrator runtime events (spec §10.4).

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        timestamp: DateTime<Utc>,
        thread_id: String,
        codex_app_server_pid: Option<String>,
    },
    StartupFailed {
        timestamp: DateTime<Utc>,
        message: String,
    },
    TurnStarted {
        timestamp: DateTime<Utc>,
        thread_id: String,
        turn_id: String,
    },
    TurnCompleted {
        timestamp: DateTime<Utc>,
        thread_id: String,
        turn_id: String,
        message: String,
        usage: Option<TokenUsage>,
    },
    TurnFailed {
        timestamp: DateTime<Utc>,
        thread_id: String,
        turn_id: String,
        message: String,
    },
    TurnCancelled {
        timestamp: DateTime<Utc>,
        thread_id: String,
        turn_id: String,
    },
    TurnEndedWithError {
        timestamp: DateTime<Utc>,
        thread_id: String,
        turn_id: String,
        message: String,
    },
    TurnInputRequired {
        timestamp: DateTime<Utc>,
        thread_id: String,
        turn_id: String,
    },
    ApprovalAutoApproved {
        timestamp: DateTime<Utc>,
        kind: String,
    },
    UnsupportedToolCall {
        timestamp: DateTime<Utc>,
        tool_name: String,
    },
    Notification {
        timestamp: DateTime<Utc>,
        message: String,
    },
    OtherMessage {
        timestamp: DateTime<Utc>,
        message: String,
    },
    Malformed {
        timestamp: DateTime<Utc>,
        raw: String,
    },
}

impl AgentEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            AgentEvent::SessionStarted { .. } => "session_started",
            AgentEvent::StartupFailed { .. } => "startup_failed",
            AgentEvent::TurnStarted { .. } => "turn_started",
            AgentEvent::TurnCompleted { .. } => "turn_completed",
            AgentEvent::TurnFailed { .. } => "turn_failed",
            AgentEvent::TurnCancelled { .. } => "turn_cancelled",
            AgentEvent::TurnEndedWithError { .. } => "turn_ended_with_error",
            AgentEvent::TurnInputRequired { .. } => "turn_input_required",
            AgentEvent::ApprovalAutoApproved { .. } => "approval_auto_approved",
            AgentEvent::UnsupportedToolCall { .. } => "unsupported_tool_call",
            AgentEvent::Notification { .. } => "notification",
            AgentEvent::OtherMessage { .. } => "other_message",
            AgentEvent::Malformed { .. } => "malformed",
        }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            AgentEvent::SessionStarted { timestamp, .. }
            | AgentEvent::StartupFailed { timestamp, .. }
            | AgentEvent::TurnStarted { timestamp, .. }
            | AgentEvent::TurnCompleted { timestamp, .. }
            | AgentEvent::TurnFailed { timestamp, .. }
            | AgentEvent::TurnCancelled { timestamp, .. }
            | AgentEvent::TurnEndedWithError { timestamp, .. }
            | AgentEvent::TurnInputRequired { timestamp, .. }
            | AgentEvent::ApprovalAutoApproved { timestamp, .. }
            | AgentEvent::UnsupportedToolCall { timestamp, .. }
            | AgentEvent::Notification { timestamp, .. }
            | AgentEvent::OtherMessage { timestamp, .. }
            | AgentEvent::Malformed { timestamp, .. } => *timestamp,
        }
    }

    pub fn summary_message(&self) -> Option<&str> {
        match self {
            AgentEvent::TurnCompleted { message, .. }
            | AgentEvent::TurnFailed { message, .. }
            | AgentEvent::TurnEndedWithError { message, .. }
            | AgentEvent::StartupFailed { message, .. }
            | AgentEvent::Notification { message, .. }
            | AgentEvent::OtherMessage { message, .. } => Some(message.as_str()),
            _ => None,
        }
    }

    pub fn usage(&self) -> Option<&TokenUsage> {
        match self {
            AgentEvent::TurnCompleted { usage, .. } => usage.as_ref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Clone)]
pub struct EventSender {
    inner: mpsc::UnboundedSender<AgentEvent>,
}

impl EventSender {
    pub fn new(tx: mpsc::UnboundedSender<AgentEvent>) -> Self {
        Self { inner: tx }
    }
    pub fn send(&self, ev: AgentEvent) {
        let _ = self.inner.send(ev);
    }
}
