//! `JiraTracker` write-surface integration tests against a recorded
//! shape of Jira Cloud's REST API (Phase 4, plan §5.2).
//!
//! Each test mounts a `wiremock` server, points a `JiraTracker` at it via
//! `tracker.endpoint`, and drives one of the five bridge-write methods to
//! confirm:
//!
//! - the adapter sends the documented HTTP verb + path,
//! - request bodies match what Jira Cloud accepts (JSON shapes),
//! - response bodies of the shape Jira returns are parsed without panic,
//! - the field-ID cache survives across method calls.
//!
//! These are *adapter-level* tests, not full bridge feedback-loop scenarios.
//! The bridge's red-CI + cap-hit scenarios are exercised against Linear in
//! `crates/sinfonia-bridge/tests/bridge_e2e.rs`; the Linear-vs-Jira parity
//! split is intentional — feedback-loop logic is tracker-agnostic and is
//! covered once by the existing harness, while the Jira-specific wire
//! shapes are covered here. (See 04-jira-VERIFY.md for the verification
//! matrix.)

use serde_json::{json, Value};
use sinfonia_tracker::config::{TrackerConfig, TrackerKind};
use sinfonia_tracker::custom_fields::{CustomFieldKind, CustomFieldSchema, CustomFieldValue};
use sinfonia_tracker::{IssueTracker, JiraTracker};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cloud_config(endpoint: &str) -> TrackerConfig {
    TrackerConfig {
        kind: TrackerKind::Jira,
        endpoint: endpoint.to_string(),
        api_key: Some("fake-token".to_string()),
        project_slug: Some("ENG".to_string()),
        active_states: vec!["In Progress".to_string()],
        terminal_states: vec!["Done".to_string()],
        jira_email: Some("bot@example.com".to_string()),
    }
}

#[tokio::test]
async fn transition_issue_resolves_name_to_id_and_posts() {
    let server = MockServer::start().await;

    // GET /rest/api/3/issue/ENG-7/transitions → returns two transitions;
    // bridge wants "Needs Fixes" → matches transition id "21".
    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/ENG-7/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transitions": [
                {"id": "11", "name": "Start", "to": {"id": "3", "name": "In Progress"}},
                {"id": "21", "name": "Fix It", "to": {"id": "5", "name": "Needs Fixes"}}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    // POST /transitions with the resolved transition id. Jira returns 204.
    Mock::given(method("POST"))
        .and(path("/rest/api/3/issue/ENG-7/transitions"))
        .and(header("authorization", "Basic Ym90QGV4YW1wbGUuY29tOmZha2UtdG9rZW4="))
        .respond_with(
            ResponseTemplate::new(204).set_body_bytes(b"".as_slice()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let tracker = JiraTracker::new(&cloud_config(&server.uri())).unwrap();
    tracker.transition_issue("ENG-7", "Needs Fixes").await.unwrap();

    let reqs = server.received_requests().await.unwrap();
    let post = reqs.iter().find(|r| r.method.as_str() == "POST").unwrap();
    let body: Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(body["transition"]["id"], "21");
}

#[tokio::test]
async fn transition_issue_errors_when_no_matching_state() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/ENG-7/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "transitions": [
                {"id": "11", "name": "Start", "to": {"id": "3", "name": "In Progress"}}
            ]
        })))
        .mount(&server)
        .await;

    let tracker = JiraTracker::new(&cloud_config(&server.uri())).unwrap();
    let err = tracker
        .transition_issue("ENG-7", "Blocked - Human Review")
        .await
        .expect_err("no matching transition should be a typed error");
    assert!(
        err.to_string().contains("no transition"),
        "actionable message expected, got: {err}"
    );
}

