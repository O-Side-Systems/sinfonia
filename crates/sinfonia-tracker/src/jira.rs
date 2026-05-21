//! Jira Cloud REST adapter.
//!
//! Not in the language-agnostic spec (§11 currently only requires Linear), but added per
//! this implementation's mandate to support Jira as a tracker. Conforms to the same
//! `IssueTracker` contract and the same normalization rules in §11.3 to the extent the
//! Jira REST API exposes equivalent data.
//!
//! Authentication: HTTP Basic with `<email>:<api_token>` (Atlassian Cloud) when
//! `tracker.email` is set, otherwise `Bearer <api_token>` (self-hosted PAT).

use crate::config::TrackerConfig;
use crate::error::{Error, Result};
use crate::types::{BlockerRef, Issue, IssueState};
use crate::IssueTracker;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::{json, Value as Json};
use std::time::Duration;
use url::Url;

const FIELDS: &str = "summary,description,priority,status,labels,issuelinks,created,updated";

pub struct JiraTracker {
    client: Client,
    base_url: Url,
    project_key: String,
    active_states: Vec<String>,
    terminal_states: Vec<String>,
}

impl JiraTracker {
    /// Construct a Jira adapter from a resolved [`TrackerConfig`].
    ///
    /// `endpoint` is the site base URL (e.g. `https://acme.atlassian.net`).
    /// When `jira_email` is set on the config, Basic auth is used
    /// (email + API token); otherwise the `api_key` is treated as a Bearer
    /// token (Jira Server / Data Center PAT mode).
    pub fn new(cfg: &TrackerConfig) -> Result<Self> {
        let api_key = cfg
            .api_key
            .clone()
            .ok_or(Error::MissingTrackerApiKey)?;
        let project_key = cfg
            .project_slug
            .clone()
            .ok_or(Error::MissingTrackerProjectSlug)?;

        // Endpoint is the base URL of the Jira site, e.g. https://acme.atlassian.net.
        if cfg.endpoint.is_empty() {
            return Err(Error::ConfigInvalid(
                "tracker.endpoint is required for Jira (e.g. https://acme.atlassian.net)".into(),
            ));
        }
        let base_url = Url::parse(&cfg.endpoint)
            .map_err(|e| Error::ConfigInvalid(format!("invalid tracker.endpoint: {e}")))?;

        let mut headers = HeaderMap::new();
        let auth_value = if let Some(email) = cfg.jira_email.as_deref() {
            let raw = format!("{}:{}", email, api_key);
            format!("Basic {}", base64_encode(raw.as_bytes()))
        } else {
            format!("Bearer {}", api_key)
        };
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_value)
                .map_err(|e| Error::ConfigInvalid(format!("bad jira auth header: {e}")))?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .timeout(Duration::from_millis(30_000))
            .default_headers(headers)
            .build()
            .map_err(|e| Error::JiraApiRequest(e.to_string()))?;

