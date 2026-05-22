# Changelog

All notable changes to Sinfonia are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **OpenCode coding-agent backend.** `provider: opencode` in `WORKFLOW.md` (and inside `states:` blocks) now drives the `opencode` CLI (<https://opencode.ai>) as a subprocess in the per-issue workspace, joining `claude_code` and `codex` as a sibling subprocess-driven backend. The prompt is piped on stdin, OpenCode events are read one JSON object per line from stdout (`--format json`), and the per-session ID is resumed on retry turns via `--session <id>`. Auth is owned by the `opencode` CLI itself (`opencode auth login`) — Sinfonia does NOT pass an api_key. The `model:` field is passed through verbatim with OpenCode's `provider/model` wire format (e.g. `anthropic/claude-sonnet-4-6`, `ollama/qwen2.5-coder:32b`). OpenCode adds LSP integration, MCP support, and 75+ provider backends — most notably an Ollama-with-LSP path that the raw `ollama` backend can't provide. Implementation lives in `crates/sinfonia/src/agent/opencode.rs`; the workspace gains the `which` crate as a workspace-level dependency for the preflight binary check. See `WORKFLOW.example.md`, the README backend table, and `docs/SPEC.md` §18.2 for usage.

## [0.3.0-alpha.1] — 2026-05-21

First v0.3 preview. Adds the `sinfonia-bridge` binary alongside the existing daemon; the daemon's behaviour is unchanged.

### Added

- **Workspace conversion.** The single-crate layout is now a Cargo workspace with three members:
  - `crates/sinfonia/` — the daemon (unchanged in behaviour).
  - `crates/sinfonia-tracker/` — the shared `IssueTracker` trait, Linear and Jira adapters, and the new `custom_fields` module.
  - `crates/sinfonia-bridge/` — the new bridge binary.
- **Custom-field plumbing** (`sinfonia-tracker::custom_fields`):
  - `CustomFieldValue` enum (`Null` / `Number` / `String`) with hand-written `Serialize` so values flatten to JSON primitives in the Liquid template scope.
  - `MARKER = "sinfonia_bridge_state_v1"` sentinel for the bridge's per-ticket envelope (`docs/SPEC.md` §11.6).
  - `WELL_KNOWN_FIELDS` registry consumed by `crates/sinfonia/src/template.rs` to pre-seed missing keys as `Null`, so templates using `{{ issue.fields.X | default: "…" }}` no longer trip strict-mode "Unknown index" errors.
  - `IssueTracker` gains five bridge-write methods: `ensure_custom_field`, `write_custom_field`, `transition_to_state`, `add_comment`, `apply_labels`. Linear implementations land in this release; Jira returns `NotImplemented` until a later milestone.
- **`Issue.fields`** map populated by the Linear adapter from the bridge's marker comment (single GraphQL hop via `comments(first: 100)`).
- **New `sinfonia-bridge` binary** (`crates/sinfonia-bridge/`):
  - `BRIDGE.md` config file (YAML front matter, mirrors `WORKFLOW.md` style) with a strict parser, nine validation rules, and a `--check` flag for config-only verification.
  - `POST /webhook` endpoint with HMAC-SHA256 signature verification (constant-time compare), SQLite-backed delivery-ID idempotency, and dispatch on `pull_request` / `check_suite` / `workflow_run`.
  - Feedback-loop orchestrator (`feedback::evaluate_ci`): categorizes failed checks, increments per-ticket attempt counters, routes to category-specific "needs fixes" states, applies the attempt cap, and posts a Liquid-rendered failure comment to the PR.
  - PR label management (`labels::LabelManager`): six canonical labels under a configurable prefix, with verbatim-alias semantics for installs that already have a competing label scheme.
  - GitHub authentication via either Personal Access Token or GitHub App (per-owner installation-scoped client cache); both modes exercised by integration tests.
  - `sinfonia-bridge --self-test` install gate: serial `PASS` / `FAIL` / `SKIP` lines per check, exit code = number of `FAIL` lines.
- **Tests.** The bridge crate ships 89 unit tests (config validation, webhook verify, storage, feedback loop, labels, GitHub auth, self-test, config round-trips) plus 9 `wiremock`-backed integration tests in `tests/bridge_e2e.rs` covering all nine scenarios from the Phase 1 plan §9.2 end-to-end. Workspace test count: 149 passing.
- **New docs.**
  - `BRIDGE.example.md` at the repo root — fully-commented working config, validated by `sinfonia-bridge BRIDGE.example.md --check` with no environment variables set.
  - `docs/SPEC.md` §11.6 — draft of the recommended bridge-service extension contract.

### Changed

- `LinearTracker::new` / `JiraTracker::new` now take a `&TrackerConfig` instead of `&ServiceConfig`. Existing callers go through `crates/sinfonia/src/tracker.rs`, so no migration is needed.
- `sinfonia::Error` gains a `Tracker` variant (`#[from] sinfonia_tracker::Error`); direct constructors of formerly-bare variants in `crates/sinfonia/src/config/typed.rs` now route through the wrap.

### Known limitations

- Phase 1 supports Linear only on the bridge side. `tracker.kind: jira` in `BRIDGE.md` is rejected at startup with a friendly "deferred to a later milestone" message.
- Budget caps (`max_tokens_per_ticket`, `max_cost_per_ticket_usd`) and the `telemetry.otlp_*` fields are accepted by the parser but unused in this release — they are scoped to a later milestone.
- The bridge does not hot-reload `BRIDGE.md`; configuration changes require a process restart.
- Linear marker comments are fetched via `comments(first: 100)`; tickets with more than 100 bot interactions may scroll the marker out of the window. See `docs/SPEC.md` §11.6.7 for RECOMMENDED mitigations.

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

[Unreleased]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.1...HEAD
[0.3.0-alpha.1]: https://github.com/O-Side-Systems/sinfonia/compare/v0.1.0...v0.3.0-alpha.1
[0.1.0]: https://github.com/O-Side-Systems/sinfonia/releases/tag/v0.1.0
