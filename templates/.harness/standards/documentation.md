# Documentation standards

> WHAT gets documented, and WHERE, when a change lands. The Plan gate
> ([`../criteria/plan.md`](../criteria/plan.md)) requires the plan to name the docs
> a change will touch; this file says how.

## What to update with a change

- **User-facing behavior change** → <!-- replace: README / docs site / changelog -->
- **New or changed public API** → <!-- replace -->
- **Architecture or cross-cutting decision** → an ADR under
  [`architecture/`](architecture/), plus the `AGENTS.md` node for the area.
- **A learning worth carrying forward** → an entry in
  [`../knowledge/`](../knowledge/) (see [`compounding.md`](compounding.md)).

## Style

- <!-- replace: tone, line length, link conventions, diagram tooling -->

## Where docs live

| Kind | Location |
|------|----------|
| Product / usage | <!-- replace --> |
| Architecture + ADRs | `.harness/standards/architecture/` |
| Agent context (the map) | `AGENTS.md` doc-graph |
| Compounded learnings | `.harness/knowledge/` |
