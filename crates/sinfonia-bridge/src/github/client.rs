//! GitHub HTTP operations the bridge performs.
//!
//! The trait is tiny on purpose: every method maps to one octocrab
//! call. Anything fancier (per-installation caches, ETag handling,
//! retries) belongs above this layer or in P1-G's App-mode work.

use crate::{Error, Result};
use async_trait::async_trait;
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
}
