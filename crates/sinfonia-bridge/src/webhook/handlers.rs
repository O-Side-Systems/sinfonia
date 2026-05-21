//! HTTP handlers.
//!
//! `health` is the trivial liveness probe; `webhook` is the real GitHub
//! webhook endpoint. The flow:
//!
//! 1. Read raw body bytes (axum [`Bytes`] preserves the payload exactly).
//! 2. Verify the `X-Hub-Signature-256` HMAC via [`super::verify`]. Any
//!    failure short-circuits to **401** with no DB mutation.
//! 3. Idempotency-record the `X-GitHub-Delivery` ID in SQLite. A
//!    duplicate becomes a **200** no-op.
//! 4. Dispatch on `X-GitHub-Event`:
//!    - `pull_request` (opened / synchronize) → extract a tracker
//!      identifier with `feedback_loop.pr_link_pattern` and upsert
//!      `pr_ticket_map`; respond **202 queued**.
//!    - `check_suite` / `workflow_run` (action `completed`) →
//!      acknowledge with **202 queued**; the real CI evaluation lands
//!      in P1-F.
//!    - anything else → **200** with a `"ignored"` body, logged at
//!      `debug`.
//!
//! See `01-bridge-mvp.md` §5 for the canonical event flow.

use crate::webhook::{verify::verify_signature, AppState};
use crate::Error;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

/// `GET /health` — liveness probe. Returns the configured service name,
/// the tenant_id (when set), and the tracker kind so a load balancer or
/// operator can distinguish multiple bridge instances at a glance.
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

const HEADER_EVENT: &str = "X-GitHub-Event";
const HEADER_DELIVERY: &str = "X-GitHub-Delivery";

/// `POST /webhook` — full implementation. See module docs.
pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    // -- 1. HMAC verify --------------------------------------------------
    let secret = match state.config.github.webhook_secret.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            warn!(
                target: "webhook",
                "webhook received but github.webhook_secret is not configured; rejecting"
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "webhook_secret not configured"})),
            )
                .into_response();
        }
    };

    let sig_header = headers
        .get(super::verify::SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok());
    if let Err(e) = verify_signature(&body, sig_header, secret) {
        debug!(target: "webhook", error = %e, "signature verification failed");
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }

    // -- 2. Idempotency --------------------------------------------------
    //
    // GitHub guarantees a unique `X-GitHub-Delivery` UUID per attempt.
    // Any redelivery (5xx retry, replay) reuses the same value, so this
    // is the canonical idempotency key.
    let delivery_id = headers
        .get(HEADER_DELIVERY)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let delivery_id = match delivery_id {
        Some(s) if !s.is_empty() => s,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("missing {HEADER_DELIVERY} header")})),
            )
                .into_response();
        }
    };

    if let Err(e) = state.store.record_delivery(&delivery_id).await {
        if matches!(&e, Error::Storage(s) if s == "duplicate") {
            debug!(target: "webhook", %delivery_id, "duplicate delivery; no-op");
            return (
                StatusCode::OK,
                Json(json!({"status": "duplicate", "delivery_id": delivery_id})),
            )
                .into_response();
        }
        warn!(target: "webhook", error = %e, "record_delivery failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }

    // -- 3. Parse + dispatch --------------------------------------------
    let event = headers
        .get(HEADER_EVENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            warn!(target: "webhook", %event, error = %e, "invalid JSON body");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid JSON: {e}")})),
            )
                .into_response();
        }
    };

    match event.as_str() {
        "pull_request" => handle_pull_request(&state, &payload, &delivery_id).await,
        "check_suite" => handle_check_suite(&payload, &delivery_id).await,
        "workflow_run" => handle_workflow_run(&payload, &delivery_id).await,
        "ping" => {
            // GitHub's startup probe. Acknowledge with 200 so the webhook
            // configuration shows green in the UI.
            info!(target: "webhook", %delivery_id, "ping received");
            (
                StatusCode::OK,
                Json(json!({"status": "pong", "delivery_id": delivery_id})),
            )
                .into_response()
        }
        other => {
            debug!(target: "webhook", event = %other, %delivery_id, "event ignored");
            (
                StatusCode::OK,
                Json(json!({"status": "ignored", "event": other, "delivery_id": delivery_id})),
            )
                .into_response()
        }
    }
}

