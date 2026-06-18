# Proposal 0003 — Feedback-Loop Reliability Seams

- **Status:** Proposed (Draft — v0.4 milestone)
- **Author:** (reliability working group)
- **Date:** 2026-06-15
- **Affects:** `crates/sinfonia-bridge` (seams 1B, 2), `crates/sinfonia` orchestrator +
  workspace (seam 3), `crates/sinfonia-tracker` (seam 1A), `docs/SPEC.md` §8.5 / §11.6 / §14.3
- **Spec sections touched:** §8.5 (reconciliation), §11.6.1–§11.6.2 (bridge writes / envelope),
  §14.3 (restart recovery)
- **Tracking milestone:** v0.4

> **Origin.** An architecture review of [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md) flagged six
> risks in the nested feedback loops. Three were confirmed against source as real, unspecified
> hardening gaps in the *implementation seams* — the parts "most likely to be subtly wrong in code
> while looking fine in the diagram." This proposal scopes those three. The other three are
> recorded in §6 (Out of Scope) with their disposition.

**Implementation status (v0.4).** Not yet implemented — all three seams remain
proposed. They are larger, cross-cutting changes (new bridge GitHub-client
methods for the reconcile sweep; orchestrator/workspace/runner changes plus a
file-lock dependency for resume-detection and the single-instance lock; a
per-ticket keyed lock in the bridge feedback path) that warrant their own
verified pass. The security-focused subset of the companion proposal
[`0004`](0004-agent-tool-surface-hardening.md) landed first.

---

## 1. Summary

The control-system shape is sound: a reader-only daemon, a tracker source-of-truth, a bridge
controller, and bounded autonomy with humans only at the boundaries. The risk is concentrated in
three seams where a correct *design* becomes a possibly-incorrect *implementation*, all tracing to
one root: **the tracker is both the coordination substrate and a third-party system we only
partially control, and neither the daemon nor the bridge holds durable scheduler state.**

1. **Seam 1 — Tracker write contention.** Agent and bridge both write the same ticket with
   **blind last-writer-wins**. No optimistic-concurrency check exists on any tracker write, and the
   Linear marker-comment envelope is a non-atomic read-modify-write.
2. **Seam 2 — Missed-webhook silent stall.** A bridge that is down (or that 200s an event without
   recording it) when CI completes never reconciles that result. The ticket neither advances to
   `Needs Fixes` nor escalates — it goes quiet. There is no startup reconciliation sweep.
3. **Seam 3 — Restart into a stale workspace.** When the daemon restarts mid-run, an active-state
   ticket is re-dispatched into the **existing workspace with no reset and `after_create` skipped**,
   so the agent inherits whatever half-done git state the killed run left behind.

Each is addressed below with a verified problem statement, a proposed mechanism, and the spec delta.

## 2. Goals and Non-Goals

### 2.1 Goals

1. Define a **write-resolution rule** for agent↔bridge contention and make the bridge's own
   envelope RMW race-free (seam 1).
2. Add a **bridge startup reconciliation sweep** so a missed CI result is recovered, not lost
   (seam 2).
3. Make **daemon restart safe** for in-flight tickets: detect an interrupted workspace and recover
   it before re-dispatch, and prevent silent double-dispatch from a second daemon (seam 3).
4. Keep all three **opt-in or default-safe** — no behavior change for a healthy single-instance
   deployment that never restarts mid-run.

### 2.2 Non-Goals

- **No durable orchestrator scheduler DB.** The in-memory + tracker + filesystem recovery model
  (§14.3) stands; this proposal hardens its edges, it does not replace it.
- **No agent-tool-surface sandboxing.** The prompt-injection blast-radius finding (review item 6)
  is a separate security effort — see §6.
- **No change to PR→ticket mapping or category routing.** (§11.6.8 is unchanged.)
- **No HA / active-active daemon.** Seam 3 prevents *accidental* double-dispatch; it does not make
  the daemon clusterable.

---

