//! Normalized issue-tracker domain types (spec §4.1).
//!
//! These are the types every `IssueTracker` adapter returns. They are kept
//! in the shared `sinfonia-tracker` crate because both the daemon and the
//! bridge consume them. Adapter-specific representations (Linear GraphQL
//! responses, Jira REST payloads) get converted into these types at the
//! adapter boundary so downstream code is tracker-agnostic.

use crate::custom_fields::FieldsMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A reference to an upstream blocker (spec §4.1.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerRef {
    pub id: Option<String>,
    pub identifier: Option<String>,
    pub state: Option<String>,
}

/// Lightweight child-issue record used to gate parent eligibility.
///
/// `state` is the raw tracker state name (matched case-insensitively against
/// `terminal_states` during dispatch).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildRef {
    pub id: Option<String>,
    pub identifier: Option<String>,
    pub state: String,
}

/// Normalized issue record (spec §4.1.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: String,
    pub branch_name: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<BlockerRef>,
    /// Sub-issues (Linear `children`, Jira classic `subtasks`). Used to gate
    /// parent execution until all children reach a `terminal_states` value —
    /// mirroring the way a human works the leaves of an epic before the epic
    /// itself.
    #[serde(default)]
    pub children: Vec<ChildRef>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    /// Bridge-written custom-field values (spec §11.6 / §11.7), exposed to
    /// the Liquid prompt scope as `{{ issue.fields.<key> }}`. Linear
    /// populates this from the bot-owned marker comment; Jira populates it
    /// from the corresponding `customfield_NNNNN` reads (Phase 4).
    ///
    /// Templates that reference a field MUST use a `| default:` filter —
    /// strict Liquid errors on unset fields. The `setup-state-machine`
    /// skill enforces this with a generator-side grep check.
    #[serde(default)]
    pub fields: FieldsMap,
}

impl Issue {
    /// Normalized state for comparison (spec §4.2).
    pub fn normalized_state(&self) -> String {
        self.state.to_lowercase()
    }
}

/// Lighter form returned by state-only refresh queries (spec §11.1.3).
#[derive(Debug, Clone)]
pub struct IssueState {
    pub id: String,
    pub identifier: String,
    pub state: String,
}
