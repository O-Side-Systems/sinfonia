//! Core domain model (spec §4) — orchestrator-side types.
//!
//! As of v0.3 the *tracker-facing* types (`Issue`, `IssueState`,
//! `BlockerRef`, `ChildRef`) live in the shared [`sinfonia_tracker`] crate
//! so the v0.3 `sinfonia-bridge` companion daemon can consume them too.
//! They're re-exported here so existing `crate::domain::Issue` call sites
//! keep working unchanged.
//!
//! Orchestrator-only state (live sessions, retry entries, the in-memory
//! state map, token totals) stays in this module — none of it is meaningful
//! outside the polling daemon.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub use sinfonia_tracker::{BlockerRef, ChildRef, Issue, IssueState};

/// Live session metadata while a coding-agent subprocess is running. §4.1.6.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiveSession {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub codex_app_server_pid: Option<String>,
    pub last_codex_event: Option<String>,
    pub last_codex_timestamp: Option<DateTime<Utc>>,
    pub last_codex_message: Option<String>,
    pub codex_input_tokens: u64,
    pub codex_output_tokens: u64,
    pub codex_total_tokens: u64,
    pub last_reported_input_tokens: u64,
    pub last_reported_output_tokens: u64,
    pub last_reported_total_tokens: u64,
    pub turn_count: u32,
}

/// Running entry held in the orchestrator's `running` map. §4.1.8.
#[derive(Debug, Clone)]
pub struct RunningEntry {
    pub issue_id: String,
    pub identifier: String,
    pub issue: Issue,
    pub workspace_path: String,
    pub session: LiveSession,
    pub retry_attempt: Option<u32>,
    pub started_at: DateTime<Utc>,
}

/// Scheduled retry state. §4.1.7.
#[derive(Debug, Clone)]
pub struct RetryEntry {
    pub issue_id: String,
    pub identifier: String,
    pub attempt: u32,
    /// Wall-clock fire time.
    pub due_at: DateTime<Utc>,
    pub error: Option<String>,
}

/// Aggregate token + runtime totals.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub seconds_running: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateLimitSnapshot(pub serde_json::Value);

/// Single-authority orchestrator state. §4.1.8.
#[derive(Debug, Default)]
pub struct OrchestratorState {
    pub poll_interval_ms: u64,
    pub max_concurrent_agents: u32,
    pub running: HashMap<String, RunningEntry>,
    pub claimed: HashSet<String>,
    pub retry_attempts: HashMap<String, RetryEntry>,
    pub completed: HashSet<String>,
    pub codex_totals: CodexTotals,
    pub codex_rate_limits: Option<RateLimitSnapshot>,
}

/// Sanitize an issue identifier into a workspace directory name (§4.2, §9.5).
///
/// Only `[A-Za-z0-9._-]` is permitted; any other character becomes `_`.
pub fn sanitize_workspace_key(identifier: &str) -> String {
    identifier
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_safe_chars() {
        assert_eq!(sanitize_workspace_key("ABC-123"), "ABC-123");
        assert_eq!(sanitize_workspace_key("foo.bar_2"), "foo.bar_2");
    }

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(sanitize_workspace_key("../etc/passwd"), ".._etc_passwd");
        assert_eq!(sanitize_workspace_key("a b/c"), "a_b_c");
    }
}
