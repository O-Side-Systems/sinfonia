# Sinfonia Repo — Actionable Change Plan

- **Status:** Actionable backlog, ready to execute
- **Version:** 1.0
- **Date:** 2026-06-13
- **Author:** O'Side Systems
- **Derived from:** `harness-improvement-analysis.md` v1.0
- **Repo:** `github.com/O-Side-Systems/sinfonia` (artifacts: `HARNESS-SPEC.md`, `WORKFLOW.example.md`, `docs/SPEC.md`, `docs/proposals/0001-…`, `src/orchestrator/`)

Legend — **P0** do first / unblocks scope · **P1** high value · **P2** follow-up. Effort in ½-day units (S ≤ ½d, M ≈ 1d, L ≥ 2d). Each item names the file, the change, and a done-check.

---

## Phase 0 — Verify before you build (P0, do these first)

These determine how much of Phase 1 you actually need.

- [ ] **0.1 — Confirm parent/child gating behavior.** *(S)*
  **Where:** `src/orchestrator/` (grep for `children`, `parent`, `sub_issue`, `subIssue`).
  **Why:** `WORKFLOW.example.md`'s `{% if issue.children %}` block assumes the orchestrator only dispatches a parent after all children reach terminal. The README documents gating **only** on `blocks` relations. (Problem 1)
  **Done-check:** you can point to the code that gates parents on children, *or* you've confirmed it doesn't exist and updated the workflow comment to stop claiming it.

- [ ] **0.2 — Confirm the blocker check keys off `blocks` relations only.** *(S)*
  **Where:** `src/tracker/` (Linear adapter) + `src/orchestrator/`.
  **Why:** Establishes that dependencies *must* be modeled as `blocks`/`blocked by`, not hierarchy or "related". (Problem 1)
  **Done-check:** documented in `docs/SPEC.md` as an explicit, tested invariant.

- [ ] **0.3 — Decide the integration model: merge queue vs. stacked PRs vs. serial.** *(S)*
  **Why:** Phase 2 items branch on this. Default recommendation: GitHub native merge queue + serial foundational stories. (Problem 2)
  **Done-check:** one-line decision recorded in `docs/SPEC.md` or an ADR.

---

## Phase 1 — `WORKFLOW.example.md` changes (P0/P1, highest leverage)

This is the template every target repo copies, so changes here propagate. All snippets go in the **`In Progress`** state prompt unless noted.

- [ ] **1.1 — STEP 0 blocker-merged guardrail.** *(M, P0 — Problem 1)*
  **Where:** STEP 0 of **both** `Todo` and `In Progress` prompts, after the existing PR/CI checks.
  **Change:** before doing work, resolve this issue's `blocks`/`blocked-by` set from Linear and verify each blocker's PR is **merged to `main`** — not merely "Done". If any blocker is unmerged, comment on the Linear issue and stop.
  ```bash
  # Blockers must be MERGED, not just terminal. Derive blocker IDs from Linear relations.
  set +e
  BLOCKERS=$(curl -sS -H "Authorization: $LINEAR_API_KEY" -H "Content-Type: application/json" \
    https://api.linear.app/graphql \
    -d '{"query":"{ issue(id:\"{{ issue.identifier }}\"){ relations{ nodes{ type relatedIssue{ identifier } } } } }"}' \
    | jq -r '.data.issue.relations.nodes[] | select(.type=="blocks" or .type=="blocked_by") | .relatedIssue.identifier')
  for B in $BLOCKERS; do
    MERGED=$(gh pr list --search "$B in:title,body" --state merged --json number -q '.[0].number')
    if [ -z "$MERGED" ]; then
      echo "BLOCKED: $B is not merged to main yet. Stopping."
      # post a Linear comment via commentCreate, then exit without transitioning
      exit 0
    fi
  done
  set -e
  ```
  **Done-check:** an issue with an unmerged blocker exits without code changes and leaves a Linear comment.

- [ ] **1.2 — Pre-PR rebase + re-run harness.** *(M, P0 — Problem 2, Gap A)*
  **Where:** `In Progress` "Implementation continuation" / "Fresh work", immediately before the PR-create step.
  **Change:**
  ```bash
  git fetch origin --quiet
  git rebase origin/main || { echo "resolve conflicts, then: git rebase --continue"; }
  # …agent resolves conflicts here…
  <run the project's harness gate>   # MUST be green post-rebase before pushing
  git push --force-with-lease -u origin "sinfonia/{{ issue.identifier | downcase }}"
  ```
  **Done-check:** PRs are opened only from a branch rebased on current `main` with a green harness run.

