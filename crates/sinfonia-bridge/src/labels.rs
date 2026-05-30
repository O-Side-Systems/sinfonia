//! PR-label management (plan §7).
//!
//! Two halves:
//!
//! 1. [`BridgeLabel`] — the closed set of labels the bridge manages, plus
//!    the pure [`BridgeLabel::full_name`] resolver that applies the
//!    "alias wins verbatim" rule from `01-bridge-mvp.md` §7 (H-4). The
//!    composition is pure so unit tests don't need an HTTP client.
//!
//! 2. [`LabelManager`] — a thin wrapper around a [`GhOps`] client that
//!    short-circuits every call when `manage_labels: false` is set in
//!    `BRIDGE.md`. The short-circuit lives here, not in the github
//!    client, so the github layer stays a dumb transport.
//!
//! See `01-bridge-mvp.md` §7 and the H-4 resolution notes in §3 of the
//! same plan for the canonical contract.

use crate::config::LabelAliases;
use crate::github::GhOps;
use crate::Result;
use std::sync::Arc;
use tracing::{debug, warn};

/// The closed set of labels the bridge manages on a PR.
///
/// Variants other than [`BridgeLabel::Failure`] map 1-to-1 to a fixed
/// base name. `Failure(category)` composes its name from the configured
/// failure prefix (default `<prefix>:failure`) plus the category slug.
///
/// `BudgetExceeded` is applied by the budget-enforcement pipeline
/// (`crates/sinfonia-bridge/src/feedback/budget.rs`; see `docs/SPEC.md`
/// §11.6.12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeLabel {
    InProgress,
    AwaitingReview,
    NeedsFixes,
    CapHit,
    BudgetExceeded,
    Failure(String),
}

impl BridgeLabel {
    /// Resolve the full GitHub label name.
    ///
    /// "Verbatim alias" semantics (plan §7, H-4): when an alias is set,
    /// it is the full label name — `prefix` is NOT prepended. When no
    /// alias is set, the label is `"<prefix>:<base>"`.
    ///
    /// For [`BridgeLabel::Failure`], the `failure_prefix` alias overrides
    /// the leading portion (`"<prefix>:failure"` by default); the
    /// category slug is always appended.
    pub fn full_name(&self, prefix: &str, aliases: &LabelAliases) -> String {
        match self {
            Self::InProgress => aliases
                .in_progress
                .clone()
                .unwrap_or_else(|| format!("{prefix}:in-progress")),
            Self::AwaitingReview => aliases
                .awaiting_review
                .clone()
                .unwrap_or_else(|| format!("{prefix}:awaiting-review")),
            Self::NeedsFixes => aliases
                .needs_fixes
                .clone()
                .unwrap_or_else(|| format!("{prefix}:needs-fixes")),
            Self::CapHit => aliases
                .cap_hit
                .clone()
                .unwrap_or_else(|| format!("{prefix}:cap-hit")),
            Self::BudgetExceeded => aliases
                .budget_exceeded
                .clone()
                .unwrap_or_else(|| format!("{prefix}:budget-exceeded")),
            Self::Failure(cat) => {
                let head = aliases
                    .failure_prefix
                    .clone()
                    .unwrap_or_else(|| format!("{prefix}:failure"));
                format!("{head}:{cat}")
            }
        }
    }

    /// Default GitHub label color (six-hex, no `#`). Colors chosen to
    /// stay readable against both light and dark UI themes.
    pub fn default_color(&self) -> &'static str {
        match self {
            Self::InProgress => "1f6feb",      // blue
            Self::AwaitingReview => "0e8a16",  // green
            Self::NeedsFixes => "fbca04",      // yellow
            Self::CapHit => "d73a4a",          // red
            Self::BudgetExceeded => "b60205",  // deeper red
            Self::Failure(_) => "eeeeee",      // neutral; the category prefix already conveys intent
        }
    }

    /// Human-readable description applied to the GitHub label.
    pub fn description(&self) -> &'static str {
        match self {
            Self::InProgress => "Sinfonia agent is working on this PR.",
            Self::AwaitingReview => "CI is green; awaiting human review.",
            Self::NeedsFixes => "CI failed; Sinfonia will retry.",
            Self::CapHit => "Attempt cap hit; routed to human review.",
            Self::BudgetExceeded => "Budget cap exceeded; routed to human review.",
            Self::Failure(_) => "Failure category tag (set by Sinfonia bridge).",
        }
    }

    /// The closed set of base labels that should be pre-created on the
    /// repo at startup (the `Failure(_)` family is created lazily as
    /// categories are seen).
    pub fn base_set() -> [BridgeLabel; 5] {
        [
            Self::InProgress,
            Self::AwaitingReview,
            Self::NeedsFixes,
            Self::CapHit,
            Self::BudgetExceeded,
        ]
    }
}

// ---------------------------------------------------------------------------
// LabelManager — short-circuits on `manage_labels: false`
// ---------------------------------------------------------------------------

