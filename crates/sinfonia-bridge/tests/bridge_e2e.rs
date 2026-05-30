//! Bridge end-to-end integration tests (plan §9.2, "Integration tests").
//!
//! Each `#[tokio::test]` here boots the full bridge daemon on a random
//! local port, points its GitHub client at a [`wiremock`] mock and its
//! Linear tracker at a second mock, and drives the system by POSTing
//! HMAC-signed webhook payloads to the bridge — exactly the way GitHub
//! would.
//!
//! ## Why two mocks
//!
//! - **GitHub.** `Octocrab::builder().base_uri(mock_uri)?.build()?` retargets
//!   every REST call (PAT *and* App-mode installation flows) at the
//!   mock. The bridge's production [`OctocrabGhOps::from_octocrab`] and
//!   [`AppModeGhOps::new`] constructors accept a pre-built `Octocrab`,
//!   so the integration tests construct one with the mock URI and wrap
//!   it without changing the production factory in `github::auth`.
//! - **Linear.** `tracker.endpoint` is already config-driven, so a
//!   `LinearTracker::new(&cfg)` whose `cfg.endpoint` points at the mock
//!   exercises the real GraphQL request/response code path through to
//!   the marker-comment storage.
//!
//! ## Scenario coverage (plan §9.2)
//!
//! 1. Green PR → no transition, `awaiting-review` label applied.
//! 2. One red CI run → counter 0→1, transition to `Needs Fixes`, comment posted.
//! 3. Three red runs with category routing → counter advances and category-specific labels.
//! 4. Cap hit → transition to `blocked_state`, counter not advanced.
//! 5. Webhook redelivery → second delivery is a 200 no-op.
//! 6. Webhook signature failure → 401, no state change.
//! 7. PR without ticket link → 200 ignored, no mapping written.
//! 8. App-mode auth → same as (2) with App credentials.
//! 9. `manage_labels: false` → transitions still fire, no label calls.
//!
//! ## Shape conventions
//!
//! All wiremock responses are minimal JSON satisfying octocrab's
//! deserialisers — we generally only inspect that an endpoint was *hit*,
//! not the response payload. The mocks record requests via
//! [`MockServer::received_requests`] so each test can assert on the set
//! of calls the bridge actually made.

use axum::serve;
use hmac::{Hmac, Mac};
use jsonwebtoken::EncodingKey;
use octocrab::models::AppId;
use octocrab::Octocrab;
use serde_json::{json, Value};
use sha2::Sha256;
use sinfonia_bridge::config::parse_bridge_str;
use sinfonia_bridge::github::{AppModeGhOps, GhOps, OctocrabGhOps};
use sinfonia_bridge::labels::LabelManager;
use sinfonia_bridge::storage::Store;
use sinfonia_bridge::webhook::{router, AppState};
use sinfonia_tracker::{custom_fields, LinearTracker, TrackerKind};
use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// HMAC secret used for every signed webhook in this test file. The
/// matching value lives in `bridge_md(...)` under `github.webhook_secret`.
const WEBHOOK_SECRET: &str = "shh-this-is-only-a-test";

/// Test RSA key (PKCS#8, 2048-bit) used to construct an App-mode
/// [`Octocrab`] for scenario 8. Generated once via `openssl genrsa` and
/// committed to the test file deliberately: the JWT it produces is only
/// ever sent to the in-process [`wiremock`] mock, which doesn't verify
/// signatures.
const TEST_APP_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDlDUq1mIrx+f6r
N1f8qaz14Dy2oLNB6ktXcE+943mBj4PBtH6M9mAk4meLsRVaxA6u3TIz4YLwuPOH
qdAMGWFh+JjF/I1LEU7lWBeEPUnulPkUySuqTzaz1ikwdhJfBEitcodqvqgjKspH
VGpwzi+J2wUm7RfDUt03CYCYKQLCevkSgurD7LYhqb7aTW1bdaxh9sddeBvOZa2x
o9/APwzLxZ37+QxEPSBst8BuhhiKWIxQYG57RxVcAMZE+QyPrKexhk85a8VFPMvB
bh63qFwkLn19/w5A+fHtf08lIZ7+uCcm63poD5VSWNrGIcqt/Ty1OW5J0D2j/EmR
8wYA2O9VAgMBAAECggEAAhTlPh7Z+tAxnYJUc5PIyJ52SGQ2SaREqJ5feya04fwl
6TEn8HjRiURHE9PskVvN+ygNEfLVhJtWfnMioUy4Jj9i7aZ/7W2FpAi+HfhYn0La
iKY5yV0/uzhFCfW1iqHru/fNSnRB67l/beqGFR33YwjxVD5vXg0QvM4cGkXoV1Ac
yO0bUBcqWL0Uev6MW1qwfHbOLhQl633RqOXvNOm3/LwoYzpWNnI911otyWUnfmF+
CWYCVrFzVTx9zUEmRN0ispWPpw4DaHLAZYYmzA5CFaYaB4pfS6VfnE4MAChGELca
fOopkvSE3g+U68hx2VuD1L8+zH9ier7dWAKqvBN4eQKBgQD9/HkQoylgRQxmFpDn
NzxFyPPFLmXHCr+KKo7ueOpB8nEytfO4dTyJOqCWLFjnprOxxG18NILeB/W/UjJ0
TlOZQxgdn/sM+QWnRMm8ucoRVEKS3HTQuu8lxrQDSrFvvmQRSProNn7QbtXlzryE
6v3ZP2FnI217bH+ymgnSYNrdrwKBgQDm3jVrPGjiti1p7OaUzBdtfqrgcy+PM9uh
EgBwJAg92GhlWSMjrRv5tndMe7Yt0osOmgcAwqUWBeQ9uwGZR/cTBZ2LQLKKid5o
n6bS2+OPy4sWdFKFNaLt0M5Cez6S41RBh1pEsJR0sXWj6vIRPKbZzkSJlNs0Jg9g
m2jLoiOoOwKBgAxEDiC0kIH6s6+WdWcFLt775nHmXLnxFfD5Py/bHQ0URU06pkuJ
NeQ2tZyrBZwiW9lA8DyoWI2aes7DjHY7diQXrml32Wr198TtOITpwA14MULgbK/L
51K+tuDV0Z3g5vpCuQNP5m3wfFn90vLkWmAMhIqzlkz4n24jrEfBr7A3AoGAO1x5
Wchfo6N6C9lo9GuBvpqqLyoO2YBZAZJSYIMzR0uklCKWQW8aWVvMUvMgRan0LV33
XP+vWPlM1X6HC7WQVujDuHF7Ntn6QOaEC4WUfn20lqJ0MWI4MXPWBQwRa81d9bdq
w2yvz01t1Sbs9PemYyyBPOr0bfU3UPzEtk9LYc8CgYEAuFG9CenFgrBhtx80nCnd
L1hPipOxDKxAT0wTjKjELJHwb4IBIQfzbtVdPIBowC9wzVHJVUgTeuhk0m7gOFuS
wVDIdvxFSGOvd/j2jSsCveloQbzJ7HdnTpF38nyrPhU8y0QkM6KjteyuwR6K9vVN
QyTN50bV2MlCc1baKjXUHnA=
-----END PRIVATE KEY-----
";

