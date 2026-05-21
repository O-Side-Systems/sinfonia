//! Error type for tracker operations.
//!
//! This is intentionally a *separate* error type from the top-level Sinfonia
//! error. Tracker code is shared between the daemon (`sinfonia`) and the
//! companion bridge (`sinfonia-bridge`), and each consumer wraps the tracker
//! error into its own top-level error type via `#[from]`. Keeping the types
//! split prevents the tracker crate from accidentally taking a dependency
//! on either consumer's concerns (workspace lifecycle, orchestrator state,
//! webhook plumbing) and keeps the public API of `sinfonia-tracker` small.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    // --- Configuration (mirrors spec §11.4 error vocabulary) ---
    #[error("unsupported_tracker_kind: {0}")]
    UnsupportedTrackerKind(String),
    #[error("missing_tracker_api_key")]
    MissingTrackerApiKey,
    #[error("missing_tracker_project_slug")]
    MissingTrackerProjectSlug,
    #[error("config_invalid: {0}")]
    ConfigInvalid(String),

    // --- Linear (spec §11.4) ---
    #[error("linear_api_request: {0}")]
    LinearApiRequest(String),
    #[error("linear_api_status: {0}")]
    LinearApiStatus(String),
    #[error("linear_graphql_errors: {0}")]
    LinearGraphqlErrors(String),
    #[error("linear_unknown_payload: {0}")]
    LinearUnknownPayload(String),
    #[error("linear_missing_end_cursor")]
    LinearMissingEndCursor,

    // --- Jira (spec §11.4) ---
    #[error("jira_api_request: {0}")]
    JiraApiRequest(String),
    #[error("jira_api_status: {0}")]
    JiraApiStatus(String),
    #[error("jira_unknown_payload: {0}")]
    JiraUnknownPayload(String),

    // --- IO / generic ---
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
