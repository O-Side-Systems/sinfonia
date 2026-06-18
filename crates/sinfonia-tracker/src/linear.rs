//! Linear GraphQL adapter (spec §11.2).

use crate::config::TrackerConfig;
use crate::custom_fields::{self, CustomFieldSchema, CustomFieldValue, MarkerEnvelope};
use crate::error::{Error, Result};
use crate::types::{BlockerRef, Issue, IssueState};
use crate::IssueTracker;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::{json, Value as Json};
use std::time::Duration;
use tracing::{debug, info_span, Instrument};

/// Build a `tracker.fetch` span (Phase 3 plan §4). Span name + attribute
/// keys are hardcoded literals so the tracker crate stays free of any
/// build-time dependency on the binary crates' `telemetry::spans` modules
/// — the strings are the operator-facing contract, identical on both
/// sides.
fn tracker_fetch_span(request_kind: &'static str) -> tracing::Span {
    info_span!(
        "tracker.fetch",
        tracker_kind = "linear",
        request_kind = request_kind,
        result_count = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    )
}

const ISSUE_FRAGMENT: &str = r#"
  id
  identifier
  title
  description
  priority
  branchName
  url
  createdAt
  updatedAt
  state { name }
  labels(first: 50) { nodes { name } }
  inverseRelations(first: 50) {
    nodes {
      type
      issue { id identifier state { name } }
    }
  }
  comments(first: 100) {
    nodes { body }
  }
"#;
// Why `comments(first: 100)` here, in the candidate-fetch fragment? Because
// Phase 1's bridge stores per-ticket state in a single bot-owned marker
// comment, and the agent prompt template reads it via `{{ issue.fields.* }}`.
// Including it in the same GraphQL hop avoids an N+1 round-trip per active
// issue. The 100-comment cap is fine in practice — the marker is created at
// the first bridge interaction and rewritten in place, so it'll always be
// among the first ~100 unless humans + the bot have generated more than that
// in a single ticket lifecycle. Document this in `docs/SPEC.md` §11.6 so
// the limit is part of the contract, not an implementation detail.

pub struct LinearTracker {
    client: Client,
    endpoint: String,
    api_key: String,
    project_slug: String,
    active_states: Vec<String>,
}

impl LinearTracker {
    /// Construct a Linear adapter from a resolved [`TrackerConfig`].
    ///
    /// Errors with `MissingTrackerApiKey` / `MissingTrackerProjectSlug` if
    /// the corresponding fields aren't populated.
    pub fn new(cfg: &TrackerConfig) -> Result<Self> {
        let api_key = cfg
            .api_key
            .clone()
            .ok_or(Error::MissingTrackerApiKey)?;
        let project_slug = cfg
            .project_slug
            .clone()
            .ok_or(Error::MissingTrackerProjectSlug)?;
        let client = Client::builder()
            .timeout(Duration::from_millis(30_000))
            .build()
            .map_err(|e| Error::LinearApiRequest(e.to_string()))?;
        Ok(LinearTracker {
            client,
            endpoint: cfg.endpoint.clone(),
            api_key,
            project_slug,
            active_states: cfg.active_states.clone(),
        })
    }

