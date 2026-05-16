//! Issue tracker abstraction (spec §11). Linear + Jira adapters live here.

mod jira;
mod linear;

use crate::config::ServiceConfig;
use crate::config::TrackerKind;
use crate::domain::{Issue, IssueState};
use crate::errors::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub use jira::JiraTracker;
pub use linear::LinearTracker;

#[async_trait]
pub trait IssueTracker: Send + Sync {
    /// §11.1.1: list candidate issues in configured active states.
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>>;
    /// §11.1.2: list issues currently in the supplied state names (startup cleanup).
    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>>;
    /// §11.1.3: minimal state refresh for the supplied issue IDs (reconciliation).
    async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<Vec<IssueState>>;

    /// Optional: raw GraphQL passthrough used by the `linear_graphql` client-side tool.
    async fn raw_graphql(
        &self,
        _query: &str,
        _variables: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        Err(crate::errors::Error::Other(
            "raw_graphql not supported for this tracker".into(),
        ))
    }
}

/// Build the right tracker for the given config.
pub fn build_from_config(cfg: &ServiceConfig) -> Result<Arc<dyn IssueTracker>> {
    match cfg.tracker.kind {
        TrackerKind::Linear => Ok(Arc::new(LinearTracker::new(cfg)?)),
        TrackerKind::Jira => Ok(Arc::new(JiraTracker::new(cfg)?)),
    }
}
