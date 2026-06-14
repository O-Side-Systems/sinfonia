---
phase: 04-merge-conflict-handling-merge-queue
plan: "02"
subsystem: docs
tags: [merge-queue, branch-protection, harness-spec, deployment, spec, merge-conflict]

# Dependency graph
requires:
  - phase: 04-merge-conflict-handling-merge-queue
    provides: CONTEXT.md and PATTERNS.md establishing D-01..D-12 decisions and edit targets

provides:
  - "HARNESS-SPEC §7.4 normative amendment: merge-queue rebase-and-test + post-merge gate + D-03 mergeable-not-CLEAN refinement + serial-foundation convention"
  - "HARNESS-SPEC §9 new conformance checklist bullet with (§7.4) back-reference"
  - "DEPLOYMENT.md operator section: Merge queue & branch protection (branch protection, merge queue settings, post-merge gate, serial-foundation note)"
  - "SPEC.md §8.2 Phase 4 cross-reference to HARNESS-SPEC §7.4 for serial-foundation concurrency"

affects:
  - 04-merge-conflict-handling-merge-queue (plan 03 -- WORKFLOW.md proactive gates reference §7.4 convention)
  - any future phase touching merge workflow or branch protection

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "D-03 mergeable-not-CLEAN: gate on DIRTY/BEHIND only; BLOCKED/UNSTABLE are ready-for-human"
    - "Serial-foundation convention: max_concurrent_agents_by_state 'In Progress': 1 enforces serial milestone progression"
    - "Post-merge harness gate: CI push-to-main trigger catches integration failures"

key-files:
  created: []
  modified:
    - docs/HARNESS-SPEC.md
    - docs/DEPLOYMENT.md
    - docs/SPEC.md

key-decisions:
  - "D-03 recorded normatively in §7.4: gate is 'mergeable w.r.t. main' (DIRTY/BEHIND loop only); literal-CLEAN deadlock tension explicitly noted to prevent reversion"
  - "D-10: serial-foundation convention is in §7.4 and cross-referenced from SPEC §8.2 and DEPLOYMENT.md"
  - "D-11 honored: all changes land in /docs only; no .planning/ content leaks into shipped docs"
  - "Post-merge gate documented as target-repo CI concern (not Sinfonia code)"

patterns-established:
  - "Note (v0.4 Phase N) sub-bullet pattern in SPEC §8.2 for milestone-specific amendments"
  - "HARNESS-SPEC §7.4 + §9 as the canonical home for merge-gating normative requirements"
  - "DEPLOYMENT.md ## sections with ### subsections + trailing --- for operator guidance"

requirements-completed: [MERGE-03, MERGE-04]

# Metrics
duration: 3min
completed: 2026-06-14
---

# Phase 04 Plan 02: Merge Queue & Post-Merge Gate Spec Amendment Summary

**HARNESS-SPEC §7.4 normatively records the merge-queue rebase-and-test requirement, post-merge gate on main, D-03 mergeable-not-CLEAN gate refinement (with BLOCKED/UNSTABLE tension note), and serial-foundation convention; DEPLOYMENT.md adds the operator setup section; SPEC.md §8.2 cross-references the serial-foundation convention.**

## Performance

- **Duration:** 3 min
- **Started:** 2026-06-14T18:13:56Z
- **Completed:** 2026-06-14T18:16:50Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Amended HARNESS-SPEC §7.4 with four normative additions: merge-queue rebase-and-test (DEC-001/MERGE-03), post-merge harness gate on `main`, D-03 mergeable-not-CLEAN gate refinement with explicit CLEAN-deadlock tension note, and serial-foundation / leaf-fan-out convention (D-10/MERGE-04)
- Added §9 conformance checklist bullet with mandatory `(§7.4)` back-reference covering merge queue, post-merge gate, and agent mergeability gate
- Added "Merge queue & branch protection" operator section to DEPLOYMENT.md before `## Upgrading`, covering branch protection rule, merge queue settings (rebase-and-test), post-merge gate, and serial-foundation cross-reference; explicitly states no Sinfonia code change needed
- Added `**Note (v0.4 Phase 4):**` sub-bullet to SPEC.md §8.2 cross-referencing HARNESS-SPEC §7.4 for serial-foundation concurrency via `max_concurrent_agents_by_state: "In Progress": 1`

## Task Commits

Each task was committed atomically:

1. **Task 1: Amend HARNESS-SPEC §7.4 + §9 checklist** - `c93e564` (docs)
2. **Task 2: Add DEPLOYMENT.md operator section + SPEC.md cross-reference** - `9cbe426` (docs)

**Plan metadata:** (committed below)

## Files Created/Modified

- `docs/HARNESS-SPEC.md` - §7.4 amended with 4 normative additions; §9 gains one new checklist bullet with (§7.4) back-reference
- `docs/DEPLOYMENT.md` - New `## Merge queue & branch protection` section inserted before `## Upgrading`
- `docs/SPEC.md` - §8.2 gains `Note (v0.4 Phase 4)` serial-foundation cross-reference sub-bullet

## Decisions Made

- Located `**Note (v0.4 Phase 4):**` in §8.2 (after Phase 3 note) rather than §8.3 -- the Phase 3 note is the direct template and the §8.2 location keeps the milestone amendment pattern coherent.
- D-03 tension note uses explicit language calling out the deadlock scenario (BLOCKED PR awaiting required review can never be CLEAN) to immunize against future "fix" reversions. This is load-bearing per the plan's constraints.

## Deviations from Plan

None -- plan executed exactly as written. The Task 1 automated verification had a false negative on `grep -F "7.4" | grep -qF "[ ]"` because the §9 checklist bullet spans multiple lines (the `[ ]` is on a different line than the `(§7.4)` continuation). Manual inspection confirmed the content satisfies all acceptance criteria.

## Issues Encountered

- Verification test `grep -F "---"` failed on macOS because `grep` interpreted `---` as flags. Confirmed trailing `---` rule present via `awk ... | tail -5`. No content issue.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness

- HARNESS-SPEC §7.4 is now the normative home for merge-gating requirements; Plan 03 (WORKFLOW.md proactive gates) can reference it as the authoritative source.
- DEPLOYMENT.md operator section is complete and standalone; no further doc changes needed for the merge-queue / branch-protection guidance.
- The D-03 mergeable-not-CLEAN refinement is on record in §7.4 -- Plan 03's prompt implementation can reference the same gate logic without re-litigating the CLEAN vs BLOCKED distinction.

## Self-Check: PASSED

- docs/HARNESS-SPEC.md: FOUND
- docs/DEPLOYMENT.md: FOUND
- docs/SPEC.md: FOUND
- 04-02-SUMMARY.md: FOUND
- c93e564 (Task 1): FOUND
- 9cbe426 (Task 2): FOUND

---
*Phase: 04-merge-conflict-handling-merge-queue*
*Completed: 2026-06-14*
