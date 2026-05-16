//! Subprocess-driven coding-agent backends.
//!
//! Unlike the raw LLM backends in `openai.rs`/`anthropic.rs`/`google.rs`/`ollama.rs`
//! (which run their own tool loop on top of an LLM API), these agents delegate the
//! entire turn — reasoning, tool use, file edits — to an external coding-agent CLI
//! invoked as a subprocess in the issue workspace:
//!
//! - `provider: claude_code` → invokes the `claude` CLI (Anthropic's Claude Code).
//! - `provider: codex`       → invokes the `codex` CLI (OpenAI's Codex CLI).
//!
//! Each backend uses the operator-configured `agent.command` (or its provider
//! default) as the command line, executed under `bash -lc <command>` with the
//! workspace path as the working directory. The prompt is delivered on stdin so
//! arbitrary shell escaping doesn't matter.
//!
//! Output handling tries to extract a structured JSON result first (Claude Code
//! emits one with `--output-format json`, Codex with `--json`) and falls back to
//! treating stdout as the final assistant message. Token usage and session id
//! are extracted opportunistically from whatever shape we recognize.

use super::events::{AgentEvent, EventSender, TokenUsage};
use super::turn::Message;
use super::{AgentSession, CodingAgent, TurnOutcome};
use crate::config::{AgentProvider, LlmConfig};
use crate::domain::Issue;
use crate::errors::{Error, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

/// Which CLI flavor we're driving. Different output parsers and resume flags.
#[derive(Debug, Clone, Copy)]
pub enum CliFlavor {
    ClaudeCode,
    Codex,
}

pub struct CliAgent {
    flavor: CliFlavor,
    command: String,
    model: Option<String>,
    turn_timeout: Duration,
}

impl CliAgent {
    pub fn new(flavor: CliFlavor, llm: &LlmConfig) -> Result<Self> {
        if llm.command.trim().is_empty() {
            return Err(Error::ConfigInvalid(format!(
                "{:?}: agent.command must be non-empty",
                flavor
            )));
        }
        let model = if llm.model.trim().is_empty() {
            None
        } else {
            Some(llm.model.clone())
        };
        Ok(CliAgent {
            flavor,
            command: llm.command.clone(),
            model,
            turn_timeout: Duration::from_millis(llm.turn_timeout_ms),
        })
    }

    /// Build the full bash-lc command for one turn.
    fn build_command_line(&self, prior_session_id: Option<&str>) -> String {
        let mut cmd = self.command.clone();
        if let Some(model) = &self.model {
            cmd.push_str(" --model ");
            cmd.push_str(&shell_escape(model));
        }
        if let Some(sid) = prior_session_id {
            match self.flavor {
                CliFlavor::ClaudeCode => {
                    cmd.push_str(" --resume ");
                    cmd.push_str(&shell_escape(sid));
                }
                CliFlavor::Codex => {
                    // Codex CLI threading is best-effort: not all builds support it.
                    cmd.push_str(" --thread ");
                    cmd.push_str(&shell_escape(sid));
                }
            }
        }
        cmd
    }
}

#[async_trait]
impl CodingAgent for CliAgent {
    async fn start_session(&self, _issue: &Issue, workspace: PathBuf) -> Result<AgentSession> {
        // Real session/thread id is discovered after the first turn's CLI output.
        // Until then, use a placeholder UUID so observability has something to log.
        Ok(AgentSession {
            thread_id: format!("pending-{}", Uuid::new_v4()),
            workspace,
            history: Vec::new(),
        })
    }

    async fn run_turn(
        &self,
        session: &mut AgentSession,
        prompt: &str,
        is_first_turn: bool,
        events: &EventSender,
    ) -> Result<TurnOutcome> {
        let turn_id = Uuid::new_v4().to_string();
        events.send(AgentEvent::TurnStarted {
            timestamp: Utc::now(),
            thread_id: session.thread_id.clone(),
            turn_id: turn_id.clone(),
        });

        let prior_session = if is_first_turn || session.thread_id.starts_with("pending-") {
            None
        } else {
            Some(session.thread_id.as_str())
        };
        let cmd_line = self.build_command_line(prior_session);

        let mut child = Command::new("bash")
            .arg("-lc")
            .arg(&cmd_line)
            .current_dir(&session.workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::CodexNotFound(format!("spawn '{cmd_line}': {e}")))?;

        // Feed the prompt via stdin.
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(prompt.as_bytes()).await {
                events.send(AgentEvent::TurnFailed {
                    timestamp: Utc::now(),
                    thread_id: session.thread_id.clone(),
                    turn_id: turn_id.clone(),
                    message: format!("stdin write: {e}"),
                });
                return Ok(TurnOutcome::Failed(format!("stdin write: {e}")));
            }
            let _ = stdin.shutdown().await;
        }

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let wait = async {
            let status = child
                .wait()
                .await
                .map_err(|e| Error::PortExit(format!("wait: {e}")))?;
            let mut so = String::new();
            if let Some(s) = stdout_handle.as_mut() {
                let _ = s.read_to_string(&mut so).await;
            }
            let mut se = String::new();
            if let Some(s) = stderr_handle.as_mut() {
                let _ = s.read_to_string(&mut se).await;
            }
            Ok::<_, Error>((status, so, se))
        };

        let (status, stdout_buf, stderr_buf) = match timeout(self.turn_timeout, wait).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                events.send(AgentEvent::TurnFailed {
                    timestamp: Utc::now(),
                    thread_id: session.thread_id.clone(),
                    turn_id,
                    message: e.to_string(),
                });
                return Ok(TurnOutcome::Failed(e.to_string()));
            }
            Err(_) => {
                let _ = child.start_kill();
                events.send(AgentEvent::TurnFailed {
                    timestamp: Utc::now(),
                    thread_id: session.thread_id.clone(),
                    turn_id,
                    message: "turn timeout".into(),
                });
                return Ok(TurnOutcome::Timeout);
            }
        };

        if !status.success() {
            let msg = format!(
                "exit {:?}; stderr={}",
                status.code(),
                truncate(&stderr_buf, 2_000)
            );
            events.send(AgentEvent::TurnFailed {
                timestamp: Utc::now(),
                thread_id: session.thread_id.clone(),
                turn_id,
                message: msg.clone(),
            });
            return Ok(TurnOutcome::Failed(msg));
        }

        let parsed = parse_cli_output(self.flavor, &stdout_buf);
        if let Some(sid) = parsed.session_id.clone() {
            session.thread_id = sid;
        }
        // Track the prompt + assistant text in history so a future agent that does
        // its own continuation logic can see it. Tool calls happened inside the
        // CLI subprocess; we only see the final assistant message.
        session.history.push(Message::User {
            content: prompt.to_string(),
        });
        session.history.push(Message::Assistant {
            content: parsed.final_message.clone(),
            tool_calls: vec![],
        });

        events.send(AgentEvent::TurnCompleted {
            timestamp: Utc::now(),
            thread_id: session.thread_id.clone(),
            turn_id,
            message: truncate(&parsed.final_message, 4_000),
            usage: parsed.usage.clone(),
        });
        Ok(TurnOutcome::Completed {
            final_message: parsed.final_message,
        })
    }
}

