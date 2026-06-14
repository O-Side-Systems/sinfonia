# Phase 5: Repository Context Contract - Context

**Gathered:** 2026-06-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Deliver a reviewed, hierarchical doc-graph so agents read *just enough* context:

1. A **Repository Context Contract** defining node shape + front-matter, nearest-wins
   `AGENTS.md` semantics, one-concern/one-screen sizing, and read/write protocol
   (diffs ride the code PR under CODEOWNERS, version bump, no autonomous mutation).
2. Shipped `templates/AGENTS.md` (with a module-ownership table) and `templates/CODEOWNERS`.
3. A HARNESS-SPEC §7 convention entry + conformance-checklist line for the contract.
4. Every state prompt switched to a **just-in-time read protocol** (start at root
   `AGENTS.md`, follow only acceptance-matching links, grep/glob for the rest) — no more
   blanket `docs/` slurp.

**Out of scope (Phase 6):** automated stale-node + overlap linters (CTXGRAPH-04),
overlap-before-build (CTXGRAPH-01), decomposition consistency pass (CTXGRAPH-05).

</domain>

<decisions>
## Implementation Decisions

### Contract Home & Spec Wiring
- **D-01:** The Repository Context Contract lives in a **standalone `docs/CONTEXT-CONTRACT.md`** (the "map"). It specifies node shape + front-matter (`title, version, status, owners, last_verified_sha, derived_from`, "See also"), nearest-wins `AGENTS.md` semantics, one-concern/one-screen sizing, read protocol, and write protocol (node diffs ride the code PR under CODEOWNERS, version bump, **no autonomous mutation**). Rationale: DEC-004 — "separate the sensor (HARNESS-SPEC) from the map (Context Contract)."
- **D-02:** `docs/HARNESS-SPEC.md` **§7** gets a one-line REQUIRED/RECOMMENDED convention entry **referencing** `docs/CONTEXT-CONTRACT.md`, plus a conformance-checklist line (same mechanism Phase 4 used in §9). HARNESS-SPEC does **not** contain the contract body.

### Root Entry Point & Read Protocol
- **D-03:** **Root `AGENTS.md` is the single agent entry point** for the doc-graph. `README.md` stays human-facing and is *not* a graph node entry. There is no `CLAUDE.md` at root today; the prompts' stale `CLAUDE.md` reference is dropped (not re-created).
- **D-04:** **Every state prompt's "Orient" step** is rewritten to the JIT protocol: *start at root `AGENTS.md` → follow only links matching the issue's acceptance criteria → grep/glob for the rest → never blanket-read `docs/`.* Applies to all state prompts in **both** `docker/WORKFLOW.md` and `docker/WORKFLOW.example.md` (Todo, In Progress, In Review, and the fallback "What to do" at `docker/WORKFLOW.md:499` / `docker/WORKFLOW.example.md:737`).

### Dogfood Scope
- **D-05:** **Full dogfood** — author real `AGENTS.md` nodes for the sinfonia repo itself: root + each crate (`crates/sinfonia`, `crates/sinfonia-bridge`, `crates/sinfonia-tracker`) + key top-level dirs (`docker/`, `docs/`). Each node renders **<1 screen** and validates against the contract. This proves the contract and satisfies SC "a sample node validates."
- **D-06:** Templates still ship for downstream repos: `templates/AGENTS.md` (with module-ownership table) + `templates/CODEOWNERS`.

### Node Placement & Ownership
- **D-07:** Node granularity = **per-crate + key top-level dirs** (see D-05). `AGENTS.md` is placed at each node's directory root.
- **D-08:** The module-ownership table keys **path-globs → owners**, consistent with `.github/CODEOWNERS`. The dogfood adds CODEOWNERS entries for the node paths so the write protocol (diffs ride the code PR under CODEOWNERS) is real, not aspirational.

### Boundary Notes (recorded to prevent scope drift — not open questions)
- Validation in Phase 5 is a **documented manual check** against the contract's conformance checklist. The automated stale-node + overlap linters are **CTXGRAPH-04 / Phase 6** — do not build them here, but node front-matter (`last_verified_sha`) must anticipate the stale linter.
- **No autonomous doc generation / self-learning** (DEC-004): all nodes are authored as reviewed surgical diffs.

### Claude's Discretion
- Exact wording and section number of the HARNESS-SPEC §7 entry and the conformance-checklist line.
- Internal layout of `templates/AGENTS.md` (as long as it is one-screen, carries the module-ownership table, and matches the contract's front-matter).
- The "See also" cross-links each dogfood node carries, and the precise per-crate node copy.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Source analyses (drove this phase)
- `.planning/intel/ingest-sources/sinfonia-harness-action-plan.md` §1.6, §2.1, §3.1, §3.2 — origin of the JIT read protocol and the Context Contract
- `.planning/intel/ingest-sources/harness-improvement-analysis.md` — doc-graph + Context Contract rationale

### Spec & docs to author/amend
- `docs/CONTEXT-CONTRACT.md` — **NEW**, to be authored (the contract / "map")
- `docs/HARNESS-SPEC.md` §7 (repo conventions) + §9-style conformance checklist — add the Context Contract convention + checklist line (reference only, not the contract body)

### Existing assets touched
- `.github/CODEOWNERS` — ownership source the module-ownership tables mirror; extend for the new node paths
- `docker/WORKFLOW.md:499` and `docker/WORKFLOW.example.md:737` — the "Orient: README.md, CLAUDE.md, docs/" slurp to replace; all state prompts (Todo, In Progress, In Review, fallback)
- `README.md` — stays human-facing root doc (not a graph node)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `.github/CODEOWNERS` (3 lines) — basis/seed for the module-ownership tables and the per-node CODEOWNERS additions.
- `docker/WORKFLOW.md` / `docker/WORKFLOW.example.md` state prompts — the single place the JIT read protocol lands. Phase 4 already edited these heavily; a consistent prompt-editing pattern is established.

### Established Patterns
- HARNESS-SPEC §7 conventions + §9 conformance checklist — the existing, proven mechanism for adding a REQUIRED/RECOMMENDED convention with a checklist line (Phase 4 split a §9 item the same way).
- DEC-004: reviewed surgical doc-diffs ride the code PR; no autonomous mutation.

### Integration Points
- Root `AGENTS.md` ← entry for every state prompt's read protocol (D-04).
- Node front-matter `last_verified_sha` ← consumed by the Phase 6 stale linter (CTXGRAPH-04); the contract's node shape must anticipate that.

</code_context>

<specifics>
## Specific Ideas

- DEC-004 wording is the design north star: **"separate the sensor (HARNESS-SPEC) from the map (Repository Context Contract)."** The contract is its own doc; HARNESS-SPEC only points to it.

</specifics>

<deferred>
## Deferred Ideas

- Automated stale-node + overlap invariant linters (CTXGRAPH-04) — Phase 6.
- Decomposition consistency pass (CTXGRAPH-05) — Phase 6.
- Overlap-before-build check (CTXGRAPH-01) — Phase 6.

None of these were scope creep from the user — they are the already-roadmapped Phase 6 work, noted here so Phase 5 stays bounded to the contract + protocol + dogfood.

</deferred>

---

*Phase: 05-repository-context-contract*
*Context gathered: 2026-06-14*
