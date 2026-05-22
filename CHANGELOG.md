# Changelog

All notable changes to Sinfonia are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0-alpha.4] — 2026-05-22

Re-publish of `[0.3.0-alpha.3]`. The Rust toolchain bump in alpha.3 fixed the cargo build, but the Docker publish pipeline still failed on a second, independent layer: the upstream Codex CLI install script (`https://github.com/openai/codex/releases/latest/download/install.sh`) errors with `Could not find SHA-256 digest for codex-package-x86_64-unknown-linux-musl.tar.gz in codex-package_SHA256SUMS` when invoked from inside the slim builder. The same `awk` lookup against the same file succeeds locally and resolves the expected digest cleanly — the failure is reproducible inside the `debian:bookworm-slim`-based build environment and resists remote debugging.

### Changed

- **Skip upstream install scripts for Codex and OpenCode in the Docker images.** Both `sinfonia-with-codex` and `sinfonia-with-opencode` (and the combined `sinfonia-all-agents`) now download the bare-binary tarballs directly (`codex-${triple}.tar.gz` from `openai/codex` releases; `opencode-linux-${arch}.tar.gz` from `sst/opencode` releases), extract, install to `/usr/local/bin/`, and self-verify with a `--version` smoke. Versions are pinned via Dockerfile `ARG CODEX_VERSION=rust-v0.133.0` / `ARG OPENCODE_VERSION=v1.15.9`; bump on each release after confirming the upstream tarballs exist for both linux/amd64 + linux/arm64. Bypasses the install scripts' SHA256SUMS-verification dance entirely. `curl --retry 3 --retry-delay 5` provides transient-network resilience. The pinned versions are operator-overridable via `docker buildx bake --set sinfonia-with-codex.args.CODEX_VERSION=<tag>`.
- **`docker-bake.hcl` comments updated** — both pinned upstreams now ship arm64-linux binaries, so the prior "MAY not publish linux/arm64" caveat is removed from the `PLATFORMS` and per-target comments.

## [0.3.0-alpha.3] — 2026-05-22

Re-publish of `[0.3.0-alpha.2]` with the toolchain fix below. The
`v0.3.0-alpha.2` tag and GitHub Release remain visible as a "did not
produce artifacts" marker — the Docker publish workflow failed on that
tag because the pinned `rust:1.78-bookworm` builder image is two stable
releases behind what current transitive dependencies need.

### Changed

- **Bump workspace MSRV from `1.78` to `1.88`.** Two stacking
  transitive-dep constraints surfaced when the Docker publish pipeline
  rebuilt against current `Cargo.lock` on the v0.3.0-alpha.2 tag:
  `hashbrown 0.17.1` requires the `edition2024` Cargo feature
  (stabilized in Rust 1.85), and `tonic 0.14` + `time-macros 0.2.27`
  require Rust 1.88. `Dockerfile` updated from `rust:1.78-bookworm` to
  `rust:1.88-bookworm`; `Cargo.toml`'s `workspace.package.rust-version`
  bumped to match; README MSRV badge and prerequisites line updated.
  `Dockerfile.dev` is unaffected — it installs via `rustup` and tracks
  stable. Existing v0.2 / v0.3.0-alpha.1 deployments running compiled
  binaries are unaffected.

## [0.3.0-alpha.2] — 2026-05-22

Second v0.3 preview. Makes Sinfonia legible as a team-grade orchestrator alongside its existing single-user shape. Six additions over `[0.3.0-alpha.1]`, all opt-in: OpenTelemetry emission tenant-tagged from day one (Phase 3), the Jira bridge write surface (Phase 4), six setup skills + `sinfonia --check` / `sinfonia init` CLIs for AI-coding-tool-driven scaffolding (Phase 5), six published Docker images (Phase 6), and the finalized doc set with `docs/DEPLOYMENT.md` + `docs/CLIENT_SETUP.md` + `docs/MIGRATION-v0.2-to-v0.3.md` (Phase 7). The daemon's behaviour against an unchanged v0.2 `WORKFLOW.md` is preserved — see [`docs/MIGRATION-v0.2-to-v0.3.md`](docs/MIGRATION-v0.2-to-v0.3.md). v0.3.0 GA waits on a manual readthrough of the doc set and the carried-forward manual-verification debts (Linear bridge end-to-end, OpenCode + Linear, Collector + Postgres cap-hit cycle, real Jira sandbox).