#[derive(Debug, Default)]
struct ParsedCliOutput {
    final_message: String,
    session_id: Option<String>,
    usage: Option<TokenUsage>,
}

/// Best-effort parse of a CLI subprocess's stdout. Both Claude Code's
/// `--output-format json` and Codex's `--json` emit a JSON document we can
/// pluck fields out of; raw text is treated as the final message verbatim.
fn parse_cli_output(flavor: CliFlavor, stdout: &str) -> ParsedCliOutput {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return ParsedCliOutput {
            final_message: "(empty CLI output)".into(),
            ..Default::default()
        };
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return parse_json(flavor, &value, trimmed);
    }
    // stream-json: multiple newline-delimited objects, last is the result.
    if let Some(last_json_line) = trimmed
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
    {
        return parse_json(flavor, &last_json_line, trimmed);
    }
    ParsedCliOutput {
        final_message: trimmed.to_string(),
        ..Default::default()
    }
}

fn parse_json(flavor: CliFlavor, v: &Value, fallback_text: &str) -> ParsedCliOutput {
    let final_message = v
        .get("result")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("output").and_then(|x| x.as_str()))
        .or_else(|| v.get("message").and_then(|x| x.as_str()))
        .map(str::to_string)
        .unwrap_or_else(|| fallback_text.to_string());

    let session_id = v
        .get("session_id")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("thread_id").and_then(|x| x.as_str()))
        .or_else(|| v.get("conversation_id").and_then(|x| x.as_str()))
        .map(str::to_string);

    let usage = extract_usage(flavor, v);

    ParsedCliOutput {
        final_message,
        session_id,
        usage,
    }
}

