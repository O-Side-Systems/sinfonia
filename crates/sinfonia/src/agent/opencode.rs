//! OpenCode subprocess backend.
//!
//! Adds `provider: opencode` as a sibling of the `claude_code` / `codex`
//! backends in `cli.rs`. OpenCode (<https://opencode.ai>) is a coding-agent
//! CLI with LSP integration, MCP tool support, and 75+ provider backends
//! (OpenAI / Anthropic / Google / Ollama / …). It's the natural middle tier
//! between Sinfonia's raw-LLM backends and the Claude Code / Codex products:
//! a team that wants local-model coding sessions (Ollama-backed) gets
//! language-aware tooling for free by routing through OpenCode instead of
//! through Sinfonia's raw `ollama` backend.
//!
//! ## Surface area
//!
//! - Command (default): `opencode run --format json`. Operators can override
//!   `agent.command` to point at a different binary path or add flags.
//! - Prompt: piped on stdin. OpenCode auto-detects non-TTY stdin and joins it
//!   into the message (`resolveRunInput()` in the upstream `run.ts`), so no
//!   special `--prompt-stdin` flag is required.
//! - JSON output: enabled with `--format json` (not `--output-format json` —
//!   this differs from Claude Code). One JSON object per line on stdout.
//! - Session resume on retry turns: `--session <id>` is appended on every
//!   turn after the first. (The boolean `--continue` flag resumes the *last*
//!   session globally; we need explicit-by-id continuation so concurrent
//!   per-issue workspaces don't cross-talk.)
//! - Auth: handled entirely by the `opencode` CLI itself via
//!   `opencode auth login`. Sinfonia does NOT pass an api_key — there's
//!   nothing to pass; OpenCode owns its own credentials.
//! - Model selection: passed through verbatim via `--model <name>`.
//!   OpenCode's wire format is `provider/model` (e.g.
//!   `anthropic/claude-sonnet-4-6`); the `model:` field in `WORKFLOW.md`
//!   gets handed to OpenCode as-is, with no client-side validation. If the
//!   value doesn't match anything in the user's local OpenCode config,
//!   OpenCode itself surfaces the error.
//!
//! ## Verified flag set (per the upstream `run.ts` as of impl time)
//!
//! The Phase 2 plan (`docs/v0.3-plan/02-opencode-backend.md` §7) proposed
//! `--prompt-stdin --output-format json --quiet --continue <id> --model <name>`
//! pending a doc spike. The doc spike (against
//! <https://opencode.ai/docs/cli/> and the
//! `sst/opencode` repository's `packages/opencode/src/cli/cmd/run.ts`) found:
//!
//! | Plan proposed                | Actual flag                              |
//! |------------------------------|------------------------------------------|
//! | `--prompt-stdin`             | (no flag — stdin auto-detected via piped TTY check) |
//! | `--output-format json`       | `--format json`                          |
//! | `--quiet`                    | (no quiet flag; `--format json` already suppresses the TUI) |
//! | `--continue <id>`            | `--session <id>` (the bare `--continue` boolean resumes the *last* session, not by ID) |
//! | `--model <name>`             | `--model <name>` (unchanged; value is `provider/model`)   |
//!
//! The JSON event stream emits one object per line on stdout. Every event
//! carries `type`, `timestamp` (unix-ms epoch), and `sessionID` (camelCase —
//! differs from Claude Code's snake_case `session_id`). Event `type`s include
//! `step_start`, `step_finish`, `text`, `tool_use`, `reasoning`, and `error`.
//! The session ID appears on the very first event so the streaming task can
//! upgrade the placeholder `pending-<uuid>` thread ID before the turn ends —
//! identical mechanic to `cli.rs::run_turn`.
//!
//! ## Cross-restart session resumption
//!
//! OpenCode sessions persist on disk (under
//! `~/.local/share/opencode/`), so in principle Sinfonia could re-attach to a
//! ticket's session across daemon restarts. Per spec §14.3, in-flight session
//! metadata is in-memory and does not survive restart; this backend follows
//! that — on restart the next turn starts a fresh session and OpenCode picks
//! up from the workspace's on-disk state (the git tree, the `.sinfonia/`
//! scratch dir, etc.) rather than from any prior session context.

