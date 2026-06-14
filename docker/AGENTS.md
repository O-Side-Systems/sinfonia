---
title: "docker — Docker Topology + WORKFLOW Templates"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "b26bc50"
derived_from: "docs/HARNESS-SPEC.md §7.2, docs/SPEC.md §5, §6"
---

# docker — Docker Topology + WORKFLOW Templates

Runtime delivery layer: the production WORKFLOW template, the annotated example,
and the container entrypoint. The fallback orient step in WORKFLOW.md and
WORKFLOW.example.md uses the JIT read protocol (CONTEXT-CONTRACT §5) — do not
revert it to the deprecated `README.md, CLAUDE.md, docs/` blanket-slurp.

## Module Ownership

| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
| Default WORKFLOW template | `docker/WORKFLOW.md` | @osidesys @leebrett | Liquid prompt + YAML front matter |
| Annotated WORKFLOW example | `docker/WORKFLOW.example.md` | @osidesys @leebrett | Full per-state machine example |
| Container entrypoint script | `docker/entrypoint.sh` | @osidesys @leebrett | Sets env, exec sinfonia binary |

## See also

- [`docs/HARNESS-SPEC.md §7.2`](../docs/HARNESS-SPEC.md) — repo conventions the template must follow
- [`docs/SPEC.md §5, §6`](../docs/SPEC.md) — WORKFLOW.md loading + rendering contract
- [`docs/CONTEXT-CONTRACT.md §5`](../docs/CONTEXT-CONTRACT.md) — JIT read protocol
- [`../AGENTS.md`](../AGENTS.md) — root entry point
