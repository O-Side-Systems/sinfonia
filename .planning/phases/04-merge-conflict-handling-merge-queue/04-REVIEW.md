---
phase: 04-merge-conflict-handling-merge-queue
reviewed: 2026-06-14T13:00:00Z
depth: standard
files_reviewed: 3
files_reviewed_list:
  - docker/WORKFLOW.example.md
  - docs/DEPLOYMENT.md
  - docs/HARNESS-SPEC.md
findings:
  critical: 2
  warning: 3
  info: 1
  total: 6
status: issues_found
---

# Phase 4: Code Review Report (Re-review after 04-03 / 04-04)

**Reviewed:** 2026-06-14
**Depth:** standard
**Files Reviewed:** 3
**Status:** issues_found

## Summary

This is a re-review of the three source files (`docker/WORKFLOW.example.md`,
`docs/DEPLOYMENT.md`, `docs/HARNESS-SPEC.md`) after gap-closure plans 04-03 and
04-04, which were written to close CR-01..CR-03, WR-01..WR-06, IN-01, and IN-03
from the prior review.

**Resolved by the gap-closure work (verified):**

- **CR-01 (loop block):** The `DIRTY|BEHIND` arm now guards the rebase —
  `if ! git rebase origin/main; then ... break; fi` (lines 558-564). Under
  `set -e` a conflicting rebase no longer aborts the whole script.
- **CR-02:** The `UNKNOWN` arm now increments `UNKNOWN_TRIES` and `exit 0`s at
  ≥5 (lines 587-593). No infinite loop.
- **CR-03 (loop block):** A repo-discovered gate now runs *between* the
  successful rebase and `git push --force-with-lease` (lines 570-584); the push
  at line 584 is reached only when a gate command was found AND it exited 0.
- **WR-01:** `DEPLOYMENT.md` no longer requires both "Require linear history"
  and "Require merge queue"; the linear-history toggle is now a clarifying note
  (lines 623-626).
- **WR-02 (docs):** `DEC-001`, `D-03`, `D-10`, and "Theme D" are gone from the
  published docs (`grep` over `docs/` returns nothing). See WR-A below — the
  same class of dangling reference (`D-01`) survives inside
  `WORKFLOW.example.md`.
- **WR-03 (step-7 block):** `PR_NUM` is now self-derived at the top of the
  MERGE-02 block with a guard (lines 541-542).
- **WR-05:** The Mergeability-procedure branch table now says "sleep 15" to
  match the loop (line 677).
- **WR-06:** The `*)` catch-all is now conservative — it does NOT proceed; it
  holds In Progress and re-polls up to 5 times, then `exit 0` (lines 599-609).
- **IN-01:** The garbled "rebase-and-tests" phrasing is reworded cleanly
  (lines 644-647).
- **IN-03:** The §9 conformance checklist item is split into two checkboxes —
  merge-queue/post-merge config (line 433-434) and agent-prompt mergeability
  behavior (lines 435-437).

**NOT resolved / newly introduced — two BLOCKERs remain:**

The fixes were applied to the MERGE-02 *loop block* but the prior review's three
critical defects also live in the standalone **Mergeability procedure** (lines
649-682) — the document's self-described "single source of truth" referenced
from three other call sites. That procedure still has the bare rebase under
`set -e`, still pushes without running a gate, and still references an unset
`$PR_NUM`. Separately, the gate-discovery mechanism added to fix CR-03 (line
570) is itself incorrect and unsafe: on the most common GitHub-workflow form it
mis-extracts and `eval`s a syntactically broken or arbitrary command.

## Critical Issues

### CR-01: Gate auto-discovery `eval`s a mis-parsed / arbitrary command — false GATE-RED on the common case, and an arbitrary-exec surface

**File:** `docker/WORKFLOW.example.md:570-584`
**Issue:** The fix for prior-review CR-03 added an automatic gate discovery:

```bash
if ! GATE_CMD=$(grep -rE '^\s*(run|cmd):' .github/workflows/*.yml 2>/dev/null | grep -i 'test\|lint\|ci' | head -1 | sed 's/.*run: //'); then
  GATE_CMD=""
fi
if [ -n "$GATE_CMD" ]; then
  if ! eval "$GATE_CMD"; then ...
```

