---
phase: 04-merge-conflict-handling-merge-queue
plan: 01
subsystem: workflow
tags: [workflow, merge-queue, git, rebase, linear, github]

# Dependency graph
requires:
  - phase: 03-dependency-gating-blocker-merged-guardrail
    provides: idempotent-comment-and-stop pattern (BLOCK-01 / Phase 3) reused for D-02/D-05 markers
provides:
  - Mergeability procedure: single source of truth for rebase + force-push + gate re-run in the In Progress prompt (D-07)
  - MERGE-01: proactive pre-PR gate (fetch/rebase/gate before push; idempotent stop on red)
  - MERGE-02: bounded mergeability loop (3-attempt cap; DIRTY/BEHIND loop; BLOCKED/UNSTABLE/CLEAN proceed; exhaustion stop)
  - MERGE-04 (prompt half): concurrency comment on "In Progress": 1 referencing serial-foundation convention and HARNESS-SPEC §7.4
  - D-06: rebase + --force-with-lease only; "or merge" hedge removed
  - D-03 refinement: BLOCKED/UNSTABLE/CLEAN = conflict-free (not just CLEAN)
affects:
  - phase-05-decomposition-context-graph (builds on serial-foundation convention established here)
  - agents reading WORKFLOW.example.md (they now receive proactive merge gates)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Mergeability procedure: named in-prompt section referenced by multiple callers (D-07 single-source-of-truth)"
    - "Idempotent Linear comment-and-stop: marker probe via curl/jq + grep -qF, exit 0 without state change (Phase 3 pattern reuse)"
    - "mergeStateStatus D-03 table: DIRTY/BEHIND=loop, UNKNOWN=re-poll, BLOCKED/UNSTABLE/CLEAN=proceed"

key-files:
  created: []
  modified:
    - docker/WORKFLOW.example.md

key-decisions:
  - "D-06: rebase + --force-with-lease only on sinfonia/<id>; drop 'or merge — match the project's convention' hedge"
  - "D-07: one shared Mergeability procedure section referenced by both proactive gates and the reactive STEP 1 handler"
  - "D-03 applied: BLOCKED/UNSTABLE/CLEAN are all conflict-free; only DIRTY/BEHIND keep the loop running"
  - "MERGE-02 cap set at 3 attempts (D-04 principle: bounded loop + re-poll on exhaustion)"

patterns-established:
  - "Mergeability procedure: reusable labeled in-prompt section for rebase mechanics"
  - "Pre-PR gate: follow Mergeability procedure before push; hard stop on red (idempotent comment + exit 0)"
  - "Mergeability loop: bounded 3-attempt poll/loop gating In Review transition"

requirements-completed: [MERGE-01, MERGE-02, MERGE-04]

# Metrics
duration: 4min
completed: 2026-06-14
---

# Phase 4 Plan 01: Proactive Pre-PR Merge Gates for WORKFLOW.example.md

**Proactive pre-PR rebase + green-gate (MERGE-01) and bounded mergeability loop (MERGE-02) added to the In Progress prompt in docker/WORKFLOW.example.md, with a shared Mergeability procedure section as single source of truth (D-07) and a serial-foundation concurrency comment (MERGE-04)**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-06-14T18:14:00Z
- **Completed:** 2026-06-14T18:18:59Z
- **Tasks:** 3
- **Files modified:** 1 (docker/WORKFLOW.example.md)

## Accomplishments

- Added `### Mergeability procedure` labeled subsection as the single source of truth for the rebase mechanic: `git fetch origin`, `git rebase origin/main`, repo-discovered gate re-run (D-01), `git push --force-with-lease`, and `mergeStateStatus` re-poll with D-03 transition table
- Refactored both the Todo STEP 1 and In Progress STEP 1 conflict bullets to reference "follow the **Mergeability procedure** below" instead of inline steps; removed the "or merge — match the project's convention" hedge (D-06)
- Inserted MERGE-01 pre-PR gate (step 4) before the push step: agent follows Mergeability procedure before pushing; on red gate posts idempotent Linear comment (`sinfonia-bot: pre-pr-gate-red`) and exits without pushing or opening a PR (D-02)
- Inserted MERGE-02 mergeability loop (step 7) before the In Review transition: bounded 3-attempt poll on `mergeStateStatus`; loops on DIRTY/BEHIND; re-polls on UNKNOWN; proceeds on BLOCKED/UNSTABLE/CLEAN (D-03/D-04); on exhaustion posts idempotent comment (`sinfonia-bot: mergeability-loop-exhausted`) and exits without transitioning (D-05)
- Added serial-foundation concurrency comment above `"In Progress": 1` in `max_concurrent_agents_by_state`, referencing HARNESS-SPEC §7.4 (MERGE-04/D-12)

