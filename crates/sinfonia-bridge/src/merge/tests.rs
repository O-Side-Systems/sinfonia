//! Unit tests for the merge coordinator (Proposal 0005 Phase 3).
//!
//! Backed by an in-memory [`Store`], a scriptable [`FakeGh`] whose
//! `get_pull_request` returns a scripted sequence of states (so a landing can
//! evolve `behind → clean` across calls), and a [`RecordingTracker`] that
//! captures the park-path transition. No network is touched.

use super::*;
use crate::config::{parse_bridge_str, LabelAliases};
use crate::github::{CheckRunSummary, GhOps, MergeOutcome, PrLanding, UpdateBranchOutcome};
use crate::labels::LabelManager;
use crate::storage::Store;
use async_trait::async_trait;
use sinfonia_tracker::{
    CustomFieldSchema, CustomFieldValue, Issue, IssueState, IssueTracker, Result as TrackerResult,
};
use std::sync::Mutex as StdMutex;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

fn pr(merged: bool, state: &str, mergeable_state: &str, head: &str) -> PrLanding {
    PrLanding {
        head_sha: head.into(),
        base_ref: "main".into(),
        merged,
        state: state.into(),
        mergeable_state: mergeable_state.into(),
        mergeable: Some(true),
    }
}

/// Scriptable GitHub client. `get_pull_request` pops the next scripted
/// `PrLanding` (repeating the last once exhausted); `update_pr_branch` and
/// `merge_pr` return configured outcomes and record their calls.
struct FakeGh {
    states: StdMutex<std::collections::VecDeque<PrLanding>>,
    summary: CheckRunSummary,
    update_outcome: UpdateBranchOutcome,
    merge_outcome: MergeOutcome,
    update_calls: StdMutex<Vec<(u64, String)>>,
    merge_calls: StdMutex<Vec<(u64, String, String)>>,
    comments: StdMutex<Vec<(u64, String)>>,
}

impl FakeGh {
    fn new(states: Vec<PrLanding>) -> Self {
        Self {
            states: StdMutex::new(states.into()),
            summary: CheckRunSummary {
                failed: vec![],
                passed: vec!["ci".into()],
                any_pending: false,
            },
            update_outcome: UpdateBranchOutcome::Accepted,
            merge_outcome: MergeOutcome::Merged {
                sha: "mergedsha".into(),
            },
            update_calls: StdMutex::new(vec![]),
            merge_calls: StdMutex::new(vec![]),
            comments: StdMutex::new(vec![]),
        }
    }
    fn with_summary(mut self, s: CheckRunSummary) -> Self {
        self.summary = s;
        self
    }
    fn with_update(mut self, o: UpdateBranchOutcome) -> Self {
        self.update_outcome = o;
        self
    }
}

#[async_trait]
impl GhOps for FakeGh {
    async fn ensure_label(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
        Ok(())
    }
    async fn apply_label_to_pr(&self, _: &str, _: u64, _: &str) -> Result<()> {
        Ok(())
    }
    async fn remove_label_from_pr(&self, _: &str, _: u64, _: &str) -> Result<()> {
        Ok(())
    }
    async fn post_pr_comment(&self, _: &str, pr_number: u64, body: &str) -> Result<()> {
        self.comments.lock().unwrap().push((pr_number, body.into()));
        Ok(())
    }
    async fn list_check_run_summary(&self, _: &str, _: &str) -> Result<CheckRunSummary> {
        Ok(self.summary.clone())
    }
    async fn whoami(&self) -> Result<String> {
        Ok("fake".into())
    }
    async fn list_run_artifacts(&self, _: &str, _: u64) -> Result<Vec<crate::github::ArtifactMeta>> {
        Ok(vec![])
    }
    async fn download_artifact(&self, _: &str, _: u64, _: u64) -> Result<Vec<u8>> {
        Ok(vec![])
    }
    async fn get_pull_request(&self, _: &str, _: u64) -> Result<PrLanding> {
        let mut q = self.states.lock().unwrap();
        if q.len() > 1 {
            Ok(q.pop_front().unwrap())
        } else {
            // Repeat the last scripted state forever.
            Ok(q.front().cloned().expect("FakeGh: no scripted PR state"))
        }
    }
    async fn update_pr_branch(
        &self,
        _: &str,
        pr_number: u64,
        expected_head_sha: &str,
    ) -> Result<UpdateBranchOutcome> {
        self.update_calls
            .lock()
            .unwrap()
            .push((pr_number, expected_head_sha.into()));
        Ok(self.update_outcome.clone())
    }
    async fn merge_pr(
        &self,
        _: &str,
        pr_number: u64,
        method: &str,
        head_sha: &str,
    ) -> Result<MergeOutcome> {
        self.merge_calls
            .lock()
            .unwrap()
            .push((pr_number, method.into(), head_sha.into()));
        Ok(self.merge_outcome.clone())
    }
}

