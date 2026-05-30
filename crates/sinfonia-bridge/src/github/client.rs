//! GitHub HTTP operations the bridge performs.
//!
//! The trait is tiny on purpose: every method maps to one octocrab
//! call. Anything fancier (per-installation caches, ETag handling,
//! retries) belongs above this layer or in P1-G's App-mode work.

use crate::{Error, Result};
use async_trait::async_trait;
use octocrab::models::{ArtifactId, RunId};
use octocrab::params::actions::ArchiveFormat;
use octocrab::params::repos::Commitish;
use octocrab::Octocrab;
use std::sync::Arc;
use tracing::{debug, warn};

/// Outcome of a single CI check run (one row in the GitHub Checks tab).
#[derive(Debug, Clone)]
pub struct CheckRunOutcome {
    pub name: String,
    /// Raw conclusion string from the API: `success` / `failure` /
    /// `neutral` / `cancelled` / `timed_out` / `action_required` /
    /// `skipped` / `stale`.
    pub conclusion: String,
}

impl CheckRunOutcome {
    /// True for conclusions that should NOT count as a passing build.
    /// Mirrors the proposal's "all_passed" semantics in §5.2 step 4.
    pub fn is_failed(&self) -> bool {
        matches!(
            self.conclusion.as_str(),
            "failure" | "cancelled" | "timed_out" | "action_required"
        )
    }
}

/// Aggregated summary of every check run on a head SHA. Computed once
/// per webhook so the orchestrator can pattern-match a single value.
#[derive(Debug, Clone, Default)]
pub struct CheckRunSummary {
    /// Names of checks that completed with a non-pass conclusion.
    pub failed: Vec<String>,
    /// Names of checks that completed with a pass-equivalent conclusion.
    pub passed: Vec<String>,
    /// True iff at least one check is still queued or in_progress. When
    /// set, the orchestrator returns "wait" — the next `check_suite`
    /// event will re-evaluate.
    pub any_pending: bool,
}

impl CheckRunSummary {
    pub fn all_passed(&self) -> bool {
        !self.any_pending && self.failed.is_empty() && !self.passed.is_empty()
    }

    pub fn has_failed(&self) -> bool {
        !self.failed.is_empty()
    }
}

/// One row of the GitHub Actions artifacts listing for a workflow run.
///
/// Deliberately a tiny owned projection of octocrab's
/// `WorkflowListArtifact` — the harness-manifest pipeline (Proposal 0001)
/// only needs the name (for glob matching) and the declared size (for the
/// pre-download cap), and keeping the trait free of octocrab types lets
/// the feedback tests fake it without the dependency.
#[derive(Debug, Clone)]
pub struct ArtifactMeta {
    pub id: u64,
    pub name: String,
    pub size_in_bytes: u64,
}

/// Operations the bridge needs against the GitHub REST API.
///
/// Implementations MUST be cheap to clone (the orchestrator holds an
/// `Arc<dyn GhOps>` in `AppState` and clones it across handlers).
#[async_trait]
pub trait GhOps: Send + Sync {
    /// Idempotently create a repo-scoped label. The default GitHub
    /// behaviour rejects duplicates with 422 — implementations should
    /// translate that into `Ok(())` so the caller can run this on
    /// every startup without coordinating.
    async fn ensure_label(
        &self,
        repo: &str,
        name: &str,
        color: &str,
        description: &str,
    ) -> Result<()>;

    /// Add a single label to a PR (which the API models as an issue).
    /// Re-applying an already-present label is a 200 no-op on GitHub
    /// side; we just return `Ok(())`.
    async fn apply_label_to_pr(&self, repo: &str, pr_number: u64, name: &str) -> Result<()>;

    /// Remove a label from a PR. A 404 (label not present on the PR)
    /// is translated into `Ok(())`.
    async fn remove_label_from_pr(&self, repo: &str, pr_number: u64, name: &str) -> Result<()>;

    /// Post a comment on a PR.
    async fn post_pr_comment(&self, repo: &str, pr_number: u64, body: &str) -> Result<()>;

    /// Aggregate every check run for a head SHA into a [`CheckRunSummary`].
    async fn list_check_run_summary(&self, repo: &str, head_sha: &str)
        -> Result<CheckRunSummary>;

