# Phase 5: Repository Context Contract - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-06-14
**Phase:** 05-repository-context-contract
**Areas discussed:** Contract home, AGENTS.md vs CLAUDE.md (root entry point), Dogfood vs templates-only, Node placement & ownership

---

## Contract home

| Option | Description | Selected |
|--------|-------------|----------|
| Standalone doc + §7 line | `docs/CONTEXT-CONTRACT.md` is the map; HARNESS-SPEC §7 adds a reference + conformance line. Matches DEC-004. | ✓ |
| New HARNESS-SPEC §N | Contract is a normative section inside HARNESS-SPEC; §7 cross-refs it. | |
| Hybrid | Normative text in §N + a short pointer doc. | |

**User's choice:** Standalone doc + §7 line (recommended)
**Notes:** Honors DEC-004 — separate the sensor (HARNESS-SPEC) from the map (Context Contract).

---

## Root entry point (AGENTS.md vs CLAUDE.md)

| Option | Description | Selected |
|--------|-------------|----------|
| Root AGENTS.md is the entry point | Root `AGENTS.md` is the single entry point; prompts' Orient → JIT protocol; drop stale CLAUDE.md ref + blanket docs/ read; README stays human-facing. | ✓ |
| Keep CLAUDE.md as root, AGENTS.md below | CLAUDE.md remains root agent doc (but doesn't currently exist at root). | |
| Root AGENTS.md + CLAUDE.md pointer | AGENTS.md canonical + thin CLAUDE.md pointer for tools. | |

**User's choice:** Root AGENTS.md is the entry point (recommended)
**Notes:** The current "Orient: README.md, CLAUDE.md, docs/" references a CLAUDE.md that doesn't exist and slurps all of docs/ — exactly the anti-pattern this phase removes.

---

## Dogfood vs templates-only

| Option | Description | Selected |
|--------|-------------|----------|
| Light dogfood | Templates + contract + real root AGENTS.md + one per-crate sample node. | |
| Templates-only | Templates + contract + standalone sample node (not wired into repo). | |
| Full dogfood | Root AGENTS.md + real nodes for all 3 crates + docker/ + docs/. | ✓ |

**User's choice:** Full dogfood
**Notes:** Chose the most complete option (above the recommended "light dogfood"). Aligns with the per-crate + key-dirs granularity selected in the next area.

---

## Node placement & ownership

| Option | Description | Selected |
|--------|-------------|----------|
| Per-crate + key dirs, glob→owner | Nodes at each crate + docker/ + docs/; ownership table keys path-globs → owners mirroring .github/CODEOWNERS. | ✓ |
| Per-crate only | Nodes only at the three crates. | |
| Flat root table | Single root AGENTS.md table, no nested nodes yet. | |

**User's choice:** Per-crate + key dirs, glob→owner (recommended)
**Notes:** Consistent with the Full-dogfood scope; each node kept one-screen.

---

## Claude's Discretion

- Exact wording/section number of the HARNESS-SPEC §7 entry and conformance-checklist line.
- Internal layout of `templates/AGENTS.md` (one-screen, module-ownership table, contract front-matter).
- Per-node "See also" cross-links and per-crate node copy.

## Deferred Ideas

- Automated stale-node + overlap linters (CTXGRAPH-04) — Phase 6.
- Decomposition consistency pass (CTXGRAPH-05) — Phase 6.
- Overlap-before-build check (CTXGRAPH-01) — Phase 6.
