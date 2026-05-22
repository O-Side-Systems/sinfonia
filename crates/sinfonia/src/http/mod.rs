//! Optional HTTP server extension (spec §13.7).

pub mod events;

pub use events::{
    spawn_emitter, EmitterConfig, EventsHttpState, RecentBuffer, Subscriber,
    SubscriberRegistry, EVENT_TYPE_SESSION_COMPLETED, EVENT_VERSION, SIGNATURE_HEADER,
};

use crate::orchestrator::Orchestrator;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{extract::Path, extract::State, Json, Router};
use chrono::Utc;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

#[derive(Clone)]
pub struct HttpState {
    pub orch: Orchestrator,
}

pub async fn serve(
    orch: Orchestrator,
    bind: SocketAddr,
    events_state: Arc<EventsHttpState>,
) -> std::io::Result<()> {
    let state = Arc::new(HttpState { orch });
    let app = Router::new()
        .route("/", get(index))
        .route("/api/v1/state", get(get_state))
        .route("/api/v1/refresh", post(post_refresh))
        .route("/api/v1/:identifier", get(get_issue))
        .with_state(state)
        // Events sub-router uses its own state. Merging keeps both
        // surfaces on the same listener under `/api/v1/events/*`.
        .merge(
            Router::new()
                .route(
                    "/api/v1/events/subscribers",
                    post(events::register_subscriber),
                )
                .route("/api/v1/events/recent", get(events::recent_events))
                .with_state(events_state),
        )
        .fallback(method_or_path_not_allowed);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    let actual = listener.local_addr()?;
    info!(target: "http", %actual, "listening");
    axum::serve(listener, app)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(())
}

async fn index(State(state): State<Arc<HttpState>>) -> Response {
    let snap = state.orch.snapshot().await;
    let html = render_html(&snap);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/html; charset=utf-8".parse().unwrap(),
    );
    (StatusCode::OK, headers, html).into_response()
}

async fn get_state(State(state): State<Arc<HttpState>>) -> Response {
    let snap = state.orch.snapshot().await;
    (StatusCode::OK, Json(snap)).into_response()
}

async fn post_refresh(State(state): State<Arc<HttpState>>) -> Response {
    state.orch.request_refresh();
    let body = json!({
        "queued": true,
        "coalesced": false,
        "requested_at": Utc::now(),
        "operations": ["poll", "reconcile"]
    });
    (StatusCode::ACCEPTED, Json(body)).into_response()
}

async fn get_issue(
    State(state): State<Arc<HttpState>>,
    Path(identifier): Path<String>,
) -> Response {
    match state.orch.issue_view(&identifier).await {
        Some(v) => (StatusCode::OK, Json(v)).into_response(),
        None => error_response(
            StatusCode::NOT_FOUND,
            "issue_not_found",
            &format!("no live state for issue {identifier}"),
        ),
    }
}

async fn method_or_path_not_allowed() -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "not_found",
        "no route matches this request",
    )
}

fn error_response(code: StatusCode, kind: &str, msg: &str) -> Response {
    let body: Value = json!({ "error": { "code": kind, "message": msg } });
    (code, Json(body)).into_response()
}

fn render_html(snap: &crate::orchestrator::SnapshotView) -> String {
    let mut s = String::new();
    s.push_str("<!doctype html><meta charset=utf-8><title>Sinfonia</title>");
    s.push_str("<style>body{font-family:system-ui,sans-serif;margin:24px;max-width:1100px}table{border-collapse:collapse;width:100%;margin:12px 0}th,td{border:1px solid #ddd;padding:6px 10px;text-align:left;font-size:13px}th{background:#f4f4f4}h1{margin-top:0}.muted{color:#888}</style>");
    s.push_str(&format!(
        "<h1>Sinfonia</h1><p class=muted>generated_at: {}</p>",
        snap.generated_at
    ));
    s.push_str(&format!(
        "<p>running: <b>{}</b> · retrying: <b>{}</b> · seconds_running: <b>{:.1}</b></p>",
        snap.counts.running, snap.counts.retrying, snap.codex_totals.seconds_running
    ));
    s.push_str(&format!(
        "<p>tokens: in {} · out {} · total {}</p>",
        snap.codex_totals.input_tokens,
        snap.codex_totals.output_tokens,
        snap.codex_totals.total_tokens
    ));

    s.push_str("<h2>Running</h2><table><tr><th>identifier</th><th>state</th><th>turn</th><th>last_event</th><th>last_message</th><th>tokens</th></tr>");
    for r in &snap.running {
        s.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}+{}={}</td></tr>",
            escape(&r.issue_identifier),
            escape(&r.state),
            r.turn_count,
            escape(r.last_event.as_deref().unwrap_or("")),
            escape(r.last_message.as_deref().unwrap_or("")),
            r.tokens.input_tokens,
            r.tokens.output_tokens,
            r.tokens.total_tokens,
        ));
    }
    s.push_str("</table>");

    s.push_str("<h2>Retrying</h2><table><tr><th>identifier</th><th>attempt</th><th>due_at</th><th>error</th></tr>");
    for r in &snap.retrying {
        s.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&r.issue_identifier),
            r.attempt,
            r.due_at,
            escape(r.error.as_deref().unwrap_or("")),
        ));
    }
    s.push_str("</table>");
    s
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