### Added

- **Finalized documentation (Phase 7).** SPEC §11.5 tightened ("orchestrator MUST NOT write"; pointer to §11.6) and §11.6 promoted from Draft to Recommended Extension; new §11.7 documents custom-field discovery per tracker (Linear marker-comment vs Jira `customfield_NNNNN` resolution); §18.2 grew bullets for the Jira tracker adapter, the CI feedback bridge, failure categorization, budget enforcement, and PR label management (alongside the OpenCode + OpenTelemetry + setup-skills entries from earlier phases). New guides: `docs/DEPLOYMENT.md` (four topologies + credential model + scaling + backup + upgrading), `docs/CLIENT_SETUP.md` (enterprise adoption checklist with trust-boundary diagram, security posture, budget controls, audit-trail queries, handoff runbook headers, and a vendor-evaluation worksheet), `docs/MIGRATION-v0.2-to-v0.3.md` (required + optional + breaking-changes sections). `WORKFLOW.example.md` gains a `telemetry:` block walkthrough, three OpenCode usage variants (default-lane / state-machine / air-gapped Ollama-with-LSP), and a full failure-categorization state-machine example wired to `BRIDGE.md`'s `feedback_loop.failure_categories`. `BRIDGE.example.md` budget-caps section ships realistic non-null example values; new cost_table_path override block; cross-link block at the bottom. `CONTRIBUTING.md` updated for the Cargo workspace layout (three-crate table; `--workspace` / `-p crate` commands; "where to add new code" guidance). Docs CI: link-rot via lychee (internal-only on PR, full-link weekly), Markdown lint via markdownlint-cli2, fenced YAML/TOML/JSON code-block syntax checks.

- **Docker images + production compose (Phase 6).** Six production images publish to `ghcr.io/o-side-systems/` from a single multi-stage `Dockerfile` driven by `docker-bake.hcl`: `sinfonia` (daemon only, Debian slim + bash/git/curl/gh), `sinfonia-bridge` (bridge only, no agent or git tooling), three single-agent variants (`sinfonia-with-claude-code` adds Node 22 + `@anthropic-ai/claude-code`; `sinfonia-with-codex` and `sinfonia-with-opencode` install the respective CLIs via upstream install scripts), and `sinfonia-all-agents` for state-machine deployments that route across agents. Each image is tagged `:VERSION`, `:VERSION_MINOR`, and `:latest`; built for `linux/amd64` and `linux/arm64` where the underlying CLI provides arm64. The build stage shares one `cargo build --release` across all six targets via BuildKit registry + target cache mounts. The new root `docker-compose.yml` demonstrates a production topology (daemon + bridge + OTel Collector + Postgres) with read-only mounts for the per-agent credential directories (`~/.claude`, `~/.codex`, `~/.opencode`) and the Phase 3 telemetry schema applied to Postgres via `docker-entrypoint-initdb.d`. The pre-existing dev-shell Dockerfile + compose move to `Dockerfile.dev` / `docker-compose.dev.yml` with their behaviour unchanged. Publishing runs through `.github/workflows/docker-publish.yml` on every `v*` tag — `docker buildx bake --push`, image-digest inspection, image / compose smoke tests, then a per-image Trivy scan with `severity: CRITICAL,HIGH` and `exit-code: 1` (CRITICAL/HIGH CVEs fail the publish; LOW/MEDIUM upload SARIF to the Security tab without blocking).