/// Build the `BRIDGE.md` source string for a given test scenario.
///
/// The integration tests vary along five axes:
/// - GitHub auth mode (PAT vs App).
/// - `feedback_loop.max_attempts`.
/// - `github.manage_labels`.
/// - Tracker endpoint (mock URI).
/// - Failure categories (none for most scenarios; scenario 3 uses two).
fn bridge_md(
    pat: Option<&str>,
    app_id: Option<u64>,
    private_key: Option<&str>,
    max_attempts: u32,
    manage_labels: bool,
    linear_endpoint: &str,
    failure_categories_yaml: Option<&str>,
) -> String {
    let auth_block = match (pat, app_id, private_key) {
        (Some(p), _, _) => format!("  pat: {p}\n"),
        (None, Some(id), Some(pk)) => {
            // Inline PEMs go through YAML's block-scalar (`|`) so the
            // multiline body is preserved verbatim. The bridge parser
            // strips no whitespace beyond leading/trailing — see
            // `load_private_key` in `github/auth.rs`.
            let indented_pem: String = pk
                .lines()
                .map(|l| format!("    {l}\n"))
                .collect::<String>();
            format!("  app_id: {id}\n  private_key: |\n{indented_pem}")
        }
        _ => panic!("bridge_md: must set either pat or (app_id + private_key)"),
    };
    let manage_labels_str = if manage_labels { "true" } else { "false" };
    let categories_block = failure_categories_yaml.unwrap_or("");

    format!(
        r#"---
tracker:
  kind: linear
  endpoint: "{linear_endpoint}"
  api_key: linear-test-key
  project_slug: my-project
github:
  webhook_secret: {WEBHOOK_SECRET}
{auth_block}  manage_labels: {manage_labels_str}
  label_prefix: sinfonia
feedback_loop:
  max_attempts: {max_attempts}
  needs_fixes_state: "Needs Fixes"
  blocked_state: "Blocked - Human Review"
{categories_block}custom_fields:
  attempt_count: sinfonia_attempt_count
  last_failure_log: sinfonia_last_ci_failure
  max_attempts_override: sinfonia_max_attempts
  failure_category: sinfonia_failure_category
  tokens_consumed: sinfonia_tokens_consumed
  cost_consumed_usd: sinfonia_cost_consumed_usd
  max_cost_override_usd: sinfonia_max_cost_usd
server:
  bind: 127.0.0.1
  port: 0
storage:
  state_db_path: /tmp/bridge-e2e-test.db
telemetry:
  service_name: sinfonia-bridge-test
---
"#
    )
}

// ---------------------------------------------------------------------------
// Linear mock — single POST handler that routes by GraphQL query keyword
// ---------------------------------------------------------------------------

/// In-memory state the Linear mock maintains across a scenario's calls.
///
/// The bridge's `write_custom_field` path is load-modify-store on a
/// bot-owned comment; the mock tracks the current marker fields per
/// ticket so successive reads see what previous writes deposited.
#[derive(Default)]
struct LinearMockState {
    /// Per-ticket field map. Keyed by ticket id (e.g. `"ENG-7"`).
    fields: HashMap<String, custom_fields::FieldsMap>,
    /// Per-ticket marker comment id. Linear's `commentUpdate` mutation
    /// requires the id; `commentCreate` returns one we hand back here so
    /// the next `load_marker_comment` finds the existing comment.
    marker_comment_ids: HashMap<String, String>,
    /// Linear workflow state names → state ids. Looked up by
    /// `transition_issue` via `resolve_state_id`.
    state_name_to_id: HashMap<String, String>,
    /// Records `(ticket_id, target_state_name)` pairs for every
    /// `issueUpdate` we processed. Lets tests assert on transition order.
    transitions: Vec<(String, String)>,
    /// Auto-incrementing id used when minting new marker comment ids.
    next_comment_id: u64,
}

impl LinearMockState {
    fn new_with_states(states: &[&str]) -> Self {
        let mut s = Self::default();
        for (idx, name) in states.iter().enumerate() {
            s.state_name_to_id
                .insert((*name).to_string(), format!("state-id-{idx}"));
        }
        s
    }
}

#[derive(Clone)]
struct LinearGraphqlMock {
    state: Arc<Mutex<LinearMockState>>,
}

impl LinearGraphqlMock {
    fn new(state: Arc<Mutex<LinearMockState>>) -> Self {
        Self { state }
    }
}

impl Respond for LinearGraphqlMock {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = match serde_json::from_slice(&req.body) {
            Ok(v) => v,
            Err(e) => {
                return ResponseTemplate::new(400).set_body_json(json!({
                    "errors": [{"message": format!("malformed json: {e}")}]
                }));
            }
        };
        let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let vars = body.get("variables").cloned().unwrap_or(Value::Null);
        let mut state = self.state.lock().unwrap();

