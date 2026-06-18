//! Coding-agent abstraction (spec §10).
//!
//! Two flavors of backend live behind the `CodingAgent` trait:
//!
//! 1. **Raw LLM** backends (`openai`, `anthropic`, `google`, `ollama`) that drive an
//!    LLM HTTP API directly and run a built-in tool loop (see `turn.rs`/`tools.rs`).
//! 2. **CLI** backends (`claude_code`, `codex`, `opencode`) that delegate the
//!    entire reasoning + tool loop to an external coding-agent CLI invoked as
//!    a subprocess in the issue workspace. `claude_code` and `codex` share
//!    `cli.rs`; `opencode` lives in `opencode.rs` as a sibling because its
//!    event-stream shape, resume flag, and auth model are different enough
//!    that a copy-and-adapt is cleaner than an abstraction.
//!
//! The workflow state machine (config `states:` block) picks one of these per
//! tracker state, so e.g. "In Progress" can run under Claude Code while "In Review"
//! runs a quick raw-Haiku pass.

pub mod anthropic;
pub mod cli;
pub mod codex_stub;
pub mod events;
pub mod google;
pub mod ollama;
pub mod openai;
pub mod opencode;
pub mod tools;
pub mod turn;

use crate::config::{AgentProvider, EnvMode, EnvPolicy, LlmConfig, ServiceConfig};
use crate::domain::Issue;
use crate::errors::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

pub use events::{AgentEvent, EventSender};

/// Apply an [`EnvPolicy`] to an agent subprocess command (Proposal 0004 §4.1).
///
/// `Inherit` (the default) is a no-op: the child inherits the daemon's full
/// environment, exactly as before. `Scrubbed` clears the environment and
/// re-adds only a minimal base plus the operator's passthrough allowlist, so
/// the agent's `shell` can no longer read arbitrary daemon secrets (e.g. tracker
/// or provider API keys) via `env`.
pub(crate) fn apply_env_policy(cmd: &mut tokio::process::Command, policy: &EnvPolicy) {
    match policy.mode {
        EnvMode::Inherit => {}
        EnvMode::Scrubbed => {
            cmd.env_clear();
            // Minimal base needed for a working shell + tool invocations.
            const BASE: &[&str] = &[
                "PATH", "HOME", "LANG", "LC_ALL", "LC_CTYPE", "TZ", "TERM", "USER", "SHELL", "TMPDIR",
            ];
            let names = BASE
                .iter()
                .map(|s| s.to_string())
                .chain(policy.passthrough.iter().cloned());
            for name in names {
                if let Ok(val) = std::env::var(&name) {
                    cmd.env(&name, val);
                }
            }
        }
    }
}

/// One turn outcome (§10.3 completion conditions). `usage` carries the
/// turn's token totals so the runner can emit them on `runner.turn` and
/// aggregate them on `runner.session` (plan §4) without re-parsing the
/// event channel. Every backend already computes this — the field
/// surfaces what was previously discarded after going into
/// `AgentEvent::TurnCompleted`.
#[derive(Debug, Clone)]
pub enum TurnOutcome {
    Completed {
        final_message: String,
        usage: events::TokenUsage,
    },
    Failed(String),
    Timeout,
    InputRequired,
}

/// Live session bound to a single coding-agent run. §10.2 session identifiers.
pub struct AgentSession {
    pub thread_id: String,
    pub workspace: PathBuf,
    /// Conversation history shared across continuation turns inside one worker run.
    pub history: Vec<turn::Message>,
    /// Environment policy applied to subprocesses spawned during this session
    /// (the `shell` tool and CLI backends). Resolved from `agent.env_policy`.
    pub env_policy: EnvPolicy,
}

#[async_trait]
pub trait CodingAgent: Send + Sync {
    /// Start a fresh thread for `issue` inside `workspace`.
    async fn start_session(
        &self,
        issue: &Issue,
        workspace: PathBuf,
    ) -> Result<AgentSession>;

