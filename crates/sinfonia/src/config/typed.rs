//! Typed view over WorkflowDefinition.config with defaults and `$VAR` resolution (spec §5.3, §6).

use crate::config::loader::WorkflowDefinition;
use crate::errors::{Error, Result};
use serde_json::Value as Json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// As of v0.3 the tracker configuration types live in the shared
// `sinfonia-tracker` crate so the bridge can consume them. Re-exported here
// so existing `crate::config::TrackerKind` and `crate::config::TrackerConfig`
// call sites in this codebase keep compiling without per-file rewrites.
pub use sinfonia_tracker::{TrackerConfig, TrackerKind};

#[derive(Debug, Clone)]
pub struct PollingConfig {
    pub interval_ms: u64,
}

#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct HooksConfig {
    pub after_create: Option<String>,
    pub before_run: Option<String>,
    pub after_run: Option<String>,
    pub before_remove: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub max_concurrent_agents: u32,
    pub max_turns: u32,
    pub max_retry_backoff_ms: u64,
    pub max_concurrent_agents_by_state: HashMap<String, u32>,
    /// Environment policy applied to agent subprocesses (Proposal 0004 §4.1).
    pub env_policy: EnvPolicy,
}

/// How agent subprocesses (`shell` tool + CLI backends) inherit the daemon's
/// environment (Proposal 0004 §4.1). Default `Inherit` = today's behavior, so
/// this is default-safe; `Scrubbed` is the opt-in hardening that stops the
/// agent's `shell` from reading arbitrary daemon secrets via `env`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EnvMode {
    /// Inherit the full daemon environment (legacy behavior).
    #[default]
    Inherit,
    /// Start from a minimal base (PATH/HOME/LANG/…) plus an explicit
    /// passthrough allowlist; everything else is cleared.
    Scrubbed,
}

/// Parsed from `agent.env_policy`. `passthrough` and `forward` are merged into a
/// single allowlist of variable names copied from the daemon environment when
/// `mode: scrubbed`; the distinction in the proposal is documentation-only.
#[derive(Debug, Clone, Default)]
pub struct EnvPolicy {
    pub mode: EnvMode,
    /// Extra variable names to copy through in `Scrubbed` mode (union of the
    /// `passthrough` and `forward` config lists).
    pub passthrough: Vec<String>,
}

/// Dispatch eligibility allowlist (Proposal 0004 §4.3). An entry-boundary gate
/// that mirrors the CODEOWNERS exit gate: a ticket only reaches the agent when
/// it satisfies the allowlist, so an externally-filed issue cannot auto-drive
/// the agent. Empty = no filter (today's behavior), so this is default-safe.
///
/// Parsed from `agent.dispatch_allowlist` in `WORKFLOW.md`. Label matching is
/// case-insensitive (issue labels are already normalized to lowercase, §11.3).
///
/// `allowed_authors` is intentionally NOT implemented yet: the normalized
/// `Issue` model carries no author field, so gating on it needs a tracker-fetch
/// change first. Documented as a follow-up rather than half-wired.
#[derive(Debug, Clone, Default)]
pub struct DispatchAllowlist {
    /// When non-empty, the issue MUST carry at least one of these labels
    /// (compared case-insensitively) to be dispatch-eligible.
    pub require_labels: Vec<String>,
}