    async fn post(&self, query: &str, variables: Json) -> Result<Json> {
        let body = json!({ "query": query, "variables": variables });
        let resp = self
            .client
            .post(&self.endpoint)
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::LinearApiRequest(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::LinearApiStatus(format!(
                "{} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let v: Json = resp
            .json()
            .await
            .map_err(|e| Error::LinearUnknownPayload(e.to_string()))?;
        if let Some(errs) = v.get("errors") {
            return Err(Error::LinearGraphqlErrors(errs.to_string()));
        }
        Ok(v)
    }

    async fn page_issues_by_state_in(&self, states: &[String]) -> Result<Vec<Issue>> {
        if states.is_empty() {
            return Ok(vec![]);
        }
        let query = format!(
            r#"query($slug: String!, $states: [String!], $first: Int!, $after: String) {{
                issues(
                    first: $first,
                    after: $after,
                    filter: {{
                        project: {{ slugId: {{ eq: $slug }} }},
                        state: {{ name: {{ in: $states }} }}
                    }}
                ) {{
                    nodes {{ {fragment} }}
                    pageInfo {{ hasNextPage endCursor }}
                }}
            }}"#,
            fragment = ISSUE_FRAGMENT
        );

        let mut out: Vec<Issue> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let vars = json!({
                "slug": self.project_slug,
                "states": states,
                "first": 50,
                "after": cursor,
            });
            let resp = self.post(&query, vars).await?;
            let issues_node = resp
                .get("data")
                .and_then(|d| d.get("issues"))
                .ok_or_else(|| Error::LinearUnknownPayload("missing data.issues".into()))?;
            let nodes = issues_node
                .get("nodes")
                .and_then(|n| n.as_array())
                .cloned()
                .unwrap_or_default();
            for n in nodes {
                out.push(normalize_full(&n)?);
            }
            let page_info = issues_node
                .get("pageInfo")
                .ok_or_else(|| Error::LinearUnknownPayload("missing pageInfo".into()))?;
            let has_next = page_info
                .get("hasNextPage")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !has_next {
                break;
            }
            let end = page_info
                .get("endCursor")
                .and_then(|v| v.as_str())
                .ok_or(Error::LinearMissingEndCursor)?;
            cursor = Some(end.to_string());
        }
        Ok(out)
    }

    // ---- Bridge-write helpers (v0.3 spec §11.6) -----------------------------
    //
    // Linear doesn't have native custom fields the way Jira does. The bridge
    // stores all of its per-ticket state inside a single bot-owned comment on
    // the issue, with the body shaped as `{"sinfonia_bridge_state_v1": {…}}`
    // (see `custom_fields` module docs). The methods below own the load /
    // mutate / store cycle on that comment.

    /// Fetch the bot-owned marker comment (if it exists) along with its
    /// Linear comment ID so we can `commentUpdate` instead of creating
    /// a duplicate. Returns `Ok((None, None))` when the issue exists but
    /// has no marker comment yet.
    async fn load_marker_comment(
        &self,
        issue_id: &str,
    ) -> Result<(Option<String>, Option<MarkerEnvelope>)> {
        let query = r#"
          query($id: String!) {
            issue(id: $id) {
              id
              comments(first: 100) {
                nodes { id body }
              }
            }
          }
        "#;
        let resp = self.post(query, json!({ "id": issue_id })).await?;
        let nodes = resp
            .get("data")
            .and_then(|d| d.get("issue"))
            .and_then(|i| i.get("comments"))
            .and_then(|c| c.get("nodes"))
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        for n in nodes {
            let body = n.get("body").and_then(|b| b.as_str()).unwrap_or("");
            if let Some(env) = custom_fields::decode_marker(body) {
                let id = n
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                return Ok((id, Some(env)));
            }
        }
        Ok((None, None))
    }

    /// Idempotent upsert of the marker comment. Creates it if absent,
    /// updates the existing one otherwise.
    async fn store_marker_comment(
        &self,
        issue_id: &str,
        envelope: &MarkerEnvelope,
        existing_comment_id: Option<&str>,
    ) -> Result<()> {
        let body = custom_fields::encode_marker(envelope);
        match existing_comment_id {
            Some(cid) => {
                let m = r#"
                  mutation($id: String!, $body: String!) {
                    commentUpdate(id: $id, input: { body: $body }) { success }
                  }
                "#;
                self.post(m, json!({ "id": cid, "body": body })).await?;
            }
            None => {
                let m = r#"
                  mutation($issueId: String!, $body: String!) {
                    commentCreate(input: { issueId: $issueId, body: $body }) { success }
                  }
                "#;
                self.post(m, json!({ "issueId": issue_id, "body": body }))
                    .await?;
            }
        }
        Ok(())
    }

