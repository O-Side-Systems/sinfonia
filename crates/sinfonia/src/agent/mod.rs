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

use crate::config::{AgentProvider, LlmConfig, ServiceConfig};
use crate::domain::Issue;
use crate::errors::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

pub use events::{AgentEvent, EventSender};

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
        AgentProvider::ClaudeCode | AgentProvider::Codex => Arc::new(cli::build_for(llm)?),
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
