---
phase: 04-merge-conflict-handling-merge-queue
plan: 03
subsystem: agent-prompt
tags: [workflow, merge-queue, shell, bash, git, github]

# Dependency graph
requires:
  - phase: 04-merge-conflict-handling-merge-queue
    provides: 04-01 and 04-02 established MERGE-01/MERGE-02 base and shared Mergeability procedure
provides:
  - Corrected MERGE-02 mergeability loop with guarded rebase (CR-01)
  - Gate-before-push enforcement in loop body (CR-03)
  - Bounded UNKNOWN polling with UNKNOWN_TRIES counter (CR-02)
  - Conservative *)  catch-all that holds In Progress on ambiguity (WR-06)
  - Self-contained PR_NUM derivation in step-7 block (WR-03/WR-04)
  - Unified UNKNOWN sleep value of 15s across loop and procedure table (WR-05)
  - Clean Mergeability procedure rationale prose (IN-01)
affects:
  - phase-05
  - phase-06

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Guard rebase with `if ! git rebase` before force-push under set -e"
    - "Bounded polling counter (UNKNOWN_TRIES) for all non-deterministic poll arms"
    - "Conservative catch-all: hold state and re-poll on unexpected/null status"
    - "Self-derive context variables (PR_NUM) at top of each independent bash block"

key-files:
  created: []
  modified:
    - docker/WORKFLOW.example.md

key-decisions:
  - "Gate discovery remains repo-discovered (D-01): loop body prompts agent to find the gate from .github/workflows/, README, or harness docs; no stack hardcoded"
  - "UNKNOWN sleep unified at 15s (chosen over 10s from procedure table) as the single authoritative value"
  - "Conservative *)  catch-all uses UNKNOWN_TRIES budget (shared with UNKNOWN arm) rather than a separate counter"

patterns-established:
  - "Guarded rebase pattern: `if ! git rebase origin/main; then` narrative-hand-back + break rather than blind push"
  - "gate-then-conditional-push: discover gate, run it, abort on red, only then force-push"

requirements-completed: [MERGE-01, MERGE-02]

# Metrics
duration: 2min
completed: 2026-06-14
---

# Phase 4 Plan 03: MERGE-02 Loop Correctness Summary

**Closed three blocking correctness defects (CR-01/02/03) and four wiring defects (WR-03/04/05/06 plus IN-01) in the MERGE-02 mergeability loop and Mergeability procedure of `docker/WORKFLOW.example.md`**

## Performance

- **Duration:** 2 min
- **Started:** 2026-06-14T19:07:33Z
- **Completed:** 2026-06-14T19:09:45Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- Guarded the rebase in `DIRTY|BEHIND` arm so `set -e` no longer aborts mid-conflict, leaving workspace broken (CR-01)
- Added explicit gate discovery + gate-then-conditional-push step in loop body; the dead comment "# Re-run the repo-discovered gate; abort if red" now has real code behind it (CR-03)
- Added `UNKNOWN_TRIES` counter with cap of 5 in both the `UNKNOWN` and `*)` arms; UNKNOWN can no longer spin forever (CR-02)
- Made `*)` catch-all conservative: holds In Progress and re-polls instead of treating null/unexpected status as conflict-free (WR-06)
- Self-derives `PR_NUM` via `gh pr list --head "$BRANCH"` at top of step-7 block with empty-value guard (WR-03/WR-04)
- Updated procedure branch table to say `sleep 15` for UNKNOWN (was `sleep 10`), matching the loop body (WR-05)
- Rewrote garbled Mergeability procedure rationale to clean prose covering force-push safety and merge-queue rebase-and-test (IN-01)

## Task Commits

Each task was committed atomically:

1. **Task 1: Fix MERGE-02 loop body (CR-01/02/03, WR-03/04/06)** - `b982e6a` (fix)
2. **Task 2: Reconcile UNKNOWN sleep and rationale prose (WR-05, IN-01)** - `8d3025a` (fix)

**Plan metadata:** (included in final docs commit)

## Files Created/Modified

- `docker/WORKFLOW.example.md` - Corrected MERGE-02 mergeability loop: guarded rebase, gate-before-push, bounded UNKNOWN, conservative catch-all, self-derived PR_NUM, unified sleep value, clean rationale

## Decisions Made

- Gate discovery in the loop body mirrors D-01 exactly: the loop now prompts the agent to discover the gate from `.github/workflows/`, README, or the harness's documented command rather than hardcoding any stack-specific invocation.
- UNKNOWN sleep unified at 15s: the loop already used 15s; the procedure table was updated to match (not the reverse), keeping the single behavior the loop had already established.
- Conservative catch-all shares the `UNKNOWN_TRIES` budget rather than a separate counter, so any persistent ambiguity (UNKNOWN or unexpected status) converges to the same bounded exit.

## Deviations from Plan

None - plan executed exactly as written. All fixes applied as specified in the plan's action steps, using the review snippets from 04-REVIEW.md as reference for the target shell patterns.

## Issues Encountered

None.

## Threat Flags

No new runtime attack surface introduced. This plan edits agent-prompt prose only. The fixes reduce risk: CR-01 prevents half-finished rebase-in-progress workspaces, CR-03 restores the green-gate-before-push invariant, CR-02 removes an infinite-loop hazard, WR-06 makes the gate fail closed on ambiguity.

## Self-Check

Files exist:
- `docker/WORKFLOW.example.md` - FOUND (modified in place)

Commits exist:
- `b982e6a` - fix(04-03): close CR-01/CR-02/CR-03/WR-03/WR-04/WR-06 in MERGE-02 loop
- `8d3025a` - fix(04-03): close WR-05 and IN-01 in Mergeability procedure

## Next Phase Readiness

- MERGE-01 and MERGE-02 requirements are now correctly implemented in `docker/WORKFLOW.example.md`
- All seven defects from 04-REVIEW.md that were assigned to this plan are closed
- Phase 4 plan 04 (MERGE-03/MERGE-04: spec amendments and concurrency comment) can proceed independently

---
*Phase: 04-merge-conflict-handling-merge-queue*
*Completed: 2026-06-14*
