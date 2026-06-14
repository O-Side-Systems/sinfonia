---
phase: 04-merge-conflict-handling-merge-queue
verified: 2026-06-14T00:00:00Z
status: gaps_found
score: 3/4 must-haves verified
overrides_applied: 0
gaps:
  - truth: "PRs are opened only from a branch rebased on current origin/main with a green harness run (SC-1) AND an issue reaches In Review only when the PR is mergeable w.r.t. main after a bounded rebase/resolve/re-run/force-push loop (SC-2)"
    status: partial
    reason: "The MERGE-02 loop shell block in WORKFLOW.example.md lines 530-590 contains three blocker-grade correctness defects confirmed by the code review (CR-01, CR-02, CR-03): (1) bare 'git rebase origin/main' under 'set -e' aborts the script on the first conflict — the exact case the loop exists to handle — leaving the workspace in a rebase-in-progress state and letting the agent bypass the exhaustion guard and attempt the In Review transition on a broken workspace (CR-01); (2) the UNKNOWN case never increments ATTEMPT and has no bounded retry, creating a potential infinite loop (CR-02); (3) the DIRTY/BEHIND arm carries a comment saying 'Re-run the repo-discovered gate; abort if red' but executes no gate command before git push --force-with-lease, meaning the loop force-pushes untested code (CR-03). These defects mean SC-1 (green harness before push) and SC-2 (bounded rebase/resolve/re-run/force-push) are not reliably satisfied by the documented shell. Additionally WR-03 (PR_NUM potentially unset across separate bash blocks) and WR-06 (catch-all proceeds on unexpected/null mergeStateStatus) are secondary wiring gaps. The pre-PR gate narrative text (MERGE-01 step 4) is correctly worded but the loop body that is supposed to implement the same invariant for MERGE-02 contradicts it."
    artifacts:
      - path: "docker/WORKFLOW.example.md"
        issue: "Lines 546-555: DIRTY/BEHIND arm runs bare git rebase under set -e (aborts on conflict), then pushes without running the gate. Lines 556-559: UNKNOWN never increments attempt counter — no bounded retry. Line 565: catch-all treats unexpected/null mergeStateStatus as conflict-free."
    missing:
      - "DIRTY/BEHIND arm must guard the rebase (if ! git rebase; then handle conflict; fi) so set -e does not abort the script mid-rebase"
      - "Gate command must execute between rebase success and git push --force-with-lease in the DIRTY/BEHIND arm"
      - "UNKNOWN case must increment a separate counter (or share ATTEMPT budget) and exit/break when exhausted"
      - "Catch-all arm should hold In Progress rather than proceeding on unexpected/null mergeStateStatus"
      - "PR_NUM should be re-derived at the top of step 7's bash block (not inherited from a separate block)"
---

# Phase 4: Merge-Conflict Handling & Merge Queue — Verification Report

**Phase Goal:** Agent PRs land cleanly under the ratified GitHub native merge queue + serial-foundation model — branches are rebased on current `main` with a green harness before PR creation, mergeability is enforced before `In Review`, and a post-merge gate runs on `main`.
**Verified:** 2026-06-14
**Status:** gaps_found
**Re-verification:** No — initial verification

---

## Step 0: Previous Verification

No previous VERIFICATION.md found. Initial verification mode.

---

## Goal Achievement

### Observable Truths

| #  | Truth (from ROADMAP.md Success Criteria)                                                                                                                                                                           | Status     | Evidence                                                                                                                                              |
|----|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------|-------------------------------------------------------------------------------------------------------------------------------------------------------|
| SC-1 | PRs are opened only from a branch rebased on current `origin/main` with a green harness run                                                                                                                       | PARTIAL    | Step 4 narrative text correctly instructs "follow the Mergeability procedure before pushing" and the hard-stop bash block (lines 478-503) is correctly wired with idempotent comment + exit 0 without push. However the MERGE-02 loop (CR-03) force-pushes without running the gate, violating the invariant in the loop path |
| SC-2 | An issue reaches `In Review` only when the PR is mergeable w.r.t. `main`, after a bounded rebase/resolve/re-run/force-push loop on non-mergeable (D-03 refinement: DIRTY/BEHIND loop; BLOCKED/UNSTABLE/CLEAN proceed; UNKNOWN re-polls) | PARTIAL    | The loop structure exists, the BLOCKED/UNSTABLE/CLEAN proceed arm is correct, the exhaustion stop block is correct. But the DIRTY/BEHIND arm abort under set -e (CR-01), missing gate before push (CR-03), and unbounded UNKNOWN (CR-02) mean the contract is not reliably fulfilled |
| SC-3 | HARNESS-SPEC §7.4 names the merge-queue rebase-and-test requirement and a post-merge harness gate on `main`; §9 checklist is updated and an operator branch-protection/merge-queue setup note exists                | VERIFIED   | §7.4 text confirmed: merge queue paragraph, post-merge gate paragraph, D-03 mergeable-not-CLEAN refinement with explicit CLEAN-deadlock note, serial-foundation convention. §9 gains one `- [ ]` bullet with `(§7.4)` back-reference (lines 433-436). DEPLOYMENT.md `## Merge queue & branch protection` section present before `## Upgrading` (line 606 vs 656) covering branch protection, merge queue settings, post-merge gate, serial-foundation cross-reference to §7.4 |
| SC-4 | `In Progress` concurrency stays at 1 and the serial-foundation / leaf-fan-out convention is documented in the concurrency config and at milestone level                                                           | VERIFIED   | WORKFLOW.example.md line 67-69: comment `serial-foundation convention (docs/HARNESS-SPEC.md §7.4)` immediately above `"In Progress": 1`. HARNESS-SPEC §7.4 defines the convention normatively. SPEC.md §8.2 line 736-738: `Note (v0.4 Phase 4)` cross-references HARNESS-SPEC §7.4. Config value unchanged at 1 |

