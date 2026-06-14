---
phase: 04-merge-conflict-handling-merge-queue
reviewed: 2026-06-14T00:00:00Z
depth: standard
files_reviewed: 4
files_reviewed_list:
  - docker/WORKFLOW.example.md
  - docs/DEPLOYMENT.md
  - docs/HARNESS-SPEC.md
  - docs/SPEC.md
findings:
  critical: 3
  warning: 6
  info: 3
  total: 12
status: issues_found
---

# Phase 4: Code Review Report

**Reviewed:** 2026-06-14
**Depth:** standard
**Files Reviewed:** 4
**Status:** issues_found

## Summary

This phase adds merge-conflict handling and GitHub merge-queue guidance across four
documents: the operator-facing `WORKFLOW.example.md` agent prompt (the largest
change, +171 lines), and three spec/operator docs (`DEPLOYMENT.md`,
`HARNESS-SPEC.md`, `SPEC.md`). The bulk of the new behavior is embedded shell in
the `WORKFLOW.example.md` `In Progress` prompt: a pre-PR gate (MERGE-01), a
mergeability polling loop (MERGE-02), and a shared "Mergeability procedure."

These are prompt/spec docs, not application source, but the embedded shell is
executed verbatim by an agent and the normative wording is acted on directly, so
correctness of the snippets and consistency of the cross-references are
load-bearing. The review found three blocking correctness defects in the
MERGE-02 loop shell, plus a self-contradicting GitHub branch-protection table in
`DEPLOYMENT.md` that, if followed literally, cannot be configured. Several
dangling decision-ID references (DEC-001, D-03, D-10) are not resolvable from the
published docs.

The cross-references to `HARNESS-SPEC.md` §7.4 / §7.2 all resolve correctly, and
the D-03 "mergeable-not-CLEAN" refinement is internally consistent with the
agent prompt's `BLOCKED`/`UNSTABLE`/`CLEAN` proceed logic.

## Critical Issues

### CR-01: MERGE-02 loop aborts mid-rebase on the exact case it exists to handle

**File:** `docker/WORKFLOW.example.md:531-569`
**Issue:** The mergeability loop runs under `set -e` (line 531). The `DIRTY|BEHIND`
case (lines 546-555) executes a bare `git rebase origin/main` (line 552) with no
conflict-resolution step. `DIRTY` *means* the branch conflicts with `main`, so the
rebase will stop with a non-zero exit on the first conflict. Under `set -e` the
entire script aborts immediately — leaving the workspace in a half-finished
`rebase-in-progress` state, the branch un-pushed, and (critically) the agent
**never reaches** the loop-exhausted comment block (lines 571-589) or the "DONE
for this turn" guard (lines 592-594). The agent will then likely proceed to STEP 8
and attempt the In Review transition on a broken workspace, defeating the gate.

The inline comment on line 550 even says "(fetch, rebase, gate, force-push)" and
the loop header (lines 526-527) says "using the Mergeability procedure," but the
inline body does none of the conflict resolution the procedure documents — it is a
blind rebase that cannot survive a real conflict.

**Fix:** Do not embed a bare rebase under `set -e`. Either guard the rebase and
hand resolution back to the agent narratively, or abort cleanly on failure:

```bash
DIRTY|BEHIND)
  ATTEMPT=$((ATTEMPT + 1))
  if [ "$ATTEMPT" -ge "$MAX_ATTEMPTS" ]; then break; fi
  echo "Conflict detected ($MSS) — resolve via the Mergeability procedure, attempt $ATTEMPT/$MAX_ATTEMPTS."
  git fetch origin
  if ! git rebase origin/main; then
    echo "Rebase hit conflicts — resolve THIS issue's files, then 'git rebase --continue' before re-running the gate. Do not force-push until the gate is green."
    # Hand control back to the agent; do not blindly force-push a conflicted tree.
    break
  fi
  # Re-run the repo-discovered gate here and abort the push if it is red.
  git push --force-with-lease origin "$BRANCH"
  ;;
```

### CR-02: `UNKNOWN` branch never advances the loop counter — infinite loop

**File:** `docker/WORKFLOW.example.md:556-559`
**Issue:** The `UNKNOWN` case sleeps 15s and re-polls but never increments
`ATTEMPT` and never `break`s. The `while [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]`
guard (line 539) only advances inside the `DIRTY|BEHIND` arm. GitHub can return
`mergeStateStatus: UNKNOWN` persistently while it recomputes (and during merge-queue
churn it can stay UNKNOWN for a while). If it does, the loop spins forever at 15s
per iteration, hanging the agent turn until the orchestrator's turn/stall timeout
kills it. There is no bounded retry on `UNKNOWN`.