fn extract_usage(_flavor: CliFlavor, v: &Value) -> Option<TokenUsage> {
    let u = v.get("usage")?;
    let input = u
        .get("input_tokens")
        .and_then(|x| x.as_u64())
        .or_else(|| u.get("prompt_tokens").and_then(|x| x.as_u64()))
        .unwrap_or(0);
    let output = u
        .get("output_tokens")
        .and_then(|x| x.as_u64())
        .or_else(|| u.get("completion_tokens").and_then(|x| x.as_u64()))
        .unwrap_or(0);
    let total = u
        .get("total_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(input + output);
    if input == 0 && output == 0 && total == 0 {
        return None;
    }
    Some(TokenUsage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: total,
    })
}

/// Quote a value safely for embedding in a `bash -lc` command line.
fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.' || c == '='
    }) {
        s.to_string()
    } else {
        let escaped = s.replace('\'', "'\\''");
        format!("'{escaped}'")
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

/// Construct a CLI agent for a `claude_code` / `codex` provider value.
pub fn build_for(llm: &LlmConfig) -> Result<CliAgent> {
    let flavor = match llm.provider {
        AgentProvider::ClaudeCode => CliFlavor::ClaudeCode,
        AgentProvider::Codex => CliFlavor::Codex,
        _ => {
            return Err(Error::ConfigInvalid(format!(
                "cli::build_for called with non-CLI provider {:?}",
                llm.provider
            )))
        }
    };
    CliAgent::new(flavor, llm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_code_json_result() {
        let out = r#"{
            "type":"result",
            "subtype":"success",
            "result":"all done",
            "session_id":"sess-123",
            "is_error":false,
            "usage":{"input_tokens":1200,"output_tokens":340}
        }"#;
        let p = parse_cli_output(CliFlavor::ClaudeCode, out);
        assert_eq!(p.final_message, "all done");
        assert_eq!(p.session_id.as_deref(), Some("sess-123"));
        let u = p.usage.unwrap();
        assert_eq!(u.input_tokens, 1200);
        assert_eq!(u.output_tokens, 340);
        assert_eq!(u.total_tokens, 1540);
    }

    #[test]
    fn falls_back_to_plain_text_when_not_json() {
        let p = parse_cli_output(CliFlavor::Codex, "made the edit; tests pass");
        assert_eq!(p.final_message, "made the edit; tests pass");
        assert!(p.session_id.is_none());
        assert!(p.usage.is_none());
    }

    #[test]
    fn picks_last_json_line_from_stream() {
        let stream = "{\"type\":\"system\",\"k\":1}\n{\"type\":\"result\",\"result\":\"done\",\"session_id\":\"abc\"}\n";
        let p = parse_cli_output(CliFlavor::ClaudeCode, stream);
        assert_eq!(p.final_message, "done");
        assert_eq!(p.session_id.as_deref(), Some("abc"));
    }

    #[test]
    fn shell_escape_quotes_when_needed() {
        assert_eq!(shell_escape("plain"), "plain");
        assert_eq!(shell_escape("v1.2-foo_bar/x=1"), "v1.2-foo_bar/x=1");
        assert_eq!(shell_escape("a b c"), "'a b c'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }
}