Three concrete defects, all reproduced locally:

1. **Mis-parse on `run: |` (the common multi-line form).** A normal workflow
   step `run: |` followed by indented commands greps the `run: |` line; `sed
   's/.*run: //'` yields `GATE_CMD="|"`. Then `[ -n "$GATE_CMD" ]` is true and
   `eval "|"` is a bash syntax error → `if ! eval` fires → prints `GATE-RED` and
   `break`. A repo with a perfectly valid CI gate is reported as gate-failed and
   the branch is **never pushed**. This is a false negative that blocks merges
   for the most idiomatic workflow shape.
2. **Wrong line selected.** `grep -i 'test\|lint\|ci'` matches *any* line
   containing those substrings — a job *named* `ci`, an `echo "deploy to ci"`,
   a coverage-upload step, a `release:` job. `head -1` then takes whichever
   comes first in file order, not the actual harness gate.
3. **Arbitrary command execution via `eval`.** Whatever string the scrape
   produces is `eval`'d verbatim against the agent's checkout. A `run:` line
   such as `echo x; rm -rf …` (or any matched non-test step) is executed.
   `eval` of text scraped out of a YAML file is an injection/arbitrary-exec
   surface and is not a reliable way to recover a runnable command.

The `if ! GATE_CMD=$(...)` guard is also dead: the exit status of the
assignment is the exit status of the pipeline's last stage (`sed`), which is
0 essentially always, so the `GATE_CMD=""` branch is never taken; emptiness is
only handled by the later `[ -n ... ]` test.

**Fix:** Do not auto-`eval` a grepped YAML fragment. Match the standalone
Mergeability procedure's safer approach and hand gate execution to the agent
narratively, with a hard "do not push until the gate is green" guard:

```bash
# Re-run the repo-discovered gate (the same gate the repo's CI runs).
# Discover it from .github/workflows/, README, or the harness's documented
# command — do NOT auto-eval a scraped YAML line (run: | blocks and named
# jobs make that both wrong and unsafe). Run the gate yourself, e.g.:
#   ./scripts/ci.sh   |   npm test   |   cargo test
echo "Run the repo's gate now. Only continue to the force-push below if it is green."
# (agent runs the gate here; if red, STOP — do not force-push, stay In Progress)
git push --force-with-lease origin "$BRANCH"
```

### CR-02: The "single source of truth" Mergeability procedure still has all three original critical defects

**File:** `docker/WORKFLOW.example.md:649-682`
**Issue:** The gap-closure work fixed the inline MERGE-02 *loop* but left the
canonical **Mergeability procedure** — referenced from Todo STEP 1 (line 234),
In Progress STEP 1 (line 447), and MERGE-01 (line 472) — carrying the same
defects the prior review flagged:

