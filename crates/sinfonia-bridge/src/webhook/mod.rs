//! GitHub webhook endpoints.
//!
//! P1-D ships only the skeleton — an axum [`router`] with `GET /health` and
//! a stub `POST /webhook` that returns 200 without doing any work. The
//! real handler logic (HMAC verification, `pull_request` / `check_suite` /
//! `workflow_run` dispatch, SQLite idempotency) lands in P1-E.

pub mod handlers;

use crate::BridgeConfig;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

/// Shared state injected into every handler via `axum::extract::State`.
///
/// Wraps the parsed `BridgeConfig` so handlers can read configuration
/// without holding their own copy. Later sub-tasks extend this with the
/// SQLite store handle (P1-E), the `octocrab` client(s) (P1-G), and the
/// telemetry / event subscriber list (Phase 3).
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<BridgeConfig>,
}

impl AppState {
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}

/// Build the axum router. All routes live here; handlers are in
/// [`handlers`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/webhook", post(handlers::webhook_stub))
        .with_state(state)
}