    /// Return a short identity label for the authenticated client. PAT
    /// mode returns the user's `login`; App mode returns the App's
    /// `slug`. Used by `sinfonia-bridge --self-test` to surface the
    /// effective identity in its `PASS  github:` line, and to actually
    /// exercise the credential (a wrong PAT fails the API call here).
    async fn whoami(&self) -> Result<String>;

    /// List the artifacts produced by a completed workflow run
    /// (`GET /repos/{repo}/actions/runs/{run_id}/artifacts`). Used by the
    /// harness-manifest ingestion path (Proposal 0001) to find the
    /// `bridge-*` bundle before downloading it. Returns the artifacts as
    /// a flat list of [`ArtifactMeta`]; pagination beyond the first page
    /// is intentionally not followed — a run emits a handful of artifacts.
    async fn list_run_artifacts(&self, repo: &str, run_id: u64) -> Result<Vec<ArtifactMeta>>;

    /// Download an artifact's zip archive by id, capped at `max_bytes`.
    /// Returns the raw zip bytes. When the downloaded body exceeds
    /// `max_bytes` this returns `Err` (not `Ok`-empty) so the caller can
    /// log and fall back distinctly from an empty/zero-entry artifact —
    /// the resource-exhaustion control in Proposal 0001 §5.
    async fn download_artifact(
        &self,
        repo: &str,
        artifact_id: u64,
        max_bytes: u64,
    ) -> Result<Vec<u8>>;
}

/// Production [`GhOps`] backed by `octocrab` in PAT mode.
///
/// One client per bridge process. P1-G adds App-mode and a per-
/// installation cache.
pub struct OctocrabGhOps {
    crab: Arc<Octocrab>,
}

impl OctocrabGhOps {
    /// Build a PAT-mode client. `token` is the resolved PAT (env-var
    /// indirection has already been applied at config-parse time).
    pub fn from_pat(token: impl Into<String>) -> Result<Self> {
        let crab = Octocrab::builder()
            .personal_token(token.into())
            .build()
            .map_err(|e| Error::GitHub(format!("octocrab build: {e}")))?;
        Ok(Self {
            crab: Arc::new(crab),
        })
    }

    /// Build from a pre-constructed Octocrab. Used by P1-G when the
    /// client is App-installed.
    pub fn from_octocrab(crab: Arc<Octocrab>) -> Self {
        Self { crab }
    }

    fn split_repo(repo: &str) -> Result<(&str, &str)> {
        repo.split_once('/').ok_or_else(|| {
            Error::GitHub(format!(
                "expected 'owner/repo', got '{repo}' (no slash)"
            ))
        })
    }
}

#[async_trait]
impl GhOps for OctocrabGhOps {
    async fn ensure_label(
        &self,
        repo: &str,
        name: &str,
        color: &str,
        description: &str,
    ) -> Result<()> {
        let (owner, name_repo) = Self::split_repo(repo)?;
        let issues = self.crab.issues(owner, name_repo);
        match issues.create_label(name, color, description).await {
            Ok(_) => {
                debug!(target: "gh", repo, name, "ensured label (created)");
                Ok(())
            }
            Err(e) => {
                // 422 here is "already_exists" — that's the success path
                // for an idempotent ensure. octocrab buries the status
                // inside the error, so we match by message; this is
                // imperfect but adequate (and P1-H will exercise it).
                let msg = e.to_string();
                if msg.contains("already_exists") || msg.contains("422") {
                    debug!(target: "gh", repo, name, "ensured label (already exists)");
                    Ok(())
                } else {
                    warn!(target: "gh", repo, name, error = %e, "create_label failed");
                    Err(Error::GitHub(format!("create_label '{name}': {e}")))
                }
            }
        }
    }

    async fn apply_label_to_pr(&self, repo: &str, pr_number: u64, name: &str) -> Result<()> {
        let (owner, name_repo) = Self::split_repo(repo)?;
        let issues = self.crab.issues(owner, name_repo);
        match issues
            .add_labels(pr_number, &[name.to_string()])
            .await
        {
            Ok(_) => {
                debug!(target: "gh", repo, pr_number, name, "applied label");
                Ok(())
            }
            Err(e) => {
                warn!(target: "gh", repo, pr_number, name, error = %e, "add_labels failed");
                Err(Error::GitHub(format!(
                    "add_labels pr={pr_number} '{name}': {e}"
                )))
            }
        }
    }

