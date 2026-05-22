//! Typed Sinfonia→bridge event channel (Phase 3 §7.2).
//!
//! The bridge subscribes to a small typed JSON event stream Sinfonia
//! emits over its existing HTTP surface — this module owns the
//! Sinfonia side of that channel. Three concerns live here:
//!
//! 1. **Registry** — a process-wide list of subscribers, each with a
//!    callback URL and the timestamp at which it was registered.
//! 2. **Emitter task** — drains the AgentEvent fan-out channel,
//!    filters for `SessionCompleted`, serializes to the typed POST
//!    body shape from plan §7.2, HMAC-signs the body, and dispatches
//!    to every registered subscriber with retry-with-backoff.
//! 3. **HTTP endpoints** — `POST /api/v1/events/subscribers` (register
//!    a callback) and `GET /api/v1/events/recent` (diagnostic ring of
//!    recent events for debugging).
//!
//! The HMAC algorithm matches `crates/sinfonia-bridge/src/webhook/verify.rs`
//! (HMAC-SHA256 over the raw body bytes, hex-encoded, header
//! `X-Sinfonia-Signature-256`). The shared secret comes from
//! `telemetry.sinfonia_events_secret` in WORKFLOW.md and must equal the
//! same field in BRIDGE.md. Mismatch → bridge returns 401 + Sinfonia
//! logs `WARN` on retry exhaustion.

use crate::agent::events::AgentEvent;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

type HmacSha256 = Hmac<Sha256>;

/// Header name carrying the HMAC-SHA256 hex digest of the body. Mirrors
/// the GitHub-webhook convention so the bridge's verify helper can be
/// reused for both inbound paths.
pub const SIGNATURE_HEADER: &str = "X-Sinfonia-Signature-256";

/// Wire format `type` field for the session-completed event.
pub const EVENT_TYPE_SESSION_COMPLETED: &str = "runner.session.completed";

/// Schema version on the wire — see §7.2 example payload. Bumped only
/// when the field shape changes incompatibly.
pub const EVENT_VERSION: u32 = 1;

/// Maximum number of recent events kept in the ring buffer surfaced by
/// `GET /api/v1/events/recent`. Diagnostic only; not durable.
pub const RECENT_BUFFER_CAPACITY: usize = 200;

/// Retry budget for an outbound POST to a subscriber. Bridge restart
/// windows are typically seconds; this covers a brief outage without
/// flooding logs with every failed delivery.
pub const MAX_DELIVERY_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscriber {
    pub id: String,
    pub callback_url: String,
    pub registered_at: DateTime<Utc>,
}

/// Process-wide subscriber registry. Cheap to clone (Arc); shared
/// between the HTTP register endpoint and the emitter task.
#[derive(Clone, Default)]
pub struct SubscriberRegistry {
    inner: Arc<Mutex<Vec<Subscriber>>>,
}

impl SubscriberRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn list(&self) -> Vec<Subscriber> {
        self.inner.lock().clone()
    }
    pub fn add(&self, s: Subscriber) {
        let mut g = self.inner.lock();
        // Idempotent on callback URL — a re-register replaces in place.
        if let Some(existing) = g.iter_mut().find(|x| x.callback_url == s.callback_url) {
            *existing = s;
        } else {
            g.push(s);
        }
    }
    pub fn remove_by_callback(&self, url: &str) -> bool {
        let mut g = self.inner.lock();
        let before = g.len();
        g.retain(|s| s.callback_url != url);
        g.len() != before
    }
}

/// Ring buffer of recently-emitted events, surfaced via `GET
/// /api/v1/events/recent` for operator debugging. Behind an `Arc<Mutex>`
/// so both the emitter task and the HTTP handler can read it.
#[derive(Clone, Default)]
pub struct RecentBuffer {
    inner: Arc<Mutex<VecDeque<Value>>>,
}

impl RecentBuffer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&self, ev: Value) {
        let mut g = self.inner.lock();
        if g.len() >= RECENT_BUFFER_CAPACITY {
            g.pop_front();
        }
        g.push_back(ev);
    }
    pub fn list(&self) -> Vec<Value> {
        self.inner.lock().iter().cloned().collect()
    }
}

/// Inputs the emitter task needs that aren't already on the registry
/// or buffer. The HMAC secret + tenant id are process-wide.
#[derive(Clone)]
pub struct EmitterConfig {
    pub tenant_id: String,
    pub secret: Option<String>,
}

