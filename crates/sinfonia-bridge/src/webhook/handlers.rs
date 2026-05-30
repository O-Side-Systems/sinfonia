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

use crate::feedback::{evaluate_ci, CiOutcome, EvaluateContext};
use crate::telemetry::spans;
use crate::webhook::{verify::verify_signature, AppState};
use crate::Error;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};
use tracing::{debug, info, info_span, warn, Instrument};

/// Resolve the tenant id from BridgeConfig for span tagging. Cheap helper
/// that avoids re-stringifying `Option<String>` at each emission site.
fn tenant_str(state: &AppState) -> String {
    state
        .config
        .telemetry
        .tenant_id
        .clone()
        .unwrap_or_else(|| crate::telemetry::tenant::DEFAULT_TENANT.to_string())
}

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
    let span = info_span!(
        target: "webhook",
        spans::BRIDGE_WEBHOOK,
        { spans::ATTR_TENANT_ID } = %tenant_str(&state),
        { spans::ATTR_EVENT_TYPE } = tracing::field::Empty,
        { spans::ATTR_DELIVERY_ID } = tracing::field::Empty,
        { spans::ATTR_REPO } = tracing::field::Empty,
        { spans::ATTR_DURATION_MS } = tracing::field::Empty,
    );
    let started = std::time::Instant::now();
    let response = webhook_inner(state, headers, body).instrument(span.clone()).await;
    span.record(
        spans::ATTR_DURATION_MS,
        started.elapsed().as_millis() as i64,
    );
    response
}

async fn webhook_inner(
    state: AppState,
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
    let current_span = tracing::Span::current();
    current_span.record(spans::ATTR_EVENT_TYPE, event.as_str());
    current_span.record(spans::ATTR_DELIVERY_ID, delivery_id.as_str());
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
        "check_suite" => handle_check_suite(&state, &payload, &delivery_id).await,
        "workflow_run" => handle_workflow_run(&state, &payload, &delivery_id).await,
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
    tracing::Span::current().record(spans::ATTR_REPO, repo);
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

    // Phase 3 §6 terminal-state detection. When a PR closes with
    // merged=true, look up the linked ticket, flush any pending
    // accumulator deltas, and emit a transition-to-done span so the
    // analytics layer can count attempts-to-close and cost-per-ticket
    // histograms span-derived (per the M-2 / metrics-deferral note).
    if action == "closed" {
        let merged = pr.and_then(|p| p.get("merged")).and_then(|v| v.as_bool());
        if matches!(merged, Some(true)) {
            if let Some(ticket_id) = state.store.lookup_pr_ticket(repo, pr_number).await.ok().flatten() {
                if let Err(e) = state.budget.flush_ticket(&ticket_id).await {
                    warn!(
                        target: "webhook",
                        error = %e, ticket = %ticket_id,
                        "pr.closed.merged budget flush failed"
                    );
                }
                info!(
                    target: "webhook",
                    %delivery_id, repo, pr_number, %ticket_id,
                    "pr closed (merged); ticket reached terminal-via-our-pipeline state"
                );
                return (
                    StatusCode::ACCEPTED,
                    Json(json!({
                        "status": "merged",
                        "event": "pull_request",
                        "action": action,
                        "repo": repo,
                        "pr_number": pr_number,
                        "ticket_id": ticket_id,
                        "delivery_id": delivery_id,
                    })),
                )
                    .into_response();
            }
        }
    }

    // Only `opened` and `synchronize` populate the mapping. Other actions
    // (closed without merge, edited, labeled, …) don't change which
    // ticket the PR belongs to, so we leave the row alone.
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

async fn handle_check_suite(
    state: &AppState,
    payload: &Value,
    delivery_id: &str,
) -> axum::response::Response {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if action != "completed" {
        debug!(target: "webhook", %delivery_id, action, "check_suite action not completed");
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "not completed", "action": action})),
        )
            .into_response();
    }
    dispatch_ci_event(state, "check_suite", payload, delivery_id, action).await
}

async fn handle_workflow_run(
    state: &AppState,
    payload: &Value,
    delivery_id: &str,
) -> axum::response::Response {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if action != "completed" {
        debug!(target: "webhook", %delivery_id, action, "workflow_run action not completed");
        return (
            StatusCode::OK,
            Json(json!({"status": "ignored", "reason": "not completed", "action": action})),
        )
            .into_response();
    }
    dispatch_ci_event(state, "workflow_run", payload, delivery_id, action).await
}

