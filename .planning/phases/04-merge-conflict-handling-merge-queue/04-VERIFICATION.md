---
phase: 04-merge-conflict-handling-merge-queue
verified: 2026-06-14T21:00:00Z
status: passed
score: 4/4 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 2/4
  gaps_closed:
    - "SC-1: MERGE-01/MERGE-02 green-gate-before-push invariant — loop body now guards rebase (CR-01), runs GATE_OK gate before force-push (CR-03)"
    - "SC-2: bounded mergeability loop — UNKNOWN arm now increments UNKNOWN_TRIES with cap-5 exit (CR-02); catch-all is conservative with independent WEIRD_TRIES counter (WR-06); PR_NUM self-derived at top of step-7 block with empty guard (WR-03/WR-04); sleep unified at 15s (WR-05); rationale prose reworded (IN-01)"
    - "DEPLOYMENT.md mutually-exclusive linear-history+merge-queue row removed (WR-01)"
    - "Dangling internal IDs DEC-001/D-03/D-10/Theme D removed from published docs/ (WR-02 in DEPLOYMENT.md and HARNESS-SPEC.md)"
    - "HARNESS-SPEC §9 bundled checklist item split into two independently-verifiable bullets (IN-03)"
    - "before_run hook heals half-finished rebase with git rebase --abort before git switch (code-review WR-01/third-pass finding)"
  gaps_remaining: []
  regressions: []
---

# Phase 4: Merge-Conflict Handling & Merge Queue — Verification Report (Re-verification)

**Phase Goal:** Agent PRs land cleanly under the ratified GitHub native merge queue + serial-foundation model — branches are rebased on current `main` with a green harness before PR creation, mergeability is enforced before `In Review`, and a post-merge gate runs on `main`.
**Verified:** 2026-06-14T21:00:00Z
**Status:** passed
**Re-verification:** Yes — after gap closure (plans 04-03 and 04-04 + orchestrator fix commits 4cbd83d, 3b2344b)

---

## Re-verification Context

The initial verification (wave 1, plans 04-01 and 04-02) recorded `gaps_found` at `2/4` because the MERGE-02 mergeability loop shell block in `docker/WORKFLOW.example.md` had three blocker-grade correctness defects (CR-01, CR-02, CR-03) and associated wiring defects. Gap-closure plans 04-03 and 04-04 were executed, followed by two orchestrator-applied fix commits (`4cbd83d` closing CR-01/CR-02/WR-A/WR-B/WR-C and `3b2344b` adding the `before_run` rebase-abort heal). The code review (fourth pass, commit `7cb7e2f`) declared status: clean with 0 findings. This re-verification starts from the gap list in the prior VERIFICATION.md frontmatter and verifies each against the current codebase.

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| SC-1 | PRs are opened only from a branch rebased on current `origin/main` with a green harness run; gate-red path posts idempotent Linear comment and exits without pushing or opening a PR | VERIFIED | `docker/WORKFLOW.example.md` step 4 (lines 476-507): narrative instructs "follow the Mergeability procedure before pushing"; hard-stop bash block uses `sinfonia-bot: pre-pr-gate-red` marker with `grep -qF "$MARKER"` idempotency probe and `exit 0` without push. The Mergeability procedure (line 644+) guards the rebase with `if ! git rebase origin/main`, sets `GATE_OK=` (safe-by-default), and skips `git push --force-with-lease` unless `GATE_OK=1`. The MERGE-02 loop DIRTY/BEHIND arm also now enforces the same pattern (lines 563-584). |
| SC-2 | An issue reaches `In Review` only when the PR is mergeable w.r.t. `main`, after a bounded rebase/resolve/re-run/force-push loop; DIRTY/BEHIND loops; UNKNOWN re-polls bounded; BLOCKED/UNSTABLE/CLEAN proceed; exhaustion posts idempotent comment and stays In Progress | VERIFIED | Step-7 mergeability-loop block (lines 534-633): `MAX_ATTEMPTS=3`; `UNKNOWN_TRIES=0` and `WEIRD_TRIES=0` initialized. DIRTY/BEHIND arm: `ATTEMPT` incremented, guarded rebase (`if ! git rebase origin/main`), gate (`GATE_OK=` pattern, skip push on non-green), then `git push --force-with-lease`. UNKNOWN arm: `UNKNOWN_TRIES` incremented, exits at 5. Catch-all `*)`: uses `WEIRD_TRIES` (independent counter), exits at 5 — does NOT treat unexpected status as conflict-free. BLOCKED/UNSTABLE/CLEAN: `break` to proceed. Exhaustion block: `sinfonia-bot: mergeability-loop-exhausted` with same idempotency probe, `exit 0`. PR_NUM self-derived at block top (`gh pr list --head "$BRANCH"`) with empty-value guard. |
| SC-3 | HARNESS-SPEC §7.4 names the merge-queue rebase-and-test requirement and a post-merge harness gate on `main`; §9 checklist updated (two bullets); operator branch-protection/merge-queue setup note exists in DEPLOYMENT.md with no mutually-exclusive row and no dangling internal IDs | VERIFIED | §7.4 (lines 341-382): four normative paragraphs present — Merge queue (rebase-and-test), Post-merge harness gate, Mergeable-not-CLEAN gate refinement (with explicit CLEAN-deadlock tension note), Serial-foundation convention. No `DEC-001`/`D-03`/`D-10`/`Theme D` IDs in §7.4. §9 (lines 433-437): two `- [ ]` bullets each ending `(§7.4)` — Bullet A (merge-queue/post-merge config), Bullet B (agent-prompt mergeability behavior). `docs/DEPLOYMENT.md` `## Merge queue & branch protection` section: three-row branch-protection table (no "Require linear history" row; blockquote note attributes linear history to the rebase merge method), merge-queue-settings table, post-merge gate subsection, serial-foundation subsection — all cross-referencing §7.4; no `DEC-001`. |
| SC-4 | `In Progress` concurrency stays at 1; serial-foundation convention is documented in the concurrency config and at milestone level | VERIFIED | `docker/WORKFLOW.example.md` lines 71-73: comment `# serial-foundation convention (docs/HARNESS-SPEC.md §7.4): one foundational / story at a time — each lands on \`main\` before the next begins.` immediately above `"In Progress": 1`. `docs/HARNESS-SPEC.md` §7.4 defines the convention normatively. `docs/SPEC.md` §8.2 lines 736-738: `Note (v0.4 Phase 4): Serial-foundation concurrency — max_concurrent_agents_by_state: "In Progress": 1 enforces foundational stories to run and land on \`main\` serially before the next begins; see docs/HARNESS-SPEC.md §7.4.` |

