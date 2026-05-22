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
//! Output handling streams stdout line-by-line. Claude Code with
//! `--output-format stream-json --verbose` and Codex with `--json` both emit
//! one event per line; the streaming task parses each as it arrives and emits
//! `TurnProgress` events with cumulative usage so the dashboard updates
//! mid-turn. The final `result` line is also extracted post-exit for the
//! session id and final message.

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
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
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

        // Stream stdout line-by-line so we can emit incremental token updates
        // (and capture the real session_id as soon as the CLI's `init` event
        // arrives) instead of sitting silent until the child exits.
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let events_for_stream = events.clone();
        let turn_id_for_stream = turn_id.clone();
        let thread_id_shared = Arc::new(Mutex::new(session.thread_id.clone()));
        let thread_id_for_stream = thread_id_shared.clone();
        let flavor_for_stream = self.flavor;

        let stdout_task: tokio::task::JoinHandle<String> = tokio::spawn(async move {
            let mut buf = String::new();
            let Some(pipe) = stdout_pipe else {
                return buf;
            };
            let mut reader = BufReader::new(pipe);
            let mut line = String::new();
            let mut cum_in: u64 = 0_u64;
            let mut cum_out: u64 = 0_u64;
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        buf.push_str(&line);
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
                            continue;
                        };
                        // Discover the real session id as soon as it appears
                        // (the CLI's `init` event carries it on the first line).
                        if let Some(sid) = v.get("session_id").and_then(|x| x.as_str()) {
                            if let Ok(mut t) = thread_id_for_stream.lock() {
                                if t.starts_with("pending-") || t.is_empty() {
                                    *t = sid.to_string();
                                }
                            }
                        }
                        if let Some((di, do_)) = extract_stream_usage(flavor_for_stream, &v) {
                            cum_in = cum_in.saturating_add(di);
                            cum_out = cum_out.saturating_add(do_);
                            let tid = thread_id_for_stream
                                .lock()
                                .map(|t| t.clone())
                                .unwrap_or_default();
                            events_for_stream.send(AgentEvent::TurnProgress {
                                timestamp: Utc::now(),
                                thread_id: tid,
                                turn_id: turn_id_for_stream.clone(),
                                usage: TokenUsage {
                                    input_tokens: cum_in,
                                    output_tokens: cum_out,
                                    total_tokens: cum_in.saturating_add(cum_out),
                                },
                            });
                        }
                    }
                    Err(_) => break,
                }
            }
            buf
        });

        let stderr_task: tokio::task::JoinHandle<String> = tokio::spawn(async move {
            let mut buf = String::new();
            if let Some(mut s) = stderr_pipe {
                let _ = s.read_to_string(&mut buf).await;
            }
            buf
        });

        let wait = async {
            let status = child
                .wait()
                .await
                .map_err(|e| Error::PortExit(format!("wait: {e}")))?;
            let so = stdout_task.await.unwrap_or_default();
            let se = stderr_task.await.unwrap_or_default();
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
        } else if let Ok(t) = thread_id_shared.lock() {
            // Streaming saw the real session_id even if the terminal parser missed it
            // (e.g. partial JSON on the last line).
            if !t.starts_with("pending-") && session.thread_id.starts_with("pending-") {
                session.thread_id = t.clone();
            }
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
            usage: parsed.usage.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Default)]
struct ParsedCliOutput {
    final_message: String,
    session_id: Option<String>,
    usage: Option<TokenUsage>,
}

/// Best-effort parse of a CLI subprocess's full stdout buffer. Handles both
/// single-document JSON (Codex `--json`, older Claude Code `--output-format
/// json`) and newline-delimited stream-json (current Claude Code default).
/// Raw text is treated as the final message verbatim.
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

/// Extract per-message (input, output) tokens from a single stream-json line.
/// Returns None for lines that aren't an `assistant` event carrying usage.
///
/// Claude Code's `--output-format stream-json --verbose` emits one assistant
/// event per model call; their `usage.input_tokens`/`usage.output_tokens` are
/// per-call values, so callers should accumulate across the turn.
fn extract_stream_usage(flavor: CliFlavor, v: &Value) -> Option<(u64, u64)> {
    match flavor {
        CliFlavor::ClaudeCode => {
            if v.get("type").and_then(|x| x.as_str())? != "assistant" {
                return None;
            }
            let u = v.get("message").and_then(|m| m.get("usage"))?;
            let i = u.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let o = u.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            if i == 0 && o == 0 {
                return None;
            }
            Some((i, o))
        }
        // Codex CLI's per-event usage shape isn't reliably documented; skip
        // streaming updates for now and let the terminal parser carry it.
        CliFlavor::Codex => None,
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
    fn extracts_per_message_usage_from_assistant_stream_event() {
        let line = r#"{"type":"assistant","message":{"id":"m","role":"assistant","content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":12,"output_tokens":5,"cache_read_input_tokens":1000}},"session_id":"sess-x"}"#;
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(extract_stream_usage(CliFlavor::ClaudeCode, &v), Some((12, 5)));
    }

    #[test]
    fn stream_usage_skips_non_assistant_lines() {
        let init = r#"{"type":"system","subtype":"init","session_id":"s","model":"claude-sonnet-4-6"}"#;
        let result = r#"{"type":"result","subtype":"success","result":"done","session_id":"s","usage":{"input_tokens":99,"output_tokens":99}}"#;
        let v1: Value = serde_json::from_str(init).unwrap();
        let v2: Value = serde_json::from_str(result).unwrap();
        assert!(extract_stream_usage(CliFlavor::ClaudeCode, &v1).is_none());
        // `result` is handled by the terminal parser, not the streaming path.
        assert!(extract_stream_usage(CliFlavor::ClaudeCode, &v2).is_none());
    }

    #[test]
    fn stream_usage_skips_zero_usage_assistant_lines() {
        let line = r#"{"type":"assistant","message":{"id":"m","role":"assistant","content":[],"usage":{"input_tokens":0,"output_tokens":0}},"session_id":"s"}"#;
        let v: Value = serde_json::from_str(line).unwrap();
        assert!(extract_stream_usage(CliFlavor::ClaudeCode, &v).is_none());
    }

    #[test]
    fn shell_escape_quotes_when_needed() {
        assert_eq!(shell_escape("plain"), "plain");
        assert_eq!(shell_escape("v1.2-foo_bar/x=1"), "v1.2-foo_bar/x=1");
        assert_eq!(shell_escape("a b c"), "'a b c'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }
}
