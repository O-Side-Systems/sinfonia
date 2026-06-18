# Architecture standards + ADRs

> WHY the system is shaped the way it is. Two things live here: durable
> architecture **standards** (the rules below) and **ADRs** — one file per
> Architecture Decision Record, recording a decision and its rationale.

## Standards

- **Boundaries / layering:** <!-- replace: allowed dependency directions, module boundaries -->
- **Cross-cutting rules:** <!-- replace: how config / logging / auth / errors are accessed -->
- **Banned dependencies or patterns:** <!-- replace, or "none" -->

These are the structural invariants the agent must not violate. Where possible,
enforce them with a linter that fails loud in CI (see `docs/HARNESS-SPEC.md §5.5`).

## ADRs

Record every architecture or cross-cutting decision as a numbered ADR next to this
file, using [`0000-adr-template.md`](0000-adr-template.md). ADRs are append-only
history: supersede, don't rewrite.

| # | Title | Status |
|---|-------|--------|
| <!-- 0001 --> | <!-- replace --> | <!-- Proposed / Accepted / Superseded --> |