**Score:** 4/4 truths verified

---

### Deferred Items

None.

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `docker/WORKFLOW.example.md` | In Progress prompt: Mergeability procedure (single source of truth), MERGE-01 pre-PR gate, MERGE-02 mergeability loop (correctly guarded), concurrency comment | VERIFIED | All structures present and correct. Guarded rebase at lines 563, 667. GATE_OK safe-by-default at lines 578-583 (loop) and 681-686 (procedure). UNKNOWN_TRIES counter at lines 542, 587-593. WEIRD_TRIES independent catch-all at lines 543, 604-610. PR_NUM self-derive at lines 545-547. `before_run` heals half-finished rebase at lines 59-62. |
| `docs/HARNESS-SPEC.md` | §7.4 amended (merge queue + post-merge gate + D-03 refinement + serial-foundation); §9 two-bullet checklist | VERIFIED | §7.4 text confirmed with all four paragraphs; no dangling IDs; §9 has exactly 2 bullets referencing `(§7.4)`. |
| `docs/DEPLOYMENT.md` | `## Merge queue & branch protection` operator section: three-row table (no linear-history row), merge-queue settings, post-merge gate, serial-foundation note; no DEC-001 | VERIFIED | Section present before `## Upgrading`. Branch-protection table has 3 rows (pull-request, status-checks, merge-queue). Blockquote note explains linear history from rebase method. No "Require linear history" table row. No DEC-001. |
| `docs/SPEC.md` | §8.2 `Note (v0.4 Phase 4)` cross-referencing HARNESS-SPEC §7.4 | VERIFIED | Lines 736-738 confirmed. |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| In Progress STEP 1 conflict bullet | Mergeability procedure section | "follow the **Mergeability procedure** below" | VERIFIED | Line 451: "Follow the **Mergeability procedure** below" |
| MERGE-01 step 4 (pre-PR gate) | Mergeability procedure section | "follow the Mergeability procedure before pushing" | VERIFIED | Line 476: instructs agent to follow the Mergeability procedure before pushing |
| MERGE-02 loop DIRTY/BEHIND arm | gate-before-push pattern | GATE_OK safe-by-default between guarded rebase and force-push | VERIFIED | Lines 563-584: `if ! git rebase origin/main` guard; `GATE_OK=` (empty/safe); gate-then-conditional-push; `git push --force-with-lease` only when `GATE_OK=1` |
| MERGE-02 UNKNOWN arm | bounded counter | `UNKNOWN_TRIES` incremented with cap-5 exit | VERIFIED | Lines 587-593: `UNKNOWN_TRIES=$((UNKNOWN_TRIES + 1))`, `[ "$UNKNOWN_TRIES" -ge 5 ]` bounded exit |
| MERGE-02 catch-all arm | conservative hold | `WEIRD_TRIES` counter, no conflict-free assumption | VERIFIED | Lines 599-611: comment explicitly states "NOT treated as conflict-free"; `WEIRD_TRIES` counter with cap-5 exit; no "treating as conflict-free and proceeding" |
| Step-7 block | PR_NUM | Self-derived via `gh pr list --head "$BRANCH"` at block top | VERIFIED | Lines 545-547: `PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number')` with `[ -n "$PR_NUM" ]` guard |
| before_run hook | workspace heal | `git rebase --abort 2>/dev/null \|\| true` before `git switch` | VERIFIED | Lines 59-62: rebase abort before fetch/switch; comment explains workspace-wedge prevention |
| HARNESS-SPEC §9 two bullets | HARNESS-SPEC §7.4 | `(§7.4)` back-reference on each bullet | VERIFIED | Lines 433-437: two distinct `- [ ]` bullets, each ending `(§7.4)` |
| DEPLOYMENT.md merge-queue section | HARNESS-SPEC §7.4 | Cross-reference links in post-merge and serial-foundation subsections | VERIFIED | Two `§7.4` hyperlinks confirmed |
| SPEC.md §8.2 Note (v0.4 Phase 4) | HARNESS-SPEC §7.4 | `see docs/HARNESS-SPEC.md §7.4` | VERIFIED | Line 738 confirmed |
| Concurrency config comment | HARNESS-SPEC §7.4 | `docs/HARNESS-SPEC.md §7.4` citation adjacent to `"In Progress": 1` | VERIFIED | Lines 71-73 confirmed |

