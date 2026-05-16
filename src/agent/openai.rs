//! OpenAI Chat Completions backend.

use super::events::{AgentEvent, EventSender, TokenUsage};
use super::tools::ToolSpec;
use super::turn::{
    default_system_prompt, run_provider_turn, LlmCaller, Message, ToolCall,
};
use super::{AgentSession, CodingAgent, TurnOutcome};
use crate::config::{LlmConfig, ServiceConfig};
use crate::domain::Issue;
use crate::errors::{Error, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

pub struct OpenAiAgent {
    client: Client,
    endpoint: String,
    api_key: String,
    model: String,
    max_iterations: u32,
    turn_timeout: Duration,
    temperature: Option<f64>,
}

impl OpenAiAgent {
    pub fn new(cfg: &ServiceConfig, llm: &LlmConfig) -> Result<Self> {
        let api_key = llm.api_key.clone().ok_or_else(|| {
            Error::ConfigInvalid("openai: missing api_key (env OPENAI_API_KEY)".into())
        })?;
        let endpoint = llm
            .endpoint
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into());
        Ok(OpenAiAgent {
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
        })
    }
}

#[async_trait]
impl LlmCaller for OpenAiAgent {
    async fn call(
        &self,
        history: &[Message],
        tools: &[ToolSpec],
    ) -> Result<(Message, TokenUsage)> {
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
        let messages: Vec<Value> = history.iter().map(message_to_openai).collect();
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "tools": tools_json,
        });
        if let Some(t) = self.temperature {
            body["temperature"] = json!(t);
        }
        let resp = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::LlmApi(format!("openai request: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::LlmApi(format!(
                "openai status {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let v: Value = resp.json().await.map_err(|e| Error::LlmApi(e.to_string()))?;
        let choice = v
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| Error::LlmApi("openai: no choices".into()))?;
        let msg = choice
            .get("message")
            .ok_or_else(|| Error::LlmApi("openai: no message".into()))?;
        let content = msg
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let mut tool_calls = Vec::new();
        if let Some(arr) = msg.get("tool_calls").and_then(|x| x.as_array()) {
            for tc in arr {
                let id = tc
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let fn_block = tc.get("function").cloned().unwrap_or(Value::Null);
                let name = fn_block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args_raw = fn_block
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");
                let arguments: Value =
                    serde_json::from_str(args_raw).unwrap_or(Value::Object(Default::default()));
                tool_calls.push(ToolCall { id, name, arguments });
            }
        }
        let usage = TokenUsage {
            input_tokens: v
                .get("usage")
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: v
                .get("usage")
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_tokens: v
                .get("usage")
                .and_then(|u| u.get("total_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
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

fn message_to_openai(m: &Message) -> Value {
    match m {
        Message::System { content } => json!({ "role": "system", "content": content }),
        Message::User { content } => json!({ "role": "user", "content": content }),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            if tool_calls.is_empty() {
                json!({ "role": "assistant", "content": content })
            } else {
                let calls: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": serde_json::to_string(&tc.arguments).unwrap_or_else(|_| "{}".into())
                            }
                        })
                    })
                    .collect();
                json!({ "role": "assistant", "content": content, "tool_calls": calls })
            }
        }
        Message::Tool {
            tool_call_id,
            name,
            content,
        } => json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "name": name,
            "content": content
        }),
    }
}

#[async_trait]
impl CodingAgent for OpenAiAgent {
    async fn start_session(&self, _issue: &Issue, workspace: PathBuf) -> Result<AgentSession> {
        let thread_id = Uuid::new_v4().to_string();
        let history = vec![Message::System {
            content: default_system_prompt(),
        }];
        let session = AgentSession {
            thread_id: thread_id.clone(),
            workspace,
            history,
        };
        // (Optional) the orchestrator emits SessionStarted itself; we don't need to here.
        let _ = AgentEvent::SessionStarted {
            timestamp: Utc::now(),
            thread_id,
            codex_app_server_pid: None,
        };
        Ok(session)
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
