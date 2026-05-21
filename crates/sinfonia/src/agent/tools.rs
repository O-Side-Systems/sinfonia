//! Built-in coding-agent tools. Each backend converts these into its own tool/function
//! call schema, then forwards execution back through `dispatch_tool` here.
//!
//! The agent is confined to its workspace path: all filesystem access is validated
//! against `workspace_root`, and shell commands run with `cwd = workspace_root`
//! (Invariant 1, §9.5).

use crate::errors::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

/// Stable tool catalog. The same shape is exposed to OpenAI, Anthropic, Google,
/// and Ollama via small per-backend adapters.
pub fn tool_catalog() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "shell".into(),
            description: "Run a non-interactive shell command in the issue workspace. \
Output is captured. Use this for any external tool: tests, formatters, git, etc."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Bash command to run." },
                    "timeout_ms": { "type": "integer", "description": "Optional per-command timeout in ms. Default 120000." }
                },
                "required": ["command"]
            }),
        },
        ToolSpec {
            name: "read_file".into(),
            description: "Read a UTF-8 text file from the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path relative to workspace root." }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "write_file".into(),
            description: "Write (create or overwrite) a UTF-8 text file in the workspace.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "contents": { "type": "string" }
                },
                "required": ["path", "contents"]
            }),
        },
        ToolSpec {
            name: "edit_file".into(),
            description: "Replace exactly one occurrence of `old_string` with `new_string` in the named file. Fails if there is not exactly one match.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        ToolSpec {
            name: "list_dir".into(),
            description: "List the contents of a directory in the workspace (non-recursive).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path; defaults to '.'." }
                }
            }),
        },
        ToolSpec {
            name: "finish".into(),
            description: "Call when the work for this turn is done. `summary` SHOULD describe what changed and what remains.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "summary": { "type": "string" }
                },
                "required": ["summary"]
            }),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Renderable output the LLM sees as the tool's response.
    pub output: String,
    /// True when this tool call ended the turn (e.g. `finish`).
    pub is_final: bool,
}

/// Dispatch a tool call. `workspace_root` is the absolute workspace path that bounds
/// any path argument.
pub async fn dispatch_tool(
    name: &str,
    arguments: &Value,
    workspace_root: &Path,
) -> Result<ToolResult> {
    match name {
        "shell" => run_shell(arguments, workspace_root).await,
        "read_file" => run_read_file(arguments, workspace_root).await,
        "write_file" => run_write_file(arguments, workspace_root).await,
        "edit_file" => run_edit_file(arguments, workspace_root).await,
        "list_dir" => run_list_dir(arguments, workspace_root).await,
        "finish" => Ok(ToolResult {
            output: arguments
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("(no summary provided)")
                .to_string(),
            is_final: true,
        }),
        other => Ok(ToolResult {
            output: format!("tool '{other}' is not supported by this Sinfonia runtime"),
            is_final: false,
        }),
    }
}

async fn run_shell(args: &Value, workspace_root: &Path) -> Result<ToolResult> {
    let cmd = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other("shell: missing 'command'".into()))?;
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(120_000);

    let mut command = Command::new("bash");
    command
        .arg("-lc")
        .arg(cmd)
        .current_dir(workspace_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| Error::Other(format!("shell spawn: {e}")))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();

    let wait = async {
        let exit = child
            .wait()
            .await
            .map_err(|e| Error::Other(format!("shell wait: {e}")))?;
        let mut so = String::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_string(&mut so).await;
        }
        let mut se = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut se).await;
        }
        Ok::<_, Error>((exit, so, se))
    };

    match timeout(Duration::from_millis(timeout_ms), wait).await {
        Ok(Ok((status, so, se))) => Ok(ToolResult {
            output: format!(
                "exit_code: {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
                truncate(&so, 16_000),
                truncate(&se, 4_000)
            ),
            is_final: false,
        }),
        Ok(Err(e)) => Ok(ToolResult {
            output: format!("shell error: {e}"),
            is_final: false,
        }),
        Err(_) => {
            let _ = child.start_kill();
            Ok(ToolResult {
                output: format!("shell timed out after {timeout_ms}ms"),
                is_final: false,
            })
        }
    }
}