## Task Commits

1. **Task 1: Add Mergeability procedure and refactor STEP 1 (D-06/D-07)** - `7c8eb46` (feat)
2. **Task 2: Add pre-PR gate (MERGE-01) and mergeability loop (MERGE-02)** - `d9f1f26` (feat)
3. **Task 3: Add MERGE-04 concurrency comment (D-12)** - `faaec35` (feat)

**Plan metadata:** (committed with SUMMARY.md)

## Files Created/Modified

- `docker/WORKFLOW.example.md` - Added Mergeability procedure section, MERGE-01 pre-PR gate (step 4), MERGE-02 mergeability loop (step 7), MERGE-04 concurrency comment; refactored STEP 1 reactive handler in both Todo and In Progress prompts; renumbered Implementation continuation steps 4-8

## Decisions Made

- **D-06 applied:** Standardized on `git rebase` + `--force-with-lease` only; removed "or merge — match the project's convention" hedge from both Todo STEP 1 and In Progress STEP 1
- **D-07 applied:** Single `### Mergeability procedure` section referenced by name by both the STEP 1 reactive handler (both prompts) and the new proactive MERGE-01 step 4; avoids prompt duplication
- **D-03 refinement applied:** STEP 1 re-check language updated from "If clean" to "If conflict-free (BLOCKED, UNSTABLE, or CLEAN)" — matching the plan's mergeStateStatus transition table; this prevents a deadlock where a fresh PR awaiting required review (BLOCKED) would never transition
- **MERGE-02 cap:** Set at 3 attempts (D-04 principle finalized at 3)
- **Single file committed:** `docker/WORKFLOW.md` is gitignored (operator's local copy); only `docker/WORKFLOW.example.md` is tracked. All plan changes land in the example file per this project's convention

## Deviations from Plan

### Clarification: WORKFLOW.md gitignored — only WORKFLOW.example.md committed

- **Found during:** Task 1 (pre-commit file discovery)
- **Issue:** The plan's `files_modified` lists both `docker/WORKFLOW.md` and `docker/WORKFLOW.example.md`. `docker/WORKFLOW.md` is explicitly gitignored (`.gitignore` line: `/docker/WORKFLOW.md`) and cannot be committed from the worktree.
- **Fix:** All changes applied exclusively to `docker/WORKFLOW.example.md` (the committed template). This is correct per project convention: `docker/WORKFLOW.md` is the operator's customized copy; operators re-apply local customizations on top of the committed template.
- **Impact:** None on requirements — MERGE-01, MERGE-02, MERGE-04, D-06, D-07 are all satisfied in WORKFLOW.example.md. All must_haves and acceptance criteria met.

---

**Total deviations:** 1 (clarification — gitignore behavior, not a bug fix)
**Impact on plan:** No scope creep. Gitignored-file behavior is correct per project convention.

## Issues Encountered

None beyond the gitignore clarification above.

## Threat Flags

No new threat surface introduced. All security-relevant patterns (LINEAR_API_KEY usage, GitHub API calls, force-push) are continuations of existing mechanisms from Phase 3:
- T-04-01 (force-push): accepted per plan threat model (agent-owned branch, --force-with-lease)
- T-04-02 (conflict resolution prompt injection): mitigated by restricting resolution to issue files + post-rebase gate
- T-04-03 (Linear commentCreate): accepted (same mechanism as Phase 3 BLOCK-01, no new credentials)

## Next Phase Readiness

- MERGE-01 and MERGE-02 proactive gates are in place in `docker/WORKFLOW.example.md`
- The shared Mergeability procedure (D-07) is ready to be referenced by any future prompt changes
- MERGE-04 (prompt half) is complete; the spec/doc half (HARNESS-SPEC §7.4 serial-foundation convention) is delivered by Plan 04-02
- Phase 5 (decomposition/context-graph) can build on the serial-foundation convention established here

## Self-Check

- [x] `docker/WORKFLOW.example.md` exists with all required changes
- [x] Commits 7c8eb46, d9f1f26, faaec35 exist in git log
- [x] Task 1 verification: Mergeability procedure + force-with-lease + no "or merge" hedge — PASS
- [x] Task 2 verification: both markers + idempotency probe + BEHIND — PASS
- [x] Task 3 verification: serial-foundation + §7.4 within 2 lines of "In Progress": 1 — PASS

---
*Phase: 04-merge-conflict-handling-merge-queue*
*Completed: 2026-06-14*
