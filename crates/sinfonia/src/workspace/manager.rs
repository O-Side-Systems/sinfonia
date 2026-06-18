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

    /// List the immediate child workspace directories under the root, each with
    /// its last-modified time, for the age-based reaper. Non-directory entries
    /// are skipped; an unreadable root yields an empty list (best-effort).
    ///
    /// `modified` is the workspace *directory's* mtime — it advances when the
    /// session adds/removes top-level entries (clone, branch switch), not on
    /// deep writes into `target/`. That is sufficient because the reaper also
    /// skips any issue currently running; mtime only ranks the idle ones.
    pub fn list_workspaces(&self) -> Vec<WorkspaceEntry> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return out;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let key = entry.file_name().to_string_lossy().into_owned();
            let modified = entry.metadata().and_then(|m| m.modified()).ok();
            out.push(WorkspaceEntry { key, path, modified });
        }
        out
    }

    /// Remove a workspace directory by its (already-sanitized) directory key,
    /// confined to the root. Used by the age-based reaper, which works from
    /// the directory names returned by [`list_workspaces`](Self::list_workspaces).
    pub fn remove_key(&self, key: &str) -> std::io::Result<()> {
        let path = self.root.join(key);
        // Confinement: a listed child key is already a single path component,
        // but guard against ever escaping or deleting the root itself.
        if path == self.root || !path.starts_with(&self.root) {
            return Ok(());
        }
        if path.exists() {
            std::fs::remove_dir_all(path)
        } else {
            Ok(())
        }
    }
}

/// One entry from [`WorkspaceManager::list_workspaces`].
#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    /// Sanitized directory name (the workspace key).
    pub key: String,
    pub path: PathBuf,
    /// Last-modified time of the workspace directory, when obtainable.
    pub modified: Option<std::time::SystemTime>,
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
            workspace: WorkspaceConfig {
                root,
                cleanup: Default::default(),
            },
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

    #[test]
    fn list_and_remove_key_roundtrip() {
        let dir = tempdir().unwrap();
        let mgr = WorkspaceManager::new(&cfg_for(dir.path().to_path_buf())).unwrap();
        mgr.ensure_for_issue("ABC-1").unwrap();
        mgr.ensure_for_issue("ABC-2").unwrap();
        // A stray non-directory entry under root is ignored by the lister.
        std::fs::write(mgr.root().join("loose.txt"), b"x").unwrap();

        let mut keys: Vec<String> = mgr.list_workspaces().into_iter().map(|w| w.key).collect();
        keys.sort();
        assert_eq!(keys, vec!["ABC-1".to_string(), "ABC-2".to_string()]);
        assert!(mgr.list_workspaces().iter().all(|w| w.modified.is_some()));

        // remove_key deletes exactly that workspace dir.
        mgr.remove_key("ABC-1").unwrap();
        let after: Vec<String> = mgr.list_workspaces().into_iter().map(|w| w.key).collect();
        assert_eq!(after, vec!["ABC-2".to_string()]);

        // Confinement: a key that resolves to the root or escapes is a no-op,
        // never a destructive delete.
        mgr.remove_key("").unwrap();
        assert!(mgr.root().exists());
    }
}