        // Order matters — the marker-comment LOAD query and the
        // state-id RESOLVE query both select `data.issue.{...}` so we
        // dispatch by the unique nested subselection.
        if query.contains("comments(first:") || query.contains("comments(first ") {
            // load_marker_comment — return the encoded marker body, if any.
            let issue_id = vars
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let nodes: Vec<Value> =
                match (state.fields.get(&issue_id), state.marker_comment_ids.get(&issue_id)) {
                    (Some(fields), Some(cid)) if !fields.is_empty() => {
                        let env = custom_fields::MarkerEnvelope {
                            fields: fields.clone(),
                        };
                        let body = custom_fields::encode_marker(&env);
                        vec![json!({"id": cid, "body": body})]
                    }
                    _ => vec![],
                };
            return ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "issue": {
                        "id": issue_id,
                        "comments": {"nodes": nodes},
                    }
                }
            }));
        }

        if query.contains("team {") && query.contains("states(first:") {
            // resolve_state_id — return the entire {name, id} map.
            let nodes: Vec<Value> = state
                .state_name_to_id
                .iter()
                .map(|(name, id)| json!({"id": id, "name": name}))
                .collect();
            return ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "issue": {
                        "id": vars.get("id").cloned().unwrap_or(Value::Null),
                        "team": {
                            "id": "team-fixture",
                            "states": {"nodes": nodes},
                        }
                    }
                }
            }));
        }

        if query.contains("commentCreate(") {
            // Mint a new id, attach to the ticket's marker_comment slot.
            let issue_id = vars
                .get("issueId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body_text = vars
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            state.next_comment_id += 1;
            let new_id = format!("comment-fixture-{}", state.next_comment_id);
            // Try to decode the marker so we can stash the field map.
            if let Some(env) = custom_fields::decode_marker(body_text) {
                state.fields.insert(issue_id.clone(), env.fields);
                state.marker_comment_ids.insert(issue_id, new_id.clone());
            } else {
                // Non-marker comment (e.g. PR-comment-on-tracker which the
                // bridge doesn't emit, but Linear's `post_comment` path
                // shares this mutation). We still mint an id; we just
                // don't update the marker slot.
            }
            return ResponseTemplate::new(200).set_body_json(json!({
                "data": {"commentCreate": {"success": true, "comment": {"id": new_id}}}
            }));
        }

        if query.contains("commentUpdate(") {
            let cid = vars
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body_text = vars
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Find the ticket id whose marker_comment_id matches.
            let owning_ticket = state
                .marker_comment_ids
                .iter()
                .find_map(|(k, v)| if v == &cid { Some(k.clone()) } else { None });
            if let Some(ticket_id) = owning_ticket {
                if let Some(env) = custom_fields::decode_marker(body_text) {
                    state.fields.insert(ticket_id, env.fields);
                }
            }
            return ResponseTemplate::new(200)
                .set_body_json(json!({"data": {"commentUpdate": {"success": true}}}));
        }

        if query.contains("issueUpdate(") {
            let ticket = vars
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let state_id = vars
                .get("stateId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Reverse-map state id back to name for the recorded tuple.
            let state_name = state
                .state_name_to_id
                .iter()
                .find_map(|(name, id)| if id == &state_id { Some(name.clone()) } else { None })
                .unwrap_or(state_id);
            state.transitions.push((ticket.clone(), state_name.clone()));
            return ResponseTemplate::new(200).set_body_json(json!({
                "data": {
                    "issueUpdate": {
                        "success": true,
                        "issue": {"id": ticket, "state": {"name": state_name}},
                    }
                }
            }));
        }

        ResponseTemplate::new(200).set_body_json(json!({
            "data": null,
            "errors": [{"message": format!("linear mock: unhandled query (first 80 chars): {}", &query.chars().take(80).collect::<String>())}]
        }))
    }
}

// ---------------------------------------------------------------------------
// GitHub mock — one wiremock route per REST endpoint the bridge touches
// ---------------------------------------------------------------------------

/// A minimal `ListCheckRuns` JSON body for a given (failed, passed) pair.
/// The struct is `#[non_exhaustive]` so missing Option fields are fine,
/// but every non-Option field has to be present.
fn check_runs_body(head_sha: &str, failed: &[&str], passed: &[&str]) -> Value {
    let mut runs = Vec::new();
    for (idx, name) in passed.iter().enumerate() {
        runs.push(check_run_entry(idx as u64, head_sha, name, "success"));
    }
    for (idx, name) in failed.iter().enumerate() {
        runs.push(check_run_entry(
            (passed.len() + idx) as u64,
            head_sha,
            name,
            "failure",
        ));
    }
    json!({
        "total_count": runs.len(),
        "check_runs": runs,
    })
}

fn check_run_entry(id: u64, head_sha: &str, name: &str, conclusion: &str) -> Value {
    json!({
        "id": id,
        "node_id": format!("MDg6Q2hlY2tSdW57}}{id}"),
        "head_sha": head_sha,
        "url": format!("https://api.github.test/repos/_/_/check-runs/{id}"),
        "html_url": null,
        "details_url": null,
        "conclusion": conclusion,
        "output": {
            "title": null,
            "summary": null,
            "text": null,
            "annotations_count": 0,
            "annotations_url": format!("https://api.github.test/repos/_/_/check-runs/{id}/annotations"),
        },
        "started_at": null,
        "completed_at": null,
        "name": name,
        "pull_requests": [],
    })
}

/// `octocrab` models the comment response with required fields — we
/// only need enough scaffolding for it to deserialise without inspection.
fn issue_comment_stub() -> Value {
    json!({
        "id": 1,
        "node_id": "IC_kwfake",
        "url": "https://api.github.test/repos/_/_/issues/comments/1",
        "html_url": "https://api.github.test/issues/comments/1",
        "issue_url": "https://api.github.test/repos/_/_/issues/1",
        "body": "stub",
        "user": stub_user(),
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "author_association": "NONE",
        "performed_via_github_app": null,
    })
}

/// `octocrab`'s `Author` model has a lot of required URL fields; we
/// satisfy the deserialiser with synthetic values that no test inspects.
fn stub_user() -> Value {
    json!({
        "login": "stub-bot",
        "id": 1,
        "node_id": "U_kwfake",
        "avatar_url": "https://avatars.github.test/u/1",
        "gravatar_id": "",
        "url": "https://api.github.test/users/stub-bot",
        "html_url": "https://github.test/stub-bot",
        "followers_url": "https://api.github.test/users/stub-bot/followers",
        "following_url": "https://api.github.test/users/stub-bot/following{/other_user}",
        "gists_url": "https://api.github.test/users/stub-bot/gists{/gist_id}",
        "starred_url": "https://api.github.test/users/stub-bot/starred{/owner}{/repo}",
        "subscriptions_url": "https://api.github.test/users/stub-bot/subscriptions",
        "organizations_url": "https://api.github.test/users/stub-bot/orgs",
        "repos_url": "https://api.github.test/users/stub-bot/repos",
        "events_url": "https://api.github.test/users/stub-bot/events{/privacy}",
        "received_events_url": "https://api.github.test/users/stub-bot/received_events",
        "type": "Bot",
        "site_admin": false,
        "patch_url": null,
    })
}

/// Empty-array response. Used for `add_labels` / `remove_label` / etc.
/// — the bridge only inspects the `Result::Ok` discriminant.
fn empty_label_array() -> Value {
    json!([])
}

