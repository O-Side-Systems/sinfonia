# Sinfonia Harness & Workflow — Improvement Analysis

- **Status:** Analysis / recommendation, pre-implementation
- **Version:** 1.0
- **Date:** 2026-06-13
- **Author:** O'Side Systems
- **Scope:** Improving `HARNESS-SPEC.md` and `WORKFLOW.example.md` for greenfield projects driven by Sinfonia + Linear + GitHub
- **Sources:** see [References](#references)

---

## TL;DR — the load-bearing reframe

The three observed problems live at **three different layers**, and only the third is mostly a harness-docs problem. Reaching for the harness to fix all three is why they've been sticky.

| # | Symptom | Real layer | Fix lives in |
|---|---|---|---|
| 1 | Linear dependencies ignored; stories start before blockers merge | Orchestrator config + tracker modeling | Linear relations + WORKFLOW STEP 0 guardrail |
| 2 | Merge conflicts; no pre-merge resolution loop | Workflow process + integration | WORKFLOW prompt loop + a real merge queue |
| 3 | Two stories implement the same logic in parallel | Decomposition + shared awareness | Spec decomposition + the repo context graph |

---

## Problem 1 — Linear dependencies ignored (orchestrator + tracker modeling, not the harness)

Sinfonia honors exactly one dependency mechanism: a Linear issue in `Todo` is skipped while a **`blocks` relation** points to a non-terminal issue; the blocked ticket becomes eligible on the next poll once the blocker closes; and **`In Progress` issues ignore blockers entirely**. That yields three failure modes — none fixable in the harness:

1. **Dependency not modeled as a `blocks`/`blocked by` relation.** Sub-issue hierarchies, "related" links, or prose are invisible to the orchestrator. The README documents gating *only* on blocks-relations — it says nothing about parent/child gating, yet `WORKFLOW.example.md`'s `{% if issue.children %}` block asserts the orchestrator only dispatched the parent because every child reached a terminal state. **That assumption is unverified.** Confirm in `src/orchestrator/` before trusting it; if absent, sub-issue dependencies are completely ungated.
2. **"Terminal" ≠ "merged."** The gate opens when the blocker hits a terminal state (`Done`/`Cancelled`), not when its PR lands on `main`. If Linear's GitHub integration or a human moves a blocker to `Done` before merge, the dependent unblocks against unmerged code. Align them: make **PR-merge-to-`main` the only thing that sets `Done`**.
3. **Dependent placed directly in `In Progress`.** `active_states: ["Todo", "In Progress"]` means a dependent in `In Progress` ignores its blocker. Dependents must wait in `Todo` until genuinely unblocked.

**Harness/workflow contribution (small):** a STEP 0 guardrail — before any work, verify each blocker is actually merged to `main` (`gh pr list --search "<blocker-id>" --state merged`); if not, comment on the Linear issue and stop, rather than build on sand.

## Problem 2 — merge conflicts (workflow loop + a real merge queue)

The literature is unanimous: **parallel generation is fine; parallel *merging* is where problems compound** — treat the merge queue as a serialization point, merge sequentially, test after each merge (Autonoma). The scale is real: one large dataset of agent PRs put Claude Code's merge-conflict rate near **26.9%** (AgenticFlict). Two gaps:

- **Gap A — no pre-PR integration.** The current workflow only resolves conflicts when a human kicks the ticket back. Add to the `In Progress` prompt, before the PR step: fetch `origin/main`, rebase/merge, resolve, **re-run the harness gate**, then push. Worktree isolation defers conflicts to intentional merge points (Augment) — so make the integration explicit at that point.
- **Gap B — no mergeability loop before handoff.** After push, poll `gh pr view --json mergeStateStatus`; if not `CLEAN`, rebase/resolve/re-run/force-push in a bounded loop; only transition to `In Review` when genuinely `CLEAN`.

**Structural fix:** adopt a real **merge queue** (GitHub native or Graphite) that auto-rebases each PR onto latest `main` and tests it before landing; use stacked PRs for dependent changes (Graphite). Run the **E2E harness on `main` post-merge**, not just on the PR — because the worst failure is silent: when two agents touch overlapping paths, the resolving agent picks a winner with no semantic understanding of the loser's intent, so features quietly vanish even though every PR passed CI (Autonoma). Tune `max_concurrent_agents` down for conflict-prone phases; keep large refactors and cross-cutting changes serial (DevSwarm).

## Problem 3 — two stories building the same thing (decomposition + the context graph)

This has a name: **"Resolved by Another PR" (RAPR)** was the single most frequent reason agent PRs went unmerged — ~22% — the issue already solved by a parallel PR (arXiv 2602.00164). Two root causes:

- **Decomposition.** Overlapping issues dispatched in parallel produce overlapping implementations. Spec-driven workflows attack this: the spec is the source of truth, the plan encodes architectural constraints, and code serves the spec (GitHub Spec Kit). Spec Kit's `specify → plan → tasks → implement` adds a **cross-artifact consistency/coverage analysis** step before implementation — a "do these tasks overlap?" check that's currently missing. Cheapest win: sequence shared-infrastructure stories as `blocks` for the leaves; build the shared thing once, then fan out. Sequential-by-default avoids duplicated/contradictory work because each agent reads the current truth (The Daily Developer).
- **No shared awareness.** Each agent sees only the repo at branch-time and its own issue — no sibling-branch visibility, no map of where shared logic lives. That is the context graph's job.

## The doc-graph / "just enough context" — right instinct, two refinements

The design described — small md docs, hyperlinked, sized so each task pulls just enough, updated as surgically as read — is current best practice, not a novel risk:

- The principle: find the **smallest set of high-signal tokens** that maximize the desired outcome; "context rot" degrades recall as the window fills (Anthropic).
- "Retrieve just enough per task" = the **just-in-time** strategy: keep lightweight identifiers (paths, links), load at runtime, enable **progressive disclosure** using folder hierarchy, naming, and timestamps as signals (Anthropic).
- "Linked via hyperlink" has a direct champion: the hyperlink as a powerfully efficient agent-traversal mechanism — "Hypertext as the Engine of Agent State" (Bleigh), the agentic analogue of HATEOAS.
- The format is standardized: **AGENTS.md** is an open "README for agents"; agents read the *nearest* file (closest wins), and large repos ship many (OpenAI's main repo has 88). Hierarchical, nearest-wins, hyperlinked AGENTS.md *is* the doc-graph — adopt it; don't invent a parallel convention.

**Refinement 1 — separate the sensor from the map.** `HARNESS-SPEC.md` is a clean *test-feedback contract* (a sensor). The doc-graph is a different artifact with a different lifecycle (the *knowledge the agent reads to build*). Don't overload one spec. Add a sibling **Repository Context Contract** the harness references and mandates as a §7-style convention.

**Refinement 2 — push back on "self-learning."** Autonomous doc generation backfires: an ETH study found LLM-generated context files reduced task success in **5 of 8** settings vs developer-written ones (via Augment). So "self-learning" must **not** mean background rewrites. It means: when a change alters an invariant, the agent proposes a **surgical diff to exactly the affected node(s), bumps that node's version, in the same PR as the code**, gated by CODEOWNERS. Knowledge accrues through *reviewed* doc-diffs — institutional memory in version control, not autonomous mutation. (Dovetails with the Wyrd provenance thesis: each node carries `version/status/derived_from/owners`, and the graph dogfoods the traceability claim.)

### Context Contract — concrete shape

- **Node shape:** one concern per file, ~one screen, front-matter (`title, version, status, owners, last_verified_sha, derived_from`), body, and a "See also" link list. `last_verified_sha` is the self-discovery hook — a node far behind `main` is flagged stale by a linter (a §5.5 invariant check).
- **Index/map node:** root `AGENTS.md` at the "right altitude" — specific enough to guide, flexible enough to leave heuristics to the model — whose job is *links out* plus a **module-ownership table** ("shared HTTP client → `src/net/`; don't roll your own"). Best single defense against Problem 3.
- **Read protocol (mandated in prompt):** start at root `AGENTS.md`; follow only links the acceptance criteria touch; `grep`/`glob` for the rest; never slurp `docs/`. This is Claude Code's own hybrid (CLAUDE.md up front + glob/grep just-in-time).
- **Write protocol (mandated):** before implementing shared logic, check the ownership map *and* open sibling PRs for overlap; after implementing, propose node diffs in the same PR; bump versions; no node edits outside the human-gated PR.
- **Structured note-taking:** keep the in-flight `.sinfonia/plans/` scratchpad (gitignored, per-issue) — endorsed by the NOTES.md persistence pattern (Anthropic).

---

## References

- Anthropic — *Effective context engineering for AI agents* — https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- M. Bleigh — *Context engineering is sleeping on the humble hyperlink* — https://mbleigh.dev/posts/context-engineering-with-links/ ; *Rules for Rules: Writing Docs for LLMs* — https://mbleigh.dev/
- AGENTS.md standard — https://agents.md/
- AGENTS.md guidance + ETH LLM-generated-context caveat — https://www.augmentcode.com/guides/how-to-build-agents-md
- GitHub Spec Kit / spec-driven development — https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/ ; https://github.com/github/spec-kit
- Serialize-merges / silent feature loss — https://getautonoma.com/blog/ai-subagent-merge-conflicts
- Merge queue + stacked PRs — https://www.graphite.com/guides/ai-code-merge-conflict-resolution
- Empirical conflict rates (AgenticFlict) — https://arxiv.org/pdf/2604.03551
- "Resolved by Another PR" — https://arxiv.org/html/2602.00164v1
- Worktree isolation / independent-units decomposition — https://www.augmentcode.com/guides/multi-agent-ai-system-code-development ; https://devswarm.ai/blog/parallel-vs-serial-development-a-visual-comparison
- Sequential agents argument — https://thedailydeveloper.substack.com/p/stop-parallelizing-your-ai-agents
