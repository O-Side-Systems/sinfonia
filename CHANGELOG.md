# Changelog

All notable changes to Sinfonia are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — 2026-05-16

Initial public release.

### Added

- Rust implementation of the Symphony Service Specification (Draft v1, `docs/SPEC.md`):
  - `WORKFLOW.md` loader with YAML front matter + Liquid prompt body and `$VAR` resolution.
  - Single-authority orchestrator with poll loop, dispatch, reconciliation, exponential retries, continuation retries, and stall detection.
  - Per-issue workspace manager with sanitized identifiers, lifecycle hooks (`after_create`, `before_run`, `after_run`, `before_remove`), and root-containment safety invariants.
  - Strict prompt templating with `issue` + `attempt` variables.
  - Structured logs with `issue_id` / `issue_identifier` / `session_id` context.
  - Dynamic `WORKFLOW.md` reload via filesystem watcher.
- Issue tracker adapters:
  - **Linear** (GraphQL, paginated, blocker normalization from `inverseRelations`).
  - **Jira** (Cloud + self-hosted, REST + JQL, Basic-or-Bearer auth, "is blocked by" link normalization).
- Coding-agent backends:
  - **Raw LLM** with built-in tool loop (`shell`, `read_file`, `write_file`, `edit_file`, `list_dir`, `finish`) targeting OpenAI, Anthropic, Google Gemini, and locally hosted Ollama.
  - **CLI subprocess** drivers for Anthropic's `claude` (Claude Code) and OpenAI's `codex` (Codex CLI), with session resume via `--resume` / `--thread`.
- Configurable per-state runner overrides (`states:` block in `WORKFLOW.md`). Each tracker state can route to a different provider, model, command, prompt, temperature, and turn timeout.
- Optional HTTP server (axum): dashboard at `/`, JSON API at `/api/v1/state`, `/api/v1/<issue_identifier>`, `POST /api/v1/refresh`. Loopback bind by default.
- CLI: positional `WORKFLOW.md`, `--port`, `--log-format pretty|json`.

### Known limitations

- Retry queue and session metadata are in-memory only and do not survive process restart (per spec §14.3).
- The `linear_graphql` client-side tool is wired on the tracker trait but not exposed in the agent tool catalog yet.
- The Codex app-server stdio protocol backend is stubbed; this release targets the `codex exec` CLI surface instead.
- One project per running daemon. Multi-project deployments use one daemon per project.

[Unreleased]: https://github.com/O-Side-Systems/sinfonia/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/O-Side-Systems/sinfonia/releases/tag/v0.1.0
