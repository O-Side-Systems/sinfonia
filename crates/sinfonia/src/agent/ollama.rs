//! Local LLM backend via Ollama (`POST /api/chat`).
//!
//! Ollama exposes an OpenAI-style `tool_calls` field on `messages` for models that
//! support tool use. The default endpoint is `http://127.0.0.1:11434/api/chat`.

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

pub struct OllamaAgent {
    client: Client,
    endpoint: String,
    model: String,
    max_iterations: u32,
    turn_timeout: Duration,
    temperature: Option<f64>,
    env_policy: crate::config::EnvPolicy,
}

impl OllamaAgent {
    pub fn new(cfg: &ServiceConfig, llm: &LlmConfig) -> Result<Self> {
        let endpoint = llm
            .endpoint
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:11434/api/chat".into());
        Ok(OllamaAgent {
            client: Client::builder()
                .timeout(Duration::from_millis(llm.turn_timeout_ms))
                .build()
                .map_err(|e| Error::LlmApi(e.to_string()))?,
            endpoint,
            model: llm.model.clone(),
            max_iterations: cfg.agent.max_turns.saturating_mul(20).max(40),
            turn_timeout: Duration::from_millis(llm.turn_timeout_ms),
            temperature: llm.temperature,
            env_policy: cfg.agent.env_policy.clone(),
        })
    }
}

#[async_trait]
impl LlmCaller for OllamaAgent {
    async fn call(
        &self,
        history: &[Message],
        tools: &[ToolSpec],
    ) -> Result<(Message, TokenUsage)> {
        let messages: Vec<Value> = history
            .iter()
            .map(|m| match m {
                Message::System { content } => json!({ "role": "system", "content": content }),
                Message::User { content } => json!({ "role": "user", "content": content }),
                Message::Assistant {
                    content,
                    tool_calls,
                } => {
                    let calls: Vec<Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments,
                                }
                            })
                        })
                        .collect();
                    let mut v = json!({ "role": "assistant", "content": content });
                    if !calls.is_empty() {
                        v["tool_calls"] = Value::Array(calls);
                    }
                    v
                }
                Message::Tool {
                    name, content, ..
                } => json!({
                    "role": "tool",
                    "name": name,
                    "content": content
                }),
            })
            .collect();
        let tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "tools": tools_json,
            "stream": false,
        });
        if let Some(t) = self.temperature {
            body["options"] = json!({ "temperature": t });
        }
        let resp = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::LlmApi(format!("ollama request: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::LlmApi(format!(
                "ollama status {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let v: Value = resp.json().await.map_err(|e| Error::LlmApi(e.to_string()))?;
        let msg = v
            .get("message")
            .ok_or_else(|| Error::LlmApi("ollama: missing message".into()))?;
        let content = msg
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let mut tool_calls = Vec::new();
        if let Some(arr) = msg.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in arr {
                let fn_block = tc.get("function").cloned().unwrap_or(Value::Null);
                let name = fn_block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let arguments = fn_block
                    .get("arguments")
                    .cloned()
                    .unwrap_or(Value::Object(Default::default()));
                tool_calls.push(ToolCall {
                    id: Uuid::new_v4().to_string(),
                    name,
                    arguments,
                });
            }
        }
        let usage = TokenUsage {
            input_tokens: v
                .get("prompt_eval_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: v.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0),
            total_tokens: v.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0)
                + v.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0),
        };
        Ok((
            Message::Assistant {
                content,
                tool_calls,
            },
            usage,
        ))
    }
}

#[async_trait]
impl CodingAgent for OllamaAgent {
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