#[tokio::test]
async fn ensure_custom_field_is_idempotent_when_field_exists() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/field"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "summary", "name": "Summary", "custom": false},
            {"id": "customfield_10037", "name": "Sinfonia Attempt Count", "custom": true}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    // POST /rest/api/3/field MUST NOT be called when the field already exists.
    Mock::given(method("POST"))
        .and(path("/rest/api/3/field"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    let tracker = JiraTracker::new(&cloud_config(&server.uri())).unwrap();
    let schema = CustomFieldSchema {
        key: "sinfonia_attempt_count".to_string(),
        display_name: "Sinfonia Attempt Count".to_string(),
        kind: CustomFieldKind::Number,
        description: None,
    };
    tracker.ensure_custom_field(&schema).await.unwrap();
}

#[tokio::test]
async fn write_custom_field_resolves_id_and_caches() {
    let server = MockServer::start().await;

    // GET /field returns the directory once; subsequent calls must use
    // the cache (so the .expect(1) below acts as a regression test).
    Mock::given(method("GET"))
        .and(path("/rest/api/3/field"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "customfield_10037", "name": "Sinfonia Attempt Count", "custom": true}
        ])))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/rest/api/3/issue/ENG-7"))
        .respond_with(ResponseTemplate::new(204))
        .expect(2)
        .mount(&server)
        .await;

    let tracker = JiraTracker::new(&cloud_config(&server.uri())).unwrap();
    tracker
        .write_custom_field(
            "ENG-7",
            "sinfonia_attempt_count",
            CustomFieldValue::Number(1.0),
        )
        .await
        .unwrap();
    // Second write — the field-ID cache must serve this without a second
    // GET /field hit.
    tracker
        .write_custom_field(
            "ENG-7",
            "sinfonia_attempt_count",
            CustomFieldValue::Number(2.0),
        )
        .await
        .unwrap();

    // Inspect the last PUT body — the resolved customfield id should
    // appear as the only key in `fields` with the literal numeric value.
    let reqs = server.received_requests().await.unwrap();
    let last_put = reqs.iter().rev().find(|r| r.method.as_str() == "PUT").unwrap();
    let body: Value = serde_json::from_slice(&last_put.body).unwrap();
    assert_eq!(body["fields"]["customfield_10037"], 2.0);
}

#[tokio::test]
async fn read_custom_field_returns_number_value() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/field"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "customfield_10037", "name": "Sinfonia Attempt Count", "custom": true}
        ])))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/ENG-7"))
        .and(query_param("fields", "customfield_10037"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "10007",
            "key": "ENG-7",
            "fields": { "customfield_10037": 4 }
        })))
        .mount(&server)
        .await;

    let tracker = JiraTracker::new(&cloud_config(&server.uri())).unwrap();
    let v = tracker
        .read_custom_field("ENG-7", "sinfonia_attempt_count")
        .await
        .unwrap();
    assert!(matches!(v, CustomFieldValue::Number(n) if (n - 4.0).abs() < 1e-9));
}

#[tokio::test]
async fn post_comment_converts_markdown_to_adf() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/rest/api/3/issue/ENG-7/comment"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "12345"})))
        .expect(1)
        .mount(&server)
        .await;

    let tracker = JiraTracker::new(&cloud_config(&server.uri())).unwrap();
    tracker
        .post_comment(
            "ENG-7",
            "CI failed on **attempt 3** of 5.\n\n```\nfailing log\n```",
        )
        .await
        .unwrap();

    let reqs = server.received_requests().await.unwrap();
    let post = reqs.iter().find(|r| r.method.as_str() == "POST").unwrap();
    let body: Value = serde_json::from_slice(&post.body).unwrap();
    // Body envelope shape.
    assert_eq!(body["body"]["version"], 1);
    assert_eq!(body["body"]["type"], "doc");
    // Block sequence: a paragraph followed by a code block.
    let blocks = body["body"]["content"].as_array().unwrap();
    let types: Vec<&str> = blocks.iter().map(|b| b["type"].as_str().unwrap()).collect();
    assert_eq!(types, vec!["paragraph", "codeBlock"]);
    // The `**attempt 3**` token must have become a strong-marked text span.
    let inline = blocks[0]["content"].as_array().unwrap();
    assert!(inline.iter().any(|n| n["marks"][0]["type"] == "strong"
        && n["text"] == "attempt 3"));
}