## 3. Seam 1 — Tracker Write Contention

### 3.1 Problem (verified)

There is **no optimistic-concurrency control on any tracker write.** Both writers blind-overwrite:

- **Linear state transition** (`crates/sinfonia-tracker/src/linear.rs:416-428`): `issueUpdate`
  with no version field.
- **Linear custom field** (`linear.rs:437-451`): `write_custom_field` is a read-modify-write —
  `load_marker_comment` (`linear.rs:199-233`) → mutate in memory → `store_marker_comment`
  (`linear.rs:237-264`), which `commentUpdate`s the whole body with no revision check.
- **Jira state transition** (`crates/sinfonia-tracker/src/jira.rs:328-360`) and **custom field**
  (`jira.rs:374-388`): the `PUT /rest/api/3/issue/{id}` sends **no `If-Match` header**, though
  Jira supports it.

Two distinct races result:

- **(1A) Envelope clobber (bridge-internal).** Two webhooks for the *same ticket* processed
  concurrently both `load → mutate → store` the marker; the later store silently drops the earlier
  one's field updates. This is the higher-probability race because the bridge can fan out webhook
  handling.
- **(1B) Transition clobber (agent↔bridge).** The agent transitions a ticket (e.g. → `In Review`)
  at nearly the same instant the bridge transitions it (e.g. → `Needs Fixes`). Last write wins;
  a dropped bridge transition means a ticket that *should* be in `Needs Fixes` sits elsewhere and
  the loop stalls. SPEC §11.6 defines no resolution rule. (Note: the *common* sequence —
  agent → `In Review`, CI runs, bridge → `Needs Fixes` — is naturally ordered by CI completion;
  the genuine race window is narrow but non-zero.)

### 3.2 Design

**1A — Serialize the envelope RMW per ticket.** The bridge MUST guarantee that the
read-modify-write of the `sinfonia_bridge_state_v1` envelope for a given `issue_id` is atomic with
respect to other bridge operations on the same ticket. RECOMMENDED: a per-ticket async mutex (a
`keyed_lock(issue_id)` over the existing `DashMap`/`HashMap` of in-flight tickets) held across
`load_marker_comment` → mutate → `store_marker_comment`. This is cheap, process-local, and removes
the bridge-vs-bridge clobber entirely. The lock is advisory within one process; combined with the
single-writer invariant (§11.6.1, the bridge is the *only* `sinfonia_*` writer) it is sufficient.

**1B — Compare-and-set on the contested transition.** The bridge MUST re-read the ticket's current
tracker state immediately before a state transition and proceed only if it still matches the state
the evaluation was based on; on mismatch it logs and aborts the transition (the next CI event, or
the seam-2 sweep, re-evaluates from fresh state). Where the tracker exposes a native precondition,
the bridge SHOULD use it:

- **Jira:** send `If-Match` with the issue version/ETag on the `PUT`.
- **Linear:** Linear has no broad ETag surface; use the read-state-then-conditional-transition
  guard above, keyed on the `state.id` last observed.

This makes the bridge a *conditional* writer for transitions while leaving `sinfonia_*` field
writes (its owned namespace) unconditional, which is correct — nothing else writes them.

### 3.3 Spec delta

- **§11.6.2** — add: a conforming bridge MUST serialize the envelope read-modify-write per ticket;
  partial/concurrent envelope updates are a conformance defect.
- **New §11.6.14 "Write Contention & Precedence"** — state the rule: the bridge owns and
  unconditionally writes the `sinfonia_*` namespace; for the shared *state* surface the bridge MUST
  perform a compare-and-set (native precondition where available, read-then-conditional otherwise)
  and MUST NOT blind-overwrite a state it did not last observe.

---

## 4. Seam 2 — Missed-Webhook Reconciliation Sweep

### 4.1 Problem (verified)

