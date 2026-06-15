//! Anthropic Messages API backend.

use super::events::{EventSender, TokenUsage};
use super::tools::ToolSpec;
use super::turn::{
    default_system_prompt, run_provider_turn, LlmCaller, Message, ToolCall,
};
use super::{AgentSession, CodingAgent, TurnOutcome};
use crate::config::{LlmConfig, ServiceConfig};
use crate::domain::Issue;
use crate::errors::{Error, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

pub struct AnthropicAgent {
    client: Client,
    endpoint: String,
    api_key: String,
    model: String,
    max_iterations: u32,
    turn_timeout: Duration,
    temperature: Option<f64>,
    env_policy: crate::config::EnvPolicy,
}

impl AnthropicAgent {
    pub fn new(cfg: &ServiceConfig, llm: &LlmConfig) -> Result<Self> {
        let api_key = llm.api_key.clone().ok_or_else(|| {
            Error::ConfigInvalid("anthropic: missing api_key (env ANTHROPIC_API_KEY)".into())
        })?;
        let endpoint = llm
            .endpoint
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".into());
        Ok(AnthropicAgent {
            client: Client::builder()
                .timeout(Duration::from_millis(llm.turn_timeout_ms))
                .build()
                .map_err(|e| Error::LlmApi(e.to_string()))?,
            endpoint,
            api_key,
            model: llm.model.clone(),
            max_iterations: cfg.agent.max_turns.saturating_mul(20).max(40),
            turn_timeout: Duration::from_millis(llm.turn_timeout_ms),
            temperature: llm.temperature,
            env_policy: cfg.agent.env_policy.clone(),
        })
    }
}

#[async_trait]
impl LlmCaller for AnthropicAgent {
    async fn call(
        &self,
        history: &[Message],
        tools: &[ToolSpec],
    ) -> Result<(Message, TokenUsage)> {
        // Anthropic expects `system` separate from the message list.
        let system = history
            .iter()
            .filter_map(|m| match m {
                Message::System { content } => Some(content.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let messages: Vec<Value> = collapse_to_anthropic(history);
        let tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();
        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": system,
            "messages": messages,
            "tools": tools_json,
        });
        if let Some(t) = self.temperature {
            body["temperature"] = json!(t);
        }
        let resp = self
            .client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::LlmApi(format!("anthropic request: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::LlmApi(format!(
                "anthropic status {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let v: Value = resp.json().await.map_err(|e| Error::LlmApi(e.to_string()))?;
        let content_arr = v.get("content").and_then(|c| c.as_array()).cloned().unwrap_or_default();
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for block in content_arr {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(s) = block.get("text").and_then(|s| s.as_str()) {
                        text.push_str(s);
                    }
                }
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = block
                        .get("input")
                        .cloned()
                        .unwrap_or(Value::Object(Default::default()));
                    tool_calls.push(ToolCall { id, name, arguments });
                }
                _ => {}
            }
        }
        let usage = TokenUsage {
            input_tokens: v
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: v
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_tokens: v
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                + v.get("usage")
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
        };
        Ok((
            Message::Assistant {
                content: text,
                tool_calls,
            },
            usage,
        ))
    }
}

/// Convert the provider-neutral history into Anthropic's `messages[]` shape.
/// Tool results must be embedded as a `user` message containing a `tool_result`
/// content block whose `tool_use_id` references the assistant's `tool_use`.
fn collapse_to_anthropic(history: &[Message]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut pending_tool_results: Vec<Value> = Vec::new();

    let flush_tool_results = |out: &mut Vec<Value>, pending: &mut Vec<Value>| {
        if !pending.is_empty() {
            out.push(json!({
                "role": "user",
                "content": std::mem::take(pending),
            }));
        }
    };

    for m in history {
        match m {
            Message::System { .. } => {} // handled separately
            Message::User { content } => {
                flush_tool_results(&mut out, &mut pending_tool_results);
                out.push(json!({
                    "role": "user",
                    "content": [{ "type": "text", "text": content }]
                }));
            }
            Message::Assistant {
                content,
                tool_calls,
            } => {
                flush_tool_results(&mut out, &mut pending_tool_results);
                let mut blocks: Vec<Value> = Vec::new();
                if !content.is_empty() {
                    blocks.push(json!({ "type": "text", "text": content }));
                }
                for tc in tool_calls {
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.name,
                        "input": tc.arguments,
                    }));
                }
                out.push(json!({ "role": "assistant", "content": blocks }));
            }
            Message::Tool {
                tool_call_id,
                content,
                ..
            } => {
                pending_tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": content,
                }));
            }
        }
    }
    flush_tool_results(&mut out, &mut pending_tool_results);
    out
}

#[async_trait]
impl CodingAgent for AnthropicAgent {
    async fn start_session(&self, _issue: &Issue, workspace: PathBuf) -> Result<AgentSession> {
        let thread_id = Uuid::new_v4().to_string();
        let history = vec![Message::System {
            content: default_system_prompt(),
        }];
        Ok(AgentSession {
            thread_id,
            workspace,
            history,
            env_policy: self.env_policy.clone(),
        })
    }

    async fn run_turn(
        &self,
        session: &mut AgentSession,
        prompt: &str,
        _is_first_turn: bool,
        events: &EventSender,
    ) -> Result<TurnOutcome> {
        run_provider_turn(
            self,
            session,
            prompt,
            self.max_iterations,
            self.turn_timeout,
            events,
        )
        .await
    }
}
