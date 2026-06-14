# Phase 5: Repository Context Contract — Research

**Researched:** 2026-06-14
**Domain:** Agent context engineering, doc-graph design, AGENTS.md conventions, CODEOWNERS gating, prompt protocol authoring
**Confidence:** HIGH (core patterns), MEDIUM (front-matter schema specifics)

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** The Repository Context Contract lives in a standalone `docs/CONTEXT-CONTRACT.md`. It specifies node shape + front-matter (`title, version, status, owners, last_verified_sha, derived_from`, "See also"), nearest-wins AGENTS.md semantics, one-concern/one-screen sizing, read protocol, and write protocol (node diffs ride the code PR under CODEOWNERS, version bump, no autonomous mutation). Rationale: DEC-004 — "separate the sensor (HARNESS-SPEC) from the map (Context Contract)."
- **D-02:** `docs/HARNESS-SPEC.md` §7 gets a one-line REQUIRED/RECOMMENDED convention entry referencing `docs/CONTEXT-CONTRACT.md`, plus a conformance-checklist line (§9). HARNESS-SPEC does NOT contain the contract body.
- **D-03:** Root `AGENTS.md` is the single agent entry point for the doc-graph. `README.md` stays human-facing. The stale `CLAUDE.md` reference in the prompts is dropped (not re-created).
- **D-04:** Every state prompt's "Orient" step is rewritten to the JIT protocol: start at root `AGENTS.md` → follow only links matching the issue's acceptance criteria → grep/glob for the rest → never blanket-read `docs/`. Applies to all state prompts in both `docker/WORKFLOW.md` and `docker/WORKFLOW.example.md` (Todo, In Progress, In Review, and the fallback "What to do" at line 499/737).
- **D-05:** Full dogfood — author real `AGENTS.md` nodes for: root + each crate (`crates/sinfonia`, `crates/sinfonia-bridge`, `crates/sinfonia-tracker`) + key top-level dirs (`docker/`, `docs/`). Each node renders <1 screen and validates against the contract.
- **D-06:** Templates ship for downstream repos: `templates/AGENTS.md` (with module-ownership table) + `templates/CODEOWNERS`.
- **D-07:** Node granularity = per-crate + key top-level dirs. `AGENTS.md` placed at each node's directory root.
- **D-08:** Module-ownership table keys path-globs → owners, consistent with `.github/CODEOWNERS`. Dogfood adds CODEOWNERS entries for the node paths.
- **Boundary:** Validation in Phase 5 is a documented manual conformance check. Automated stale-node + overlap linters are CTXGRAPH-04 / Phase 6. No autonomous doc generation (DEC-004).

### Claude's Discretion

