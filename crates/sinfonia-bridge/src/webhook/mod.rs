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

use crate::{storage::Store, BridgeConfig};
use axum::routing::{get, post};
use axum::Router;
use sinfonia_tracker::IssueTracker;
use std::sync::Arc;

/// Shared state injected into every handler via `axum::extract::State`.
///
/// P1-E extends the P1-D shape with the SQLite [`Store`] (idempotency +
/// PR ↔ ticket map) and an `Arc<dyn IssueTracker>` so handlers can
/// dispatch tracker writes when P1-F lands without re-reading config or
/// re-instantiating an adapter per request.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<BridgeConfig>,
    pub store: Arc<Store>,
    pub tracker: Arc<dyn IssueTracker>,
}

impl AppState {
    pub fn new(config: BridgeConfig, store: Store, tracker: Arc<dyn IssueTracker>) -> Self {
        Self {
            config: Arc::new(config),
            store: Arc::new(store),
            tracker,
        }
    }
}

/// Build the axum router. All routes live here; handlers are in
/// [`handlers`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/webhook", post(handlers::webhook))
        .with_state(state)
}