**Score:** 2/4 truths fully verified (SC-3, SC-4). SC-1 and SC-2 are PARTIAL — the stated intent is present in narrative/structure but the MERGE-02 shell body has confirmed correctness defects that prevent the invariant from being reliably enforced.

---

### Required Artifacts

| Artifact                   | Expected                                                                                              | Status       | Details                                                                                                                                          |
|----------------------------|-------------------------------------------------------------------------------------------------------|--------------|-------------------------------------------------------------------------------------------------------------------------------------------------|
| `docker/WORKFLOW.example.md` | In Progress prompt with Mergeability procedure, MERGE-01 pre-PR gate, MERGE-02 mergeability loop, concurrency comment | PARTIAL      | File exists and is substantive (+171 lines). Mergeability procedure section at line 600, pre-PR gate at lines 472-503, loop at lines 525-595, concurrency comment at line 67-69. Loop body has three confirmed correctness defects (CR-01, CR-02, CR-03). |
| `docs/HARNESS-SPEC.md`     | §7.4 amended with merge queue + post-merge gate + D-03 refinement + serial-foundation; §9 checklist bullet | VERIFIED     | §7.4 contains all four normative additions. §9 gains the `(§7.4)` bullet at lines 433-436.                                                      |
| `docs/DEPLOYMENT.md`       | `## Merge queue & branch protection` operator section before `## Upgrading`                           | VERIFIED     | Section at line 606; `## Upgrading` at line 656. Contains branch-protection table, merge queue settings, post-merge gate subsection, serial-foundation note, §7.4 cross-references. Note: WR-01 flags "Require linear history" + "Require merge queue" as mutually exclusive in GitHub UI — this is a warning-level doc correctness issue (see Anti-Patterns). |
| `docs/SPEC.md`             | §8.2 `Note (v0.4 Phase 4)` serial-foundation cross-reference to HARNESS-SPEC §7.4                    | VERIFIED     | Lines 736-738 confirmed.                                                                                                                         |

---

### Key Link Verification

| From                                          | To                                  | Via                                     | Status       | Details                                                                                                                          |
|-----------------------------------------------|-------------------------------------|-----------------------------------------|--------------|----------------------------------------------------------------------------------------------------------------------------------|
| In Progress STEP 1 conflict bullet            | Mergeability procedure section      | "follow the **Mergeability procedure** below" | VERIFIED | Line 447 confirmed.                                                                                                              |
| MERGE-01 step 4 (pre-PR gate)                 | Mergeability procedure section      | "follow the Mergeability procedure before pushing" | VERIFIED | Line 472 confirmed. The step-4 hard-stop block with `sinfonia-bot: pre-pr-gate-red` and `exit 0` without push is correctly wired. |
| MERGE-02 loop DIRTY/BEHIND arm                | Mergeability procedure (claimed)    | Comment "Follow the Mergeability procedure (fetch, rebase, gate, force-push)" | PARTIAL | Line 550 has the comment, but the body implements only bare rebase + push — missing conflict handling and gate run. The link is claimed in narrative but not implemented. |
| HARNESS-SPEC §9 checklist bullet              | HARNESS-SPEC §7.4                   | `(§7.4)` back-reference                 | VERIFIED     | Line 436 confirmed.                                                                                                              |
| DEPLOYMENT.md merge-queue section             | HARNESS-SPEC §7.4                   | Cross-reference links                   | VERIFIED     | Lines 643, 652 confirmed.                                                                                                        |
| SPEC.md §8.2 Note (v0.4 Phase 4)              | HARNESS-SPEC §7.4                   | `see docs/HARNESS-SPEC.md §7.4`         | VERIFIED     | Line 738 confirmed.                                                                                                              |
| WORKFLOW.example.md concurrency comment       | HARNESS-SPEC §7.4                   | `docs/HARNESS-SPEC.md §7.4` citation    | VERIFIED     | Line 67 confirmed.                                                                                                              |

