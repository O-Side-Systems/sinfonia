---
title: "Sinfonia — Repository Agent Entry Point"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "b26bc50"
derived_from: "README.md, docs/SPEC.md"
---

# Sinfonia — Agent Context

Sinfonia is a polling orchestrator that turns Linear/Jira issues into agent sessions.
Three Rust crates: `sinfonia` (orchestrator + agent runners), `sinfonia-bridge`
(CI-feedback loop), `sinfonia-tracker` (Linear/Jira adapters).

## Module Ownership

| Capability | Path-glob | Owned By | Don't Roll Your Own |
|------------|-----------|----------|---------------------|
| Issue dispatch / orchestration | `crates/sinfonia/src/orchestrator/` | @osidesys @leebrett | See [crates/sinfonia/AGENTS.md](crates/sinfonia/AGENTS.md) |
| Agent runners (Anthropic/OpenAI/CLI) | `crates/sinfonia/src/agent/` | @osidesys @leebrett | See [crates/sinfonia/AGENTS.md](crates/sinfonia/AGENTS.md) |
| CI-to-issue feedback bridge | `crates/sinfonia-bridge/src/` | @osidesys @leebrett | See [crates/sinfonia-bridge/AGENTS.md](crates/sinfonia-bridge/AGENTS.md) |
| Linear/Jira adapters | `crates/sinfonia-tracker/src/` | @osidesys @leebrett | See [crates/sinfonia-tracker/AGENTS.md](crates/sinfonia-tracker/AGENTS.md) |
| Docker topology + WORKFLOW templates | `docker/` | @osidesys @leebrett | See [docker/AGENTS.md](docker/AGENTS.md) |
| Spec docs + proposals | `docs/` | @osidesys @leebrett | See [docs/AGENTS.md](docs/AGENTS.md) |

## See also

- [`docs/SPEC.md`](docs/SPEC.md) — orchestrator + bridge contract
- [`docs/HARNESS-SPEC.md`](docs/HARNESS-SPEC.md) — harness authoring spec
- [`docs/CONTEXT-CONTRACT.md`](docs/CONTEXT-CONTRACT.md) — this doc-graph's contract