The bridge is **event-driven only**. Boot (`crates/sinfonia-bridge/src/main.rs:81-192`) parses
config, opens the SQLite store, spawns the *budget* debounce reconciler (coalescing only — not
webhook recovery), and binds the listener. There is no PR scan and no backfill. The store
(`crates/sinfonia-bridge/src/storage.rs:155-165`) has exactly two tables — `processed_deliveries`
and `pr_ticket_map` — and no "unprocessed run" tracking. The `X-GitHub-Delivery` dedup
(`handlers.rs:121-156`, `storage.rs:84-99`) defends against *double*-delivery, not *missed*
delivery.

Recovery therefore relies entirely on GitHub's bounded webhook retries (~8 attempts over ~30 days);
`docs/CLIENT_SETUP.md` and `docs/DEPLOYMENT.md` both say so explicitly ("webhook retries from
GitHub fill the gap"). Two failure modes leak through:

- A bridge deploy/outage longer than GitHub's retry window drops the result permanently.
- A handler that returns 200 *without* recording the delivery (a bug, or a crash after the response
  flushes) makes GitHub stop retrying — silent loss with no outage at all.

The symptom is the worst kind: a ticket goes quiet — no `Needs Fixes`, no escalation, no error.

### 4.2 Design

Add an **idempotent reconciliation sweep** that runs on bridge startup and (OPTIONAL) on a slow
timer:

1. List open PRs across configured repos (new `GhOps::list_open_prs`), filter to those whose
   title/body matches the PR→ticket regex (§11.6.8) — i.e. PRs the bridge owns.
2. For each, fetch the latest *completed* check-suite / workflow-run conclusion for the PR head SHA
   (new `GhOps::latest_completed_run_for`).
3. Reconcile when the observed CI conclusion is inconsistent with the ticket's current state —
   e.g. CI is red but the ticket is not in a `Needs Fixes`/blocked state, or CI is green but the
   PR carries no `awaiting-review` label. Reconciliation re-uses the existing `evaluate_one_pr`
   path; no new transition logic.
4. Idempotency: the sweep keys on the run's delivery-equivalent id and the existing
   `processed_deliveries` table (or a `run_id` it records), so a result already processed by a live
   webhook is a no-op. The sweep MUST be safe to run concurrently with live webhook handling
   (it shares the seam-1A per-ticket lock).

The sweep is bounded (configurable max PRs scanned per pass) and logs what it reconciled and what
it skipped — no silent truncation.

### 4.3 Config surface (`BRIDGE.md`)

```yaml
feedback_loop:
  # Missed-webhook recovery (Proposal 0003). All optional.
  startup_reconcile: true            # scan open PRs on boot; default true
  reconcile_interval_s: 0            # 0 = startup-only; >0 = also sweep on this cadence
  reconcile_max_prs: 200             # bound per sweep pass
```

Default-safe: a healthy bridge that never misses an event sees only no-op sweeps.

### 4.4 Spec delta

- **§8.5 is the daemon's reconciliation; add a bridge analogue.** New **§11.6.15 "Reconciliation
  Sweep (RECOMMENDED)"**: a conforming bridge SHOULD reconcile open mapped PRs against current CI
  conclusion on startup, idempotently, so a webhook missed during downtime (or dropped after
  GitHub's retry budget) is recovered rather than lost. Document the bound and the idempotency key.

---

## 5. Seam 3 — Restart Into a Stale Workspace

### 5.1 Problem (verified)

Daemon scheduler state is 100% in-memory (`crates/sinfonia/src/domain.rs:74-84`); a restart starts
with empty `running`/`claimed`. On the first post-restart tick, `reconcile_running_issues` runs
first (`orchestrator/mod.rs:229-232`) but finds an empty `running` map, so it is a no-op; the
still-active ticket is then re-fetched as a fresh candidate and dispatched with `attempt=None`.

`Workspace::ensure_for_issue` (`crates/sinfonia/src/workspace/manager.rs:38-79`) finds the existing
directory and returns `created_now=false` (`manager.rs:49-56`) — **no reset, no wipe**. Because
`created_now=false`, `run_agent_attempt` skips `after_create` (`orchestrator/runner.rs:76-79`) and
runs only `before_run`. Startup cleanup touches **terminal-state workspaces only**
(`mod.rs:685-700`), never active-but-orphaned ones. Net effect: the agent resumes inside whatever
the killed run left — a half-finished rebase, a dirty tree, a partial clone.

Adjacent finding: there is **no guard against two daemons** double-dispatching the same ticket into
the same deterministic workspace path (`manager.rs:81-84`) — no lockfile, no durable claim. It is a
singleton by construction but nothing enforces it.

### 5.2 Design

**5A — Detect and recover an interrupted run.** When the daemon dispatches into a workspace that
**already exists but has no in-memory run from this process lifetime**, it MUST treat the run as
*resumed-after-interruption* rather than *fresh*:

- Drop a run marker (`.sinfonia/run.active`, gitignored like the rest of `.sinfonia/`) when a worker
  starts, and remove it on clean exit. A pre-existing marker at dispatch time ⇒ interrupted run.
- On an interrupted run, the orchestrator MUST run a recovery step before launching the agent.
  Two options, RECOMMENDED to ship both:
  - a new OPTIONAL **`hooks.on_resume`** (runs only on interrupted re-dispatch; the natural home
    for `git rebase --abort || true; git merge --abort || true; git reset --hard; git clean -fd`
    or a repo-appropriate equivalent), and
  - pass an explicit **`resumed: true`** template variable (alongside `attempt`) so the prompt can
    instruct the agent to assess and stabilize workspace state first.
- This keeps workspace *population/reset* policy implementation-defined (§9.3) — the orchestrator
  signals "this is a resume," the hook/prompt decides recovery — consistent with the existing
  hook contract.

**5B — Advisory single-instance lock.** The daemon SHOULD acquire an advisory lock on
`workspace.root` (e.g. a `flock`ed `.sinfonia-daemon.lock`) at startup and refuse to start if held,
with an operator-visible error. This converts silent double-dispatch corruption into a loud,
early failure. OPTIONAL override flag for operators who knowingly shard by project.

### 5.3 Config / template surface

- `hooks.on_resume` (multiline shell, OPTIONAL) — runs before `before_run` only on an interrupted
  re-dispatch; failure is fatal to the attempt (same semantics as `after_create`).
- `resumed` (bool) added to the template input variables (§5.4 / §12.1), `false` on a normal first
  dispatch, `true` on a post-interruption resume. Guarded by the same strict-mode rules; existing
  prompts that don't reference it are unaffected.
- `workspace.single_instance_lock` (bool, default `true`).

### 5.4 Spec delta

- **§9.3 / §9.4** — add `on_resume` to the hook set with its trigger (existing workspace, no live
  run this lifetime) and fatal-on-failure semantics.
- **§12.1 / §5.4** — add the `resumed` template variable.
- **§14.3** — replace the silent gap with an explicit rule: on restart, a still-active ticket whose
  workspace pre-exists MUST be treated as a *resume* (run `on_resume`, set `resumed=true`), not a
  fresh create; and the daemon SHOULD hold a single-instance advisory lock on `workspace.root`.

---

## 6. Out of Scope (review items not in this proposal)

| Review item | Disposition |
|---|---|
| **Duplicate PR for one ticket** (retry idempotency) | Real but **mitigated**: deterministic `sinfonia/<id>` branch + reused workspace means a retry normally re-pushes the same branch → same PR. A second PR requires the agent to deviate from the branch convention. Tracked as a follow-up (a bridge guard: "open PR already exists for this ticket") if it shows up in practice. |
| **Budget loop best-effort** | **Already specified** (§11.6.12): the budget cap is an SLO, the attempt counter is the robust runaway guard, and it is tracker-persisted (`feedback/attempts.rs:73-103`) so it survives restart. Operational note only: do not treat the budget cap as the primary safety net (it under-counts across a restart). No code change. |
| **Semantic prompt injection / agent tool surface** | **Separate security effort.** Structural defenses on `bridge.json` (§11.6.13) hold, but the agent's `shell` tool is unrestricted, claude_code launches with `--dangerously-skip-permissions` by default, and the subprocess inherits the full environment — so secret exfiltration and out-of-workspace writes are unmitigated and independent of the CODEOWNERS merge gate. Deserves its own proposal (sandbox posture, env scoping, shell audit); deliberately excluded here to keep 0003 a reliability change. |

## 7. Rollout & Changelog Plan

Each seam is independently shippable and reversible:

1. **Seam 1A** (per-ticket envelope lock) — pure bridge-internal correctness fix, no config, no
   spec-visible behavior change. Ship first; lowest risk.
2. **Seam 2** (startup reconcile) — add `GhOps::list_open_prs` / `latest_completed_run_for`, the
   sweep behind `startup_reconcile` (default on), idempotency tests. Ship dark-then-default.
3. **Seam 1B** (conditional transition) — compare-and-set + Jira `If-Match`; add a race test
   (concurrent agent/bridge transition fixture).
4. **Seam 3** (resume detection + lock) — `on_resume` hook, `resumed` variable, advisory lock.
   Ship the lock first (loud-fail safety), then the resume path.

Each merged step gets a Keep-a-Changelog `[Unreleased]` entry. All additive and default-safe →
**minor** bump within the v0.4 line; the envelope stays `sinfonia_bridge_state_v1` (no field-shape
change → no `_v2` migration per §11.6.2).

## 8. Open Questions

1. **Sweep cadence default.** Startup-only (`reconcile_interval_s: 0`) is the safe default; is a
   periodic sweep worth the GitHub API budget for long-lived bridges, or should missed events
   during *uptime* be treated as a bug to fix rather than a condition to sweep?
2. **Linear CAS granularity.** Linear exposes `updatedAt` but no first-class precondition on
   `issueUpdate`. Is read-state-then-conditional-transition (a TOCTOU-narrowed window) sufficient,
   or do we need a bridge-side per-ticket transition lock mirroring seam 1A?
3. **`on_resume` vs. always-reset.** Should an interrupted resume default to a hard reset (safe but
   discards in-progress agent work) or to a hook-defined recovery (preserves work but trusts the
   hook)? Proposed default: hook-defined, because discarding a near-complete change to re-run from
   scratch is the more expensive failure for an autonomous loop.
4. **Lock scope.** One lock per `workspace.root` assumes one daemon per root. Operators sharding one
   tracker across daemons by project slug would need a per-(root,project) lock or the override flag.

## 9. Cross-References

| Reference | Relevance |
|-----------|-----------|
| [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md) | The loops these seams harden; review origin |
| `crates/sinfonia-tracker/src/linear.rs:199-264,416-451` | Marker RMW + blind writes (seam 1) |
| `crates/sinfonia-tracker/src/jira.rs:328-388` | Blind PUT, no `If-Match` (seam 1) |
| `crates/sinfonia-bridge/src/main.rs:81-192`, `storage.rs:155-165` | No startup sweep, two tables (seam 2) |
| `crates/sinfonia-bridge/src/webhook/handlers.rs:121-156` | Delivery dedup ≠ missed-delivery recovery (seam 2) |
| `crates/sinfonia/src/workspace/manager.rs:38-79`, `orchestrator/runner.rs:76-79` | Reuse-no-reset, `after_create` skipped (seam 3) |
| `crates/sinfonia/src/orchestrator/mod.rs:229-232,685-700`, `domain.rs:74-84` | In-memory state, terminal-only cleanup (seam 3) |
| `docs/SPEC.md` §8.5 / §11.6.1–§11.6.2 / §14.3 | Sections amended by this proposal |
| [`0001-harness-feedback-ingestion.md`](0001-harness-feedback-ingestion.md), [`0002-orchestrator-gating-ground-truth.md`](0002-orchestrator-gating-ground-truth.md) | ADR format + numbering precedent |