impl DispatchAllowlist {
    /// True when `labels` satisfies the allowlist. Empty allowlist ⇒ always true.
    pub fn permits(&self, labels: &[String]) -> bool {
        if self.require_labels.is_empty() {
            return true;
        }
        self.require_labels.iter().any(|req| {
            labels.iter().any(|l| l.eq_ignore_ascii_case(req))
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProvider {
    /// Direct LLM API → built-in tool loop.
    OpenAi,
    /// Direct LLM API → built-in tool loop.
    Anthropic,
    /// Direct LLM API → built-in tool loop.
    Google,
    /// Direct LLM API → built-in tool loop (locally hosted via Ollama).
    Ollama,
    /// Drives Anthropic's `claude` CLI as a subprocess in the workspace.
    ClaudeCode,
    /// Drives OpenAI's `codex` CLI as a subprocess in the workspace.
    Codex,
    /// Drives the `opencode` CLI (https://opencode.ai) as a subprocess in the
    /// workspace. OpenCode brings LSP integration, MCP tool support, and 75+
    /// provider backends (including local Ollama with LSP); auth is owned by
    /// the OpenCode CLI itself, configured via `opencode auth login` — Sinfonia
    /// does not pass an api_key. See `agent/opencode.rs`.
    OpenCode,
    /// Original spec backend (Codex app-server stdio protocol). Stubbed.
    CodexAppServer,
}

impl AgentProvider {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(AgentProvider::OpenAi),
            "anthropic" => Ok(AgentProvider::Anthropic),
            "google" | "gemini" => Ok(AgentProvider::Google),
            "ollama" | "local" => Ok(AgentProvider::Ollama),
            "claude_code" | "claude-code" | "claudecode" | "claude" => {
                Ok(AgentProvider::ClaudeCode)
            }
            "codex" | "codex_cli" | "codex-cli" => Ok(AgentProvider::Codex),
            "opencode" => Ok(Self::OpenCode),
            "codex_app_server" | "codex-app-server" => Ok(AgentProvider::CodexAppServer),
            other => Err(Error::ConfigInvalid(format!(
                "unknown agent.provider: {other}"
            ))),
        }
    }

    /// True for backends that talk a raw LLM API and use the built-in tool loop.
    pub fn is_raw_llm(&self) -> bool {
        matches!(
            self,
            AgentProvider::OpenAi
                | AgentProvider::Anthropic
                | AgentProvider::Google
                | AgentProvider::Ollama
        )
    }

    /// True for backends that delegate to an external coding-agent CLI.
    pub fn is_cli(&self) -> bool {
        matches!(
            self,
            AgentProvider::ClaudeCode | AgentProvider::Codex | AgentProvider::OpenCode
        )
    }
}

/// LLM/agent backend config. Replaces / extends the spec's `codex` namespace so this
/// implementation can target OpenAI, Anthropic, Google, and locally hosted models in
/// addition to the original Codex app-server protocol.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: AgentProvider,
    /// Model name passed to the upstream API (e.g. `gpt-4o-mini`, `claude-sonnet-4-6`,
    /// `gemini-1.5-pro`, `llama3.1`). Ignored by the CodexAppServer backend.
    pub model: String,
    /// HTTP endpoint for the LLM (provider default if `None`).
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    /// Original `codex.command` for the CodexAppServer backend.
    pub command: String,
    pub turn_timeout_ms: u64,
    pub read_timeout_ms: u64,
    /// 0 (or negative) disables stall detection (§5.3.6).
    pub stall_timeout_ms: i64,
    pub approval_policy: Option<String>,
    pub thread_sandbox: Option<String>,
    pub turn_sandbox_policy: Option<String>,
    pub temperature: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
    pub port: Option<u16>,
    pub bind: String, // default "127.0.0.1"
}

/// Telemetry section (plan §3.1). All fields are optional; when nothing is
/// configured the OTel layer stays disabled and behavior matches today's
/// stdout-only logging. The bridge has a parallel `BridgeTelemetrySection`
/// in `sinfonia-bridge`; the two share the same field semantics but live in
/// separate types because the bridge has extra fields (cost_table_path,
/// sinfonia event callback URLs) that don't apply on the daemon side.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// OTLP exporter endpoint. When unset, the OTel SDK falls back to the
    /// standard `OTEL_EXPORTER_OTLP_ENDPOINT` env var; if both are unset the
    /// OTel layer is disabled and the binary runs stdout-only.
    pub otlp_endpoint: Option<String>,

    /// `service.name` attribute on the OTel Resource. Defaults to `"sinfonia"`.
    pub service_name: String,

    /// Resolved tenant id. See `telemetry::tenant` for the precedence chain.
    pub tenant_id: crate::telemetry::TenantId,

    /// Extra HTTP / gRPC headers forwarded to the OTLP endpoint (Honeycomb,
    /// Datadog API keys, etc.). Set by writing
    /// `OTEL_EXPORTER_OTLP_HEADERS=k=v,...` before exporter init.
    pub headers: HashMap<String, String>,

    /// Shared HMAC secret for the typed Sinfonia↔bridge event channel
    /// (plan §7.2). Both BRIDGE.md and WORKFLOW.md must carry the same
    /// value, set either as a literal or `$ENV_VAR`. When the daemon has
    /// any subscribers registered AND this is unset, every outbound POST
    /// would be unsigned — startup validation flags that as a config error.
    pub sinfonia_events_secret: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: None,
            service_name: "sinfonia".to_string(),
            tenant_id: crate::telemetry::TenantId::resolve(None),
            headers: HashMap::new(),
            sinfonia_events_secret: None,
        }
    }
}