- **CR-01 (rebase under `set -e`)** — line 650 sets `set -e`; line 659 runs a
  bare `git rebase origin/main` with no guard. On a real conflict the rebase
  exits non-zero and `set -e` aborts the whole block mid-rebase, leaving a
  half-applied rebase and an un-pushed branch. The comment on line 660 ("If
  conflicts arise, resolve them, then `git rebase --continue`") is a comment
  *after* the failing line — it is never reached as executable flow.
- **CR-03 (push without gate)** — the gate is only prose (lines 662-666); the
  `git push --force-with-lease` on line 669 runs unconditionally right after the
  rebase, with nothing enforcing "gate green before push." The procedure's own
  preamble claims force-push is safe because the merge queue rebases-and-tests,
  but the procedure itself pushes untested.
- **WR-04 (unset `$PR_NUM`)** — line 672 runs `gh pr view "$PR_NUM"` but the
  block only sets `BRANCH` (line 651). Callers from Todo STEP 1 (no PR may
  exist) or MERGE-01 (before the PR is created in step 6) have no `PR_NUM`, so
  `gh pr view ""` runs and `MSS` becomes `null`/garbage. With `set -e` and no
  `set -u`, this does not fail loudly.

Because this is the document's declared single source of truth and is the
procedure the conflict-resolution entry points (`mergeable == "CONFLICTING"`,
STEP 1 "Merge conflicts") send the agent to, the originally-reported critical
behavior is still reachable — the loop-block fix did not cover the path most
conflict cases actually take.

**Fix:** Apply the same three fixes already made in the loop block to this
procedure: guard the rebase (`if ! git rebase origin/main; then … break/stop;
fi`), self-derive `PR_NUM`
(`PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number')`)
and skip step 5 when it is empty, and gate the push (run the gate; do not
`git push --force-with-lease` unless it is green — see CR-01's safer pattern).
Better: make the loop block *call* this single procedure rather than inlining a
diverging copy, so a future fix lands in one place.

## Warnings

### WR-A: `D-01` decision ID still dangles inside `WORKFLOW.example.md`

**File:** `docker/WORKFLOW.example.md:567`, `581`
**Issue:** The docs cleanup removed `DEC-001` / `D-03` / `D-10` / "Theme D" from
`docs/`, but two references to `(D-01)` survive in the WORKFLOW prompt
("follow the Mergeability procedure (D-01)" and "the harness's documented
command (D-01)"). `grep -rn 'D-01' docs/ docker/` finds only these two
forward references; `D-01` is defined nowhere. This is the same
dangling-reference class as the prior WR-02, just in the file that was not
swept. An operator copying this template cannot resolve `D-01`.

**Fix:** Drop the `(D-01)` parentheticals or replace with self-contained
descriptive text (e.g. "(see the Mergeability procedure below)").

### WR-B: Final-guard "exhausted" comment only fires on `DIRTY`/`BEHIND`, but the loop can also exit via a red gate

**File:** `docker/WORKFLOW.example.md:613-636`
**Issue:** When the gate fails after rebase (line 576 `break`) or cannot be
auto-discovered (line 582 `break`), control falls out of the `while` to the
post-loop check at lines 613-631. That check only posts the loop-exhausted
marker / `exit 0` when `MSS_FINAL` is `DIRTY` or `BEHIND`. After a successful
rebase the branch is typically no longer `DIRTY`/`BEHIND` even though the gate
was red and nothing was pushed — so `MSS_FINAL` is something else, the guard is
skipped, and flow continues past line 632 to step 8 (the In Review transition).
The "GATE-RED … Staying In Progress" echo was printed, but there is no
`exit 0` enforcing it, so an agent reading the block top-to-bottom proceeds to
transition anyway. The gate fails open on the gate-red path.

**Fix:** On the gate-red / gate-unknown `break` paths, `exit 0` (do not just
`break`), or broaden the post-loop guard to also halt when the last action was a
red/undiscovered gate (e.g. set a `GATE_OK=0` flag and check it before the
In Review transition).

### WR-C: Loop counter `UNKNOWN_TRIES` is shared between the `UNKNOWN` and `*)` arms, capping legitimate UNKNOWN polls early

**File:** `docker/WORKFLOW.example.md:587-593`, `602-608`
**Issue:** Both the `UNKNOWN` arm and the conservative `*)` catch-all increment
and test the *same* `UNKNOWN_TRIES` counter. The fix is bounded (good — no
infinite loop), but the shared counter means a run that sees a few genuine
`UNKNOWN` polls and then one anomalous status will hit the cap sooner than the
"5 polls" each message advertises, and vice versa. The two conditions are
semantically different (GitHub-still-computing vs. unexpected-value) and the
per-arm messages each claim their own `/5` budget, which is misleading when
they share one counter.

**Fix:** Use two separate counters (`UNKNOWN_TRIES` and `WEIRD_TRIES`) so each
arm's `($n/5)` message is accurate and the budgets are independent.

## Info

### IN-A: Loop-block gate comment and the procedure's gate prose have drifted

**File:** `docker/WORKFLOW.example.md:565-569` vs `662-666`
**Issue:** The loop block now *attempts* automatic gate discovery (lines
570-584), while the standalone Mergeability procedure (lines 662-666) leaves
the gate as agent-run prose. After CR-01/CR-02 are addressed these two should
describe one mechanism; right now a reader comparing them sees two different
gate stories ("we auto-discover and eval it" vs. "you run it"). Consolidating
on a single procedure (per CR-02's fix) removes the drift.
**Fix:** Converge both on the same gate guidance once CR-01/CR-02 are resolved.

---

_Reviewed: 2026-06-14_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
