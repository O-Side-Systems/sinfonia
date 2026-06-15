# Proposal 0005 — Sinfonia-Native Merge Coordinator

- **Status:** Proposed (Design resolved §8; **Phase 1 of §9 implemented** in v0.4 — GhOps primitives + landing-queue store, flag-gated/unused; Phases 2–4 pending)
- **Author:** (harness working group)
- **Date:** 2026-06-15
- **Affects:** `crates/sinfonia/src/orchestrator` (merge/landing lifecycle),
  `crates/sinfonia-bridge/src` (reuses `mergeStateStatus` polling + CI-status ingestion),
  `docs/SPEC.md §8` (dispatch/landing lifecycle), `docs/HARNESS-SPEC.md §7.4`
  (record a Sinfonia-side coordinator as an accepted substitute for the native queue)
- **Spec sections touched:** §8.2 (landing lifecycle — additive), HARNESS-SPEC §7.4 (note only).
  No change to the `bridge.json` contract (§7.1).
- **Tracking milestone:** unscheduled (post-v0.4)

> **Origin.** Surfaced while bootstrapping a target repo (`wyrd-builder`) to be Sinfonia-ready.
> HARNESS-SPEC §7.4 **RECOMMENDS** a GitHub native merge queue ("rebase-and-test each PR against
> the latest `main` before merging"). But for **private** repositories the native queue is
> **GitHub Enterprise Cloud only** — it is not available on the Team plan. Target repos below
> Enterprise therefore cannot satisfy the recommendation with native tooling. This proposal sketches
> a **plan-independent** equivalent owned by Sinfonia, the orchestrator that already holds the merge
> decision. It is a *sketch*, not an implementation plan — the open questions in §6 are unresolved.

---

## 1. Summary

§7.4's native merge queue exists to close one gap: a PR that was **green at submit time** can still
**break `main`** once it integrates with concurrent work that landed in between. The queue prevents
this by rebasing each PR onto the latest `main` and re-running the required checks *before* the merge
commits.

Sinfonia already owns every input this needs:

- the §7.4 **mergeable-w.r.t.-`main`** gate (loop on `mergeStateStatus`; only `DIRTY`/`BEHIND` keep
  looping; `BLOCKED`/`UNSTABLE` count as ready-for-human),
- the workflow **STEP 0** authoritative *blocker-PR-merged-to-`main`* gate (§8.2),
- **serial-foundation** concurrency (`max_concurrent_agents_by_state: "In Progress": 1`, §8.2 / §7.4),
- the **overlap linter** that blocks two `sinfonia/*` PRs touching the same owned module,
- the **bridge's** existing CI-status ingestion (it already reads the harness gate result per PR).

A **merge coordinator** is the small missing piece: a serialized *rebase → re-test → merge* step that
turns "this PR is approved and was green" into "this PR is green **against the `main` it will actually
land on**." It composes with the gates above rather than replacing them, and works on any GitHub tier.

## 2. Motivation

Today, without a queue, a target repo has two options, both partial:

1. **Branch-protection "Require branches to be up to date before merging"** (all tiers). Forces a
   rebase + re-check before the merge button enables. Correct, but it is a *GitHub setting* the target
   repo must configure, it serializes merges with no speculation, and it cannot coordinate with
   Sinfonia's own dispatch state.
2. **Post-merge harness gate on `main`** (HARNESS-SPEC §7.4, REQUIRED — already shipped). This is a
   **reactive backstop**: it detects integration breakage *after* it lands and alerts operators. It
   does not *prevent* the broken merge, so the next agent dispatch can still see a broken base in the
   window before the alert is actioned.

Neither gives the preventive, orchestrator-coordinated guarantee the native queue does. Since Sinfonia
already drives landing, owning the coordinator keeps the guarantee inside the deterministic-sensor +
orchestrator boundary and removes the Enterprise-tier dependency from "Sinfonia-ready."

## 3. Design sketch

A coordinator that sits between "PR is ready to land" and "PR is merged," serializing landings:

```
ready PR (mergeStateStatus ∈ {BLOCKED, UNSTABLE}, human-approved, harness green)
   │
   ▼
[enqueue]──► landing queue (persisted, FIFO by §8.2 sort order)
   │
   ▼  (one in-flight at a time — serial-foundation default)
[rebase head onto latest main]
   │   ├─ conflict (DIRTY) ─► kick back to agent loop (Needs Fixes), dequeue next
   ▼
[push rebased head to its sinfonia/<id> branch] ──► triggers harness gate on the rebased commit
   │
   ▼  (await via the bridge's existing CI-status ingestion)
[harness gate result]
   ├─ green ─► merge (method: rebase) ─► dequeue next
   └─ red   ─► kick back to agent loop (Needs Fixes) ─► dequeue next
```

Key properties:

- **Serialization first.** v1 lands one PR at a time, matching the existing serial-foundation
  invariant. Leaf fan-out (non-overlapping surface, already overlap-linter-gated) is the natural place
  to add **speculative batching** later (v2) — out of scope here.
- **Reuses existing seams.** The "await CI on the rebased head" step is exactly what the bridge already
  does for a PR's harness gate; the coordinator just re-arms it after the rebase push. No new
  `bridge.json` fields.
- **Idempotent + crash-safe.** Queue state is persisted (cf. the bridge's per-issue marker comment /
  existing state stores), so a coordinator restart resumes the in-flight landing rather than
  double-merging. A landing is keyed by `(issue-id, head-sha)`.
- **Composes with the gates, doesn't duplicate them.** STEP 0 still gates *starting* work on blocked
  issues; the coordinator gates *landing*. The mergeable-w.r.t.-`main` loop still classifies
  `DIRTY`/`BEHIND`; the coordinator is what *acts* on `BEHIND` by rebasing instead of merely waiting.
- **Human gate preserved.** The coordinator only enqueues PRs that already cleared CODEOWNERS review
  (§7.3/§7.4). It never self-approves; it serializes already-approved landings.

## 4. Alternatives considered

| Option | Verdict |
|---|---|
| GitHub **native merge queue** | Rejected as the *baseline*: Enterprise-Cloud-only for private repos; reintroduces the tier dependency this proposal removes. Still the right choice for repos that *are* on Enterprise. |
| Branch-protection **"require up to date"** | Keep as the **interim default** for target repos (documented in HARNESS.md). The coordinator is the upgrade for shops wanting tier-independence + Sinfonia-coordinated serialization. Not mutually exclusive. |
| **Post-merge gate only** (status quo) | Insufficient alone — reactive, not preventive. Retained as the backstop underneath the coordinator. |

## 5. Scope / non-goals

- **In scope (v1):** serialized rebase → re-test → merge for `sinfonia/<id>` PRs; reuse of the
  bridge's CI-status ingestion; persisted, crash-safe landing queue; a §8.2 note + an HARNESS-SPEC §7.4
  note recording the coordinator as an accepted native-queue substitute.
- **Non-goals:** speculative parallel batching (v2); any change to the `bridge.json` contract;
  target-repo-specific CI wiring (the coordinator drives whatever the harness gate already is).

## 6. Open questions

> **Resolved.** All seven questions below are answered in **§8 (Resolved design)**, grounded in a
> code audit of what the bridge can actually do today. The short version: the coordinator lives in
> the **bridge** (the daemon has zero GitHub access by invariant), and because the bridge is a thin
> HTTP client with **no git checkout** it uses GitHub's `update-branch` (merge-from-base) rather than
> a true local rebase. The questions are retained here for provenance.

1. **Crate home.** Orchestrator (`crates/sinfonia`) vs bridge (`crates/sinfonia-bridge`)? The merge
   decision lives in the orchestrator; the CI-status polling lives in the bridge. Likely a thin
   orchestrator coordinator that calls bridge primitives.
2. **Re-test trigger.** Pushing the rebased head to `sinfonia/<id>` re-fires the PR's harness workflow
   naturally — but is a force-push to the agent branch acceptable, and how does it interact with the
   §7.4 mergeable loop's `BEHIND` handling? (Avoid a push/poll race.)
3. **Merge method.** Rebase (matches §7.4 "Rebase and merge") vs squash. Squash changes
   `last_verified_sha` stamping ergonomics for the context graph (CONTEXT-CONTRACT §6.2).
4. **Queue persistence + ownership.** Where does landing-queue state live, and how does a restart
   reconcile against GitHub's actual merge state to avoid double-merge?
5. **Failure / timeout policy.** How many rebase→test cycles before a PR is parked to a blocked state?
   How does this compose with Sinfonia's attempt/budget caps (SPEC §11.6)?
6. **Human force-merge.** If a maintainer merges out-of-band, the coordinator must detect it and
   dequeue cleanly.
7. **Speculative batching (v2).** Worth it given Sinfonia's largely-serial dispatch + overlap gating?
   Quantify the throughput win on realistic leaf fan-out before building it.

## 7. References

- `docs/HARNESS-SPEC.md` §7.4 (merge gating; mergeable-not-CLEAN refinement; serial-foundation /
  leaf-fan-out; post-merge gate).
- `docs/SPEC.md` §8.2 (dispatch eligibility + STEP 0 authoritative merged-to-`main` gate).
- [`0002-orchestrator-gating-ground-truth.md`](0002-orchestrator-gating-ground-truth.md) — the
  verified dispatch-gate predicate this coordinator lands *downstream* of.
- GitHub merge queue availability (Enterprise-Cloud-only for private repos), GitHub Docs —
  *Managing a merge queue*.

---

## 8. Resolved design (grounded in a code audit)

A code audit of the bridge fixed the two facts that decide the whole shape:

- **The daemon has zero GitHub access** — no `octocrab` dependency, no `GhOps`, no token, confirmed
  by grep across `crates/sinfonia/src`. This is the §11.6.1 / §15.1 trust boundary, and it is
  load-bearing (the daemon "does not need any GitHub credentials"). A coordinator that rebases,
  pushes, and merges therefore **cannot** live in the daemon without breaking it.
- **The bridge already holds every GitHub primitive's prerequisite** — `octocrab 0.39` directly, a
  durable SQLite store, webhook-driven CI ingestion (`feedback::evaluate_ci`), and the persisted
  `pr_ticket_map`. But its `GhOps` trait today is **8 read/label methods only** — no PR-detail
  fetch, no merge, no branch update.

### 8.1 Q1 — Crate home → **the bridge**

The coordinator is a new module in `crates/sinfonia-bridge` (e.g. `src/merge/`). The orchestrator's
contribution is unchanged: it already enforces serial-foundation dispatch and the overlap linter.
This *corrects* the sketch's "thin orchestrator coordinator that calls bridge primitives" — the
orchestrator has no GitHub creds to call anything with. Daemon-side: **no change.**

### 8.2 Q2 — Re-test trigger → **`update-branch` (merge-from-base), not local rebase**

The sketch's §3 step "[rebase head onto latest main] → [push rebased head]" assumes a working tree.
The bridge has none (it is a thin HTTP client; every `GhOps` method maps to one `octocrab` call).
GitHub also exposes **no API that rebases a PR branch** — the only base-sync primitive is
`PUT /repos/{o}/{r}/pulls/{n}/update-branch`, which **merges** the base into the head (a new merge
commit), not a rebase. So v1:

1. Call `update-branch`. This advances the PR head to a new commit that includes the latest `main`,
   and re-fires the harness gate **naturally** (a new head SHA → a fresh `check_suite`/`workflow_run`).
   No force-push, no checkout, and it is exactly the action GitHub's `BEHIND` state is asking for, so
   it composes with the §7.4 mergeable loop rather than fighting it.
2. Record the new head SHA from the `update-branch` response and **await CI on that specific SHA**
   via the existing webhook path — this closes the push/poll race the sketch worried about (we never
   guess the head; GitHub tells us).

The §7.4 *merge method* ("Rebase and merge") still applies at the final merge step (§8.3); only the
*base-sync* step is a merge rather than a rebase. The merge commit `update-branch` adds is collapsed
by a rebase/squash final merge, so linear history on `main` is preserved either way.

> Corrects §3: read "[rebase head onto latest main]" as "[update-branch: merge latest main into the
> head]". A true local-rebase variant (clone + `git rebase` + force-push) is a possible v2 if linear
> *PR-branch* history matters to a target repo; it needs a checkout sidecar and is out of v1 scope.

