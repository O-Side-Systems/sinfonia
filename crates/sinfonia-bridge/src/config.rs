//! `BRIDGE.md` parser + schema + validation.
//!
//! Same YAML-front-matter convention as `WORKFLOW.md`. The Markdown body
//! is for human notes and is not parsed. See `BRIDGE.example.md` for a
//! fully-commented working config and `docs/SPEC.md` §11.6 for the
//! recommended-extension contract this schema implements.
//!
//! ## Why a private front-matter splitter
//!
//! The companion sinfonia crate has its own `split_front_matter` in
//! `crates/sinfonia/src/config/loader.rs`. We deliberately copy the
//! ~30-line splitter rather than introducing a shared
//! `sinfonia-frontmatter` micro-crate; if cross-crate sharing later
//! becomes worth the dependency hop, the extraction is mechanical.

use crate::{Error, Result};
use regex::Regex;
use serde_json::Value as Json;
use serde_yaml::Value as YamlValue;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Public schema
// ---------------------------------------------------------------------------

/// Top-level resolved `BRIDGE.md` config.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub tracker: TrackerSection,
    pub github: GitHubSection,
    pub feedback_loop: FeedbackLoopSection,
    pub custom_fields: CustomFieldsSection,
    pub server: ServerSection,
    pub storage: StorageSection,
    pub telemetry: TelemetrySection,
    /// Source file location, normalized to an absolute path when known.
    pub source_path: PathBuf,
}

/// Tracker subset used by the bridge (mirrors `sinfonia_tracker::TrackerConfig`
/// but typed locally so the bridge's parser remains self-contained). Built
/// into a `sinfonia_tracker::TrackerConfig` at runtime via [`TrackerSection::to_tracker_config`].
#[derive(Debug, Clone)]
pub struct TrackerSection {
    pub kind: sinfonia_tracker::TrackerKind,
    /// Linear: GraphQL endpoint. Jira: base URL of the site.
    pub endpoint: String,
    pub api_key: Option<String>,
    pub project_slug: Option<String>,
    /// `active_states` and `terminal_states` are read from BRIDGE.md when
    /// present (some validation rules need them) but the bridge mostly
    /// defers to Sinfonia's own `WORKFLOW.md` for what counts as active.
    pub active_states: Vec<String>,
    pub terminal_states: Vec<String>,
    pub jira_email: Option<String>,
}

