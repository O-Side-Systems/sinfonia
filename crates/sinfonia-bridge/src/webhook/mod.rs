//! GitHub webhook endpoints.
//!
//! P1-E ships the real implementation:
//!
//! - `GET  /health`   — liveness probe (carried over from P1-D).
//! - `POST /webhook`  — HMAC-verifies and dispatches `pull_request` /
//!   `check_suite` / `workflow_run` events; persists delivery IDs for
//!   idempotency and the PR ↔ ticket mapping in SQLite. Ticket-state
//!   transitions themselves land in P1-F.

pub mod handlers;
pub mod verify;

use crate::feedback::budget::BudgetManager;
use crate::github::GhOps;
use crate::labels::LabelManager;
use crate::{storage::Store, BridgeConfig};
use axum::routing::{get, post};
use axum::Router;
use sinfonia_tracker::IssueTracker;
use std::sync::Arc;

/// Shared state injected into every handler via `axum::extract::State`.
///
/// P1-F extends the P1-E shape with:
/// - `gh`: the GitHub client (PAT-only in P1-F; P1-G adds App mode).
/// - `labels`: the [`LabelManager`] that short-circuits on
///   `manage_labels: false`.
///
/// Phase 3 adds:
/// - `budget`: the [`BudgetManager`] that handles cost/token cap
///   accounting and 30 s debounce flushing. The
///   `POST /api/v1/sinfonia-events` handler feeds it; the bridge's
///   own debounce reconciler task triggers idle flushes.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<BridgeConfig>,
    pub store: Arc<Store>,
    pub tracker: Arc<dyn IssueTracker>,
    pub gh: Arc<dyn GhOps>,
    pub labels: LabelManager,
    pub budget: BudgetManager,
}

impl AppState {
    pub fn new(
        config: BridgeConfig,
        store: Store,
        tracker: Arc<dyn IssueTracker>,
        gh: Arc<dyn GhOps>,
        labels: LabelManager,
        budget: BudgetManager,
    ) -> Self {
        Self {
            config: Arc::new(config),
            store: Arc::new(store),
            tracker,
            gh,
            labels,
            budget,
        }
    }

    /// Convenience constructor for tests + the existing P1-H integration
    /// suite — auto-builds a `BudgetManager` from the embedded cost
    /// table and the supplied config. Production code (`main.rs`) uses
    /// `AppState::new` directly so it can optionally override the cost
    /// table from `bridge.cost_table_path`.
    pub fn with_default_budget(
        config: BridgeConfig,
        store: Store,
        tracker: Arc<dyn IssueTracker>,
        gh: Arc<dyn GhOps>,
        labels: LabelManager,
    ) -> Self {
        let budget = BudgetManager::new(
            crate::feedback::cost::CostTable::embedded_default(),
            &config,
            tracker.clone(),
        );
        Self::new(config, store, tracker, gh, labels, budget)
    }
}

/// Build the axum router. All routes live here; handlers are in
/// [`handlers`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/webhook", post(handlers::webhook))
        // Phase 3 §7.2 — inbound typed Sinfonia event channel.
        .route(
            "/api/v1/sinfonia-events",
            post(crate::events::sinfonia_events),
        )
        .with_state(state)
}