    /// Look up the workflow-state ID for a given state name on the team that
    /// owns this issue. Linear `issueUpdate` needs the state ID; users
    /// configure state *names*. Two GraphQL hops in the worst case (no
    /// cache as of v0.3).
    async fn resolve_state_id(&self, issue_id: &str, state_name: &str) -> Result<String> {
        let query = r#"
          query($id: String!) {
            issue(id: $id) {
              id
              team { id states(first: 100) { nodes { id name } } }
            }
          }
        "#;
        let resp = self.post(query, json!({ "id": issue_id })).await?;
        let nodes = resp
            .get("data")
            .and_then(|d| d.get("issue"))
            .and_then(|i| i.get("team"))
            .and_then(|t| t.get("states"))
            .and_then(|s| s.get("nodes"))
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        for n in nodes {
            let name = n.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.eq_ignore_ascii_case(state_name) {
                return n
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        Error::LinearUnknownPayload(format!(
                            "state '{}' has no id",
                            state_name
                        ))
                    });
            }
        }
        Err(Error::Other(format!(
            "linear: no workflow state named '{}' on the team of issue {}",
            state_name, issue_id
        )))
    }
}

#[async_trait]
impl IssueTracker for LinearTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        let span = tracker_fetch_span("candidate_issues");
        let started = std::time::Instant::now();
        let res = self
            .page_issues_by_state_in(&self.active_states)
            .instrument(span.clone())
            .await;
        if let Ok(v) = &res {
            span.record("result_count", v.len() as i64);
        }
        span.record("duration_ms", started.elapsed().as_millis() as i64);
        res
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        if states.is_empty() {
            return Ok(vec![]);
        }
        let span = tracker_fetch_span("issues_by_states");
        let started = std::time::Instant::now();
        let res = self
            .page_issues_by_state_in(states)
            .instrument(span.clone())
            .await;
        if let Ok(v) = &res {
            span.record("result_count", v.len() as i64);
        }
        span.record("duration_ms", started.elapsed().as_millis() as i64);
        res
    }

    async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<Vec<IssueState>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let span = tracker_fetch_span("issue_states_by_ids");
        let started = std::time::Instant::now();
        let res: Result<Vec<IssueState>> = (async {
        // GraphQL ID typing per §11.2.
        let query = r#"query($ids: [ID!]) {
            issues(filter: { id: { in: $ids } }) {
                nodes { id identifier state { name } }
            }
        }"#;
        let resp = self
            .post(query, json!({ "ids": ids }))
            .await?;
        let nodes = resp
            .get("data")
            .and_then(|d| d.get("issues"))
            .and_then(|i| i.get("nodes"))
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(nodes.len());
        for n in nodes {
            let id = n
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::LinearUnknownPayload("missing id".into()))?
                .to_string();
            let identifier = n
                .get("identifier")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let state = n
                .get("state")
                .and_then(|s| s.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.push(IssueState {
                id,
                identifier,
                state,
            });
        }
        Ok(out)
        })
        .instrument(span.clone())
        .await;
        if let Ok(v) = &res {
            span.record("result_count", v.len() as i64);
        }
        span.record("duration_ms", started.elapsed().as_millis() as i64);
        res
    }

    async fn raw_graphql(
        &self,
        query: &str,
        variables: Option<Json>,
    ) -> Result<Json> {
        let resp = self
            .post(query, variables.unwrap_or(Json::Object(Default::default())))
            .await?;
        debug!(target: "tracker.linear", "raw graphql ok");
        Ok(resp)
    }

    // ---- Bridge-only writes (spec §11.6, v0.3) -----------------------------

    async fn transition_issue(&self, id: &str, target_state: &str) -> Result<()> {
        let state_id = self.resolve_state_id(id, target_state).await?;
        let m = r#"
          mutation($id: String!, $stateId: String!) {
            issueUpdate(id: $id, input: { stateId: $stateId }) {
              success issue { id state { name } }
            }
          }
        "#;
        self.post(m, json!({ "id": id, "stateId": state_id })).await?;
        debug!(target: "tracker.linear", issue_id=%id, target=%target_state, "transition ok");
        Ok(())
    }

    async fn read_custom_field(&self, id: &str, key: &str) -> Result<CustomFieldValue> {
        let (_, env) = self.load_marker_comment(id).await?;
        Ok(env
            .and_then(|e| e.fields.get(key).cloned())
            .unwrap_or(CustomFieldValue::Null))
    }

    async fn write_custom_field(
        &self,
        id: &str,
        key: &str,
        value: CustomFieldValue,
    ) -> Result<()> {
        let (cid, env) = self.load_marker_comment(id).await?;
        let mut env = env.unwrap_or_default();
        if value.is_null() {
            env.fields.remove(key);
        } else {
            env.fields.insert(key.to_string(), value);
        }
        self.store_marker_comment(id, &env, cid.as_deref()).await
    }

    async fn ensure_custom_field(&self, _schema: &CustomFieldSchema) -> Result<()> {
        // No-op on Linear: the marker comment carries the schema implicitly.
        // Jira's implementation in Phase 4 creates a real customfield_NNNNN.
        Ok(())
    }

    async fn post_comment(&self, id: &str, body: &str) -> Result<()> {
        let m = r#"
          mutation($issueId: String!, $body: String!) {
            commentCreate(input: { issueId: $issueId, body: $body }) { success }
          }
        "#;
        self.post(m, json!({ "issueId": id, "body": body })).await?;
        Ok(())
    }
}

