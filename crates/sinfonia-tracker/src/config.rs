//! Minimal tracker-side configuration.
//!
//! `TrackerConfig` is the slice of a top-level Sinfonia config (or a
//! `BRIDGE.md` config) that the tracker layer actually needs. Both
//! `sinfonia` and `sinfonia-bridge` build a `TrackerConfig` from their own
//! full config and pass it to `LinearTracker::new` / `JiraTracker::new`.

use crate::error::{Error, Result};

/// Which tracker we're driving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerKind {
    Linear,
    Jira,
}

impl TrackerKind {
    /// Parse from the YAML string (`"linear"` / `"jira"`, case-insensitive).
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "linear" => Ok(TrackerKind::Linear),
            "jira" => Ok(TrackerKind::Jira),
            other => Err(Error::UnsupportedTrackerKind(other.to_string())),
        }
    }
}

/// The resolved tracker section of a Sinfonia / bridge config.
#[derive(Debug, Clone)]
pub struct TrackerConfig {
    pub kind: TrackerKind,
    /// Linear: GraphQL endpoint. Jira: base URL of the site.
    pub endpoint: String,
    pub api_key: Option<String>,
    /// `project_slug` for Linear; project key (e.g. "ABC") for Jira.
    pub project_slug: Option<String>,
    pub active_states: Vec<String>,
    pub terminal_states: Vec<String>,
    /// Jira-only: account email used with the API token for Basic auth.
    pub jira_email: Option<String>,
}