/// Run `evaluate_ci` for a `check_suite` or `workflow_run` event and
/// turn the per-PR outcomes into a single HTTP response.
///
/// HTTP status:
/// - 202 ACCEPTED whenever the bridge took an action (any non-Pending,
///   non-NoMappedPr outcome on any PR).
/// - 200 OK when every outcome is `Pending` or `NoMappedPr` — both are
///   "the bridge saw the event but had nothing to do this time."
async fn dispatch_ci_event(
    state: &AppState,
    event: &str,
    payload: &Value,
    delivery_id: &str,
    action: &str,
) -> axum::response::Response {
    let repo = payload
        .get("repository")
        .and_then(|v| v.get("full_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    tracing::Span::current().record(spans::ATTR_REPO, repo);

    let ctx = EvaluateContext {
        config: state.config.as_ref(),
        store: state.store.as_ref(),
        tracker: state.tracker.as_ref(),
        gh: &state.gh,
        labels: &state.labels,
    };

    let ci_span = info_span!(
        target: "feedback",
        spans::BRIDGE_CI_RESULT,
        { spans::ATTR_TENANT_ID } = %tenant_str(state),
        { spans::ATTR_EVENT_TYPE } = event,
        { spans::ATTR_REPO } = repo,
        { spans::ATTR_OUTCOME } = tracing::field::Empty,
        { spans::ATTR_ATTEMPT_COUNT } = tracing::field::Empty,
        { spans::ATTR_FAILURE_CATEGORY } = tracing::field::Empty,
    );
    let outcomes = match evaluate_ci(ctx, event, payload)
        .instrument(ci_span.clone())
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!(target: "webhook", event, %delivery_id, error = %e, "evaluate_ci failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string(), "delivery_id": delivery_id})),
            )
                .into_response();
        }
    };

    // Annotate the bridge.ci_result span with the first-PR outcome shape.
    // Most webhook deliveries carry a single PR's worth of outcomes; for
    // the rare multi-PR fan-out the span attribute reflects the headline
    // result and the JSON body still carries the full per-PR breakdown.
    if let Some(first) = outcomes.first() {
        let (outcome_label, category, attempt) = match first {
            CiOutcome::Green => ("green", None, None),
            CiOutcome::Red {
                category,
                next_attempt,
                ..
            } => ("red", Some(category.as_str()), Some(*next_attempt)),
            CiOutcome::CapHit { .. } => ("cap_hit", None, None),
            CiOutcome::Pending => ("pending", None, None),
            CiOutcome::NoMappedPr => ("no_mapped_pr", None, None),
        };
        ci_span.record(spans::ATTR_OUTCOME, outcome_label);
        if let Some(c) = category {
            ci_span.record(spans::ATTR_FAILURE_CATEGORY, c);
        }
        if let Some(a) = attempt {
            ci_span.record(spans::ATTR_ATTEMPT_COUNT, a as i64);
        }
    }

    let any_action = outcomes
        .iter()
        .any(|o| !matches!(o, CiOutcome::Pending | CiOutcome::NoMappedPr));
    let status = if any_action {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    let body = json!({
        "status": if any_action { "queued" } else { "no-op" },
        "event": event,
        "action": action,
        "delivery_id": delivery_id,
        "outcomes": outcomes.iter().map(outcome_to_json).collect::<Vec<_>>(),
    });
    info!(target: "webhook", event, %delivery_id, ?outcomes, "ci event dispatched");
    (status, Json(body)).into_response()
}

fn outcome_to_json(o: &CiOutcome) -> Value {
    match o {
        CiOutcome::NoMappedPr => json!({"kind": "no_mapped_pr"}),
        CiOutcome::Pending => json!({"kind": "pending"}),
        CiOutcome::Green => json!({"kind": "green"}),
        CiOutcome::Red {
            category,
            next_attempt,
            max_attempts,
            target_state,
        } => json!({
            "kind": "red",
            "category": category,
            "next_attempt": next_attempt,
            "max_attempts": max_attempts,
            "target_state": target_state,
        }),
        CiOutcome::CapHit { stayed_at, max } => json!({
            "kind": "cap_hit",
            "stayed_at": stayed_at,
            "max": max,
        }),
    }
}

// ---------------------------------------------------------------------------
// Tests — end-to-end through the axum service. wiremock-driven coverage
// of the nine §9.2 scenarios is P1-H's responsibility; the unit tests
// below verify the handler's own dispatch logic against the §9.1 table.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{parse_bridge_str, LabelAliases};
    use crate::github::{CheckRunSummary, GhOps};
    use crate::labels::LabelManager;
    use crate::storage::Store;
    use crate::webhook::router;
    use crate::Result;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::Request;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use sinfonia_tracker::{LinearTracker, TrackerConfig, TrackerKind};
    use std::sync::Arc;
    use tower::ServiceExt;

    type HmacSha256 = Hmac<Sha256>;

    /// No-op GhOps for the P1-E handler tests, which don't exercise CI
    /// evaluation. Every method returns success without doing anything.
    /// (P1-H's wiremock harness exercises the real client.)
    struct NoopGh;

    #[async_trait]
    impl GhOps for NoopGh {
        async fn ensure_label(
            &self,
            _repo: &str,
            _name: &str,
            _color: &str,
            _description: &str,
        ) -> Result<()> {
            Ok(())
        }
        async fn apply_label_to_pr(
            &self,
            _repo: &str,
            _pr_number: u64,
            _name: &str,
        ) -> Result<()> {
            Ok(())
        }
        async fn remove_label_from_pr(
            &self,
            _repo: &str,
            _pr_number: u64,
            _name: &str,
        ) -> Result<()> {
            Ok(())
        }
        async fn post_pr_comment(
            &self,
            _repo: &str,
            _pr_number: u64,
            _body: &str,
        ) -> Result<()> {
            Ok(())
        }
        async fn list_check_run_summary(
            &self,
            _repo: &str,
            _head_sha: &str,
        ) -> Result<CheckRunSummary> {
            Ok(CheckRunSummary::default())
        }
        async fn whoami(&self) -> Result<String> {
            Ok("noop-fake".into())
        }
        async fn list_run_artifacts(
            &self,
            _repo: &str,
            _run_id: u64,
        ) -> Result<Vec<crate::github::ArtifactMeta>> {
            Ok(vec![])
        }
        async fn download_artifact(
            &self,
            _repo: &str,
            _artifact_id: u64,
            _max_bytes: u64,
        ) -> Result<Vec<u8>> {
            Ok(vec![])
        }
    }

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
        let gh: Arc<dyn GhOps> = Arc::new(NoopGh);
        let labels = LabelManager::new(gh.clone(), false, "sinfonia", LabelAliases::default());
        AppState::with_default_budget(cfg, store, tracker, gh, labels)
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
    async fn check_suite_completed_with_no_mapped_pr_returns_200_no_op() {
        // P1-F contract: 202 ACCEPTED only when the bridge took an
        // action. The event payload here has no `check_suite.pull_requests`
        // array (and no PR ↔ ticket mapping exists for any number anyway),
        // so every per-PR outcome is `NoMappedPr` → 200 OK no-op.
        let state = make_state().await;
        let body = serde_json::to_vec(&json!({"action": "completed"})).unwrap();
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "check_suite", "deliv-cs-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 8192)
            .await
            .expect("body");
        let v: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], json!("no-op"));
    }

    #[tokio::test]
    async fn workflow_run_completed_with_no_mapped_pr_returns_200_no_op() {
        // See `check_suite_completed_with_no_mapped_pr_returns_200_no_op`.
        let state = make_state().await;
        let body = serde_json::to_vec(&json!({"action": "completed"})).unwrap();
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "workflow_run", "deliv-wf-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 8192)
            .await
            .expect("body");
        let v: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["status"], json!("no-op"));
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

    // -- End-to-end dispatch tests (P1-F wiring) ----------------------------

    /// Scriptable [`GhOps`] for the handler-level wiring tests. Lets each
    /// test seed a [`CheckRunSummary`] and inspect which labels were
    /// applied / removed / posted as comments.
    use std::sync::Mutex as StdMutex;

    struct ScriptedGh {
        summary: CheckRunSummary,
        applied: StdMutex<Vec<(u64, String)>>,
        removed: StdMutex<Vec<(u64, String)>>,
        comments: StdMutex<Vec<(u64, String)>>,
    }

    impl ScriptedGh {
        fn new(summary: CheckRunSummary) -> Self {
            Self {
                summary,
                applied: StdMutex::new(Vec::new()),
                removed: StdMutex::new(Vec::new()),
                comments: StdMutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl GhOps for ScriptedGh {
        async fn ensure_label(
            &self,
            _repo: &str,
            _name: &str,
            _color: &str,
            _description: &str,
        ) -> Result<()> {
            Ok(())
        }
        async fn apply_label_to_pr(
            &self,
            _repo: &str,
            pr_number: u64,
            name: &str,
        ) -> Result<()> {
            self.applied
                .lock()
                .unwrap()
                .push((pr_number, name.to_string()));
            Ok(())
        }
        async fn remove_label_from_pr(
            &self,
            _repo: &str,
            pr_number: u64,
            name: &str,
        ) -> Result<()> {
            self.removed
                .lock()
                .unwrap()
                .push((pr_number, name.to_string()));
            Ok(())
        }
        async fn post_pr_comment(
            &self,
            _repo: &str,
            pr_number: u64,
            body: &str,
        ) -> Result<()> {
            self.comments
                .lock()
                .unwrap()
                .push((pr_number, body.to_string()));
            Ok(())
        }
        async fn list_check_run_summary(
            &self,
            _repo: &str,
            _head_sha: &str,
        ) -> Result<CheckRunSummary> {
            Ok(self.summary.clone())
        }
        async fn whoami(&self) -> Result<String> {
            Ok("scripted-fake".into())
        }
        async fn list_run_artifacts(
            &self,
            _repo: &str,
            _run_id: u64,
        ) -> Result<Vec<crate::github::ArtifactMeta>> {
            Ok(vec![])
        }
        async fn download_artifact(
            &self,
            _repo: &str,
            _artifact_id: u64,
            _max_bytes: u64,
        ) -> Result<Vec<u8>> {
            Ok(vec![])
        }
    }

    async fn state_with_gh(gh: Arc<ScriptedGh>, manage_labels: bool) -> AppState {
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
        let gh_dyn: Arc<dyn GhOps> = gh;
        let labels = LabelManager::new(
            gh_dyn.clone(),
            manage_labels,
            "sinfonia",
            LabelAliases::default(),
        );
        AppState::with_default_budget(cfg, store, tracker, gh_dyn, labels)
    }

    fn check_suite_payload(repo: &str, head_sha: &str, pr_number: u64) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "action": "completed",
            "check_suite": {
                "head_sha": head_sha,
                "pull_requests": [{"number": pr_number}],
            },
            "repository": {"full_name": repo},
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn check_suite_green_applies_awaiting_review_no_transition() {
        // Two passing runs, no pending, no failures → green path.
        let summary = CheckRunSummary {
            failed: vec![],
            passed: vec!["unit".into(), "lint".into()],
            any_pending: false,
        };
        let gh = Arc::new(ScriptedGh::new(summary));
        let state = state_with_gh(gh.clone(), /* manage_labels = */ true).await;

        // Seed the mapping: PR 42 → ticket ENG-42.
        state
            .store
            .upsert_pr_ticket("acme/widgets", 42, "ENG-42")
            .await
            .expect("seed");

        let body = check_suite_payload("acme/widgets", "head-sha-1", 42);
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "check_suite", "deliv-green-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED, "green should be 202");

        let applied = gh.applied.lock().unwrap().clone();
        let removed = gh.removed.lock().unwrap().clone();
        // The awaiting-review label was applied to PR 42.
        assert!(
            applied
                .iter()
                .any(|(pr, name)| *pr == 42 && name == "sinfonia:awaiting-review"),
            "expected awaiting-review label applied to PR 42; got {applied:?}",
        );
        // in-progress and needs-fixes were removed.
        assert!(removed.iter().any(|(_, n)| n == "sinfonia:in-progress"));
        assert!(removed.iter().any(|(_, n)| n == "sinfonia:needs-fixes"));
        // No PR comment was posted on green.
        assert!(gh.comments.lock().unwrap().is_empty(), "no comment on green");
    }

    #[tokio::test]
    async fn check_suite_pending_returns_no_op() {
        let summary = CheckRunSummary {
            failed: vec![],
            passed: vec![],
            any_pending: true,
        };
        let gh = Arc::new(ScriptedGh::new(summary));
        let state = state_with_gh(gh.clone(), true).await;
        state
            .store
            .upsert_pr_ticket("acme/widgets", 42, "ENG-42")
            .await
            .expect("seed");

        let body = check_suite_payload("acme/widgets", "head-sha-2", 42);
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "check_suite", "deliv-pending-1", body, Some(sig)).await;
        // Pending → 200 OK no-op.
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(gh.applied.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn green_short_circuits_when_manage_labels_false() {
        let summary = CheckRunSummary {
            failed: vec![],
            passed: vec!["unit".into()],
            any_pending: false,
        };
        let gh = Arc::new(ScriptedGh::new(summary));
        let state = state_with_gh(gh.clone(), /* manage_labels = */ false).await;
        state
            .store
            .upsert_pr_ticket("acme/widgets", 1, "ENG-1")
            .await
            .expect("seed");

        let body = check_suite_payload("acme/widgets", "head-sha-3", 1);
        let sig = sign("shh", &body);
        let resp = post_webhook(state, "check_suite", "deliv-nlbl-1", body, Some(sig)).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        // No label operations should have hit the github client.
        assert!(gh.applied.lock().unwrap().is_empty());
        assert!(gh.removed.lock().unwrap().is_empty());
    }
}