impl TrackerSection {
    /// Convert into the tracker crate's `TrackerConfig`. Used at runtime
    /// when instantiating a `LinearTracker` or `JiraTracker`.
    pub fn to_tracker_config(&self) -> sinfonia_tracker::TrackerConfig {
        sinfonia_tracker::TrackerConfig {
            kind: self.kind,
            endpoint: self.endpoint.clone(),
            api_key: self.api_key.clone(),
            project_slug: self.project_slug.clone(),
            active_states: self.active_states.clone(),
            terminal_states: self.terminal_states.clone(),
            jira_email: self.jira_email.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitHubSection {
    /// HMAC secret used to verify inbound webhooks (P1-E).
    pub webhook_secret: Option<String>,
    /// Personal-access-token mode. Mutually exclusive with `app_id` / `private_key`.
    pub pat: Option<String>,
    /// GitHub App ID (parsed as a u64). Mutually exclusive with `pat`.
    pub app_id: Option<u64>,
    /// GitHub App private key. PEM contents or `@/path/to/key.pem`.
    pub private_key: Option<String>,
    pub manage_labels: bool,
    pub label_prefix: String,
    pub label_aliases: LabelAliases,
}

/// User-supplied overrides for the canonical six bridge labels. Each value,
/// when set, is the FULL label name verbatim — `label_prefix` is NOT
/// prepended (H-4 resolution in `01-bridge-mvp.md` §7).
#[derive(Debug, Clone, Default)]
pub struct LabelAliases {
    pub in_progress: Option<String>,
    pub awaiting_review: Option<String>,
    pub needs_fixes: Option<String>,
    pub cap_hit: Option<String>,
    pub budget_exceeded: Option<String>,
    /// Optional override for the `<prefix>:failure:<category>` family. When
    /// set, the bridge appends `:<category>` to this string instead of
    /// `<prefix>:failure:<category>`.
    pub failure_prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FeedbackLoopSection {
    pub max_attempts: u32,
    pub needs_fixes_state: String,
    pub blocked_state: String,
    /// Compiled regex to extract a tracker identifier from a PR title/body.
    pub pr_link_pattern: Regex,
    pub required_checks: Vec<String>,
    /// Token cap per ticket. Accepted in P1-D, enforced in Phase 3.
    pub max_tokens_per_ticket: Option<u64>,
    /// Cost cap per ticket in USD. Accepted in P1-D, enforced in Phase 3.
    pub max_cost_per_ticket_usd: Option<f64>,
    pub budget_exceeded_state: String,
    pub failure_comment_template: String,
    /// Resolved category list. When the user provided an explicit
    /// `failure_categories:` block, that's it; otherwise this is a single
    /// synthetic default category routing to `needs_fixes_state`.
    pub failure_categories: Vec<FailureCategory>,
    /// Harness `bridge.json` manifest ingestion (Proposal 0001). All keys
    /// are optional; an omitted block disables ingestion and the bridge
    /// behaves exactly as it does on the check-name path.
    pub harness_manifest: HarnessManifestSection,
}

/// Optional harness-feedback ingestion settings (Proposal 0001 §6).
///
/// Defaults are safe and conservative; `ingest_harness_manifest` is the
/// master switch and is `false` unless explicitly enabled.
#[derive(Debug, Clone)]
pub struct HarnessManifestSection {
    /// Master switch. When `false`, the bridge never fetches or parses a
    /// manifest and stays on today's check-name behavior.
    pub ingest: bool,
    /// Glob (a single `*` wildcard is honored) matched against run
    /// artifact names to find the bundle holding `bridge.json`.
    pub artifact_glob: String,
    /// Entry name read from inside the matched artifact zip.
    pub filename: String,
    /// Hard cap on the downloaded artifact zip size (resource-exhaustion
    /// control). Bytes.
    pub max_artifact_bytes: u64,
    /// Cap on the number of scenarios folded into the digest.
    pub max_failures_parsed: usize,
    /// Cap on the rendered `sinfonia_last_ci_failure` digest length. Bytes.
    pub max_failure_digest_bytes: usize,
}

impl Default for HarnessManifestSection {
    fn default() -> Self {
        Self {
            ingest: false,
            artifact_glob: "bridge-*".to_string(),
            filename: "bridge.json".to_string(),
            max_artifact_bytes: 5_242_880, // 5 MiB
            max_failures_parsed: 20,
            max_failure_digest_bytes: 8_192, // 8 KiB
        }
    }
}

#[derive(Debug, Clone)]
pub struct FailureCategory {
    pub name: String,
    /// `None` for the default (catch-all) category.
    pub check_pattern: Option<Regex>,
    pub target_state: String,
    pub priority: i64,
}

#[derive(Debug, Clone)]
pub struct CustomFieldsSection {
    pub attempt_count: String,
    pub last_failure_log: String,
    pub max_attempts_override: String,
    pub failure_category: String,
    // Phase 3 fields. Required in P1-D so config files survive the upgrade.
    pub tokens_consumed: String,
    pub cost_consumed_usd: String,
    pub max_cost_override_usd: String,
}

#[derive(Debug, Clone)]
pub struct ServerSection {
    pub bind: String,
    pub port: u16,
    /// Externally reachable URL of this bridge instance, e.g.
    /// `https://bridge.example.com`. Used by `sinfonia-bridge --self-test`
    /// to probe `/health` from the outside. When `None`, the reachability
    /// check `SKIP`s. Pre-parsed at config-load time so consumers can use
    /// the URL without a second parse.
    pub public_url: Option<url::Url>,
}

#[derive(Debug, Clone)]
pub struct StorageSection {
    /// Path to the SQLite DB used by P1-E for idempotency + PR↔ticket map.
    pub state_db_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct TelemetrySection {
    pub otlp_endpoint: Option<String>,
    pub service_name: String,
    /// Raw tenant id from BRIDGE.md. Resolved into a `TenantId` at
    /// telemetry-init time via `telemetry::TenantId::resolve` so the
    /// config struct stays purely declarative.
    pub tenant_id: Option<String>,
    /// Extra HTTP / gRPC headers forwarded to the OTLP endpoint
    /// (Honeycomb, Datadog API keys, etc.). Forwarded via
    /// `OTEL_EXPORTER_OTLP_HEADERS=k=v,...` at exporter init time.
    pub headers: std::collections::HashMap<String, String>,
    /// Shared HMAC secret for the typed Sinfonia↔bridge event channel
    /// (Phase 3 §7.2). Validation rule (N-1 fix): required when
    /// `sinfonia_event_subscribe_url` is set.
    pub sinfonia_events_secret: Option<String>,
    pub sinfonia_event_subscribe_url: Option<String>,
    pub sinfonia_event_callback_url: Option<String>,
}

// ---------------------------------------------------------------------------
// File entry points
// ---------------------------------------------------------------------------

/// Read a `BRIDGE.md` from disk, parse + validate.
pub fn read_bridge_file(path: &Path) -> Result<BridgeConfig> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::MissingBridgeFile(format!("{}: {}", path.display(), e)))?;
    let mut cfg = parse_bridge_str(&text)?;
    cfg.source_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Ok(cfg)
}

/// Parse + validate a `BRIDGE.md` from a string.
pub fn parse_bridge_str(text: &str) -> Result<BridgeConfig> {
    let (front_matter, _body) = split_front_matter(text);
    let config = if let Some(fm) = front_matter {
        let yaml: YamlValue =
            serde_yaml::from_str(&fm).map_err(|e| Error::BridgeParseError(e.to_string()))?;
        match &yaml {
            YamlValue::Mapping(_) => yaml_to_json(&yaml)?,
            YamlValue::Null => Json::Object(Default::default()),
            _ => return Err(Error::BridgeFrontMatterNotMap),
        }
    } else {
        return Err(Error::BridgeParseError(
            "BRIDGE.md has no YAML front matter (expected `---` fence)".into(),
        ));
    };

    let tracker = parse_tracker(&config)?;
    let github = parse_github(&config)?;
    let feedback_loop = parse_feedback_loop(&config)?;
    let custom_fields = parse_custom_fields(&config)?;
    let server = parse_server(&config)?;
    let storage = parse_storage(&config)?;
    let telemetry = parse_telemetry(&config)?;

    let cfg = BridgeConfig {
        tracker,
        github,
        feedback_loop,
        custom_fields,
        server,
        storage,
        telemetry,
        source_path: PathBuf::new(),
    };
    validate(&cfg)?;
    Ok(cfg)
}

// ---------------------------------------------------------------------------
// Section parsers
// ---------------------------------------------------------------------------

fn parse_tracker(config: &Json) -> Result<TrackerSection> {
    let t = config.get("tracker").cloned().unwrap_or(Json::Null);
    let kind_str = t
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::BridgeConfigInvalid("tracker.kind is required".into()))?;
    let kind = sinfonia_tracker::TrackerKind::parse(kind_str)?;

    let default_endpoint = match kind {
        sinfonia_tracker::TrackerKind::Linear => "https://api.linear.app/graphql".to_string(),
        sinfonia_tracker::TrackerKind::Jira => "".to_string(),
    };
    let endpoint = t
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or(default_endpoint);

    let api_key = t
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .and_then(|s| resolve_var_string(&s));
    let project_slug = t
        .get("project_slug")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .and_then(|s| resolve_var_string(&s));

    let active_states = read_str_array(&t, "active_states");
    let terminal_states = read_str_array(&t, "terminal_states");
    let jira_email = t
        .get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .and_then(|s| resolve_var_string(&s));

    Ok(TrackerSection {
        kind,
        endpoint,
        api_key,
        project_slug,
        active_states,
        terminal_states,
        jira_email,
    })
}

fn parse_github(config: &Json) -> Result<GitHubSection> {
    let g = config.get("github").cloned().unwrap_or(Json::Null);
    let webhook_secret = g
        .get("webhook_secret")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .and_then(|s| resolve_var_string(&s));
    let pat = g
        .get("pat")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .and_then(|s| resolve_var_string(&s));
    let app_id = g
        .get("app_id")
        .and_then(|v| match v {
            // Allow numeric or stringly-typed App IDs (the latter is what
            // env-var indirection produces). `resolve_var_string` returns
            // `None` on empty, which we then treat as "App mode not used".
            Json::Number(n) => n.as_u64(),
            Json::String(s) => resolve_var_string(s).and_then(|v| v.parse::<u64>().ok()),
            _ => None,
        });
    let private_key = g
        .get("private_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .and_then(|s| resolve_var_string(&s));

    let manage_labels = g
        .get("manage_labels")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let label_prefix = g
        .get("label_prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("sinfonia")
        .to_string();

    let mut label_aliases = LabelAliases::default();
    if let Some(map) = g.get("label_aliases").and_then(|v| v.as_object()) {
        // Verbatim semantics per H-4 — values are full label names; we do
        // NOT prepend the `label_prefix`.
        label_aliases.in_progress = map.get("in_progress").and_then(|v| v.as_str()).map(str::to_string);
        label_aliases.awaiting_review = map
            .get("awaiting_review")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        label_aliases.needs_fixes = map.get("needs_fixes").and_then(|v| v.as_str()).map(str::to_string);
        label_aliases.cap_hit = map.get("cap_hit").and_then(|v| v.as_str()).map(str::to_string);
        label_aliases.budget_exceeded = map
            .get("budget_exceeded")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        label_aliases.failure_prefix = map
            .get("failure_prefix")
            .and_then(|v| v.as_str())
            .map(str::to_string);
    }

    Ok(GitHubSection {
        webhook_secret,
        pat,
        app_id,
        private_key,
        manage_labels,
        label_prefix,
        label_aliases,
    })
}

fn parse_feedback_loop(config: &Json) -> Result<FeedbackLoopSection> {
    let f = config.get("feedback_loop").cloned().unwrap_or(Json::Null);

    let max_attempts = f
        .get("max_attempts")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as u32;
    let needs_fixes_state = f
        .get("needs_fixes_state")
        .and_then(|v| v.as_str())
        .unwrap_or("Needs Fixes")
        .to_string();
    let blocked_state = f
        .get("blocked_state")
        .and_then(|v| v.as_str())
        .unwrap_or("Blocked - Human Review")
        .to_string();

    // Default pattern matches "Closes ABC-123" / "Fixes lin-456" etc.
    const DEFAULT_PR_PATTERN: &str = r"(?i)(?:closes|fixes|resolves)\s+([A-Z]+-\d+|[a-z]+-\d+)";
    let pr_pattern_str = f
        .get("pr_link_pattern")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_PR_PATTERN)
        .to_string();
    let pr_link_pattern = Regex::new(&pr_pattern_str).map_err(|e| {
        Error::BridgeConfigInvalid(format!(
            "feedback_loop.pr_link_pattern is not a valid regex: {e}"
        ))
    })?;

    let required_checks = read_str_array(&f, "required_checks");

    let max_tokens_per_ticket = f
        .get("max_tokens_per_ticket")
        .and_then(|v| v.as_u64());
    let max_cost_per_ticket_usd = f
        .get("max_cost_per_ticket_usd")
        .and_then(|v| v.as_f64());
    let budget_exceeded_state = f
        .get("budget_exceeded_state")
        .and_then(|v| v.as_str())
        .unwrap_or("Blocked - Budget Cap")
        .to_string();

    let failure_comment_template = f
        .get("failure_comment_template")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_FAILURE_COMMENT)
        .to_string();

    let failure_categories = parse_failure_categories(&f, &needs_fixes_state)?;
    let harness_manifest = parse_harness_manifest(&f);

    Ok(FeedbackLoopSection {
        max_attempts,
        needs_fixes_state,
        blocked_state,
        pr_link_pattern,
        required_checks,
        max_tokens_per_ticket,
        max_cost_per_ticket_usd,
        budget_exceeded_state,
        failure_comment_template,
        failure_categories,
        harness_manifest,
    })
}

/// Parse the optional harness-manifest ingestion keys from a
/// `feedback_loop` block. Every key falls back to
/// [`HarnessManifestSection::default`] when absent, so an omitted block
/// yields a fully-defaulted (and disabled) section.
fn parse_harness_manifest(f: &Json) -> HarnessManifestSection {
    let d = HarnessManifestSection::default();
    HarnessManifestSection {
        ingest: f
            .get("ingest_harness_manifest")
            .and_then(|v| v.as_bool())
            .unwrap_or(d.ingest),
        artifact_glob: f
            .get("harness_manifest_artifact_glob")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or(d.artifact_glob),
        filename: f
            .get("harness_manifest_filename")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or(d.filename),
        max_artifact_bytes: f
            .get("max_artifact_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(d.max_artifact_bytes),
        max_failures_parsed: f
            .get("max_failures_parsed")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(d.max_failures_parsed),
        max_failure_digest_bytes: f
            .get("max_failure_digest_bytes")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(d.max_failure_digest_bytes),
    }
}

const DEFAULT_FAILURE_COMMENT: &str = "CI failed on attempt {{ attempt }} of {{ max_attempts }}.\n\nFailed checks:\n{{ failed_checks }}\n\nPlease address the failures and push to the same branch.\n";

fn parse_failure_categories(f: &Json, needs_fixes_state: &str) -> Result<Vec<FailureCategory>> {
    let arr = match f.get("failure_categories") {
        Some(Json::Array(a)) => a,
        Some(Json::Null) | None => {
            // No categories defined — synthesize a single default routing to
            // the configured needs_fixes_state. This keeps the matching code
            // path uniform whether or not the user configured categories.
            return Ok(vec![FailureCategory {
                name: "default".to_string(),
                check_pattern: None,
                target_state: needs_fixes_state.to_string(),
                priority: 0,
            }]);
        }
        Some(_) => {
            return Err(Error::BridgeConfigInvalid(
                "feedback_loop.failure_categories must be an array".into(),
            ));
        }
    };

    let mut out: Vec<FailureCategory> = Vec::with_capacity(arr.len());
    let mut saw_default = false;
    for (idx, entry) in arr.iter().enumerate() {
        let obj = entry.as_object().ok_or_else(|| {
            Error::BridgeConfigInvalid(format!(
                "feedback_loop.failure_categories[{idx}] is not an object"
            ))
        })?;
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::BridgeConfigInvalid(format!(
                    "feedback_loop.failure_categories[{idx}].name is required"
                ))
            })?
            .to_string();
        let target_state = obj
            .get("target_state")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                Error::BridgeConfigInvalid(format!(
                    "feedback_loop.failure_categories[{idx}].target_state is required"
                ))
            })?
            .to_string();
        let priority = obj.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
        let check_pattern_str = obj.get("check_pattern").and_then(|v| v.as_str());