/// Spawn the subscriber-emitter task. Reads (issue_id, AgentEvent) from
/// `rx`, filters for SessionCompleted, dispatches HMAC-signed POSTs to
/// every registered subscriber.
pub fn spawn_emitter(
    mut rx: mpsc::UnboundedReceiver<(String, AgentEvent)>,
    registry: SubscriberRegistry,
    recent: RecentBuffer,
    cfg: EmitterConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Re-use one HTTP client; reqwest pools connections internally.
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                warn!(target: "events", error = %e, "failed to build reqwest client; emitter disabled");
                return;
            }
        };

        while let Some((_issue_id, ev)) = rx.recv().await {
            let AgentEvent::SessionCompleted {
                timestamp,
                thread_id: _,
                issue_id: ev_issue_id,
                issue_identifier,
                state,
                provider,
                model,
                turn_count,
                prompt_tokens,
                completion_tokens,
                total_tokens: _,
                duration_ms,
                exit_reason,
            } = ev
            else {
                continue;
            };

            let body = json!({
                "type": EVENT_TYPE_SESSION_COMPLETED,
                "version": EVENT_VERSION,
                "tenant_id": cfg.tenant_id,
                "issue_id": ev_issue_id,
                "issue_identifier": issue_identifier,
                "state": state,
                "provider": provider,
                "model": model,
                "turn_count": turn_count,
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "duration_ms": duration_ms,
                "exit_reason": exit_reason,
                "occurred_at": timestamp.to_rfc3339(),
            });
            let payload_bytes = match serde_json::to_vec(&body) {
                Ok(b) => b,
                Err(e) => {
                    warn!(target: "events", error = %e, "serialize event failed");
                    continue;
                }
            };

            recent.push(body.clone());

            let signature = cfg
                .secret
                .as_deref()
                .map(|s| sign(&payload_bytes, s.as_bytes()));

            for sub in registry.list() {
                let url = sub.callback_url.clone();
                let bytes = payload_bytes.clone();
                let sig = signature.clone();
                let client = client.clone();
                tokio::spawn(async move {
                    deliver_with_retry(&client, &url, &bytes, sig.as_deref()).await;
                });
            }
        }
    })
}

fn sign(body: &[u8], secret: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(body);
    let digest = mac.finalize().into_bytes();
    let mut out = String::with_capacity(7 + digest.len() * 2);
    out.push_str("sha256=");
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

async fn deliver_with_retry(
    client: &reqwest::Client,
    url: &str,
    body: &[u8],
    signature: Option<&str>,
) {
    let mut backoff_ms: u64 = 250;
    for attempt in 1..=MAX_DELIVERY_ATTEMPTS {
        let mut req = client
            .post(url)
            .header("Content-Type", "application/json")
            .body(body.to_vec());
        if let Some(sig) = signature {
            req = req.header(SIGNATURE_HEADER, sig);
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                debug!(target: "events", %url, status = %resp.status(), "delivered");
                return;
            }
            Ok(resp) => {
                warn!(
                    target: "events",
                    %url, status = %resp.status(), attempt,
                    "subscriber returned non-success"
                );
            }
            Err(e) => {
                warn!(target: "events", %url, error = %e, attempt, "subscriber POST failed");
            }
        }
        if attempt < MAX_DELIVERY_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            backoff_ms = backoff_ms.saturating_mul(2);
        }
    }
    warn!(
        target: "events",
        %url, max = MAX_DELIVERY_ATTEMPTS,
        "retry budget exhausted; dropping event for this subscriber"
    );
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// State threaded through the events endpoints. Cheap to clone.
#[derive(Clone)]
pub struct EventsHttpState {
    pub registry: SubscriberRegistry,
    pub recent: RecentBuffer,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub callback_url: String,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub id: String,
    pub callback_url: String,
    pub registered_at: DateTime<Utc>,
}

pub async fn register_subscriber(
    State(state): State<Arc<EventsHttpState>>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    if req.callback_url.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "callback_url is required"})),
        )
            .into_response();
    }
    // Reject non-HTTP(S) callbacks so the emitter doesn't accept a
    // file:// or javascript: URL by mistake.
    if !(req.callback_url.starts_with("http://") || req.callback_url.starts_with("https://"))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "callback_url must be http(s)://"})),
        )
            .into_response();
    }
    let sub = Subscriber {
        id: uuid::Uuid::new_v4().to_string(),
        callback_url: req.callback_url,
        registered_at: Utc::now(),
    };
    state.registry.add(sub.clone());
    info!(target: "events", callback_url = %sub.callback_url, "subscriber registered");
    (
        StatusCode::CREATED,
        Json(RegisterResponse {
            id: sub.id,
            callback_url: sub.callback_url,
            registered_at: sub.registered_at,
        }),
    )
        .into_response()
}

pub async fn recent_events(
    State(state): State<Arc<EventsHttpState>>,
) -> impl IntoResponse {
    let events = state.recent.list();
    Json(json!({"events": events}))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_replaces_on_duplicate_url() {
        let r = SubscriberRegistry::new();
        r.add(Subscriber {
            id: "a".into(),
            callback_url: "http://x".into(),
            registered_at: Utc::now(),
        });
        r.add(Subscriber {
            id: "b".into(),
            callback_url: "http://x".into(),
            registered_at: Utc::now(),
        });
        let list = r.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "b");
    }

    #[test]
    fn recent_buffer_caps_at_capacity() {
        let r = RecentBuffer::new();
        for i in 0..(RECENT_BUFFER_CAPACITY + 50) {
            r.push(json!({"i": i}));
        }
        assert_eq!(r.list().len(), RECENT_BUFFER_CAPACITY);
    }

    #[test]
    fn sign_produces_sha256_prefixed_hex() {
        let s = sign(b"hello", b"secret");
        assert!(s.starts_with("sha256="));
        assert_eq!(s.len(), "sha256=".len() + 64); // hex-encoded SHA-256 is 64 chars
    }
}
