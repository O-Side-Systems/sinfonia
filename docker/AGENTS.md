---
title: "docker — Docker Topology + WORKFLOW Templates"
version: "1.1.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "b0c7217"
derived_from: "docs/HARNESS-SPEC.md §7.2, docs/SPEC.md §5, §6"
---

# docker — Docker Topology + WORKFLOW Templates

Runtime delivery layer: the annotated WORKFLOW example (committed) and the container
entrypoint. The operator's live `docker/WORKFLOW.md` is **per-machine and gitignored**
(`.gitignore`) — it is NOT a committed artifact; edit `WORKFLOW.example.md` and copy it.
The fallback orient step in `WORKFLOW.example.md` uses the JIT read protocol
(CONTEXT-CONTRACT §5) — do not revert it to the deprecated `README.md, CLAUDE.md,
docs/` blanket-slurp. The example's state prompts reference the target repo's
`.harness/` workspace (HARNESS-SPEC §11) for the standards/criteria an agent reads
and the `knowledge/` it writes back.

## Module Ownership

| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
| Annotated WORKFLOW example (the committed template) | `docker/WORKFLOW.example.md` | @osidesys @leebrett | Full per-state machine example; Liquid prompt + YAML front matter. The live `WORKFLOW.md` is operator-local/gitignored. |
| Container entrypoint script | `docker/entrypoint.sh` | @osidesys @leebrett | Sets env, exec sinfonia binary |

## See also

- [`docs/HARNESS-SPEC.md §7.2`](../docs/HARNESS-SPEC.md) — repo conventions the template must follow
- [`docs/SPEC.md §5, §6`](../docs/SPEC.md) — WORKFLOW.md loading + rendering contract
- [`docs/CONTEXT-CONTRACT.md §5`](../docs/CONTEXT-CONTRACT.md) — JIT read protocol
- [`../AGENTS.md`](../AGENTS.md) — root entry point