/// Per-tracker-state runner overrides. Every field is optional; absent fields
/// fall back to the global `llm` / workflow body.
#[derive(Debug, Clone, Default)]
pub struct StateOverride {
    pub provider: Option<AgentProvider>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub command: Option<String>,
    pub temperature: Option<f64>,
    pub turn_timeout_ms: Option<u64>,
    /// Overrides the workflow body when this state's run renders its prompt.
    pub prompt_template: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub tracker: TrackerConfig,
    pub polling: PollingConfig,
    pub workspace: WorkspaceConfig,
    pub hooks: HooksConfig,
    pub agent: AgentConfig,
    /// Dispatch eligibility allowlist (Proposal 0004 §4.3). Empty = no filter.
    pub dispatch_allowlist: DispatchAllowlist,
    pub llm: LlmConfig,
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    /// Per-tracker-state overrides. Keys are state names normalized to lowercase.
    pub states: HashMap<String, StateOverride>,
    /// Path to the source WORKFLOW.md (used to resolve relative paths). §6.1.
    pub workflow_path: PathBuf,
}

impl ServiceConfig {
    /// Build typed config from a parsed WorkflowDefinition. §6.1.
    pub fn from_workflow(def: &WorkflowDefinition) -> Result<Self> {
        let workflow_dir = def
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let tracker = parse_tracker(&def.config)?;
        let polling = parse_polling(&def.config);
        let workspace = parse_workspace(&def.config, &workflow_dir);
        let hooks = parse_hooks(&def.config)?;
        let agent = parse_agent(&def.config)?;
        let dispatch_allowlist = parse_dispatch_allowlist(&def.config);
        let llm = parse_llm(&def.config, &tracker.kind)?;
        let server = parse_server(&def.config)?;
        let telemetry = parse_telemetry(&def.config)?;
        let states = parse_states(&def.config)?;

        Ok(ServiceConfig {
            tracker,
            polling,
            workspace,
            hooks,
            agent,
            dispatch_allowlist,
            llm,
            server,
            telemetry,
            states,
            workflow_path: def.path.clone(),
        })
    }

    /// Resolve the effective LLM config for a tracker state. §6.4 + state-machine extension.
    /// Returns a clone of `self.llm` with any per-state fields applied.
    ///
    /// When the override switches the provider (e.g. global `anthropic` → state
    /// `claude_code`), provider-shaped defaults are also re-applied so command,
    /// api_key, and model line up with the new provider rather than inheriting
    /// values that only made sense for the previous one.
    pub fn effective_llm_for_state(&self, state: &str) -> LlmConfig {
        let key = state.to_lowercase();
        let mut llm = self.llm.clone();
        let Some(o) = self.states.get(&key) else {
            return llm;
        };

        let provider_changed = matches!(&o.provider, Some(p) if *p != llm.provider);
        if let Some(p) = o.provider.clone() {
            llm.provider = p;
        }
        if provider_changed {
            // Re-pick provider defaults that the global config can't have known.
            let default_cmd = default_command(&llm.provider);
            if !default_cmd.is_empty() {
                llm.command = default_cmd.to_string();
            }
            if llm.api_key.is_none() {
                llm.api_key = default_llm_env(&llm.provider);
            }
        }
        if let Some(m) = o.model.clone() {
            llm.model = m;
        }
        if let Some(e) = o.endpoint.clone() {
            llm.endpoint = Some(e);
        }
        if let Some(k) = o.api_key.clone() {
            llm.api_key = Some(k);
        }
        if let Some(c) = o.command.clone() {
            llm.command = c;
        }
        if let Some(t) = o.temperature {
            llm.temperature = Some(t);
        }
        if let Some(t) = o.turn_timeout_ms {
            llm.turn_timeout_ms = t;
        }
        llm
    }