**Fix:** Bound the `UNKNOWN` re-polls independently (or share the attempt budget):

```bash
UNKNOWN_TRIES=0
# ...
UNKNOWN)
  UNKNOWN_TRIES=$((UNKNOWN_TRIES + 1))
  if [ "$UNKNOWN_TRIES" -ge 5 ]; then
    echo "mergeStateStatus stuck UNKNOWN after 5 polls — staying In Progress, re-evaluating next poll."
    exit 0
  fi
  echo "GitHub still computing mergeStateStatus — sleeping 15s and re-polling ($UNKNOWN_TRIES/5)..."
  sleep 15
  ;;
```

### CR-03: Loop force-pushes without running the gate it claims to run

**File:** `docker/WORKFLOW.example.md:550-554`
**Issue:** The inline `DIRTY|BEHIND` body comment on line 553 says
"# Re-run the repo-discovered gate; abort if red" but there is **no gate command**
between the rebase (line 552) and the `git push --force-with-lease` (line 554).
The branch is force-pushed unconditionally after a rebase, with no test run. This
directly violates the Mergeability procedure's own invariant ("The gate MUST be
green before you push," line 624) and the MERGE-01 pre-PR contract ("The gate MUST
be green before you push," line 474). An agent following the loop literally will
push rebased-but-untested code, which the merge queue may then reject or, worse,
land if checks are misconfigured. This is a data/quality-integrity defect in the
documented contract: the loop advertises a gate it does not run.

**Fix:** Either run the repo-discovered gate and abort the push on failure before
the `git push --force-with-lease`, or remove the misleading comment and explicitly
delegate to the Mergeability procedure rather than inlining a partial copy of it.
Inlining a divergent, weaker copy of the procedure (no resolution, no gate) is the
root cause of CR-01 and CR-03 both — prefer a single authoritative procedure and
call it.

## Warnings

### WR-01: `DEPLOYMENT.md` branch-protection table is self-contradicting / not configurable

**File:** `docs/DEPLOYMENT.md:617-622`
**Issue:** The table instructs enabling **both** "Require linear history: Enabled"
and "Require merge queue: Enabled" on `main`. In GitHub branch protection these
are mutually exclusive — the UI disables "Require linear history" when a merge
queue is required (the merge queue owns history shape via its own merge method).
An operator following this table literally cannot apply both. Additionally
`HARNESS-SPEC.md` §7.4 (the normative source this section says it mirrors) does
**not** mention "Require linear history" at all, so the operator doc adds a
requirement the spec doesn't state.

**Fix:** Drop "Require linear history" from the branch-protection table (the merge
queue with "Rebase and merge" already produces linear history), or move it to a
note clarifying it is implied by the merge method, not a separately-toggled rule.

### WR-02: `DEC-001` referenced as a known decision but undefined in any published doc

**File:** `docs/DEPLOYMENT.md:609`, `docs/HARNESS-SPEC.md:348`
**Issue:** Both the operator guide and the normative spec cite "(DEC-001)" as the
merge-queue decision of record, but no published doc defines DEC-001. A reader of
`docs/` cannot resolve it. (`grep -rn DEC-001 docs/` returns only these two
forward references.) Same class of dangling reference applies to D-03
(HARNESS-SPEC.md:359), D-10 (HARNESS-SPEC.md:373), and "Theme D"
(HARNESS-SPEC.md:379) — these decision/theme IDs are used as if canonical but are
not anchored in any published document.

**Fix:** Either add a one-line glossary / decisions index under `docs/` that
defines DEC-001 / D-03 / D-10 / Theme D, or change the citations to plain
descriptive text (e.g. "(GitHub native merge-queue model)") so the published docs
are self-contained.

### WR-03: MERGE-02 loop reuses `$PR_NUM` from a separate code block — fragile across step boundaries

**File:** `docker/WORKFLOW.example.md:522`, `540`, `572`, `630`
**Issue:** `PR_NUM` is set at the end of step 6's code block (line 522) and then
consumed by step 7's block (lines 540, 572) and the Mergeability procedure
(line 630), each of which is a *separate* fenced ```bash``` block with its own
`set -e`. An agent that runs each block as an independent shell (the natural
reading of "run this block") will hit an unset `$PR_NUM`. With `set -e` but no
`set -u`, `gh pr view ""` runs against an empty number and `jq` yields `null`,
which then falls through the `case` to the `*)` arm (line 564) and "proceeds" —
silently skipping the entire mergeability gate. The gate fails open.

**Fix:** Re-derive `PR_NUM` at the top of each block that uses it (the pattern
already exists at line 522), or add `set -u` and an explicit guard:
`[ -n "$PR_NUM" ] || { echo "PR_NUM unset"; exit 1; }`.