/// Tracker that records the park-path transition + custom-field write.
#[derive(Default)]
struct RecordingTracker {
    transitions: StdMutex<Vec<(String, String)>>,
    fields: StdMutex<Vec<(String, String)>>,
}

#[async_trait]
impl IssueTracker for RecordingTracker {
    async fn fetch_candidate_issues(&self) -> TrackerResult<Vec<Issue>> {
        Ok(vec![])
    }
    async fn fetch_issues_by_states(&self, _: &[String]) -> TrackerResult<Vec<Issue>> {
        Ok(vec![])
    }
    async fn fetch_issue_states_by_ids(&self, _: &[String]) -> TrackerResult<Vec<IssueState>> {
        Ok(vec![])
    }
    async fn read_custom_field(&self, _: &str, _: &str) -> TrackerResult<CustomFieldValue> {
        Ok(CustomFieldValue::Null)
    }
    async fn write_custom_field(
        &self,
        id: &str,
        key: &str,
        _: CustomFieldValue,
    ) -> TrackerResult<()> {
        self.fields.lock().unwrap().push((id.into(), key.into()));
        Ok(())
    }
    async fn ensure_custom_field(&self, _: &CustomFieldSchema) -> TrackerResult<()> {
        Ok(())
    }
    async fn post_comment(&self, _: &str, _: &str) -> TrackerResult<()> {
        Ok(())
    }
    async fn transition_issue(&self, id: &str, target: &str) -> TrackerResult<()> {
        self.transitions
            .lock()
            .unwrap()
            .push((id.into(), target.into()));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn cfg_str(enabled: bool, max_cycles: u32) -> String {
    format!(
        r#"---
tracker:
  kind: linear
  api_key: test-key
  project_slug: my-project
github:
  webhook_secret: shh
  pat: ghp_xxx
feedback_loop:
  max_attempts: 5
  needs_fixes_state: "Needs Fixes"
  blocked_state: "Blocked - Human Review"
  merge_coordinator:
    enabled: {enabled}
    merge_method: rebase
    max_update_cycles: {max_cycles}
custom_fields:
  attempt_count: sinfonia_attempt_count
  last_failure_log: sinfonia_last_ci_failure
  max_attempts_override: sinfonia_max_attempts
  failure_category: sinfonia_failure_category
  tokens_consumed: sinfonia_tokens_consumed
  cost_consumed_usd: sinfonia_cost_consumed_usd
  max_cost_override_usd: sinfonia_max_cost_usd
server:
  bind: "0.0.0.0"
  port: 8081
storage:
  state_db_path: /tmp/test-bridge.db
telemetry:
  service_name: sinfonia-bridge
---
"#
    )
}

/// Owns the fakes + config so a [`Ctx`] can borrow from it within a test.
struct Harness {
    config: BridgeConfig,
    store: Store,
    tracker: RecordingTracker,
    gh: Arc<FakeGh>,
    gh_dyn: Arc<dyn GhOps>,
    labels: LabelManager,
}

impl Harness {
    async fn new(enabled: bool, max_cycles: u32, gh: FakeGh) -> Self {
        let config = parse_bridge_str(&cfg_str(enabled, max_cycles)).expect("config parses");
        let store = Store::open_in_memory().await.expect("store");
        let gh = Arc::new(gh);
        let gh_dyn: Arc<dyn GhOps> = gh.clone();
        // manage_labels=false → label calls short-circuit without touching gh.
        let labels = LabelManager::new(gh_dyn.clone(), false, "sinfonia", LabelAliases::default());
        Self {
            config,
            store,
            tracker: RecordingTracker::default(),
            gh,
            gh_dyn,
            labels,
        }
    }
    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            config: &self.config,
            store: &self.store,
            tracker: &self.tracker,
            gh: &self.gh_dyn,
            labels: &self.labels,
        }
    }
}