/// Per-bridge label-management state. Built once at startup and held in
/// `AppState` for the lifetime of the process.
///
/// All `apply` / `remove` / `ensure` calls are idempotent and silently
/// no-op when `manage_labels` is `false`. The bridge logs the disabled
/// status once at startup (`main.rs::run`) so operators see it without
/// the orchestrator having to log per-call.
#[derive(Clone)]
pub struct LabelManager {
    gh: Arc<dyn GhOps>,
    enabled: bool,
    prefix: String,
    aliases: LabelAliases,
}

impl LabelManager {
    pub fn new(
        gh: Arc<dyn GhOps>,
        enabled: bool,
        prefix: impl Into<String>,
        aliases: LabelAliases,
    ) -> Self {
        Self {
            gh,
            enabled,
            prefix: prefix.into(),
            aliases,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Compose the full label name without issuing any HTTP. Exposed so
    /// the orchestrator can put the resolved name into log lines and
    /// response bodies even when labels are disabled.
    pub fn full_name(&self, label: &BridgeLabel) -> String {
        label.full_name(&self.prefix, &self.aliases)
    }

    /// Idempotently ensure every base label exists on `repo`. Lazy
    /// invariant: `Failure(_)` labels are created on first use by
    /// [`ensure_failure`].
    pub async fn ensure_base_set(&self, repo: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        for label in BridgeLabel::base_set() {
            let name = self.full_name(&label);
            self.ensure_one(repo, &name, label.default_color(), label.description())
                .await?;
        }
        Ok(())
    }

    /// Idempotently ensure a single `Failure(<category>)` label exists.
    pub async fn ensure_failure(&self, repo: &str, category: &str) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let label = BridgeLabel::Failure(category.to_string());
        let name = self.full_name(&label);
        self.ensure_one(repo, &name, label.default_color(), label.description())
            .await
    }

    async fn ensure_one(
        &self,
        repo: &str,
        name: &str,
        color: &str,
        description: &str,
    ) -> Result<()> {
        match self.gh.ensure_label(repo, name, color, description).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // ensure is best-effort: GitHub will reject a duplicate
                // create, and we don't want a transient hiccup to take
                // the bridge offline. Log and continue.
                warn!(target: "labels", repo, name, error = %e, "ensure_label failed (continuing)");
                Ok(())
            }
        }
    }

    /// Apply `label` to PR `pr_number` in `repo`. No-op when
    /// `manage_labels: false`.
    pub async fn apply(&self, repo: &str, pr_number: u64, label: &BridgeLabel) -> Result<()> {
        if !self.enabled {
            debug!(target: "labels", repo, pr_number, ?label, "manage_labels=false; apply skipped");
            return Ok(());
        }
        // For Failure(_), ensure the label exists before we try to apply
        // it — GitHub silently creates issue labels on `add_labels`
        // anyway, but pre-creation gives the label a stable color/
        // description.
        if let BridgeLabel::Failure(cat) = label {
            self.ensure_failure(repo, cat).await?;
        }
        let name = self.full_name(label);
        self.gh.apply_label_to_pr(repo, pr_number, &name).await
    }

    /// Remove `label` from PR `pr_number`. The github client treats a
    /// 404 (label not present) as success.
    pub async fn remove(&self, repo: &str, pr_number: u64, label: &BridgeLabel) -> Result<()> {
        if !self.enabled {
            debug!(target: "labels", repo, pr_number, ?label, "manage_labels=false; remove skipped");
            return Ok(());
        }
        let name = self.full_name(label);
        self.gh.remove_label_from_pr(repo, pr_number, &name).await
    }
}