### WR-04: Mergeability procedure references `$PR_NUM` but is documented as the "single source of truth" callable standalone

**File:** `docker/WORKFLOW.example.md:600-640`
**Issue:** The procedure is introduced as "The single source of truth for
rebasing..." (line 602) and is referenced from three call sites (Todo step 1,
In Progress STEP 1, and MERGE-01/MERGE-02). But its final step (lines 630-632)
depends on `$PR_NUM` being already set, which is not established within the
procedure block and is not part of its `set -e` preamble (lines 608-609 only set
`BRANCH`). A caller invoking it from the Todo prompt or from MERGE-01 (before a PR
exists) has no `$PR_NUM` — step 5's `gh pr view "$PR_NUM"` then runs against an
empty value. A "single source of truth" procedure should be self-contained.

**Fix:** Either derive `PR_NUM` inside the procedure
(`PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number')`)
and skip step 5 when empty, or document `$PR_NUM` as a required precondition the
caller must set.

### WR-05: Loop comment says "sleep 10" while code sleeps 15 — inconsistent guidance

**File:** `docker/WORKFLOW.example.md:557-558` vs `635`
**Issue:** The MERGE-02 loop sleeps 15s on `UNKNOWN` (line 558) and its echo says
15s (line 557), but the Mergeability procedure's documented branch table says
"UNKNOWN → GitHub still computing; sleep 10 and re-poll" (line 635). Two
authoritative-sounding spots give different sleep values for the same condition.
An agent reconciling them has no basis to choose.

**Fix:** Pick one value and use it in both places (15s is fine; just update
line 635 to match).

### WR-06: `*)` catch-all "proceeds" on any unexpected `mergeStateStatus` — fails open

**File:** `docker/WORKFLOW.example.md:564-567`, `573-574`
**Issue:** The `*)` arm treats any unrecognized `mergeStateStatus` as
"conflict-free and proceeding" (lines 565-566). Combined with WR-03 (empty
`PR_NUM` → `null` → falls to `*)`), and with the final guard only checking for
literal `DIRTY`/`BEHIND` (line 574), any anomaly (`null`, a future GitHub status
value, an API error returning empty) routes straight to the In Review transition.
The gate is designed to fail closed on conflicts but here fails open on
ambiguity. The "treating as conflict-free" choice is questionable for a defensive
gate.

**Fix:** Make the catch-all conservative — log the unexpected value and stay In
Progress rather than transitioning, or at minimum require an explicit allowlist
(`BLOCKED|UNSTABLE|CLEAN`) to proceed and treat everything else as "re-poll once
then hold."

## Info

### IN-01: "rebase-and-tests" grammar slip in Mergeability procedure rationale

**File:** `docker/WORKFLOW.example.md:604-605`
**Issue:** "a linear history is what the GitHub native merge queue rebase-and-tests
before merging" — garbled phrasing ("a linear history is what ... rebase-and-tests").
Reads as a copy/edit artifact.
**Fix:** Reword, e.g. "force-push is safe here because `sinfonia/<id>` is an
agent-owned branch, and the GitHub native merge queue rebases-and-tests each PR
against `main` before merging."

### IN-02: Duplicated 70-line BLOCK-01 guardrail across Todo and In Progress prompts

**File:** `docker/WORKFLOW.example.md:111-168` and `336-393`
**Issue:** The BLOCK-01 blocker-merged guardrail is copy-pasted near-verbatim in
both prompt bodies (this predates phase 4 but is adjacent to the changed region).
The new MERGE-01/MERGE-02 comment-posting idempotency blocks (marker-check +
commentCreate) repeat the same ~15-line pattern three more times. Divergence risk
is high — a fix to one copy (e.g. the CR-series fixes above) will not propagate.
**Fix:** Not directly fixable in a flat prompt file, but worth a note: consider a
documented shared snippet or accept the duplication explicitly with a comment so
future editors know to update all copies.

### IN-03: HARNESS-SPEC conformance checklist item bundles four distinct requirements into one checkbox

**File:** `docs/HARNESS-SPEC.md:433-436`
**Issue:** The new checklist item (§7.4) folds "merge queue configured" +
"post-merge gate on push" + "DIRTY/BEHIND-only loop" + "BLOCKED/UNSTABLE-ready
gate" into a single `[ ]`. A repo could satisfy two and miss two while ticking the
box. Other checklist items are more granular.
**Fix:** Split into two checkboxes (merge-queue/post-merge config vs. agent-prompt
mergeability behavior) for verifiability.

---

_Reviewed: 2026-06-14_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