### 8.3 Q3 — Merge method → **default `rebase`; operator-overridable (preference call)**

Default to GitHub's `rebase` merge to match §7.4 and keep `main` linear. Expose
`feedback_loop.merge_coordinator.merge_method: rebase|squash|merge`. **This is the one genuine
preference call**: `squash` collapses an agent PR to one commit (clean history) but changes
`last_verified_sha` stamping ergonomics for the context graph (CONTEXT-CONTRACT §6.2), since the
landed SHA no longer corresponds 1:1 to the reviewed head. Recommendation: `rebase` default, document
the squash trade-off, let the operator choose.

### 8.4 Q4 + Q6 — Persistence & restart reconciliation → **a `landing_queue` table + reconcile-on-boot**

Add one durable table to the bridge store, keyed `(repo, pr_number)` (the same key as
`pr_ticket_map`) and carrying the head SHA the sketch wants for the `(issue-id, head-sha)` identity:

```sql
CREATE TABLE IF NOT EXISTS landing_queue (
    repo         TEXT NOT NULL,
    pr_number    INTEGER NOT NULL,
    ticket_id    TEXT NOT NULL,
    head_sha     TEXT NOT NULL,        -- the SHA we last acted on
    status       TEXT NOT NULL,        -- queued|updating|awaiting_ci|merging|merged|conflict|failed
    attempt      INTEGER NOT NULL DEFAULT 0,  -- update->test cycles spent
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY(repo, pr_number)
);
```

