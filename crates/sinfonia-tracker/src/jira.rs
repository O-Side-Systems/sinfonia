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
use crate::custom_fields::{CustomFieldKind, CustomFieldSchema, CustomFieldValue};
use crate::error::{Error, Result};
use crate::jira_adf::markdown_to_adf;
use crate::types::{BlockerRef, Issue, IssueState};
use crate::IssueTracker;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde_json::{json, Value as Json};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use url::Url;

const FIELDS: &str = "summary,description,priority,status,labels,issuelinks,created,updated";

pub struct JiraTracker {
    client: Client,
    base_url: Url,
    project_key: String,
    active_states: Vec<String>,
    terminal_states: Vec<String>,
    /// Cache of `bridge custom-field key → Jira customfield_NNNNN id`.
    /// Survives the process lifetime — Jira custom-field IDs don't change.
    /// Populated lazily on first read/write per key.
    field_id_cache: Arc<RwLock<HashMap<String, String>>>,
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
            field_id_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Build a URL relative to the configured base, joining the given REST
    /// path. The path must NOT begin with a slash — `Url::join` treats a
    /// leading slash as "from the host root" and would drop any context
    /// path on self-hosted deployments running Jira at a sub-path.
    fn rest_url(&self, path: &str) -> Result<Url> {
        let trimmed = path.trim_start_matches('/');
        self.base_url
            .join(trimmed)
            .map_err(|e| Error::JiraApiRequest(format!("bad rest url {path}: {e}")))
    }

