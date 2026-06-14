# Roadmap: Sinfonia (v0.4)

## Overview

The v0.4 milestone hardens Sinfonia's agentic loop against three observed failure
modes and closes out the harness feedback-ingestion work. We start by verifying the
actual orchestrator gating behavior in code (the UNVERIFIED `{% if issue.children %}`
parent-child assumption), then close out Theme A (harness manifest ingestion, already
reflected in the spec), then build dependency gating, merge-conflict handling under the
ratified GitHub-native-merge-queue model, and finally the repository context graph and
decomposition discipline that prevents two stories from building the same thing.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [x] **Phase 1: Orchestrator Gating Verification** - Confirm in code how dependency/parent-child gating actually works before building on it (completed 2026-06-14)
- [x] **Phase 2: Harness Manifest Ingestion Closure** - Verify and close Proposal 0001's opt-in `bridge.json` ingestion path (completed 2026-06-14)
- [ ] **Phase 3: Dependency Gating (Blocker-Merged Guardrail)** - Gate work on blocker PRs being merged to main, keyed on Linear `blocks`
- [ ] **Phase 4: Merge-Conflict Handling & Merge Queue** - Pre-PR rebase, mergeability loop, merge queue + post-merge gate, conflict concurrency
- [ ] **Phase 5: Repository Context Contract** - Doc-graph contract, nearest-wins AGENTS.md, just-in-time read protocol, templates
- [ ] **Phase 6: Decomposition Discipline & Invariant Linters** - Overlap-before-build, stale/overlap linters, decomposition consistency pass

## Phase Details

### Phase 1: Orchestrator Gating Verification
**Goal**: Establish ground truth on how the orchestrator actually gates dispatch (blocker and parent-child) so Theme B/C work builds on verified behavior, not the UNVERIFIED WORKFLOW.example.md assumption.
**Depends on**: Nothing (first phase)
**Requirements**: BLOCK-02
**Success Criteria** (what must be TRUE):
  1. The dispatch/eligibility logic in `crates/sinfonia/src/orchestrator/dispatch.rs` is read and documented: exactly which relations gate dispatch, and in which states (per SPEC §8.2).
  2. The `{% if issue.children %}` parent-child gating claim in WORKFLOW.example.md is confirmed against code and either backed by a code reference or removed.
  3. A written invariant states dependency gating keys only on Linear `blocks` relations (not hierarchy/related/prose), captured for SPEC §8.2 amendment.
  4. The current behavior (blocker rule applies only to `Todo`; `In Progress` ignores blockers; gate opens on terminal state, not PR-merge) is confirmed as the baseline the next phases amend.
**Plans**: 2 plans
  - [x] 01-01-PLAN.md — Blocker-gate pinning tests + docker workflow code-ref annotation (Wave 1)
  - [x] 01-02-PLAN.md — 01-VERIFICATION.md findings doc + durable 0002 gating-ground-truth ADR (Wave 2)

### Phase 2: Harness Manifest Ingestion Closure
**Goal**: The opt-in `bridge.json` ingestion path (Proposal 0001, already reflected in SPEC §11.6.13/§12.5) is verified end-to-end against its contract, defaults are flipped where fixtures are green, and the adversarial surface is closed.
**Depends on**: Phase 1
**Requirements**: HARNESS-01, HARNESS-02, HARNESS-03, HARNESS-04, HARNESS-05, HARNESS-06
**Success Criteria** (what must be TRUE):
  1. On a red `workflow_run` for a mapped PR with a parseable `schema_version: 2` `bridge.json`, `sinfonia_last_ci_failure` renders scenario/feature_file/step/assertion per failing scenario (not a check-name list).
  2. The version gate behaves per the matrix: supported → ingest, newer → warn+forward-read, older/absent/unparseable → warn+check-name fallback; every non-additive degradation row falls back to today's path.
  3. Adversarial fixtures pass: size cap (5 MiB), zip-bomb decompressed cap, `max_failures_parsed`=20, `max_failure_digest_bytes`=8 KiB, in-memory parse, no disk write, no server-side `artifact_urls` resolution, scalar injection, control chars stripped.
  4. The `feedback_loop` config surface exists in `BRIDGE.md` with safe defaults; absent/`false` master switch reproduces today's behavior with no `WORKFLOW.md` change.
**Plans**: 2 plans
  - [x] 02-01-PLAN.md — Gap-closing tests: golden-snapshot exact field rendering + no-disk-write proof (Wave 1)
  - [x] 02-02-PLAN.md — Closure docs: BRIDGE.example.md note, Proposal 0001 status flips, 02-CLOSURE.md evidence map (Wave 2)

