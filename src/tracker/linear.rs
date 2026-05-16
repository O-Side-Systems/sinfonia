//! Linear GraphQL adapter (spec §11.2).

use crate::config::ServiceConfig;
use crate::domain::{BlockerRef, Issue, IssueState};
use crate::errors::{Error, Result};
use crate::tracker::IssueTracker;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::{json, Value as Json};
use std::time::Duration;
use tracing::debug;

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
"#;

pub struct LinearTracker {
    client: Client,
    endpoint: String,
    api_key: String,
    project_slug: String,
    active_states: Vec<String>,
}

impl LinearTracker {
    pub fn new(cfg: &ServiceConfig) -> Result<Self> {
        let api_key = cfg
            .tracker
            .api_key
            .clone()
            .ok_or(Error::MissingTrackerApiKey)?;
        let project_slug = cfg
            .tracker
            .project_slug
            .clone()
            .ok_or(Error::MissingTrackerProjectSlug)?;
        let client = Client::builder()
            .timeout(Duration::from_millis(30_000))
            .build()
            .map_err(|e| Error::LinearApiRequest(e.to_string()))?;
        Ok(LinearTracker {
            client,
            endpoint: cfg.tracker.endpoint.clone(),
            api_key,
            project_slug,
            active_states: cfg.tracker.active_states.clone(),
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
}

#[async_trait]
impl IssueTracker for LinearTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        self.page_issues_by_state_in(&self.active_states).await
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        if states.is_empty() {
            return Ok(vec![]);
        }
        self.page_issues_by_state_in(states).await
    }

    async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<Vec<IssueState>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
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
        created_at,
        updated_at,
    })
}

fn parse_ts(v: Option<&Json>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
