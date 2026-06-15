//! Per-issue workspace creation + safety invariants.

use crate::config::ServiceConfig;
use crate::domain::sanitize_workspace_key;
use crate::errors::{Error, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub path: PathBuf,
    pub workspace_key: String,
    pub created_now: bool,
}

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    root: PathBuf,
}

impl WorkspaceManager {
    pub fn new(cfg: &ServiceConfig) -> Result<Self> {
        let root = cfg.workspace.root.clone();
        if !root.exists() {
            std::fs::create_dir_all(&root)
                .map_err(|e| Error::WorkspaceCreate(format!("create root {}: {e}", root.display())))?;
        }
        let root = root.canonicalize().map_err(|e| {
            Error::WorkspaceCreate(format!("canonicalize root {}: {e}", root.display()))
        })?;
        Ok(WorkspaceManager { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create-or-reuse the workspace directory for `identifier`.
    /// §9.2, §9.5.
    pub fn ensure_for_issue(&self, identifier: &str) -> Result<Workspace> {
        let key = sanitize_workspace_key(identifier);
        if key.is_empty() {
            return Err(Error::WorkspaceCreate(
                "sanitized identifier is empty".into(),
            ));
        }
        let mut path = self.root.clone();
        path.push(&key);

        let created_now = if path.exists() {
            if !path.is_dir() {
                return Err(Error::WorkspaceCreate(format!(
                    "{} exists and is not a directory",
                    path.display()
                )));
            }
            false
        } else {
            std::fs::create_dir_all(&path)
                .map_err(|e| Error::WorkspaceCreate(format!("create {}: {e}", path.display())))?;
            true
        };

        // Containment check (Invariant 2, §9.5).
        let canonical_path = path.canonicalize().map_err(|e| {
            Error::WorkspaceCreate(format!("canonicalize {}: {e}", path.display()))
        })?;
        if !canonical_path.starts_with(&self.root) {
            return Err(Error::WorkspaceOutOfRoot {
                workspace: canonical_path.display().to_string(),
                root: self.root.display().to_string(),
            });
        }

        Ok(Workspace {
            path: canonical_path,
            workspace_key: key,
            created_now,
        })
    }

    pub fn workspace_path_for(&self, identifier: &str) -> PathBuf {
        let key = sanitize_workspace_key(identifier);
        self.root.join(key)
    }

    /// Best-effort recursive removal of an issue's workspace.
    pub fn remove(&self, identifier: &str) -> std::io::Result<()> {
        let path = self.workspace_path_for(identifier);
        if path.exists() {
            std::fs::remove_dir_all(path)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentConfig, HooksConfig, LlmConfig, AgentProvider, PollingConfig, ServerConfig, ServiceConfig, TrackerConfig, TrackerKind, WorkspaceConfig};
    use tempfile::tempdir;

    fn cfg_for(root: PathBuf) -> ServiceConfig {
        ServiceConfig {
            tracker: TrackerConfig {
                kind: TrackerKind::Linear,
                endpoint: "x".into(),
                api_key: Some("x".into()),
                project_slug: Some("x".into()),
                active_states: vec![],
                terminal_states: vec![],
                jira_email: None,
            },
            polling: PollingConfig { interval_ms: 1000 },
            workspace: WorkspaceConfig { root },
            hooks: HooksConfig {
                after_create: None,
                before_run: None,
                after_run: None,
                before_remove: None,
                timeout_ms: 60_000,
            },
            agent: AgentConfig {
                max_concurrent_agents: 1,
                max_turns: 1,
                max_retry_backoff_ms: 1000,
                max_concurrent_agents_by_state: Default::default(),
                env_policy: Default::default(),
            },
            dispatch_allowlist: Default::default(),
            llm: LlmConfig {
                provider: AgentProvider::Anthropic,
                model: "m".into(),
                endpoint: None,
                api_key: None,
                command: "x".into(),
                turn_timeout_ms: 1,
                read_timeout_ms: 1,
                stall_timeout_ms: 1,
                approval_policy: None,
                thread_sandbox: None,
                turn_sandbox_policy: None,
                temperature: None,
            },
            server: ServerConfig::default(),
            telemetry: Default::default(),
            states: Default::default(),
            workflow_path: PathBuf::from("/dev/null"),
        }
    }

    #[test]
    fn create_and_reuse_workspace() {
        let dir = tempdir().unwrap();
        let mgr = WorkspaceManager::new(&cfg_for(dir.path().to_path_buf())).unwrap();
        let a = mgr.ensure_for_issue("ABC-1").unwrap();
        assert!(a.created_now);
        let b = mgr.ensure_for_issue("ABC-1").unwrap();
        assert!(!b.created_now);
        assert_eq!(a.path, b.path);
    }

    #[test]
    fn rejects_traversal_via_sanitization() {
        let dir = tempdir().unwrap();
        let mgr = WorkspaceManager::new(&cfg_for(dir.path().to_path_buf())).unwrap();
        let ws = mgr.ensure_for_issue("../../etc").unwrap();
        // The sanitized form keeps it inside root.
        assert!(ws.path.starts_with(mgr.root()));
        assert_eq!(ws.workspace_key, ".._.._etc");
    }
}