    async fn get_json(&self, path: &str) -> Result<Json> {
        let url = self.rest_url(path)?;
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::JiraApiRequest(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::JiraApiStatus(format!(
                "{} {} {}",
                status,
                path,
                resp.text().await.unwrap_or_default()
            )));
        }
        resp.json()
            .await
            .map_err(|e| Error::JiraUnknownPayload(e.to_string()))
    }

    async fn post_json(&self, path: &str, body: &Json) -> Result<Json> {
        let url = self.rest_url(path)?;
        let resp = self
            .client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| Error::JiraApiRequest(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::JiraApiStatus(format!(
                "{} {} {}",
                status,
                path,
                resp.text().await.unwrap_or_default()
            )));
        }
        // Some Jira endpoints (notably `POST /transitions` and `POST
        // /comment`) return 204 No Content or an empty body. Treat that
        // as `null` rather than failing the JSON parser.
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Error::JiraApiRequest(e.to_string()))?;
        if bytes.is_empty() {
            return Ok(Json::Null);
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::JiraUnknownPayload(e.to_string()))
    }

    async fn put_json(&self, path: &str, body: &Json) -> Result<()> {
        let url = self.rest_url(path)?;
        let resp = self
            .client
            .put(url)
            .json(body)
            .send()
            .await
            .map_err(|e| Error::JiraApiRequest(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::JiraApiStatus(format!(
                "{} {} {}",
                status,
                path,
                resp.text().await.unwrap_or_default()
            )));
        }
        Ok(())
    }

    /// Resolve a bridge-stable custom-field key (e.g. `sinfonia_attempt_count`)
    /// to a Jira `customfield_NNNNN` identifier. The bridge identifies its
    /// fields by their `name` (display name) in Jira, since Jira allocates
    /// the numeric IDs at creation time and the bridge can't know them up
    /// front. We list all fields once per key, find the entry whose name
    /// matches the bridge's display name for that key, and cache the result
    /// for subsequent calls.
    pub(crate) async fn resolve_field_id(&self, key: &str) -> Result<String> {
        {
            let guard = self.field_id_cache.read().await;
            if let Some(id) = guard.get(key) {
                return Ok(id.clone());
            }
        }
        let display = display_name_for_key(key);
        // GET /rest/api/3/field returns a flat array of every field
        // (system + custom) on the instance. This is admin-free and
        // cacheable. The /search variant requires `manage:jira-configuration`
        // scope, which we don't want to depend on.
        let v = self.get_json("rest/api/3/field").await?;
        let arr = v.as_array().ok_or_else(|| {
            Error::JiraUnknownPayload("GET /rest/api/3/field: expected array".into())
        })?;
        let id = arr
            .iter()
            .find(|f| {
                f.get("name").and_then(|n| n.as_str())
                    .map(|n| n.eq_ignore_ascii_case(&display))
                    .unwrap_or(false)
            })
            .and_then(|f| f.get("id").and_then(|n| n.as_str()))
            .map(str::to_string)
            .ok_or_else(|| {
                Error::JiraApiStatus(format!(
                    "custom field for bridge key '{key}' (display name '{display}') not found — \
                     call ensure_custom_field at bridge startup"
                ))
            })?;
        self.field_id_cache
            .write()
            .await
            .insert(key.to_string(), id.clone());
        Ok(id)
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

    // ---- Bridge-only write surface (v0.3 spec §11.6) -----------------------
    //
    // Each method here is the Jira side of the trait extension P1-B added.
    // Linear's equivalents live in `linear.rs`. Phase 4 plan: 04-jira-bridge.md.

    async fn transition_issue(&self, id: &str, target_state: &str) -> Result<()> {
        let path = format!("rest/api/3/issue/{id}/transitions");
        let v = self.get_json(&path).await?;
        let transitions = v
            .get("transitions")
            .and_then(|t| t.as_array())
            .ok_or_else(|| {
                Error::JiraUnknownPayload(format!(
                    "GET {path}: expected `transitions` array"
                ))
            })?;
        let target_id = transitions
            .iter()
            .find(|t| {
                t.get("to")
                    .and_then(|to| to.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|n| n.eq_ignore_ascii_case(target_state))
                    .unwrap_or(false)
            })
            .and_then(|t| t.get("id").and_then(|i| i.as_str()))
            .ok_or_else(|| {
                Error::JiraApiStatus(format!(
                    "no transition to state '{target_state}' available from issue {id}; \
                     edit the project workflow to add a transition into '{target_state}'"
                ))
            })?
            .to_string();
        let body = json!({ "transition": { "id": target_id } });
        self.post_json(&path, &body).await?;
        debug!(target: "tracker.jira", issue_id=%id, target=%target_state, "transition ok");
        Ok(())
    }

    async fn read_custom_field(&self, id: &str, key: &str) -> Result<CustomFieldValue> {
        let field_id = self.resolve_field_id(key).await?;
        let path = format!("rest/api/3/issue/{id}?fields={field_id}");
        let v = self.get_json(&path).await?;
        let raw = v
            .get("fields")
            .and_then(|f| f.get(&field_id))
            .cloned()
            .unwrap_or(Json::Null);
        Ok(parse_field_value(&raw))
    }

    async fn write_custom_field(
        &self,
        id: &str,
        key: &str,
        value: CustomFieldValue,
    ) -> Result<()> {
        let field_id = self.resolve_field_id(key).await?;
        let payload = json!({
            "fields": { field_id: serialize_field_value(&value) },
        });
        self.put_json(&format!("rest/api/3/issue/{id}"), &payload)
            .await?;
        debug!(target: "tracker.jira", issue_id=%id, key=%key, "custom_field write ok");
        Ok(())
    }

    async fn ensure_custom_field(&self, schema: &CustomFieldSchema) -> Result<()> {
        let display_name = &schema.display_name;
        // Idempotency check: list all fields once and look for the display
        // name. The fetch is hundreds of bytes; running it on every bridge
        // startup is cheap.
        let v = self.get_json("rest/api/3/field").await?;
        let arr = v.as_array().ok_or_else(|| {
            Error::JiraUnknownPayload("GET /rest/api/3/field: expected array".into())
        })?;
        if let Some(existing) = arr.iter().find(|f| {
            f.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.eq_ignore_ascii_case(display_name))
                .unwrap_or(false)
        }) {
            if let Some(id) = existing.get("id").and_then(|n| n.as_str()) {
                self.field_id_cache
                    .write()
                    .await
                    .insert(schema.key.clone(), id.to_string());
            }
            return Ok(());
        }
        let body = json!({
            "name": display_name,
            "description": schema.description.clone().unwrap_or_default(),
            "type": jira_field_type(schema.kind),
            "searcherKey": jira_searcher_key(schema.kind),
        });
        let created = self.post_json("rest/api/3/field", &body).await?;
        if let Some(id) = created.get("id").and_then(|n| n.as_str()) {
            self.field_id_cache
                .write()
                .await
                .insert(schema.key.clone(), id.to_string());
        }
        // Best-effort screen-scheme bind. Jira created the field but it is
        // not visible on any screen until bound; `read_custom_field` and
        // `write_custom_field` still work via the REST API regardless. If
        // we lack admin perms (403) or the instance returns 404, log and
        // point to the manual setup doc. Plan §3.4.
        if let Err(e) = self.bind_field_to_default_screen(
            created.get("id").and_then(|n| n.as_str()).unwrap_or(""),
        ).await {
            warn!(
                target: "tracker.jira",
                field = %display_name,
                error = %e,
                "created custom field but could not bind it to any screen — the field will \
                 work programmatically but won't be visible in the Jira UI. See \
                 docs/JIRA-SCREEN-SCHEME.md to bind manually."
            );
        }
        Ok(())
    }

    async fn post_comment(&self, id: &str, body: &str) -> Result<()> {
        let adf = markdown_to_adf(body);
        let payload = json!({ "body": adf });
        self.post_json(&format!("rest/api/3/issue/{id}/comment"), &payload)
            .await?;
        Ok(())
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
        children: vec![], // D-05: children no longer fetched; field kept empty (struct cleanup deferred)
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

// --- Bridge-write helpers (Phase 4) ----------------------------------------

impl JiraTracker {
    /// Attempt to bind a freshly-created custom field to a default screen
    /// so it is visible in the Jira UI. Best-effort: the bridge needs only
    /// REST read/write access to function. Plan §3.4 documents the manual
    /// fallback when this returns Err.
    async fn bind_field_to_default_screen(&self, field_id: &str) -> Result<()> {
        if field_id.is_empty() {
            return Err(Error::JiraApiStatus("created field has no id".into()));
        }
        // Discover screens. `/rest/api/3/screens` is paged; the first page is
        // plenty for the bridge's "any default screen" heuristic — the goal
        // is visibility, not exhaustive coverage. Admins who run multiple
        // workflows can complete the bind by following docs/JIRA-SCREEN-SCHEME.md.
        let screens = self
            .get_json("rest/api/3/screens?maxResults=50")
            .await?;
        let values = screens
            .get("values")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        // Prefer a screen whose name contains the project key, falling back
        // to anything with "Default" in the name.
        let project_key_lower = self.project_key.to_lowercase();
        let chosen = values
            .iter()
            .find(|s| {
                s.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.to_lowercase().contains(&project_key_lower))
                    .unwrap_or(false)
            })
            .or_else(|| {
                values.iter().find(|s| {
                    s.get("name")
                        .and_then(|n| n.as_str())
                        .map(|n| n.to_lowercase().contains("default"))
                        .unwrap_or(false)
                })
            })
            .ok_or_else(|| {
                Error::JiraApiStatus(
                    "no candidate screen found for field bind".into(),
                )
            })?;
        let screen_id = chosen
            .get("id")
            .and_then(|i| i.as_i64().map(|n| n.to_string()).or_else(|| i.as_str().map(str::to_string)))
            .ok_or_else(|| {
                Error::JiraUnknownPayload("screen entry has no id".into())
            })?;
        // Find the first tab on the screen and add the field there.
        let tabs = self
            .get_json(&format!("rest/api/3/screens/{screen_id}/tabs"))
            .await?;
        let first_tab = tabs
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| Error::JiraApiStatus("screen has no tabs".into()))?;
        let tab_id = first_tab
            .get("id")
            .and_then(|i| i.as_i64().map(|n| n.to_string()).or_else(|| i.as_str().map(str::to_string)))
            .ok_or_else(|| Error::JiraUnknownPayload("tab entry has no id".into()))?;
        let body = json!({ "fieldId": field_id });
        self.post_json(
            &format!("rest/api/3/screens/{screen_id}/tabs/{tab_id}/fields"),
            &body,
        )
        .await?;
        debug!(
            target: "tracker.jira",
            field = %field_id,
            screen = %screen_id,
            tab = %tab_id,
            "field bound to default screen"
        );
        Ok(())
    }
}

