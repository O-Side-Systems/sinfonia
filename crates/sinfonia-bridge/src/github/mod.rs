//! GitHub client surface used by the feedback loop.
//!
//! Two layers:
//!
//! - [`GhOps`] — the small async trait every consumer takes. Exposed so
//!   that the orchestrator and the label manager can be unit-tested with
//!   a counting fake (see `labels::tests`) without bringing up an
//!   HTTP server. The trait is intentionally narrow: only the operations
//!   the bridge actually performs.
//! - [`OctocrabGhOps`] — the production implementation backed by the
//!   `octocrab` crate. PAT-only auth in P1-F; App-mode auth + a per-
//!   installation client cache land in P1-G.

pub mod client;

pub use client::{CheckRunOutcome, CheckRunSummary, GhOps, OctocrabGhOps};