- [ ] **1.3 — Mergeability loop before handoff.** *(M, P0 — Problem 2, Gap B)*
  **Where:** `In Progress`, after push, before the transition-to-`In Review` step.
  **Change:**
  ```bash
  MS=""
  for i in 1 2 3 4 5; do
    sleep 5
    MS=$(gh pr view "$PR_NUM" --json mergeStateStatus -q .mergeStateStatus 2>/dev/null)
    [ "$MS" = "CLEAN" ] && break
    case "$MS" in
      DIRTY|BEHIND|BLOCKED)
        git fetch origin --quiet && git rebase origin/main
        # …resolve…; re-run harness; 
        git push --force-with-lease ;;
    esac
  done
  [ "$MS" = "CLEAN" ] || { echo "NOT MERGEABLE ($MS) — not transitioning to In Review"; exit 1; }
  ```
  **Done-check:** the issue reaches `In Review` only when `mergeStateStatus == CLEAN`.

- [ ] **1.4 — "Check before you build" overlap step.** *(M, P1 — Problem 3)*
  **Where:** `Todo` (planning) and `In Progress` "Fresh work", before writing shared/utility code.
  **Change (prompt prose + commands):** instruct the agent to (a) consult the repo's module-ownership map in root `AGENTS.md`, and (b) scan sibling branches/PRs for overlapping work:
  ```bash
  echo "=== sibling sinfonia branches touching likely-shared paths ==="
  git ls-remote --heads origin 'sinfonia/*' | awk '{print $2}'
  gh pr list --state open --json number,title,headRefName,files \
    -q '.[] | {number, title, headRefName, files: [.files[].path]}'
  ```
  Prompt rule: *"If an open sibling PR already introduces the shared capability you need, depend on it (add a `blocks` relation and stop) rather than re-implementing."*
  **Done-check:** agent logs a sibling-scan and chooses reuse-or-block over duplicate implementation in a seeded test case.

- [ ] **1.5 — Lower concurrency for conflict-prone phases + document it.** *(S, P1 — Problem 2)*
  **Where:** `agent.max_concurrent_agents` / `max_concurrent_agents_by_state` block.
  **Change:** keep `In Progress` at 1 (already set); add a comment that foundational/cross-cutting stories run serially and only leaf stories fan out. Consider `max_concurrent_agents: 1` while a foundational milestone is in flight.
  **Done-check:** comment present; a milestone-level convention written down.

- [ ] **1.6 — Add the read protocol to every state prompt.** *(S, P1 — Problem 3)*
  **Where:** top of `Todo` and `In Progress` prompts (orientation section, replacing the bare "Orient: README.md, CLAUDE.md, docs/").
  **Change:** *"Start at root `AGENTS.md`. Follow only the links whose scope matches this issue's acceptance criteria. Use `grep`/`glob` for anything not yet linked. Do **not** read all of `docs/`."*
  **Done-check:** prompts no longer instruct a blanket `docs/` read.

---

## Phase 2 — `HARNESS-SPEC.md` changes

- [ ] **2.1 — New section: "§N Repository Context Contract".** *(L, P1 — Problem 3)*
  **Where:** new top-level section in `HARNESS-SPEC.md` (or sibling `docs/CONTEXT-CONTRACT.md` referenced from §7).
  **Change:** specify the doc-graph as a producer-side contract (see Phase 3 for the node/protocol detail it points to). Mandate: node shape + front-matter, the read protocol, the write protocol, and that all node edits ride in the code PR under CODEOWNERS.
  **Done-check:** §7 lists the Context Contract as a REQUIRED/RECOMMENDED convention with a conformance checklist line.

- [ ] **2.2 — Tighten §7.4 Merge gating: require merge queue + post-merge harness gate.** *(M, P0 — Problem 2)*
  **Where:** `HARNESS-SPEC.md` §7.4.
  **Change:** add MUST/SHOULD language: a target repo SHOULD run a merge queue that rebases-and-tests each PR against latest `main`; the harness gate MUST also run on `main` post-merge (not only on the PR) so "green" reflects integrated state.
  **Done-check:** §7.4 names the post-merge gate; §9 checklist updated.

