//! Workspace lifecycle hooks (spec §9.4).

use crate::errors::{Error, Result};
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy)]
pub enum HookKind {
    AfterCreate,
    BeforeRun,
    AfterRun,
    BeforeRemove,
}

impl HookKind {
    pub fn name(self) -> &'static str {
        match self {
            HookKind::AfterCreate => "after_create",
            HookKind::BeforeRun => "before_run",
            HookKind::AfterRun => "after_run",
            HookKind::BeforeRemove => "before_remove",
        }
    }
}

/// Run a hook script under `bash -lc <script>` with `cwd = workspace_path` (§9.4).
/// Returns `Ok(())` on a 0 exit code. Times out using `timeout_ms`.
pub async fn run_hook(
    kind: HookKind,
    script: &str,
    workspace_path: &Path,
    timeout_ms: u64,
) -> Result<()> {
    if script.trim().is_empty() {
        return Ok(());
    }
    info!(target: "workspace.hook", hook = kind.name(), cwd = %workspace_path.display(), "running");

    let mut cmd = Command::new("bash");
    cmd.arg("-lc")
        .arg(script)
        .current_dir(workspace_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| Error::HookFailed {
            name: kind.name().into(),
            reason: format!("spawn: {e}"),
        })?;

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();

    let wait = async {
        let exit = child.wait().await.map_err(|e| Error::HookFailed {
            name: kind.name().into(),
            reason: format!("wait: {e}"),
        })?;
        let mut so = String::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_string(&mut so).await;
        }
        let mut se = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut se).await;
        }
        Ok::<(std::process::ExitStatus, String, String), Error>((exit, so, se))
    };

    match timeout(Duration::from_millis(timeout_ms), wait).await {
        Ok(Ok((status, so, se))) => {
            if status.success() {
                Ok(())
            } else {
                warn!(target: "workspace.hook", hook = kind.name(), code = ?status.code(), "non-zero exit");
                Err(Error::HookFailed {
                    name: kind.name().into(),
                    reason: format!(
                        "exit {:?}; stdout={}; stderr={}",
                        status.code(),
                        truncate(&so, 1024),
                        truncate(&se, 1024)
                    ),
                })
            }
        }
        Ok(Err(e)) => Err(e),
        Err(_) => {
            let _ = child.start_kill();
            Err(Error::HookTimeout {
                name: kind.name().into(),
            })
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}