fn resolve_in_workspace(workspace_root: &Path, raw: &str) -> Result<PathBuf> {
    let candidate = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        workspace_root.join(raw)
    };
    // Manually normalize without requiring the file to exist.
    let mut normalized = PathBuf::new();
    for c in candidate.components() {
        use std::path::Component;
        match c {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if !normalized.starts_with(workspace_root) {
        return Err(Error::InvalidWorkspaceCwd(format!(
            "path '{}' is outside workspace root '{}'",
            normalized.display(),
            workspace_root.display()
        )));
    }
    Ok(normalized)
}

async fn run_read_file(args: &Value, workspace_root: &Path) -> Result<ToolResult> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other("read_file: missing 'path'".into()))?;
    let resolved = resolve_in_workspace(workspace_root, path)?;
    match tokio::fs::read_to_string(&resolved).await {
        Ok(text) => Ok(ToolResult {
            output: truncate(&text, 32_000),
            is_final: false,
        }),
        Err(e) => Ok(ToolResult {
            output: format!("read_file error: {e}"),
            is_final: false,
        }),
    }
}

async fn run_write_file(args: &Value, workspace_root: &Path) -> Result<ToolResult> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other("write_file: missing 'path'".into()))?;
    let contents = args
        .get("contents")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other("write_file: missing 'contents'".into()))?;
    let resolved = resolve_in_workspace(workspace_root, path)?;
    if let Some(parent) = resolved.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    match tokio::fs::write(&resolved, contents).await {
        Ok(()) => Ok(ToolResult {
            output: format!("wrote {} bytes to {}", contents.len(), resolved.display()),
            is_final: false,
        }),
        Err(e) => Ok(ToolResult {
            output: format!("write_file error: {e}"),
            is_final: false,
        }),
    }
}

async fn run_edit_file(args: &Value, workspace_root: &Path) -> Result<ToolResult> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other("edit_file: missing 'path'".into()))?;
    let old_string = args
        .get("old_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other("edit_file: missing 'old_string'".into()))?;
    let new_string = args
        .get("new_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Other("edit_file: missing 'new_string'".into()))?;
    let resolved = resolve_in_workspace(workspace_root, path)?;
    let content = match tokio::fs::read_to_string(&resolved).await {
        Ok(t) => t,
        Err(e) => {
            return Ok(ToolResult {
                output: format!("edit_file read error: {e}"),
                is_final: false,
            })
        }
    };
    let occurrences = content.matches(old_string).count();
    if occurrences == 0 {
        return Ok(ToolResult {
            output: "edit_file error: old_string not found".into(),
            is_final: false,
        });
    }
    if occurrences > 1 {
        return Ok(ToolResult {
            output: format!(
                "edit_file error: {occurrences} matches found; old_string must be unique"
            ),
            is_final: false,
        });
    }
    let new_content = content.replacen(old_string, new_string, 1);
    if let Err(e) = tokio::fs::write(&resolved, new_content).await {
        return Ok(ToolResult {
            output: format!("edit_file write error: {e}"),
            is_final: false,
        });
    }
    Ok(ToolResult {
        output: format!("edited {}", resolved.display()),
        is_final: false,
    })
}

async fn run_list_dir(args: &Value, workspace_root: &Path) -> Result<ToolResult> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let resolved = resolve_in_workspace(workspace_root, path)?;
    let mut rd = match tokio::fs::read_dir(&resolved).await {
        Ok(rd) => rd,
        Err(e) => {
            return Ok(ToolResult {
                output: format!("list_dir error: {e}"),
                is_final: false,
            })
        }
    };
    let mut lines = Vec::new();
    while let Ok(Some(entry)) = rd.next_entry().await {
        let ft = entry.file_type().await.ok();
        let marker = match ft {
            Some(t) if t.is_dir() => "/",
            Some(t) if t.is_symlink() => "@",
            _ => "",
        };
        lines.push(format!("{}{}", entry.file_name().to_string_lossy(), marker));
    }
    lines.sort();
    Ok(ToolResult {
        output: lines.join("\n"),
        is_final: false,
    })
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!(
            "{}\n…[truncated {} bytes]",
            &s[..n],
            s.len() - n
        )
    }
}