async fn handle_pull_request(
    state: &AppState,
    payload: &Value,
    delivery_id: &str,
) -> axum::response::Response {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let repo = payload
        .get("repository")
        .and_then(|v| v.get("full_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pr = payload.get("pull_request");
    let pr_number = payload
        .get("number")
        .and_then(|v| v.as_u64())
        .or_else(|| pr.and_then(|p| p.get("number")).and_then(|v| v.as_u64()));
    let title = pr.and_then(|p| p.get("title")).and_then(|v| v.as_str()).unwrap_or("");
    let body = pr.and_then(|p| p.get("body")).and_then(|v| v.as_str()).unwrap_or("");

    let pr_number = match pr_number {
        Some(n) => n,
        None => {
            warn!(target: "webhook", %delivery_id, "pull_request payload missing PR number");
            return (
                StatusCode::OK,
                Json(json!({"status": "ignored", "reason": "missing pr number"})),
            )
                .into_response();
        }
    };

    // Only `opened` and `synchronize` populate the mapping. Other actions
    // (closed, edited, labeled, …) don't change which ticket the PR
    // belongs to, so we leave the row alone.
    if action != "opened" && action != "synchronize" && action != "reopened" {
        debug!(
            target: "webhook",
            %delivery_id, action, repo, pr_number,
            "pull_request action not mapped"
        );
        return (
            StatusCode::OK,
            Json(json!({
                "status": "ignored",
                "reason": "action does not update mapping",
                "action": action,
            })),
        )
            .into_response();
    }

    // Match the regex against body first, then title. The plan example
    // pattern (`closes ENG-42`) is normally in the PR body but a few
    // contributors put it in the title.
    let combined = format!("{body}\n{title}");
    let ticket_id = state
        .config
        .feedback_loop
        .pr_link_pattern
        .captures(&combined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string());

    let ticket_id = match ticket_id {
        Some(id) => id,
        None => {
            debug!(
                target: "webhook",
                %delivery_id, repo, pr_number,
                "no tracker link in PR; skipping"
            );
            return (
                StatusCode::OK,
                Json(json!({
                    "status": "ignored",
                    "reason": "no tracker link in PR",
                    "delivery_id": delivery_id,
                })),
            )
                .into_response();
        }
    };

    if let Err(e) = state
        .store
        .upsert_pr_ticket(repo, pr_number, &ticket_id)
        .await
    {
        warn!(target: "webhook", error = %e, repo, pr_number, "upsert_pr_ticket failed");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }

    info!(
        target: "webhook",
        %delivery_id, repo, pr_number, %ticket_id, action,
        "pr→ticket mapping upserted"
    );
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "queued",
            "event": "pull_request",
            "action": action,
            "repo": repo,
            "pr_number": pr_number,
            "ticket_id": ticket_id,
            "delivery_id": delivery_id,
        })),
    )
        .into_response()
}

async fn handle_check_suite(payload: &Value, delivery_id: &str) -> axum::response::Response {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if action != "completed" {
        debug!(target: "webhook", %delivery_id, action, "check_suite action not completed");
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "not completed", "action": action})),
        )
            .into_response();
    }
    // P1-F replaces this with the real CI evaluation. P1-E only records
    // that the event was received — the delivery_id is already in
    // `processed_deliveries`, which is the visibility test the plan
    // calls for in the §2 exit criteria.
    info!(target: "webhook", %delivery_id, "check_suite completed received (queued for P1-F)");
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "queued",
            "event": "check_suite",
            "action": action,
            "delivery_id": delivery_id,
        })),
    )
        .into_response()
}

