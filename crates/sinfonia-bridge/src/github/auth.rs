//! GitHub authentication for the bridge.
//!
//! P1-F landed PAT-only auth as a single `match` in `main.rs::run`. P1-G
//! consolidates that branch behind a [`build_gh_ops`] factory and adds
//! GitHub App authentication, including a per-installation client cache
//! so a single bridge process can serve multiple installations of the
//! same App.
//!
//! ## Modes
//!
//! - **PAT.** One `Octocrab` built with `personal_token`. Cheap to share
//!   across handlers; PAT scopes (`repo`, `read:org`) are documented in
//!   `docs/v0.3-plan/05-skills-cli.md`.
//! - **App.** A JWT-authenticated "bare app" `Octocrab` plus a
//!   `tokio::sync::RwLock<HashMap<owner, Arc<Octocrab>>>` cache. On the
//!   first GhOps call against an `owner/repo`, the bridge calls
//!   `apps().get_repository_installation(owner, repo)`, derives an
//!   installation-scoped client via the synchronous
//!   `Octocrab::installation(InstallationId)` helper, and caches it
//!   keyed by **owner** — one installation per (App, owner) is the
//!   GitHub data model, so sibling repos under the same owner reuse
//!   the cached client without a second lookup.

use crate::config::GitHubSection;
use crate::github::client::OctocrabGhOps;
use crate::github::{CheckRunSummary, GhOps};
use crate::{Error, Result};
use async_trait::async_trait;
use jsonwebtoken::EncodingKey;
use octocrab::models::AppId;
use octocrab::Octocrab;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// The two authentication modes recognised by the bridge. Derived from
/// the parsed `[github]` config section.
#[derive(Debug, Clone)]
pub enum BridgeAuthMode {
    Pat(String),
    App { app_id: u64, private_key: String },
}

impl BridgeAuthMode {
    /// Resolve a mode from a parsed GitHub section.
    ///
    /// Validation rule 1 in `config::validate` already enforces that
    /// exactly one of `pat` or `app_id` is set, and that App mode also
    /// supplies a `private_key`. We re-check the same invariants here so
    /// that any future caller (tests, alternate config sources, a Phase
    /// 2 reload path) cannot drift past those rules silently.
    pub fn from_github_section(g: &GitHubSection) -> Result<Self> {
        match (&g.pat, g.app_id, &g.private_key) {
            (Some(token), None, _) => Ok(Self::Pat(token.clone())),
            (None, Some(app_id), Some(pk)) => Ok(Self::App {
                app_id,
                private_key: pk.clone(),
            }),
            (None, Some(_), None) => Err(Error::BridgeConfigInvalid(
                "github: app_id set but private_key missing".into(),
            )),
            (Some(_), Some(_), _) => Err(Error::BridgeConfigInvalid(
                "github: must set either pat or app_id (mutually exclusive)".into(),
            )),
            (None, None, _) => Err(Error::BridgeConfigInvalid(
                "github: must set either pat or app_id (mutually exclusive)".into(),
            )),
        }
    }

    /// Short label for log lines and self-test output.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pat(_) => "PAT mode",
            Self::App { .. } => "App mode",
        }
    }
}

/// Resolve a private-key value that may be either inline PEM or
/// `@/path/to/key.pem`. `~` in the path is expanded via
/// `shellexpand::tilde`. Inline PEM is returned as-is (whitespace
/// trimmed; the underlying parser doesn't care about trailing newlines).
pub fn load_private_key(value: &str) -> Result<String> {
    let s = value.trim();
    if s.is_empty() {
        return Err(Error::BridgeConfigInvalid(
            "github.private_key is empty".into(),
        ));
    }
    if let Some(path) = s.strip_prefix('@') {
        let expanded = shellexpand::tilde(path.trim());
        let path_buf = std::path::PathBuf::from(expanded.as_ref());
        std::fs::read_to_string(&path_buf).map_err(|e| {
            Error::BridgeConfigInvalid(format!(
                "github.private_key: cannot read '{}': {e}",
                path_buf.display()
            ))
        })
    } else {
        Ok(s.to_string())
    }
}