/// `octocrab::models::Label` body for `create_label` happy path.
fn label_stub(name: &str) -> Value {
    json!({
        "id": 1,
        "node_id": "L_kwfake",
        "url": format!("https://api.github.test/repos/_/_/labels/{name}"),
        "name": name,
        "color": "ffffff",
        "default": false,
        "description": null,
    })
}

/// Mount a check-runs response *for a single `head_sha`*. The bridge
/// fetches check-runs once per CI event by SHA; using `head_sha` as a
/// literal path segment keeps wiremock's match table unambiguous when a
/// scenario fires multiple sequential CI events (otherwise wiremock
/// returns the *first* matching mock, not the most-recently mounted —
/// see plan §9.2 scenario 3, which exercises three reds back-to-back).
async fn mount_github_check_runs(
    server: &MockServer,
    head_sha: &str,
    failed: &[&'static str],
    passed: &[&'static str],
) {
    let p = format!(
        r"^/repos/[^/]+/[^/]+/commits/{}/check-runs$",
        regex::escape(head_sha)
    );
    Mock::given(method("GET"))
        .and(path_regex(p))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(check_runs_body(head_sha, failed, passed)),
        )
        .mount(server)
        .await;
}

/// Mount the per-repo REST label + comment endpoints. Owner / repo
/// segments use `[^/]+` (not `.+`) so the create-label route
/// `/repos/X/Y/labels` doesn't accidentally swallow the add-labels route
/// `/repos/X/Y/issues/N/labels` — `.+` is greedy across slashes and
/// would have wiremock return the wrong response shape (octocrab then
/// fails with `JSON Error: expected sequence, got map`).
async fn mount_github_label_ops(server: &MockServer) {
    // Create label (idempotent on the bridge side).
    Mock::given(method("POST"))
        .and(path_regex(r"^/repos/[^/]+/[^/]+/labels$"))
        .respond_with(ResponseTemplate::new(201).set_body_json(label_stub("sinfonia:any")))
        .mount(server)
        .await;

    // Add labels to a PR.
    Mock::given(method("POST"))
        .and(path_regex(r"^/repos/[^/]+/[^/]+/issues/[0-9]+/labels$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_label_array()))
        .mount(server)
        .await;

    // Remove a label from a PR.
    Mock::given(method("DELETE"))
        .and(path_regex(r"^/repos/[^/]+/[^/]+/issues/[0-9]+/labels/.+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_label_array()))
        .mount(server)
        .await;

    // Post a PR comment.
    Mock::given(method("POST"))
        .and(path_regex(r"^/repos/[^/]+/[^/]+/issues/[0-9]+/comments$"))
        .respond_with(ResponseTemplate::new(201).set_body_json(issue_comment_stub()))
        .mount(server)
        .await;
}

/// Convenience: mount label/comment ops plus check-runs for one SHA.
async fn mount_github_rest(
    server: &MockServer,
    head_sha: &str,
    failed: &[&'static str],
    passed: &[&'static str],
) {
    mount_github_label_ops(server).await;
    mount_github_check_runs(server, head_sha, failed, passed).await;
}

/// Mount the App-mode discovery + token endpoints so the JWT-mode
/// `Octocrab` can answer `apps().get_repository_installation(...)` and
/// the scoped client can mint installation tokens before its REST calls.
async fn mount_github_app_endpoints(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_regex(r"^/repos/.+/.+/installation$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(installation_stub()))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/app/installations/[0-9]+/access_tokens$"))
        .respond_with(ResponseTemplate::new(201).set_body_json(installation_token_stub()))
        .mount(server)
        .await;
}

fn installation_stub() -> Value {
    json!({
        "id": 4242,
        "account": stub_user(),
        "access_tokens_url": "https://api.github.test/app/installations/4242/access_tokens",
        "repositories_url": "https://api.github.test/installation/repositories",
        "html_url": "https://github.test/apps/sinfonia/installations/4242",
        "app_id": 12345,
        "target_id": 1,
        "target_type": "User",
        "permissions": {"checks": "read", "issues": "write", "metadata": "read"},
        "events": ["pull_request", "check_suite"],
        "single_file_name": null,
        "repository_selection": "selected",
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
    })
}

fn installation_token_stub() -> Value {
    json!({
        "token": "ghs_installtokenfake",
        "expires_at": "2099-01-01T00:00:00Z",
        "permissions": {"checks": "read", "issues": "write", "metadata": "read"},
        "repositories": null,
    })
}

// ---------------------------------------------------------------------------
// Bridge boot helper
// ---------------------------------------------------------------------------

/// The handle the test holds for one bridge instance. Dropping the
/// `_server_task` aborts the listener so each `#[tokio::test]` cleans up
/// after itself.
struct BridgeHarness {
    addr: SocketAddr,
    _server_task: JoinHandle<()>,
}

/// Boot the bridge configured in `bridge_md_src`, swapping the
/// production GitHub client out for one whose base URI is `github_uri`.
/// Returns the live bridge address.
async fn start_bridge(
    bridge_md_src: &str,
    github_uri: &str,
    auth_mode: BridgeAuthForTest,
) -> BridgeHarness {
    let cfg = parse_bridge_str(bridge_md_src).expect("test BRIDGE.md parses");
    let store = Store::open_in_memory().await.expect("in-memory store");
    let tracker_cfg = cfg.tracker.to_tracker_config();
    assert_eq!(tracker_cfg.kind, TrackerKind::Linear);
    let tracker = Arc::new(LinearTracker::new(&tracker_cfg).expect("linear tracker"));

    let gh: Arc<dyn GhOps> = match auth_mode {
        BridgeAuthForTest::Pat(token) => {
            let crab = Octocrab::builder()
                .personal_token(token)
                .base_uri(github_uri)
                .expect("base_uri parse")
                .build()
                .expect("octocrab pat build");
            Arc::new(OctocrabGhOps::from_octocrab(Arc::new(crab)))
        }
        BridgeAuthForTest::App { app_id } => {
            let key = EncodingKey::from_rsa_pem(TEST_APP_PRIVATE_KEY_PEM.as_bytes())
                .expect("test rsa pem parses");
            let crab = Octocrab::builder()
                .app(AppId(app_id), key)
                .base_uri(github_uri)
                .expect("base_uri parse")
                .build()
                .expect("octocrab app build");
            Arc::new(AppModeGhOps::new(Arc::new(crab)))
        }
    };

    let labels = LabelManager::new(
        gh.clone(),
        cfg.github.manage_labels,
        cfg.github.label_prefix.clone(),
        cfg.github.label_aliases.clone(),
    );
    let state = AppState::with_default_budget(cfg, store, tracker, gh, labels);
    let app = router(state);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local addr");
    let task = tokio::spawn(async move {
        let _ = serve(listener, app).await;
    });
    BridgeHarness {
        addr,
        _server_task: task,
    }
}

enum BridgeAuthForTest {
    Pat(String),
    App { app_id: u64 },
}

// ---------------------------------------------------------------------------
// Webhook posting helpers
// ---------------------------------------------------------------------------

type HmacSha256 = Hmac<Sha256>;

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(body);
    let bytes = mac.finalize().into_bytes();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256={hex}")
}

async fn post_webhook(
    bridge: &BridgeHarness,
    event: &str,
    delivery: &str,
    payload: &Value,
    signature: Option<&str>,
) -> (reqwest::StatusCode, Value) {
    let url = format!("http://{}/webhook", bridge.addr);
    let body = serde_json::to_vec(payload).expect("payload");
    let mut req = reqwest::Client::new()
        .post(&url)
        .header("X-GitHub-Event", event)
        .header("X-GitHub-Delivery", delivery)
        .header("content-type", "application/json")
        .body(body);
    if let Some(sig) = signature {
        req = req.header("X-Hub-Signature-256", sig);
    }
    let resp = req.send().await.expect("send webhook");
    let status = resp.status();
    let json: Value = resp.json().await.unwrap_or(Value::Null);
    (status, json)
}

async fn post_webhook_signed(
    bridge: &BridgeHarness,
    event: &str,
    delivery: &str,
    payload: &Value,
) -> (reqwest::StatusCode, Value) {
    let body = serde_json::to_vec(payload).expect("payload");
    let sig = sign(WEBHOOK_SECRET, &body);
    post_webhook(bridge, event, delivery, payload, Some(&sig)).await
}

fn pr_opened_payload(repo: &str, pr_number: u64, body: &str) -> Value {
    json!({
        "action": "opened",
        "number": pr_number,
        "pull_request": {
            "number": pr_number,
            "title": format!("test pr #{pr_number}"),
            "body": body,
        },
        "repository": {"full_name": repo},
    })
}

fn check_suite_payload(repo: &str, head_sha: &str, pr_number: u64) -> Value {
    json!({
        "action": "completed",
        "check_suite": {
            "head_sha": head_sha,
            "pull_requests": [{"number": pr_number}],
        },
        "repository": {"full_name": repo},
    })
}

fn workflow_run_payload(repo: &str, head_sha: &str, pr_number: u64, run_id: u64) -> Value {
    json!({
        "action": "completed",
        "workflow_run": {
            "id": run_id,
            "head_sha": head_sha,
            "pull_requests": [{"number": pr_number}],
        },
        "repository": {"full_name": repo},
    })
}

/// Build an in-memory zip carrying a single `bridge.json` entry.
fn bridge_zip(manifest: &Value) -> Vec<u8> {
    let bytes = serde_json::to_vec(manifest).unwrap();
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        w.start_file("bridge.json", opts).unwrap();
        w.write_all(&bytes).unwrap();
        w.finish().unwrap();
    }
    buf
}

/// One `WorkflowListArtifact` JSON row (octocrab's deserializer needs every
/// non-Option field present).
fn artifact_meta_json(id: u64, name: &str, size: u64) -> Value {
    json!({
        "id": id,
        "node_id": format!("MDg6QXJ0aWZhY3R7}}{id}"),
        "name": name,
        "size_in_bytes": size,
        "url": format!("https://api.github.test/repos/_/_/actions/artifacts/{id}"),
        "archive_download_url": format!("https://api.github.test/repos/_/_/actions/artifacts/{id}/zip"),
        "expired": false,
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "expires_at": "2099-01-01T00:00:00Z",
    })
}

/// Mount the Actions artifacts list + download endpoints for one run so the
/// bridge can fetch and unzip a `bridge.json` (Proposal 0001).
async fn mount_github_run_artifacts(
    server: &MockServer,
    run_id: u64,
    artifact_id: u64,
    artifact_name: &str,
    zip_bytes: Vec<u8>,
) {
    let list_path = format!(
        r"^/repos/[^/]+/[^/]+/actions/runs/{run_id}/artifacts$"
    );
    let size = zip_bytes.len() as u64;
    Mock::given(method("GET"))
        .and(path_regex(list_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "artifacts": [artifact_meta_json(artifact_id, artifact_name, size)],
        })))
        .mount(server)
        .await;

    let dl_path = format!(r"^/repos/[^/]+/[^/]+/actions/artifacts/{artifact_id}/zip$");
    Mock::given(method("GET"))
        .and(path_regex(dl_path))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_bytes))
        .mount(server)
        .await;
}