---

### Data-Flow Trace (Level 4)

Not applicable. This phase delivers agent-prompt documents and specification amendments — not application source code with data rendering. The "data" is agent instructions read verbatim from `WORKFLOW.example.md`, verified structurally above.

---

### Behavioral Spot-Checks

| Behavior | Check | Result | Status |
|----------|-------|--------|--------|
| Guarded rebase in DIRTY/BEHIND arm | `grep -c "if ! git rebase origin/main"` | 2 occurrences (loop body + Mergeability procedure) | PASS |
| GATE_OK safe-by-default (gate before push) | `grep -n "GATE_OK="` | Lines 578, 681 — both with conditional push gate | PASS |
| UNKNOWN_TRIES counter (bounded UNKNOWN) | `grep -c "UNKNOWN_TRIES"` | 4 occurrences (init, increment, cap check, message) | PASS |
| WEIRD_TRIES independent counter (conservative catch-all) | `grep -c "WEIRD_TRIES"` | 5 occurrences; "treating as conflict-free" string absent | PASS |
| PR_NUM self-derived with empty guard | `grep "PR_NUM unset"` | Line 547 confirmed with `exit 0` guard | PASS |
| before_run heals workspace | `grep "git rebase --abort"` | Line 60: `git rebase --abort 2>/dev/null \|\| true` in before_run | PASS |
| Procedure sleep unified at 15s | `grep "sleep 10" in procedure` | None found; `sleep 15` at line in procedure table | PASS |
| Garbled rationale absent | `grep "a linear history is what"` | None found | PASS |
| pre-pr-gate-red marker + idempotency probe | `grep -c "pre-pr-gate-red"` + `grep -qF` | 1 occurrence of marker; 4 `grep -qF` probes in file | PASS |
| mergeability-loop-exhausted marker | `grep -c "mergeability-loop-exhausted"` | 1 occurrence with idempotency probe | PASS |
| §7.4 no DEC-001/D-03/D-10/Theme D | `grep` each in §7.4 block | All absent from §7.4 | PASS |
| §9 has 2 bullets with (§7.4) | `grep -c "(§7.4)"` in §9 block | 2 confirmed | PASS |
| DEPLOYMENT.md no "Require linear history" table row | `grep "^|" \| grep "linear history"` | None in table rows | PASS |
| DEPLOYMENT.md no DEC-001 | `grep "DEC-001"` in section | None found | PASS |
| Debt markers (TBD/FIXME/XXX) | `grep` all modified files | None found | PASS |

---

### Probe Execution

No conventional `scripts/*/tests/probe-*.sh` files are declared or expected for this documentation-only phase. Step 7c SKIPPED (no probes).

---

### Requirements Coverage