use super::events::{AgentEvent, EventSender, TokenUsage};
use super::turn::Message;
use super::{AgentSession, CodingAgent, TurnOutcome};
use crate::config::LlmConfig;
use crate::domain::Issue;
use crate::errors::{Error, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use uuid::Uuid;

/// `provider: opencode` backend. Spawns the `opencode run --format json`
/// CLI per turn, pipes the prompt on stdin, and parses one JSON event per
/// line on stdout. See the module-level docs for the verified flag set and
/// the auth model.
pub struct OpenCodeAgent {
    /// Full command line (without the resume / model flags) — operators
    /// override this in `WORKFLOW.md` to point at a non-PATH binary or add
    /// flags. Defaults to `opencode run --format json` (see
    /// `config::typed::default_command`).
    command: String,
    /// Optional `provider/model` string passed through to OpenCode via
    /// `--model`. None means OpenCode picks based on its own config.
    model: Option<String>,
    /// Per-turn timeout. Hitting this kills the subprocess and returns
    /// `TurnOutcome::Timeout`. Matches `cli.rs`'s timeout semantic.
    turn_timeout: Duration,
}

impl OpenCodeAgent {
    /// Construct an OpenCode backend for the supplied LLM config.
    ///
    /// Validates that `command` is non-empty (the CLI invocation has to
    /// resolve to a real binary on `PATH`) and runs a preflight `which`
    /// check on the binary name so misconfigurations fail loudly at
    /// startup rather than turn-time.
    pub fn new(_cfg: &crate::config::ServiceConfig, llm: &LlmConfig) -> Result<Self> {
        if llm.command.trim().is_empty() {
            return Err(Error::ConfigInvalid(
                "OpenCode: agent.command must be non-empty".into(),
            ));
        }
        preflight(&llm.command)?;
        let model = if llm.model.trim().is_empty() {
            None
        } else {
            Some(llm.model.clone())
        };
        Ok(OpenCodeAgent {
            command: llm.command.clone(),
            model,
            turn_timeout: Duration::from_millis(llm.turn_timeout_ms),
        })
    }

    /// Build the full `bash -lc` command line for one turn.
    ///
    /// `session_id` is the prior turn's session id (if any). We treat any
    /// id that starts with `pending-` as "no real session yet" — that's the
    /// placeholder convention `cli.rs` uses and `start_session` populates
    /// it the same way. See the module-level docs for the rationale.
    fn build_command_line(&self, session_id: Option<&str>) -> String {
        let mut cmd = self.command.clone();
        if let Some(model) = &self.model {
            cmd.push_str(" --model ");
            cmd.push_str(&shell_escape(model));
        }
        if let Some(id) = session_id {
            if !id.starts_with("pending-") {
                cmd.push_str(" --session ");
                cmd.push_str(&shell_escape(id));
            }
        }
        cmd
    }
}

#[async_trait]
impl CodingAgent for OpenCodeAgent {
    /// Start a fresh session. The real OpenCode session ID is discovered on
    /// the first JSON event line, so until then we hand back a
    /// `pending-<uuid>` placeholder so the orchestrator's observability has
    /// something to log. `run_turn` upgrades the thread ID to the real
    /// `sessionID` as soon as the first event arrives.
    async fn start_session(&self, _issue: &Issue, workspace: PathBuf) -> Result<AgentSession> {
        Ok(AgentSession {
            thread_id: format!("pending-{}", Uuid::new_v4()),
            workspace,
            history: Vec::new(),
        })
    }

    /// Run a single turn. Spawns `opencode run --format json` (plus
    /// `--session <id>` on resume turns), pipes `prompt` to stdin, and
    /// streams stdout line-by-line so the dashboard updates mid-turn.
    ///
    /// Token usage is currently not surfaced by OpenCode's per-event JSON
    /// stream in a stable shape; this backend opportunistically reads any
    /// `usage` block on `step_finish`-type events (best effort). The final
    /// text message is taken from the last `text` event on stdout.
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

        // Feed the prompt via stdin (no escaping concerns).
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

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let events_for_stream = events.clone();
        let turn_id_for_stream = turn_id.clone();
        let thread_id_shared = Arc::new(Mutex::new(session.thread_id.clone()));
        let thread_id_for_stream = thread_id_shared.clone();

        // Streaming task: read one JSON event per line from stdout and
        // (a) upgrade the placeholder `pending-` thread ID as soon as the
        // first event carrying `sessionID` arrives, and (b) emit mid-turn
        // TurnProgress events whenever the event payload exposes a usage
        // block so the dashboard updates instead of sitting silent.
        let stdout_task: tokio::task::JoinHandle<String> = tokio::spawn(async move {
            let mut buf = String::new();
            let Some(pipe) = stdout_pipe else {
                return buf;
            };
            let mut reader = BufReader::new(pipe);
            let mut line = String::new();
            let mut cum_in: u64 = 0;
            let mut cum_out: u64 = 0;
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
                        // OpenCode emits `sessionID` (camelCase). The first
                        // event already carries it, so this fires before any
                        // tool output reaches us.
                        if let Some(sid) = extract_session_id(&v) {
                            if let Ok(mut t) = thread_id_for_stream.lock() {
                                if t.starts_with("pending-") || t.is_empty() {
                                    *t = sid.to_string();
                                }
                            }
                        }
                        if let Some((di, do_)) = extract_stream_usage(&v) {
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

        let parsed = parse_opencode_output(&stdout_buf);
        if let Some(sid) = parsed.session_id.clone() {
            session.thread_id = sid;
        } else if let Ok(t) = thread_id_shared.lock() {
            // Streaming saw the real session id even if the terminal parser
            // missed it (e.g. final line was partial JSON).
            if !t.starts_with("pending-") && session.thread_id.starts_with("pending-") {
                session.thread_id = t.clone();
            }
        }
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

    /// No teardown work — each turn spawns a fresh subprocess and exits
    /// cleanly; there's no persistent app-server connection to close.
    async fn stop_session(&self, _session: AgentSession) -> Result<()> {
        Ok(())
    }
}

/// Preflight check: confirm the binary referenced by `agent.command` is on
/// `PATH`. The plan (§3.4) calls for this so misconfigurations fail at
/// startup with a clear "install opencode first" message rather than as a
/// generic spawn failure once a ticket lands in the eligible state.
fn preflight(command: &str) -> Result<()> {
    let bin = command.split_whitespace().next().unwrap_or("opencode");
    which::which(bin).map_err(|_| {
        Error::CodexNotFound(format!(
            "opencode: binary '{bin}' not found on PATH. Install OpenCode \
             (https://opencode.ai/docs/) and run `opencode auth login` once \
             before starting the daemon."
        ))
    })?;
    Ok(())
}

#[derive(Debug, Default)]
struct ParsedOpenCodeOutput {
    final_message: String,
    session_id: Option<String>,
    usage: Option<TokenUsage>,
}

/// Parse the full stdout buffer from one `opencode run` turn.
///
/// OpenCode's stream is line-delimited JSON (one event per line), so the
/// "final" message is the last `text`-type event we saw — the model's
/// closing assistant message after all tool use is done. We also scan for
/// the latest `sessionID` (every line carries it; the first one carries
/// the real value when our placeholder is in effect) and any final usage
/// block, both for record-keeping in the orchestrator.
fn parse_opencode_output(stdout: &str) -> ParsedOpenCodeOutput {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return ParsedOpenCodeOutput {
            final_message: "(empty CLI output)".into(),
            ..Default::default()
        };
    }
    let mut session_id: Option<String> = None;
    let mut final_message: Option<String> = None;
    let mut usage: Option<TokenUsage> = None;
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(sid) = extract_session_id(&v) {
            session_id = Some(sid);
        }
        if let Some(text) = extract_text_message(&v) {
            final_message = Some(text);
        }
        if let Some(u) = extract_terminal_usage(&v) {
            usage = Some(u);
        }
    }
    ParsedOpenCodeOutput {
        final_message: final_message.unwrap_or_else(|| trimmed.to_string()),
        session_id,
        usage,
    }
}

/// Pull the OpenCode session ID off any event. Upstream uses camelCase
/// (`sessionID`); we also tolerate the snake_case `session_id` form in case
/// a future release adopts it for parity with Claude Code.
fn extract_session_id(v: &Value) -> Option<String> {
    v.get("sessionID")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("session_id").and_then(|x| x.as_str()))
        .map(str::to_string)
}

/// Pull the assistant text out of a `text`-type event. OpenCode wraps the
/// model's reply inside `part.text` (see the upstream `run.ts` `emit("text",
/// { part })` call), and only emits the event when `part.time.end` is set
/// — i.e. the text part is finalized rather than still streaming.
fn extract_text_message(v: &Value) -> Option<String> {
    if v.get("type").and_then(|x| x.as_str())? != "text" {
        return None;
    }
    v.get("part")
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .map(str::to_string)
}

/// Best-effort per-event token usage. OpenCode's event schema is still
/// evolving; we read whichever shape is present and feed cumulative
/// updates through the streaming progress events. Returns `None` for
/// lines without a `usage` block.
fn extract_stream_usage(v: &Value) -> Option<(u64, u64)> {
    // Step events carry usage inside `part.tokens` or `part.usage` depending
    // on the OpenCode build. Probe both before giving up.
    let part = v.get("part")?;
    let u = part.get("tokens").or_else(|| part.get("usage"))?;
    let i = u
        .get("input")
        .or_else(|| u.get("input_tokens"))
        .or_else(|| u.get("prompt_tokens"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let o = u
        .get("output")
        .or_else(|| u.get("output_tokens"))
        .or_else(|| u.get("completion_tokens"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    if i == 0 && o == 0 {
        return None;
    }
    Some((i, o))
}

/// Final usage extracted from any single event for the `TurnCompleted`
/// summary. Same fields as `extract_stream_usage` but returns a typed
/// `TokenUsage` and is keyed off the most recent event seen rather than
/// being accumulated.
fn extract_terminal_usage(v: &Value) -> Option<TokenUsage> {
    let (i, o) = extract_stream_usage(v)?;
    let t = v
        .get("part")
        .and_then(|p| p.get("tokens").or_else(|| p.get("usage")))
        .and_then(|u| u.get("total_tokens").and_then(|x| x.as_u64()))
        .unwrap_or(i + o);
    Some(TokenUsage {
        input_tokens: i,
        output_tokens: o,
        total_tokens: t,
    })
}

/// Quote a value safely for embedding in a `bash -lc` command line. Lifted
/// verbatim from `cli.rs` — keeping the helpers private to each backend
/// rather than promoting them to a shared module is deliberate per the
/// Phase 2 plan's "copy-and-adapt over abstraction" guidance.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentProvider;

    fn agent_with(model: Option<&str>) -> OpenCodeAgent {
        OpenCodeAgent {
            command: "opencode run --format json".into(),
            model: model.map(str::to_string),
            turn_timeout: Duration::from_secs(60),
        }
    }

    #[test]
    fn build_command_line_first_turn() {
        // No prior session and no model → only the bare command line.
        let a = agent_with(None);
        let cmd = a.build_command_line(None);
        assert_eq!(cmd, "opencode run --format json");
        // With a model it appears via `--model <name>`.
        let a = agent_with(Some("anthropic/claude-sonnet-4-6"));
        let cmd = a.build_command_line(None);
        assert_eq!(
            cmd,
            "opencode run --format json --model anthropic/claude-sonnet-4-6"
        );
        // No `--session` anywhere on the first turn.
        assert!(!cmd.contains("--session"));
    }

    #[test]
    fn build_command_line_pending_session() {
        // Placeholder thread IDs are treated as "no real session" — the
        // streaming task hasn't seen the first sessionID yet, so we
        // shouldn't pass --session on this invocation either.
        let a = agent_with(None);
        let cmd = a.build_command_line(Some("pending-abc-123"));
        assert_eq!(cmd, "opencode run --format json");
        assert!(!cmd.contains("--session"));
    }

    #[test]
    fn build_command_line_resumed_session() {
        // A real session id triggers --session, with shell-escaping when
        // needed.
        let a = agent_with(Some("anthropic/claude-sonnet-4-6"));
        let cmd = a.build_command_line(Some("ses_abc123"));
        assert_eq!(
            cmd,
            "opencode run --format json --model anthropic/claude-sonnet-4-6 --session ses_abc123"
        );
        // A session id with a space (synthetic, but exercises the
        // escape path) gets single-quoted.
        let a = agent_with(None);
        let cmd = a.build_command_line(Some("ses one"));
        assert_eq!(cmd, "opencode run --format json --session 'ses one'");
    }

    #[test]
    fn parse_init_event() {
        // The first JSON line carries `sessionID` (camelCase) and is
        // enough to upgrade the pending placeholder thread id.
        let line = r#"{"type":"step_start","timestamp":1234,"sessionID":"ses_init_42","part":{"id":"s1"}}"#;
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(extract_session_id(&v), Some("ses_init_42".to_string()));

        // Also tolerate the snake_case form on the off chance OpenCode
        // adopts it for parity with Claude Code.
        let alt = r#"{"type":"step_start","timestamp":1234,"session_id":"snake_42"}"#;
        let v: Value = serde_json::from_str(alt).unwrap();
        assert_eq!(extract_session_id(&v), Some("snake_42".to_string()));
    }

    #[test]
    fn parse_token_event() {
        // OpenCode's step_finish events carry usage inside `part.tokens`.
        let line = r#"{
            "type":"step_finish",
            "timestamp":1234,
            "sessionID":"ses_abc",
            "part":{"id":"s1","tokens":{"input":120,"output":34,"total_tokens":154}}
        }"#;
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(extract_stream_usage(&v), Some((120, 34)));
        let u = extract_terminal_usage(&v).unwrap();
        assert_eq!(u.input_tokens, 120);
        assert_eq!(u.output_tokens, 34);
        assert_eq!(u.total_tokens, 154);

        // Tolerate the older `usage` / `input_tokens` shape too.
        let alt = r#"{
            "type":"step_finish",
            "sessionID":"ses_abc",
            "part":{"usage":{"input_tokens":7,"output_tokens":3}}
        }"#;
        let v: Value = serde_json::from_str(alt).unwrap();
        assert_eq!(extract_stream_usage(&v), Some((7, 3)));

        // Lines without a usage block return None.
        let none = r#"{"type":"text","sessionID":"x","part":{"text":"hi"}}"#;
        let v: Value = serde_json::from_str(none).unwrap();
        assert_eq!(extract_stream_usage(&v), None);
    }

    #[test]
    fn parse_full_stdout_picks_last_text() {
        // Realistic multi-event stream: init → text → step_finish → text.
        // The terminal parser should return the *last* text event.
        let stream = "\
{\"type\":\"step_start\",\"sessionID\":\"ses_real\",\"part\":{}}\n\
{\"type\":\"text\",\"sessionID\":\"ses_real\",\"part\":{\"text\":\"first\"}}\n\
{\"type\":\"step_finish\",\"sessionID\":\"ses_real\",\"part\":{\"tokens\":{\"input\":10,\"output\":5}}}\n\
{\"type\":\"text\",\"sessionID\":\"ses_real\",\"part\":{\"text\":\"all done\"}}\n";
        let p = parse_opencode_output(stream);
        assert_eq!(p.final_message, "all done");
        assert_eq!(p.session_id.as_deref(), Some("ses_real"));
        let u = p.usage.unwrap();
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.output_tokens, 5);
        assert_eq!(u.total_tokens, 15);
    }

    #[test]
    fn preflight_missing_binary() {
        // A binary that almost-certainly doesn't exist on any PATH.
        let err = preflight("definitely-not-a-real-binary-name-7f3a opencode --foo").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("definitely-not-a-real-binary-name-7f3a"),
            "expected the missing binary name in the error, got: {msg}"
        );
        assert!(
            msg.contains("opencode.ai") || msg.contains("auth login"),
            "expected a pointer to the OpenCode install docs, got: {msg}"
        );
    }

    #[test]
    fn opencode_provider_is_cli_provider() {
        // Smoke check: keep `is_cli()` in sync with this backend.
        assert!(AgentProvider::OpenCode.is_cli());
        assert!(!AgentProvider::OpenCode.is_raw_llm());
    }
}