/// Did the GitHub mock receive a POST to `/repos/{repo}/issues/{pr}/labels`
/// containing the given label name?
async fn server_received_label_apply(
    server: &MockServer,
    repo: &str,
    pr_number: u64,
    label_name: &str,
) -> bool {
    let needle_path = format!("/repos/{repo}/issues/{pr_number}/labels");
    for r in server.received_requests().await.unwrap_or_default() {
        if r.method.as_str() == "POST" && r.url.path() == needle_path {
            let body: Value = match serde_json::from_slice(&r.body) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if body
                .get("labels")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().any(|v| v.as_str() == Some(label_name)))
                .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

/// Same shape as [`server_received_label_apply`] but for the
/// `POST /repos/{repo}/issues/{pr}/comments` route.
async fn server_received_pr_comment(
    server: &MockServer,
    repo: &str,
    pr_number: u64,
) -> bool {
    let needle = format!("/repos/{repo}/issues/{pr_number}/comments");
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .any(|r| r.method.as_str() == "POST" && r.url.path() == needle)
}

// ---------------------------------------------------------------------------
// Scenario 1 — Green PR (plan §9.2 #1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_1_green_pr_applies_awaiting_review_no_transition() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&[
        "Needs Fixes",
        "Blocked - Human Review",
        "In Progress",
    ])));

    mount_github_rest(&gh, "head-green-1", &[], &["unit", "lint"]).await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;

    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        /* max_attempts = */ 3,
        /* manage_labels = */ true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    // 1. Open PR with a tracker link so the mapping lands.
    let (st, _) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-pr-open",
        &pr_opened_payload("acme/widgets", 42, "Closes ENG-42"),
    )
    .await;
    assert_eq!(st.as_u16(), 202, "PR-open mapping should be queued");

    // 2. Fire the check_suite event — green CI should hit awaiting-review.
    let (st, body) = post_webhook_signed(
        &bridge,
        "check_suite",
        "deliv-cs-green",
        &check_suite_payload("acme/widgets", "head-green-1", 42),
    )
    .await;
    assert_eq!(st.as_u16(), 202, "green CI should respond 202; body={body}");
    assert_eq!(
        body["outcomes"][0]["kind"], "green",
        "outcome should be green; body={body}"
    );

    // 3. The bridge should have applied `sinfonia:awaiting-review`.
    assert!(
        server_received_label_apply(&gh, "acme/widgets", 42, "sinfonia:awaiting-review").await,
        "expected awaiting-review label apply call"
    );
    // 4. No tracker transitions should have happened on green.
    assert!(
        linear_state.lock().unwrap().transitions.is_empty(),
        "no Linear transitions on green"
    );
    // 5. No PR comment on green either.
    assert!(
        !server_received_pr_comment(&gh, "acme/widgets", 42).await,
        "green should not post a PR comment"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2 — One red CI run (plan §9.2 #2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_2_one_red_ci_run_transitions_to_needs_fixes() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&[
        "Needs Fixes",
        "Blocked - Human Review",
    ])));
    mount_github_rest(&gh, "head-red-1", &["unit/lint"], &[]).await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;

    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        3,
        true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    // 1. Map PR 7 → ENG-7.
    let (st, _) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-pr-7",
        &pr_opened_payload("acme/widgets", 7, "Closes ENG-7"),
    )
    .await;
    assert_eq!(st.as_u16(), 202);

    // 2. Red check_suite.
    let (st, body) = post_webhook_signed(
        &bridge,
        "check_suite",
        "deliv-red-7",
        &check_suite_payload("acme/widgets", "head-red-1", 7),
    )
    .await;
    assert_eq!(st.as_u16(), 202);
    assert_eq!(body["outcomes"][0]["kind"], "red");
    assert_eq!(body["outcomes"][0]["next_attempt"], 1);
    assert_eq!(body["outcomes"][0]["max_attempts"], 3);
    assert_eq!(body["outcomes"][0]["target_state"], "Needs Fixes");

    // 3. The bridge should have transitioned the ticket.
    let st = linear_state.lock().unwrap();
    assert_eq!(
        st.transitions,
        vec![("ENG-7".into(), "Needs Fixes".into())],
        "ticket should transition to Needs Fixes"
    );
    // Counter should have advanced to 1.
    let count = st.fields.get("ENG-7").and_then(|f| f.get("sinfonia_attempt_count"));
    assert!(
        matches!(count, Some(custom_fields::CustomFieldValue::Number(n)) if (*n - 1.0).abs() < 1e-9),
        "attempt_count should be 1 after first red, got {count:?}",
    );
    drop(st);

    // 4. GitHub side: needs-fixes label applied, failure comment posted.
    assert!(
        server_received_label_apply(&gh, "acme/widgets", 7, "sinfonia:needs-fixes").await,
        "expected needs-fixes label apply"
    );
    assert!(
        server_received_pr_comment(&gh, "acme/widgets", 7).await,
        "expected failure-comment POST"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3 — Three red CI runs with category routing (plan §9.2 #3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_3_three_red_runs_route_by_category() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&[
        "Needs Fixes",
        "Needs Lint Fixes",
        "Needs E2E Fixes",
        "Blocked - Human Review",
    ])));
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;

    // Two real categories + the synthetic default.
    let categories = r#"  failure_categories:
    - name: lint
      check_pattern: "(?i)lint"
      target_state: "Needs Lint Fixes"
      priority: 20
    - name: e2e
      check_pattern: "(?i)e2e"
      target_state: "Needs E2E Fixes"
      priority: 10