| Requirement | Source Plan(s) | Description | Status | Evidence |
|-------------|----------------|-------------|--------|----------|
| MERGE-01 | 04-01-PLAN.md | Pre-PR rebase: fetch, rebase/resolve, green gate, then push; hard stop on red with idempotent comment | SATISFIED | Step 4 in In Progress Implementation-continuation instructs Mergeability procedure before push; hard-stop block with `sinfonia-bot: pre-pr-gate-red` marker, idempotency probe, and `exit 0` without push confirmed at lines 476-507 |
| MERGE-02 | 04-01-PLAN.md, 04-03-PLAN.md | Bounded mergeability loop after push; DIRTY/BEHIND loops with guarded rebase + gate + force-push; UNKNOWN bounded; exhaustion stays In Progress | SATISFIED | Step-7 bash block (lines 534-633): all defects from initial verification closed — guarded rebase (CR-01), gate-before-push (CR-03), bounded UNKNOWN (CR-02), conservative catch-all (WR-06), self-derived PR_NUM (WR-03/WR-04) |
| MERGE-03 | 04-02-PLAN.md, 04-04-PLAN.md | HARNESS-SPEC §7.4: post-merge gate + merge-queue rebase-and-test; §9 checklist updated; operator setup note; no dangling IDs | SATISFIED | §7.4 four normative paragraphs; §9 two `(§7.4)` bullets (split from original bundled item per IN-03); DEPLOYMENT.md operator section with corrected branch-protection table (no linear-history row, WR-01) and no DEC-001 (WR-02) |
| MERGE-04 | 04-01-PLAN.md, 04-02-PLAN.md | Comment in concurrency config; serial-foundation convention written at milestone level | SATISFIED | Concurrency comment at lines 71-73 citing §7.4; §7.4 defines convention; SPEC.md §8.2 cross-references it |

All four requirements for Phase 4 are SATISFIED.

---

### Anti-Patterns Found

| File | Location | Pattern | Severity | Impact |
|------|----------|---------|----------|--------|
| — | — | No anti-patterns found in any modified file | — | — |

No `TBD`, `FIXME`, or `XXX` debt markers in any file modified by this phase. No stub patterns, no placeholder returns, no hardcoded empties outside of correct initial-state variable declarations (`GATE_OK=` is correct — it is the safe-by-default pattern, not a stub).

---

### Human Verification Required

None. All correctness properties of this phase are verifiable from static code inspection. The code review (fourth pass) confirms 0 findings. No visual, real-time, or external-service behavior requires human testing.

---

### Gaps Summary

No gaps remain. All items from the initial `gaps_found` verdict are closed:

- **CR-01** (bare rebase under `set -e`): closed — DIRTY/BEHIND arm now uses `if ! git rebase origin/main; then ... exit 0; fi` guard in both the loop body and the Mergeability procedure.
- **CR-02** (unbounded UNKNOWN): closed — `UNKNOWN_TRIES` counter with cap-5 bounded exit.
- **CR-03** (no gate before force-push in loop): closed — `GATE_OK=` safe-by-default pattern; push skipped unless `GATE_OK=1`.
- **WR-01** (mutually exclusive linear-history+merge-queue table row): closed — "Require linear history" row removed; blockquote note attributes it to rebase merge method.
- **WR-02** (dangling internal IDs in published docs): closed — `DEC-001`/`D-03`/`D-10`/`Theme D` removed from both DEPLOYMENT.md and HARNESS-SPEC §7.4.
- **WR-03/WR-04** (PR_NUM not self-derived in step-7 block): closed — `PR_NUM=$(gh pr list --head "$BRANCH" ...)` at block top with empty guard.
- **WR-05** (inconsistent UNKNOWN sleep value): closed — procedure table and loop body both use `sleep 15`.
- **WR-06** (catch-all proceeds on unexpected status): closed — `*)` arm uses `WEIRD_TRIES` counter, exits conservatively, comment reads "NOT treated as conflict-free".
- **IN-01** (garbled Mergeability procedure rationale): closed — clean prose "agent-owned branch, and the GitHub native merge queue rebases-and-tests each PR against `main` before merging".
- **IN-03** (bundled §9 checklist item): closed — split into two independently-verifiable `- [ ]` bullets.
- **Code-review third-pass WR-01** (workspace wedge on conflict-path exit): closed — `before_run` hook runs `git rebase --abort 2>/dev/null || true` before `git switch`.

---

_Verified: 2026-06-14T21:00:00Z_
_Verifier: Claude (gsd-verifier)_
_Re-verification: Yes — initial gaps_found (2/4) → passed (4/4)_
