---
phase: 05
slug: repository-context-contract
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-14
---

# Phase 05 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> This is a **documentation-authoring** phase: validation is POSIX smoke checks
> (grep/wc/ls) plus a manual conformance checklist defined by the contract itself.
> Automated stale/overlap linters are deferred to Phase 6 (CTXGRAPH-04).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Manual conformance checklist + POSIX smoke (`grep`/`wc`/`ls`) — no automated test framework for this phase |
| **Config file** | `docs/CONTEXT-CONTRACT.md` §Conformance Checklist (the checklist IS the test) — created in Wave 0 |
| **Quick run command** | `grep -n "Orient.*docs/" docker/WORKFLOW.md docker/WORKFLOW.example.md` (expect 0 hits) + per-node `wc -l` ≤ one screen |
| **Full suite command** | Step through `docs/CONTEXT-CONTRACT.md` §Conformance Checklist for all 7 `AGENTS.md` nodes (6 dogfood + 1 template) |
| **Estimated runtime** | <10 seconds (smoke) + manual review |

---

## Sampling Rate

- **After every task commit:** Run the smoke commands below (all < 5s, pure grep/ls/wc)
- **After every plan wave:** Full manual conformance checklist for every authored `AGENTS.md` node
- **Before `/gsd:verify-work`:** All smoke commands green AND manual checklist complete
- **Max feedback latency:** ~10 seconds

---

## Per-Task Verification Map

> Task IDs are filled in once plans are authored. Rows are keyed to the phase
> requirements and the research's smoke checks; the planner links each to a task.

| Task (TBD) | Requirement | Behavior | Test Type | Automated Command | Status |
|------------|-------------|----------|-----------|-------------------|--------|
| TBD | CTXGRAPH-03 | Contract exists | smoke | `ls docs/CONTEXT-CONTRACT.md` | ⬜ pending |
| TBD | CTXGRAPH-03 | Each dogfood node passes the conformance checklist | manual | Run §Conformance Checklist per node (7 nodes) | ⬜ pending |
| TBD | CTXGRAPH-03 | Templates exist and render <1 screen | smoke | `wc -l templates/AGENTS.md templates/CODEOWNERS` (≤ ~58 lines) | ⬜ pending |
| TBD | CTXGRAPH-03 | HARNESS-SPEC §7 references the Context Contract | smoke | `grep -c 'CONTEXT-CONTRACT' docs/HARNESS-SPEC.md` (≥ 1) | ⬜ pending |
| TBD | CTXGRAPH-03 | HARNESS-SPEC §9-style conformance line added | smoke | `grep -c 'CONTEXT-CONTRACT\|AGENTS.md' docs/HARNESS-SPEC.md` (≥ 2) | ⬜ pending |
| TBD | CTXGRAPH-02 | No blanket `docs/` read remains in WORKFLOW files | smoke | `grep -n "Orient.*docs/" docker/WORKFLOW.md docker/WORKFLOW.example.md` (0 hits) | ⬜ pending |
| TBD | CTXGRAPH-02 | JIT orient (`AGENTS.md`) present in prompts | smoke | `grep -c "AGENTS.md" docker/WORKFLOW.md docker/WORKFLOW.example.md` (≥ 1 each) | ⬜ pending |
| TBD | CTXGRAPH-02 | No `CLAUDE.md` reference in orient step | smoke | `grep -n "CLAUDE.md" docker/WORKFLOW.md docker/WORKFLOW.example.md` (0 hits) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

All target artifacts are net-new (nothing exists yet):

- [ ] `docs/CONTEXT-CONTRACT.md` — the contract + conformance checklist (the "test")
- [ ] `AGENTS.md` (root) — entry node
- [ ] `crates/sinfonia/AGENTS.md`
- [ ] `crates/sinfonia-bridge/AGENTS.md`
- [ ] `crates/sinfonia-tracker/AGENTS.md`
- [ ] `docker/AGENTS.md`
- [ ] `docs/AGENTS.md`
- [ ] `templates/AGENTS.md` (and the `templates/` dir)
- [ ] `templates/CODEOWNERS`

No test framework install is required — smoke checks use standard POSIX tools (`git`, `grep`, `wc`, `ls`), all confirmed available.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Each `AGENTS.md` node satisfies one-concern/one-screen sizing and front-matter completeness | CTXGRAPH-03 | Doc quality/structure judgment can't be fully asserted by grep | Walk the §Conformance Checklist in `docs/CONTEXT-CONTRACT.md` for each node: front-matter fields present, nearest-wins scope correct, module-ownership table matches CODEOWNERS, "See also" links resolve, body ≤ one screen |
| Nearest-wins resolution + acceptance-matching link-following reads correctly in prompts | CTXGRAPH-02 | Prompt clarity for an executing agent is a review judgment | Read the rewritten orient step in both WORKFLOW files; confirm an agent can deterministically pick the applicable node and follow only matching links |
| `last_verified_sha` set from `git rev-parse --short HEAD` at merge time | CTXGRAPH-03 | No automated stale linter until Phase 6 | Confirm the contract documents the manual update procedure; sample node carries a real short SHA present in `main` ancestry |

---

## Validation Sign-Off

- [ ] Every smoke check above maps to a task or Wave 0 dependency
- [ ] Sampling continuity: no 3 consecutive tasks without a smoke/manual verify
- [ ] Wave 0 creates all listed artifacts
- [ ] No watch-mode flags
- [ ] Manual conformance checklist exists in `docs/CONTEXT-CONTRACT.md` and covers all node fields
- [ ] `nyquist_compliant: true` set in frontmatter (after plan-checker confirms coverage)

**Approval:** pending