/// Build the production [`GhOps`] client from a parsed `[github]`
/// section. Called by `main.rs::run` (serve path) and
/// `selftest::run_selftest` (`--self-test` path).
pub fn build_gh_ops(g: &GitHubSection) -> Result<Arc<dyn GhOps>> {
    let mode = BridgeAuthMode::from_github_section(g)?;
    match mode {
        BridgeAuthMode::Pat(token) => {
            let ops = OctocrabGhOps::from_pat(token)?;
            Ok(Arc::new(ops))
        }
        BridgeAuthMode::App {
            app_id,
            private_key,
        } => {
            let pem = load_private_key(&private_key)?;
            let key = EncodingKey::from_rsa_pem(pem.as_bytes()).map_err(|e| {
                Error::GitHub(format!("github.private_key: invalid RSA PEM: {e}"))
            })?;
            let crab = Octocrab::builder()
                .app(AppId(app_id), key)
                .build()
                .map_err(|e| Error::GitHub(format!("octocrab App-mode build: {e}")))?;
            Ok(Arc::new(AppModeGhOps::new(Arc::new(crab))))
        }
    }
}

/// [`GhOps`] implementation backed by a GitHub App.
///
/// Holds the JWT-authenticated "bare app" client (`jwt`) that knows
/// which installations exist, and a per-owner cache of installation-
/// scoped clients (`cache`) used for repo-targeted operations. The
/// cache is owner-keyed because one App has exactly one installation
/// per owner; sibling repos under the same owner share the same client.
pub struct AppModeGhOps {
    jwt: Arc<Octocrab>,
    cache: RwLock<HashMap<String, Arc<Octocrab>>>,
}

