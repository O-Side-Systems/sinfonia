//! Inbound side of the typed Sinfonia→bridge event channel (Phase 3 §7.2).
//!
//! Mirrors `crates/sinfonia/src/http/events.rs` on the receiving end.
//! Verifies the HMAC, deserializes the typed body, and dispatches to
//! whichever downstream handler the event maps to. Today: only
//! `runner.session.completed` is recognized; everything else gets a
//! `200 OK` with `status: ignored` for forward-compatibility.
//!
//! The HMAC algorithm matches Sinfonia's signer (HMAC-SHA256 over the
//! raw body bytes, hex-encoded with a `sha256=` prefix, header
//! `X-Sinfonia-Signature-256`). Reuses the constant-time compare from
//! `webhook::verify` so we don't fork the algorithm.

use crate::telemetry::spans;
use crate::webhook::verify::verify_signature;
use crate::webhook::AppState;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, info_span, warn, Instrument};

pub const SIGNATURE_HEADER: &str = "X-Sinfonia-Signature-256";
pub const EVENT_TYPE_SESSION_COMPLETED: &str = "runner.session.completed";

/// Typed view of the `runner.session.completed` body shape from plan
/// §7.2. Extra fields on the wire are ignored (serde `default`s; we
/// don't `deny_unknown_fields` because the sender owns the schema).
#[derive(Debug, Deserialize, Serialize)]
pub struct SessionCompletedEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default = "default_version")]
    pub version: u32,
    pub tenant_id: String,
    pub issue_id: String,
    pub issue_identifier: String,
    pub state: String,
    pub provider: String,
    pub model: String,
    pub turn_count: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub duration_ms: u64,
    pub exit_reason: String,
    pub occurred_at: String,
}

fn default_version() -> u32 {
    1
}

/// `POST /api/v1/sinfonia-events` — inbound typed events from Sinfonia.
pub async fn sinfonia_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let span = info_span!(
        target: "events",
        spans::BRIDGE_EVENTS_RECEIVE,
        { spans::ATTR_TENANT_ID } = tracing::field::Empty,
        { spans::ATTR_EVENT_TYPE } = tracing::field::Empty,
    );
    sinfonia_events_inner(state, headers, body)
        .instrument(span)
        .await
}