---

### Data-Flow Trace (Level 4)

Not applicable — this phase delivers prompt documents and specification amendments, not application source code with data rendering. The "data" is agent instructions read verbatim from WORKFLOW.example.md, verified structurally above.

---

### Behavioral Spot-Checks

| Behavior                                           | Check                                                                      | Result                                                  | Status |
|----------------------------------------------------|----------------------------------------------------------------------------|---------------------------------------------------------|--------|
| Mergeability procedure section present in file     | `grep -qF "Mergeability procedure" docker/WORKFLOW.example.md`             | Match found at lines 600, 234, 447, 472, 527, 549, 550 | PASS   |
| force-with-lease present, "or merge" hedge absent  | `grep -qF "force-with-lease" && ! grep -qiF "or merge — match the project"` | force-with-lease at lines 235, 554, 627; no "or merge" hedge | PASS |
| Both stop markers present with idempotency probe   | `grep -qF "sinfonia-bot: pre-pr-gate-red"` + `grep -qF "sinfonia-bot: mergeability-loop-exhausted"` | Both present (lines 481, 575) with `grep -qF "$MARKER"` probe | PASS |
| Serial-foundation comment + §7.4 cite adjacent to "In Progress": 1 | `grep -B2 '"In Progress": 1' \| grep -q serial-foundation` | Lines 67-69 confirmed | PASS |
| HARNESS-SPEC §7.4 contains "merge queue", "post-merge", "BLOCKED", "serial" | awk extraction verified | All four terms confirmed | PASS |
| §9 checklist has `- [ ]` bullet with `(§7.4)` | awk extraction verified | Lines 433-436 confirmed | PASS |
| DEPLOYMENT.md `## Merge queue & branch protection` before `## Upgrading` | line 606 vs 656 | Confirmed | PASS |
| SPEC.md has `Note (v0.4 Phase 4)` cross-referencing §7.4 | lines 736-738 | Confirmed | PASS |
| MERGE-02 DIRTY/BEHIND arm: gate command before push | `sed -n '546,555p'` inspected | **No gate command found between rebase and push** — comment says "Re-run the repo-discovered gate; abort if red" but no command exists | FAIL |
| MERGE-02 UNKNOWN arm: bounded retry | lines 556-559 inspected | `sleep 15` + loop but ATTEMPT never incremented — unbounded | FAIL |
| MERGE-02 set-e + bare rebase survival | lines 531, 552 inspected | `set -e` on line 531, bare `git rebase origin/main` on line 552 — will abort script on conflict | FAIL |

---

### Probe Execution

No conventional `scripts/*/tests/probe-*.sh` files are declared or expected for this documentation-only phase. Step 7c SKIPPED (no probes).

---

### Requirements Coverage

| Requirement | Source Plan   | Description                                                                                       | Status   | Evidence                                                                                                                        |
|-------------|---------------|---------------------------------------------------------------------------------------------------|----------|---------------------------------------------------------------------------------------------------------------------------------|
| MERGE-01    | 04-01-PLAN.md | Pre-PR rebase: fetch origin/main, rebase/resolve, green gate, then push; hard stop on red        | PARTIAL  | Pre-PR gate narrative (step 4) and stop block correctly implement the intent. The MERGE-02 loop's CR-03 violates the same invariant in the loop path. MERGE-01 itself (the pre-PR stop) is structurally correct. |
| MERGE-02    | 04-01-PLAN.md | Bounded mergeability loop after push; transition only when conflict-free; exhaustion stays In Progress | PARTIAL  | Loop structure, BLOCKED/UNSTABLE/CLEAN arm, and exhaustion stop block are correct. DIRTY/BEHIND arm has three confirmed defects (CR-01, CR-02, CR-03) that prevent the loop from reliably enforcing the bounded rebase/resolve/re-run/force-push contract. |
| MERGE-03    | 04-02-PLAN.md | HARNESS-SPEC §7.4 names post-merge gate; §9 updated; operator setup note in DEPLOYMENT.md       | SATISFIED | §7.4 amended, §9 updated, DEPLOYMENT.md section confirmed — all three acceptance criteria met.                                  |
| MERGE-04    | 04-01-PLAN.md, 04-02-PLAN.md | Comment in concurrency config; serial-foundation convention written at milestone level      | SATISFIED | WORKFLOW.example.md concurrency comment confirmed. HARNESS-SPEC §7.4 defines convention. SPEC.md §8.2 cross-references it.     |