        Ok(JiraTracker {
            client,
            base_url,
            project_key,
            active_states: cfg.active_states.clone(),
            terminal_states: cfg.terminal_states.clone(),
        })
    }

    fn search_url(&self) -> Result<Url> {
        self.base_url
            .join("rest/api/3/search")
            .map_err(|e| Error::JiraApiRequest(format!("bad search url: {e}")))
    }

    async fn search(&self, jql: &str) -> Result<Vec<Json>> {
        let mut out: Vec<Json> = Vec::new();
        let mut start_at: i64 = 0;
        let max_results: i64 = 50;
        loop {
            let body = json!({
                "jql": jql,
                "startAt": start_at,
                "maxResults": max_results,
                "fields": FIELDS.split(',').collect::<Vec<_>>(),
            });
            let resp = self
                .client
                .post(self.search_url()?)
                .json(&body)
                .send()
                .await
                .map_err(|e| Error::JiraApiRequest(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(Error::JiraApiStatus(format!(
                    "{} {}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                )));
            }
            let v: Json = resp
                .json()
                .await
                .map_err(|e| Error::JiraUnknownPayload(e.to_string()))?;
            let issues = v
                .get("issues")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            let returned = issues.len() as i64;
            out.extend(issues);
            let total = v.get("total").and_then(|x| x.as_i64()).unwrap_or(0);
            start_at += returned;
            if returned == 0 || start_at >= total {
                break;
            }
        }
        Ok(out)
    }

    fn jql_for_states(&self, states: &[String]) -> String {
        let quoted: Vec<String> = states
            .iter()
            .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
            .collect();
        format!(
            "project = \"{}\" AND status in ({})",
            self.project_key.replace('"', "\\\""),
            quoted.join(",")
        )
    }
}

#[async_trait]
impl IssueTracker for JiraTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        if self.active_states.is_empty() {
            return Ok(vec![]);
        }
        let jql = self.jql_for_states(&self.active_states);
        let raw = self.search(&jql).await?;
        let mut out = Vec::with_capacity(raw.len());
        for r in raw {
            out.push(normalize_jira(&r, &self.terminal_states)?);
        }
        Ok(out)
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        if states.is_empty() {
            return Ok(vec![]);
        }
        let jql = self.jql_for_states(states);
        let raw = self.search(&jql).await?;
        let mut out = Vec::with_capacity(raw.len());
        for r in raw {
            out.push(normalize_jira(&r, &self.terminal_states)?);
        }
        Ok(out)
    }

    async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<Vec<IssueState>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        // Jira IDs are numeric strings; resolve with `issue IN (...)`.
        let quoted: Vec<String> = ids.iter().map(|s| format!("\"{}\"", s)).collect();
        let jql = format!(
            "project = \"{}\" AND id IN ({})",
            self.project_key.replace('"', "\\\""),
            quoted.join(",")
        );
        let raw = self.search(&jql).await?;
        let mut out = Vec::with_capacity(raw.len());
        for r in raw {
            let id = r
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::JiraUnknownPayload("issue.id missing".into()))?
                .to_string();
            let identifier = r
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let state = r
                .get("fields")
                .and_then(|f| f.get("status"))
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
}

fn normalize_jira(r: &Json, terminal_states: &[String]) -> Result<Issue> {
    let id = r
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::JiraUnknownPayload("issue.id missing".into()))?
        .to_string();
    let identifier = r
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::JiraUnknownPayload("issue.key missing".into()))?
        .to_string();
    let fields = r.get("fields").cloned().unwrap_or(Json::Null);
    let title = fields
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = jira_doc_to_text(fields.get("description"));
    let priority = fields
        .get("priority")
        .and_then(|p| p.get("id"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok());
    let state = fields
        .get("status")
        .and_then(|s| s.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let url = r
        .get("self")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let labels = fields
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    // Jira "is blocked by" comes through `issuelinks` where the type label
    // matches `Blocks` and the link contains `inwardIssue`.
    let blocked_by = fields
        .get("issuelinks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|link| {
                    let type_name = link
                        .get("type")
                        .and_then(|t| t.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !type_name.eq_ignore_ascii_case("Blocks") {
                        return None;
                    }
                    let inward = link.get("inwardIssue")?;
                    let inward_state = inward
                        .get("fields")
                        .and_then(|f| f.get("status"))
                        .and_then(|s| s.get("name"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    Some(BlockerRef {
                        id: inward.get("id").and_then(|v| v.as_str()).map(str::to_string),
                        identifier: inward.get("key").and_then(|v| v.as_str()).map(str::to_string),
                        state: inward_state,
                    })
                })
                .filter(|b| {
                    // Drop already-terminal blockers so dispatch sees them as resolved.
                    match b.state.as_deref() {
                        None => true,
                        Some(s) => !terminal_states
                            .iter()
                            .any(|t| t.eq_ignore_ascii_case(s)),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // Sub-tasks (classic Jira hierarchy) — returned by default on the issue.
    // Team-managed projects with arbitrary Epic→Story hierarchies are NOT
    // covered here; a separate JQL by `parent` would be needed for those.
    let children = fields
        .get("subtasks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| crate::types::ChildRef {
                    id: c.get("id").and_then(|v| v.as_str()).map(str::to_string),
                    identifier: c
                        .get("key")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    state: c
                        .get("fields")
                        .and_then(|f| f.get("status"))
                        .and_then(|s| s.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let created_at = parse_ts(fields.get("created"));
    let updated_at = parse_ts(fields.get("updated"));

    // `fields` stays empty on Jira until Phase 4 wires real customfield_NNNNN
    // reads. Templates that reference these MUST use a `| default:` filter.
    Ok(Issue {
        id,
        identifier,
        title,
        description,
        priority,
        state,
        branch_name: None,
        url,
        labels,
        blocked_by,
        children,
        created_at,
        updated_at,
        fields: Default::default(),
    })
}

fn parse_ts(v: Option<&Json>) -> Option<DateTime<Utc>> {
    let s = v?.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Flatten the Atlassian Document Format `description` into plain text.
fn jira_doc_to_text(v: Option<&Json>) -> Option<String> {
    let v = v?;
    if v.is_string() {
        return v.as_str().map(str::to_string);
    }
    let mut out = String::new();
    walk_doc(v, &mut out);
    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn walk_doc(v: &Json, out: &mut String) {
    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        out.push_str(text);
    }
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        for child in arr {
            walk_doc(child, out);
            out.push(' ');
        }
    }
}

fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        out.push(ALPHABET[(n & 63) as usize] as char);
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut n: u32 = 0;
        for (i, b) in rem.iter().enumerate() {
            n |= (*b as u32) << (16 - i * 8);
        }
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        if rem.len() == 2 {
            out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
            out.push('=');
        } else {
            out.push_str("==");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_basic() {
        assert_eq!(base64_encode(b"foo:bar"), "Zm9vOmJhcg==");
        assert_eq!(base64_encode(b"M"), "TQ==");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"Man"), "TWFu");
    }
}