const REPO: &str = "acme/widgets";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disabled_coordinator_does_nothing() {
    let h = Harness::new(false, 3, FakeGh::new(vec![pr(false, "open", "clean", "h1")])).await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok");
    // Nothing enqueued, nothing merged.
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn approval_of_ready_green_pr_merges_immediately() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "clean", "h1")])).await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok");
    // Merged with the configured method + head, row dequeued.
    let merges = h.gh.merge_calls.lock().unwrap().clone();
    assert_eq!(merges, vec![(1, "rebase".into(), "h1".into())]);
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
}

#[tokio::test]
async fn approval_without_tracker_mapping_is_ignored() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "clean", "h1")])).await;
    enqueue_on_approval(&h.ctx(), REPO, 9).await.expect("ok");
    assert!(h.store.get_landing(REPO, 9).await.unwrap().is_none());
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn behind_pr_updates_branch_then_waits() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "behind", "h1")])).await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok");
    // update-branch was called with the current head; no merge yet; row waits
    // in `updating` with attempt incremented.
    assert_eq!(
        h.gh.update_calls.lock().unwrap().clone(),
        vec![(1, "h1".into())]
    );
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
    let row = h.store.get_landing(REPO, 1).await.unwrap().expect("row");
    assert_eq!(row.status, LandingStatus::Updating);
    assert_eq!(row.attempt, 1);
}

#[tokio::test]
async fn behind_then_green_head_merges() {
    // First get → behind; after update-branch the synchronize→green webhook
    // re-enters and the second get → clean → merge.
    let h = Harness::new(
        true,
        3,
        FakeGh::new(vec![
            pr(false, "open", "behind", "h1"),
            pr(false, "open", "clean", "h2"),
        ]),
    )
    .await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok"); // → updating
    on_ci_green(&h.ctx(), REPO, 1).await.expect("ok"); // → merge on new head
    let merges = h.gh.merge_calls.lock().unwrap().clone();
    assert_eq!(merges, vec![(1, "rebase".into(), "h2".into())]);
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
}

#[tokio::test]
async fn exceeding_max_update_cycles_parks() {
    // max_update_cycles = 1: first behind consumes the cycle (attempt→1);
    // the next behind advance parks (attempt >= max).
    let h = Harness::new(
        true,
        1,
        FakeGh::new(vec![pr(false, "open", "behind", "h1")]),
    )
    .await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok"); // attempt→1, updating
    on_ci_green(&h.ctx(), REPO, 1).await.expect("ok"); // behind again, attempt>=1 → park
    // Parked: ticket transitioned to needs-fixes, failure context written, row gone.
    assert_eq!(
        h.tracker.transitions.lock().unwrap().clone(),
        vec![("ENG-1".into(), "Needs Fixes".into())]
    );
    assert_eq!(
        h.tracker.fields.lock().unwrap().clone(),
        vec![("ENG-1".into(), "sinfonia_last_ci_failure".into())]
    );
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn dirty_pr_parks_without_update_or_merge() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "dirty", "h1")])).await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok");
    assert_eq!(
        h.tracker.transitions.lock().unwrap().clone(),
        vec![("ENG-1".into(), "Needs Fixes".into())]
    );
    assert!(h.gh.update_calls.lock().unwrap().is_empty());
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
}

#[tokio::test]
async fn ready_but_red_retest_relinquishes() {
    // Ready + checks failed → the coordinator dequeues (the red path owns the
    // needs-fixes transition) and does NOT merge or transition itself.
    let h = Harness::new(
        true,
        3,
        FakeGh::new(vec![pr(false, "open", "unstable", "h1")]).with_summary(CheckRunSummary {
            failed: vec!["e2e".into()],
            passed: vec![],
            any_pending: false,
        }),
    )
    .await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok");
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
    assert!(h.tracker.transitions.lock().unwrap().is_empty());
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
}

#[tokio::test]
async fn out_of_band_merge_dequeues_without_acting() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(true, "closed", "clean", "h1")])).await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    // Pre-seed an in-flight row, then advance.
    h.store
        .enqueue_landing(REPO, 1, "ENG-1", "h1")
        .await
        .expect("enqueue");
    kick(&h.ctx(), REPO).await;
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
}