The landing is a small state machine (`queued → updating → awaiting_ci → merging → merged`, with
`conflict`/`failed` as parked terminals). **On bridge boot**, before processing webhooks, reconcile
every non-terminal row against GitHub's *actual* state (one `get_pull_request`): if GitHub already
shows the PR merged or closed (a human/out-of-band merge — **Q6**), mark the row terminal and dequeue;
if the head SHA moved underneath us, re-enter `awaiting_ci` on the new SHA; otherwise resume. This is
the same "fresh-state on restart" discipline the budget accumulator already uses (§11.6.12), and it
makes double-merge structurally impossible because the final `merge` call is guarded by a re-read of
mergeability.

### 8.5 Q5 — Failure / timeout policy → **bounded cycles, then reuse the existing cap-hit park**

Bound update→test cycles at `merge_coordinator.max_update_cycles` (default 3). On exceed, on a rebase
`conflict` (`update-branch` → `DIRTY`), or on a red re-test, **reuse the existing
`feedback::transition` path**: transition the ticket to the configured blocked/needs-fixes state and
post the standard PR comment — i.e. the agent loop picks the PR back up exactly as it does for any
red CI today. This composes with the §11.6 attempt/budget caps for free, because parking *is* a
tracker transition the existing attempt counter already governs; the coordinator adds no new cap
system, it feeds the one that exists.