async fn handle_workflow_run(payload: &Value, delivery_id: &str) -> axum::response::Response {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if action != "completed" {
        debug!(target: "webhook", %delivery_id, action, "workflow_run action not completed");
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "not completed", "action": action})),
        )
            .into_response();
    }
    info!(target: "webhook", %delivery_id, "workflow_run completed received (queued for P1-F)");
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "queued",
            "event": "workflow_run",
            "action": action,
            "delivery_id": delivery_id,
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Tests — end-to-end through the axum service. wiremock-driven coverage
// of the nine §9.2 scenarios is P1-H's responsibility; the unit tests
// below verify the handler's own dispatch logic against the §9.1 table.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_bridge_str;
    use crate::storage::Store;
    use crate::webhook::router;
    use axum::body::Body;
    use axum::http::Request;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use sinfonia_tracker::{LinearTracker, TrackerConfig, TrackerKind};
    use std::sync::Arc;
    use tower::ServiceExt;

    type HmacSha256 = Hmac<Sha256>;

    fn baseline_bridge_cfg() -> &'static str {
        r#"---
tracker:
  kind: linear
  api_key: test-key
  project_slug: my-project
github:
  webhook_secret: shh
  pat: ghp_xxx
feedback_loop:
  max_attempts: 5
  needs_fixes_state: "Needs Fixes"
  blocked_state: "Blocked - Human Review"
custom_fields:
  attempt_count: sinfonia_attempt_count
  last_failure_log: sinfonia_last_ci_failure
  max_attempts_override: sinfonia_max_attempts
  failure_category: sinfonia_failure_category
  tokens_consumed: sinfonia_tokens_consumed
  cost_consumed_usd: sinfonia_cost_consumed_usd
  max_cost_override_usd: sinfonia_max_cost_usd
server:
  bind: "0.0.0.0"
  port: 8081
storage:
  state_db_path: /tmp/test-bridge.db
telemetry:
  service_name: sinfonia-bridge