        // The "default" category is a sentinel — it matches everything and
        // doesn't carry a `check_pattern`. Any other category MUST carry a
        // pattern.
        let is_default = name == "default";
        let check_pattern = match check_pattern_str {
            Some(s) if !s.is_empty() => Some(Regex::new(s).map_err(|e| {
                Error::BridgeConfigInvalid(format!(
                    "feedback_loop.failure_categories[{idx}].check_pattern is not a valid regex: {e}"
                ))
            })?),
            _ if is_default => None,
            _ => {
                return Err(Error::BridgeConfigInvalid(format!(
                    "feedback_loop.failure_categories[{idx}] '{name}': check_pattern is required for non-default categories"
                )));
            }
        };
        if is_default {
            saw_default = true;
        }
        out.push(FailureCategory {
            name,
            check_pattern,
            target_state,
            priority,
        });
    }

    // If the user supplied categories but no `default`, add one so the
    // matcher always has a fallback. Uses the configured needs_fixes_state.
    if !saw_default {
        out.push(FailureCategory {
            name: "default".to_string(),
            check_pattern: None,
            target_state: needs_fixes_state.to_string(),
            priority: 0,
        });
    }

    Ok(out)
}

fn parse_custom_fields(config: &Json) -> Result<CustomFieldsSection> {
    let c = config.get("custom_fields").cloned().unwrap_or(Json::Null);
    let pick = |key: &str, default: &str| -> String {
        c.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    };
    Ok(CustomFieldsSection {
        attempt_count: pick("attempt_count", "sinfonia_attempt_count"),
        last_failure_log: pick("last_failure_log", "sinfonia_last_ci_failure"),
        max_attempts_override: pick("max_attempts_override", "sinfonia_max_attempts"),
        failure_category: pick("failure_category", "sinfonia_failure_category"),
        tokens_consumed: pick("tokens_consumed", "sinfonia_tokens_consumed"),
        cost_consumed_usd: pick("cost_consumed_usd", "sinfonia_cost_consumed_usd"),
        max_cost_override_usd: pick("max_cost_override_usd", "sinfonia_max_cost_usd"),
    })
}

