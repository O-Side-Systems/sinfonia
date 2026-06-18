---
title: "sinfonia — Orchestrator + Agent Runners"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "b26bc50"
derived_from: "docs/SPEC.md §7, §8, §10"
---

# sinfonia — Orchestrator + Agent Runners

Core daemon: single-authority state machine that polls the tracker, dispatches
per-issue workspaces, and drives coding-agent sessions. Do not edit
`orchestrator/dispatch.rs` without also checking `orchestrator/state.rs` —
they share the candidate-eligibility contract (SPEC §8.2).

## Module Ownership

| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
| Candidate dispatch + eligibility | `crates/sinfonia/src/orchestrator/dispatch.rs` | @osidesys @leebrett | Coupled to state.rs (§8.2) |
| State machine + retry queue | `crates/sinfonia/src/orchestrator/state.rs` | @osidesys @leebrett | Single authoritative in-memory FSM |
| Per-session runner loop | `crates/sinfonia/src/orchestrator/runner.rs` | @osidesys @leebrett | Thread per issue |
| Backoff / retry scheduling | `crates/sinfonia/src/orchestrator/retries.rs` | @osidesys @leebrett | Exponential backoff §8 |
| Agent backends (OpenAI/Anthropic/CLI) | `crates/sinfonia/src/agent/` | @osidesys @leebrett | See mod.rs for trait |
| WORKFLOW.md config + watcher | `crates/sinfonia/src/config/` | @osidesys @leebrett | Dynamic reload §5, §6 |
| Workspace lifecycle + hooks | `crates/sinfonia/src/workspace/` | @osidesys @leebrett | Per-issue dirs §9 |
| HTTP dashboard + API | `crates/sinfonia/src/http/` | @osidesys @leebrett | /api/v1/* §13.7 |

## See also

- [`docs/SPEC.md §7–§10`](../../docs/SPEC.md) — state machine, dispatch, workspace, agent protocol
- [`../../AGENTS.md`](../../AGENTS.md) — root entry point
