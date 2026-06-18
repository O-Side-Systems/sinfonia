//! Google Gemini API (v1beta `generateContent`).

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

pub struct GoogleAgent {
    client: Client,
    endpoint: String,
    api_key: String,
    model: String,
    max_iterations: u32,
    turn_timeout: Duration,
    temperature: Option<f64>,
    env_policy: crate::config::EnvPolicy,
}

impl GoogleAgent {
    pub fn new(cfg: &ServiceConfig, llm: &LlmConfig) -> Result<Self> {
        let api_key = llm.api_key.clone().ok_or_else(|| {
            Error::ConfigInvalid("google: missing api_key (env GOOGLE_API_KEY)".into())
        })?;
        let endpoint = llm.endpoint.clone().unwrap_or_else(|| {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                llm.model
            )
        });
        Ok(GoogleAgent {
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
impl LlmCaller for GoogleAgent {
    async fn call(
        &self,
        history: &[Message],
        tools: &[ToolSpec],
    ) -> Result<(Message, TokenUsage)> {
        // Gemini uses `contents` (turns) + `tools.functionDeclarations` and a
        // separate `systemInstruction`. Tool calls/results are encoded as parts
        // (`functionCall`, `functionResponse`) inside the user/model turns.
        let system_text = history
            .iter()
            .filter_map(|m| match m {
                Message::System { content } => Some(content.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut contents: Vec<Value> = Vec::new();
        for m in history {
            match m {
                Message::System { .. } => {}
                Message::User { content } => contents.push(json!({
                    "role": "user",
                    "parts": [{ "text": content }],
                })),
                Message::Assistant { content, tool_calls } => {
                    let mut parts: Vec<Value> = Vec::new();
                    if !content.is_empty() {
                        parts.push(json!({ "text": content }));
                    }
                    for tc in tool_calls {
                        parts.push(json!({
                            "functionCall": {
                                "name": tc.name,
                                "args": tc.arguments,
                            }
                        }));
                    }
                    if !parts.is_empty() {
                        contents.push(json!({ "role": "model", "parts": parts }));
                    }
                }
                Message::Tool { name, content, .. } => contents.push(json!({
                    "role": "user",
                    "parts": [{
                        "functionResponse": {
                            "name": name,
                            "response": { "content": content },
                        }
                    }]
                })),
            }
        }

        let function_declarations: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();

        let mut body = json!({
            "contents": contents,
            "systemInstruction": { "parts": [{ "text": system_text }] },
            "tools": [{ "functionDeclarations": function_declarations }],
        });
        if let Some(t) = self.temperature {
            body["generationConfig"] = json!({ "temperature": t });
        }

        let url = format!("{}?key={}", self.endpoint, self.api_key);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::LlmApi(format!("google request: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::LlmApi(format!(
                "google status {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        let v: Value = resp.json().await.map_err(|e| Error::LlmApi(e.to_string()))?;

        let candidate = v
            .get("candidates")
            .and_then(|c| c.get(0))
            .ok_or_else(|| Error::LlmApi("google: no candidates".into()))?;
        let parts = candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for part in parts {
            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                text.push_str(t);
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let arguments = fc.get("args").cloned().unwrap_or(Value::Object(Default::default()));
                tool_calls.push(ToolCall {
                    id: Uuid::new_v4().to_string(),
                    name,
                    arguments,
                });
            }
        }
        let usage = TokenUsage {
            input_tokens: v
                .get("usageMetadata")
                .and_then(|u| u.get("promptTokenCount"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: v
                .get("usageMetadata")
                .and_then(|u| u.get("candidatesTokenCount"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_tokens: v
                .get("usageMetadata")
                .and_then(|u| u.get("totalTokenCount"))
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

#[async_trait]
impl CodingAgent for GoogleAgent {
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
        let _ = &self.model;
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