fn parse_server(config: &Json) -> Result<ServerSection> {
    let s = config.get("server").cloned().unwrap_or(Json::Null);
    let bind = s
        .get("bind")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0.0")
        .to_string();
    let port = s
        .get("port")
        .and_then(|v| v.as_u64())
        .map(|n| n as u16)
        .unwrap_or(8081);
    let public_url = match s.get("public_url").and_then(|v| v.as_str()) {
        // Treat an explicit empty string the same as "not configured" so
        // operators can blank the field via env-var indirection (BRIDGE.md
        // value `$BRIDGE_PUBLIC_URL` resolves to "" when the env var is
        // unset; that path should not become a hard error).
        None | Some("") => None,
        Some(raw) => {
            let resolved = resolve_var_string(raw);
            match resolved {
                None => None,
                Some(text) => Some(url::Url::parse(&text).map_err(|e| {
                    Error::BridgeConfigInvalid(format!(
                        "server.public_url: invalid URL '{text}': {e}"
                    ))
                })?),
            }
        }
    };
    Ok(ServerSection {
        bind,
        port,
        public_url,
    })
}

fn parse_storage(config: &Json) -> Result<StorageSection> {
    let s = config.get("storage").cloned().unwrap_or(Json::Null);
    let raw = s
        .get("state_db_path")
        .and_then(|v| v.as_str())
        .unwrap_or("~/.sinfonia/bridge.db")
        .to_string();
    let expanded = shellexpand::tilde(&raw).into_owned();
    Ok(StorageSection {
        state_db_path: PathBuf::from(expanded),
    })
}