#[tokio::test]
async fn on_ci_green_without_landing_is_noop() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "clean", "h1")])).await;
    on_ci_green(&h.ctx(), REPO, 1).await.expect("ok");
    // No landing → get_pull_request never consulted for a merge.
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn on_ci_red_dequeues_landing() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "behind", "h1")])).await;
    h.store
        .enqueue_landing(REPO, 1, "ENG-1", "h1")
        .await
        .expect("enqueue");
    on_ci_red(&h.ctx(), REPO, 1).await.expect("ok");
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
}

#[tokio::test]
async fn serialization_only_head_of_queue_progresses() {
    // Two approved PRs in one repo. Head (PR 1) is behind → update-branch +
    // wait. PR 2 must not be touched (no merge, no update) while 1 is head.
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "behind", "h1")])).await;
    for n in [1u64, 2] {
        h.store
            .upsert_pr_ticket(REPO, n, &format!("ENG-{n}"))
            .await
            .expect("map");
    }
    h.store
        .enqueue_landing(REPO, 1, "ENG-1", "h1")
        .await
        .expect("e1");
    h.store
        .enqueue_landing(REPO, 2, "ENG-2", "h2")
        .await
        .expect("e2");
    kick(&h.ctx(), REPO).await;
    // Only PR 1 (head) was acted on.
    let updates = h.gh.update_calls.lock().unwrap().clone();
    assert_eq!(updates, vec![(1, "h1".into())]);
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
    // PR 2 still queued, untouched.
    assert!(h.store.get_landing(REPO, 2).await.unwrap().is_some());
}

#[tokio::test]
async fn boot_reconcile_dequeues_out_of_band_merge() {
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(true, "closed", "clean", "h1")])).await;
    h.store
        .enqueue_landing(REPO, 1, "ENG-1", "h1")
        .await
        .expect("enqueue");
    reconcile_on_boot(&h.ctx()).await.expect("ok");
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
    assert!(h.gh.merge_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn boot_reconcile_rearms_moved_head() {
    // Stored head h0; GitHub now shows h1 (still behind). Reconcile re-arms to
    // h1, then kick acts on h1 (update-branch with the new head).
    let h = Harness::new(true, 3, FakeGh::new(vec![pr(false, "open", "behind", "h1")])).await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    h.store
        .enqueue_landing(REPO, 1, "ENG-1", "h0")
        .await
        .expect("enqueue");
    reconcile_on_boot(&h.ctx()).await.expect("ok");
    // update-branch used the reconciled head h1, not the stale h0.
    let updates = h.gh.update_calls.lock().unwrap().clone();
    assert_eq!(updates, vec![(1, "h1".into())]);
}

#[tokio::test]
async fn merge_method_threads_through_to_merge_call() {
    let mut cfg = cfg_str(true, 3);
    cfg = cfg.replace("merge_method: rebase", "merge_method: squash");
    let config = parse_bridge_str(&cfg).expect("config parses");
    let store = Store::open_in_memory().await.expect("store");
    let gh = Arc::new(FakeGh::new(vec![pr(false, "open", "clean", "h1")]));
    let gh_dyn: Arc<dyn GhOps> = gh.clone();
    let labels = LabelManager::new(gh_dyn.clone(), false, "sinfonia", LabelAliases::default());
    let tracker = RecordingTracker::default();
    store.upsert_pr_ticket(REPO, 1, "ENG-1").await.expect("map");
    let ctx = Ctx {
        config: &config,
        store: &store,
        tracker: &tracker,
        gh: &gh_dyn,
        labels: &labels,
    };
    enqueue_on_approval(&ctx, REPO, 1).await.expect("ok");
    assert_eq!(
        gh.merge_calls.lock().unwrap().clone(),
        vec![(1, "squash".into(), "h1".into())]
    );
}

/// `with_update` is exercised indirectly via the stale/conflict paths in
/// integration; keep a direct conflict-park check here.
#[tokio::test]
async fn update_branch_conflict_parks() {
    let h = Harness::new(
        true,
        3,
        FakeGh::new(vec![pr(false, "open", "behind", "h1")])
            .with_update(UpdateBranchOutcome::Conflict),
    )
    .await;
    h.store
        .upsert_pr_ticket(REPO, 1, "ENG-1")
        .await
        .expect("map");
    enqueue_on_approval(&h.ctx(), REPO, 1).await.expect("ok");
    assert_eq!(
        h.tracker.transitions.lock().unwrap().clone(),
        vec![("ENG-1".into(), "Needs Fixes".into())]
    );
    assert!(h.store.get_landing(REPO, 1).await.unwrap().is_none());
}
