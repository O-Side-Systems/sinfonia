//! HTTP handlers.
//!
//! P1-D scope: `health` plus a stub `webhook_stub` that returns 200 so a
//! configured GitHub webhook delivery doesn't bounce while the rest of
//! the bridge is being built. P1-E replaces `webhook_stub` with the real
//! HMAC-verifying, event-dispatching implementation.

use crate::webhook::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

/// `GET /health` — liveness probe. Returns the configured service name
/// and the tenant_id (when set) so a load balancer or operator can
/// distinguish multiple bridge instances at a glance.
pub async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": state.config.telemetry.service_name,
        "tenant_id": state.config.telemetry.tenant_id,
        "tracker": match state.config.tracker.kind {
            sinfonia_tracker::TrackerKind::Linear => "linear",
            sinfonia_tracker::TrackerKind::Jira => "jira",
        },
    }))
}

/// `POST /webhook` — P1-D stub.
///
/// Accepts and acknowledges any payload without inspection. Replaced in
/// P1-E with HMAC signature verification + `pull_request` / `check_suite` /
/// `workflow_run` dispatch + SQLite-based delivery-ID idempotency.
///
/// Returning 200 here is deliberate: configuring a webhook at
/// `https://…/webhook` against the P1-D bridge confirms reachability
/// without crashing GitHub's redelivery logic. A 501 would have been
/// more honest but would cause GitHub to flag the webhook as broken.
pub async fn webhook_stub(_state: State<AppState>, _body: String) -> StatusCode {
    StatusCode::OK
}