    async fn remove_label_from_pr(&self, repo: &str, pr_number: u64, name: &str) -> Result<()> {
        let (owner, name_repo) = Self::split_repo(repo)?;
        let issues = self.crab.issues(owner, name_repo);
        match issues.remove_label(pr_number, name).await {
            Ok(_) => {
                debug!(target: "gh", repo, pr_number, name, "removed label");
                Ok(())
            }
            Err(e) => {
                // 404 here means "label not present" — that's the
                // success path for an idempotent remove. Same caveat
                // as ensure_label: message-matching is best-effort.
                let msg = e.to_string();
                if msg.contains("404") || msg.contains("Not Found") {
                    debug!(target: "gh", repo, pr_number, name, "remove_label 404; treated as success");
                    Ok(())
                } else {
                    warn!(target: "gh", repo, pr_number, name, error = %e, "remove_label failed");
                    Err(Error::GitHub(format!(
                        "remove_label pr={pr_number} '{name}': {e}"
                    )))
                }
            }
        }
    }

    async fn post_pr_comment(&self, repo: &str, pr_number: u64, body: &str) -> Result<()> {
        let (owner, name_repo) = Self::split_repo(repo)?;
        let issues = self.crab.issues(owner, name_repo);
        issues
            .create_comment(pr_number, body)
            .await
            .map(|_| ())
            .map_err(|e| Error::GitHub(format!("create_comment pr={pr_number}: {e}")))
    }

    async fn whoami(&self) -> Result<String> {
        // PAT-mode clients authenticate as a real user.
        let value: serde_json::Value = self
            .crab
            .get("/user", None::<&()>)
            .await
            .map_err(|e| Error::GitHub(format!("GET /user: {e}")))?;
        value
            .get("login")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| Error::GitHub("GET /user: missing 'login' field".into()))
    }

    async fn list_check_run_summary(
        &self,
        repo: &str,
        head_sha: &str,
    ) -> Result<CheckRunSummary> {
        let (owner, name_repo) = Self::split_repo(repo)?;
        let checks = self.crab.checks(owner, name_repo);
        // Single page; v0.3 traffic doesn't justify a paginator yet, but
        // we ask for the max page size so a moderately-busy PR fits.
        let page = checks
            .list_check_runs_for_git_ref(Commitish(head_sha.to_string()))
            .per_page(100u8)
            .send()
            .await
            .map_err(|e| {
                Error::GitHub(format!("list_check_runs_for_git_ref {head_sha}: {e}"))
            })?;

        let mut summary = CheckRunSummary::default();
        for run in page.check_runs {
            // octocrab 0.39's `CheckRun` exposes `conclusion: Option<String>`
            // but not `status`. We treat `conclusion = None` as "still
            // pending" — the next `check_suite` redelivery will retrigger
            // the evaluation. (The GitHub API contract: `conclusion` is
            // only set when `status == "completed"`.)
            let Some(conclusion) = run.conclusion else {
                summary.any_pending = true;
                continue;
            };
            let outcome = CheckRunOutcome {
                name: run.name,
                conclusion: conclusion.to_ascii_lowercase(),
            };
            if outcome.is_failed() {
                summary.failed.push(outcome.name);
            } else {
                summary.passed.push(outcome.name);
            }
        }
        Ok(summary)
    }

    async fn list_run_artifacts(&self, repo: &str, run_id: u64) -> Result<Vec<ArtifactMeta>> {
        let (owner, name_repo) = Self::split_repo(repo)?;
        // First page only; a CI run emits a small, fixed set of artifacts
        // and we just need to find one by name. `per_page(100)` keeps the
        // glob match exhaustive in practice without a paginator.
        let page = self
            .crab
            .actions()
            .list_workflow_run_artifacts(owner, name_repo, RunId(run_id))
            .per_page(100u8)
            .send()
            .await
            .map_err(|e| {
                Error::GitHub(format!("list_workflow_run_artifacts run={run_id}: {e}"))
            })?;
        // `value` is `None` only on a 304 Not Modified, which we never
        // request (no `If-None-Match`); treat the absent case as empty.
        let artifacts = page
            .value
            .map(|p| p.items)
            .unwrap_or_default()
            .into_iter()
            .map(|a| ArtifactMeta {
                id: a.id.0,
                name: a.name,
                size_in_bytes: a.size_in_bytes as u64,
            })
            .collect();
        Ok(artifacts)
    }

    async fn download_artifact(
        &self,
        repo: &str,
        artifact_id: u64,
        max_bytes: u64,
    ) -> Result<Vec<u8>> {
        let (owner, name_repo) = Self::split_repo(repo)?;
        // octocrab buffers the whole body into `Bytes` (no streaming hook),
        // so the cap is enforced on the materialized length. The caller is
        // expected to additionally pre-check `ArtifactMeta::size_in_bytes`
        // from the listing so the common oversize case never downloads;
        // this is the belt-and-suspenders check against a lying header.
        let bytes = self
            .crab
            .actions()
            .download_artifact(owner, name_repo, ArtifactId(artifact_id), ArchiveFormat::Zip)
            .await
            .map_err(|e| {
                Error::GitHub(format!("download_artifact id={artifact_id}: {e}"))
            })?;
        if bytes.len() as u64 > max_bytes {
            return Err(Error::GitHub(format!(
                "artifact id={artifact_id} is {} bytes, over the {max_bytes}-byte cap",
                bytes.len()
            )));
        }
        Ok(bytes.to_vec())
    }
}

