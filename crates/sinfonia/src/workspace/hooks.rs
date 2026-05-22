//! Workspace lifecycle hooks (spec §9.4).

use crate::errors::{Error, Result};
use crate::telemetry::spans;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, info_span, warn, Instrument};

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
///
/// Wrapped in the `workspace.hook` span (plan §4). Tenant inheritance comes
/// from the ambient `runner.session` span via the OTel parent-child
/// relationship; the resource-level `service.namespace = tenant_id`
/// attribute also flows through.
pub async fn run_hook(
    kind: HookKind,
    script: &str,
    workspace_path: &Path,
    timeout_ms: u64,
) -> Result<()> {
    if script.trim().is_empty() {
        return Ok(());
    }
    let span = info_span!(
        target: "workspace.hook",
        spans::WORKSPACE_HOOK,
        { spans::ATTR_HOOK_NAME } = kind.name(),
        { spans::ATTR_DURATION_MS } = tracing::field::Empty,
        { spans::ATTR_EXIT_CODE } = tracing::field::Empty,
    );
    run_hook_inner(kind, script, workspace_path, timeout_ms)
        .instrument(span)
        .await
}

async fn run_hook_inner(
    kind: HookKind,
    script: &str,
    workspace_path: &Path,
    timeout_ms: u64,
) -> Result<()> {
    let started = std::time::Instant::now();
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

    let result = match timeout(Duration::from_millis(timeout_ms), wait).await {
        Ok(Ok((status, so, se))) => {
            let code = status.code().unwrap_or(-1) as i64;
            tracing::Span::current().record(spans::ATTR_EXIT_CODE, code);
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
            tracing::Span::current().record(spans::ATTR_EXIT_CODE, -1_i64);
            Err(Error::HookTimeout {
                name: kind.name().into(),
            })
        }
    };
    tracing::Span::current().record(
        spans::ATTR_DURATION_MS,
        started.elapsed().as_millis() as i64,
    );
    result
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}