    /// Resolve the prompt template body to use for a state's run. Falls back to
    /// the workflow body when no per-state override is set.
    pub fn effective_prompt_template<'a>(
        &'a self,
        state: &str,
        workflow_body: &'a str,
    ) -> &'a str {
        let key = state.to_lowercase();
        self.states
            .get(&key)
            .and_then(|o| o.prompt_template.as_deref())
            .unwrap_or(workflow_body)
    }

    /// Preflight validation (§6.3).
    pub fn validate_for_dispatch(&self) -> Result<()> {
        if self.tracker.api_key.as_deref().unwrap_or("").is_empty() {
            return Err(sinfonia_tracker::Error::MissingTrackerApiKey.into());
        }
        if self.tracker.project_slug.as_deref().unwrap_or("").is_empty() {
            return Err(sinfonia_tracker::Error::MissingTrackerProjectSlug.into());
        }
        // CLI-based providers need a non-empty command line.
        if matches!(
            self.llm.provider,
            AgentProvider::Codex
                | AgentProvider::ClaudeCode
                | AgentProvider::OpenCode
                | AgentProvider::CodexAppServer
        ) && self.llm.command.trim().is_empty()
        {
            return Err(Error::ConfigInvalid(format!(
                "{:?}: agent.command must be set",
                self.llm.provider
            )));
        }
        // Sanity-check every state's effective LLM resolves.
        for state_name in self.tracker.active_states.iter() {
            let eff = self.effective_llm_for_state(state_name);
            if eff.provider.is_cli() && eff.command.trim().is_empty() {
                return Err(Error::ConfigInvalid(format!(
                    "state '{state_name}': CLI provider requires a command"
                )));
            }
        }
        Ok(())
    }
}

fn parse_tracker(config: &Json) -> Result<TrackerConfig> {
    let t = config.get("tracker").cloned().unwrap_or(Json::Null);
    let kind_str = t
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            sinfonia_tracker::Error::UnsupportedTrackerKind("(missing)".to_string())
        })?;
    let kind = TrackerKind::parse(kind_str)?;

    let default_endpoint = match kind {
        TrackerKind::Linear => "https://api.linear.app/graphql".to_string(),
        TrackerKind::Jira => "".to_string(),
    };
    let endpoint = t
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or(default_endpoint);

    let canonical_env = match kind {
        TrackerKind::Linear => "LINEAR_API_KEY",
        TrackerKind::Jira => "JIRA_API_TOKEN",
    };

    let api_key_raw = t.get("api_key").and_then(|v| v.as_str()).map(str::to_string);
    let api_key = match api_key_raw {
        Some(s) => resolve_var_string(&s),
        None => std::env::var(canonical_env).ok().filter(|s| !s.is_empty()),
    };

    let project_slug = t
        .get("project_slug")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let active_states = string_list(t.get("active_states"))
        .unwrap_or_else(|| vec!["Todo".into(), "In Progress".into()]);
    let terminal_states = string_list(t.get("terminal_states")).unwrap_or_else(|| {
        vec![
            "Closed".into(),
            "Cancelled".into(),
            "Canceled".into(),
            "Duplicate".into(),
            "Done".into(),
        ]
    });

    let jira_email = t
        .get("email")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| std::env::var("JIRA_EMAIL").ok());

    Ok(TrackerConfig {
        kind,
        endpoint,
        api_key,
        project_slug,
        active_states,
        terminal_states,
        jira_email,
    })
}

fn parse_polling(config: &Json) -> PollingConfig {
    let interval = config
        .get("polling")
        .and_then(|v| v.get("interval_ms"))
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000);
    PollingConfig {
        interval_ms: interval,
    }
}

