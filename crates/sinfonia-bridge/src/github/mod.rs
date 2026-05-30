//! GitHub client surface used by the feedback loop.
//!
//! Three layers:
//!
//! - [`GhOps`] — the small async trait every consumer takes. Exposed so
//!   that the orchestrator and the label manager can be unit-tested with
//!   a counting fake (see `labels::tests`) without bringing up an
//!   HTTP server. The trait is intentionally narrow: only the operations
//!   the bridge actually performs.
//! - [`OctocrabGhOps`] — the production implementation backed by the
//!   `octocrab` crate, used for both PAT-mode (one client per process)
//!   and as the per-call wrapper around App-mode installation clients.
//! - [`auth`] — mode selection ([`BridgeAuthMode`]), private-key
//!   resolution ([`load_private_key`]), and the App-mode client cache
//!   ([`AppModeGhOps`]). [`build_gh_ops`] is the factory `main.rs` and
//!   the self-test runner call.

pub mod auth;
pub mod client;

pub use auth::{build_gh_ops, load_private_key, AppModeGhOps, BridgeAuthMode};
pub use client::{ArtifactMeta, CheckRunOutcome, CheckRunSummary, GhOps, OctocrabGhOps};
