//! Thin shim over the shared [`sinfonia_tracker`] crate.
//!
//! The trait, the Linear/Jira adapters, and the normalized issue types
//! all live in `sinfonia-tracker` so that the v0.3 `sinfonia-bridge`
//! companion daemon can reuse them. This module re-exports the public
//! surface under `sinfonia::tracker::*` (preserved across the v0.3
//! workspace conversion) and provides the `build_from_config` factory
//! that depends on Sinfonia's full [`ServiceConfig`] — and therefore
//! cannot live in the shared crate.

use std::sync::Arc;

pub use sinfonia_tracker::{
    BlockerRef, ChildRef, Issue, IssueState, IssueTracker, JiraTracker, LinearTracker,
    TrackerConfig, TrackerKind,
};

use crate::config::ServiceConfig;
use crate::errors::Result;

/// Build the right tracker adapter for the given top-level Sinfonia config.
///
/// Internally converts the `tracker:` slice of `ServiceConfig` into a
/// [`TrackerConfig`] and dispatches on `kind`.
pub fn build_from_config(cfg: &ServiceConfig) -> Result<Arc<dyn IssueTracker>> {
    let tcfg = &cfg.tracker;
    let arc: Arc<dyn IssueTracker> = match tcfg.kind {
        TrackerKind::Linear => Arc::new(LinearTracker::new(tcfg)?),
        TrackerKind::Jira => Arc::new(JiraTracker::new(tcfg)?),
    };
    Ok(arc)
}