"#;
    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        /* max_attempts = */ 5,
        true,
        &format!("{}/", linear.uri()),
        Some(categories),
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    // Map PR 33 → ENG-33.
    let (st, _) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-pr-33",
        &pr_opened_payload("acme/widgets", 33, "Closes ENG-33"),
    )
    .await;
    assert_eq!(st.as_u16(), 202);

    // Mount the label/comment endpoints once; the per-SHA check-runs
    // routes get mounted per-iteration since each red event has a
    // distinct head_sha.
    mount_github_label_ops(&gh).await;

    let red_runs: &[(&str, &str, &[&'static str], &str, &str)] = &[
        ("head-red-A", "deliv-red-A", &["unit/lint"][..], "lint", "Needs Lint Fixes"),
        ("head-red-B", "deliv-red-B", &["e2e/login"][..], "e2e", "Needs E2E Fixes"),
        ("head-red-C", "deliv-red-C", &["unit/lint"][..], "lint", "Needs Lint Fixes"),
    ];

    for (idx, (head, deliv, failed, category, target_state)) in red_runs.iter().enumerate() {
        mount_github_check_runs(&gh, head, failed, &[]).await;
        let (st, body) = post_webhook_signed(
            &bridge,
            "check_suite",
            deliv,
            &check_suite_payload("acme/widgets", head, 33),
        )
        .await;
        assert_eq!(st.as_u16(), 202, "red iteration {idx}");
        let next = (idx as u64) + 1;
        assert_eq!(body["outcomes"][0]["next_attempt"], next, "iter {idx}: counter");
        assert_eq!(body["outcomes"][0]["category"], *category, "iter {idx}: category");
        assert_eq!(body["outcomes"][0]["target_state"], *target_state, "iter {idx}: target");

        assert!(
            server_received_label_apply(
                &gh,
                "acme/widgets",
                33,
                &format!("sinfonia:failure:{category}"),
            )
            .await,
            "iter {idx}: expected sinfonia:failure:{category} apply",
        );
    }

    // Final attempt_count should be 3, transitions vector should have 3
    // entries in the order of the iterations.
    let st = linear_state.lock().unwrap();
    assert_eq!(st.transitions.len(), 3, "three transitions total");
    assert_eq!(st.transitions[0].1, "Needs Lint Fixes");
    assert_eq!(st.transitions[1].1, "Needs E2E Fixes");
    assert_eq!(st.transitions[2].1, "Needs Lint Fixes");
    let count = st.fields.get("ENG-33").and_then(|f| f.get("sinfonia_attempt_count"));
    assert!(
        matches!(count, Some(custom_fields::CustomFieldValue::Number(n)) if (*n - 3.0).abs() < 1e-9),
        "attempt_count should be 3 after three reds, got {count:?}",
    );
}

// ---------------------------------------------------------------------------
// Scenario 4 — Cap hit (plan §9.2 #4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_4_fourth_red_hits_cap_and_blocks() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&[
        "Needs Fixes",
        "Blocked - Human Review",
    ])));
    // Pre-seed the marker comment to attempt_count = 3 so the next red
    // run is the cap-hit step. (max_attempts = 3 → prior == max → cap.)
    {
        let mut st = linear_state.lock().unwrap();
        let mut fields = custom_fields::FieldsMap::new();
        fields.insert(
            "sinfonia_attempt_count".into(),
            custom_fields::CustomFieldValue::Number(3.0),
        );
        st.fields.insert("ENG-99".into(), fields);
        st.marker_comment_ids
            .insert("ENG-99".into(), "comment-seed".into());
    }

    mount_github_rest(&gh, "head-cap", &["unit/lint"], &[]).await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;

    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        /* max_attempts = */ 3,
        true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    let (st, _) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-pr-99",
        &pr_opened_payload("acme/widgets", 99, "Closes ENG-99"),
    )
    .await;
    assert_eq!(st.as_u16(), 202);

    let (st, body) = post_webhook_signed(
        &bridge,
        "check_suite",
        "deliv-cap-99",
        &check_suite_payload("acme/widgets", "head-cap", 99),
    )
    .await;
    assert_eq!(st.as_u16(), 202);
    assert_eq!(body["outcomes"][0]["kind"], "cap_hit");
    assert_eq!(body["outcomes"][0]["stayed_at"], 3);
    assert_eq!(body["outcomes"][0]["max"], 3);

    let st = linear_state.lock().unwrap();
    assert_eq!(
        st.transitions,
        vec![("ENG-99".into(), "Blocked - Human Review".into())],
        "cap-hit should transition to blocked_state"
    );
    // The counter must NOT have advanced past max.
    let count = st.fields.get("ENG-99").and_then(|f| f.get("sinfonia_attempt_count"));
    assert!(
        matches!(count, Some(custom_fields::CustomFieldValue::Number(n)) if (*n - 3.0).abs() < 1e-9),
        "attempt_count should stay at 3 on cap-hit, got {count:?}",
    );
    drop(st);

    assert!(
        server_received_label_apply(&gh, "acme/widgets", 99, "sinfonia:cap-hit").await,
        "expected cap-hit label apply"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5 — Webhook redelivery is a no-op (plan §9.2 #5)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_5_webhook_redelivery_is_no_op() {
    // The redelivery path short-circuits before any GitHub / Linear call,
    // so neither mock needs handler mounts beyond the GraphQL catch-all.
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&["Needs Fixes"])));
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;
    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        3,
        true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    let payload = pr_opened_payload("acme/widgets", 11, "Closes ENG-11");
    let (st1, _) = post_webhook_signed(&bridge, "pull_request", "deliv-dup", &payload).await;
    assert_eq!(st1.as_u16(), 202, "first delivery accepted");
    let (st2, body2) =
        post_webhook_signed(&bridge, "pull_request", "deliv-dup", &payload).await;
    assert_eq!(st2.as_u16(), 200, "redelivery is 200 no-op");
    assert_eq!(body2["status"], "duplicate");
}