- [ ] **2.3 — Extend §5.5 invariant gating with two new checks.** *(M, P1 — Problems 2 & 3)*
  **Where:** `HARNESS-SPEC.md` §5.5.
  **Change:**
  - **Stale-node check:** fail (or warn) when a context-graph node's `last_verified_sha` lags `main` by more than a configured window.
  - **Overlap check:** fail when two open `sinfonia/*` PRs modify the same owned module (per the ownership map).
  **Done-check:** both checks specified as manifest-driven linters that "fail loud" locally and in CI.

- [ ] **2.4 — Add a STEP-0 dependency-verification expectation to §7.3 conventions.** *(S, P1 — Problem 1)*
  **Where:** `HARNESS-SPEC.md` §7.3 (Repository conventions).
  **Change:** document that dependencies MUST be modeled as Linear `blocks` relations, that `Done` SHOULD be set by PR-merge, and that the workflow verifies blocker-merge before work (links to WORKFLOW 1.1).
  **Done-check:** convention text present; cross-referenced from the workflow.

---

## Phase 3 — New artifacts the specs prescribe (target-repo bootstrap)

These are templates Sinfonia ships so a `Sinfonia-ready` repo bootstraps the context graph. Put them under a `templates/` or `docs/templates/` dir in the sinfonia repo.

- [ ] **3.1 — Root `AGENTS.md` template + module-ownership table.** *(M, P1 — Problem 3)*
  **Where:** `templates/AGENTS.md`.
  **Change:** entry-point node at the "right altitude": project orientation, links out to area nodes, and a **module-ownership table** (capability → canonical path → "don't roll your own"). Include the front-matter convention.
  **Done-check:** template renders as a < 1-screen node that is mostly links + ownership table.

- [ ] **3.2 — Context-graph node front-matter convention.** *(S, P1 — Problem 3)*
  **Where:** `docs/CONTEXT-CONTRACT.md` (or §N of the harness spec).
  **Change:** define required front-matter: `title, version, status, owners, last_verified_sha, derived_from` + a "See also" link list. State the nearest-wins precedence (AGENTS.md semantics) and the one-concern/one-screen sizing rule.
  **Done-check:** a sample node validates against the convention; the stale-node linter (2.3) reads `last_verified_sha`.

- [ ] **3.3 — `CODEOWNERS` template covering context-graph + Sinfonia-touched paths.** *(S, P1 — Problems 2 & 3)*
  **Where:** `templates/CODEOWNERS`.
  **Change:** require human review on `AGENTS.md`/context nodes and on Sinfonia-touched code paths so the agent can edit but never self-merge doc or code.
  **Done-check:** template present; referenced from §7.3.

- [ ] **3.4 — Branch-protection / merge-queue setup note.** *(S, P1 — Problem 2)*
  **Where:** `docs/` operator note (e.g., `docs/integration-setup.md`).
  **Change:** step-by-step to enable GitHub merge queue + required status checks (incl. the harness gate) + "Done on PR merge" Linear automation.
  **Done-check:** a new target repo can be made conformant by following the note.

---

## Phase 4 — Decomposition discipline (process, P2)

- [ ] **4.1 — Add a pre-implementation consistency check to the spec-deriving step.** *(M, P2 — Problem 3)*
  **Where:** `HARNESS-SPEC.md` §4.1 (the OPTIONAL spec-deriving step) + your authoring playbook.
  **Change:** borrow Spec Kit's pattern — after decomposition into stories, run a cross-artifact "do these overlap/contradict?" pass and an explicit ordering pass that emits `blocks` relations for shared-infrastructure-before-leaves.
  **Done-check:** decomposition output includes an overlap report and a dependency graph expressed as `blocks` relations.

- [ ] **4.2 — Sequence foundational stories serially.** *(S, P2 — Problems 2 & 3)*
  **Where:** authoring convention.
  **Change:** shared infra (clients, base types, cross-cutting utilities) ships in serialized stories that `blocks` the parallel leaf stories.
  **Done-check:** milestone plans show a serial foundation followed by a parallel fan-out.

---

## Suggested execution order

1. **Phase 0** (verify scope) → **1.1–1.3** (the dependency guardrail + the two merge loops; these alone address Problems 1 & 2 at the workflow layer).
2. **2.2** + **3.3/3.4** (merge queue + CODEOWNERS + branch protection; the structural Problem-2 fix).
3. **3.1/3.2** + **1.4/1.6** + **2.1** (stand up the context graph; addresses Problem 3).
4. **2.3** + **4.1/4.2** (invariant linters + decomposition discipline; hardening).

Problems 1 and 2 are mostly closed after step 1–2; Problem 3 needs the context graph (step 3) plus decomposition discipline (step 4).
