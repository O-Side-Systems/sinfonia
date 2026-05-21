//! `sinfonia-tracker` — issue-tracker abstraction shared by the Sinfonia
//! daemon and the v0.3 `sinfonia-bridge` feedback-loop service.
//!
//! Both consumers depend on this crate so that tracker writes (used only
//! by the bridge per spec §11.5) and tracker reads (used by both) go
//! through the same Linear / Jira adapters. The trait surface is:
//!
//! - **Reads** (`fetch_candidate_issues`, `fetch_issues_by_states`,
//!   `fetch_issue_states_by_ids`) — used by Sinfonia's polling loop.
//! - **Raw passthrough** (`raw_graphql`) — used by Sinfonia's optional
//!   `linear_graphql` client-side tool.
//!
//! Phase 1 of v0.3 adds the bridge-side write surface; see the trait below.
//!
//! The crate exports:
//! - [`IssueTracker`] — the async trait every adapter implements.
//! - [`Issue`], [`IssueState`], [`BlockerRef`], [`ChildRef`] — normalized
//!   tracker types (spec §4.1).
//! - [`TrackerKind`], [`TrackerConfig`] — the resolved configuration slice
//!   each adapter consumes at construction.
//! - [`LinearTracker`], [`JiraTracker`] — the bundled adapters.
//! - [`Error`], [`Result`] — the crate's error type.

pub mod config;
pub mod custom_fields;
pub mod error;
pub mod jira;
pub mod linear;
pub mod types;

pub use config::{TrackerConfig, TrackerKind};
pub use custom_fields::{CustomFieldKind, CustomFieldSchema, CustomFieldValue, FieldsMap};
pub use error::{Error, Result};
pub use jira::JiraTracker;
pub use linear::LinearTracker;
pub use types::{BlockerRef, ChildRef, Issue, IssueState};

use async_trait::async_trait;

/// The async tracker interface (spec §11).
///
/// Implementations MUST be `Send + Sync` because the orchestrator holds the
/// trait object in an `Arc<dyn IssueTracker>` and clones it across worker
/// tasks.
#[async_trait]
pub trait IssueTracker: Send + Sync {
    /// §11.1.1: list candidate issues in configured active states.
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>>;

    /// §11.1.2: list issues currently in the supplied state names. Used at
    /// daemon startup to reconcile in-flight workspaces with current tracker
    /// state.
    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>>;

    /// §11.1.3: minimal state refresh for the supplied issue IDs.
    async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<Vec<IssueState>>;

    /// Optional: raw GraphQL passthrough used by the `linear_graphql`
    /// client-side tool. Adapters that don't speak GraphQL (Jira) should
    /// leave the default implementation in place — it returns a typed error.
    async fn raw_graphql(
        &self,
        _query: &str,
        _variables: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        Err(Error::Other(
            "raw_graphql not supported for this tracker".into(),
        ))
    }

    // ---- Bridge-only write surface (v0.3, spec §11.5 + §11.6) -----------
    //
    // Sinfonia itself does not call any of these — the orchestrator never
    // writes to the tracker. They exist on the trait because the companion
    // `sinfonia-bridge` daemon needs them, and both binaries share this
    // crate. Each adapter that implements writes must do so idempotently
    // (Linear updates the marker comment in place; Jira upserts custom
    // fields).
    //
    // Adapters that don't support writes (e.g. Jira in Phase 1) leave the
    // default implementation in place, which returns a typed
    // `Error::Other("…not supported until Phase N")`.

    /// Move the issue to the supplied state. `target_state` is the raw
    /// tracker state name as it appears in the project workflow
    /// (e.g. `"Needs Fixes"`, `"Blocked - Human Review"`).
    async fn transition_issue(&self, _id: &str, _target_state: &str) -> Result<()> {
        Err(Error::Other(
            "transition_issue not supported for this tracker".into(),
        ))
    }

    /// Read a custom field value by stable key. Returns
    /// [`CustomFieldValue::Null`] if the field is unset on this ticket.
    async fn read_custom_field(&self, _id: &str, _key: &str) -> Result<CustomFieldValue> {
        Err(Error::Other(
            "read_custom_field not supported for this tracker".into(),
        ))
    }

    /// Write a single custom field value by stable key. Linear coalesces
    /// the write into the marker comment; Jira issues a `PUT` against the
    /// resolved `customfield_NNNNN` ID.
    async fn write_custom_field(
        &self,
        _id: &str,
        _key: &str,
        _value: CustomFieldValue,
    ) -> Result<()> {
        Err(Error::Other(
            "write_custom_field not supported for this tracker".into(),
        ))
    }

    /// Ensure a custom-field definition exists in the tracker, creating it
    /// if necessary. Idempotent. On Linear this is a no-op (the marker
    /// comment carries the schema implicitly); on Jira it creates the
    /// field via the REST API.
    async fn ensure_custom_field(&self, _schema: &CustomFieldSchema) -> Result<()> {
        Err(Error::Other(
            "ensure_custom_field not supported for this tracker".into(),
        ))
    }

    /// Post a free-form comment on the issue. Used by the bridge to leave
    /// human-readable failure-summary comments alongside the marker. The
    /// `body` is the raw Markdown (Linear) or ADF JSON (Jira, Phase 4).
    async fn post_comment(&self, _id: &str, _body: &str) -> Result<()> {
        Err(Error::Other(
            "post_comment not supported for this tracker".into(),
        ))
    }
}