---
"#
    }

    async fn make_state() -> AppState {
        let cfg = parse_bridge_str(baseline_bridge_cfg()).expect("baseline parses");
        let store = Store::open_in_memory().await.expect("store");
        let tracker_cfg = TrackerConfig {
            kind: TrackerKind::Linear,
            endpoint: "https://api.linear.app/graphql".into(),
            api_key: Some("test".into()),
            project_slug: Some("my-project".into()),
            active_states: vec![],
            terminal_states: vec![],
            jira_email: None,
        };
        let tracker = Arc::new(LinearTracker::new(&tracker_cfg).expect("linear tracker"));
        AppState::new(cfg, store, tracker)
    }

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let digest = mac.finalize().into_bytes();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256={hex}")
    }

    fn pr_payload(action: &str, repo: &str, number: u64, body: &str, title: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "action": action,
            "number": number,
            "pull_request": {
                "number": number,
                "title": title,
                "body": body,
            },
            "repository": {
                "full_name": repo,
            },
        }))
        .unwrap()
    }

    async fn post_webhook(
        state: AppState,
        event: &str,
        delivery: &str,
        body: Vec<u8>,
        sig: Option<String>,
    ) -> axum::http::Response<Body> {
        let app = router(state);
        let mut req = Request::builder()
            .uri("/webhook")
            .method("POST")
            .header("X-GitHub-Event", event)
            .header("X-GitHub-Delivery", delivery)
            .header("content-type", "application/json");
        if let Some(s) = sig {
            req = req.header(super::super::verify::SIGNATURE_HEADER, s);
        }
        let req = req.body(Body::from(body)).expect("build request");
        app.oneshot(req).await.expect("service oneshot")
    }

    #[tokio::test]
    async fn pull_request_opened_with_link_upserts_mapping() {
        let state = make_state().await;
        let store = state.store.clone();
        let body = pr_payload(
            "opened",
            "acme/widgets",
            42,
            "This change closes ENG-7.",
            "feat: add widget",
        );
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "pull_request", "deliv-001", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let ticket = store
            .lookup_pr_ticket("acme/widgets", 42)
            .await
            .expect("lookup");
        assert_eq!(ticket.as_deref(), Some("ENG-7"));
    }

    #[tokio::test]
    async fn pull_request_without_link_is_no_op() {
        let state = make_state().await;
        let store = state.store.clone();
        let body = pr_payload("opened", "acme/widgets", 7, "no tracker mention", "x");
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "pull_request", "deliv-002", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            store
                .lookup_pr_ticket("acme/widgets", 7)
                .await
                .expect("lookup"),
            None
        );
    }

    #[tokio::test]
    async fn invalid_hmac_rejected() {
        let state = make_state().await;
        let store = state.store.clone();
        let body = pr_payload("opened", "acme/widgets", 1, "closes ENG-1", "x");
        // Sign with the wrong secret.
        let bad_sig = sign("not-the-real-secret", &body);
        let resp = post_webhook(state, "pull_request", "deliv-003", body, Some(bad_sig)).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        // No DB mutation.
        assert_eq!(
            store
                .lookup_pr_ticket("acme/widgets", 1)
                .await
                .expect("lookup"),
            None
        );
    }

    #[tokio::test]
    async fn missing_signature_header_rejected() {
        let state = make_state().await;
        let body = pr_payload("opened", "acme/widgets", 1, "closes ENG-1", "x");
        let resp = post_webhook(state, "pull_request", "deliv-004", body, None).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn duplicate_delivery_returns_200_no_op() {
        let state = make_state().await;
        let store = state.store.clone();
        let body = pr_payload(
            "opened",
            "acme/widgets",
            55,
            "closes ENG-55",
            "x",
        );
        let sig = sign("shh", &body);

        // First delivery — accepted.
        let resp1 = post_webhook(
            state.clone(),
            "pull_request",
            "delivery-dup",
            body.clone(),
            Some(sig.clone()),
        )
        .await;
        assert_eq!(resp1.status(), StatusCode::ACCEPTED);

        // Second delivery with same ID — duplicate, 200.
        let resp2 = post_webhook(
            state,
            "pull_request",
            "delivery-dup",
            body,
            Some(sig),
        )
        .await;
        assert_eq!(resp2.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp2.into_body(), 1024)
            .await
            .expect("body");
        let v: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], json!("duplicate"));

        // Mapping should reflect the first delivery only (one row, ENG-55).
        assert_eq!(
            store
                .lookup_pr_ticket("acme/widgets", 55)
                .await
                .expect("lookup"),
            Some("ENG-55".into()),
        );
    }

    #[tokio::test]
    async fn check_suite_completed_acknowledged() {
        let state = make_state().await;
        let body = serde_json::to_vec(&json!({"action": "completed"})).unwrap();
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "check_suite", "deliv-cs-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn workflow_run_completed_acknowledged() {
        let state = make_state().await;
        let body = serde_json::to_vec(&json!({"action": "completed"})).unwrap();
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "workflow_run", "deliv-wf-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn unknown_event_is_ignored() {
        let state = make_state().await;
        let body = serde_json::to_vec(&json!({"hello": "world"})).unwrap();
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "issues", "deliv-other-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ping_event_acknowledged() {
        let state = make_state().await;
        let body = serde_json::to_vec(&json!({"zen": "Speak like a human."})).unwrap();
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "ping", "deliv-ping-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_delivery_header_rejected() {
        let state = make_state().await;
        let app = router(state);
        let body = pr_payload("opened", "acme/widgets", 1, "closes ENG-1", "x");
        let sig = sign("shh", &body);
        let req = Request::builder()
            .uri("/webhook")
            .method("POST")
            .header("X-GitHub-Event", "pull_request")
            .header(super::super::verify::SIGNATURE_HEADER, sig)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("build request");
        let resp = app.oneshot(req).await.expect("service oneshot");
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn pull_request_link_from_title_is_used() {
        let state = make_state().await;
        let store = state.store.clone();
        let body = pr_payload(
            "synchronize",
            "acme/widgets",
            13,
            "long body without any tracker link mentioned at all",
            "fixes ENG-13: tighten the gizmo",
        );
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "pull_request", "deliv-title", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert_eq!(
            store
                .lookup_pr_ticket("acme/widgets", 13)
                .await
                .expect("lookup"),
            Some("ENG-13".into()),
        );
    }
}
