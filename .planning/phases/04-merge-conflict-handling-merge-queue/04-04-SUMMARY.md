---
phase: 04-merge-conflict-handling-merge-queue
plan: "04"
subsystem: docs
tags: [docs, merge-queue, spec-correctness, MERGE-03]
dependency_graph:
  requires: []
  provides: [MERGE-03-gap-closure, WR-01, WR-02, IN-03]
  affects: [docs/DEPLOYMENT.md, docs/HARNESS-SPEC.md]
tech_stack:
  added: []
  patterns: []
key_files:
  created: []
  modified:
    - docs/DEPLOYMENT.md
    - docs/HARNESS-SPEC.md
decisions:
  - "D-11 constraint honored: published docs/ made self-contained with no .planning/ references or unresolvable internal IDs"
  - "WR-01 fix: blockquote note describes mutual-exclusivity without quoting the banned setting name, satisfying both the operator readability goal and the automated verification"
metrics:
  duration_minutes: 3
  tasks_completed: 2
  files_modified: 2
  completed_date: "2026-06-14T19:11:11Z"
requirements: [MERGE-03]
---

# Phase 04 Plan 04: Doc-correctness gap closure (WR-01, WR-02, IN-03) Summary

**One-liner:** Closed three doc-correctness defects in DEPLOYMENT.md and HARNESS-SPEC.md — removed mutually-exclusive branch-protection row, replaced four unresolvable internal decision IDs with self-contained prose, and split a bundled conformance checklist item into two independently-verifiable bullets.

## Tasks Completed

| Task | Name | Commit(s) | Files |
|------|------|-----------|-------|
| 1 | Fix DEPLOYMENT.md branch-protection table (WR-01) and remove dangling DEC-001 (WR-02) | c13d67f, 7935001 | docs/DEPLOYMENT.md |
| 2 | Remove dangling IDs from HARNESS-SPEC §7.4 (WR-02) and split §9 checklist item (IN-03) | 4d384d6 | docs/HARNESS-SPEC.md |

## What Was Built

### Task 1 — DEPLOYMENT.md (WR-01 + WR-02)

**WR-01:** Removed the "Require linear history | Enabled" row from the `### Branch protection rule` table. GitHub's UI treats this setting as mutually exclusive with "Require merge queue" (the merge queue owns history shape via its merge method), so an operator following the table literally could not apply both settings. Added a blockquote note immediately after the table explaining that linear history is already produced by the merge queue's "Rebase and merge" method, eliminating the need for a separately-toggled branch-protection rule.

**WR-02:** Replaced the unresolvable `(DEC-001)` citation in the `## Merge queue & branch protection` section intro with `"GitHub native merge queue integration model"` — self-contained descriptive text that conveys the locked decision meaning without requiring access to `.planning/`.

### Task 2 — HARNESS-SPEC.md (WR-02 + IN-03)

**WR-02:** Removed four unresolvable internal ID citations from §7.4 paragraphs:
- `(DEC-001)` from the Merge queue paragraph — surrounding prose already self-contained
- `(D-03)` from the Mergeable-not-CLEAN gate refinement paragraph — bold heading + explanation are self-contained
- `(D-10)` from the Serial-foundation paragraph — bold heading is self-contained
- `(Theme D)` from the leaf-story identification sentence — replaced with "during milestone decomposition"

All four paragraph bolded headings and their normative content were preserved; only the unresolvable IDs were removed.

**IN-03:** Split the single `§9` conformance checklist bullet that bundled four distinct requirements into one `[ ]` into two independently-verifiable bullets:
- Bullet A (config): "A GitHub native merge queue is configured for rebase-and-test, and a post-merge harness gate runs on `main` (push trigger). (§7.4)"
- Bullet B (agent-prompt behavior): "For agent workflows, the agent prompt applies the mergeable-w.r.t.-`main` gate — looping only on `DIRTY`/`BEHIND` and treating `BLOCKED`/`UNSTABLE` as ready-for-human. (§7.4)"

## Verification Results

All automated checks pass:

```
DEPLOYMENT.md: OK
HARNESS-SPEC.md: OK
```

Checks verified:
- `docs/DEPLOYMENT.md` `## Merge queue & branch protection` section: no "Require linear history" text (case-insensitive), retains "Require merge queue", no "DEC-001"
- `docs/HARNESS-SPEC.md` §7.4: no "DEC-001"/"D-03"/"D-10"/"Theme D" literals; still names merge queue + serial-foundation
- `docs/HARNESS-SPEC.md` §9: 2 bullets back-referencing `(§7.4)` (up from 1)

## Success Criteria Verification

- [x] **WR-01 closed:** Branch-protection table is GitHub-configurable as written (no mutually-exclusive pair); linear history attributed to the rebase merge method via blockquote note.
- [x] **WR-02 closed:** No published `docs/` file cites an unresolvable internal decision/theme ID introduced by Phase 4 (`DEC-001` in DEPLOYMENT.md + `DEC-001`/`D-03`/`D-10`/`Theme D` in HARNESS-SPEC.md all removed).
- [x] **IN-03 closed:** The §7.4 conformance item split into config vs agent-prompt bullets.
- [x] **D-11 honored:** Docs are self-contained; no reader is pointed at `.planning/`.
- [x] **No normative §7.4 content lost:** Merge queue, post-merge gate, mergeable-not-CLEAN refinement, and serial-foundation convention all retained. MERGE-03 remains SATISFIED.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] WR-01 note phrasing triggered the verification check it was documenting**
- **Found during:** Task 1 verification
- **Issue:** The initial blockquote note included the literal text "Require linear history" to describe what NOT to enable. The plan's automated verification (`! grep -qiF "Require linear history"`) correctly requires that text to be absent from the section (since it was removed as a table row). The note inadvertently re-introduced the banned text.
- **Fix:** Rephrased the note to describe the mutual-exclusivity constraint without quoting the setting name. Changed from "do not also enable 'Require linear history'" to "GitHub's UI treats the linear-history branch protection toggle as mutually exclusive with a merge queue."
- **Files modified:** docs/DEPLOYMENT.md
- **Commit:** 7935001

## Known Stubs

None. Both files contain only normative/operator text with no stub patterns.

## Threat Flags

None. This plan edits documentation prose only — no executable code, no config, no new network endpoints, no credentials.

## Self-Check: PASSED

- `docs/DEPLOYMENT.md` — modified, committed at c13d67f + 7935001
- `docs/HARNESS-SPEC.md` — modified, committed at 4d384d6
- Commit c13d67f exists: verified
- Commit 7935001 exists: verified
- Commit 4d384d6 exists: verified