    /// Run a single turn. The agent must drive its own internal tool loop and
    /// emit events through `events` as it goes.
    async fn run_turn(
        &self,
        session: &mut AgentSession,
        prompt: &str,
        is_first_turn: bool,
        events: &EventSender,
    ) -> Result<TurnOutcome>;

    /// Tear down the session (best effort).
    async fn stop_session(&self, _session: AgentSession) -> Result<()> {
        Ok(())
    }
}

/// Build an agent for the supplied LLM config. Used by the per-attempt runner so
/// each issue can be handled by the right backend for its current tracker state.
pub fn build_for(cfg: &ServiceConfig, llm: &LlmConfig) -> Result<Arc<dyn CodingAgent>> {
    Ok(match llm.provider {
        AgentProvider::OpenAi => Arc::new(openai::OpenAiAgent::new(cfg, llm)?),
        AgentProvider::Anthropic => Arc::new(anthropic::AnthropicAgent::new(cfg, llm)?),
        AgentProvider::Google => Arc::new(google::GoogleAgent::new(cfg, llm)?),
        AgentProvider::Ollama => Arc::new(ollama::OllamaAgent::new(cfg, llm)?),
        AgentProvider::ClaudeCode | AgentProvider::Codex => Arc::new(cli::build_for(cfg, llm)?),
        AgentProvider::OpenCode => Arc::new(opencode::OpenCodeAgent::new(cfg, llm)?),
        AgentProvider::CodexAppServer => Arc::new(codex_stub::CodexStubAgent::new(llm)?),
    })
}

/// Build an agent using the global default LLM (`cfg.llm`). Equivalent to
/// `build_for(cfg, &cfg.llm)`.
pub fn build_from_config(cfg: &ServiceConfig) -> Result<Arc<dyn CodingAgent>> {
    build_for(cfg, &cfg.llm)
}

/// Convenience: open an unbounded MPSC channel for agent events.
pub fn event_channel() -> (EventSender, mpsc::UnboundedReceiver<AgentEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (EventSender::new(tx), rx)
}

#[cfg(test)]
mod env_policy_tests {
    use super::*;
    use std::ffi::OsStr;

    fn explicit_env_keys(cmd: &tokio::process::Command) -> Vec<String> {
        cmd.as_std()
            .get_envs()
            .filter_map(|(k, v)| v.map(|_| k.to_string_lossy().into_owned()))
            .collect()
    }

    #[test]
    fn inherit_is_a_noop() {
        let policy = EnvPolicy::default(); // Inherit
        let mut cmd = tokio::process::Command::new("bash");
        apply_env_policy(&mut cmd, &policy);
        // No explicit env mutations: the child inherits the parent environment.
        assert!(cmd.as_std().get_envs().next().is_none());
    }

    #[test]
    fn scrubbed_drops_unlisted_secret_keeps_allowlisted() {
        std::env::set_var("SINFONIA_TEST_SECRET", "shh");
        std::env::set_var("SINFONIA_TEST_ALLOWED", "ok");
        let policy = EnvPolicy {
            mode: EnvMode::Scrubbed,
            passthrough: vec!["SINFONIA_TEST_ALLOWED".to_string()],
        };
        let mut cmd = tokio::process::Command::new("bash");
        apply_env_policy(&mut cmd, &policy);
        let keys = explicit_env_keys(&cmd);
        // The allowlisted var is forwarded; the unlisted secret is not.
        assert!(keys.iter().any(|k| k == "SINFONIA_TEST_ALLOWED"));
        assert!(!keys.iter().any(|k| k == "SINFONIA_TEST_SECRET"));
        // env_clear was called, so the child won't inherit the secret either.
        assert!(!cmd
            .as_std()
            .get_envs()
            .any(|(k, _)| k == OsStr::new("SINFONIA_TEST_SECRET")));
        std::env::remove_var("SINFONIA_TEST_SECRET");
        std::env::remove_var("SINFONIA_TEST_ALLOWED");
    }
}
