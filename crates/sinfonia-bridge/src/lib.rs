//! `sinfonia-bridge` — companion CI feedback-loop daemon for Sinfonia.
//!
//! The bridge is the v0.3 answer to "what closes the CI → tracker → agent
//! retry loop?" It runs as a sibling daemon (or as a standalone host, or
//! as a GitHub Actions step), listens for GitHub `pull_request` /
//! `check_suite` / `workflow_run` webhook events, correlates each event
//! to a tracker ticket, and:
//!
//! - leaves the ticket alone if CI is green (humans review on their own time);
//! - transitions the ticket back to a `Needs Fixes` state on red CI and
//!   increments an attempt counter custom field — Sinfonia then picks up
//!   the ticket on its next poll and runs the configured agent;
//! - transitions the ticket to a `Blocked` state if the attempt cap or
//!   the configured budget cap is hit.
//!
//! The bridge is the only component in the v0.3 architecture that holds
//! both tracker-write credentials and GitHub webhook secrets. Sinfonia
//! itself stays narrowly focused on "ticket is in an active state → run
//! agent." See `docs/v0.3-plan/01-bridge-mvp.md` for the full design.
//!
//! ## Module tree
//!
//! - [`config`] — `BRIDGE.md` parser + schema validation. Stable in P1-D.
//! - [`webhook`] — axum routes for GitHub webhooks. Skeleton in P1-D;
//!   full handler logic + HMAC verify + idempotency in P1-E.
//! - Later sub-tasks add `feedback/`, `labels`, `github/`, `storage`,
//!   and `selftest` modules; declared here as they land so that this
//!   tree mirrors the layout in the phase plan.
//!
//! ## Error model
//!
//! [`Error`] is the bridge's own error type; tracker errors flow in via
//! `#[from] sinfonia_tracker::Error`. The pattern matches `sinfonia`'s
//! own error type — the two binaries do not share an error type because
//! they own genuinely different concerns.

pub mod config;
pub mod feedback;
pub mod github;
pub mod labels;
pub mod selftest;
pub mod storage;
pub mod webhook;

pub use config::BridgeConfig;
pub use github::{build_gh_ops, BridgeAuthMode, GhOps, OctocrabGhOps};
pub use labels::{BridgeLabel, LabelManager};
pub use selftest::{run_selftest, CheckLine, CheckResult};
pub use storage::Store;

use thiserror::Error;

/// Bridge-level error type.
#[derive(Debug, Error)]
pub enum Error {
    /// Anything propagated from the tracker layer. Wrapped via `#[from]`
    /// so `?` works transparently against `sinfonia_tracker::Result<_>`.
    #[error(transparent)]
    Tracker(#[from] sinfonia_tracker::Error),

    // --- BRIDGE.md parsing + validation ----------------------------------
    #[error("missing_bridge_file: {0}")]
    MissingBridgeFile(String),
    #[error("bridge_parse_error: {0}")]
    BridgeParseError(String),
    #[error("bridge_front_matter_not_a_map")]
    BridgeFrontMatterNotMap,
    #[error("bridge_config_invalid: {0}")]
    BridgeConfigInvalid(String),

    // --- Runtime (filled in by P1-E / P1-F / P1-G) -----------------------
    /// Reserved for the webhook layer in P1-E.
    #[error("webhook: {0}")]
    Webhook(String),
    /// Reserved for the GitHub client layer in P1-G.
    #[error("github: {0}")]
    GitHub(String),
    /// Reserved for the SQLite storage layer in P1-E.
    #[error("storage: {0}")]
    Storage(String),

    // --- IO / generic ----------------------------------------------------
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