// ---------------------------------------------------------------------------
// Scenario 6 — Bad signature is rejected (plan §9.2 #6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_6_signature_failure_returns_401() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&["Needs Fixes"])));
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;
    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        3,
        true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    let payload = pr_opened_payload("acme/widgets", 13, "Closes ENG-13");
    let body = serde_json::to_vec(&payload).unwrap();
    let bad_sig = sign("wrong-secret", &body);
    let (st, _) = post_webhook(
        &bridge,
        "pull_request",
        "deliv-bad-sig",
        &payload,
        Some(&bad_sig),
    )
    .await;
    assert_eq!(st.as_u16(), 401);
    // No GitHub / Linear calls should have leaked through.
    let github_reqs = gh.received_requests().await.unwrap_or_default();
    assert!(github_reqs.is_empty(), "no GitHub calls on bad sig");
    let linear_reqs = linear.received_requests().await.unwrap_or_default();
    assert!(linear_reqs.is_empty(), "no Linear calls on bad sig");
}

// ---------------------------------------------------------------------------
// Scenario 7 — PR without a tracker link (plan §9.2 #7)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_7_pr_without_tracker_link_is_ignored() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&["Needs Fixes"])));
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;
    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        3,
        true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    let (st, body) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-no-link",
        &pr_opened_payload("acme/widgets", 5, "no tracker reference at all"),
    )
    .await;
    assert_eq!(st.as_u16(), 200);
    assert_eq!(body["status"], "ignored");
    assert_eq!(body["reason"], "no tracker link in PR");
    // No outbound calls.
    let github_reqs = gh.received_requests().await.unwrap_or_default();
    assert!(github_reqs.is_empty(), "no GitHub calls when link is missing");
}

// ---------------------------------------------------------------------------
// Scenario 8 — GitHub App auth, equivalent of scenario 2 (plan §9.2 #8)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_8_app_mode_one_red_ci_run() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&[
        "Needs Fixes",
        "Blocked - Human Review",
    ])));
    mount_github_rest(&gh, "head-app-red", &["unit/lint"], &[]).await;
    mount_github_app_endpoints(&gh).await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;

    let cfg = bridge_md(
        None,
        Some(12345),
        Some(TEST_APP_PRIVATE_KEY_PEM),
        3,
        true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(
        &cfg,
        &gh.uri(),
        BridgeAuthForTest::App { app_id: 12345 },
    )
    .await;

    // Map PR 88 → ENG-88.
    let (st, _) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-pr-88",
        &pr_opened_payload("acme/widgets", 88, "Closes ENG-88"),
    )
    .await;
    assert_eq!(st.as_u16(), 202);

    // Red check_suite — App-mode resolves the installation lazily on
    // the first GhOps call (`list_check_run_summary`), so this single
    // POST exercises the installation-discovery + token-mint paths.
    let (st, body) = post_webhook_signed(
        &bridge,
        "check_suite",
        "deliv-app-red",
        &check_suite_payload("acme/widgets", "head-app-red", 88),
    )
    .await;
    assert_eq!(st.as_u16(), 202, "App-mode red CI should respond 202; body={body}");
    assert_eq!(body["outcomes"][0]["kind"], "red");

    let stl = linear_state.lock().unwrap();
    assert_eq!(
        stl.transitions,
        vec![("ENG-88".into(), "Needs Fixes".into())],
        "App-mode should still drive the transition",
    );
    drop(stl);

    // Sanity: the App discovery + token-mint endpoints saw traffic.
    let reqs = gh.received_requests().await.unwrap_or_default();
    assert!(
        reqs.iter()
            .any(|r| r.url.path() == "/repos/acme/widgets/installation"),
        "expected GET /repos/acme/widgets/installation",
    );
    assert!(
        reqs.iter()
            .any(|r| r.url.path().starts_with("/app/installations/")
                && r.url.path().ends_with("/access_tokens")),
        "expected POST /app/installations/{{id}}/access_tokens",
    );
}