### 8.6 Enqueue trigger & the human gate

A PR enters the queue only when **both**: the harness gate is green (the bridge already computes this
in `evaluate_one_pr`'s `all_passed()` path) **and** it is human-approved. The bridge does not see
review events today, so this adds one webhook subscription — `pull_request_review` (action
`submitted`, state `approved`) — plus a `get_pull_request` that returns `reviewDecision` /
`mergeStateStatus` so enqueue can confirm "approved + mergeable-but-`BEHIND`." The coordinator
**never approves** anything; it serializes landings that already cleared CODEOWNERS (§7.3/§7.4). The
human merge gate is preserved exactly.

### 8.7 GhOps additions (the only new GitHub surface)

Three methods on the `GhOps` trait, each one `octocrab 0.39` call (the trait's existing one-call-per-
method discipline):

| New method | octocrab call | Notes |
|---|---|---|
| `get_pull_request(repo, n) -> PrLanding` | `pulls(o,r).get(n)` (+ GraphQL for `mergeStateStatus`/`reviewDecision`) | returns merge-state, head SHA, merged flag |
| `update_pr_branch(repo, n, expected_head) -> NewHead` | `PUT …/pulls/{n}/update-branch` (raw, octocrab lacks a typed builder) | pass `expected_head_sha` so GitHub no-ops on a stale call |
| `merge_pr(repo, n, method, head_sha) -> MergeResult` | `pulls(o,r).merge(n)` with method + `sha` precondition | `sha` makes the merge a compare-and-set; handle 405 (not mergeable) / 409 (sha moved) |

`list_check_run_summary` (the await-CI seam), `post_pr_comment`, the label methods, and
`lookup_pr_ticket` are **reused unchanged**.

### 8.8 Config surface (`BRIDGE.md`)

```yaml
feedback_loop:
  merge_coordinator:
    enabled: false               # opt-in; default off = today's behavior exactly
    merge_method: rebase         # rebase | squash | merge
    max_update_cycles: 3         # update-branch -> re-test attempts before parking
    # parks reuse feedback_loop.needs_fixes_state / blocked_state — no new state
```

Default `enabled: false` makes the whole feature inert until an operator opts in, so this is
additive and ships dark.

## 9. Implementation plan (phased)

Each phase is independently shippable, flag-gated off, and verifiable.

1. **GhOps primitives (no behavior change).** ✅ **DONE (v0.4).** Added `get_pull_request`,
   `update_pr_branch`, `merge_pr` to `GhOps` as **default-`not supported`** methods (so the 5 test
   fakes + `AppModeGhOps` compile unchanged), overridden in `OctocrabGhOps` via raw REST calls. Status
   routing reads the typed `GitHubError::status_code` (405→`NotMergeable`, 409→`ShaMismatch`,
   422+`expected_head_sha`→`Stale`), not the Display string. 5 wiremock tests cover the happy +
   precondition paths. Nothing calls them yet.
2. **Landing-queue store.** ✅ **DONE (v0.4).** Added the `landing_queue` table (additive migration)
   with `Store::{enqueue_landing, get_landing, advance_landing, dequeue_landing, list_landings}` and a
   `LandingStatus` enum. `enqueue` is idempotent (won't reset an in-flight row); `list_landings` is
   FIFO for the §8.4 boot sweep. 3 tests incl. restart-persistence.
3. **Coordinator state machine, behind `merge_coordinator.enabled`.** Wire the enqueue trigger
   (green + `pull_request_review approved`), the `update-branch → await CI → merge` loop reusing the
   webhook CI path, and the §8.5 park-on-failure via the existing transition path. Boot-time
   reconciliation (§8.4) before the webhook server binds.
4. **Docs + spec notes.** `BRIDGE.example.md` keys; a §8.2 landing-lifecycle note (additive); the
   HARNESS-SPEC §7.4 note recording the coordinator as an accepted native-queue substitute for
   sub-Enterprise repos.

**Trust-boundary invariant to preserve throughout:** every new capability lands in
`crates/sinfonia-bridge`; `crates/sinfonia` gains no GitHub dependency. A test/CI grep guard
(`! grep -r octocrab crates/sinfonia/src`) is cheap insurance.
