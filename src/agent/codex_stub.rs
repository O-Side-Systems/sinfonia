//! Placeholder for the Codex app-server backend.
//!
//! The spec (§10) is written around launching a `codex app-server` subprocess and
//! speaking the Codex protocol over stdio. This implementation focuses on direct
//! LLM API integration (OpenAI / Anthropic / Google / Ollama) and ships a stub for
//! Codex app-server mode that fails fast at session start with a clear error. Once
//! the Codex app-server protocol is wired up here, this module is where it lives.

use super::events::EventSender;
use super::{AgentSession, CodingAgent, TurnOutcome};
use crate::config::LlmConfig;
use crate::domain::Issue;
use crate::errors::{Error, Result};
use async_trait::async_trait;
use std::path::PathBuf;

pub struct CodexStubAgent {
    command: String,
}

impl CodexStubAgent {
    pub fn new(llm: &LlmConfig) -> Result<Self> {
        Ok(CodexStubAgent {
            command: llm.command.clone(),
        })
    }
}

#[async_trait]
impl CodingAgent for CodexStubAgent {
    async fn start_session(&self, _issue: &Issue, _workspace: PathBuf) -> Result<AgentSession> {
        Err(Error::CodexNotFound(format!(
            "agent.provider=codex_app_server selected but this Sinfonia build does not implement the Codex app-server protocol. command was `{}`. Use agent.provider=codex (the codex CLI) or pick a raw LLM provider (openai/anthropic/google/ollama) in WORKFLOW.md.",
            self.command
        )))
    }

    async fn run_turn(
        &self,
        _session: &mut AgentSession,
        _prompt: &str,
        _is_first_turn: bool,
        _events: &EventSender,
    ) -> Result<TurnOutcome> {
        Err(Error::CodexNotFound("codex backend not built".into()))
    }
}
