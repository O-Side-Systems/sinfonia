# Requirements: Sinfonia (v0.4 milestone)

All 16 requirements for the v0.4 milestone, organized by theme. REQ-IDs use
`[CATEGORY]-[NUMBER]` format, numbered from 01 per category (fresh bootstrap). The
synthesizer's slug-style names are recorded for traceability back to the intel.

Existing-system invariants (SPEC / HARNESS-SPEC) are captured as constraints in
PROJECT.md, NOT as buildable requirements here.

---

## Theme A — Harness feedback ingestion (HARNESS-*)

Proposal 0001 is already reflected in SPEC §11.6.13/§12.5. These requirements are
**verification/closure** work: confirm the shipped/dark behavior matches the contract,
flip defaults where appropriate, and close the adversarial test surface.

### HARNESS-01 — Structured manifest ingestion
- **slug:** REQ-harness-manifest-ingest
- **source:** docs/proposals/0001-harness-feedback-ingestion.md §3.1
- **description:** The bridge MUST make `sinfonia_last_ci_failure` carry structured,
  scenario-level diagnostics sourced from `bridge.json` when present.
- **acceptance:** On a red `workflow_run` for a mapped PR with a `bridge-*` artifact
  containing a parseable `schema_version: 2` `bridge.json`, the rendered failure field
  contains scenario, feature_file, step, and assertion per failing scenario instead of
  a check-name list.

### HARNESS-02 — Artifact references surfaced
- **slug:** REQ-artifact-references
- **source:** docs/proposals/0001-harness-feedback-ingestion.md §3.1
- **description:** Surface `artifact_urls` (trace/video/a11y references) to the inner
  agent loop so it can pull artifacts on demand.
- **acceptance:** Digest renders bundle-relative artifact names + bundle artifact name;
  bridge never fetches/resolves the URLs server-side.

### HARNESS-03 — Manifest version gate
- **slug:** REQ-manifest-version-gate
- **source:** docs/proposals/0001-harness-feedback-ingestion.md §4.3
- **description:** Version the harness contract the bridge accepts
  (`SUPPORTED_BRIDGE_MANIFEST_VERSIONS = [2]`) with warn/fallback on mismatch.
- **acceptance:** supported → ingest; newer → warn + forward-read known fields;
  older/absent/unparseable → warn + check-name fallback.

### HARNESS-04 — Optional, additive degradation
- **slug:** REQ-ingestion-optional-degrade
- **source:** docs/proposals/0001-harness-feedback-ingestion.md §3.1, §4.4
- **description:** Keep the whole path optional and strictly additive; a repo emitting no
  `bridge.json` behaves exactly as today (`ingest_harness_manifest: false` floor).
- **acceptance:** Every row of the §4.4 degradation matrix falls back to the current
  check-name path except the supported/newer-additive row.

### HARNESS-05 — Untrusted-input handling
- **slug:** REQ-ingestion-untrusted-input
- **source:** docs/proposals/0001-harness-feedback-ingestion.md §5
- **description:** Treat `bridge.json` as hostile (fork-PR) input end to end.
- **acceptance:** `max_artifact_bytes` (5 MiB) download cap, per-entry decompressed cap
  (zip-bomb), `max_failures_parsed` (20), `max_failure_digest_bytes` (8 KiB), in-memory
  parse only, no disk write, no execution, no server-side `artifact_urls` resolution,
  scalar (non-template) injection, control chars stripped.

### HARNESS-06 — Feedback-loop config surface
- **slug:** REQ-feedback-loop-config-surface
- **source:** docs/proposals/0001-harness-feedback-ingestion.md §6
- **description:** Add optional `feedback_loop` keys to `BRIDGE.md` with safe defaults:
  `harness_manifest_artifact_glob`, `harness_manifest_filename`, `max_artifact_bytes`,
  `max_failures_parsed`, `max_failure_digest_bytes`, `ingest_harness_manifest`.
- **acceptance:** Absent/`false` master switch reproduces today's behavior; no
  `WORKFLOW.md` change.

---

## Theme B — Orchestrator / tracker dependency gating (BLOCK-*)

### BLOCK-01 — Blocker-merged guardrail
- **slug:** REQ-blocker-merged-guardrail
- **source:** sinfonia-harness-action-plan.md (1.1); harness-improvement-analysis.md (Problem 1)
- **priority:** P0
- **description:** Before doing work, workflow STEP 0 (both `Todo` and `In Progress`
  prompts) MUST resolve the issue's `blocks`/`blocked-by` set from Linear and verify each
  blocker's PR is merged to `main` — not merely in a terminal state. If any blocker is
  unmerged, comment on the Linear issue and stop.