### Phase 3: Dependency Gating (Blocker-Merged Guardrail)
**Goal**: Agents refuse to start work whose blockers have not actually merged to `main`, closing the §8.2 gap where `In Progress` issues ignore blockers and a terminal-but-unmerged blocker lets work proceed.
**Depends on**: Phase 1
**Requirements**: BLOCK-01
**Success Criteria** (what must be TRUE):
  1. Workflow STEP 0 (both `Todo` and `In Progress` prompts) resolves the issue's `blocks`/`blocked-by` set from Linear and checks each blocker's PR is merged to `main`.
  2. An issue with an unmerged blocker exits without code changes and leaves a Linear comment.
  3. The blocker-merged guardrail keys only on Linear `blocks` relations, consistent with the Phase 1 invariant.
  4. The §8.2 amendment (gate on PR-merge, not merely terminal state) is documented as a scoped delta to the current contract.
**Plans**: 3 plans
  - [ ] 03-01-PLAN.md — Remove parent-child dispatch gate (Rust) + inverse pinning test (Wave 1)
  - [ ] 03-02-PLAN.md — STEP 0 blocker-merged guardrail in both prompts + marker regression test (Wave 1)
  - [ ] 03-03-PLAN.md — SPEC §8.2 amendment + ADR 0002 RESOLVED (Wave 2)

### Phase 4: Merge-Conflict Handling & Merge Queue
**Goal**: Agent PRs land cleanly under the ratified GitHub native merge queue + serial-foundation model — branches are rebased on current `main` with a green harness before PR creation, mergeability is enforced before `In Review`, and a post-merge gate runs on `main`.
**Depends on**: Phase 3
**Requirements**: MERGE-01, MERGE-02, MERGE-03, MERGE-04
**Success Criteria** (what must be TRUE):
  1. PRs are opened only from a branch rebased on current `origin/main` with a green harness run.
  2. An issue reaches `In Review` only when `gh pr view --json mergeStateStatus` reports `CLEAN`, after a bounded rebase/resolve/re-run/force-push loop on non-CLEAN.
  3. HARNESS-SPEC §7.4 names the merge-queue rebase-and-test requirement and a post-merge harness gate on `main`; §9 checklist is updated and an operator branch-protection/merge-queue setup note exists.
  4. `In Progress` concurrency stays at 1 and the serial-foundation / leaf-fan-out convention is documented in the concurrency config and at milestone level.
**Plans**: TBD

### Phase 5: Repository Context Contract
**Goal**: A reviewed, hierarchical doc-graph exists so agents read just enough context — nearest-wins `AGENTS.md` with a module-ownership map, a Repository Context Contract defining node shape and read/write protocol, and prompts that stop slurping `docs/`.
**Depends on**: Phase 4
**Requirements**: CTXGRAPH-03, CTXGRAPH-02
**Success Criteria** (what must be TRUE):
  1. A Repository Context Contract (HARNESS-SPEC §N or `docs/CONTEXT-CONTRACT.md`) specifies node front-matter, nearest-wins AGENTS.md semantics, one-concern/one-screen sizing, and read/write protocol (diffs ride the code PR under CODEOWNERS, version bump, no autonomous mutation).
  2. `templates/AGENTS.md` (with module-ownership table) and `templates/CODEOWNERS` ship and render as <1-screen nodes; a sample node validates against the contract.
  3. §7 lists the Context Contract as a REQUIRED/RECOMMENDED convention with a conformance checklist line.
  4. Every state prompt uses the just-in-time read protocol (start at root `AGENTS.md`, follow only acceptance-matching links, grep/glob for the rest) and no longer instructs a blanket `docs/` read.
**Plans**: TBD
**UI hint**: yes

### Phase 6: Decomposition Discipline & Invariant Linters
**Goal**: Two stories never build the same thing — agents check for overlap before writing shared code, fail-loud linters catch stale/overlapping nodes, and decomposition emits a `blocks`-ordered dependency graph that serializes foundation before fan-out.
**Depends on**: Phase 5
**Requirements**: CTXGRAPH-01, CTXGRAPH-04, CTXGRAPH-05
**Success Criteria** (what must be TRUE):
  1. Before writing shared/utility code an agent consults the module-ownership map and scans sibling `sinfonia/*` branches/open PRs; in a seeded test it logs the sibling-scan and chooses reuse-or-block (adds `blocks`, stops) over duplicate implementation.
  2. Two manifest-driven, fail-loud linters are specified and run locally and in CI: a stale-node check (`last_verified_sha` lags `main`) and an overlap check (two open `sinfonia/*` PRs modify the same owned module).
  3. The decomposition step emits an overlap report and a `blocks`-relation dependency graph; milestone plans show serial foundation then parallel fan-out.
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Orchestrator Gating Verification | 2/2 | Complete   | 2026-06-14 |
| 2. Harness Manifest Ingestion Closure | 2/2 | Complete   | 2026-06-14 |
| 3. Dependency Gating (Blocker-Merged Guardrail) | 0/3 | Planned | - |
| 4. Merge-Conflict Handling & Merge Queue | 0/TBD | Not started | - |
| 5. Repository Context Contract | 0/TBD | Not started | - |
| 6. Decomposition Discipline & Invariant Linters | 0/TBD | Not started | - |
