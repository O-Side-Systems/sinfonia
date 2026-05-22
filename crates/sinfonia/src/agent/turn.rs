//! Provider-neutral message + tool-call types and the shared per-turn loop that
//! every LLM backend (OpenAI, Anthropic, Google, Ollama) runs on top of.

use super::events::{AgentEvent, EventSender, TokenUsage};
use super::tools::{dispatch_tool, tool_catalog, ToolSpec};
use super::{AgentSession, TurnOutcome};
use crate::errors::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
    },
    Tool {
        tool_call_id: String,
        name: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Per-backend LLM call interface. Implementations send `history` and the standard
/// tool catalog to the upstream API and return the next assistant message (which
/// may contain tool calls) plus any reported token usage.
#[async_trait]
pub trait LlmCaller: Send + Sync {
    async fn call(
        &self,
        history: &[Message],
        tools: &[ToolSpec],
    ) -> Result<(Message, TokenUsage)>;
}

/// Run one Sinfonia "turn" of the tool-using loop. The turn ends when the LLM
/// replies without tool calls (or calls `finish`), the per-turn timeout hits, or
/// the iteration cap is reached.
pub async fn run_provider_turn<C: LlmCaller>(
    caller: &C,
    session: &mut AgentSession,
    prompt: &str,
    max_iterations: u32,
    turn_timeout: std::time::Duration,
    events: &EventSender,
) -> Result<TurnOutcome> {
    let turn_id = Uuid::new_v4().to_string();
    session.history.push(Message::User {
        content: prompt.to_string(),
    });
    events.send(AgentEvent::TurnStarted {
        timestamp: Utc::now(),
        thread_id: session.thread_id.clone(),
        turn_id: turn_id.clone(),
    });

    let tools = tool_catalog();
    let mut total_usage = TokenUsage::default();
    let started = tokio::time::Instant::now();

    for _ in 0..max_iterations.max(1) {
        if started.elapsed() > turn_timeout {
            events.send(AgentEvent::TurnFailed {
                timestamp: Utc::now(),
                thread_id: session.thread_id.clone(),
                turn_id,
                message: "turn timeout".into(),
            });
            return Ok(TurnOutcome::Timeout);
        }

        let (assistant_msg, usage) = match caller.call(&session.history, &tools).await {
            Ok(v) => v,
            Err(e) => {
                events.send(AgentEvent::TurnFailed {
                    timestamp: Utc::now(),
                    thread_id: session.thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message: e.to_string(),
                });
                return Ok(TurnOutcome::Failed(e.to_string()));
            }
        };
        accumulate_usage(&mut total_usage, &usage);
        session.history.push(assistant_msg.clone());

        let (content, tool_calls) = match &assistant_msg {
            Message::Assistant {
                content,
                tool_calls,
            } => (content.clone(), tool_calls.clone()),
            _ => unreachable!("LlmCaller must return an Assistant message"),
        };

        if tool_calls.is_empty() {
            events.send(AgentEvent::TurnCompleted {
                timestamp: Utc::now(),
                thread_id: session.thread_id.clone(),
                turn_id,
                message: content.clone(),
                usage: Some(total_usage.clone()),
            });
            return Ok(TurnOutcome::Completed {
                final_message: content,
                usage: total_usage,
            });
        }

        let mut hit_final = false;
        let mut final_message = String::new();
        for tc in tool_calls {
            let res = dispatch_tool(&tc.name, &tc.arguments, &session.workspace).await?;
            events.send(AgentEvent::Notification {
                timestamp: Utc::now(),
                message: format!("tool {}", tc.name),
            });
            if tc.name == "finish" {
                hit_final = true;
                final_message = res.output.clone();
            }
            session.history.push(Message::Tool {
                tool_call_id: tc.id.clone(),
                name: tc.name.clone(),
                content: res.output,
            });
            if res.is_final {
                hit_final = true;
            }
        }
        if hit_final {
            events.send(AgentEvent::TurnCompleted {
                timestamp: Utc::now(),
                thread_id: session.thread_id.clone(),
                turn_id,
                message: final_message.clone(),
                usage: Some(total_usage.clone()),
            });
            return Ok(TurnOutcome::Completed {
                final_message,
                usage: total_usage,
            });
        }
    }

    events.send(AgentEvent::TurnFailed {
        timestamp: Utc::now(),
        thread_id: session.thread_id.clone(),
        turn_id,
        message: "exceeded max tool-call iterations".into(),
    });
    Ok(TurnOutcome::Failed("max iterations".into()))
}

fn accumulate_usage(total: &mut TokenUsage, delta: &TokenUsage) {
    total.input_tokens = total.input_tokens.saturating_add(delta.input_tokens);
    total.output_tokens = total.output_tokens.saturating_add(delta.output_tokens);
    total.total_tokens = total.total_tokens.saturating_add(delta.total_tokens);
}

/// Default system prompt for tool-using runs. Shared across all backends.
pub fn default_system_prompt() -> String {
    r#"You are a coding agent invoked by Sinfonia, a service that orchestrates work
on an issue tracker. You are working inside an isolated per-issue workspace and have
the following tools at your disposal:

- `shell`: run a non-interactive bash command (cwd = workspace root)
- `read_file`, `write_file`, `edit_file`, `list_dir`: filesystem operations confined to the workspace
- `finish`: call this exactly once when the turn's work is complete, with a short summary

Operating principles:
- Stay inside the workspace. Never reference paths outside it.
- Prefer small, targeted edits over wholesale rewrites.
- Always run tests / formatters / type-checkers when they exist before finishing.
- Use the `finish` tool to end the turn; include a concise summary of the changes and
  the recommended next state for the ticket (e.g. "ready for Human Review")."#
        .to_string()
}