// ---------------------------------------------------------------------------
// Tests — artifacts access (Proposal 0001 Task 1). Backed by wiremock so no
// live network is touched; an `Octocrab` is retargeted at the mock via
// `base_uri`, exactly as the integration tests do.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ops_for(uri: &str) -> OctocrabGhOps {
        let crab = Octocrab::builder()
            .personal_token("ghp_test".to_string())
            .base_uri(uri)
            .expect("base_uri")
            .build()
            .expect("octocrab build");
        OctocrabGhOps::from_octocrab(Arc::new(crab))
    }

    fn artifact_json(id: u64, name: &str, size: u64) -> serde_json::Value {
        json!({
            "id": id,
            "node_id": format!("MDg6QXJ0aWZhY3R7}}{id}"),
            "name": name,
            "size_in_bytes": size,
            "url": format!("https://api.github.test/repos/acme/widgets/actions/artifacts/{id}"),
            "archive_download_url": format!("https://api.github.test/repos/acme/widgets/actions/artifacts/{id}/zip"),
            "expired": false,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "expires_at": "2099-01-01T00:00:00Z",
        })
    }

    #[tokio::test]
    async fn list_run_artifacts_maps_synthetic_list() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/repos/[^/]+/[^/]+/actions/runs/[0-9]+/artifacts$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 2,
                "artifacts": [
                    artifact_json(11, "bridge-1820934", 4096),
                    artifact_json(12, "coverage", 200),
                ],
            })))
            .mount(&server)
            .await;

        let ops = ops_for(&server.uri());
        let arts = ops
            .list_run_artifacts("acme/widgets", 1820934)
            .await
            .expect("list ok");
        assert_eq!(arts.len(), 2);
        assert_eq!(arts[0].id, 11);
        assert_eq!(arts[0].name, "bridge-1820934");
        assert_eq!(arts[0].size_in_bytes, 4096);
        assert_eq!(arts[1].name, "coverage");
    }

    #[tokio::test]
    async fn download_artifact_under_cap_returns_bytes() {
        let server = MockServer::start().await;
        let body = b"PK\x03\x04 not-a-real-zip but bytes".to_vec();
        Mock::given(method("GET"))
            .and(path_regex(r"^/repos/[^/]+/[^/]+/actions/artifacts/[0-9]+/zip$"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;

        let ops = ops_for(&server.uri());
        let got = ops
            .download_artifact("acme/widgets", 11, 1024)
            .await
            .expect("download ok");
        assert_eq!(got, body);
    }

    #[tokio::test]
    async fn download_artifact_over_cap_errors() {
        let server = MockServer::start().await;
        let body = vec![0u8; 2048];
        Mock::given(method("GET"))
            .and(path_regex(r"^/repos/[^/]+/[^/]+/actions/artifacts/[0-9]+/zip$"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;

        let ops = ops_for(&server.uri());
        let err = ops
            .download_artifact("acme/widgets", 11, 1024)
            .await
            .expect_err("should exceed cap");
        assert!(
            matches!(err, Error::GitHub(ref s) if s.contains("over the 1024-byte cap")),
            "expected cap error, got: {err:?}"
        );
    }
}