impl AppModeGhOps {
    pub fn new(jwt: Arc<Octocrab>) -> Self {
        Self {
            jwt,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Get (or populate) the installation-scoped client for `owner/repo`.
    /// Subsequent calls for the same owner hit the read-lock fast path.
    async fn client_for(&self, owner: &str, repo: &str) -> Result<Arc<Octocrab>> {
        if let Some(cached) = self.cache.read().await.get(owner).cloned() {
            return Ok(cached);
        }
        // Cache miss: ask the App API which installation owns this repo.
        let installation = self
            .jwt
            .apps()
            .get_repository_installation(owner, repo)
            .await
            .map_err(|e| {
                Error::GitHub(format!(
                    "App mode: get_repository_installation({owner}/{repo}): {e}"
                ))
            })?;
        // `Octocrab::installation` is synchronous — it just clones the
        // builder configuration into an installation-scoped variant.
        // The actual installation-token round-trip happens on the first
        // request the scoped client makes.
        let scoped = Arc::new(self.jwt.installation(installation.id));
        // Take the write lock and re-check the slot: a concurrent caller
        // may have populated it while we were waiting on the API.
        let mut w = self.cache.write().await;
        let entry = w
            .entry(owner.to_string())
            .or_insert_with(|| scoped.clone());
        debug!(
            target: "gh:app",
            owner,
            installation_id = %installation.id,
            "cached installation-scoped client"
        );
        Ok(entry.clone())
    }

    fn split_repo(repo: &str) -> Result<(&str, &str)> {
        repo.split_once('/').ok_or_else(|| {
            Error::GitHub(format!("expected 'owner/repo', got '{repo}' (no slash)"))
        })
    }

    /// Build a per-call PAT-style wrapper around the scoped client so we
    /// inherit `OctocrabGhOps`'s already-tested error mapping (e.g. the
    /// 422 "already_exists" path for `ensure_label`). The wrap is one
    /// `Arc` clone; the underlying HTTP call still goes through the
    /// scoped client.
    fn scoped_ops(crab: Arc<Octocrab>) -> OctocrabGhOps {
        OctocrabGhOps::from_octocrab(crab)
    }
}

#[async_trait]
impl GhOps for AppModeGhOps {
    async fn ensure_label(
        &self,
        repo: &str,
        name: &str,
        color: &str,
        description: &str,
    ) -> Result<()> {
        let (owner, repo_name) = Self::split_repo(repo)?;
        let crab = self.client_for(owner, repo_name).await?;
        Self::scoped_ops(crab)
            .ensure_label(repo, name, color, description)
            .await
    }

    async fn apply_label_to_pr(&self, repo: &str, pr_number: u64, name: &str) -> Result<()> {
        let (owner, repo_name) = Self::split_repo(repo)?;
        let crab = self.client_for(owner, repo_name).await?;
        Self::scoped_ops(crab)
            .apply_label_to_pr(repo, pr_number, name)
            .await
    }

    async fn remove_label_from_pr(&self, repo: &str, pr_number: u64, name: &str) -> Result<()> {
        let (owner, repo_name) = Self::split_repo(repo)?;
        let crab = self.client_for(owner, repo_name).await?;
        Self::scoped_ops(crab)
            .remove_label_from_pr(repo, pr_number, name)
            .await
    }

    async fn post_pr_comment(&self, repo: &str, pr_number: u64, body: &str) -> Result<()> {
        let (owner, repo_name) = Self::split_repo(repo)?;
        let crab = self.client_for(owner, repo_name).await?;
        Self::scoped_ops(crab)
            .post_pr_comment(repo, pr_number, body)
            .await
    }

    async fn list_check_run_summary(
        &self,
        repo: &str,
        head_sha: &str,
    ) -> Result<CheckRunSummary> {
        let (owner, repo_name) = Self::split_repo(repo)?;
        let crab = self.client_for(owner, repo_name).await?;
        Self::scoped_ops(crab)
            .list_check_run_summary(repo, head_sha)
            .await
    }

    async fn whoami(&self) -> Result<String> {
        // For App mode the JWT-authenticated bare-app client knows the
        // App's own identity. Per-installation clients have no usable
        // `/user` endpoint, so we deliberately probe the App-scoped one.
        let value: serde_json::Value = self
            .jwt
            .get("/app", None::<&()>)
            .await
            .map_err(|e| Error::GitHub(format!("GET /app: {e}")))?;
        value
            .get("slug")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| Error::GitHub("GET /app: missing 'slug' field".into()))
    }
}

// ---------------------------------------------------------------------------
// Tests
//
// Unit-test scope: mode selection, PEM resolver, factory rejection of
// invalid input. End-to-end App-mode coverage (JWT → installation
// token → live REST call) is P1-H's responsibility via wiremock.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GitHubSection, LabelAliases};

    fn pat_only() -> GitHubSection {
        GitHubSection {
            webhook_secret: Some("shh".into()),
            pat: Some("ghp_test".into()),
            app_id: None,
            private_key: None,
            manage_labels: true,
            label_prefix: "sinfonia".into(),
            label_aliases: LabelAliases::default(),
        }
    }

    fn app_only() -> GitHubSection {
        GitHubSection {
            webhook_secret: Some("shh".into()),
            pat: None,
            app_id: Some(12345),
            private_key: Some("-----BEGIN RSA PRIVATE KEY-----\nfake\n-----END RSA PRIVATE KEY-----".into()),
            manage_labels: true,
            label_prefix: "sinfonia".into(),
            label_aliases: LabelAliases::default(),
        }
    }

    #[test]
    fn mode_pat_when_only_pat_set() {
        let g = pat_only();
        let mode = BridgeAuthMode::from_github_section(&g).expect("pat-only");
        assert!(matches!(mode, BridgeAuthMode::Pat(t) if t == "ghp_test"));
    }

    #[test]
    fn mode_app_when_app_id_and_key_set() {
        let g = app_only();
        let mode = BridgeAuthMode::from_github_section(&g).expect("app-only");
        match mode {
            BridgeAuthMode::App { app_id, .. } => assert_eq!(app_id, 12345),
            other => panic!("expected App, got {other:?}"),
        }
    }