/// Map a [`CustomFieldKind`] to the Jira `customfield` type identifier.
pub(crate) fn jira_field_type(kind: CustomFieldKind) -> &'static str {
    match kind {
        CustomFieldKind::Number => {
            "com.atlassian.jira.plugin.system.customfieldtypes:float"
        }
        CustomFieldKind::Decimal => {
            "com.atlassian.jira.plugin.system.customfieldtypes:float"
        }
        CustomFieldKind::LongText => {
            "com.atlassian.jira.plugin.system.customfieldtypes:textarea"
        }
        CustomFieldKind::Url => "com.atlassian.jira.plugin.system.customfieldtypes:url",
    }
}

/// Map a [`CustomFieldKind`] to the Jira `searcherKey` paired with the type.
pub(crate) fn jira_searcher_key(kind: CustomFieldKind) -> &'static str {
    match kind {
        CustomFieldKind::Number | CustomFieldKind::Decimal => {
            "com.atlassian.jira.plugin.system.customfieldtypes:exactnumber"
        }
        CustomFieldKind::LongText => {
            "com.atlassian.jira.plugin.system.customfieldtypes:textsearcher"
        }
        CustomFieldKind::Url => {
            "com.atlassian.jira.plugin.system.customfieldtypes:exacttextsearcher"
        }
    }
}