fn parse_workspace(config: &Json, workflow_dir: &Path) -> WorkspaceConfig {
    let raw = config
        .get("workspace")
        .and_then(|v| v.get("root"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(default_workspace_root);
    let expanded = resolve_path_string(&raw);
    let path = Path::new(&expanded);
    let absolute: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workflow_dir.join(path)
    };
    let normalized = absolute.canonicalize().unwrap_or(absolute);
    WorkspaceConfig { root: normalized }
}

fn default_workspace_root() -> String {
    let mut p = std::env::temp_dir();
    p.push("sinfonia_workspaces");
    p.to_string_lossy().into_owned()
}

fn parse_hooks(config: &Json) -> Result<HooksConfig> {
    let h = config.get("hooks");
    let h_obj = match h {
        Some(v) if v.is_object() => v,
        _ => &Json::Null,
    };
    let timeout = h_obj
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(60_000);
    if timeout == 0 {
        return Err(Error::ConfigInvalid("hooks.timeout_ms must be > 0".into()));
    }
    Ok(HooksConfig {
        after_create: h_obj
            .get("after_create")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        before_run: h_obj
            .get("before_run")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        after_run: h_obj
            .get("after_run")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        before_remove: h_obj
            .get("before_remove")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        timeout_ms: timeout,
    })
}

fn parse_agent(config: &Json) -> Result<AgentConfig> {
    let a = config.get("agent");
    let a_obj = match a {
        Some(v) if v.is_object() => v,
        _ => &Json::Null,
    };
    let max_concurrent_agents = a_obj
        .get("max_concurrent_agents")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(10);
    let max_turns = a_obj
        .get("max_turns")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(20);
    if max_turns == 0 {
        return Err(Error::ConfigInvalid("agent.max_turns must be > 0".into()));
    }
    let max_retry_backoff_ms = a_obj
        .get("max_retry_backoff_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(300_000);

    let mut per_state: HashMap<String, u32> = HashMap::new();
    if let Some(map) = a_obj
        .get("max_concurrent_agents_by_state")
        .and_then(|v| v.as_object())
    {
        for (k, v) in map.iter() {
            if let Some(n) = v.as_u64() {
                if n > 0 {
                    per_state.insert(k.to_lowercase(), n as u32);
                }
            }
        }
    }

    let env_policy = parse_env_policy(a_obj);

    Ok(AgentConfig {
        max_concurrent_agents,
        max_turns,
        max_retry_backoff_ms,
        max_concurrent_agents_by_state: per_state,
        env_policy,
    })
}

fn parse_env_policy(agent: &Json) -> EnvPolicy {
    let ep = match agent.get("env_policy") {
        Some(v) if v.is_object() => v,
        _ => return EnvPolicy::default(),
    };
    let mode = match ep.get("mode").and_then(|v| v.as_str()) {
        Some(s) if s.eq_ignore_ascii_case("scrubbed") => EnvMode::Scrubbed,
        _ => EnvMode::Inherit,
    };
    let mut passthrough: Vec<String> = Vec::new();
    for key in ["passthrough", "forward"] {
        if let Some(arr) = ep.get(key).and_then(|v| v.as_array()) {
            passthrough.extend(arr.iter().filter_map(|x| x.as_str().map(str::to_string)));
        }
    }
    passthrough.sort();
    passthrough.dedup();
    EnvPolicy { mode, passthrough }
}

fn parse_dispatch_allowlist(config: &Json) -> DispatchAllowlist {
    let require_labels = config
        .get("agent")
        .and_then(|a| a.get("dispatch_allowlist"))
        .and_then(|d| d.get("require_labels"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    DispatchAllowlist { require_labels }
}

fn parse_llm(config: &Json, _tracker_kind: &TrackerKind) -> Result<LlmConfig> {
    // Look at both `agent` (extensions to spec) and the spec's `codex` key.
    let agent = config.get("agent").cloned().unwrap_or(Json::Null);
    let codex = config.get("codex").cloned().unwrap_or(Json::Null);

    let provider_str = agent
        .get("provider")
        .and_then(|v| v.as_str())
        .or_else(|| codex.get("provider").and_then(|v| v.as_str()))
        .unwrap_or("anthropic");
    let provider = AgentProvider::parse(provider_str)?;

    let default_model = match provider {
        AgentProvider::OpenAi => "gpt-4o-mini",
        AgentProvider::Anthropic => "claude-sonnet-4-6",
        AgentProvider::Google => "gemini-1.5-pro",
        AgentProvider::Ollama => "llama3.1",
        AgentProvider::ClaudeCode => "claude-sonnet-4-6",
        // OpenCode wants `provider/model` (e.g. `anthropic/claude-sonnet-4-6`);
        // leaving this empty falls through to whatever the user's local
        // `~/.config/opencode/config.json` selects.
        AgentProvider::OpenCode => "",
        AgentProvider::Codex | AgentProvider::CodexAppServer => "",
    };
    let model = agent
        .get("model")
        .and_then(|v| v.as_str())
        .or_else(|| codex.get("model").and_then(|v| v.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| default_model.to_string());

    let endpoint = agent
        .get("endpoint")
        .and_then(|v| v.as_str())
        .or_else(|| codex.get("endpoint").and_then(|v| v.as_str()))
        .map(|s| resolve_var_string(s).unwrap_or_default());

    let api_key_raw = agent
        .get("api_key")
        .and_then(|v| v.as_str())
        .or_else(|| codex.get("api_key").and_then(|v| v.as_str()));
    let api_key = match api_key_raw {
        Some(s) => resolve_var_string(s),
        None => default_llm_env(&provider),
    };

    let command = agent
        .get("command")
        .and_then(|v| v.as_str())
        .or_else(|| codex.get("command").and_then(|v| v.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| default_command(&provider).to_string());

    let turn_timeout_ms = codex
        .get("turn_timeout_ms")
        .and_then(|v| v.as_u64())
        .or_else(|| agent.get("turn_timeout_ms").and_then(|v| v.as_u64()))
        .unwrap_or(3_600_000);
    let read_timeout_ms = codex
        .get("read_timeout_ms")
        .and_then(|v| v.as_u64())
        .or_else(|| agent.get("read_timeout_ms").and_then(|v| v.as_u64()))
        .unwrap_or(5_000);
    let stall_timeout_ms = codex
        .get("stall_timeout_ms")
        .and_then(|v| v.as_i64())
        .or_else(|| agent.get("stall_timeout_ms").and_then(|v| v.as_i64()))
        .unwrap_or(300_000);

    let temperature = agent
        .get("temperature")
        .and_then(|v| v.as_f64())
        .or_else(|| codex.get("temperature").and_then(|v| v.as_f64()));

    Ok(LlmConfig {
        provider,
        model,
        endpoint: endpoint.filter(|s| !s.is_empty()),
        api_key,
        command,
        turn_timeout_ms,
        read_timeout_ms,
        stall_timeout_ms,
        approval_policy: codex
            .get("approval_policy")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        thread_sandbox: codex
            .get("thread_sandbox")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        turn_sandbox_policy: codex
            .get("turn_sandbox_policy")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        temperature,
    })
}

fn default_llm_env(provider: &AgentProvider) -> Option<String> {
    let var = match provider {
        AgentProvider::OpenAi => "OPENAI_API_KEY",
        AgentProvider::Anthropic => "ANTHROPIC_API_KEY",
        AgentProvider::Google => "GOOGLE_API_KEY",
        AgentProvider::Ollama
        | AgentProvider::ClaudeCode
        | AgentProvider::Codex
        | AgentProvider::OpenCode
        | AgentProvider::CodexAppServer => return None,
    };
    std::env::var(var).ok().filter(|s| !s.is_empty())
}

/// Default subprocess command line for CLI-backed providers. `bash -lc` invokes this
/// with the per-issue workspace as `cwd`.
pub(crate) fn default_command(provider: &AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => {
            // `stream-json` (not `json`) is required so per-event lines stream as
            // they happen; sinfonia's stream-parser in `agent/cli.rs` reads them
            // to emit incremental TurnProgress events. With plain `json`, claude
            // buffers everything into a single document at exit and the dashboard
            // sits at 0+0=0 for the whole turn.
            "claude -p --output-format stream-json --verbose --dangerously-skip-permissions"
        }
        AgentProvider::Codex => "codex exec --json",
        AgentProvider::OpenCode => {
            // `--format json` switches stdout from the TUI to one JSON event
            // per line (see opencode `packages/opencode/src/cli/cmd/run.ts`,
            // the `emit()` helper). The prompt body is piped on stdin —
            // OpenCode auto-detects non-TTY stdin and uses it as the message
            // (`resolveRunInput()`), so we don't need a `--prompt-stdin`-style
            // flag. Session resume on later turns is wired by
            // `OpenCodeAgent::build_command_line` via `--session <id>`.
            "opencode run --format json"
        }
        AgentProvider::CodexAppServer => "codex app-server",
        _ => "",
    }
}

fn parse_server(config: &Json) -> Result<ServerConfig> {
    let s = config.get("server").cloned().unwrap_or(Json::Null);
    let port = s.get("port").and_then(|v| v.as_u64()).map(|v| v as u16);
    let bind = s
        .get("bind")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();
    Ok(ServerConfig { port, bind })
}

fn parse_telemetry(config: &Json) -> Result<TelemetryConfig> {
    let t = config.get("telemetry").cloned().unwrap_or(Json::Null);

    let otlp_endpoint = t
        .get("otlp_endpoint")
        .and_then(|v| v.as_str())
        .and_then(resolve_var_string);

    let service_name = t
        .get("service_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| "sinfonia".to_string());

    let raw_tenant = t
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .and_then(resolve_var_string);
    let tenant_id = crate::telemetry::TenantId::resolve(raw_tenant.as_deref());

    let mut headers: HashMap<String, String> = HashMap::new();
    if let Some(obj) = t.get("headers").and_then(|v| v.as_object()) {
        for (k, raw) in obj {
            if let Some(value_str) = raw.as_str() {
                if let Some(resolved) = resolve_var_string(value_str) {
                    headers.insert(k.clone(), resolved);
                }
            }
        }
    }

    let sinfonia_events_secret = t
        .get("sinfonia_events_secret")
        .and_then(|v| v.as_str())
        .and_then(resolve_var_string);

    Ok(TelemetryConfig {
        otlp_endpoint,
        service_name,
        tenant_id,
        headers,
        sinfonia_events_secret,
    })
}

/// Resolve a single value that may be `$VAR_NAME`. Returns `Some(value)` if the literal
/// or the resolved env var produces a non-empty string; otherwise `None`. (§5.3.1.)
fn resolve_var_string(s: &str) -> Option<String> {
    if let Some(name) = s.strip_prefix('$') {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    } else if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// `$VAR` and `~` expansion for a path-typed value (§6.1).
fn resolve_path_string(s: &str) -> String {
    // First $VAR (only if the whole string is `$NAME` or `$NAME/...`)
    let var_resolved = if let Some(rest) = s.strip_prefix('$') {
        match rest.find('/') {
            Some(i) => {
                let var = &rest[..i];
                let tail = &rest[i..];
                match std::env::var(var) {
                    Ok(v) => format!("{v}{tail}"),
                    Err(_) => s.to_string(),
                }
            }
            None => std::env::var(rest).unwrap_or_else(|_| s.to_string()),
        }
    } else {
        s.to_string()
    };
    shellexpand::tilde(&var_resolved).into_owned()
}

fn string_list(v: Option<&Json>) -> Option<Vec<String>> {
    let arr = v?.as_array()?;
    Some(
        arr.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

fn parse_states(config: &Json) -> Result<HashMap<String, StateOverride>> {
    let mut out: HashMap<String, StateOverride> = HashMap::new();
    let Some(obj) = config.get("states").and_then(|v| v.as_object()) else {
        return Ok(out);
    };
    for (name, raw) in obj {
        if !raw.is_object() {
            return Err(Error::ConfigInvalid(format!(
                "states.{name}: expected object"
            )));
        }
        let provider = raw
            .get("provider")
            .and_then(|p| p.as_str())
            .map(|s| AgentProvider::parse(s))
            .transpose()?;
        let model = raw
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let endpoint = raw
            .get("endpoint")
            .and_then(|v| v.as_str())
            .and_then(resolve_var_string);
        let api_key = raw
            .get("api_key")
            .and_then(|v| v.as_str())
            .and_then(resolve_var_string);
        let command = raw
            .get("command")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let temperature = raw.get("temperature").and_then(|v| v.as_f64());
        let turn_timeout_ms = raw.get("turn_timeout_ms").and_then(|v| v.as_u64());
        let prompt_template = raw
            .get("prompt")
            .and_then(|v| v.as_str())
            .or_else(|| raw.get("prompt_template").and_then(|v| v.as_str()))
            .map(str::to_string);
        out.insert(
            name.to_lowercase(),
            StateOverride {
                provider,
                model,
                endpoint,
                api_key,
                command,
                temperature,
                turn_timeout_ms,
                prompt_template,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::loader::parse_workflow_str;

    #[test]
    fn defaults_apply_when_optional_missing() {
        let def = parse_workflow_str(
            "---\ntracker:\n  kind: linear\n  api_key: secret\n  project_slug: alpha\n---\nbody\n",
        )
        .unwrap();
        let cfg = ServiceConfig::from_workflow(&def).unwrap();
        assert_eq!(cfg.polling.interval_ms, 30_000);
        assert_eq!(cfg.agent.max_turns, 20);
        assert_eq!(cfg.agent.max_retry_backoff_ms, 300_000);
        assert_eq!(cfg.hooks.timeout_ms, 60_000);
        assert_eq!(cfg.tracker.kind, TrackerKind::Linear);
        assert_eq!(cfg.tracker.endpoint, "https://api.linear.app/graphql");
    }

    #[test]
    fn var_indirection_resolves() {
        std::env::set_var("MY_LINEAR_KEY", "abc");
        let def = parse_workflow_str(
            "---\ntracker:\n  kind: linear\n  api_key: $MY_LINEAR_KEY\n  project_slug: x\n---",
        )
        .unwrap();
        let cfg = ServiceConfig::from_workflow(&def).unwrap();
        assert_eq!(cfg.tracker.api_key.as_deref(), Some("abc"));
    }

    #[test]
    fn unsupported_tracker_kind_errors() {
        let def = parse_workflow_str(
            "---\ntracker:\n  kind: pivotal\n  api_key: x\n  project_slug: y\n---",
        )
        .unwrap();
        let err = ServiceConfig::from_workflow(&def).unwrap_err();
        matches!(
            err,
            Error::Tracker(sinfonia_tracker::Error::UnsupportedTrackerKind(_))
        );
    }

    #[test]
    fn env_policy_defaults_to_inherit() {
        let def = parse_workflow_str(
            "---\ntracker:\n  kind: linear\n  api_key: x\n  project_slug: y\n---\nbody",
        )
        .unwrap();
        let cfg = ServiceConfig::from_workflow(&def).unwrap();
        assert_eq!(cfg.agent.env_policy.mode, EnvMode::Inherit);
        assert!(cfg.agent.env_policy.passthrough.is_empty());
    }

    #[test]
    fn env_policy_scrubbed_merges_passthrough_and_forward() {
        let yaml = "---\ntracker:\n  kind: linear\n  api_key: x\n  project_slug: y\nagent:\n  env_policy:\n    mode: scrubbed\n    passthrough: [\"FOO\", \"BAR\"]\n    forward: [\"ANTHROPIC_API_KEY\", \"FOO\"]\n---\nbody";
        let def = parse_workflow_str(yaml).unwrap();
        let cfg = ServiceConfig::from_workflow(&def).unwrap();
        assert_eq!(cfg.agent.env_policy.mode, EnvMode::Scrubbed);
        // union + dedup, sorted
        assert_eq!(
            cfg.agent.env_policy.passthrough,
            vec!["ANTHROPIC_API_KEY", "BAR", "FOO"]
        );
    }

    #[test]
    fn per_state_concurrency_lowercased_and_filtered() {
        let def = parse_workflow_str(
            "---\ntracker:\n  kind: linear\n  api_key: x\n  project_slug: y\nagent:\n  max_concurrent_agents_by_state:\n    \"In Progress\": 3\n    Todo: 0\n    Bad: foo\n---",
        )
        .unwrap();
        let cfg = ServiceConfig::from_workflow(&def).unwrap();
        assert_eq!(cfg.agent.max_concurrent_agents_by_state.get("in progress"), Some(&3));
        assert!(cfg.agent.max_concurrent_agents_by_state.get("todo").is_none());
        assert!(cfg.agent.max_concurrent_agents_by_state.get("bad").is_none());
    }

    #[test]
    fn state_override_swaps_provider_model_and_prompt() {
        let yaml = r#"---
tracker:
  kind: linear
  api_key: x
  project_slug: y
agent:
  provider: anthropic
  model: claude-sonnet-4-6
states:
  "In Progress":
    provider: claude_code
    model: claude-opus-4-7
    prompt: |
      Resume work on {{ issue.identifier }}.
  "In Review":
    provider: anthropic
    model: claude-haiku-4-5-20251001
---
default body for {{ issue.identifier }}
"#;
        let def = parse_workflow_str(yaml).unwrap();
        let cfg = ServiceConfig::from_workflow(&def).unwrap();

        let in_progress_llm = cfg.effective_llm_for_state("In Progress");
        assert_eq!(in_progress_llm.provider, AgentProvider::ClaudeCode);
        assert_eq!(in_progress_llm.model, "claude-opus-4-7");
        // Switching to a CLI provider re-picks the provider-shaped default command.
        assert!(in_progress_llm.command.starts_with("claude -p"));

        let review_llm = cfg.effective_llm_for_state("In Review");
        assert_eq!(review_llm.provider, AgentProvider::Anthropic);
        assert_eq!(review_llm.model, "claude-haiku-4-5-20251001");

        // States with no override fall back to global llm + workflow body.
        let todo_llm = cfg.effective_llm_for_state("Todo");
        assert_eq!(todo_llm.provider, AgentProvider::Anthropic);
        assert_eq!(todo_llm.model, "claude-sonnet-4-6");

        // Prompt resolution: per-state override beats workflow body.
        let p = cfg.effective_prompt_template("In Progress", "fallback");
        assert!(p.contains("Resume work on"));
        let p2 = cfg.effective_prompt_template("Todo", "fallback");
        assert_eq!(p2, "fallback");
    }
}