- **acceptance:** An issue with an unmerged blocker exits without code changes and leaves
  a Linear comment.

### BLOCK-02 — Dependency-modeling invariant
- **slug:** REQ-dependency-modeling-invariant
- **source:** sinfonia-harness-action-plan.md (0.1, 0.2, 2.4); harness-improvement-analysis.md (Problem 1)
- **priority:** P0/P1
- **description:** Establish + document as a tested invariant that orchestrator dependency
  gating keys only on Linear `blocks` relations (not hierarchy/"related"/prose), and that
  `Done` SHOULD be set by PR-merge-to-`main`. **Verify/confirm parent-child gating behavior
  in `crates/sinfonia/src/orchestrator/` first** (the `{% if issue.children %}` assumption
  in WORKFLOW.example.md is UNVERIFIED).
- **acceptance:** Invariant documented in docs/SPEC.md and cross-referenced from
  HARNESS-SPEC §7.3; parent/child gating claim in WORKFLOW.example.md either backed by
  code or removed.

---

## Theme C — Merge-conflict handling (MERGE-*)

Sequenced under the LOCKED integration model: **GitHub native merge queue + serial
foundational stories** (DEC-001).

### MERGE-01 — Pre-PR rebase
- **slug:** REQ-pre-pr-rebase
- **source:** sinfonia-harness-action-plan.md (1.2); harness-improvement-analysis.md (Problem 2, Gap A)
- **priority:** P0
- **description:** Before the PR-create step, fetch `origin/main`, rebase/resolve, re-run
  the harness gate (MUST be green), then push.
- **acceptance:** PRs are opened only from a branch rebased on current `main` with a green
  harness run.

### MERGE-02 — Mergeability loop
- **slug:** REQ-mergeability-loop
- **source:** sinfonia-harness-action-plan.md (1.3); harness-improvement-analysis.md (Problem 2, Gap B)
- **priority:** P0
- **description:** After push and before transition to `In Review`, poll
  `gh pr view --json mergeStateStatus`; on non-CLEAN, rebase/resolve/re-run/force-push in
  a bounded loop; transition only when `CLEAN`.
- **acceptance:** Issue reaches `In Review` only when `mergeStateStatus == CLEAN`.

### MERGE-03 — Merge queue + post-merge gate
- **slug:** REQ-merge-queue-and-post-merge-gate
- **source:** sinfonia-harness-action-plan.md (2.2, 3.4); harness-improvement-analysis.md (Problem 2)
- **priority:** P0
- **description:** HARNESS-SPEC §7.4: target repo SHOULD run a merge queue that
  rebases-and-tests each PR against latest `main`; the harness gate MUST also run on `main`
  post-merge. Ship a branch-protection / merge-queue setup note.
- **acceptance:** §7.4 names the post-merge gate; §9 checklist updated; an operator setup
  note exists.

### MERGE-04 — Conflict-phase concurrency policy
- **slug:** REQ-concurrency-policy-for-conflict-phases
- **source:** sinfonia-harness-action-plan.md (1.5); harness-improvement-analysis.md (Problem 2)
- **priority:** P1
- **description:** Keep `In Progress` concurrency at 1; document that
  foundational/cross-cutting stories run serially and only leaf stories fan out.
- **acceptance:** Comment present in the concurrency config; milestone-level
  serial-foundation convention written.

---

## Theme D — Decomposition + repository context graph (CTXGRAPH-*)

### CTXGRAPH-01 — Overlap check before build
- **slug:** REQ-overlap-check-before-build
- **source:** sinfonia-harness-action-plan.md (1.4); harness-improvement-analysis.md (Problem 3)
- **priority:** P1
- **description:** Before writing shared/utility code, the agent MUST consult the
  module-ownership map in root `AGENTS.md` and scan sibling `sinfonia/*` branches/open PRs
  for overlap; if an open sibling PR already introduces the needed capability, depend on it
  (add `blocks`, stop) rather than re-implement.
- **acceptance:** Agent logs a sibling-scan and chooses reuse-or-block over duplicate
  implementation in a seeded test case.

### CTXGRAPH-02 — Just-in-time read protocol in prompts
- **slug:** REQ-read-protocol-in-prompts
- **source:** sinfonia-harness-action-plan.md (1.6); harness-improvement-analysis.md (doc-graph)
- **priority:** P1
- **description:** Add a just-in-time read protocol to every state prompt: start at root
  `AGENTS.md`, follow only links matching the issue's acceptance criteria, grep/glob for the
  rest, never slurp `docs/`.