/// The display name a bridge-stable key takes on in the Jira UI. The bridge
/// uses these names to look up the customfield IDs after creation, so the
/// mapping must be stable across releases.
pub(crate) fn display_name_for_key(key: &str) -> String {
    // The well-known v0.3 bridge keys (see `custom_fields::WELL_KNOWN_FIELDS`)
    // map to title-cased English. For everything else we title-case the
    // underscored key directly. The convention is documented in SPEC.md §11.6.
    match key {
        "sinfonia_attempt_count" => "Sinfonia Attempt Count".to_string(),
        "sinfonia_last_ci_failure" => "Sinfonia Last CI Failure".to_string(),
        "sinfonia_failure_category" => "Sinfonia Failure Category".to_string(),
        "sinfonia_max_attempts" => "Sinfonia Max Attempts".to_string(),
        "sinfonia_tokens_consumed" => "Sinfonia Tokens Consumed".to_string(),
        "sinfonia_cost_consumed_usd" => "Sinfonia Cost Consumed USD".to_string(),
        "sinfonia_max_cost_usd" => "Sinfonia Max Cost USD".to_string(),
        "sinfonia_budget_exhausted_at" => "Sinfonia Budget Exhausted At".to_string(),
        _ => title_case_key(key),
    }
}

fn title_case_key(key: &str) -> String {
    key.split('_')
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a Jira field's raw JSON value into a [`CustomFieldValue`].
///
/// Jira returns numeric custom fields as bare JSON numbers and text fields
/// as bare strings. Unset fields come back as JSON `null`. The bridge's
/// three-variant value type covers all three.
pub(crate) fn parse_field_value(raw: &Json) -> CustomFieldValue {
    if raw.is_null() {
        return CustomFieldValue::Null;
    }
    if let Some(n) = raw.as_f64() {
        return CustomFieldValue::Number(n);
    }
    if let Some(s) = raw.as_str() {
        return CustomFieldValue::String(s.to_string());
    }
    // Anything else (object / array) is unexpected for the custom-field
    // shapes this phase writes; stringify so the caller at least gets the
    // raw shape rather than a silent null.
    CustomFieldValue::String(raw.to_string())
}

/// Serialize a [`CustomFieldValue`] for a Jira `PUT /issue/{id}` body.
///
/// The serialization is identical to [`CustomFieldValue`]'s `Serialize`
/// impl (bare primitive / null), but written out long-form so the Jira
/// path doesn't depend on the Serde impl's stability.
pub(crate) fn serialize_field_value(v: &CustomFieldValue) -> Json {
    match v {
        CustomFieldValue::Null => Json::Null,
        CustomFieldValue::Number(n) => json!(n),
        CustomFieldValue::String(s) => Json::String(s.clone()),
    }
}

fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
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
    use super::*;
    use crate::custom_fields::{CustomFieldKind, CustomFieldValue};

    #[test]
    fn base64_basic() {
        assert_eq!(base64_encode(b"foo:bar"), "Zm9vOmJhcg==");
        assert_eq!(base64_encode(b"M"), "TQ==");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"Man"), "TWFu");
    }

    #[test]
    fn field_type_mapping_covers_all_kinds() {
        assert!(jira_field_type(CustomFieldKind::Number).ends_with(":float"));
        assert!(jira_field_type(CustomFieldKind::Decimal).ends_with(":float"));
        assert!(jira_field_type(CustomFieldKind::LongText).ends_with(":textarea"));
        assert!(jira_field_type(CustomFieldKind::Url).ends_with(":url"));
    }

    #[test]
    fn searcher_key_mapping_covers_all_kinds() {
        for kind in [
            CustomFieldKind::Number,
            CustomFieldKind::Decimal,
            CustomFieldKind::LongText,
            CustomFieldKind::Url,
        ] {
            assert!(jira_searcher_key(kind).starts_with(
                "com.atlassian.jira.plugin.system.customfieldtypes:"
            ));
        }
    }

    #[test]
    fn display_name_round_trip_for_well_known_keys() {
        assert_eq!(
            display_name_for_key("sinfonia_attempt_count"),
            "Sinfonia Attempt Count"
        );
        assert_eq!(
            display_name_for_key("sinfonia_cost_consumed_usd"),
            "Sinfonia Cost Consumed USD"
        );
        // Falls back to title-casing the underscored key.
        assert_eq!(
            display_name_for_key("user_defined_field"),
            "User Defined Field"
        );
    }

    #[test]
    fn parse_field_value_handles_each_shape() {
        assert!(matches!(
            parse_field_value(&serde_json::Value::Null),
            CustomFieldValue::Null
        ));
        assert!(matches!(
            parse_field_value(&json!(3.5)),
            CustomFieldValue::Number(n) if (n - 3.5).abs() < 1e-9
        ));
        assert!(matches!(
            parse_field_value(&json!("hello")),
            CustomFieldValue::String(s) if s == "hello"
        ));
    }

    #[test]
    fn serialize_field_value_emits_bare_primitives() {
        assert_eq!(serialize_field_value(&CustomFieldValue::Null), Json::Null);
        assert_eq!(serialize_field_value(&CustomFieldValue::Number(3.0)), json!(3.0));
        assert_eq!(
            serialize_field_value(&CustomFieldValue::text("8.23")),
            Json::String("8.23".to_string())
        );
    }

    /// Mini transition-lookup helper modeled on the in-method logic so the
    /// branchpoint is testable without a wiremock server. Mirrors the
    /// search done in `transition_issue`.
    fn pick_transition_id<'a>(transitions: &'a [Json], target: &str) -> Option<&'a str> {
        transitions
            .iter()
            .find(|t| {
                t.get("to")
                    .and_then(|to| to.get("name"))
                    .and_then(|n| n.as_str())
                    .map(|n| n.eq_ignore_ascii_case(target))
                    .unwrap_or(false)
            })
            .and_then(|t| t.get("id").and_then(|i| i.as_str()))
    }

    #[test]
    fn transition_lookup_happy_path() {
        let transitions = vec![
            json!({ "id": "11", "name": "Start", "to": { "name": "In Progress" } }),
            json!({ "id": "21", "name": "Fix Required", "to": { "name": "Needs Fixes" } }),
        ];
        assert_eq!(pick_transition_id(&transitions, "Needs Fixes"), Some("21"));
        assert_eq!(
            pick_transition_id(&transitions, "needs fixes"),
            Some("21"),
            "match should be case-insensitive"
        );
    }

    #[test]
    fn transition_lookup_no_match() {
        let transitions = vec![json!({
            "id": "11",
            "name": "Start",
            "to": { "name": "In Progress" }
        })];
        assert!(pick_transition_id(&transitions, "Blocked").is_none());
    }
}