/// Normalize a Linear `Issue` node into our domain model.
fn normalize_full(n: &Json) -> Result<Issue> {
    let id = n
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::LinearUnknownPayload("issue.id missing".into()))?
        .to_string();
    let identifier = n
        .get("identifier")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::LinearUnknownPayload("issue.identifier missing".into()))?
        .to_string();
    let title = n
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = n.get("description").and_then(|v| v.as_str()).map(str::to_string);
    let priority = n.get("priority").and_then(|v| {
        if v.is_i64() {
            v.as_i64()
        } else if v.is_f64() {
            // Linear sometimes returns floats; only accept integer-valued floats.
            v.as_f64().and_then(|f| {
                if f.fract() == 0.0 {
                    Some(f as i64)
                } else {
                    None
                }
            })
        } else {
            None
        }
    });
    let state = n
        .get("state")
        .and_then(|s| s.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let branch_name = n.get("branchName").and_then(|v| v.as_str()).map(str::to_string);
    let url = n.get("url").and_then(|v| v.as_str()).map(str::to_string);

    let labels = n
        .get("labels")
        .and_then(|l| l.get("nodes"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()).map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    // Blockers come from `inverseRelations` where type == "blocks". §11.3.
    let blocked_by = n
        .get("inverseRelations")
        .and_then(|r| r.get("nodes"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|rel| {
                    let kind = rel.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if kind != "blocks" {
                        return None;
                    }
                    let issue = rel.get("issue")?;
                    Some(BlockerRef {
                        id: issue.get("id").and_then(|v| v.as_str()).map(str::to_string),
                        identifier: issue
                            .get("identifier")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        state: issue
                            .get("state")
                            .and_then(|s| s.get("name"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let created_at = parse_ts(n.get("createdAt"));
    let updated_at = parse_ts(n.get("updatedAt"));

    // Pull bridge-written custom fields out of the marker comment, if any
    // (spec §11.6 / §11.7.1). The marker is the first comment whose body
    // decodes as a `sinfonia_bridge_state_v1` envelope. We scan up to 100
    // comments; see `ISSUE_FRAGMENT` for the rationale.
    let fields = n
        .get("comments")
        .and_then(|c| c.get("nodes"))
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|c| c.get("body").and_then(|b| b.as_str()))
                .find_map(crate::custom_fields::decode_marker)
        })
        .map(|env| env.fields)
        .unwrap_or_default();

    Ok(Issue {
        id,
        identifier,
        title,
        description,
        priority,
        state,
        branch_name,
        url,
        labels,
        blocked_by,
        children: vec![], // D-05: children no longer fetched; field kept empty (struct cleanup deferred)
        created_at,
        updated_at,
        fields,
    })
}

fn parse_ts(v: Option<&Json>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