// ---------------------------------------------------------------------------
// Scenario 9 — manage_labels: false skips label ops (plan §9.2 #9)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_9_manage_labels_false_skips_label_calls() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&[
        "Needs Fixes",
        "Blocked - Human Review",
    ])));
    mount_github_rest(&gh, "head-nlbl", &["unit/lint"], &[]).await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;

    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        3,
        /* manage_labels = */ false,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    let (st, _) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-pr-nlbl",
        &pr_opened_payload("acme/widgets", 21, "Closes ENG-21"),
    )
    .await;
    assert_eq!(st.as_u16(), 202);

    let (st, body) = post_webhook_signed(
        &bridge,
        "check_suite",
        "deliv-red-nlbl",
        &check_suite_payload("acme/widgets", "head-nlbl", 21),
    )
    .await;
    assert_eq!(st.as_u16(), 202);
    assert_eq!(body["outcomes"][0]["kind"], "red");

    // Transition + comment still happen.
    let stl = linear_state.lock().unwrap();
    assert_eq!(
        stl.transitions,
        vec![("ENG-21".into(), "Needs Fixes".into())],
    );
    drop(stl);
    assert!(
        server_received_pr_comment(&gh, "acme/widgets", 21).await,
        "failure-comment should still post when manage_labels=false"
    );

    // But no label endpoint should have been hit.
    let reqs = gh.received_requests().await.unwrap_or_default();
    for r in &reqs {
        let p = r.url.path();
        assert!(
            !p.ends_with("/labels") && !p.contains("/labels/"),
            "expected zero label calls; got {} {}",
            r.method,
            p,
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 10 — workflow_run red CI ingests the bridge.json manifest and
// folds the structured digest into sinfonia_last_ci_failure (Proposal 0001).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scenario_10_workflow_run_ingests_harness_manifest_digest() {
    let gh = MockServer::start().await;
    let linear = MockServer::start().await;
    let linear_state = Arc::new(Mutex::new(LinearMockState::new_with_states(&[
        "Needs Fixes",
        "Blocked - Human Review",
    ])));

    // Red check-runs on the head SHA so the feedback loop takes the red path.
    mount_github_rest(&gh, "head-wf-red", &["e2e/playwright"], &[]).await;

    // The run's artifact bundle holds a v2 bridge.json with one failure.
    let manifest = json!({
        "schema_version": 2,
        "pr_number": 77,
        "run_url": "https://github.com/acme/widgets/actions/runs/555",
        "artifact_bundle_name": "harness-runs-555",
        "failures": [{
            "scenario": "Create tenant persists across reload",
            "feature_file": "requirements/features/tenant/create-tenant.feature",
            "step": "Then the tenant list shows \"Acme\"",
            "assertion": "Expected [data-testid='tenant-row-acme'] to be visible; was not present in DOM",
            "artifact_urls": {
                "result": "<dir>/result.json",
                "trace": "<dir>/trace.zip",
                "video": "<dir>/video.webm",
                "a11y": "<dir>/a11y.json"
            }
        }],
    });
    mount_github_run_artifacts(&gh, 555, 9001, "bridge-555", bridge_zip(&manifest)).await;

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(LinearGraphqlMock::new(linear_state.clone()))
        .mount(&linear)
        .await;

    // bridge_md sets no ingest key → it defaults ON (Task 7 flip).
    let cfg = bridge_md(
        Some("ghp_test"),
        None,
        None,
        3,
        true,
        &format!("{}/", linear.uri()),
        None,
    );
    let bridge = start_bridge(&cfg, &gh.uri(), BridgeAuthForTest::Pat("ghp_test".into())).await;

    // Map PR 77 → ENG-77.
    let (st, _) = post_webhook_signed(
        &bridge,
        "pull_request",
        "deliv-pr-77",
        &pr_opened_payload("acme/widgets", 77, "Closes ENG-77"),
    )
    .await;
    assert_eq!(st.as_u16(), 202);

    // Fire the red workflow_run — this is the ingestion trigger.
    let (st, body) = post_webhook_signed(
        &bridge,
        "workflow_run",
        "deliv-wf-77",
        &workflow_run_payload("acme/widgets", "head-wf-red", 77, 555),
    )
    .await;
    assert_eq!(st.as_u16(), 202, "red workflow_run should respond 202; body={body}");
    assert_eq!(body["outcomes"][0]["kind"], "red");

    // The structured digest — not a comma-joined check name — must have
    // landed in sinfonia_last_ci_failure on the ticket.
    let stl = linear_state.lock().unwrap();
    let field = stl
        .fields
        .get("ENG-77")
        .and_then(|f| f.get("sinfonia_last_ci_failure"))
        .cloned();
    drop(stl);
    let text = match field {
        Some(custom_fields::CustomFieldValue::String(s)) => s,
        other => panic!("expected a string digest in sinfonia_last_ci_failure, got {other:?}"),
    };
    assert!(
        text.contains("harness reported 1 failing scenario(s):"),
        "field should carry the structured digest header; got: {text}"
    );
    assert!(
        text.contains("Create tenant persists across reload"),
        "digest should name the failing scenario; got: {text}"
    );
    assert!(
        text.contains("schema_version=2"),
        "digest should cite the manifest version; got: {text}"
    );

    // Sanity: the bridge actually hit the artifact list + download endpoints.
    let reqs = gh.received_requests().await.unwrap_or_default();
    assert!(
        reqs.iter()
            .any(|r| r.url.path() == "/repos/acme/widgets/actions/runs/555/artifacts"),
        "expected the run-artifacts list call",
    );
    assert!(
        reqs.iter()
            .any(|r| r.url.path() == "/repos/acme/widgets/actions/artifacts/9001/zip"),
        "expected the artifact zip download call",
    );
}