---

### Anti-Patterns Found

| File | Location | Pattern | Severity | Impact |
|------|----------|---------|----------|--------|
| `docker/WORKFLOW.example.md` | Line 552 | `git rebase origin/main` bare under `set -e` in DIRTY/BEHIND case | BLOCKER | Rebase of a DIRTY branch exits non-zero (conflicts stop it); set -e aborts the entire script. The agent never reaches the exhaustion guard or In Review gate check — it will likely fall through to step 8 and attempt the In Review transition on a broken workspace. |
| `docker/WORKFLOW.example.md` | Lines 553-554 | Comment says "Re-run the repo-discovered gate; abort if red" but no gate command exists before `git push --force-with-lease` | BLOCKER | Loop force-pushes untested code, directly violating the Mergeability procedure's own stated invariant ("The gate MUST be green before you push," line 624) and the SC-1 contract. |
| `docker/WORKFLOW.example.md` | Lines 556-559 | `UNKNOWN` case sleeps 15s and re-polls but never increments `ATTEMPT`; no bounded retry | BLOCKER | Persistent `UNKNOWN` from GitHub creates an infinite loop that spins until the orchestrator's turn/stall timeout kills the agent. |
| `docker/WORKFLOW.example.md` | Lines 564-567 | Catch-all `*)` arm treats any unexpected/null `mergeStateStatus` as conflict-free and proceeds to In Review | WARNING | Combined with WR-03 (PR_NUM potentially empty across blocks), a null mergeStateStatus falls to `*)` and silently skips the gate. Gate fails open on ambiguity. |
| `docker/WORKFLOW.example.md` | Line 540 (`$PR_NUM`) | `PR_NUM` used in step 7's block but set in a separate step 6 block; separate `set -e` scopes | WARNING | If the agent runs bash blocks independently, `PR_NUM` is unset; `gh pr view ""` returns a result that produces `null` through jq, which falls to `*)` and proceeds. |
| `docs/DEPLOYMENT.md` | Lines 620-622 | Table enables both "Require linear history: Enabled" and "Require merge queue: Enabled" | WARNING | These are mutually exclusive in GitHub branch protection settings. An operator following the table literally cannot configure both. |
| `docs/HARNESS-SPEC.md`, `docs/DEPLOYMENT.md` | Lines 348, 609 | `DEC-001`, `D-03`, `D-10`, "Theme D" referenced as if defined in published docs | INFO | Decision IDs are internal planning references not resolvable from `docs/`. Published docs cite them as canonical without defining them. |
| `docker/WORKFLOW.example.md` | Line 635 vs line 557 | Mergeability procedure table says "sleep 10" for UNKNOWN; loop says "sleeping 15s" | INFO | Two authoritative-sounding values for the same condition; agent has no basis to choose. |

**Debt marker gate:** No `TBD`, `FIXME`, or `XXX` markers found in modified files.

---

### Gaps Summary

**Root cause of the gap:** The MERGE-02 loop's shell body (lines 530-595 of `docker/WORKFLOW.example.md`) is a partial, divergent copy of the Mergeability procedure rather than a delegation to it. The plan's Task 2 intended the loop to "follow the Mergeability procedure" (the comment on line 550 says exactly that) but the body inlines a weaker, broken version: no conflict resolution step, no gate run, and bare rebase under `set -e`. This violates both SC-1 (green harness before push) and SC-2 (bounded rebase/resolve/re-run/force-push loop).

**What is correctly delivered:** The narrative framing of all four gates (MERGE-01, MERGE-02, MERGE-03, MERGE-04), the hard-stop blocks for pre-PR-gate-red and mergeability-loop-exhausted (both idempotent), the §7.4/§9/DEPLOYMENT.md/SPEC.md spec amendments, and the concurrency comment are all present and correct. The defects are concentrated in the 10-line DIRTY/BEHIND arm of the MERGE-02 loop.

**SC-3 and SC-4 are fully satisfied.** SC-1 and SC-2 are partially satisfied — the intent and most of the machinery are right, but the loop body that is supposed to enforce the "rebase/resolve/re-run/force-push" invariant does not resolve conflicts, does not run the gate before pushing, and does not bound UNKNOWN retries.

---

### Human Verification Required

None identified for the documentation / specification artifacts. The correctness defects above are verifiable from static code inspection (the code review confirms them), so no human behavioral testing is needed — the gap is a code fix, not a human judgment call.

---

_Verified: 2026-06-14_
_Verifier: Claude (gsd-verifier)_