    #[test]
    fn mode_app_without_private_key_errors() {
        let mut g = app_only();
        g.private_key = None;
        let err = BridgeAuthMode::from_github_section(&g).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("private_key")),
            "got: {err:?}"
        );
    }

    #[test]
    fn mode_neither_set_errors() {
        let mut g = pat_only();
        g.pat = None;
        let err = BridgeAuthMode::from_github_section(&g).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("either pat or app_id")),
        );
    }

    #[test]
    fn mode_both_set_errors() {
        let mut g = pat_only();
        g.app_id = Some(12345);
        g.private_key = Some("inline".into());
        let err = BridgeAuthMode::from_github_section(&g).unwrap_err();
        assert!(
            matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("mutually exclusive")),
        );
    }

    #[test]
    fn load_private_key_inline_passthrough() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nXYZ\n-----END RSA PRIVATE KEY-----";
        let out = load_private_key(pem).expect("inline pem");
        assert_eq!(out, pem);
    }

    #[test]
    fn load_private_key_inline_trims_whitespace() {
        let pem = "\n  -----BEGIN RSA PRIVATE KEY-----\nXYZ\n-----END RSA PRIVATE KEY-----  \n";
        let out = load_private_key(pem).expect("inline pem with whitespace");
        assert!(out.starts_with("-----BEGIN"));
        assert!(out.ends_with("-----"));
    }

    #[test]
    fn load_private_key_empty_errors() {
        let err = load_private_key("   ").unwrap_err();
        assert!(matches!(err, Error::BridgeConfigInvalid(ref s) if s.contains("empty")));
    }

    #[test]
    fn load_private_key_at_path_reads_file() {
        // tempfile is a dev-dep already.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key.pem");
        let body = "-----BEGIN RSA PRIVATE KEY-----\nfromfile\n-----END RSA PRIVATE KEY-----";
        std::fs::write(&path, body).expect("write fixture");
        let spec = format!("@{}", path.display());
        let out = load_private_key(&spec).expect("read from file");
        assert_eq!(out, body);
    }

    #[test]
    fn load_private_key_at_missing_file_errors() {
        let err = load_private_key("@/definitely/not/a/real/path.pem").unwrap_err();
        match err {
            Error::BridgeConfigInvalid(s) => {
                assert!(s.contains("/definitely/not/a/real/path.pem"));
                assert!(s.contains("cannot read"));
            }
            other => panic!("expected BridgeConfigInvalid, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn build_gh_ops_pat_mode_succeeds() {
        // `Octocrab::builder().personal_token(...).build()` internally
        // wires up a `tower::buffer::Buffer` that needs a live Tokio
        // reactor at construction time — hence `#[tokio::test]` here
        // even though we never make a real HTTP call.
        let g = pat_only();
        let ops = build_gh_ops(&g).expect("pat-mode build");
        // We can't easily introspect the concrete type behind the
        // `Arc<dyn GhOps>` boundary without downcasting; this assertion
        // just confirms the factory returned a live trait object.
        let _ = Arc::strong_count(&ops);
    }

    #[test]
    fn build_gh_ops_app_mode_with_bogus_pem_errors() {
        // Mode selection succeeds (validation rules are satisfied),
        // but EncodingKey::from_rsa_pem refuses the fake body. This
        // verifies the error path lands inside `build_gh_ops` rather
        // than panicking deep in octocrab.
        //
        // `Arc<dyn GhOps>` does not implement `Debug`, so we can't use
        // `unwrap_err()` here — destructure the `Result` explicitly.
        let g = app_only();
        match build_gh_ops(&g) {
            Ok(_) => panic!("expected build_gh_ops to reject the fake PEM"),
            Err(Error::GitHub(s)) => assert!(
                s.contains("invalid RSA PEM"),
                "expected 'invalid RSA PEM' in error, got: {s}"
            ),
            Err(other) => panic!("expected Error::GitHub, got: {other:?}"),
        }
    }
}