// ---------------------------------------------------------------------------
// Tests (plan §9.1 table — `labels` row)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::GhOps;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // -- Pure name composition (no client needed) -----------------------

    #[test]
    fn label_name_uses_prefix_when_no_alias() {
        let aliases = LabelAliases::default();
        assert_eq!(
            BridgeLabel::InProgress.full_name("sinfonia", &aliases),
            "sinfonia:in-progress"
        );
        assert_eq!(
            BridgeLabel::AwaitingReview.full_name("automation", &aliases),
            "automation:awaiting-review"
        );
    }

    #[test]
    fn label_alias_supplies_full_name_verbatim() {
        // H-4: alias values are full label names; the prefix is ignored.
        let aliases = LabelAliases {
            in_progress: Some("ai:working".to_string()),
            ..LabelAliases::default()
        };
        assert_eq!(
            BridgeLabel::InProgress.full_name("sinfonia", &aliases),
            "ai:working"
        );
        // The prefix MUST be ignored even when it would normally apply.
        assert_eq!(
            BridgeLabel::InProgress.full_name("anything", &aliases),
            "ai:working"
        );
    }

    #[test]
    fn failure_label_composes_with_category_suffix() {
        let aliases = LabelAliases::default();
        assert_eq!(
            BridgeLabel::Failure("e2e".into()).full_name("sinfonia", &aliases),
            "sinfonia:failure:e2e"
        );
        // failure_prefix alias replaces only the head — the category
        // suffix still applies.
        let with_alias = LabelAliases {
            failure_prefix: Some("ai:fail".to_string()),
            ..LabelAliases::default()
        };
        assert_eq!(
            BridgeLabel::Failure("lint".into()).full_name("sinfonia", &with_alias),
            "ai:fail:lint"
        );
    }

    #[test]
    fn base_set_covers_all_non_failure_variants() {
        let names: Vec<_> = BridgeLabel::base_set()
            .iter()
            .map(|l| l.full_name("sinfonia", &LabelAliases::default()))
            .collect();
        assert!(names.contains(&"sinfonia:in-progress".to_string()));
        assert!(names.contains(&"sinfonia:awaiting-review".to_string()));
        assert!(names.contains(&"sinfonia:needs-fixes".to_string()));
        assert!(names.contains(&"sinfonia:cap-hit".to_string()));
        assert!(names.contains(&"sinfonia:budget-exceeded".to_string()));
        assert_eq!(names.len(), 5, "base_set should not include Failure(_)");
    }

    // -- LabelManager short-circuit on `manage_labels: false` -----------
    //
    // The plan §9.1 row for `labels` explicitly calls for: "manage_labels:
    // false short-circuits both `apply` and `ensure_labels`". We assert
    // call-count zero via a counting GhOps fake.

    #[derive(Default)]
    struct CountingGh {
        ensures: AtomicUsize,
        applies: AtomicUsize,
        removes: AtomicUsize,
        posts: AtomicUsize,
        checks: AtomicUsize,
    }

    #[async_trait]
    impl GhOps for CountingGh {
        async fn ensure_label(
            &self,
            _repo: &str,
            _name: &str,
            _color: &str,
            _description: &str,
        ) -> Result<()> {
            self.ensures.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn apply_label_to_pr(
            &self,
            _repo: &str,
            _pr_number: u64,
            _name: &str,
        ) -> Result<()> {
            self.applies.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn remove_label_from_pr(
            &self,
            _repo: &str,
            _pr_number: u64,
            _name: &str,
        ) -> Result<()> {
            self.removes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn post_pr_comment(
            &self,
            _repo: &str,
            _pr_number: u64,
            _body: &str,
        ) -> Result<()> {
            self.posts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn list_check_run_summary(
            &self,
            _repo: &str,
            _head_sha: &str,
        ) -> Result<crate::github::CheckRunSummary> {
            self.checks.fetch_add(1, Ordering::SeqCst);
            Ok(crate::github::CheckRunSummary::default())
        }
        async fn whoami(&self) -> Result<String> {
            Ok("counting-fake".into())
        }
        async fn list_run_artifacts(
            &self,
            _repo: &str,
            _run_id: u64,
        ) -> Result<Vec<crate::github::ArtifactMeta>> {
            Ok(vec![])
        }
        async fn download_artifact(
            &self,
            _repo: &str,
            _artifact_id: u64,
            _max_bytes: u64,
        ) -> Result<Vec<u8>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn manage_labels_false_short_circuits_ensure_apply_remove() {
        let gh = Arc::new(CountingGh::default());
        let mgr = LabelManager::new(
            gh.clone(),
            /* enabled = */ false,
            "sinfonia",
            LabelAliases::default(),
        );
        mgr.ensure_base_set("acme/widgets").await.expect("ensure");
        mgr.ensure_failure("acme/widgets", "e2e")
            .await
            .expect("ensure_failure");
        mgr.apply("acme/widgets", 1, &BridgeLabel::AwaitingReview)
            .await
            .expect("apply");
        mgr.remove("acme/widgets", 1, &BridgeLabel::InProgress)
            .await
            .expect("remove");

        assert_eq!(gh.ensures.load(Ordering::SeqCst), 0);
        assert_eq!(gh.applies.load(Ordering::SeqCst), 0);
        assert_eq!(gh.removes.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn manage_labels_true_invokes_client() {
        let gh = Arc::new(CountingGh::default());
        let mgr = LabelManager::new(
            gh.clone(),
            true,
            "sinfonia",
            LabelAliases::default(),
        );
        mgr.ensure_base_set("acme/widgets").await.expect("ensure");
        mgr.apply("acme/widgets", 1, &BridgeLabel::AwaitingReview)
            .await
            .expect("apply");
        mgr.remove("acme/widgets", 1, &BridgeLabel::InProgress)
            .await
            .expect("remove");

        // 5 base labels created at startup.
        assert_eq!(gh.ensures.load(Ordering::SeqCst), 5);
        assert_eq!(gh.applies.load(Ordering::SeqCst), 1);
        assert_eq!(gh.removes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn apply_failure_creates_label_lazily() {
        let gh = Arc::new(CountingGh::default());
        let mgr = LabelManager::new(gh.clone(), true, "sinfonia", LabelAliases::default());
        mgr.apply("acme/widgets", 1, &BridgeLabel::Failure("e2e".into()))
            .await
            .expect("apply");
        // ensure() called for the new failure label, then apply().
        assert_eq!(gh.ensures.load(Ordering::SeqCst), 1);
        assert_eq!(gh.applies.load(Ordering::SeqCst), 1);
    }
}