- **acceptance:** Prompts no longer instruct a blanket `docs/` read.

### CTXGRAPH-03 — Repository Context Contract
- **slug:** REQ-repository-context-contract
- **source:** sinfonia-harness-action-plan.md (2.1, 3.1, 3.2); harness-improvement-analysis.md (Context Contract)
- **priority:** P1
- **description:** Add a Repository Context Contract (new HARNESS-SPEC §N or sibling
  docs/CONTEXT-CONTRACT.md) specifying the doc-graph: node shape + front-matter
  (`title, version, status, owners, last_verified_sha, derived_from`, "See also"),
  nearest-wins AGENTS.md semantics, one-concern/one-screen sizing, read protocol, write
  protocol (node diffs ride in the code PR under CODEOWNERS, version bump, no autonomous
  mutation). Ship `templates/AGENTS.md` (with module-ownership table) and
  `templates/CODEOWNERS`.
- **acceptance:** §7 lists the Context Contract as a REQUIRED/RECOMMENDED convention with a
  conformance checklist line; templates render as <1-screen nodes; a sample node validates.

### CTXGRAPH-04 — Stale + overlap invariant linters
- **slug:** REQ-invariant-linters-stale-and-overlap
- **source:** sinfonia-harness-action-plan.md (2.3); harness-improvement-analysis.md (Refinement 1/2)
- **priority:** P1
- **description:** Extend HARNESS-SPEC §5.5 with two manifest-driven, fail-loud linters: a
  stale-node check (`last_verified_sha` lags `main` beyond a window) and an overlap check
  (two open `sinfonia/*` PRs modify the same owned module).
- **acceptance:** Both checks specified as manifest-driven linters that fail loud locally
  and in CI.

### CTXGRAPH-05 — Decomposition consistency pass
- **slug:** REQ-decomposition-consistency-pass
- **source:** sinfonia-harness-action-plan.md (4.1, 4.2); harness-improvement-analysis.md (Problem 3)
- **priority:** P2
- **description:** Add a pre-implementation cross-artifact consistency/coverage check to the
  spec-deriving step (Spec-Kit pattern): after decomposition, run a "do these
  overlap/contradict?" pass + an ordering pass that emits `blocks` relations sequencing
  shared-infrastructure stories before leaves.
- **acceptance:** Decomposition output includes an overlap report and a `blocks`-relation
  dependency graph; milestone plans show serial foundation then parallel fan-out.

---

## Traceability

100% coverage: all 16 requirements map to exactly one phase.

| Requirement | Slug | Theme | Phase | Status |
|-------------|------|-------|-------|--------|
| BLOCK-02 | REQ-dependency-modeling-invariant | B | Phase 1 | Pending |
| HARNESS-01 | REQ-harness-manifest-ingest | A | Phase 2 | Pending |
| HARNESS-02 | REQ-artifact-references | A | Phase 2 | Pending |
| HARNESS-03 | REQ-manifest-version-gate | A | Phase 2 | Pending |
| HARNESS-04 | REQ-ingestion-optional-degrade | A | Phase 2 | Pending |
| HARNESS-05 | REQ-ingestion-untrusted-input | A | Phase 2 | Pending |
| HARNESS-06 | REQ-feedback-loop-config-surface | A | Phase 2 | Pending |
| BLOCK-01 | REQ-blocker-merged-guardrail | B | Phase 3 | Pending |
| MERGE-01 | REQ-pre-pr-rebase | C | Phase 4 | Pending |
| MERGE-02 | REQ-mergeability-loop | C | Phase 4 | Pending |
| MERGE-03 | REQ-merge-queue-and-post-merge-gate | C | Phase 4 | Pending |
| MERGE-04 | REQ-concurrency-policy-for-conflict-phases | C | Phase 4 | Pending |
| CTXGRAPH-03 | REQ-repository-context-contract | D | Phase 5 | Pending |
| CTXGRAPH-02 | REQ-read-protocol-in-prompts | D | Phase 5 | Pending |
| CTXGRAPH-01 | REQ-overlap-check-before-build | D | Phase 6 | Pending |
| CTXGRAPH-04 | REQ-invariant-linters-stale-and-overlap | D | Phase 6 | Pending |
| CTXGRAPH-05 | REQ-decomposition-consistency-pass | D | Phase 6 | Pending |

**Mapped: 16/16 ✓** — no orphans, no duplicates.