- **Compose smoke harness.** `tests/docker-compose-smoke.sh` brings the full production compose stack up with the `docker-compose.ci.yml` overlay (which strips the user-credential bind mounts that don't exist in CI and swaps in `tests/fixtures/WORKFLOW.smoke.md` + `tests/fixtures/BRIDGE.smoke.md`), polls `http://localhost:8080/api/v1/state` and `http://localhost:8081/health` on a bounded 30-second retry, and tears the stack back down. The `!reset` Compose-spec tag clears the inherited volume list so the overlay merges cleanly on Compose v2.24+. `tests/docker-smoke.sh` is the lighter per-image variant — `docker run --rm <img> --help` against all six images plus `sinfonia --check WORKFLOW.example.md` against the daemon image.

- **`docker-bake.hcl`** as the single source of truth for the production image matrix — targets, registry, platforms, and the `tags(name)` helper that fans `${VERSION}` out into the three-tag set per image.

- **Setup skills + CLI extensions (Phase 5).** Six setup skills ship at `skills/` in the repo root: `setup-workflow`, `setup-bridge`, `setup-state-machine`, `setup-telemetry`, `setup-agent-backend`, `migrate-from-symphony`. Each is a self-contained folder with a `SKILL.md` runbook (YAML front-matter format with `name`/`description`/`version` keys), Liquid templates for the artifacts the skill produces (WORKFLOW.md, BRIDGE.md, docker-compose snippets, GitHub Actions workflow, per-state prompts, per-backend agent blocks), and optional shell-script validators wrapping the CLI gates. AI tools (Claude Code, OpenCode, Codex) invoke the skills directly; humans can read each `SKILL.md` as a runbook. `docs/SKILLS.md` is the cross-vendor pointer table. Phase 5 §1 distribution model is locked: Sinfonia ships the skills; auto-install into AI-tool-specific directories is owned by each tool vendor.

- **`sinfonia --check <WORKFLOW.md>`** (Phase 5 §3.1). Validates a workflow without running. Loads YAML, runs `validate_for_dispatch`, then renders every prompt template (workflow body + per-state overrides) against a stub Issue to catch strict-Liquid errors before the operator hits "go." Exit codes: `0` ok / `2` YAML parse / `3` schema / `4` template / `5` tracker auth — skills branch on the exit code to give specific remediation prompts.

- **`sinfonia init`** (Phase 5 §3.2). Interactive `inquire`-driven REPL that scaffolds a `WORKFLOW.md`. The AI-tool-free equivalent of the `setup-workflow` skill. Walks tracker selection, project slug, endpoint/email when applicable, active/terminal states, default agent backend, and workspace root. Linear flow with abort-on-error (re-run to start over).

- **State-machine prompt invariant.** `skills/setup-state-machine/templates/*.liquid` enforce the strict-Liquid contract by construction — every `{{ issue.fields.X }}` reference is followed by `| default: "…"`. The `state_machine_prompts_have_no_unguarded_issue_fields` integration test greps for unguarded references and fails CI on a hit. A human can drag a ticket into Needs Fixes without any prior bridge run; the prompt renders cleanly because the `| default:` filter handles absent fields.

- **`docs/SKILLS.md`** with the cross-vendor pointer table for skill installation (Claude Code, OpenCode, Codex CLI) and the skill-contract documentation (front-matter keys + strict-Liquid invariant).

- **Jira bridge support (Phase 4).** The five `IssueTracker` bridge-write methods (`transition_issue`, `read_custom_field`, `write_custom_field`, `ensure_custom_field`, `post_comment`) are implemented for `JiraTracker` against the Atlassian Cloud REST API v3. Custom-field IDs (`customfield_NNNNN`) are resolved once per bridge-stable key and cached for the process lifetime via a `tokio::sync::RwLock<HashMap>`. State transitions go through `GET /transitions` to resolve the target state to a transition id and `POST /transitions` with that id. Comments are emitted as Atlassian Document Format (ADF) via a narrow-scope hand-rolled Markdown→ADF converter (paragraphs, fenced code blocks, lists, inline strong/em/code/link) in `crates/sinfonia-tracker/src/jira_adf.rs`. `ensure_custom_field` creates missing fields via `POST /rest/api/3/field` and attempts a best-effort screen-scheme bind so the field shows up in the Jira UI; failures log a WARN pointing to `docs/JIRA-SCREEN-SCHEME.md` for manual setup. Self-hosted Jira Server / Data Center is supported via PAT-only auth (omit `tracker.email`, put the token in `tracker.api_key` — the adapter switches from Basic to Bearer auth).

- **Bridge config validation accepts `tracker.kind: jira`** (was: rejected with "deferred to Phase 4"). Two new positive rules: `tracker.endpoint` is required for Jira (no sensible per-tenant default), and `tracker.email` is required when the endpoint matches `*.atlassian.net` (Cloud uses Basic auth with email + token). Self-hosted endpoints proceed without an email.

- **Jira self-test probe.** `sinfonia-bridge --self-test` now exercises the Jira candidate-fetch path (`POST /rest/api/3/search`) when `tracker.kind: jira` — auth + project visibility surface as the `tracker` check line (replaces the previous "unimplemented (Phase 4)" stub).

- **OpenCode coding-agent backend.** `provider: opencode` in `WORKFLOW.md` (and inside `states:` blocks) now drives the `opencode` CLI (<https://opencode.ai>) as a subprocess in the per-issue workspace, joining `claude_code` and `codex` as a sibling subprocess-driven backend. The prompt is piped on stdin, OpenCode events are read one JSON object per line from stdout (`--format json`), and the per-session ID is resumed on retry turns via `--session <id>`. Auth is owned by the `opencode` CLI itself (`opencode auth login`) — Sinfonia does NOT pass an api_key. The `model:` field is passed through verbatim with OpenCode's `provider/model` wire format (e.g. `anthropic/claude-sonnet-4-6`, `ollama/qwen2.5-coder:32b`). OpenCode adds LSP integration, MCP support, and 75+ provider backends — most notably an Ollama-with-LSP path that the raw `ollama` backend can't provide. Implementation lives in `crates/sinfonia/src/agent/opencode.rs`; the workspace gains the `which` crate as a workspace-level dependency for the preflight binary check. See `WORKFLOW.example.md`, the README backend table, and `docs/SPEC.md` §18.2 for usage.

- **OpenTelemetry emission (Phase 3).** Both binaries layer an optional OTLP exporter over the existing `tracing` subscribers. When `OTEL_EXPORTER_OTLP_ENDPOINT` is unset and no `telemetry:` block is configured, behaviour matches today — the OTel layer is `None` and the binaries run stdout-only. When configured, Sinfonia emits six spans (`orchestrator.tick`, `orchestrator.dispatch`, `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`) and the bridge emits six more (`bridge.webhook`, `bridge.ci_result`, `bridge.state_transition`, `bridge.cap_hit`, `bridge.cost_update`, `bridge.events_receive`). Every span carries the resolved `tenant_id` (precedence: `telemetry.tenant_id` → `SINFONIA_TENANT_ID` env → `"default"`); resource-level `service.namespace = tenant_id` lets a Collector routing-processor split per-tenant data without touching emission code. Crate set: `opentelemetry 0.32` / `opentelemetry_sdk 0.32` / `opentelemetry-otlp 0.32` / `tracing-opentelemetry 0.33`.

- **Typed Sinfonia↔bridge event channel.** The bridge no longer requires running an OTLP receiver (the original H-2 plan-review fix). Sinfonia POSTs typed events (`runner.session.completed`) to subscribers registered via `POST /api/v1/events/subscribers` — HMAC-SHA256 signed (header `X-Sinfonia-Signature-256`, same scheme as the GitHub webhook) with a `telemetry.sinfonia_events_secret` shared between `WORKFLOW.md` and `BRIDGE.md`. Mismatch returns HTTP 401 and the Sinfonia retry loop logs `WARN` on exhaustion. The bridge handler at `POST /api/v1/sinfonia-events` parses the body, feeds it into the budget pipeline, and (on cap-hit) transitions the ticket to `feedback_loop.budget_exceeded_state`. Diagnostic surface: `GET /api/v1/events/recent` returns the last 200 emitted events.

- **Budget enforcement.** `crates/sinfonia-bridge/src/feedback/budget.rs` adds a per-process per-ticket cost / token accumulator. Token + cost caps from `BRIDGE.md` (`max_tokens_per_ticket`, `max_cost_per_ticket_usd`) are enforced at the tracker write boundary; cap-crossings transition the ticket to `budget_exceeded_state` and write the `sinfonia_tokens_consumed` / `sinfonia_cost_consumed_usd` / `sinfonia_budget_exhausted_at` custom fields. Cost values are stringified via `rust_decimal::Decimal` per STATUS §5.1 (never f64 on the wire). A 30 s idle-debounce reconciler coalesces under-cap writes so a busy ticket emits one Linear API hit per quiet window instead of one per session.

- **Cost table** (`config/cost_table.yaml`) embedded into the bridge via `include_str!`, overridable at runtime via `bridge.cost_table_path`. Includes Anthropic, OpenAI, Google, and Ollama (zero-cost local) entries verified against provider pricing pages on 2026-05-21. Two freshness gates: `WARN` log at startup if `verified_at` is more than 90 days stale, and the M-2 plan-checker fix — the bridge refuses to apply COST caps (token caps stay enforced) when the table is more than 180 days stale.

- **`AgentEvent::SessionCompleted`** variant emitted by the runner immediately after `agent.stop_session(...)` per the N-3 plan-checker fix. Carries the per-session token totals, exit reason, and provider/model the bridge needs without re-parsing the event stream.

- **`WELL_KNOWN_FIELDS`** registry gains `sinfonia_budget_exhausted_at` so templates referencing it via `| default: …` don't trip strict-mode Liquid.

- **Reference Collector + Postgres assets** at `examples/telemetry/`:
  - `postgres-schema.sql` — sessions / attempts / events tables with the indexes the §8.2 dashboard queries expect.
  - `otel-collector-config.yaml` — receiver + routing-by-tenant processor + Postgres exporter starter.
  - `queries/*.sql` — the three reference dashboard queries: tenant monthly cost, first-try rate, top-budget tickets.
  - `README.md` — wiring guide + span / attribute reference + multi-tenant notes.

### Changed

- **`Dockerfile` and `docker-compose.yml` are now production-shaped (Phase 6).** The previous dev-shell `Dockerfile` (Node 22 + Rust + Claude Code, bind-mounted repo, `--dangerously-skip-permissions` entrypoint) moves verbatim to `Dockerfile.dev`. The previous `docker-compose.yml` (two services, both built from source) moves verbatim to `docker-compose.dev.yml` and its `build.dockerfile` value is updated to `Dockerfile.dev`. Local dev workflows continue to work — just pass `-f docker-compose.dev.yml` to compose commands.
- `TurnOutcome::Completed` now carries a `usage: TokenUsage` field so the runner aggregates session totals without re-parsing the event channel. All four implementers (`turn.rs`, `cli.rs`, `opencode.rs`) emit the same field they were already passing into `AgentEvent::TurnCompleted`.
- `Orchestrator::dispatch_one` returns a `DispatchOutcome::{Dispatched, Skipped, NoSlot}` enum instead of a boolean. `retries::tick_retries` uses the new `continue_loop()` helper to preserve its existing "no slot → requeue" semantics.
- `AppState::with_default_budget(...)` constructor added on the bridge side for tests / fixtures — production wires the `BudgetManager` explicitly so the embedded cost table can be overridden via `bridge.cost_table_path`.

### Deferred to v0.3.1

The 9-instrument OTel metrics layer (`sinfonia.agent.tokens_total`, `bridge.ci_outcome`, etc.) is deferred. The reference dashboard SQL in `examples/telemetry/queries/*.sql` reads from span attributes via the `events` table, not from OTel metric points, so the dashboards work span-derived. Filing this here so a future maintainer knows the metrics layer was a deliberate scope cut, not an oversight. The bridge `--once` single-shot mode described as one of the Topology 4 options in `docs/DEPLOYMENT.md` is also a v0.3.1 candidate — as of v0.3.0 the bridge always runs as a server. The four-topology guide in `docs/DEPLOYMENT.md` documents a working v0.3.0 alternative (POST to the existing `/webhook` handler from within the Action, then kill the bridge).

### Migration

- See [`docs/MIGRATION-v0.2-to-v0.3.md`](docs/MIGRATION-v0.2-to-v0.3.md).

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

[Unreleased]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.4...HEAD
[0.3.0-alpha.4]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.3...v0.3.0-alpha.4
[0.3.0-alpha.3]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.2...v0.3.0-alpha.3
[0.3.0-alpha.2]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.1...v0.3.0-alpha.2
[0.3.0-alpha.1]: https://github.com/O-Side-Systems/sinfonia/compare/v0.1.0...v0.3.0-alpha.1
[0.1.0]: https://github.com/O-Side-Systems/sinfonia/releases/tag/v0.1.0