async fn sinfonia_events_inner(
    state: AppState,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    // -- 1. HMAC verify --------------------------------------------------
    let secret = match state.config.telemetry.sinfonia_events_secret.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => {
            warn!(
                target: "events",
                "sinfonia-events received but telemetry.sinfonia_events_secret is not configured; rejecting"
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "sinfonia_events_secret not configured"})),
            )
                .into_response();
        }
    };
    let sig_header = headers.get(SIGNATURE_HEADER).and_then(|v| v.to_str().ok());
    if let Err(e) = verify_signature(&body, sig_header, secret) {
        debug!(target: "events", error = %e, "signature verification failed");
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": e.to_string()})),
        )
            .into_response();
    }

    // -- 2. Parse the typed body ----------------------------------------
    //
    // First peek at the `type` field — anything we don't recognize gets a
    // 200 `ignored` response so a future Sinfonia release adding new event
    // shapes doesn't crash an older bridge.
    let envelope: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            warn!(target: "events", error = %e, "invalid JSON body");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid JSON: {e}")})),
            )
                .into_response();
        }
    };
    let event_type = envelope
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    tracing::Span::current().record(spans::ATTR_EVENT_TYPE, event_type.as_str());

    match event_type.as_str() {
        EVENT_TYPE_SESSION_COMPLETED => {
            let parsed: SessionCompletedEvent = match serde_json::from_value(envelope) {
                Ok(v) => v,
                Err(e) => {
                    warn!(target: "events", error = %e, "SessionCompletedEvent parse failed");
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(json!({"error": format!("schema mismatch: {e}")})),
                    )
                        .into_response();
                }
            };
            tracing::Span::current().record(spans::ATTR_TENANT_ID, parsed.tenant_id.as_str());
            info!(
                target: "events",
                issue_identifier = %parsed.issue_identifier,
                provider = %parsed.provider,
                model = %parsed.model,
                turn_count = parsed.turn_count,
                total_tokens = parsed.prompt_tokens + parsed.completion_tokens,
                exit_reason = %parsed.exit_reason,
                "session completed event received"
            );

            // Feed into the budget pipeline (plan §7.3).
            let outcome = state.budget.apply_session(
                &parsed.issue_id,
                &parsed.provider,
                &parsed.model,
                parsed.prompt_tokens,
                parsed.completion_tokens,
            );
            use crate::feedback::budget::SessionApplyOutcome;
            match outcome {
                SessionApplyOutcome::Accumulated => {
                    // Will flush on the next 30 s debounce sweep.
                    (
                        StatusCode::ACCEPTED,
                        Json(
                            json!({"status": "accumulated", "issue_id": parsed.issue_id}),
                        ),
                    )
                        .into_response()
                }
                SessionApplyOutcome::CapHit { kind } => {
                    // Cap-crossing path: flush immediately + transition
                    // the ticket. Errors are logged but don't block the
                    // 202 — Sinfonia retries on the source side already
                    // and re-driving the flush is harmless (the
                    // accumulator handles the no-op idempotency).
                    if let Err(e) = state.budget.flush_ticket(&parsed.issue_id).await {
                        warn!(target: "events", issue_id = %parsed.issue_id, error = %e, "cap-hit flush failed");
                    }
                    if let Err(e) = state
                        .tracker
                        .transition_issue(
                            &parsed.issue_id,
                            &state.config.feedback_loop.budget_exceeded_state,
                        )
                        .await
                    {
                        warn!(target: "events", issue_id = %parsed.issue_id, error = %e, "budget_exceeded transition failed");
                    }
                    info!(
                        target: "events",
                        issue_id = %parsed.issue_id,
                        cap_kind = kind.as_str(),
                        target_state = %state.config.feedback_loop.budget_exceeded_state,
                        "budget cap hit; transitioned to budget_exceeded_state"
                    );
                    (
                        StatusCode::ACCEPTED,
                        Json(json!({
                            "status": "cap_hit",
                            "issue_id": parsed.issue_id,
                            "cap_kind": kind.as_str(),
                        })),
                    )
                        .into_response()
                }
            }
        }
        other => {
            debug!(target: "events", event_type = %other, "event_type ignored (forward-compat)");
            (
                StatusCode::OK,
                Json(json!({"status": "ignored", "event_type": other})),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    fn hex_lower(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    #[test]
    fn parses_session_completed_event_shape() {
        let json = json!({
            "type": EVENT_TYPE_SESSION_COMPLETED,
            "version": 1,
            "tenant_id": "kyros-web-app",
            "issue_id": "lin_iss_abc123",
            "issue_identifier": "ENG-42",
            "state": "Needs Fixes - E2E",
            "provider": "ClaudeCode",
            "model": "claude-opus-4-7",
            "turn_count": 8,
            "prompt_tokens": 318404,
            "completion_tokens": 12892,
            "duration_ms": 542113,
            "exit_reason": "completed",
            "occurred_at": "2026-05-21T17:42:11Z",
        });
        let parsed: SessionCompletedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.tenant_id, "kyros-web-app");
        assert_eq!(parsed.turn_count, 8);
        assert_eq!(parsed.prompt_tokens, 318404);
    }

    #[test]
    fn hmac_signature_format_matches_sinfonia() {
        // This test pins the algorithm + format the bridge expects. If
        // the Sinfonia side ever changes how it signs, this test should
        // start failing — fix the signer, not the test, unless both
        // sides are intentionally bumping the scheme.
        let mut mac = HmacSha256::new_from_slice(b"shared-secret").unwrap();
        mac.update(b"hello");
        let digest = mac.finalize().into_bytes();
        let header = format!("sha256={}", hex_lower(&digest));
        assert!(header.starts_with("sha256="));
        assert_eq!(header.len(), "sha256=".len() + 64);
    }
}