fn parse_telemetry(config: &Json) -> Result<TelemetrySection> {
    let t = config.get("telemetry").cloned().unwrap_or(Json::Null);
    let resolve = |key: &str| -> Option<String> {
        t.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .and_then(|s| resolve_var_string(&s))
    };

    let mut headers: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if let Some(obj) = t.get("headers").and_then(|v| v.as_object()) {
        for (k, raw) in obj {
            if let Some(value_str) = raw.as_str() {
                if let Some(resolved) = resolve_var_string(value_str) {
                    headers.insert(k.clone(), resolved);
                }
            }
        }
    }

    Ok(TelemetrySection {
        otlp_endpoint: resolve("otlp_endpoint"),
        service_name: t
            .get("service_name")
            .and_then(|v| v.as_str())
            .unwrap_or("sinfonia-bridge")
            .to_string(),
        tenant_id: resolve("tenant_id"),
        headers,
        sinfonia_events_secret: resolve("sinfonia_events_secret"),
        sinfonia_event_subscribe_url: resolve("sinfonia_event_subscribe_url"),
        sinfonia_event_callback_url: resolve("sinfonia_event_callback_url"),
    })
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate(cfg: &BridgeConfig) -> Result<()> {
    // Rule 1: exactly one of github.pat or github.app_id is set.
    match (cfg.github.pat.is_some(), cfg.github.app_id.is_some()) {
        (true, true) => {
            return Err(Error::BridgeConfigInvalid(
                "github: must set either pat or app_id (mutually exclusive)".into(),
            ));
        }
        (false, false) => {
            return Err(Error::BridgeConfigInvalid(
                "github: must set either pat or app_id (mutually exclusive)".into(),
            ));
        }
        _ => {}
    }
    // App mode also requires a private_key.
    if cfg.github.app_id.is_some() && cfg.github.private_key.is_none() {
        return Err(Error::BridgeConfigInvalid(
            "github: app_id set but private_key missing".into(),
        ));
    }

    // Rule 2 (Phase 4): both trackers are supported. Jira has two extra
    // required fields beyond Linear's (api_key, project_slug):
    //
    //   - `tracker.endpoint` — the site base URL (e.g. https://acme.atlassian.net).
    //     Linear has a sensible default; Jira can't because every tenant
    //     gets its own subdomain.
    //   - `tracker.email` — paired with `api_key` for HTTP Basic auth on
    //     Atlassian Cloud. Self-hosted Jira (Server / Data Center) authenticates
    //     via a bare PAT in `api_key`; in that mode `email` is omitted and the
    //     adapter switches to `Authorization: Bearer …`. We can't distinguish
    //     Cloud from self-hosted from the URL alone, so the rule is "either
    //     email is set, or the operator has confirmed self-hosted intent".
    //     For Phase 4 we keep it pragmatic: warn but don't reject when email
    //     is unset (self-hosted PAT). The selftest probe catches a misconfig.
    if matches!(cfg.tracker.kind, sinfonia_tracker::TrackerKind::Jira) {
        if cfg.tracker.endpoint.trim().is_empty() {
            return Err(Error::BridgeConfigInvalid(
                "tracker.endpoint is required when kind: jira (e.g. https://acme.atlassian.net)".into(),
            ));
        }
        let endpoint_lower = cfg.tracker.endpoint.to_lowercase();
        let looks_like_cloud = endpoint_lower.contains(".atlassian.net");
        if looks_like_cloud && cfg.tracker.jira_email.is_none() {
            return Err(Error::BridgeConfigInvalid(
                "tracker.email is required for Atlassian Cloud (kind: jira); \
                 omit it only for self-hosted Jira Server / Data Center PAT auth"
                    .into(),
            ));
        }
    }

    // Rule 3: max_attempts >= 1.
    if cfg.feedback_loop.max_attempts < 1 {
        return Err(Error::BridgeConfigInvalid(
            "feedback_loop.max_attempts must be >= 1".into(),
        ));
    }

    // Rule 4: needs_fixes_state and blocked_state non-empty.
    if cfg.feedback_loop.needs_fixes_state.trim().is_empty() {
        return Err(Error::BridgeConfigInvalid(
            "feedback_loop.needs_fixes_state must be non-empty".into(),
        ));
    }
    if cfg.feedback_loop.blocked_state.trim().is_empty() {
        return Err(Error::BridgeConfigInvalid(
            "feedback_loop.blocked_state must be non-empty".into(),
        ));
    }

    // Rules 5 + 6: regex compilation. Already checked at parse time; the
    // validator just confirms the values landed (no second compile pass).
    let _ = &cfg.feedback_loop.pr_link_pattern;
    for c in &cfg.feedback_loop.failure_categories {
        let _ = &c.check_pattern;
    }

    // Rule 7: priorities are unique across failure_categories.
    let mut seen: BTreeMap<i64, String> = BTreeMap::new();
    for c in &cfg.feedback_loop.failure_categories {
        if let Some(prev) = seen.insert(c.priority, c.name.clone()) {
            // Allow the synthetic default category (priority 0) appended
            // automatically by `parse_failure_categories` when a user-defined
            // category at priority 0 already exists.
            if !(prev == "default" || c.name == "default") {
                return Err(Error::BridgeConfigInvalid(format!(
                    "feedback_loop: duplicate priority {} across categories '{}' and '{}'",
                    c.priority, prev, c.name
                )));
            }
        }
    }

    // Rule 8: custom_fields.* are non-empty.
    for (label, value) in [
        ("attempt_count", &cfg.custom_fields.attempt_count),
        ("last_failure_log", &cfg.custom_fields.last_failure_log),
        ("max_attempts_override", &cfg.custom_fields.max_attempts_override),
        ("failure_category", &cfg.custom_fields.failure_category),
        ("tokens_consumed", &cfg.custom_fields.tokens_consumed),
        ("cost_consumed_usd", &cfg.custom_fields.cost_consumed_usd),
        ("max_cost_override_usd", &cfg.custom_fields.max_cost_override_usd),
    ] {
        if value.trim().is_empty() {
            return Err(Error::BridgeConfigInvalid(format!(
                "custom_fields.{label} must be a non-empty string"
            )));
        }
    }

    // Rule 9: telemetry.sinfonia_events_secret required when
    // sinfonia_event_subscribe_url is set (N-1 from the plan-checker run).
    if cfg.telemetry.sinfonia_event_subscribe_url.is_some()
        && cfg
            .telemetry
            .sinfonia_events_secret
            .as_deref()
            .unwrap_or("")
            .is_empty()
    {
        return Err(Error::BridgeConfigInvalid(
            "telemetry.sinfonia_events_secret is required when sinfonia_event_subscribe_url is configured"
                .into(),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers (front-matter splitter + env-var resolver + small array reader)
// ---------------------------------------------------------------------------

/// Return `(Some(front_matter_yaml), body)` when the file starts with `---`
/// and the terminator is found. Otherwise `(None, whole_text)`.
fn split_front_matter(text: &str) -> (Option<String>, String) {
    let trimmed_start = text.trim_start_matches('\u{feff}');
    if !trimmed_start.starts_with("---") {
        return (None, trimmed_start.to_string());
    }
    let mut lines = trimmed_start.lines();
    let first = lines.next().unwrap_or("");
    if first.trim() != "---" {
        return (None, trimmed_start.to_string());
    }
    let mut fm = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.trim() == "---" {
            in_body = true;
            continue;
        }
        if in_body {
            body.push_str(line);
            body.push('\n');
        } else {
            fm.push_str(line);
            fm.push('\n');
        }
    }
    if !in_body {
        return (None, trimmed_start.to_string());
    }
    (Some(fm), body)
}

fn yaml_to_json(value: &YamlValue) -> Result<Json> {
    let s = serde_yaml::to_string(value).map_err(|e| Error::BridgeParseError(e.to_string()))?;
    let v: Json = serde_yaml::from_str(&s).map_err(|e| Error::BridgeParseError(e.to_string()))?;
    Ok(v)
}

/// Resolve a value that may be `$VAR_NAME`. Returns `Some(value)` if the
/// literal or the resolved env var is a non-empty string. Mirrors the
/// same helper in `sinfonia::config::typed` (deliberate copy — see module
/// docs).
fn resolve_var_string(s: &str) -> Option<String> {
    if let Some(name) = s.strip_prefix('$') {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    } else if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn read_str_array(value: &Json, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests — one per validation rule (§3 table)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // A minimum-viable, valid `BRIDGE.md` that passes every rule. Tests
    // mutate exactly the field under test against this baseline.
    fn baseline() -> &'static str {
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

    #[test]
    fn baseline_parses_cleanly() {
        let cfg = parse_bridge_str(baseline()).expect("baseline should parse");
        assert!(matches!(cfg.tracker.kind, sinfonia_tracker::TrackerKind::Linear));
        assert_eq!(cfg.feedback_loop.max_attempts, 5);
        assert_eq!(cfg.server.port, 8081);
        // The synthetic default failure_category is always added.
        assert!(cfg
            .feedback_loop
            .failure_categories
            .iter()
            .any(|c| c.name == "default"));
    }

    // -- Rule 1: github.pat XOR github.app_id ----------------------------

    #[test]
    fn rule1_both_pat_and_app_id_errors() {
        let yaml = baseline()
            .replace("  pat: ghp_xxx", "  pat: ghp_xxx\n  app_id: 12345\n  private_key: PEM");
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("either pat or app_id")),
            "expected mutually-exclusive error, got: {err:?}"
        );
    }

    #[test]
    fn rule1_neither_pat_nor_app_id_errors() {
        let yaml = baseline().replace("  pat: ghp_xxx", "");
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("either pat or app_id")));
    }

    #[test]
    fn rule1_app_mode_without_private_key_errors() {
        let yaml = baseline().replace("  pat: ghp_xxx", "  app_id: 12345");
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("private_key missing")));
    }

    // -- Rule 2 (Phase 4): Jira is supported, with extra required fields ---

    #[test]
    fn rule2_jira_cloud_with_email_is_accepted() {
        // Replace the tracker block to use Jira Cloud (atlassian.net URL).
        let yaml = baseline().replace(
            "tracker:\n  kind: linear\n  api_key: test-key\n  project_slug: my-project",
            "tracker:\n  kind: jira\n  api_key: test-key\n  project_slug: ENG\n  endpoint: https://acme.atlassian.net\n  email: a@b.com",
        );
        let cfg = parse_bridge_str(&yaml).expect("jira cloud config should parse");
        assert!(matches!(
            cfg.tracker.kind,
            sinfonia_tracker::TrackerKind::Jira
        ));
        assert_eq!(cfg.tracker.endpoint, "https://acme.atlassian.net");
        assert_eq!(cfg.tracker.jira_email.as_deref(), Some("a@b.com"));
    }

    #[test]
    fn rule2_jira_self_hosted_pat_is_accepted_without_email() {
        // Self-hosted Jira (not *.atlassian.net) uses a bare PAT in api_key;
        // email is intentionally omitted in that mode.
        let yaml = baseline().replace(
            "tracker:\n  kind: linear\n  api_key: test-key\n  project_slug: my-project",
            "tracker:\n  kind: jira\n  api_key: test-key\n  project_slug: ENG\n  endpoint: https://jira.example.com",
        );
        let cfg = parse_bridge_str(&yaml).expect("self-hosted jira should parse without email");
        assert!(cfg.tracker.jira_email.is_none());
    }

    #[test]
    fn rule2_jira_missing_endpoint_errors() {
        // Jira requires an endpoint — there's no sensible default per-tenant.
        let yaml = baseline().replace(
            "tracker:\n  kind: linear\n  api_key: test-key\n  project_slug: my-project",
            "tracker:\n  kind: jira\n  api_key: test-key\n  project_slug: ENG\n  email: a@b.com",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("endpoint")),
            "expected endpoint-required error, got: {err:?}"
        );
    }

    #[test]
    fn rule2_jira_cloud_missing_email_errors() {
        // atlassian.net URL but no email: Cloud requires Basic auth (email + token).
        let yaml = baseline().replace(
            "tracker:\n  kind: linear\n  api_key: test-key\n  project_slug: my-project",
            "tracker:\n  kind: jira\n  api_key: test-key\n  project_slug: ENG\n  endpoint: https://acme.atlassian.net",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("email is required")),
            "expected email-required error, got: {err:?}"
        );
    }

    // -- Rule 3: max_attempts >= 1 ---------------------------------------

    #[test]
    fn rule3_zero_max_attempts_errors() {
        let yaml = baseline().replace("  max_attempts: 5", "  max_attempts: 0");
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("max_attempts")));
    }

    // -- Rule 4: state names non-empty ----------------------------------

    #[test]
    fn rule4_empty_needs_fixes_state_errors() {
        let yaml = baseline().replace(r#"  needs_fixes_state: "Needs Fixes""#, r#"  needs_fixes_state: """#);
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("needs_fixes_state")));
    }

    #[test]
    fn rule4_empty_blocked_state_errors() {
        let yaml = baseline().replace(r#"  blocked_state: "Blocked - Human Review""#, r#"  blocked_state: """#);
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("blocked_state")));
    }

    // -- Rule 5: pr_link_pattern compiles --------------------------------

    #[test]
    fn rule5_invalid_pr_link_pattern_errors() {
        let yaml = baseline().replace(
            "  blocked_state: \"Blocked - Human Review\"",
            "  blocked_state: \"Blocked - Human Review\"\n  pr_link_pattern: '[unclosed'",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("pr_link_pattern")),
            "expected pr_link_pattern regex error, got: {err:?}"
        );
    }

    // -- Rule 6: each failure_categories[*].check_pattern compiles -------

    #[test]
    fn rule6_invalid_category_pattern_errors() {
        // Insert the failure_categories block inside feedback_loop, not at
        // the top level — anchoring on blocked_state ensures the indentation
        // and section ownership are correct.
        let yaml = baseline().replace(
            "  blocked_state: \"Blocked - Human Review\"",
            "  blocked_state: \"Blocked - Human Review\"\n  failure_categories:\n    - name: bad\n      check_pattern: '[unclosed'\n      target_state: X\n      priority: 1",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("check_pattern")),
            "expected category regex error, got: {err:?}"
        );
    }

    // -- Rule 7: unique priorities --------------------------------------

    #[test]
    fn rule7_duplicate_priority_errors() {
        let yaml = baseline().replace(
            "  blocked_state: \"Blocked - Human Review\"",
            "  blocked_state: \"Blocked - Human Review\"\n  failure_categories:\n    - name: lint\n      check_pattern: lint\n      target_state: A\n      priority: 10\n    - name: e2e\n      check_pattern: e2e\n      target_state: B\n      priority: 10",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("duplicate priority")),
            "expected duplicate-priority error, got: {err:?}"
        );
    }

    // -- Rule 8: non-empty custom_fields values --------------------------

    #[test]
    fn rule8_empty_custom_field_errors() {
        let yaml = baseline().replace(
            "  attempt_count: sinfonia_attempt_count",
            "  attempt_count: \"\"",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("attempt_count")));
    }

    // -- Rule 9 (N-1): events_secret required when subscribe_url is set --

    #[test]
    fn rule9_events_subscribe_without_secret_errors() {
        let yaml = baseline().replace(
            "  service_name: sinfonia-bridge",
            "  service_name: sinfonia-bridge\n  sinfonia_event_subscribe_url: http://sinfonia:8080/api/v1/events/subscribers",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("sinfonia_events_secret")),
            "expected events_secret-required error, got: {err:?}"
        );
    }

    #[test]
    fn rule9_events_subscribe_with_secret_ok() {
        let yaml = baseline().replace(
            "  service_name: sinfonia-bridge",
            "  service_name: sinfonia-bridge\n  sinfonia_event_subscribe_url: http://sinfonia:8080/api/v1/events/subscribers\n  sinfonia_events_secret: shared",
        );
        let cfg = parse_bridge_str(&yaml).expect("should parse");
        assert_eq!(cfg.telemetry.sinfonia_events_secret.as_deref(), Some("shared"));
    }

    // -- server.public_url (P1-G addition) -------------------------------

    #[test]
    fn server_public_url_absent_is_none() {
        // baseline() never sets public_url; should round-trip as None.
        let cfg = parse_bridge_str(baseline()).expect("baseline parses");
        assert!(cfg.server.public_url.is_none());
    }

    #[test]
    fn server_public_url_valid_url_parses() {
        let yaml = baseline().replace(
            "  port: 8081",
            "  port: 8081\n  public_url: https://bridge.example.com",
        );
        let cfg = parse_bridge_str(&yaml).expect("should parse");
        let url = cfg.server.public_url.as_ref().expect("public_url set");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("bridge.example.com"));
    }

    #[test]
    fn server_public_url_invalid_url_errors() {
        let yaml = baseline().replace(
            "  port: 8081",
            "  port: 8081\n  public_url: \"not a url\"",
        );
        let err = parse_bridge_str(&yaml).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("public_url")),
            "expected public_url validation error, got: {err:?}"
        );
    }

    // -- Front-matter edge cases ----------------------------------------

    #[test]
    fn missing_front_matter_errors() {
        let err = parse_bridge_str("just a body").unwrap_err();
        assert!(matches!(err, Error::BridgeParseError(_)));
    }

    // -- Harness manifest ingestion (Proposal 0001 Task 6) --------------

    #[test]
    fn harness_manifest_defaults_when_omitted() {
        // baseline() sets no harness keys; the section should be the
        // disabled, fully-defaulted one.
        let cfg = parse_bridge_str(baseline()).expect("baseline parses");
        let h = &cfg.feedback_loop.harness_manifest;
        assert!(!h.ingest, "ingestion is off by default");
        assert_eq!(h.artifact_glob, "bridge-*");
        assert_eq!(h.filename, "bridge.json");
        assert_eq!(h.max_artifact_bytes, 5_242_880);
        assert_eq!(h.max_failures_parsed, 20);
        assert_eq!(h.max_failure_digest_bytes, 8_192);
    }

    #[test]
    fn harness_manifest_explicit_block_parses() {
        let yaml = baseline().replace(
            "  blocked_state: \"Blocked - Human Review\"",
            "  blocked_state: \"Blocked - Human Review\"\n  \
             ingest_harness_manifest: true\n  \
             harness_manifest_artifact_glob: \"harness-*\"\n  \
             harness_manifest_filename: \"manifest.json\"\n  \
             max_artifact_bytes: 1048576\n  \
             max_failures_parsed: 5\n  \
             max_failure_digest_bytes: 2048",
        );
        let cfg = parse_bridge_str(&yaml).expect("should parse");
        let h = &cfg.feedback_loop.harness_manifest;
        assert!(h.ingest);
        assert_eq!(h.artifact_glob, "harness-*");
        assert_eq!(h.filename, "manifest.json");
        assert_eq!(h.max_artifact_bytes, 1_048_576);
        assert_eq!(h.max_failures_parsed, 5);
        assert_eq!(h.max_failure_digest_bytes, 2_048);
    }

    // -- Label alias verbatim semantics (H-4) ----------------------------

    #[test]
    fn label_aliases_are_verbatim() {
        let yaml = baseline().replace(
            "github:\n  webhook_secret: shh",
            "github:\n  webhook_secret: shh\n  label_prefix: \"automation\"\n  label_aliases:\n    in_progress: \"ai:working\"",
        );
        let cfg = parse_bridge_str(&yaml).expect("should parse");
        assert_eq!(cfg.github.label_prefix, "automation");
        // Per H-4: alias value is the full label name, prefix is NOT
        // prepended. labels.rs (P1-F) will consume this.
        assert_eq!(
            cfg.github.label_aliases.in_progress.as_deref(),
            Some("ai:working")
        );
    }
}