- Exact wording and section number of the HARNESS-SPEC §7 entry and the conformance-checklist line.
- Internal layout of `templates/AGENTS.md` (as long as it is one-screen, carries the module-ownership table, and matches the contract's front-matter).
- The "See also" cross-links each dogfood node carries, and the precise per-crate node copy.

### Deferred Ideas (OUT OF SCOPE)

- Automated stale-node + overlap invariant linters (CTXGRAPH-04) — Phase 6.
- Decomposition consistency pass (CTXGRAPH-05) — Phase 6.
- Overlap-before-build check (CTXGRAPH-01) — Phase 6.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CTXGRAPH-03 | Repository Context Contract: standalone doc specifying node shape, front-matter (`title, version, status, owners, last_verified_sha, derived_from`, "See also"), nearest-wins semantics, one-concern/one-screen sizing, read protocol, write protocol. Ship `templates/AGENTS.md` (with module-ownership table) and `templates/CODEOWNERS`. | Front-matter schema, nearest-wins resolution, write protocol, CODEOWNERS patterns all researched below. |
| CTXGRAPH-02 | Just-in-time read protocol in every state prompt: start at root `AGENTS.md`, follow only links matching the issue's acceptance criteria, grep/glob for the rest, never slurp `docs/`. | JIT protocol wording, prompt insertion points, non-disruption to existing prompt structure all documented below. |
</phase_requirements>

---

## Summary

Phase 5 is a pure documentation and prompt-authoring phase — no code to compile, no packages to install, no external services. The deliverables are: a new `docs/CONTEXT-CONTRACT.md`, six new `AGENTS.md` dogfood nodes, two template files, two-line edits to `docs/HARNESS-SPEC.md`, and targeted prompt-text replacements in two WORKFLOW files.

The AGENTS.md format is an established open standard (agents.md), supported by 30+ tools. Its nearest-wins semantics are consistent across OpenAI Codex, Claude Code, and GitHub Copilot: the closest AGENTS.md to the file being edited takes precedence; deeper-nested files override root-level guidance. This is already proven — the OpenAI main repo ships 88 AGENTS.md files hierarchically. The standard itself has no required front-matter fields (it is plain Markdown), which means Phase 5's contract is introducing a *project-level* front-matter convention on top of the open standard. The chosen fields (`title, version, status, owners, last_verified_sha, derived_from`) align with the Wyrd provenance pattern documented in the harness-improvement-analysis, and `last_verified_sha` is the hook the Phase-6 stale linter will consume.

The JIT read protocol for the state prompts is applied per the controlling locked decision D-04: the blanket `docs/` slurp at the fallback "What to do" section (`docker/WORKFLOW.md:499` / `docker/WORKFLOW.example.md:737`) is rewritten to the full JIT protocol, AND a one-sentence JIT orient preamble is added to every other state prompt (Todo, In Progress) so each prompt positively directs the agent to the protocol. ("In Review" is named in D-04 but is intentionally not an active state and ships no prompt to edit — see Open Question 1 RESOLVED below.) The write protocol is already enforced by the existing CODEOWNERS `* @osidesys @leebrett` catch-all; Phase 5 adds per-path CODEOWNERS lines for `AGENTS.md` files and the new node paths to make it explicit.

**Primary recommendation:** Author the six dogfood nodes first (they validate the contract in practice), then derive the `docs/CONTEXT-CONTRACT.md` spec from what those nodes actually contain. This prevents the contract from being purely theoretical and ensures it stays <1 screen and actually fits in the target format before being locked down.

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Context Contract specification | Docs layer (`docs/CONTEXT-CONTRACT.md`) | HARNESS-SPEC §7 (reference + checklist line only) | D-01 explicitly separates "map" from "sensor" |
| Agent entry-point node | Repo root (`AGENTS.md`) | Per-directory area nodes | D-03: single entry point, nearest-wins resolution picks up area nodes |
| Module-ownership map | Root `AGENTS.md` body | Per-crate `AGENTS.md` (sub-tables) | Root is the index; crate nodes carry crate-scope tables |
| Write gate (no self-merge) | `.github/CODEOWNERS` | HARNESS-SPEC §7.3 (prose requirement) | CODEOWNERS is the enforced mechanism; §7.3 names it as convention |
| JIT read protocol | `docker/WORKFLOW.example.md` + `docker/WORKFLOW.md` (state prompts) | `docs/CONTEXT-CONTRACT.md` §Read Protocol (spec) | Prompts are where agents receive instructions; contract is the authoritative prose spec |
| Node conformance validation | Manual checklist (`docs/CONTEXT-CONTRACT.md` §Conformance) | HARNESS-SPEC §9 (checklist line) | Phase 6 owns the automated linter; Phase 5 owns the manual checklist |
| Templates for downstream repos | `templates/AGENTS.md` + `templates/CODEOWNERS` | — | D-06 |

---

## Standard Stack

### Core — No external packages

Phase 5 is entirely file authoring. There are no packages to install, no build steps, and no external tooling introduced.

| Artifact | Format | Location | Purpose |
|----------|--------|----------|---------|
| Context Contract | Markdown | `docs/CONTEXT-CONTRACT.md` | Canonical spec for the doc-graph |
| Root node | Markdown | `AGENTS.md` | Agent entry-point + module-ownership index |
| Crate nodes | Markdown | `crates/sinfonia/AGENTS.md`, `crates/sinfonia-bridge/AGENTS.md`, `crates/sinfonia-tracker/AGENTS.md` | Per-crate scope + ownership |
| Area nodes | Markdown | `docker/AGENTS.md`, `docs/AGENTS.md` | Top-level dir scope + ownership |
| Template node | Markdown | `templates/AGENTS.md` | Downstream-repo starter with module-ownership table |
| Template CODEOWNERS | Text | `templates/CODEOWNERS` | Downstream-repo starter with agent-path gates |
| CODEOWNERS additions | Text | `.github/CODEOWNERS` | Extend existing file with per-path agent gates |
| HARNESS-SPEC amendments | Markdown patch | `docs/HARNESS-SPEC.md` §7 + §9 | Convention entry + checklist line |
| Prompt amendments | YAML/Markdown patch | `docker/WORKFLOW.md` (line 499) + `docker/WORKFLOW.example.md` (line 737) | Replace blanket `docs/` slurp with JIT protocol |

### Package Legitimacy Audit

Not applicable — Phase 5 installs no external packages.

---

## Architecture Patterns

### System Architecture Diagram

```
Agent starts session
       │
       ▼
Read root AGENTS.md          ← entry point (D-03)
       │
       ├─ scan module-ownership table
       │     "Which paths own the capability I need?"
       │
       ├─ match acceptance criteria to linked area nodes
       │     follow ONLY matching links
       │     e.g. "webhook path" → crates/sinfonia-bridge/AGENTS.md
       │
       ├─ grep/glob for anything not yet linked
       │     "Where does X live? grep -r 'trait X' crates/"
       │
       └─ NEVER read docs/ wholesale
              ↑
        JIT protocol (D-04) replaces blanket slurp

Node write event (any AGENTS.md edit):
       │
       ▼
Surgical diff in same PR as code change
       ├─ bump node `version` in front-matter
       ├─ update `last_verified_sha` to HEAD commit
       ├─ CODEOWNERS triggers review by @osidesys @leebrett
       └─ NO autonomous mutation outside this gated PR path

Phase-6 stale linter (OUT OF SCOPE NOW):
       │
       ▼
reads last_verified_sha from each node front-matter
compares to main HEAD to detect staleness
```

### Recommended Project Structure

```
/                         ← root node (entry point)
├── AGENTS.md             ← root agent node: project orientation + module-ownership index
├── .github/
│   └── CODEOWNERS        ← extend with **AGENTS.md @osidesys @leebrett
├── crates/
│   ├── sinfonia/
│   │   └── AGENTS.md     ← crate node: orchestrator + agent runners scope
│   ├── sinfonia-bridge/
│   │   └── AGENTS.md     ← crate node: bridge + CI-feedback scope
│   └── sinfonia-tracker/
│       └── AGENTS.md     ← crate node: Linear/Jira adapter scope
├── docker/
│   └── AGENTS.md         ← area node: WORKFLOW.md + docker topology scope
├── docs/
│   ├── AGENTS.md         ← area node: spec docs, not an agent entry point
│   └── CONTEXT-CONTRACT.md  ← the contract (D-01)
└── templates/
    ├── AGENTS.md         ← downstream-repo template (D-06)
    └── CODEOWNERS        ← downstream-repo CODEOWNERS template (D-06)
```

---

## Pattern 1: Front-Matter Schema

**What:** YAML front-matter block at the top of every AGENTS.md node. The open AGENTS.md standard has no required front-matter (it is plain Markdown), so this is a project-level convention defined in `docs/CONTEXT-CONTRACT.md`. [CITED: agents.md — "there are no required fields"]

**Why these fields:**
- `title` — human label, also consumed by Phase-6 linter for graph indexing [ASSUMED: Phase 6 design intent]
- `version` — semver string, bumped with every doc diff in the PR (write protocol enforcement)
- `status` — `active | deprecated | draft` — lets the Phase-6 linter skip deprecated nodes [ASSUMED]
- `owners` — mirrors CODEOWNERS pattern owners; single source of truth for overlap check [CITED: harness-improvement-analysis.md — "Wyrd provenance" pattern]
- `last_verified_sha` — short Git SHA (7–8 chars) of the commit when a human last verified the node's accuracy against the code; the self-discovery hook for the Phase-6 stale linter [CITED: harness-improvement-analysis.md §Context Contract — concrete shape]
- `derived_from` — path or URL of the authoritative source this node distils (e.g., `docs/SPEC.md §11.6`) — supports DEC-004 traceability
- `See also` — in the node body, not front-matter — a bullet list of cross-links to related nodes

**Example:**
```yaml
---
title: "sinfonia-bridge — CI Feedback Crate"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "6598fe4"
derived_from: "docs/SPEC.md §11.6, docs/proposals/0001-harness-feedback-ingestion.md"
---
```

**Source:** Derived from harness-improvement-analysis.md §Context Contract (Wyrd provenance pattern) + agents.md standard conventions. [CITED: .planning/intel/ingest-sources/harness-improvement-analysis.md]

---

## Pattern 2: Nearest-Wins Resolution

**What:** When an agent edits a file in `crates/sinfonia-bridge/`, it reads *both* the root `AGENTS.md` AND `crates/sinfonia-bridge/AGENTS.md`. The bridge node takes precedence for bridge-specific guidance (more-specific overrides less-specific). [VERIFIED from: agents.md, OpenAI Codex documentation — "Files closer to your current directory override earlier guidance because they appear later in the combined prompt"]

**Concrete resolution algorithm (for the contract spec):**

1. Start at repository root: read `AGENTS.md` (always — it is the entry point per D-03).
2. Walk down the directory tree toward the file being edited; read each `AGENTS.md` found along the path (they stack, more-specific last = wins).
3. Deeper nodes can override root guidance. Root guidance applies everywhere unless overridden.
4. This is NOT a recursive traversal of all nodes — the agent reads only nodes on the path from root to the file it is working in, plus any nodes linked from root whose scope matches acceptance criteria.

**How "follow only acceptance-matching links" is specified concretely:**

The root `AGENTS.md` module-ownership table provides the link selection rule. Each row maps a path-glob to a crate node. An agent follows a link if and only if the acceptance criteria of its current issue mentions, touches, or implies a capability owned by that path-glob. Agents do not follow links whose owned paths are not implicated by the issue.

Example resolution rule for the state prompt:
> "Read root `AGENTS.md`. In the module-ownership table, identify which rows' path-globs match files the issue's acceptance criteria explicitly reference (by path, module name, or named capability). Open only those crate/area `AGENTS.md` nodes. For anything not yet discoverable via a link — run `grep -r '<term>' crates/` or `git ls-files | grep '<pattern>'`."

[CITED: sinfonia-harness-action-plan.md §1.6; harness-improvement-analysis.md §Read protocol]

---

## Pattern 3: One-Concern / One-Screen Sizing

**What:** Each AGENTS.md node covers exactly one concern (one crate, one top-level dir, one topic area) and fits within one terminal screen (~50 lines, ~3000 characters). [CITED: harness-improvement-analysis.md — "one concern per file, ~one screen"]

**Concrete heuristics:**
- Hard limit: 50 lines rendered (excluding front-matter YAML block — that adds ~8 lines). Total file <= 58 lines.
- The root node is *links out* + module-ownership table — NOT a prose description of the entire codebase.
- Each crate node contains: front-matter, 1–3 sentence scope summary, module-ownership sub-table for that crate's internal modules, key invariants (what the agent must not break), and "See also" links. No implementation prose.
- Test: render the file in a 80-column × 50-row terminal. If it does not fit without scrolling, it is too long.
- 32 KiB cap is the Codex engine hard-limit on AGENTS.md content. [CITED: morphllm.com/agents-md-guide — "32 KiB default cap (Codex enforces silent truncation)"] — this is not a practical concern at one-screen sizing.

**What goes in the node vs. what stays in the linked doc:**
- Node: "Orchestrator lives in `crates/sinfonia/src/orchestrator/`; don't edit `dispatch.rs` without also checking `retries.rs` coupling."
- NOT in node: explanations of *how* dispatch works (link to `docs/SPEC.md §8` for that).

**Why ETH study matters:** LLM-generated context files in ETH research reduced task success in 5/8 settings and increased inference cost 20–23%. Human-curated nodes that respond to observed failures outperform speculative rules. [CITED: augmentcode.com/guides/how-to-build-agents-md — ETH Zurich study summary] — confirms DEC-004's "no autonomous mutation" requirement.

---

## Pattern 4: Write Protocol

**What:** Node edits ride in the code PR that changes the related code. Write-only path: edit node in same branch → CODEOWNERS triggers required review → merge on approval. No standalone "doc-only" PRs for AGENTS.md nodes are required, but they are permitted if the code itself is unchanged.

**Version bump rule:** When a node changes in any way, `version` in front-matter increments (patch bump for corrections, minor bump for new sections). This gives the Phase-6 linter a monotone signal.

**`last_verified_sha` maintenance WITHOUT the Phase-6 linter:**
Since Phase 6 hasn't shipped the automated linter yet, `last_verified_sha` is set manually by the author at PR time. The procedure is:
1. Author runs `git rev-parse --short HEAD` on the commit being merged.
2. Updates `last_verified_sha` in the node to that value.
3. This value is what the Phase-6 linter will compare against `main HEAD` to flag staleness.

The contract must state: "At merge time, set `last_verified_sha` to the short SHA of the merge commit (or the last commit on the branch if squash-merged). The Phase-6 linter compares this value against the current `main` HEAD to detect drift." [ASSUMED: Phase 6 linter design — compatible with this protocol but not yet built]

**No autonomous mutation rule:** An agent that discovers a node is stale MUST propose a draft diff and stop — it MUST NOT commit or push a node edit outside a human-reviewed PR. This is DEC-004. The contract must contain this rule explicitly. [CITED: harness-improvement-analysis.md — "Refinement 2 — push back on 'self-learning'"]

---

## Pattern 5: CODEOWNERS Glob Mapping

**What:** `.github/CODEOWNERS` uses last-match-wins precedence. [VERIFIED from: GitHub docs — "Order is important; the last matching pattern takes the most precedence."]

**Existing CODEOWNERS (3 lines):**
```
# Default owners for everything in the repo.
# These users are requested for review when someone opens a PR.
* @osidesys @leebrett
```

The wildcard `*` already covers every AGENTS.md node. Phase 5 adds explicit entries to make ownership visible and to match the module-ownership table's path-globs:

```gitignore
# Context graph nodes — require review on any AGENTS.md edit
**/AGENTS.md @osidesys @leebrett

# Crate-level ownership (mirrors module-ownership table)
/crates/sinfonia/         @osidesys @leebrett
/crates/sinfonia-bridge/  @osidesys @leebrett
/crates/sinfonia-tracker/ @osidesys @leebrett
/docker/                  @osidesys @leebrett
/docs/                    @osidesys @leebrett
/templates/               @osidesys @leebrett
```

Note: `**/AGENTS.md` will match any AGENTS.md anywhere in the tree — this is the right pattern per GitHub docs. [VERIFIED from: GitHub CODEOWNERS docs — "`**/logs @octocat` will match `/logs`, `/build/logs`, `/scripts/logs`"]

**Module-ownership table in `AGENTS.md` vs. CODEOWNERS:** These are two representations of the same data. CODEOWNERS is the enforcement mechanism; the AGENTS.md module-ownership table is the *agent-readable* version the JIT protocol uses to pick which nodes to follow. They must be kept consistent — the contract must state that the AGENTS.md table mirrors CODEOWNERS; if they diverge, CODEOWNERS is authoritative.

---

## Pattern 6: JIT Read Protocol in State Prompts

**What:** Replace the blanket-slurp `Orient:` step with a precise three-step JIT protocol, and add a one-sentence JIT orient preamble to every other state prompt (D-04).

**Current state (blanket slurp — two locations):**
- `docker/WORKFLOW.md:499` (fallback prompt "What to do" step 1)
- `docker/WORKFLOW.example.md:737` (fallback prompt "What to do" step 1)

Both currently read:
```
1. Orient: `README.md`, `CLAUDE.md`, `docs/`.
```

The Todo and In Progress prompts in both files do NOT have an explicit orient step — they go directly to STEP 0 scripting. Per the controlling locked decision D-04, the JIT protocol is therefore applied in two ways:
1. The fallback "What to do" section: replace the slurp line with the full lettered JIT protocol.
2. The Todo and In Progress prompts: add a one-sentence JIT orient preamble (a positive direction to use the protocol) immediately after the prompt's intro line, before STEP 0. ("In Review" is named in D-04 but is intentionally not an active state — see Open Question 1 RESOLVED — so it ships no prompt to edit.)

**Replacement wording (Claude's discretion on exact text, but this is the contract-mandated substance):**

```
1. Orient (just-in-time):
   a. Read root `AGENTS.md` — always the first step.
   b. Locate the module-ownership table. Identify which path-globs match
      this issue's acceptance criteria.
   c. Read ONLY the `AGENTS.md` nodes linked from root whose scope matches
      those path-globs.
   d. For anything not yet linked, use `grep -r '<term>' crates/` or
      `git ls-files | grep '<pattern>'` — do NOT read `docs/` wholesale.
   Do NOT open `README.md`, `CLAUDE.md`, or browse `docs/` unless a link
   in an AGENTS.md node specifically directs you there.
```

**One-sentence preamble for Todo / In Progress (D-04):**
```
**Orient:** start at the repo-root `AGENTS.md` and follow only the links
relevant to this issue's acceptance criteria; grep/glob for anything else —
do not bulk-read `docs/`.
```

**Why the "CLAUDE.md" reference must be dropped (D-03):** There is no `CLAUDE.md` at the repo root today. The prompts currently instruct agents to read a non-existent file. Phase 5 removes this stale reference. The root `AGENTS.md` replaces it as the single agent entry point. [VERIFIED by: `find /Users/brettlee/work/sinfonia -maxdepth 1 -name "CLAUDE.md"` returns nothing]

**Non-disruption constraint:** The In Progress prompt is ~210 lines and contains complex STEP 0 scripting. The JIT orient preamble must be inserted as a self-contained sentence *before* STEP 0, not woven into the bash script. Precedence is: existing STEP 0 scripting is unchanged; JIT orient is added as an orientation preamble that the agent reads before running STEP 0.

---

## Pattern 7: HARNESS-SPEC §7 Amendment

**What:** Add a single convention entry to §7.3 Repository conventions (REQUIRED section) and a corresponding checklist line to §9.

**Section to amend:** §7.3 already lists four bullet conventions (branch naming, PR body, labels, CODEOWNERS, CI gates). Phase 5 adds a fifth:

```markdown
- **Context graph:** The repo MUST maintain a hierarchical `AGENTS.md` doc-graph
  conforming to `docs/CONTEXT-CONTRACT.md`. The root `AGENTS.md` is the agent
  entry point; all node edits ride in the code PR under CODEOWNERS. See
  [`docs/CONTEXT-CONTRACT.md`](docs/CONTEXT-CONTRACT.md) for the full contract.
```

**§9 checklist line (REQUIRED):**
```markdown
- [ ] A root `AGENTS.md` exists and conforms to `docs/CONTEXT-CONTRACT.md`;
      CODEOWNERS gates all `**/AGENTS.md` edits. (§7.3)
```

Note: The existing §7.3 already requires CODEOWNERS coverage. The context-graph entry reinforces this for AGENTS.md nodes specifically.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Graph traversal / link following | Custom file-walker code | Plain markdown links + agent grep/glob | Agents handle hypertext traversal natively; adding tooling creates a parallel mechanism to maintain |
| Front-matter parsing / validation | Custom YAML parser | Manual conformance checklist (Phase 5); CI linter (Phase 6 CTXGRAPH-04) | Phase 6 owns the linter; premature automation violates phase boundary |
| Stale-node detection | Time-based or git-hook automation | `last_verified_sha` updated manually at PR merge | Phase 6 owns the stale linter; the field shape just needs to anticipate it |
| Context Contract enforcement | Pre-commit hooks, CI gates | CODEOWNERS review gate (already in place) | The human review gate IS the enforcement; automated structural linting is Phase 6 |
| AGENTS.md generation | LLM auto-generation | Human-authored surgical diffs | ETH study: auto-generated context reduces task success in 5/8 settings (DEC-004) |

**Key insight:** The doc-graph is a *knowledge governance* problem, not a tooling problem. The value is in humans curating the content and reviewing changes — not in automation enforcing shape. Phase 5 builds the governance artifact; Phase 6 adds the safety net.

---

## Common Pitfalls

### Pitfall 1: Making the Root Node a Prose Document

**What goes wrong:** Root `AGENTS.md` becomes a 200-line architectural overview instead of a navigational index + module-ownership table.
**Why it happens:** Authors want to put "everything useful" in the root node because it's the entry point.
**How to avoid:** Root node body = front-matter + 1 orientation sentence + module-ownership table + "See also" links. All prose goes in the linked docs the table points to.
**Warning signs:** Root AGENTS.md exceeds 50 lines. It contains implementation details, not ownership links.

### Pitfall 2: CODEOWNERS Pattern Order Gotcha

**What goes wrong:** Adding `**/AGENTS.md @osidesys @leebrett` before the existing `* @osidesys @leebrett` wildcard and expecting the specific pattern to "win."
**Why it happens:** Confusion about last-match-wins precedence. The existing `*` wildcard at the top already covers everything; adding a more specific pattern AFTER it makes no practical difference for the current owner set, but matters if you ever want to add a different owner for AGENTS.md files.
**How to avoid:** Put the `**/AGENTS.md` entry AFTER the `*` catch-all line. The more specific pattern appears later = wins. [VERIFIED from: GitHub CODEOWNERS docs — last matching pattern takes most precedence]
**Warning signs:** Attempting to override ownership for a specific pattern and the override seems to have no effect.

### Pitfall 3: Conflating Contract Spec with Node Content

**What goes wrong:** `docs/CONTEXT-CONTRACT.md` grows to contain the actual module-ownership data that belongs in `AGENTS.md` nodes.
**Why it happens:** The contract IS the spec, not an instance. Ownership data goes in the nodes; the contract describes the shape.
**How to avoid:** Contract contains: front-matter schema definition, sizing rules, resolution algorithm, read protocol prose, write protocol prose, conformance checklist. No paths, no owners, no crate-specific data.

### Pitfall 4: JIT Protocol Breaks Existing STEP 0 Reference to CLAUDE.md

**What goes wrong:** The JIT replacement in the fallback prompt drops `CLAUDE.md` but the STEP 0 scripting in Todo/In Progress prompts still has comments referencing a CLAUDE.md workflow.
**Why it happens:** The two WORKFLOW files are long (504 and 742 lines) and the `CLAUDE.md` reference appears only in the fallback. A search for `CLAUDE.md` confirms only 2 occurrences (lines 499 and 737 — both in the fallback "Orient" step).
**How to avoid:** Search both files for `CLAUDE.md` before and after the edit. Confirm the only hits are the ones being replaced.

### Pitfall 5: `last_verified_sha` Set to Branch HEAD Instead of Merge SHA

**What goes wrong:** Author sets `last_verified_sha` to the branch tip before merge; after squash-merge, the SHA doesn't exist in `main` history, breaking the Phase-6 linter.
**Why it happens:** The merge commit SHA is not known until merge time.
**How to avoid:** The contract should instruct: "After merge, update `last_verified_sha` to the merge commit SHA in a follow-up commit, OR use the pre-merge HEAD SHA knowing the Phase-6 linter treats any SHA in `main` ancestry as valid." Practical recommendation: use the last commit on the branch; squash-merge preserves the merge base SHA in git history.
**Warning signs:** Phase-6 linter (when built) reports "SHA not found in main history."

### Pitfall 6: Template Node Too Large

**What goes wrong:** `templates/AGENTS.md` has extensive comments and explanations making it multi-screen, then gets copied as-is and bloats target repos.
**Why it happens:** Template authors want to be helpful with in-file documentation.
**How to avoid:** Template comments strip to `<!-- replace: ... -->` markers only. The actual guidance for filling in the template lives in `docs/CONTEXT-CONTRACT.md`. The template file itself must fit in one screen after the comments are replaced.

### Pitfall 7: Dogfood Nodes Copy-Paste Each Other

**What goes wrong:** All six dogfood AGENTS.md nodes look nearly identical, providing no real coverage differentiation per area.
**Why it happens:** The "validate the contract" goal gets satisfied superficially.
**How to avoid:** Each node's module-ownership table must name real paths and real shared capabilities that Phase-6's overlap-check will actually use. The crate nodes should reference actual source files (e.g., `crates/sinfonia/src/orchestrator/dispatch.rs` for the dispatch capability).

---

## Code Examples

### Front-matter Schema (Root Node)

```markdown
---
title: "Sinfonia — Repository Agent Entry Point"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "6598fe4"
derived_from: "README.md, docs/SPEC.md"
---

# Sinfonia — Agent Context

Sinfonia is a polling orchestrator that turns Linear/Jira issues into
agent sessions. Three Rust crates: `sinfonia` (orchestrator), `sinfonia-bridge`
(CI-feedback loop), `sinfonia-tracker` (Linear/Jira adapters).

## Module Ownership

| Capability | Path-glob | Owned By | Don't Roll Your Own |
|------------|-----------|----------|---------------------|
| Issue dispatch / orchestration | `crates/sinfonia/src/orchestrator/` | @osidesys @leebrett | See crates/sinfonia/AGENTS.md |
| Agent runners (Anthropic/OpenAI/CLI) | `crates/sinfonia/src/agent/` | @osidesys @leebrett | See crates/sinfonia/AGENTS.md |
| CI-to-issue feedback bridge | `crates/sinfonia-bridge/src/` | @osidesys @leebrett | See crates/sinfonia-bridge/AGENTS.md |
| Linear/Jira adapters | `crates/sinfonia-tracker/src/` | @osidesys @leebrett | See crates/sinfonia-tracker/AGENTS.md |
| Docker topology + WORKFLOW templates | `docker/` | @osidesys @leebrett | See docker/AGENTS.md |
| Spec docs + proposals | `docs/` | @osidesys @leebrett | See docs/AGENTS.md |

## See also
- [`docs/SPEC.md`](docs/SPEC.md) — orchestrator + bridge contract
- [`docs/HARNESS-SPEC.md`](docs/HARNESS-SPEC.md) — harness authoring spec
- [`docs/CONTEXT-CONTRACT.md`](docs/CONTEXT-CONTRACT.md) — this doc-graph's contract
```

**Source:** Derived from harness-improvement-analysis.md §Context Contract concrete shape. [CITED: .planning/intel/ingest-sources/harness-improvement-analysis.md]

---

### JIT Read Protocol (Prompt Replacement)

**Target location:** `docker/WORKFLOW.md:499` and `docker/WORKFLOW.example.md:737`

**Current text:**
```
1. Orient: `README.md`, `CLAUDE.md`, `docs/`.
```

**Replacement text:**
```
1. Orient (just-in-time — follow this order exactly):
   a. Read root `AGENTS.md`.
   b. In its module-ownership table, find rows whose path-globs match files
      this issue's acceptance criteria mention or imply.
   c. Read ONLY the `AGENTS.md` nodes linked from those matching rows.
   d. For anything not yet found: `grep -r '<term>' crates/` or
      `git ls-files | grep '<pattern>'`.
   Do NOT open `docs/` wholesale. Do NOT read `README.md` unless a node
   links to it for a specific reason.
```

**Source:** sinfonia-harness-action-plan.md §1.6; harness-improvement-analysis.md §Read protocol. [CITED]

---

### CODEOWNERS Extension

```gitignore
# Default owners for everything in the repo.
# These users are requested for review when someone opens a PR.
* @osidesys @leebrett

# Context graph nodes — explicit gate on all AGENTS.md edits (last-match-wins)
**/AGENTS.md @osidesys @leebrett

# Per-area ownership (mirrors AGENTS.md module-ownership table)
/crates/sinfonia/         @osidesys @leebrett
/crates/sinfonia-bridge/  @osidesys @leebrett
/crates/sinfonia-tracker/ @osidesys @leebrett
/docker/                  @osidesys @leebrett
/docs/                    @osidesys @leebrett
/templates/               @osidesys @leebrett
```

**Note:** Since all current owners are the same pair, the per-area entries are primarily documentation and anticipate future differentiated ownership. The `**/AGENTS.md` line makes the doc-node gate visible and explicit, even though `*` already covers it. [VERIFIED pattern: GitHub CODEOWNERS `**/` syntax]

---

### Manual Conformance Checklist (for docs/CONTEXT-CONTRACT.md §Conformance)

```markdown
## Conformance Checklist

A node is **contract-valid** when:

- [ ] Front-matter YAML block is present with all six required fields:
      `title`, `version`, `status`, `owners`, `last_verified_sha`, `derived_from`
- [ ] `status` is one of: `active`, `draft`, `deprecated`
- [ ] `owners` lists at least one owner matching a CODEOWNERS entry
- [ ] `last_verified_sha` is a 7-char git SHA present in `main` history
- [ ] Node body fits in one terminal screen (<=50 lines rendered, excluding front-matter)
- [ ] Body covers exactly one concern (one crate, one dir, one capability area)
- [ ] A "See also" link list is present at the end of the body
- [ ] The node does NOT duplicate content available in a linked spec doc
- [ ] If the node contains a module-ownership table, every row's path-glob
      matches a pattern present in `.github/CODEOWNERS`
```

---

## Runtime State Inventory

Not applicable — this is a greenfield documentation phase. No data migrations, no runtime service config changes, no OS-registered state, no secrets affected.

---

## Validation Architecture

> `workflow.nyquist_validation` is absent from `.planning/config.json` — treated as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Manual conformance checklist (no automated framework for this phase) |
| Config file | `docs/CONTEXT-CONTRACT.md` §Conformance Checklist (the checklist IS the test) |
| Quick run command | `grep -c '^---' AGENTS.md` (front-matter present), `wc -l AGENTS.md` (<= 58), manual review |
| Full suite command | Step through §Conformance Checklist for each of the 6 dogfood nodes + 1 template node |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CTXGRAPH-03 | Contract exists at `docs/CONTEXT-CONTRACT.md` | smoke | `ls docs/CONTEXT-CONTRACT.md` | Wave 0 creates it |
| CTXGRAPH-03 | Each dogfood node validates against conformance checklist | manual | Run checklist per node (7 nodes total) | Wave 0 creates nodes |
| CTXGRAPH-03 | `templates/AGENTS.md` + `templates/CODEOWNERS` exist and render <1 screen | smoke | `wc -l templates/AGENTS.md` (expect <= 58) | Wave 0 creates them |
| CTXGRAPH-03 | HARNESS-SPEC §7.3 has Context Contract convention entry | smoke | `grep -c 'CONTEXT-CONTRACT' docs/HARNESS-SPEC.md` (expect >= 1) | Wave 0 amends §7.3 |
| CTXGRAPH-03 | HARNESS-SPEC §9 checklist has context-graph line | smoke | `grep -c 'CONTEXT-CONTRACT\|AGENTS.md' docs/HARNESS-SPEC.md` (expect >= 2) | Wave 0 amends §9 |
| CTXGRAPH-02 | No blanket `docs/` read instruction remains in WORKFLOW files | smoke | `grep -n "Orient.*docs/" docker/WORKFLOW.md docker/WORKFLOW.example.md` (expect 0 hits) | Wave 0 replaces |
| CTXGRAPH-02 | JIT orient step present in every state prompt | smoke | `grep -c "AGENTS.md" docker/WORKFLOW.md docker/WORKFLOW.example.md` (expect >= 1 per state prompt) | Wave 0 amends |
| CTXGRAPH-02 | No `CLAUDE.md` reference in orient step | smoke | `grep -n "CLAUDE.md" docker/WORKFLOW.md docker/WORKFLOW.example.md` (expect 0 hits) | Wave 0 replaces |

### Sampling Rate

- **Per task commit:** Run the smoke commands above (all take < 5 seconds, pure grep/ls)
- **Per wave merge:** Full manual conformance checklist for all 7 AGENTS.md nodes
- **Phase gate:** All smoke commands green + manual checklist complete before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `docs/CONTEXT-CONTRACT.md` — does not exist yet; Wave 0 creates it
- [ ] `AGENTS.md` (root) — does not exist yet; Wave 0 creates it
- [ ] `crates/sinfonia/AGENTS.md` — does not exist yet
- [ ] `crates/sinfonia-bridge/AGENTS.md` — does not exist yet
- [ ] `crates/sinfonia-tracker/AGENTS.md` — does not exist yet
- [ ] `docker/AGENTS.md` — does not exist yet
- [ ] `docs/AGENTS.md` — does not exist yet
- [ ] `templates/AGENTS.md` — does not exist yet (`templates/` dir does not exist)
- [ ] `templates/CODEOWNERS` — does not exist yet

---

## Security Domain

Phase 5 introduces no code, no external inputs, no authentication, and no data flows. ASVS categories V2–V6 do not apply. The only security-adjacent consideration is DEC-004 (no autonomous mutation), which is enforced by the CODEOWNERS human gate already in place.

---

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | Generating `last_verified_sha` values at merge time | ✓ | (in repo) | — |
| `grep` / `wc` / `ls` | Smoke test commands | ✓ | (standard POSIX) | — |
| Text editor / PR tooling | Authoring AGENTS.md nodes, HARNESS-SPEC patches, WORKFLOW prompt edits | ✓ | — | — |

**Missing dependencies with no fallback:** None.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Blanket `docs/` slurp in orient step | JIT: root AGENTS.md → acceptance-matching links → grep/glob | Phase 5 (this phase) | Reduces irrelevant context tokens; prevents docs/ from growing into a cost sink |
| No agent entry point; agents explore freely | Root `AGENTS.md` as the defined entry point | Phase 5 | Predictable, auditable starting point for every agent session |
| CODEOWNERS covers only code paths | CODEOWNERS extended to gate `**/AGENTS.md` explicitly | Phase 5 | Makes doc-node write protocol enforceable, not aspirational |
| Context in README (human-facing) | Context in AGENTS.md (agent-facing) | Phase 5 | Clear separation of concerns; README stays human-facing |

**Deprecated/outdated:**
- `Orient: README.md, CLAUDE.md, docs/` prompt instruction: removed in Phase 5 (stale CLAUDE.md reference + blanket docs/ slurp both eliminated)

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Phase 6 stale linter will compare `last_verified_sha` against `main` HEAD ancestry | Pattern 1 (Front-Matter Schema), Pattern 4 (Write Protocol) | If Phase 6 uses a different stale-detection mechanism, `last_verified_sha` field may be renamed or reshared; low risk since Phase 5 owns the field definition |
| A2 | Phase 6 stale linter will accept a short SHA (7 chars) present in `main` ancestry as valid | Pitfall 5 | If Phase 6 requires a full 40-char SHA, the manual process for setting `last_verified_sha` needs to change |
| A3 | `status: deprecated` is a valid field value that the Phase-6 linter will use to skip stale-detection on deprecated nodes | Pattern 1 | If Phase 6 ignores the `status` field, deprecated nodes will generate false-positive stale alerts |
| A4 | The Todo and In Progress prompts in both WORKFLOW files do NOT currently have an explicit orient step (confirmed by grep — only fallback at lines 499/737 has "Orient: README.md, CLAUDE.md, docs/") | Pattern 6 (JIT Protocol) | If a mid-prompt orient instruction was missed by grep, the phase edit is incomplete. Note: D-04 still requires adding a JIT orient *preamble* to Todo/In Progress even though they have no slurp to remove. |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed. (Four assumptions logged above — all low-risk and self-contained within this phase.)

---

## Open Questions (RESOLVED)

1. **Should the JIT orient step also be added to the Todo and In Progress prompts, or only to the fallback?**
   - What we know: sinfonia-harness-action-plan.md §1.6 says "top of `Todo` and `In Progress` prompts (orientation section, replacing the bare 'Orient: README.md, CLAUDE.md, docs/')." But grepping both WORKFLOW files shows the bare orient line appears ONLY in the fallback (lines 499/737) — the Todo and In Progress prompts have no orient step at all.
   - **RESOLVED:** Per the controlling locked decision **D-04**, the JIT orient protocol is added to **ALL** state prompts (Todo, In Progress, In Review, and the fallback) in both WORKFLOW files — not the fallback only. D-04 overrides the earlier fallback-only recommendation. Concretely: the fallback "What to do" Orient step is rewritten to the full lettered JIT protocol; the Todo and In Progress prompts each gain a one-sentence JIT orient preamble (positively directing the agent to the protocol) inserted before STEP 0. "In Review" is named in D-04 but is intentionally *not* an active state in either WORKFLOW file (it is the human review gate and ships no agent prompt), so there is no In Review prompt to edit — honoring D-04 means covering every state prompt that actually exists: Todo, In Progress, and the fallback. Plan 05-04 implements this across both files.

2. **What is the "right altitude" for the root AGENTS.md to function as an index without leaking implementation?**
   - What we know: harness-improvement-analysis.md says "specific enough to guide, flexible enough to leave heuristics to the model." The module-ownership table is the primary content.
   - **RESOLVED:** Use **crate-level path-globs** (e.g. `crates/sinfonia/`) in the root node's module-ownership table, with finer-grained **sub-module globs** (e.g. `crates/sinfonia/src/orchestrator/`) only inside each per-crate node. This preserves the hierarchical nearest-wins resolution semantics — coarse at root, fine-grained at the area node — and matches Plan 05-02 Task 2 (root node carries crate-level globs; per-crate nodes carry sub-module tables).

---

## Sources

### Primary (HIGH confidence)

- `.planning/intel/ingest-sources/harness-improvement-analysis.md` — doc-graph design, Context Contract concrete shape, write protocol, read protocol, ETH study, refinement 1 + 2
- `.planning/intel/ingest-sources/sinfonia-harness-action-plan.md` — §1.6 (JIT read protocol), §2.1 (HARNESS-SPEC amendment), §3.1 (root AGENTS.md template), §3.2 (front-matter convention)
- `.planning/phases/05-repository-context-contract/05-CONTEXT.md` — locked decisions D-01 through D-08
- `docs/HARNESS-SPEC.md` — §7.3 (existing conventions), §7.4 (merge gating), §9 (conformance checklist) — amendment targets
- `docker/WORKFLOW.md` + `docker/WORKFLOW.example.md` — prompt structure, amendment targets at lines 499/737
- `.github/CODEOWNERS` — current ownership state (3 lines: `* @osidesys @leebrett`)
- GitHub CODEOWNERS documentation — last-match-wins precedence, `**/` pattern syntax [CITED: docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners]

### Secondary (MEDIUM confidence)

- agents.md standard — nearest-wins semantics, no required front-matter, 32 KiB cap [CITED: agents.md]
- OpenAI Codex AGENTS.md documentation — hierarchy resolution algorithm, nearest-wins confirmed [CITED: developers.openai.com/codex/guides/agents-md]
- Augment Code AGENTS.md guide + ETH Zurich study summary — sizing (100–150 lines), module-level documentation effectiveness, auto-generation risk [CITED: augmentcode.com/guides/how-to-build-agents-md]
- Anthropic context engineering — just-in-time loading, progressive disclosure, smallest high-signal tokens [CITED: anthropic.com/engineering/effective-context-engineering-for-ai-agents]
- mbleigh.dev hyperlink article — hyperlink as agent traversal mechanism, JIT loading rationale [CITED: mbleigh.dev/posts/context-engineering-with-links/]
- morphllm.com AGENTS.md 2026 guide — AGENTS.md vs CLAUDE.md comparison, 32 KiB cap [CITED: morphllm.com/agents-md-guide]

### Tertiary (LOW confidence — not used in primary recommendations)

- agentsmd/agents.md GitHub issue #10 — proposed front-matter fields (`description`, `alwaysApply`, `globs`) — these are tool-specific extensions, not the project-level convention Phase 5 is defining [LOW — tool-dependent, not authoritative for this project's schema]

---

## Metadata

**Confidence breakdown:**
- Front-matter schema (fields + rationale): HIGH — derived directly from harness-improvement-analysis.md which cites specific sources (Wyrd provenance, Anthropic)
- Nearest-wins semantics: HIGH — verified from agents.md standard and OpenAI Codex docs
- JIT protocol wording: HIGH — direct derivation from harness-action-plan §1.6 with concrete prompt locations grep-verified in WORKFLOW files
- CODEOWNERS patterns: HIGH — verified from GitHub docs, existing CODEOWNERS read
- `last_verified_sha` / Phase-6 linter compatibility: MEDIUM — Phase-6 design not yet written; assumptions logged in A1–A3
- One-screen sizing (50 lines): MEDIUM — "~one screen" from harness-improvement-analysis.md; 50 lines is a reasonable operationalization, not a spec-stated number

**Research date:** 2026-06-14
**Valid until:** 90 days (stable domain — AGENTS.md standard, CODEOWNERS syntax, and git conventions are stable)
