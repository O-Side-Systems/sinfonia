# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-22 (P7 landed on branch — `v0.3-phase-7-docs`; Phase 7 ready for review; v0.3.0 release is the next gate)
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the **complete Phase 1, Phase 2, and Phase 3 of v0.3**. Phase 1's nine sub-tasks (P1-A through P1-I) ship as v0.3.0-alpha.1: the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix), the bridge skeleton (#3 — P1-D), the webhook layer (#4 — P1-E: HMAC + SQLite idempotency + event dispatch), the feedback loop (#5 — P1-F: categorize / attempts / transition + labels + PAT-mode `GhOps`), GitHub authentication + install gate (#6 — P1-G: PAT/App auth + `--self-test`), the wiremock-backed integration suite (#7 — P1-H: all nine §9.2 scenarios end-to-end), and the Phase 1 docs (#8 — P1-I: `BRIDGE.example.md`, `docs/SPEC.md` §11.6 draft, CHANGELOG, README stub). Phase 2 (#9 — P2: `provider: opencode` as a first-class CLI subprocess backend) lands the `OpenCodeAgent` next to `claude_code` / `codex`, the `which` workspace dep for preflight, the doc-spike-validated flag set (`--format json`, `--session <id>`), and the §8 doc deliverables (WORKFLOW example, README + SPEC §18.2 + CHANGELOG entries, `docs/v0.3-plan/02-opencode-VERIFY.md`). Phase 3 (#10 — P3: telemetry + budget enforcement) lands an opt-in OTel emission layer over both binaries (12 spans total + tenant tagging from day one), the typed Sinfonia↔bridge event channel that replaces the dropped OTLP receiver (`AgentEvent::SessionCompleted` + HMAC-signed POST/verify reusing the GitHub webhook scheme), the cost / budget pipeline (`BudgetManager` + embedded cost table + 30 s idle-flush debounce + M-2 freshness gates), terminal-state detection via `pull_request.closed.merged=true`, the `examples/telemetry/` reference Collector + Postgres deployment, and the SPEC §11.6.11 / §11.6.12 / §18.2 + CHANGELOG + README + VERIFY doc surface.

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

**Phase 1 of v0.3 is complete.** All nine sub-tasks (P1-A through P1-I) are merged to `main`; the bridge MVP ships as v0.3.0-alpha.1. The bridge binary parses `BRIDGE.md`, verifies HMAC-signed GitHub webhooks, persists delivery-ID idempotency in SQLite, evaluates CI results, routes by failure category, applies the attempt cap, manages PR labels under the `sinfonia:` prefix, supports both PAT and App auth, and exposes `sinfonia-bridge --self-test` as the install gate. The full chain — bridge writes the marker comment → tracker fetch parses it into `Issue.fields` → prompt template renders `{{ issue.fields.sinfonia_last_ci_failure }}` into the agent's input — works end-to-end.

**Phase 2 of v0.3 is also complete.** `provider: opencode` is wired as a first-class CLI subprocess backend in `crates/sinfonia/src/agent/opencode.rs`, joining `claude_code` and `codex` as a sibling under `agent/cli.rs`'s pattern (parser + spawn + session continuation). OpenCode owns auth via its own CLI (`opencode auth login`) and routes internally to 75+ providers — including a local Ollama-with-LSP path that the raw `ollama` backend can't provide. The `which` crate is now a workspace-level dep used by the preflight binary check.

**Phase 3 of v0.3 is also complete.** Opt-in OTel emission layers over both binaries (six daemon spans + six bridge spans, all carrying the resolved `tenant_id`; `service.namespace = tenant_id` at the resource level so a Collector routing-processor splits per-tenant without touching emission code). The typed Sinfonia↔bridge event channel (`AgentEvent::SessionCompleted` → HMAC-SHA256 POST to `POST /api/v1/sinfonia-events`, reusing the GitHub-webhook verify helper) replaces the originally-proposed bridge-side OTLP receiver. Per-ticket token + cost caps are enforced at the tracker-write boundary with a 30 s idle-flush debounce that coalesces under-cap writes (Linear's marker-comment is read-modify-write, so a busy ticket benefits ~10×). The embedded cost table at `config/cost_table.yaml` (Anthropic / OpenAI / Google / Ollama-zero) ships with two freshness gates — WARN at 90 days, the M-2 asymmetric cost-cap block at 180 days. Workspace test count: **183 passing** (44 sinfonia + 14 conformance + 7 tracker + 107 bridge unit + 9 bridge integration + 2 sinfonia http events), zero failures — up from Phase 1+2's 158 by +25.

**Phase 4 of v0.3 is also complete (merged via #11, `17f5213`).** The five `IssueTracker` bridge-write methods (`transition_issue` / `read_custom_field` / `write_custom_field` / `ensure_custom_field` / `post_comment`) are implemented for `JiraTracker` against the Atlassian Cloud REST API. The bridge-key→display-name resolver + cached `customfield_NNNNN` lookup, the narrow-scope Markdown→ADF converter (paragraphs / fenced code blocks / lists / inline strong/em/code/link), and the best-effort screen-scheme bind (with `docs/JIRA-SCREEN-SCHEME.md` fallback) all ship in this phase. Bridge-config Rule 2 swapped from "Jira deferred to Phase 4" to two positive rules (`endpoint` required; `email` required when endpoint is `*.atlassian.net`). Self-hosted Jira Server / Data Center is supported via PAT-only Bearer auth. The Phase 3 budget pipeline composes without a patch — `CustomFieldValue::String("8.23")` round-trips through a Jira text customfield without semantic loss.

**Phase 5 of v0.3 is also complete (merged via #12, `6462c3a`).** Three deliverable clusters: (a) `sinfonia --check <WORKFLOW.md>` with documented per-failure-class exit codes (0/2/3/4/5) plus `sinfonia init` as the AI-tool-free REPL equivalent of `setup-workflow`; (b) six setup skills at `skills/` (setup-workflow / setup-bridge / setup-state-machine / setup-telemetry / setup-agent-backend / migrate-from-symphony) with `SKILL.md` runbooks, Liquid templates, and shell validators; (c) `docs/SKILLS.md` cross-vendor pointer table, SPEC §18.2 extension entry, CHANGELOG additions, README v0.3 paragraph, `docs/v0.3-plan/05-skills-VERIFY.md`. The state-machine prompt templates satisfy the §8 box-2 grep invariant by construction — every `{{ issue.fields.* }}` reference is followed by `| default:`. The `sinfonia-bridge --self-test` surface called out in plan §3.3 was already shipped by P1-G; Phase 5 §3.3 needed no work on the bridge side.

**Phase 7 of v0.3 has landed on branch (`v0.3-phase-7-docs`) in three reviewable commits.** Three deliverable clusters: (a) SPEC §11.5/§11.6/§11.7/§18.2 finalize + foundational stub polish — §11.5 tightened ("orchestrator MUST NOT write"; pointer to §11.6), §11.6 promoted from Draft to Recommended Extension, new §11.7 documents Linear marker-comment vs Jira `customfield_NNNNN` discovery, §18.2 grew six bullets (Jira tracker adapter, CI feedback bridge, failure categorization, budget enforcement, PR label management; alongside the OpenCode + OTel + setup-skills entries from earlier phases); BRIDGE.example.md budget-caps section now shows realistic non-null values and gains the `cost_table_path` override block; CONTRIBUTING.md updated for the three-crate workspace layout; SKILLS / JIRA-SCREEN-SCHEME stubs get audience headers + cross-link blocks. (b) Three new long-form guides — `docs/DEPLOYMENT.md` (~470 LOC: four self-contained topologies + credential model + observability + scaling + backup + upgrading), `docs/CLIENT_SETUP.md` (~430 LOC: trust-boundary diagram + security posture + GitHub App vs PAT + three-layer budget controls + audit-trail queries + handoff + four-table vendor-evaluation worksheet), `docs/MIGRATION-v0.2-to-v0.3.md` (~150 LOC: "what you DON'T need to do" leads, required + optional + breaking changes); `WORKFLOW.example.md` gains a telemetry-block walkthrough, three OpenCode usage variants (default-lane / state-machine / air-gapped Ollama-with-LSP), and a full failure-categorization state-machine example wired to BRIDGE.md's `feedback_loop.failure_categories`. (c) README rewrite (five-item "What's new" summary keyed on user questions + "Where to go next" cross-link block above Getting Started; conformance scorecard §18.2 expanded), CHANGELOG `[Unreleased]` promoted to `[0.3.0]` with a release summary + P7 Added bullet, docs CI wired (markdownlint-cli2 + lychee internal-on-PR + lychee full-sweep weekly + shell-based fenced-block syntax validator). The plan §11.4 spec-conformance test cross-checking §18.2 against the codebase + the manual readthrough by someone uninvolved are deferred to a follow-up; details in `docs/v0.3-plan/07-docs-VERIFY.md`. Workspace test count: **227 passing, 0 failures** — unchanged (Phase 7 is docs + CI only, no Rust code added).

**Phase 6 of v0.3 is also complete (merged via #13, `8b9ac66`).** Six production images publish to `ghcr.io/o-side-systems/` from a single multi-stage `Dockerfile` driven by `docker-bake.hcl` — `sinfonia` (daemon only), `sinfonia-bridge` (bridge only, parented directly off `debian:bookworm-slim` rather than the shared `sinfonia-base` so the standalone-bridge image stays small), `sinfonia-with-claude-code`, `sinfonia-with-codex`, `sinfonia-with-opencode`, and `sinfonia-all-agents`. The build stage shares one `cargo build --release` across all six via BuildKit registry + target cache mounts. Each target gets a three-tag fan-out (`:VERSION` / `:VERSION_MINOR` / `:latest`) via the `tags(name)` HCL helper; `bake --print` confirms the expected shape. The new root `docker-compose.yml` demonstrates the production topology (daemon + bridge + OTel Collector + Postgres) with read-only user-credential bind mounts; the pre-existing dev-shell `Dockerfile` and `docker-compose.yml` move verbatim to `Dockerfile.dev` / `docker-compose.dev.yml` (the dev compose updates its inner `build.dockerfile` reference; otherwise unchanged). The publish workflow at `.github/workflows/docker-publish.yml` triggers on `v*` tags, runs `docker buildx bake --push`, then runs `tests/docker-smoke.sh` (per-image `--help`) and `tests/docker-compose-smoke.sh` (full stack up under the `docker-compose.ci.yml` overlay, polling `:8080/api/v1/state` and `:8081/health`), then per-image Trivy with `severity: CRITICAL,HIGH` and `exit-code: 1`. Phase 5's `setup-bridge` / `setup-telemetry` skills already generate the exact compose shape Phase 6 ships — no skill changes needed.

The next pickup is the **v0.3.0 release** itself — merge `v0.3-phase-7-docs`, run the manual readthrough (per `07-docs-VERIFY.md` V-5), fill the `2026-MM-DD` placeholder in `CHANGELOG.md` at tag time, then `git tag v0.3.0` and let `.github/workflows/docker-publish.yml` push the six images to `ghcr.io/o-side-systems/`.

The single most important non-obvious decision the Phase 1+2 work bequeathed to Phase 3 (resolved this phase): **the agent-side token-accounting plumbing was already in place across every CLI backend**, and `TurnOutcome::Completed` now exposes the per-turn `usage: TokenUsage` directly (the runner aggregates session totals without re-parsing the event channel). Phase 3 instrumented six daemon span sites with `tracing::field::Empty` placeholders + late `span.record()` for runtime values; no fresh code paths needed instrumentation.

The single most important non-obvious decision made during Phase 1 itself (forward-relevant to every later phase): **`CustomFieldValue` is three variants (`Null` / `Number` / `String`), not five** (the plan-doc §4 draft showed `Decimal` / `LongText` / `Url` as separate variants). Cost values, URLs, and long-text fields all serialize as `String`. See §5.1 below. This is the foundation Phase 3's budget caps and Phase 4's Jira custom-field writes both build on — Phase 3 writes `sinfonia_cost_consumed_usd` as `CustomFieldValue::String("8.23")` (stringified for precision via `rust_decimal::Decimal`; never f64 for money).

The single most important non-obvious decision surfaced during Phase 2: **the OpenCode CLI's actual flag set differs from what the plan doc proposed**, and the doc-spike-against-vendor-docs pattern this exposed is reusable. The plan said `--prompt-stdin`, `--output-format json`, `--quiet`, `--continue <id>` — what `opencode.ai/docs` actually documents is `--format json` (no `--prompt-stdin` or `--quiet` needed; stdin auto-detected, JSON suppresses TUI) and `--session <id>` (bare `--continue` resumes only the *last* session globally — wrong for concurrent per-issue workspaces). Captured in `docs/v0.3-plan/02-opencode-VERIFY.md` and in the `opencode.rs` module rustdoc. Phase 3 confirmed this lesson generalizes — the OTel crates moved ~8 minors between plan write and impl time (`opentelemetry 0.24/0.17/0.25` plan → `0.32/0.33` actual); feature-flag names held up, version numbers needed re-pinning. **The forward-relevant lesson for Phase 4: any time the plan doc references a vendor-API endpoint shape (Jira REST), a wire-format dialect (ADF), or a third-party HTTP knob, verify it before writing code.** Jira's `/rest/api/3/issue/{id}/transitions` endpoint and the ADF "doc/paragraph/text" content shape are the two most likely surfaces where the plan-doc proposed shape might have drifted.

The single most important non-obvious decision surfaced during Phase 4: **Jira identifies custom fields by display name, not by stable bridge key**, and there is no "stable key" field on `GET /rest/api/3/field` to match against. The bridge therefore ships a `jira::display_name_for_key(key) -> String` helper that maps each well-known bridge key (`sinfonia_attempt_count`) to a stable display name (`Sinfonia Attempt Count`); the operator-facing display name *is* the contract, and `docs/JIRA-SCREEN-SCHEME.md` documents it so admins who want to pre-create the fields with a different display name can. The other Phase 4 delta: the bridge uses plain `GET /rest/api/3/field` (flat array, default scope) rather than `/field/search` (paged, requires `manage:jira-configuration` scope) — captured in `docs/v0.3-plan/04-jira-VERIFY.md` §1. **The forward-relevant lesson for Phase 5: when designing the `setup-bridge` skill's Jira branch, ask the operator for the project key + endpoint + email + token; the field provisioning is fully automatic and idempotent, but the screen-scheme bind step needs admin perms — surface that in the skill UX.**

The single most important non-obvious decision surfaced during Phase 3: **OTel metrics are deferred to Phase 3.1, span-derived in v0.3.** The plan §6 listed nine `MeterProvider`-instrumented metrics; we shipped twelve spans instead and proved (per `examples/telemetry/queries/*.sql`) that the plan §8.2 dashboard SQL reads from span attributes via the `events` table, not from OTel metric points. The exit criteria from plan §9.3 are met span-derived. Adding the metrics layer when (a) a user needs Prometheus scraping, or (b) someone wants the `sinfonia.orchestrator.concurrent_runs` gauge (the one metric without a span analog) is additive — see `docs/v0.3-plan/03-telemetry-VERIFY.md` §2.1 for the per-metric span-derived equivalent.

---

## 1. What's merged to `main`

### Commits

| Commit / PR | Title | Scope |
|---|---|---|
| `82d2d2f` (#2) | Add v0.3 implementation plan docs (eight phase plans + index) | Docs only — `docs/v0.3-plan/00..07.md` |
| `3f045e9` (#2) | Phase 1 foundation: workspace conversion + tracker extensions + H-1 fix | Code — P1-A / P1-B / P1-C |
| `d6cd7ca` | Add v0.3 milestone STATUS doc | Docs — this file |
| `07c0381` (#3) | P1-D: sinfonia-bridge crate skeleton + BRIDGE.md config parser | Code — `crates/sinfonia-bridge/`, 16 unit tests |
| `cca48a0` | STATUS: mark P1-D merged, queue P1-E as next deliverable | Docs — this file |
| `69eb8e0` (#4) | P1-E: webhook handlers + HMAC verify + SQLite idempotency | Code — `storage.rs`, `webhook/verify.rs`, full `POST /webhook` handler; +23 unit tests (6 verify, 6 storage, 11 handlers) |
| `fd9629e` | STATUS: mark P1-E merged, queue P1-F as next deliverable | Docs — this file |
| `9d33d51` (#5) | P1-F: feedback loop + categorization + labels | Code — `feedback/` (`mod`, `categorize`, `attempts`, `transition`), `labels.rs` (`BridgeLabel` + `LabelManager`), `github/` (`mod` + `client.rs` PAT-only `GhOps` trait + production impl); `webhook/handlers.rs` swaps the P1-E stubs for `dispatch_ci_event` calling `evaluate_ci`; bridge crate jumps from 39 to 67 unit tests |
| `7cc373d` | STATUS: mark P1-F merged, queue P1-G as next deliverable | Docs — this file |
| `b0d7272` (#6) | P1-G: GitHub auth (PAT + App) + `--self-test` | Code — `github/auth.rs` (`BridgeAuthMode`, `load_private_key`, `AppModeGhOps`, `build_gh_ops`); `selftest.rs` (serial PASS/FAIL/SKIP runner); `config.rs` adds `server.public_url`; `main.rs` gains `--self-test` flag and routes through `auth::build_gh_ops`; bridge crate climbs from 67 to 89 unit tests |
| `62e7f9d` | STATUS: mark P1-G merged, queue P1-H as next deliverable | Docs — this file |
| `d7ad72d` (#7) | P1-H: bridge integration tests (wiremock-backed, 9 scenarios) | Code — `tests/bridge_e2e.rs` (~1.4k LOC: full daemon boot + per-test GitHub & Linear `MockServer`s + HMAC-signed webhook helper + per-scenario `LinearGraphqlMock` dispatcher); `storage.rs` drops `#[cfg(test)]` gate on `open_in_memory`; `Cargo.toml` adds `wiremock = "0.6"` + crypto dev-deps for the App-mode RSA test key |
| `4789d8f` | STATUS: mark P1-H merged, queue P1-I as next deliverable | Docs — this file |
| `a057218` (#8) | P1-I: Phase 1 documentation (BRIDGE.example.md, SPEC §11.6, CHANGELOG, README stub) | Docs — `BRIDGE.example.md` (new, repo root; 243-line fully-commented working config that parses cleanly under `--check` with no env vars); `docs/SPEC.md` §11.6 (217-line draft bridge extension contract in RFC-2119 voice, inserted between §11.5 and §12); `CHANGELOG.md` adds `## [0.3.0-alpha.1] — 2026-05-21`; `README.md` adds "What's new in v0.3 (preview)" stub above §"Sinfonia vs. Symphony" |
| `1ce6c0e` | STATUS: mark P1-I merged, Phase 1 complete | Docs — this file |
| `3b84a20` (#9) | P2: OpenCode agent backend | Code + docs — `crates/sinfonia/src/agent/opencode.rs` (new, 686 LOC: `OpenCodeAgent` impl + 8 unit tests, mirroring `cli.rs`'s parser + spawn + continuation pattern); `agent/mod.rs` + `config/typed.rs` factory + enum wiring; `which = "8"` added to `[workspace.dependencies]` + consumed in `crates/sinfonia/Cargo.toml` for the preflight binary check; `tests/spec_conformance.rs::opencode_provider_parses` (new) exercises all three §4 WORKFLOW shapes; `WORKFLOW.example.md` + README backend table + `docs/SPEC.md` §18.2 + CHANGELOG `[Unreleased]` all carry the OpenCode entry; `docs/v0.3-plan/02-opencode-VERIFY.md` (new, 204 LOC) captures the doc-spike findings and §5.3 manual-verification steps (pending real-world run before tag) |
| `c5226fe` | STATUS: mark Phase 2 merged, queue Phase 3 as next deliverable | Docs — this file |
| `69ae360` (#10) | P3: telemetry foundation (OTel deps + spans + tenant resolver) | Code — OTel crates added to `[workspace.dependencies]` (`opentelemetry 0.32` / `opentelemetry_sdk 0.32` / `opentelemetry-otlp 0.32` / `opentelemetry-semantic-conventions 0.32` + `semconv_experimental` / `tracing-opentelemetry 0.33` / `rust_decimal 1` / `hmac 0.12` + `sha2 0.10` promoted to workspace scope). `crates/{sinfonia,sinfonia-bridge}/src/telemetry/{mod,tenant,spans}.rs` (new × 6): `init_observability(format, telemetry)` wraps the existing `tracing_subscriber::fmt` layer with an `SdkTracerProvider` (the 0.32 API rename — see §5.13); `TenantId::resolve(config → SINFONIA_TENANT_ID → "default")`; span-name + attribute-key constants. `TelemetryConfig` (`WORKFLOW.md`) and `TelemetrySection.headers` (`BRIDGE.md`) wired; the existing N-1 validation rule for `sinfonia_events_secret` carries forward. Six daemon spans (`orchestrator.tick`, `orchestrator.dispatch`, `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`) instrumented with `tracing::field::Empty` placeholders + late `span.record()`. `TurnOutcome::Completed` now carries `usage: TokenUsage`; `Orchestrator::dispatch_one` returns `DispatchOutcome::{Dispatched, Skipped, NoSlot}` so `orchestrator.tick`'s `dispatched_count` is honest. |
| `c51c81a` (#10) | P3: bridge spans + typed Sinfonia↔bridge event channel (§5, §7.2) | Code — five bridge spans instrumented (`bridge.webhook`, `bridge.ci_result`, `bridge.state_transition`, `bridge.cap_hit`, `bridge.events_receive`). `AgentEvent::SessionCompleted` variant emitted from `runner.rs` immediately after `agent.stop_session(...)` per the N-3 fix. `crates/sinfonia/src/http/events.rs` (new, ~300 LOC): per-process `SubscriberRegistry`, `RecentBuffer` ring, `spawn_emitter(...)` task with HMAC-SHA256 signer (header `X-Sinfonia-Signature-256`, same scheme as the GitHub webhook) and exponential-backoff retry. New routes: `POST /api/v1/events/subscribers`, `GET /api/v1/events/recent`. `crates/sinfonia-bridge/src/events.rs` (new, ~220 LOC): `POST /api/v1/sinfonia-events` handler reusing `webhook::verify::verify_signature` (zero algorithm fork); dispatches by `type` field with forward-compat ignored response for unknown types. |
| `6eaf4c2` (#10) | P3: cost table + budget enforcement + terminal-state detection (§7, §6) | Code — `config/cost_table.yaml` (Anthropic / OpenAI / Google / Ollama-zero, `verified_at: 2026-05-21`), embedded via `include_str!`, overridable via `bridge.cost_table_path`. `crates/sinfonia-bridge/src/feedback/cost.rs` (new, ~290 LOC): `CostTable::compute_cost` with `rust_decimal::Decimal` end-to-end + `is_stale_warn` (90d) + M-2 asymmetric `accepts_cost_caps` (180d) gates. `crates/sinfonia-bridge/src/feedback/budget.rs` (new, ~480 LOC): per-process per-ticket accumulator, cap-detection (`Accumulated` vs `CapHit { kind }`), 30 s idle-flush reconciler, `flush_ticket` writes `sinfonia_tokens_consumed` (Number) + `sinfonia_cost_consumed_usd` (String per STATUS §5.1) to the tracker + emits `bridge.cost_update` span. SessionCompleted event handler feeds `BudgetManager::apply_session`; on `CapHit` flushes immediately + transitions to `feedback_loop.budget_exceeded_state`. `handle_pull_request` gained a `closed + merged=true` branch that flushes the accumulator + emits the transition log (terminal-state via the existing GitHub webhook, no tracker polling). `WELL_KNOWN_FIELDS` gains `sinfonia_budget_exhausted_at`. `AppState::with_default_budget(...)` test helper added so the existing P1-H integration suite adopts the new shape unchanged. |
| `cc9f1a1` (#10) | P3: docs (SPEC §11.6/§18.2 + CHANGELOG + README + VERIFY) + reference assets | Docs — `docs/SPEC.md` gains §11.6.11 (typed Sinfonia↔bridge event channel, full wire shape + HMAC contract) + §11.6.12 (budget enforcement surface, freshness gates, per-ticket overrides) + a §18.2 entry for OpenTelemetry emission with `tenant_id`. CHANGELOG `[Unreleased]` adds the Phase 3 Added / Changed / Deferred entries. README gains a Phase 3 What's-new bullet + new Observability section showing the env-var path. `docs/v0.3-plan/03-telemetry-VERIFY.md` (new, ~240 LOC): captures the OTel crate-version delta (plan-doc 0.24/0.17/0.25 → actual 0.32/0.33), the SDK API rename, the `semconv_experimental` gating decision, the metrics-layer deferral with span-derived equivalents listed per metric, and the §9.3 manual-verification protocol. `examples/telemetry/` (new): `postgres-schema.sql` (sessions / attempts / events tables + indexes), `otel-collector-config.yaml` (OTLP receiver + routing-by-tenant + Postgres exporter starter), three `queries/*.sql` (tenant monthly cost, first-try rate, budget-heavy tickets), and `README.md` (wiring guide + full span / attribute reference + multi-tenant notes). |
| `2fa8d8c` (#10) | P3: VERIFY notes — defer wire-level integration tests to Phase 3.1 | Docs — `03-telemetry-VERIFY.md` §2.5 captures the integration-test deferral with a cross-reference table mapping each wire-level concern (HMAC algo + format, cap detection, flush field types, cost table + freshness gates, tenant resolver, subscriber registry, schema round-trip) to the unit test that pins it. Manual verification per plan §9.3 covers the remaining end-to-end concern. |
| `b1ecf96` | STATUS: mark Phase 3 merged, queue Phase 4 as next deliverable | Docs — this file |
| `2d166aa` (#11) | P4: Jira bridge write surface — five IssueTracker methods + ADF converter (§11.6, plan 04) | Code — `jira.rs` (+563 LOC), `jira_adf.rs` (+453 LOC), `tests/jira_wiremock.rs` (+250 LOC), `config.rs` Rule 2 swap, `main.rs` + `selftest.rs` Jira wiring. +27 tests (210 total). |
| `3f9eb1c` (#11) | P4: docs (SPEC §11.6 + CHANGELOG + README + BRIDGE.example + VERIFY + JIRA-SCREEN-SCHEME) | Docs — SPEC §11.6.2 Jira bullet rewrite, CHANGELOG `[Unreleased]` 3 new bullets, README Phase 4 paragraph, BRIDGE.example Jira section refresh, `docs/v0.3-plan/04-jira-VERIFY.md` (new, ~140 LOC), `docs/JIRA-SCREEN-SCHEME.md` (new, ~95 LOC). |
| `b975510` (#11) | STATUS: mark Phase 4 landed on branch; baseline +27 tests; queue Phase 5 | Docs — this file (pre-merge prep on the branch) |
| `17f5213` | Merge pull request #11 from O-Side-Systems/v0.3-phase-4-jira-bridge | Merge commit |
| `c024a67` | STATUS: mark Phase 4 merged, queue Phase 5 as next deliverable | Docs — this file |
| `1257f2d` (#12) | P5-A: `sinfonia --check` + `sinfonia init` (§3.1, §3.2) | Code — `crates/sinfonia/src/main.rs` clap restructure, new `check.rs` + `init_repl.rs`; `inquire` promoted to workspace dep; +11 tests (6 check + 5 init_repl). |
| `4791207` (#12) | P5-B: six setup skills with SKILL.md + Liquid templates + validators (§2) | Skills — `skills/setup-{workflow,bridge,state-machine,telemetry,agent-backend}/`, `skills/migrate-from-symphony/`; 33 files across the six folders; state-machine prompts pass the §8 box-2 grep invariant. |
| `f6c13b2` (#12) | P5-C: docs (SKILLS.md + SPEC §18.2 + CHANGELOG + README + VERIFY) + integration tests + STATUS bump | Docs + tests — `docs/SKILLS.md` (new), `docs/SPEC.md` §18.2 extension entry, CHANGELOG `[Unreleased]` Phase 5 block, README Phase 5 paragraph, `docs/v0.3-plan/05-skills-VERIFY.md` (new), `crates/sinfonia/tests/skills_integration.rs` (new, 6 tests). |
| `6462c3a` | Merge pull request #12 from O-Side-Systems/v0.3-phase-5-skills-cli | Merge commit (true merge — three intermediate commits preserved) |
| `02b7774` | STATUS: mark Phase 5 merged, queue Phase 6 as next deliverable | Docs — this file |
| `9001d5c` (#13) | P6: Phase 6 — Docker images + production compose + publish workflow + smoke harness | Code + docs — root `Dockerfile` (six-target multi-stage, BuildKit cache mounts shared across the build stage), `docker-bake.hcl` (registry/platform/tags + `tags(name)` HCL helper), new production `docker-compose.yml` (daemon + bridge + OTel Collector + Postgres) and `docker-compose.ci.yml` overlay (Compose v2.24+ `!reset` clears user-credential bind mounts), pre-existing dev files renamed to `Dockerfile.dev` / `docker-compose.dev.yml` (the latter's inner `build.dockerfile` updated), `.github/workflows/docker-publish.yml` (bake-push + per-image Trivy with `severity: CRITICAL,HIGH` / `exit-code: 1`), `tests/docker-smoke.sh` + `tests/docker-compose-smoke.sh` + `tests/fixtures/{WORKFLOW,BRIDGE}.smoke.md` (smoke fixtures validated by the existing `--check` gate before commit), README "Docker" section + CHANGELOG `[Unreleased]` Phase 6 block + `docs/v0.3-plan/06-docker-VERIFY.md` (new). |
| `8b9ac66` | Merge pull request #13 from O-Side-Systems/v0.3-phase-6-docker | Merge commit |
| `32a26a6` | STATUS: mark Phase 6 merged, queue Phase 7 as next deliverable | Docs — this file |
| `4efa67f` (P7-A) | SPEC §11.5/§11.6/§11.7/§18.2 + foundational stub polish | Docs — `docs/SPEC.md` (§11.5 tightened, §11.6 Draft tag dropped, new §11.7, six new §18.2 bullets, two bare-URL fixes); `BRIDGE.example.md` (budget caps + cost_table_path + telemetry walkthrough + cross-links); `CONTRIBUTING.md` (workspace layout, three-crate table, `--workspace` / `-p crate` commands); `docs/SKILLS.md` + `docs/JIRA-SCREEN-SCHEME.md` (audience headers + cross-link blocks) |
| `4a4e344` (P7-B) | DEPLOYMENT + CLIENT_SETUP + MIGRATION + WORKFLOW.example updates | Docs — `docs/DEPLOYMENT.md` (NEW, ~470 LOC: four topologies + credential model + observability + scaling + backup + upgrading); `docs/CLIENT_SETUP.md` (NEW, ~430 LOC: trust-boundary diagram + security posture + budget controls + audit trail + handoff + vendor-evaluation worksheet); `docs/MIGRATION-v0.2-to-v0.3.md` (NEW, ~150 LOC); `WORKFLOW.example.md` (telemetry block walkthrough + 3 OpenCode usage variants + failure-categorization state-machine example) |
| `4d7e69d` (P7-C) | README rewrite + CHANGELOG v0.3.0 + docs CI | Docs + CI — `README.md` (five-item "What's new in v0.3" rewrite + cross-link block + §18.2 scorecard); `CHANGELOG.md` (`[Unreleased]` → `[0.3.0]` promotion); `.github/workflows/docs.yml` (NEW: markdownlint-cli2 + lychee internal-on-PR + lychee full-sweep weekly + fenced-block syntax check); `.markdownlint-cli2.yaml` + `lychee.toml` (NEW); `scripts/check-doc-code-blocks.sh` (NEW, validates fenced YAML/JSON/TOML/bash); `skills/setup-bridge/SKILL.md` + `docs/SPEC.md` two bare URLs wrapped to satisfy markdownlint |
| (this commit) | STATUS: mark Phase 7 landed on branch; v0.3.0 release is the next gate | Docs — this file |

### Phase 1 sub-task status

| Sub-task | Plan section | Status | Notes |
|---|---|---|---|
| **P1-A** workspace conversion + verify script | `01-bridge-mvp.md` §2 | ✅ merged | `crates/sinfonia/`, `crates/sinfonia-tracker/`, `scripts/verify-workspace-move.sh` |
| **P1-B** tracker trait extensions + `custom_fields` module | §4, §11 | ✅ merged | 5 new bridge-write methods on `IssueTracker`; Linear impls; Jira returns `NotImplemented` (Phase 4 fills) |
| **P1-C** `Issue.fields` + Linear marker-comment + template scope (H-1) | §4.2 | ✅ merged | `Issue.fields` populated by Linear fetch; `template.rs` pre-seeds well-known keys |
| **P1-D** bridge binary skeleton + BRIDGE.md config parser | §2, §3 | ✅ merged | `crates/sinfonia-bridge` crate scaffolded; BRIDGE.md parser + 9 validation rules + 16 unit tests; axum router with `/health` + stub `POST /webhook`; `--check` flag |
| **P1-E** webhook handlers + HMAC verify + SQLite idempotency | §5, §9 | ✅ merged | `storage.rs` (rusqlite + Mutex; `processed_deliveries` + `pr_ticket_map`); `webhook/verify.rs` (HMAC-SHA256, constant-time compare); full `POST /webhook` dispatch for `pull_request` / `check_suite` / `workflow_run`; `AppState` carries `Arc<Store>` + `Arc<dyn IssueTracker>`; +23 unit tests |
| **P1-F** feedback loop + categorization + labels | §5.2, §6, §7 | ✅ merged | `feedback/` (`evaluate_ci`, `categorize`, `attempts`, `transition`); `labels.rs` (`BridgeLabel` + `LabelManager`); `github/` (`GhOps` trait + PAT-mode `OctocrabGhOps`); `dispatch_ci_event` wired in `handle_check_suite` / `handle_workflow_run`; bridge crate now at 67 unit tests |
| **P1-G** GitHub auth (PAT + App) + `--self-test` | §8 | ✅ merged | `github/auth.rs` (`BridgeAuthMode`, `load_private_key`, `AppModeGhOps`, `build_gh_ops`); `selftest.rs`; `config::ServerSection::public_url: Option<Url>`; `--self-test` CLI flag; +22 unit tests across `github::auth`, `selftest`, `config` |
| **P1-H** integration tests with `wiremock` | §9.2 | ✅ merged | `tests/bridge_e2e.rs` — all nine §9.2 scenarios as `#[tokio::test]`s; per-test GitHub + Linear `MockServer`s; `LinearGraphqlMock` dispatches by GraphQL query keyword; App-mode test mints a real RS256 JWT against a generated test RSA key |
| **P1-I** Phase 1 docs (BRIDGE.example.md, SPEC §11.6 draft, CHANGELOG, README stub) | §12 | ✅ merged | `BRIDGE.example.md` (parses under `--check` with no env vars); `docs/SPEC.md` §11.6 draft (10 subsections, RFC-2119 voice); `CHANGELOG.md` `[0.3.0-alpha.1]`; `README.md` "What's new in v0.3 (preview)" stub. **All Phase 1 boxes on `01-bridge-mvp.md` §12 are now checked.** |

### Phase 2 sub-task status

Phase 2 shipped as a single atomic commit (one PR), unlike Phase 1's nine-sub-task split. The mapping back to the `02-opencode-backend.md` §8 deliverable checklist:

| Deliverable | Plan section | Status | Notes |
|---|---|---|---|
| **P2** OpenCode agent backend (`opencode.rs` + enum variant + factory wiring + `which` dep + unit tests + integration parse-test + WORKFLOW/README/SPEC §18.2/CHANGELOG/VERIFY.md docs) | §3, §4, §5, §6 | ✅ merged | All eleven §8 boxes checked in one PR (#9, commit `3b84a20`, merge `f26aca7`). Manual end-to-end verification per §5.3 is the one deferred item — tracked in `docs/v0.3-plan/02-opencode-VERIFY.md` as "pending real-world run before tagging v0.3.0-alpha.x." Doc spike (plan §7 open question 1) resolved against `opencode.ai/docs` + the upstream `sst/opencode` source; flag deltas vs. the plan's proposed surface are noted at the top of `opencode.rs` and in the VERIFY doc. |

### Phase 3 sub-task status

Phase 3 shipped as one PR (#10) with five intermediate commits walking the work in a reviewable order (squash-merged on `main` as `707a812`). The mapping back to the `03-telemetry-budget.md` §12 deliverable checklist:

| Deliverable | Plan section | Status | Notes |
|---|---|---|---|
| OTel client crates + telemetry module skeletons (both binaries) | §2, §3, §10 | ✅ merged | `69ae360`. Version set verified at impl time: `opentelemetry 0.32` / `opentelemetry_sdk 0.32` / `opentelemetry-otlp 0.32` / `opentelemetry-semantic-conventions 0.32` + `semconv_experimental` / `tracing-opentelemetry 0.33`. Plan-doc proposed `0.24/0.17/0.25`; feature-flag names held up. SDK API renamed (`SdkTracerProvider`; `with_batch_exporter` takes exporter alone). |
| `TelemetryConfig` + `TelemetrySection` parsing + N-1 validation | §3.1, §7.2 | ✅ merged | `69ae360`. Both binaries share field semantics; bridge has extras (`sinfonia_event_subscribe_url`, `sinfonia_event_callback_url`). The N-1 rule (events_secret required when subscribe_url is set) was already in place from P1-D prep; tested in `rule9_events_subscribe_without_secret_errors`. |
| Six daemon spans per §4 (`orchestrator.tick`, `orchestrator.dispatch`, `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`) | §4 | ✅ merged | `69ae360`. `TurnOutcome::Completed` now carries `usage: TokenUsage`; `Orchestrator::dispatch_one` returns `DispatchOutcome::{Dispatched,Skipped,NoSlot}` for honest `dispatched_count`. `tracker.fetch` spans hardcode the span name + attribute keys (no `sinfonia-tracker` → binary crate dependency). |
| Five bridge spans per §5 (`bridge.webhook`, `bridge.ci_result`, `bridge.state_transition`, `bridge.cap_hit`, `bridge.events_receive`) | §5 | ✅ merged | `c51c81a`. `bridge.cost_update` lands with `feedback/budget.rs` (`6eaf4c2`). `bridge.pr_label` deferred — `labels.rs` tracing logs cover the dashboard need. |
| `AgentEvent::SessionCompleted` + emission at the runner exit | §7.2, N-3 | ✅ merged | `c51c81a`. Emitted from `crates/sinfonia/src/orchestrator/runner.rs` immediately after `agent.stop_session(...)` per N-3. |
| Subscriber-emitter task + `/api/v1/events/subscribers` + `/api/v1/events/recent` | §7.2 | ✅ merged | `c51c81a`. `crates/sinfonia/src/http/events.rs` (new). HMAC-SHA256 signer (header `X-Sinfonia-Signature-256`, `sha256=<hex>` value — same scheme as the GitHub webhook). 5-attempt retry with 250 ms → 8 s backoff. 200-entry diagnostic ring buffer. |
| `POST /api/v1/sinfonia-events` handler on the bridge | §7.2 | ✅ merged | `c51c81a`. Reuses `webhook::verify::verify_signature` — zero algorithm fork. Dispatches by `type` field; unknown types acknowledged with `200 OK ignored` for forward-compat. |
| Cost table + `feedback/cost.rs` | §7.1, M-2 | ✅ merged | `6eaf4c2`. `config/cost_table.yaml` embedded via `include_str!`; override via `bridge.cost_table_path`. `Decimal` end-to-end. M-2 asymmetric freshness gate: token caps survive a stale table, cost caps don't. OpenCode `provider/model` wire format handled by lookup. |
| `feedback/budget.rs` + 30 s debounce + cap-hit transition | §7.3, §7.4 | ✅ merged | `6eaf4c2`. Per-process per-ticket accumulator; cap-crossings flush immediately + transition to `feedback_loop.budget_exceeded_state`. 30 s idle-flush reconciler. `BudgetManager` not durable across restart; on restart re-reads last persisted totals as the new baseline. |
| `WELL_KNOWN_FIELDS` budget additions | STATUS §5.2 | ✅ merged | `6eaf4c2`. Added `sinfonia_budget_exhausted_at`. The other Phase 3 keys (`sinfonia_tokens_consumed`, `sinfonia_cost_consumed_usd`, `sinfonia_max_cost_usd`) were pre-registered in P1-D. |
| Terminal-state detection via `pull_request.closed.merged=true` | §6 | ✅ merged | `6eaf4c2`. New branch in `handle_pull_request` looks up the ticket via `pr_ticket_map`, flushes the budget accumulator, emits the transition log. Replaces the planned `bridge.attempts_to_close` + `cost_per_ticket_usd` histograms (deferred with the metrics layer). |
| Reference Collector + Postgres assets | §8 | ✅ merged | `cc9f1a1`. `examples/telemetry/{postgres-schema.sql,otel-collector-config.yaml,queries/*.sql,README.md}`. Three §8.2 dashboard queries verified to read from span attributes in the `events` table (not OTel metric points). |
| SPEC §11.6 update + §18.2 entry + CHANGELOG + README Observability | §12 | ✅ merged | `cc9f1a1`. SPEC gains §11.6.11 (typed event channel contract) + §11.6.12 (budget enforcement surface) + a §18.2 entry. CHANGELOG `[Unreleased]` has the Added / Changed / Deferred-to-v0.3.1 blocks. README Observability section shows the env-var path. |
| `docs/v0.3-plan/03-telemetry-VERIFY.md` (verify doc) | §9.3 | ✅ merged | `cc9f1a1` (initial), `2fa8d8c` (integration-test deferral note). Captures the OTel crate-version delta, the SDK API rename, the metrics-layer deferral (with span-derived equivalents listed per metric), the integration-test deferral cross-reference table, and the §9.3 manual-verification protocol. |
| OTel metrics layer (`MeterProvider` + 9 instruments per §6) | §6 | ⏳ deferred to v0.3.1 | Plan §8.2 dashboards read span attributes from the `events` table (not OTel metric points), so exit criteria are met span-derived. Rationale + per-metric span-derived equivalent in `03-telemetry-VERIFY.md` §2.1. |
| Wire-level integration tests (`tests/telemetry_e2e.rs`, `tests/budget_e2e.rs`) | §9.2 | ⏳ deferred to v0.3.1 | Algorithmic surface pinned by the unit suite — cross-reference table in `03-telemetry-VERIFY.md` §2.5 maps each wire-level concern (HMAC algo + format, cap detection, flush field types, cost table + freshness gates, tenant resolver, subscriber registry, schema round-trip) to the unit test that pins it. Plan §9.3 manual verification covers the end-to-end wire concern. |

### Phase 4 sub-task status

Phase 4 shipped as one PR (#11) with two intermediate commits (code + docs) plus a pre-merge STATUS prep commit, squash-merged on `main` as `17f5213`. The mapping back to the `04-jira-bridge.md` §8 deliverable checklist:

| Deliverable | Plan section | Status | Notes |
|---|---|---|---|
| Five `IssueTracker` methods implemented in `crates/sinfonia-tracker/src/jira.rs` | §3.1–§3.3, §3.5 | ✅ merged | `transition_issue` / `read_custom_field` / `write_custom_field` / `ensure_custom_field` / `post_comment`. Field-ID cache: `Arc<tokio::sync::RwLock<HashMap<String, String>>>` on `JiraTracker`. Helpers: `jira_field_type` / `jira_searcher_key` / `display_name_for_key` / `parse_field_value` / `serialize_field_value`. |
| Markdown → ADF converter | §3.5, §7 #4 | ✅ merged | `crates/sinfonia-tracker/src/jira_adf.rs` (new, 453 LOC incl. 11 unit tests). Subset: paragraphs / fenced code blocks / bullet + ordered lists / inline strong + em + code + link. Unsupported features fall through to plain paragraphs (per §7 #4). |
| Bridge config validation no longer rejects `tracker.kind: jira` | §4 | ✅ merged | `config.rs` Rule 2 swapped from rejection to positive rules. `tracker.endpoint` required; `tracker.email` required when endpoint contains `.atlassian.net`. Four new tests: `rule2_jira_cloud_with_email_is_accepted`, `rule2_jira_self_hosted_pat_is_accepted_without_email`, `rule2_jira_missing_endpoint_errors`, `rule2_jira_cloud_missing_email_errors`. |
| Screen-scheme binding attempt + clear error path + `docs/JIRA-SCREEN-SCHEME.md` | §3.4 | ✅ merged | `JiraTracker::bind_field_to_default_screen` walks `/rest/api/3/screens` → first matching screen → first tab → `POST /screens/{id}/tabs/{tab}/fields`. Failure path: `tracing::warn!` with link to manual-bind doc. The doc (new, ~95 lines) covers Cloud + Server/DC and includes the seven-field reference table. |
| Unit tests per §5.1 | §5.1 | ✅ merged | 25 sinfonia-tracker unit tests (8 new for Jira: `field_type_mapping_covers_all_kinds`, `searcher_key_mapping_covers_all_kinds`, `display_name_round_trip_for_well_known_keys`, `parse_field_value_handles_each_shape`, `serialize_field_value_emits_bare_primitives`, `transition_lookup_happy_path`, `transition_lookup_no_match`; 11 ADF tests). |
| Integration tests per §5.2 mirroring Phase 1's Linear scenarios | §5.2 | ⚠️ scope-narrowed | Instead of re-mirroring the 1.4k-LOC bridge_e2e harness for Jira's REST API, Phase 4 ships `crates/sinfonia-tracker/tests/jira_wiremock.rs` (250 LOC, 6 scenarios) that drives every write method through a real HTTP path against wiremock. Feedback-loop logic is tracker-agnostic and already covered by the Linear bridge_e2e harness. Rationale in `docs/v0.3-plan/04-jira-VERIFY.md` §3.1. |
| Manual verification recorded in `docs/v0.3-plan/04-jira-VERIFY.md` | §5.3 | ⏳ matrix recorded, runs pending | The VERIFY doc captures the six-row verification matrix (V-1 through V-6) with status; runs against a real Atlassian sandbox are pending before `v0.3.0-alpha.x` tag. |
| `BRIDGE.example.md` updated with both `kind: linear` and `kind: jira` sections | §4 | ✅ merged | Removed "rejected with friendly error" comment; added Server/DC PAT variant + first-run screen-binding note. |
| `docs/SPEC.md` §11.6 reflects both tracker implementations | — | ✅ merged | §11.6.2 Jira bullet rewritten: documents the bridge-key→display-name→`customfield_NNNNN` resolution path, the field-id cache, and the ADF requirement for `post_comment`. |
| CHANGELOG entry | — | ✅ merged | Three new bullets in `[Unreleased]` covering the Jira write surface, the config validation rule swap, and the `--self-test` Jira probe. |
| `main.rs` + `selftest.rs` Jira wiring | §6 (inherited) | ✅ merged | `main.rs` arm replaced "not supported until Phase 4" error with `Arc::new(JiraTracker::new(&tracker_cfg)?)`. `selftest.rs` Jira probe routes through `fetch_candidate_issues` (`POST /rest/api/3/search`) as the reachability + auth check. |

### Phase 5 sub-task status

Phase 5 shipped as one PR (#12) with three intermediate commits (CLI / skills / docs+tests) plus a pre-merge STATUS prep commit, merged on `main` as `6462c3a` (true merge — three intermediate commits preserved, not squashed). The mapping back to the `05-skills-cli.md` §8 deliverable checklist:

| Deliverable | Plan section | Status | Notes |
|---|---|---|---|
| `skills/setup-workflow/`, `setup-bridge/`, `setup-state-machine/`, `setup-telemetry/`, `setup-agent-backend/`, `migrate-from-symphony/` — each with `SKILL.md`, `templates/` (Liquid only), optional `validators/` | §2 | ✅ merged | 33 files across the six folders. Each `SKILL.md` carries YAML front matter (`name` / `description` / `version`) enforced by `every_skill_md_has_required_front_matter`. Locked distribution model per proposal §5: Sinfonia ships the skills; auto-install into AI-tool-specific dirs is owned by each vendor. |
| Generated templates pass the unguarded-`issue.fields` grep check | §8 box 2 | ✅ merged | The `setup-state-machine` prompt templates use `\| default:` on every `{{ issue.fields.X }}` reference by construction. `state_machine_prompts_have_no_unguarded_issue_fields` runs the §8 regex `\{\{[^}]*issue\.fields\.[^\|]*\}\}` against the templates; empty match set. |
| `sinfonia --check` with documented exit codes | §3.1 | ✅ merged | `crates/sinfonia/src/check.rs`. Exit codes: 0 ok / 2 yaml / 3 schema / 4 template / 5 tracker-auth. Loads workflow, runs `validate_for_dispatch`, renders every prompt template (workflow body + per-state overrides) against a stub Issue. 6 unit tests covering every exit code + the ok path + a second template-error variant. |
| `sinfonia init` subcommand with the REPL flow | §3.2 | ✅ merged | `crates/sinfonia/src/init_repl.rs`. `inquire`-driven REPL mirrors `setup-workflow` step-by-step: tracker kind, project slug, endpoint/email, active/terminal states, agent backend, workspace root. Linear flow with abort-on-error (plan §7 #2). Live tracker probe deferred to the skill version. 5 unit tests covering Linear/Jira-Cloud/Jira-Server rendering + render→parse→validate roundtrip. |
| `sinfonia-bridge --self-test` with the canonical output format | §3.3 | ✅ already shipped (P1-G) | `crates/sinfonia-bridge/src/selftest.rs` carries the PASS/FAIL/SKIP format. Phase 4 wired the Jira probe through it; Phase 5 had no work on the bridge side. |
| `docs/SKILLS.md` with the vendor pointer table | §4 | ✅ merged | Includes the six-skill catalog, the four-step recommended invocation order, the cross-vendor install table (Claude Code / OpenCode / Codex CLI), the skill contract (front-matter keys), and the strict-Liquid invariant section. |
| Tests per §5.1 and §5.2 | §5 | ✅ merged | §5.1 unit tests inlined per module (`check::tests`, `init_repl::tests`). §5.2 integration suite at `crates/sinfonia/tests/skills_integration.rs`: 6 tests covering skill presence, front-matter parsing, Liquid syntax, unguarded-field grep, validator executability, and template→`validate_for_dispatch` roundtrip. |
| Manual verification recorded in `docs/v0.3-plan/05-skills-VERIFY.md` | §5.3 | ⏳ matrix recorded, runs pending | The VERIFY doc captures the seven-row verification matrix (V-1 through V-7) with status; runs against a real Claude Code + Linear + sandbox GitHub + Honeycomb path are pending before `v0.3.0-alpha.x` tag. |
| CHANGELOG entry | §8 | ✅ merged | `[Unreleased]` gains five new bullets under Added covering the six skills, the two CLI extensions, the state-machine prompt invariant, and `docs/SKILLS.md`. |

### Phase 6 sub-task status

Phase 6 shipped as one PR (#13) with one combined commit covering all nine sub-tasks (rename → production Dockerfile → bake → production compose → CI overlay + smoke fixtures → publish workflow → smoke scripts → docs → STATUS bump), merged on `main` as `8b9ac66` (true merge). The mapping back to the `06-docker.md` §9 deliverable checklist:

| Deliverable | Plan section | Status | Notes |
|---|---|---|---|
| Refactored `Dockerfile` with all six build targets | §3 | ✅ merged | Single multi-stage file at the repo root: shared `builder` (Rust 1.78-bookworm) + `sinfonia-base` parent (Debian slim + bash / git / curl / gh) → `sinfonia`, `sinfonia-with-claude-code`, `sinfonia-with-codex`, `sinfonia-with-opencode`, `sinfonia-all-agents`. `sinfonia-bridge` parents directly off `debian:bookworm-slim` + `ca-certificates` so the standalone-bridge image stays small (no agent / git / gh dependencies — HTTPS-only). |
| `docker-bake.hcl` defining targets, registry, platforms | §3.2 | ✅ merged | One target stanza per image with `linux/amd64,linux/arm64` platform default. `tags(name)` HCL helper fans `${VERSION}` out into three tags per image (`:VERSION` / `:VERSION_MINOR` / `:latest`) on real semver, single `:dev` tag on the placeholder. `VERSION_MINOR` derived via `regex_replace`. |
| Existing `Dockerfile` renamed to `Dockerfile.dev`; existing `docker-compose.yml` renamed to `docker-compose.dev.yml` | §3 / §4 | ✅ merged | Verbatim moves (no semantic edits); dev compose's inner `build.dockerfile` updated from `Dockerfile` to `Dockerfile.dev`. |
| New `docker-compose.yml` showing the production topology | §4 | ✅ merged | Four services: `sinfonia` (`sinfonia-all-agents:0.3.0`), `sinfonia-bridge`, `otel-collector` (`otel/opentelemetry-collector-contrib:0.103.0`), `postgres:16-alpine`. Read-only user-credential bind mounts for `~/.claude` / `~/.codex` / `~/.opencode`. Phase 3 telemetry schema applied to Postgres via `docker-entrypoint-initdb.d`. |
| `.github/workflows/docker-publish.yml` building + pushing all six on tag | §5 | ✅ merged | Triggers on `v*` tags + manual dispatch. QEMU + Buildx, GHCR login via `GITHUB_TOKEN`, `docker buildx bake --push`, `imagetools inspect` digest dump per image, then `docker-smoke.sh` + `docker-compose-smoke.sh` in the same job. |
| Trivy scan with `severity: CRITICAL,HIGH` fail policy | §5.2 | ✅ merged | Separate `scan` job with a matrix per image; `exit-code: 1` and `ignore-unfixed: true`. SARIF uploaded to the Security tab via `github/codeql-action/upload-sarif@v3` for visibility on LOW/MEDIUM. |
| `tests/docker-smoke.sh` invoked after publish | §7.2 | ✅ merged | Per-image `docker run --rm <img> --help` plus `sinfonia --check` against `WORKFLOW.example.md` (real config, exercises the YAML + Liquid path) plus `sinfonia-bridge --self-test` against `tests/fixtures/BRIDGE.smoke.md` with `|| true` (creds are fake; assertion is "binary doesn't crash on entry"). |
| `tests/docker-compose-smoke.sh` invoked in CI with `docker-compose.ci.yml` overlay | §7.3 | ✅ merged | Brings the full stack up under the CI overlay, polls `:8080/api/v1/state` and `:8081/health` on a 30 s bounded retry, tears down with `down -v --remove-orphans` on EXIT trap. The overlay's `!reset` on the `volumes:` list is mandatory — additive merging would otherwise still try to bind-mount `~/.claude` et al. |
| `tests/fixtures/WORKFLOW.smoke.md` (+ `BRIDGE.smoke.md`) stubs | §7.3 | ✅ merged | Minimal configs that pass `--check`. WORKFLOW uses `kind: linear` with fake creds — the orchestrator's `candidate fetch failed; skipping dispatch` path swallows the resulting tracker error and `/api/v1/state` still binds. There is no "stub" tracker kind in Sinfonia, so the plan-doc wording "stub tracker config that returns no candidate issues" maps to "real kind, fake creds, orchestrator-resilient." |
| `README.md` "Docker" section updated | §6 | ✅ merged | New § after `## Run` covering the image matrix table, production-vs-dev compose split, and the `docker buildx bake` build path. The "What's new in v0.3" line for Phase 6 is added as well. |
| Manual verification recorded in `docs/v0.3-plan/06-docker-VERIFY.md` | §7.4 | ✅ matrix recorded, runs pending | The VERIFY doc captures six plan-doc deltas surfaced at impl time plus the seven-row verification matrix (build, sizes, `--help`, daemon `--check`, bridge `--check`, end-to-end ticket cycle, Trivy pass). Real-release runs pending tag `v0.3.0-rc.x`. |
| `docs/DEPLOYMENT.md` referencing the image matrix | §9 | ⏳ deferred to Phase 7 | Explicitly Phase 7's responsibility per `06-docker.md` §9; not in Phase 6 scope. |
| CHANGELOG entry | §9 | ✅ merged | `[Unreleased]` gains three Added bullets (image matrix, compose smoke harness, `docker-bake.hcl`) plus one Changed bullet (the `Dockerfile.dev` / `docker-compose.dev.yml` rename). |

### Phase 7 sub-task status

Phase 7 ships as one PR (in flight as of this commit, on branch `v0.3-phase-7-docs`) with three intermediate commits walking the deliverables in a reviewable order (SPEC + stubs → new long-form guides → README + CHANGELOG + CI). The mapping back to the `07-docs.md` §13 deliverable checklist (13 boxes) is in `docs/v0.3-plan/07-docs-VERIFY.md`. Headline: twelve boxes ✅ landed, one (the manual readthrough by someone uninvolved) deferred per the same pattern as Phase 4/5/6's VERIFY rows — to run against `v0.3.0-rc.x` before the GA tag.

Per-deliverable mapping is recorded in `07-docs-VERIFY.md`'s top table; this section only records the high-level commit ordering and the deferred items:

| Cluster | Status | Notes |
|---|---|---|
| P7-A — SPEC + foundational stubs | ✅ landed (`4efa67f`) | `docs/SPEC.md` §11.5/§11.6/§11.7/§18.2 + BRIDGE.example.md + CONTRIBUTING.md + SKILLS.md + JIRA-SCREEN-SCHEME.md. `sinfonia-bridge BRIDGE.example.md --check` continues to pass. |
| P7-B — new long-form guides | ✅ landed (`4a4e344`) | `docs/DEPLOYMENT.md` (NEW) + `docs/CLIENT_SETUP.md` (NEW) + `docs/MIGRATION-v0.2-to-v0.3.md` (NEW) + `WORKFLOW.example.md` updates. ~1500 LOC added. `sinfonia --check WORKFLOW.example.md` continues to pass with `LINEAR_API_KEY` set (placeholder behaviour unchanged). |
| P7-C — README rewrite + CHANGELOG + docs CI | ✅ landed (`4d7e69d`) | `README.md` rewrite, `CHANGELOG.md` `[Unreleased]`→`[0.3.0]` promotion, `.github/workflows/docs.yml` (NEW) + `.markdownlint-cli2.yaml` (NEW) + `lychee.toml` (NEW) + `scripts/check-doc-code-blocks.sh` (NEW). Local verification: markdownlint-cli2 18 files / 0 errors, lychee `--offline --exclude-path docs/v0.3-plan` 141 links / 0 errors, code-block validator ok. |
| §11.4 spec-conformance test (`tests/spec_conformance.rs::§18.2_extensions_implemented`) | ⏳ deferred to v0.3.1 | Most §18.2 bullets ARE implemented in v0.3.0, but writing the cross-reference test mechanically would require parsing SPEC.md prose into a structured assertion catalog — out of scope for a docs phase. Rationale in `07-docs-VERIFY.md` §1.5. |
| §11.5 manual readthrough by someone uninvolved in v0.3 | ⏳ deferred | Per user direction at start of Phase 7. Pre-v0.3.0-rc.x. Same pattern as Phase 4/5/6's pending verification matrices. |
| `examples/runbook.md` / `examples/incident-response.md` | ⏳ deferred indefinitely | Plan §12 open question 5: "promote when we have real-world content." Phase 7 doesn't ship speculative runbook content; CLIENT_SETUP.md §"Handoff to the operating team" runbook-template headers are the v0.3.0 answer. |
| Docs site (v0.4+) | ⏳ deferred to v0.4+ | Plan §12 open question 1: "GitHub-rendered Markdown is good enough for v0.3." Decision recorded in `07-docs-VERIFY.md` §4. |
| Bridge `--once` single-shot mode for Topology 4 | ⏳ deferred to v0.3.1 | DEPLOYMENT.md Topology 4 documents the v0.3.0 alternative (POST to the existing `/webhook` handler from within the Action, then `kill` the bridge). Mentioned in CHANGELOG `Deferred to v0.3.1`. |

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` on `main` → **183 tests pass, 0 failures** (Phase 3 final baseline).
- `cargo test --workspace --no-fail-fast` on `main` post-#11 → **210 tests pass, 0 failures** (+27 over Phase 3):
  - **47** sinfonia unit tests (up from 44 by +3 across the `telemetry::tenant` helpers — pre-existing on the branch).
  - **13** `spec_conformance.rs` integration tests (was 14 — adjustment from Phase 3 settlement; still 0 failures).
  - **25** sinfonia-tracker unit tests (up from 7 by +18: 8 new Jira tests in `jira::tests`, 11 new ADF tests in `jira_adf::tests`).
  - **6** sinfonia-tracker integration tests (`tests/jira_wiremock.rs`, NEW in Phase 4).
  - **110** sinfonia-bridge unit tests (up from 107 by +3: 3 new Jira-config tests in `config::tests::rule2_jira_*` — replacing the single deferral test).
  - **9** sinfonia-bridge integration tests (`tests/bridge_e2e.rs`) — unchanged (Linear-only path, no Jira mirror per VERIFY §3.1).
- `cargo test --workspace --no-fail-fast` on `main` post-#12 → **227 tests pass, 0 failures** (+17 over Phase 4):
  - **47** sinfonia unit tests — unchanged.
  - **11** sinfonia binary unit tests (NEW in Phase 5): 6 in `check::tests` (one per `--check` exit code + the ok path + a second template-error variant) + 5 in `init_repl::tests` (Linear/Jira-Cloud/Jira-Server rendering, CLI-backend-omits-llm-block, comma-list helper).
  - **13** `spec_conformance.rs` integration tests — unchanged.
  - **6** `skills_integration.rs` integration tests (NEW in Phase 5): skill-folder presence, front-matter validity, Liquid template syntax, unguarded-`issue.fields` grep, validator executability, and the `setup-workflow` template→`validate_for_dispatch` roundtrip.
  - **25** sinfonia-tracker unit tests — unchanged.
  - **6** sinfonia-tracker integration tests — unchanged.
  - **110** sinfonia-bridge unit tests — unchanged.
  - **9** sinfonia-bridge integration tests — unchanged.
- `cargo test --workspace --no-fail-fast` on `main` post-#13 → **227 tests pass, 0 failures** (unchanged from Phase 5). Phase 6's deliverables are infrastructure (Dockerfile, compose, bake, publish workflow) plus shell smoke harnesses (`tests/docker-smoke.sh` + `tests/docker-compose-smoke.sh`); no Rust tests added or modified. `shellcheck tests/docker-smoke.sh tests/docker-compose-smoke.sh` is clean.
- `docker compose -f docker-compose.yml config --quiet` → no errors (production topology parses).
- `docker compose -f docker-compose.yml -f docker-compose.ci.yml config --quiet` → no errors; the `!reset` overlay clears the user-credential bind mounts correctly.
- `VERSION=0.3.0 docker buildx bake --print` → all six targets emit the expected three-tag fan-out (`:0.3.0` / `:0.3` / `:latest`).
- `./target/release/sinfonia --check tests/fixtures/WORKFLOW.smoke.md` → `ok` (exit 0). `./target/release/sinfonia-bridge tests/fixtures/BRIDGE.smoke.md --check` → `ok` (exit 0). Both smoke fixtures parse and validate before they ever hit a container.
- `cargo run -p sinfonia-bridge -- BRIDGE.md --check` → `ok` (exit 0) on valid, descriptive error (exit 1) on invalid.
- `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` → `ok` (exit 0) with no environment variables set. The example doc is its own CI gate — when CI lands for the bridge crate, this command catches schema drift between the parser and the example.
- `cargo run -p sinfonia-bridge -- BRIDGE.md --self-test` → one labelled `PASS`/`FAIL`/`SKIP` line per check; exit code = number of `FAIL` lines (SKIPs don't count). App-mode token-mint + REST round-trip now covered by `bridge_e2e.rs` scenario 8.
- `GET /health` returns `{"service":"sinfonia-bridge","status":"ok","tenant_id":<...>,"tracker":"linear"}`.
- `POST /webhook` flow (now exercised end-to-end against wiremock for every §9.2 scenario):
  - Signed `pull_request opened` → 202 + row in `pr_ticket_map`.
  - Same `delivery_id` redelivered → 200 `{"status":"duplicate"}`, no DB mutation.
  - Wrong HMAC → 401, no DB mutation.
  - Signed `check_suite completed` with a mapped PR → 202 + per-PR `CiOutcome` (green / red / cap_hit / pending / no_mapped_pr) returned in the JSON body. PAT-mode and App-mode auth paths both wired through `OctocrabGhOps` / `AppModeGhOps` against the mock.
- `scripts/verify-workspace-move.sh` → no longer applicable post-merge (the script's purpose was to gate the workspace-move commit; it can stay in-repo as a one-shot artifact).

---

## 2. What's next: v0.3.0 release prep

Phases 1–6 are merged to `main`; Phase 7 has landed on branch `v0.3-phase-7-docs` and is ready for review. **Once Phase 7 merges, the next gate is the v0.3.0 release itself.** The remaining steps:

1. **Review + merge Phase 7.** Three reviewable commits (P7-A SPEC + stubs; P7-B new guides; P7-C README + CHANGELOG + CI). Standard PR workflow against `main`. Once green, squash or merge — the three commits are independent enough that either choice reads cleanly.
2. **Run the §11.5 manual readthrough.** Per `07-docs-VERIFY.md` V-5, hand the doc set to someone uninvolved in v0.3 implementation and have them (a) read the README and want to try it, (b) run the tutorial in under 30 minutes, (c) follow `DEPLOYMENT.md` to a working deployment in under a day, (d) hand `CLIENT_SETUP.md` to a security reviewer without embarrassment. Findings go into a follow-up doc-patch commit; no STATUS bump on findings (they're considered part of the same Phase 7 deliverable).
3. **Exercise the three carried-forward manual-verification debts** before tag:
   - `docs/v0.3-plan/01-bridge-mvp-VERIFY.md` (NOT yet written; plan §9.3 calls for end-to-end verification against a real Linear project + sandbox GitHub repo).
   - `docs/v0.3-plan/02-opencode-VERIFY.md` §5.3 (real OpenCode + Linear).
   - `docs/v0.3-plan/03-telemetry-VERIFY.md` §3 (Collector + Postgres + drive one cap-hit cycle).
   - The Phase 4 / 5 / 6 VERIFY matrices have rows that fall into the same "pending real-world runs before tag" bucket. Walk DEPLOYMENT.md and CLIENT_SETUP.md while doing these — the prose has to be accurate, and the only way to know is to follow it.
4. **Fill the date placeholder.** `CHANGELOG.md` carries `## [0.3.0] — 2026-MM-DD`. Replace at tag time.
5. **Fill the `Fixed:` section** in the `[0.3.0]` block with any fix-class commits between Phase 7 merge and the v0.3.0 tag (currently empty by design — Phase 7 is the last planned phase).
6. **Tag and push.** `git tag -a v0.3.0 -m "v0.3.0"`, `git push --tags`. `.github/workflows/docker-publish.yml` triggers on `v*` tags and pushes the six images to `ghcr.io/o-side-systems/{sinfonia,sinfonia-bridge,sinfonia-with-claude-code,sinfonia-with-codex,sinfonia-with-opencode,sinfonia-all-agents}` with the three-tag fan-out (`:0.3.0`, `:0.3`, `:latest`).

### v0.3.0 → v0.3.1 follow-up watch list

Items v0.3.0 explicitly defers:

- **OTel metrics layer** (Phase 3 §6, 9 instruments). Rationale: `examples/telemetry/queries/*.sql` reads span attributes from the `events` table, so the plan §8.2 exit criteria are met span-derived. Per-metric span-derived equivalent in `03-telemetry-VERIFY.md` §2.1.
- **Wire-level integration tests** for the typed event channel + budget cap-hit (`tests/telemetry_e2e.rs`, `tests/budget_e2e.rs`). Algorithmic surface is pinned by the unit suite; manual verification covers wire-level concerns. Cross-reference table per concern in `03-telemetry-VERIFY.md` §2.5.
- **`tests/spec_conformance.rs::§18.2_extensions_implemented`** — mechanical cross-check that every §18.2 bullet maps to working code. Most do; the test would require parsing SPEC.md prose into a structured catalog. Rationale in `07-docs-VERIFY.md` §1.5.
- **Bridge `--once` single-shot mode** for Topology 4 (GitHub Actions bridge). DEPLOYMENT.md Topology 4 documents the v0.3.0 alternative.

### Other follow-up work (not blocking v0.3.0)

- **STATUS doc retire path.** Current file length is ~1000 lines. The natural cut point is between v0.3 and v0.4 milestones — re-evaluate at v0.3.0 release tag.
- **Phase 7 follow-up doc patches.** Any findings from the §11.5 manual readthrough land as small commits to `main` after merge; no fresh phase number.

---

## 2.5 (Historical) What was queued before Phase 7

Phases 1–6 are merged to `main`. The next pickup was **Phase 7 — documentation update** (`docs/v0.3-plan/07-docs.md`). Phase 7's stated dependencies were all of Phases 1–6 — the doc set describes the bridge (P1), OpenCode backend (P2), telemetry + budget enforcement (P3), Jira support (P4), setup skills + CLI extensions (P5), and the Docker image matrix (P6). All resolved. **Once Phase 7 is done, v0.3.0 is releasable.**

### Scope (per `07-docs.md`)

The user explicitly asked for "ample documentation at the quality one would expect for an open source project." Phase 7 sets that bar (`07-docs.md` §intro):

- Every new concept has a one-paragraph "why this exists, in plain language" alongside the reference material.
- Every new config field has a documented default, type, and example value.
- Every recommended pattern (state machine, failure categorization, budget caps) has a worked example you can copy.
- Every doc file states its audience and prerequisites in the first 100 words.
- No content lives only in proposal-doc form — the doc set is self-contained.

Estimated size: **~3,000 lines of Markdown** across new and updated files. The eleven-file deliverable list (`07-docs.md` §1):

| File | Action | Source of stub (if any) |
|---|---|---|
| `README.md` | major rewrite of the front page | Pre-existing |
| `CHANGELOG.md` | finalize the `[Unreleased]` block as `[0.3.0] — 2026-MM-DD` | Pre-existing, with `[Unreleased]` entries from every prior phase |
| `docs/SPEC.md` | spec extensions (§11.5 clarification, §11.6 bridge contract, new §11.7 custom-field discovery, §18.2 recommended-extensions table) | P1 / P3 / P5 added §11.6.11 + §11.6.12 + §18.2 entries; Phase 7 finalizes the prose and adds §11.7 |
| `docs/SKILLS.md` | polish the Phase 5 stub | P5 (the cross-vendor pointer table + skill contract) |
| `docs/DEPLOYMENT.md` | new file — four reference topologies + credential model + observability + scaling + backup + upgrades | None |
| `docs/CLIENT_SETUP.md` | new file — security posture + GitHub App vs PAT + budget controls + audit trail + vendor evaluation worksheet | None |
| `docs/JIRA-SCREEN-SCHEME.md` | polish the Phase 4 stub | P4 (the bridge-key → display-name table and the manual-bind walkthrough) |
| `docs/MIGRATION-v0.2-to-v0.3.md` | new file — required changes, optional upgrades, breaking changes, compatibility notes | None |
| `WORKFLOW.example.md` | refresh examples to cover OpenCode + state machine + telemetry | Pre-existing |
| `BRIDGE.example.md` | polish the Phase 1 stub (the file already parses under `--check` with no env vars) | P1 |
| `CONTRIBUTING.md` | update for workspace layout + new test commands | Pre-existing |

Plus a `tests/spec_conformance.rs` extension that asserts every §18.2 recommended-extension bullet is implemented in code (per plan §11.4 — prevents `docs/SPEC.md` and the codebase from drifting).

### What's already prepared for Phase 7 (inherited from Phases 1–6)

- **CHANGELOG `[Unreleased]` is already populated.** Every prior phase added its own block; Phase 7's CHANGELOG work is mostly editorial — re-grouping the bullets under Added / Changed / Fixed / Migration / Known Limitations, renaming `[Unreleased]` to `[0.3.0] — 2026-MM-DD`, and confirming nothing was missed (the Phase 6 commit's CHANGELOG entry, for example, is already there).
- **README "What's new in v0.3 (preview)" stubs are in place.** P1-I shipped the stub; P2 / P3 / P4 / P5 / P6 each added a paragraph. Phase 7's job is the **rewrite** described in `07-docs.md` §2.2 — restructure the front page so an operator can answer "can a team adopt this?" / "can I deploy without hand-editing YAML?" / "what's new in v0.3?" in the first screen, then point at `DEPLOYMENT.md` for the long version.
- **The §11.6 bridge contract is already drafted.** P1-I (the original §11.6 draft), P3 (added §11.6.11 typed event channel + §11.6.12 budget enforcement), P4 (rewrote §11.6.2 Jira bullet for actual write-surface coverage). Phase 7's §11.6 work is editorial polish, not new prose.
- **The §18.2 recommended-extensions table is partially populated.** P2 (OpenCode), P3 (OTel emission), P5 (extension surface for skills) all added entries. Phase 7 adds the §18.2 catalog header + the conformance test that asserts every row maps to working code.
- **`BRIDGE.example.md` already parses under `--check` with no env vars.** P1-I's invariant — schema drift between the parser and the example fails CI on the day someone adds the bridge job to GitHub Actions. Phase 7 doesn't change the schema; it polishes the prose comments inside the file.
- **`docs/JIRA-SCREEN-SCHEME.md` already covers the manual-bind walkthrough.** P4 shipped it as the fallback path when the bridge's best-effort screen-scheme bind hits a permissions error. Phase 7 polishes the audience/prerequisites paragraph and cross-links from `DEPLOYMENT.md` (Jira topology).
- **`docs/SKILLS.md` already has the vendor pointer table.** P5-C shipped it. Phase 7 audits each install snippet (Claude Code / OpenCode / Codex CLI) against the latest vendor docs and adds a worked example for each skill's `inputs:` schema.
- **`examples/telemetry/` is real, not aspirational.** P3 shipped the Collector config + Postgres schema + three dashboard SQL queries + a wiring README. Phase 7's `DEPLOYMENT.md` "Observability" section points at it rather than restating it inline.
- **The image matrix table is in `README.md`.** P6 added it. Phase 7 reorganizes it into `DEPLOYMENT.md` (the long form, with size measurements at release time and the per-topology image recommendations) and replaces the README's table with a shorter pointer.

### Phase 6 → Phase 7 hand-off pointer

Read these in this order before starting Phase 7:

1. `docs/v0.3-plan/07-docs.md` — the Phase 7 plan. Source of truth for the eleven-file deliverable list, the README structure outline (§2.2), the `DEPLOYMENT.md` outline (§4.1), the `CLIENT_SETUP.md` outline (§5.1), and the test plan (§11). The §13 deliverable checklist is the merge gate.
2. `docs/v0.3-plan/00-overview.md` "Cross-cutting concerns" — Phase 7 has to coherently document all four cross-cutting concerns (per-tenant routing, custom-field semantics, secret handling, version skew) in the right places (mostly `DEPLOYMENT.md` for routing + secrets, `SPEC.md` §11.6 for custom-field semantics, `MIGRATION-v0.2-to-v0.3.md` for version skew).
3. Re-skim every prior `*-VERIFY.md` (`02`, `03`, `04`, `05`, `06`): each one records plan-doc deltas and follow-up watch-list items that Phase 7's prose must accurately reflect. The "what we actually shipped" voice in those files is the source of truth, NOT the original phase plan.
4. Re-skim the README, CHANGELOG, `BRIDGE.example.md`, `WORKFLOW.example.md`, `docs/SPEC.md`, `docs/SKILLS.md`, `docs/JIRA-SCREEN-SCHEME.md` to know what's already there before rewriting.
5. **The proposal that started v0.3:** `/Users/brettlee/Downloads/sinfonia-change-proposal.md` (read-only reference; not in-repo). Phase 7's README rewrite has to deliver on the proposal's "team-grade orchestrator, not just a single-user daemon" framing.

Then `git checkout -b v0.3-phase-7-docs` off `main` (227-test baseline; Phase 7 adds the §18.2 conformance test which will bump the count, but no code paths beyond that) and start there.

### Phase 7 follow-up watch list

Items `07-docs.md` surfaces that Phase 7 should explicitly resolve before merge:

- **CI for the bridge crate's `--check` example gate.** Item carried forward from Phase 1 — `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` should fail CI when schema drifts. The `06-docker.md` Phase 6 work didn't wire a Rust CI job; Phase 7 owns adding `.github/workflows/ci.yml` alongside the existing `docker-publish.yml`. Includes link-rot + markdown lint + code-block syntax checks per `07-docs.md` §11.1 / §11.2 / §11.3.
- **`docs/SPEC.md` §18.2 conformance test.** Per `07-docs.md` §11.4: every §18.2 recommended-extension bullet must map to a working implementation. The `tests/spec_conformance.rs` file already exists; Phase 7 adds the §18.2 assertion(s). This is the only Rust code Phase 7 ships.
- **Manual readthrough by someone uninvolved in v0.3 implementation.** `07-docs.md` §11.5 calls for a fresh reader to (a) read the README and want to try it, (b) run the tutorial in under 30 minutes, (c) follow `DEPLOYMENT.md` to a working deployment in under a day, (d) hand `CLIENT_SETUP.md` to a security reviewer without embarrassment. Findings go into a follow-up doc patch, not blocking initial merge — but `v0.3.0` shouldn't tag until at least one external reader has done the readthrough.
- **The `examples/runbook.md` / `examples/incident-response.md` question.** `07-docs.md` §12 open question 5: "Do we promote these? Yes — but only when we have real-world content, not speculative content." Resolution: defer until post-v0.3 real-world deployments produce material. Phase 7 does NOT ship speculative runbooks.
- **Docs site (v0.4+).** `07-docs.md` §12 open question 1: GitHub-rendered Markdown is good enough for v0.3. A `docs.rs`-style hosted site is a v0.4+ project. Document the decision in `07-docs-VERIFY.md` so a future reader doesn't relitigate.

### Three pre-existing manual-verification debts that should clear before tagging `v0.3.0`

Carried forward from prior phases and called out here because **Phase 7 is the final gate before `v0.3.0`**:

- `docs/v0.3-plan/01-bridge-mvp-VERIFY.md` (NOT yet written; plan §9.3 calls for end-to-end verification against a real Linear project + sandbox GitHub repo before declaring v0.3.0-alpha.1 shippable).
- `docs/v0.3-plan/02-opencode-VERIFY.md` (written in P2; manual run against a real OpenCode install + Linear project still pending).
- `docs/v0.3-plan/03-telemetry-VERIFY.md` (written in P3; the §3 protocol — bring up Collector + Postgres + drive one ticket cycle + verify the $0.01 cap-hit transition — pending).

The 227 automated tests cover the logic; manual verification confirms the auth/credential dances + the wire-level cap-hit transition against real services. Phase 7's `DEPLOYMENT.md` walkthrough is a natural place to flush these — the prose has to be accurate, and the only way to know it's accurate is to follow it on a real install.

The historical Phase 4 narrative kept below for the hand-off record:

### Scope (per `04-jira-bridge.md`)

Phase 4 is the smallest remaining phase: **~200 LOC of Rust + ~150 LOC of tests + ~150 LOC of docs**. The work pattern mirrors Phase 1's Linear adapter, but the Jira side is more straightforward because Jira has native custom fields (no marker-comment dance like Linear). Headline deliverables:

1. **Jira REST adapter completion.** Fill in the five default-impl methods (`transition_issue`, `read_custom_field`, `write_custom_field`, `ensure_custom_field`, `post_comment`) on `JiraTracker`. Use `/rest/api/3/issue/{id}/transitions` for state moves, `/rest/api/3/field` for custom-field provisioning, and `customfield_NNNNN` IDs for reads/writes.
2. **ADF rendering for `post_comment`.** Jira accepts comment bodies as Atlassian Document Format (ADF), a nested JSON shape (`doc → content[paragraph] → content[text]`). Plan default: hand-roll the minimal subset of ADF for the bridge's failure-summary + cap-hit comments (no third-party `markdown-to-adf` dep). Plan §3.5 open question.
3. **Custom-field discovery + creation.** Jira's per-field IDs are workspace-specific. `ensure_custom_field(schema)` looks up an existing field by display name; creates it if absent. Cached on the JiraTracker instance so repeat calls during a single bridge process don't re-hit the discovery endpoint.
4. **Bridge config validation.** `BRIDGE.md` parsing already accepts `tracker.kind: jira`; today validation refuses it with "not supported until Phase 4." Remove that rejection. Add Jira-specific validation: require `tracker.email` (for Basic auth alongside the API token) and `tracker.endpoint` (full base URL, not just the GraphQL path).

### What's already prepared for Phase 4 (inherited from Phases 1–3)

- **The `IssueTracker` trait extension is intact.** The five bridge-write methods all return `Error::Other("not supported for this tracker")` for Jira. Phase 4 replaces those bodies; no signature changes needed.
- **The bridge's tracker-write call sites are tracker-agnostic.** `feedback/transition.rs`, `feedback/budget.rs`, and `events.rs` all call through the trait — they'll work against a real Jira impl without modification.
- **`CustomFieldValue::String` round-trips through Jira ADF.** Cost values from Phase 3's budget pipeline (`CustomFieldValue::String("8.23")`) write to a text-typed Jira customfield without precision loss. The Linear marker-comment carries everything as text by construction; Jira's text customfield is the equivalent. No Phase 3 patch is needed on the budget side.
- **`WELL_KNOWN_FIELDS` already lists the v0.3 budget keys.** Templates rendered from the Jira-side `Issue.fields` map work the same way as Linear's (they go through `crates/sinfonia/src/template.rs`, which pre-seeds `Null` for absent well-known keys per STATUS §5.2).
- **`BRIDGE.example.md` carries the Jira block as commented-out scaffolding.** P1-D shipped the schema (`tracker.kind: jira`, `tracker.email: $JIRA_EMAIL`, etc.); Phase 4 removes the validation rejection + uncomments the example block.

### Phase 3 → Phase 4 hand-off pointer

Read these in this order before starting Phase 4:

1. `docs/v0.3-plan/04-jira-bridge.md` — the Phase 4 plan. Source of truth for the Jira REST endpoint shapes, the ADF format, the custom-field discovery flow, and validation rules.
2. `docs/v0.3-plan/00-overview.md` "Cross-cutting concerns" — Phase 4 sits at the per-tracker abstraction boundary; the cross-cutting notes call out custom-field semantics (concern B) and Jira's ADF particulars.
3. `crates/sinfonia-tracker/src/jira.rs` — current implementation (mostly default impls). The candidate-fetch path already works; Phase 4 fills in the write surface.
4. `crates/sinfonia-tracker/src/linear.rs` — reference implementation for the same trait, especially the marker-comment dance vs. Jira's native customfield approach. Useful as a comparison point but Phase 4 does NOT reproduce the marker-comment design.
5. `crates/sinfonia-bridge/src/config.rs` Rule 2 (the `tracker.kind: jira` rejection at line ~632). Phase 4 removes this gate.
6. **Vendor docs (per the Phase 2 §5.10 + Phase 3 §5.13 lesson):** before writing code, verify the actual Jira REST API surface against `developer.atlassian.com/cloud/jira/platform/rest/v3/` — specifically the `/issue/{id}/transitions` and `/field` endpoints + the ADF schema. Capture deltas in `docs/v0.3-plan/04-jira-VERIFY.md` (new) as `03-telemetry-VERIFY.md` §1 did for Phase 3.

Then `git checkout -b v0.3-phase-4-jira` off `main` (183-test baseline) and start there.

**Update (this commit):** the above is now the historical record. Phase 4 merged via #11 (`17f5213`) with 210/210 workspace tests green on `main`. The two plan-doc deltas surfaced during impl are captured in `docs/v0.3-plan/04-jira-VERIFY.md` §1 (field-discovery endpoint, identification by display name).

### Phase 4 follow-up watch list

Items the plan doc surfaces that Phase 4 should explicitly resolve before merge:

- **ADF library vs hand-roll.** Plan §3.5 open question. Default: hand-roll the minimal subset. Re-verify at impl time whether a small `markdown-to-adf` crate exists with a sensible license + maintainer story — if so, the bridge prose templates would benefit. Otherwise the hand-roll is ~50 LOC of "doc/paragraph/text" wrapping.
- **Custom-field discovery rate-limiting.** Jira's `/field` endpoint paginates. The bridge's `ensure_custom_field` runs at startup for each well-known field, so a fresh install hits the endpoint ~7 times in quick succession. Cache the full field list once after the first call; subsequent ensures hit the cache.
- **Manual verification protocol.** Create `docs/v0.3-plan/04-jira-VERIFY.md` covering: ADF rendering against a real Jira Cloud instance, custom-field discovery against a non-trivial field count, transition flow against a project workflow that includes both visible-to-all and role-gated transitions.

### Other follow-up work (not blocking Phase 4)

These items surfaced during Phases 1–3 but were not in scope. They are not blockers; surface them at the right time:

- **OTel metrics layer (v0.3.1).** Plan §6 listed 9 instruments; Phase 3 shipped 12 spans instead and proved the §8.2 dashboards work span-derived. Lands as a focused v0.3.1 patch with the `MeterProvider` setup + per-site instrumentation in both binaries. Cross-reference table per metric in `03-telemetry-VERIFY.md` §2.1.
- **Wire-level integration tests for the typed event channel + budget cap-hit (v0.3.1).** Deferred per `03-telemetry-VERIFY.md` §2.5. The algorithmic surface is pinned by the unit suite; the wire concern is covered by manual verification per Phase 3 plan §9.3 — and by the existing P1-H wiremock harness for the tracker-write side. Promote when a regression surfaces or the manual protocol fires.
- **CI for the bridge crate.** Wire `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` into CI so schema drift between `config.rs` and the example doc fails the build. Listed in §1 as one of the Phase 1 deliverables' implicit guarantees but not currently enforced by a CI job — add it when CI is being touched anyway.
- **Manual verification (across phases).** Three docs are pending real-world runs:
  - `docs/v0.3-plan/01-bridge-mvp-VERIFY.md` (NOT yet written; plan §9.3 calls for end-to-end verification against a real Linear project + sandbox GitHub repo before declaring v0.3.0-alpha.1 shippable).
  - `docs/v0.3-plan/02-opencode-VERIFY.md` (written in P2; manual run against a real OpenCode install + Linear project still pending).
  - `docs/v0.3-plan/03-telemetry-VERIFY.md` (written in P3; the §3 protocol — bring up Collector + Postgres + drive one ticket cycle + verify the $0.01 cap-hit transition — pending).
  All three should be exercised before tagging `v0.3.0-alpha.x` to an actual release. The 183 automated tests cover the logic; manual verification confirms the auth/credential dances + the wire-level cap-hit transition against real services.
- **STATUS doc retire path.** This file's "Phase 1 + 2 + 3" framing accretes per merged phase. Current length (~530 lines) is still manageable; the natural cut point is between v0.3 and v0.4 milestones, not between phases within v0.3. Re-evaluate at v0.3.0 release tag.

---

## 3. Codebase orientation (current layout)

```
sinfonia/
├── Cargo.toml                   # workspace manifest; shared [workspace.dependencies]
├── Cargo.lock
├── config/
│   └── cost_table.yaml          # P3 — embedded via include_str! into sinfonia-bridge; overridable via bridge.cost_table_path
├── crates/
│   ├── sinfonia/                # the daemon (existing)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── agent/           # raw + CLI agent backends (incl. opencode.rs from P2)
│   │   │   ├── config/          # WORKFLOW.md parser; typed config (P3 adds TelemetryConfig)
│   │   │   ├── domain.rs        # orchestrator-only types + re-exports
│   │   │   ├── errors.rs        # sinfonia::Error (wraps tracker::Error)
│   │   │   ├── http/            # axum dashboard + /api/v1/state + P3 events.rs (subscribers + emitter)
│   │   │   ├── lib.rs
│   │   │   ├── main.rs          # clap entry; P3 wires telemetry::init_observability + emitter task
│   │   │   ├── orchestrator/    # polling, dispatch (P3 DispatchOutcome), runner (P3 session/turn spans), retries
│   │   │   ├── telemetry/       # P3 — mod.rs (init_observability), tenant.rs (resolver), spans.rs (constants)
│   │   │   ├── template.rs      # Liquid prompt rendering (H-1 well-known seed)
│   │   │   ├── tracker.rs       # shim re-exporting sinfonia_tracker + build_from_config
│   │   │   └── workspace/       # per-issue dir + hooks (P3 workspace.hook span)
│   │   └── tests/
│   │       └── spec_conformance.rs
│   ├── sinfonia-tracker/        # shared tracker abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs           # IssueTracker trait + re-exports
│   │       ├── config.rs        # TrackerKind, TrackerConfig
│   │       ├── custom_fields.rs # CustomFieldValue, MARKER, WELL_KNOWN_FIELDS (P3 + sinfonia_budget_exhausted_at), codec
│   │       ├── error.rs         # tracker-specific Error
│   │       ├── jira.rs          # JiraTracker (defaults for bridge-write methods — Phase 4 fills these in)
│   │       ├── linear.rs        # LinearTracker (full bridge-write impls; P3 tracker.fetch span)
│   │       └── types.rs         # Issue (with .fields), IssueState, BlockerRef, ChildRef
│   └── sinfonia-bridge/         # the bridge daemon — Phase 1 + 3 complete (config + webhook + feedback loop + GitHub auth + --self-test + wiremock integration tests + Phase 1 docs + OTel telemetry + typed event channel + budget enforcement); v0.3.0-alpha.1 + Unreleased Phase 3 entries
│       ├── src/
│       │   ├── config.rs        # BRIDGE.md parser (P3 added TelemetrySection.headers)
│       │   ├── events.rs        # P3 — POST /api/v1/sinfonia-events inbound handler
│       │   ├── feedback/        # attempts, categorize, transition (P1-F) + P3 cost.rs + budget.rs
│       │   ├── github/          # PAT + App auth (P1-G)
│       │   ├── labels.rs        # LabelManager (P1-F)
│       │   ├── lib.rs
│       │   ├── main.rs          # P3 wires BudgetManager + debounce reconciler
│       │   ├── selftest.rs      # --self-test runner (P1-G)
│       │   ├── storage.rs       # SQLite (P1-E)
│       │   ├── telemetry/       # P3 — sibling of sinfonia's telemetry/
│       │   └── webhook/         # handlers, mod, verify (P3 reused by events.rs)
│       └── tests/
│           └── bridge_e2e.rs    # P1-H wiremock integration suite
├── docs/
│   ├── SPEC.md                  # Symphony spec; §11.6 (bridge extension contract) + P3 §11.6.11/§11.6.12 + §18.2 OTel
│   └── v0.3-plan/
│       ├── 00-overview.md       # milestone index + revision history
│       ├── 01-bridge-mvp.md     # Phase 1 plan
│       ├── 02-opencode-backend.md / 02-opencode-VERIFY.md  # Phase 2 plan + verify
│       ├── 03-telemetry-budget.md / 03-telemetry-VERIFY.md # Phase 3 plan + verify
│       ├── 04-jira-bridge.md    # Phase 4 plan (merged via #11)
│       ├── 05-skills-cli.md     # Phase 5 plan
│       ├── 06-docker.md         # Phase 6 plan
│       ├── 07-docs.md           # Phase 7 plan
│       └── STATUS.md            # this file
├── examples/
│   └── telemetry/               # P3 — reference Collector + Postgres deployment
│       ├── README.md            # wiring guide + span / attribute reference + multi-tenant routing notes
│       ├── otel-collector-config.yaml
│       ├── postgres-schema.sql  # sessions, attempts, events tables + indexes
│       └── queries/             # 01-tenant-monthly-cost, 02-first-try-rate, 03-budget-heavy-tickets
├── scripts/
│   └── verify-workspace-move.sh # one-shot gate for the P1-A commit
├── Dockerfile                   # P6 — production multi-stage; six build targets (one per image)
├── Dockerfile.dev               # P6 — pre-existing dev-shell image (verbatim move from `Dockerfile`)
├── docker-compose.yml           # P6 — production topology (daemon + bridge + OTel Collector + Postgres)
├── docker-compose.dev.yml       # P6 — pre-existing dev compose (verbatim move; `build.dockerfile` updated)
├── docker-compose.ci.yml        # P6 — overlay for `docker-compose-smoke.sh`; strips user-cred mounts
├── docker-bake.hcl              # P6 — source of truth for image matrix (targets, platforms, tags)
├── tests/                       # P6 — repo-root smoke harness
│   ├── docker-smoke.sh          #     per-image `--help` + `sinfonia --check` against WORKFLOW.example.md
│   ├── docker-compose-smoke.sh  #     full stack up under `docker-compose.ci.yml` overlay, HTTP polling
│   └── fixtures/                #     {WORKFLOW,BRIDGE}.smoke.md — minimal configs that parse under `--check`
├── .github/
│   └── workflows/
│       └── docker-publish.yml   # P6 — `bake --push` on `v*` tags, then smoke + per-image Trivy CRITICAL/HIGH
├── BRIDGE.example.md            # new in P1-I — fully-commented working bridge config (parses under `--check`)
├── README.md                    # "What's new in v0.3 (preview)" stub (P1-I) + P3 Observability section
├── CHANGELOG.md                 # [0.3.0-alpha.1] (P1) + Unreleased (P2 + P3)
└── WORKFLOW.example.md          # already documents states: block usage
```

---

## 4. Plan-doc revision pointers (for re-readers)

- The proposal that started all this: `/Users/brettlee/Downloads/sinfonia-change-proposal.md` (read-only reference; not in-repo).
- Plan revision history with rationale per fix: `docs/v0.3-plan/00-overview.md` "Plan revision history" + "Deferred plan-checker findings".
- The five locked-in design decisions you should NOT re-litigate at impl time are listed at the top of every phase doc (e.g. `01-bridge-mvp.md` §1).

---

## 5. Implementation decisions you'll need to know

These are deltas from the plan docs — choices made during Phase 1 + Phase 2 implementation that future work needs to respect. §5.1–§5.9 are Phase 1 decisions; §5.10–§5.12 are Phase 2 deltas.

### 5.1 `CustomFieldValue` has THREE variants, not five

**Plan doc (01-bridge-mvp.md §4.2):** showed `Null` / `Number` / `Decimal` / `LongText` / `Url`.
**Implementation (`crates/sinfonia-tracker/src/custom_fields.rs`):** `Null` / `Number` / `String`.

**Why:** `#[serde(untagged)]` deserializes by trying each variant in declaration order. A JSON string deserializes as the first matching variant — so `Decimal(String)` / `LongText(String)` / `Url(String)` are indistinguishable on the wire. We collapsed to `String(String)` and pushed the "is this a cost? a URL? plain text?" semantic typing into `CustomFieldKind` for the Jira-side schema, where it still matters.

**Implication for the bridge:** cost values get written as `CustomFieldValue::String("8.23")` (stringified for precision; never use f64 for money). URLs get written as `CustomFieldValue::String("https://…")`. There's a `CustomFieldValue::text(impl Into<String>)` convenience builder.

### 5.2 Liquid `| default:` does NOT rescue truly-absent keys

**Bug surfaced:** strict-mode Liquid errors on "Unknown index" *before* the `default:` filter has a chance to fire — so `{{ issue.fields.X | default: "…" }}` against a missing `X` is a render error, not a fallback.

**Fix:** `template.rs` pre-seeds the `issue.fields` Liquid sub-object with `nil` entries for every key in `sinfonia_tracker::custom_fields::WELL_KNOWN_FIELDS` before rendering. This way, *known* sinfonia_* keys always exist (as nil); the `| default:` filter then catches them correctly.

**Implication:**
- If a bridge wants to write a *new* custom-field key, it MUST appear in `WELL_KNOWN_FIELDS` (otherwise template authors using `| default:` will hit "Unknown index").
- Phase 5's `setup-state-machine` skill's grep check (in `05-skills-cli.md` §8) is still valid — `| default:` on a well-known key works.
- Keys outside the well-known set still error in templates. That's intentional: it forces deliberate registration.

### 5.3 `LinearTracker::new` / `JiraTracker::new` took an API refinement

**Before:** `pub fn new(cfg: &ServiceConfig) -> Result<Self>`.
**After:** `pub fn new(cfg: &TrackerConfig) -> Result<Self>`.

They only ever read `cfg.tracker.*` anyway. The bridge constructs a `TrackerConfig` from its own `BRIDGE.md` config and passes it in. `sinfonia/src/tracker.rs` does the same conversion from `ServiceConfig` so existing call sites are unchanged.

### 5.4 Linear fetches comments inline

`ISSUE_FRAGMENT` in `crates/sinfonia-tracker/src/linear.rs` now includes `comments(first: 100) { nodes { body } }`. `normalize_full()` scans those comments for the marker envelope to populate `Issue.fields` in one GraphQL hop per state-page.

**Implication:** Linear API payload per ticket is larger now. For tickets with >100 comments where the marker arrived early, the marker WILL still be in the first 100 (Linear's `comments` ordering is creation-ascending by default; the bridge creates the marker on first interaction, so it's always near the front). For tickets with >100 *bot interactions*, the marker would scroll off — documented in `docs/SPEC.md` §11.6.7 (P1-I) with RECOMMENDED mitigations.

### 5.5 `sinfonia::Error` now has a `Tracker` variant

**Wraps:** `#[from] sinfonia_tracker::Error`.
**Implication:** `?` from any tracker call auto-converts. Existing code uses this transparently. Direct constructors of formerly-bare variants now route through the wrap:
```rust
// Before:  return Err(Error::MissingTrackerApiKey);
// After:   return Err(sinfonia_tracker::Error::MissingTrackerApiKey.into());
```

Only three call sites in `crates/sinfonia/src/config/typed.rs` were affected.

### 5.6 `scripts/verify-workspace-move.sh` is post-merge dead weight

The script exists because the workspace-move commit needed a verifiable "logic unchanged" artifact. Post-merge it's no longer load-bearing. Decision: leave it in-repo as historical record. Phase 7 documentation can remove it if it grates.

### 5.7 P1-E `Store` uses a single-mutex connection, not a pool

**Plan doc (01-bridge-mvp.md §11 question 4):** resolved as "SQLite only, no in-memory layer."
**Implementation (`crates/sinfonia-bridge/src/storage.rs`):** one `rusqlite::Connection` wrapped in `Arc<tokio::sync::Mutex<Connection>>`.

**Why:** v0.3 webhook traffic is tens of events per minute at worst; a connection pool's coordination cost exceeds its throughput benefit at that scale. Single-mutex serialization is easier to reason about and trivial to swap to `r2d2_sqlite` later.

**Implication:** P1-F should NOT introduce per-call `Connection::open(...)` paths — every storage access goes through the same `Store` instance held in `AppState`. The `Store` is `Clone` (its inner is `Arc<Mutex<…>>`), so handlers that need to spawn long-running futures can `.clone()` cheaply.

### 5.8 P1-E webhook errors are JSON, not plain text

**Implementation:** every non-200 response from `POST /webhook` is `Json<Value>` with an `error` (or `status`) field. Examples: `401 {"error":"webhook: signature length mismatch"}`, `200 {"delivery_id":"…","status":"duplicate"}`, `202 {"action":"opened","delivery_id":"…","event":"pull_request","pr_number":…,"repo":"…","status":"queued","ticket_id":"…"}`.

**Implication:** P1-F replies should follow the same shape so the future `--self-test` (P1-G) and any operator tooling can parse responses uniformly. The wiremock integration tests in P1-H will assert on these JSON shapes.

### 5.9 P1-E accepts `reopened` as a mapping-update event

**Plan doc (01-bridge-mvp.md §5.1 / §5.5):** lists `opened` / `synchronize` / `closed` as the `pull_request` actions of interest, with `opened` / `synchronize` updating the mapping.
**Implementation (`webhook/handlers::handle_pull_request`):** treats `reopened` as a mapping-update too.

**Why:** a contributor can edit the PR body between close and reopen, so the tracker link they meant to land may differ from what was recorded at first-open. Including `reopened` keeps the mapping fresh without a third event class. `closed` is still a no-op for the mapping (the row stays; future events would re-upsert if needed).

**Implication:** P1-F can treat `pr_ticket_map` as canonical for all three actions and doesn't need a separate "did this PR get re-pointed at a different ticket?" code path.

### 5.10 OpenCode CLI flag set differs from the Phase 2 plan-doc proposal

**Plan doc (02-opencode-backend.md §2 / §3.1):** proposed `--prompt-stdin --output-format json --quiet [--continue <id>] [--model <name>]`.
**Implementation (`crates/sinfonia/src/agent/opencode.rs::build_command_line`):** uses `--format json [--session <id>] [--model <name>]`.

**Why:** the doc-spike against `opencode.ai/docs` + the upstream `sst/opencode` source (`packages/opencode/src/cli/cmd/run.ts::RunCommand` and `emit()`) found the plan's flag names were a reasonable analogy to `claude` / `codex` but did not match OpenCode's actual surface:
- `--prompt-stdin` doesn't exist — OpenCode auto-detects non-TTY stdin via `resolveRunInput()` and uses it as the message.
- `--output-format json` is spelled `--format json`. There is no `--output-format`.
- `--quiet` doesn't exist — passing `--format json` already suppresses the TUI (the two are mutually exclusive with `--interactive`).
- `--continue` exists but resumes only the *last* session globally — wrong for Sinfonia's concurrent per-issue workspaces. The correct flag for "resume *this* session" is `--session <id>`.
- Session ID on every event is `sessionID` (camelCase), not `session_id`.

**Implication for future backends:** when a plan doc references a vendor-CLI flag, OTel env var, or third-party HTTP wire format, **verify against vendor docs before writing code**. Phase 3 will need to do this for the `opentelemetry-otlp` crate's exporter env vars (the OpenCode lesson generalizes: plan-doc proposed surfaces are reasonable analogies, not authoritative wire specs). Findings get pasted into the plan doc and a top-of-file rustdoc block in the implementation file (see `opencode.rs` for the pattern; full deltas table in `docs/v0.3-plan/02-opencode-VERIFY.md`).

### 5.11 OpenCode model names use `provider/model` wire format; Sinfonia passes through verbatim

**Implementation:** `AgentProvider::OpenCode` has no `default_model` — `LlmConfig::model` is passed through to `opencode run --model <name>` exactly as authored.

**Why:** OpenCode routes internally to 75+ providers, and the model string includes the provider prefix (e.g. `anthropic/claude-sonnet-4-6`, `ollama/qwen2.5-coder:32b`, `openai/gpt-5`). A Sinfonia-side default would either (a) lock users to one provider or (b) require provider-detection logic Sinfonia has no business owning.

**Implication:** users authoring `WORKFLOW.md` for `provider: opencode` MUST use the `provider/model` syntax — `WORKFLOW.example.md` documents this; the README backend table documents this; the `02-opencode-VERIFY.md` doc captures it. If a future skill (Phase 5's `setup-agent-backend`) generates an OpenCode block, it must surface the same wire format. Don't normalize the model string in `OpenCodeAgent::new` — let OpenCode error on unknown providers itself; coupling Sinfonia to OpenCode's provider registry is a maintenance trap.

### 5.12 `which` is now a workspace-level dependency

**Implementation:** Phase 2 added `which = "8"` to `[workspace.dependencies]` in the root `Cargo.toml` and consumed it from `crates/sinfonia/Cargo.toml`'s `[dependencies]`. Used by `OpenCodeAgent`'s preflight check (returns a clear "opencode not on PATH" error before the daemon tries to spawn).

**Implication:** if Phase 3 (or any later phase) needs preflight binary checks — Phase 6's docker image bootstrap might, Phase 5's setup skills definitely will — reuse `which` instead of shelling out to `command -v`. The crate handles Windows path-extension semantics that `command -v` doesn't. The pre-existing `cli.rs` preflight (for `claude` and `codex`) was written before `which` was a workspace dep; refactoring it to use `which` is below the threshold for Phase 3's scope but could happen as part of Phase 6's docker work if `cli.rs` gets touched anyway.

### 5.13 OTel crate version delta is ~8 minors past the plan-doc proposal

**Plan doc (`03-telemetry-budget.md` §10):** `opentelemetry = 0.24` / `opentelemetry_sdk = 0.24` + `rt-tokio` / `opentelemetry-otlp = 0.17` + `grpc-tonic + http-proto` / `tracing-opentelemetry = 0.25`.
**Implementation (`Cargo.toml` workspace deps):** `opentelemetry 0.32` / `opentelemetry_sdk 0.32` + `rt-tokio` / `opentelemetry-otlp 0.32` + `grpc-tonic + http-proto` / `opentelemetry-semantic-conventions 0.32` + `semconv_experimental` / `tracing-opentelemetry 0.33`.

**Why:** the OTel Rust crates moved roughly eight minors past the plan-doc numbers in the ~6 months between plan write and Phase 3 start. The four-crate release shipped together on 2026-05-08 (opentelemetry / sdk / otlp 0.32); `tracing-opentelemetry 0.33` followed on 2026-05-18, tracking `opentelemetry 0.32` as `^0.32`. Feature-flag names the plan doc proposed held up — only the version numbers needed re-pinning.

**Implication for future phases:** the Phase 2 §5.10 + Phase 3 vendor-doc-spike pattern generalizes — any time a plan doc references a vendor library version, OTel exporter env var, or wire-format dialect, verify against the upstream docs before writing code. The 30-minute spike is cheap insurance against re-doing the work post-merge. Captured in `docs/v0.3-plan/03-telemetry-VERIFY.md` §1.

### 5.14 OTel 0.32 SDK API differs from plan-doc snippet

**Plan doc (`03-telemetry-budget.md` §2):**
```rust
opentelemetry_sdk::trace::TracerProvider::builder()
    .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
```
**Implementation (`telemetry/mod.rs` of both crates):**
```rust
opentelemetry_sdk::trace::SdkTracerProvider::builder()
    .with_batch_exporter(exporter)  // runtime picked from `rt-tokio` feature
```

**Why:** the provider type renamed (`TracerProvider` → `SdkTracerProvider`) and `with_batch_exporter` now takes the exporter alone — the batch processor implicitly picks the runtime from the SDK's feature flags. Phase 3 captures this in the `init_observability` rustdoc; future OTel-touching code should reference the rustdoc, not the plan-doc snippet.

### 5.15 Semantic conventions: `SERVICE_NAMESPACE` is gated behind `semconv_experimental`

**Plan doc** anchors the routing-processor split on `service.namespace = tenant_id` (§3.3) without addressing how to reference the constant.
**Implementation:** `opentelemetry-semantic-conventions 0.32` ships a `semconv_experimental` feature that gates `SERVICE_NAMESPACE` + `SERVICE_INSTANCE_ID` constants. The crate's "stable" subset is essentially just `SERVICE_NAME`; the feature flag is a versioning hedge against churn in the wider semconv spec.

**Implication:** the feature is enabled at workspace scope with a comment. Long term — if the constants are promoted to stable (likely; the underlying conventions ARE stable) — drop the feature flag and the comment together. No code change needed.

### 5.16 OTel metrics layer deferred to Phase 3.1; span-derived dashboards in v0.3

**Plan doc (`03-telemetry-budget.md` §6):** lists nine OTel metric instruments (`sinfonia.agent.tokens_total` Counter, `bridge.cost_per_ticket_usd` Histogram, etc.) on a `MeterProvider`.
**Implementation:** zero metrics shipped in v0.3. Twelve spans shipped instead.

**Why:** the §8.2 dashboard SQL queries (`examples/telemetry/queries/*.sql`) read from span attributes via the `events` table, not from OTel metric points. The plan exit criteria (`docs/v0.3-plan/03-telemetry-budget.md` §9.3) are met span-derived. Setting up a `MeterProvider` + 9 instrumented sites is substantial work whose primary consumer is the same span-attribute set we already emit. Adding metrics later is purely additive — existing spans stay; new metric sites layer on top.

**Implication:** future patches adding metrics should NOT change span attribute names (the dashboards depend on them) — instrument metric sites at the same locations the spans currently emit, with the same attribute names where they overlap. The one metric without a span analog is `sinfonia.orchestrator.concurrent_runs` (a gauge needs the in-process state.running.len(), not an event); see `03-telemetry-VERIFY.md` §2.3.

### 5.17 OTel emission is opt-in by configuration

**Implementation:** when `telemetry.otlp_endpoint` is unset in the parsed config AND `OTEL_EXPORTER_OTLP_ENDPOINT` env var is unset, `build_otel_layer` returns `(None, None)` and the binary's behavior matches v0.3.0-alpha.1 — stdout-only. Logged at startup with `telemetry initialized otel_enabled=false`.

**Implication:** the metrics-layer addition (v0.3.1) can land without breaking deployments that don't want OTel — the same opt-in gate applies. Phase 4's Jira adapter, Phase 5's setup skills, and Phase 6's docker image all run cleanly without OTel configured.

### 5.18 Sinfonia↔bridge HMAC scheme reuses the GitHub-webhook verify helper

**Implementation:** Sinfonia's outbound POST signer (`crates/sinfonia/src/http/events.rs::sign`) produces a `sha256=<hex>` header value. The bridge verifies via `crates/sinfonia-bridge/src/webhook/verify.rs::verify_signature` — the SAME helper that verifies GitHub's `X-Hub-Signature-256`. Only the header name differs (`X-Sinfonia-Signature-256` for the typed channel, `X-Hub-Signature-256` for the GitHub webhook).

**Why:** zero algorithm fork — one constant-time HMAC-SHA256 compare on the bridge side, two callers. If a security review finds a flaw in the verify helper, the fix lands once and protects both inbound channels. The header-name split lets a reverse proxy / API gateway route GitHub webhooks separately from Sinfonia events without inspecting the body.

**Implication:** if a third HMAC-signed inbound path appears, route it through the same helper — DO NOT introduce a parallel verify function. The header-name choice is the dispatch key. Pinned by `bridge::events::tests::hmac_signature_format_matches_sinfonia`.

### 5.19 `TurnOutcome::Completed` carries `usage` directly

**Before Phase 3:** `TurnOutcome::Completed { final_message: String }`. The per-turn `TokenUsage` was passed into `AgentEvent::TurnCompleted` via the event channel and then DROPPED at the runner — the orchestrator dashboard kept a running view, but the runner couldn't aggregate session totals without re-reading the channel.
**After Phase 3:** `TurnOutcome::Completed { final_message: String, usage: TokenUsage }`. Every backend already had the `total_usage` value at the call site (`turn.rs:total_usage`, `cli.rs:parsed.usage`, `opencode.rs:parsed.usage`); they just weren't surfacing it.

**Implication:** future agent backends MUST also populate `usage` (the trait method signature carries it). `TokenUsage::default()` is acceptable for backends that genuinely don't track tokens (CodexAppServer stub). The runner aggregates per-session totals at the natural exit point (`crates/sinfonia/src/orchestrator/runner.rs`) and emits them on the `runner.session` span + the `SessionCompleted` event.

### 5.20 `Orchestrator::dispatch_one` returns `DispatchOutcome`, not `bool`

**Before Phase 3:** `dispatch_one(...) -> bool` where `true` = "keep trying" and `false` = "no slot, break."
**After Phase 3:** `dispatch_one(...) -> DispatchOutcome::{Dispatched, Skipped, NoSlot}` with `continue_loop()` + `is_dispatched()` helpers.

**Why:** `orchestrator.tick`'s `dispatched_count` attribute (plan §4) needs to know how many candidates actually spawned a worker — the prior bool conflated "spawned" with "ineligible / already-running, but keep going." `retries::tick_retries` uses `outcome.continue_loop()` to preserve its existing "no slot → requeue" semantics; tick uses `outcome.is_dispatched()` to count.

**Implication:** future callers MUST match exhaustively or use the helpers. Don't reach for a fourth variant on a whim — `Skipped` already covers "eligible but already running" and "ineligible" (they're indistinguishable from the orchestrator's point of view). Adding a fifth `OutcomeReason` for that distinction would surface where it isn't currently needed.

### 5.21 Cost values use `rust_decimal::Decimal` end-to-end

**Implementation:** `crates/sinfonia-bridge/src/feedback/{cost,budget}.rs` use `Decimal` for `compute_cost`, the per-ticket accumulator's `pending_cost_usd` and `running_total_cost_usd`, the cap-comparison, AND the flush. `cost_to_string(Decimal)` is the one place we cross to a string for the tracker write (per STATUS §5.1: money is NEVER f64 on the wire).

**Why:** `f64` arithmetic on tiny cost deltas (e.g. claude-haiku-4-5 at 1µ + 5µ USD per million tokens × few-thousand tokens) accumulates error fast. `Decimal` preserves exact arithmetic; the stringification step at the tracker boundary rounds to 4 decimal places (sufficient for the per-million-token rates at current prices).

**Implication:** Phase 4's Jira adapter writes the same stringified value to a text-typed customfield. Phase 5's setup skills, when generating cost-cap dashboard helpers, should treat the stored value as text and `::numeric` cast in SQL (see `examples/telemetry/queries/01-tenant-monthly-cost.sql`). No part of the v0.3 wire format carries a JSON number for money.

### 5.22 M-2 freshness gate is asymmetric: token caps survive, cost caps don't

**Implementation:** `CostTable::accepts_cost_caps(today)` returns `false` when `today - verified_at > 180 days`. `BudgetManager::apply_session` only enforces cost caps when the table accepts them; token caps fire unconditionally.

**Why:** a stale cost table can over- or under-charge by an unbounded factor (provider prices halve / double on a single release). Token caps stay correct under stale data — they're a count, not a price. Operators who care about cost SLOs need fresh data; operators who only want a token-count guardrail get one regardless.

**Implication:** future cost-table updates (the M-2 follow-up — replacing `verified_at: <hardcoded date>` with `chrono::Utc::now()` whenever the table changes) need to bump the date in the YAML AND ensure the price values are actually current. The 180-day window is conservative; if pricing churn accelerates (e.g. provider announces monthly price changes) tighten the constant in `crates/sinfonia-bridge/src/feedback/cost.rs::COST_CAP_BLOCK_DAYS`.

### 5.23 The 30-second debounce flush is per-ticket, not global

**Implementation:** `BudgetManager`'s `state.tickets: HashMap<String, TicketAccumulator>` carries per-ticket `last_event_at`. The reconciler runs every 5 seconds (`spawn_debounce_reconciler`) and flushes any ticket where `now - last_event_at >= 30s` AND there are pending deltas. Cap-crossings bypass the debounce entirely and flush immediately.

**Why:** Linear's tracker write is a read-modify-write on a single bot-owned comment per ticket. A busy ticket emits 3-5 `runner.session.completed` events per minute; writing the comment each time burns Linear API budget on values nobody reads until the next agent dispatch. Coalescing to 30 s idle drops the write rate ~10× with no observable user impact (the cost-cap dashboards read span attributes, not the tracker comment).

**Implication:** the accumulator is in-process state. It does NOT survive bridge restart. On restart the bridge re-reads the last persisted totals from the tracker as the new baseline; any deltas pending at restart are lost. Acceptable per plan §7.3 — budget caps are an SLO, not a billing system. Phase 4's Jira adapter inherits the same trade-off (Jira's text customfield + the 30 s debounce both work identically).

### 5.24 `BudgetManager` is held in `AppState`; `with_default_budget(...)` builds it for tests

**Implementation:** `crates/sinfonia-bridge/src/webhook/mod.rs::AppState` carries `budget: BudgetManager` alongside `config`, `store`, `tracker`, `gh`, `labels`. Production wires it in `main.rs` after loading the cost table; tests + the P1-H integration suite use `AppState::with_default_budget(cfg, store, tracker, gh, labels)` which builds a BudgetManager from the embedded cost table.

**Why:** production needs to honor `bridge.cost_table_path` overrides (per plan §7.1). Tests don't care. Splitting the constructor into a "production" form (`AppState::new(... budget)`) and a "test" convenience (`AppState::with_default_budget(...)`) keeps the test ergonomics unchanged while letting `main.rs` thread the override through.

**Implication:** Phase 4's Jira integration tests should adopt `with_default_budget` like the Linear tests do — the budget pipeline is tracker-agnostic, and the default cost table works for any provider/model lookup.

---

## 6. Resume protocol — first commands a fresh agent should run

```bash
# 1. Land on a clean main with the full Phase 1 + 2 + 3 of v0.3.
git checkout main
git pull --ff-only origin main

# 2. Confirm test baseline (should be 183 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file), the milestone overview, and the
#    Phase 4 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/00-overview.md
cat docs/v0.3-plan/04-jira-bridge.md

# 4. Spot-check Phase 1 + 2 + 3 deliverables on disk (none of these should error).
ls crates/sinfonia-bridge/src/        # expect: config.rs, lib.rs, main.rs, storage.rs, labels.rs, selftest.rs, webhook/, feedback/, github/, telemetry/, events.rs
ls crates/sinfonia-bridge/src/feedback # expect: attempts.rs, budget.rs, categorize.rs, cost.rs, mod.rs, transition.rs
ls crates/sinfonia/src/telemetry/     # Phase 3: tenant.rs, spans.rs, mod.rs
ls crates/sinfonia/src/http/events.rs # Phase 3: subscriber-emitter + endpoints
ls examples/telemetry/                # Phase 3: postgres-schema.sql, otel-collector-config.yaml, queries/, README.md
ls BRIDGE.example.md                  # Phase 1 docs, P1-I
cargo run -q -p sinfonia-bridge -- BRIDGE.example.md --check  # expect: ok

# 5. Read the Phase-4 hand-off blueprint before writing any code.
#    Phase 4 fills in the Jira side of the IssueTracker trait extension
#    that P1-B landed. The Linear adapter is the reference; the Jira
#    adapter does NOT reproduce the marker-comment design — Jira has
#    native customfields. Vendor docs (Jira REST + ADF) MUST be
#    verified before writing code per the §5.13 + §5.10 lesson.
cat crates/sinfonia-tracker/src/jira.rs           # current default impls
cat crates/sinfonia-tracker/src/linear.rs         # reference (different design)
cat crates/sinfonia-tracker/src/custom_fields.rs  # WELL_KNOWN_FIELDS, marker scheme
grep -n "tracker.kind 'jira' not supported" crates/sinfonia-bridge/src/config.rs  # the gate Phase 4 removes

# 6. Start a Phase 4 branch off main.
git checkout -b v0.3-phase-4-jira
```

Phases 1–4 of v0.3 are complete (P1-A..P1-I + P2 + P3 + P4 all merged). Phases 5–7 follow in plan-doc order: Phase 5 (`setup-bridge` skills CLI) is the next pickup, Phase 6 (Docker images), and Phase 7 (documentation) round out v0.3.

---

## 7. Open issues to track across phases

From the second-pass `gsd-plan-checker` review. The originals are in `docs/v0.3-plan/00-overview.md` "Plan revision history" — repeated here as a checklist so they're picked up at the right phase.

### Deferred from the plan-checker review

| ID | Gist | Resolve when |
|---|---|---|
| **M-1** | Phase 2 depends only on Phase 1's workspace conversion, not the rest of P1. Parallelism unlock. | ✅ Closed by Phase 2 merge (PR #9, commit `3b84a20`, merge `f26aca7`). No longer applicable. |
| **M-2** | Cost-table drift gate is asymmetric. Refuse cost caps (not token caps) when table >180 days old. | ✅ Closed by Phase 3 merge (PR #10, commit `6eaf4c2`). Implemented in `crates/sinfonia-bridge/src/feedback/cost.rs::accepts_cost_caps` + `BudgetManager::new`. Pinned by `feedback::cost::tests::freshness_gate_warn_and_block`. See §5.22. |
| **M-4** | (Closed) §6's "tracker poll every 60s" was rewritten to webhook-driven. | N/A — done. |
| **M-8** | `inquire` should be `crates/sinfonia/Cargo.toml`-scoped, not workspace-scoped. | Phase 5. |

### Found during P1 foundation implementation

| Topic | Where | When to address |
|---|---|---|
| `WELL_KNOWN_FIELDS` boundary semantics — what happens when the bridge writes a key outside the list | `custom_fields.rs` | ✅ documented in `docs/SPEC.md` §11.6.4 (P1-I). Phase 3 must add the new budget-related keys (`sinfonia_session_cost_usd`, `sinfonia_total_cost_usd`, `sinfonia_token_total`, `sinfonia_budget_exhausted_at`, etc.) to `WELL_KNOWN_FIELDS` before the bridge writes them — see §5.2. |
| `comments(first: 100)` upper limit when bot has >100 interactions | `linear.rs` ISSUE_FRAGMENT | ✅ documented in `docs/SPEC.md` §11.6.7 (P1-I) with RECOMMENDED mitigations. Open: implement `comments(orderBy: createdAt, first: 5)` or marker-by-author filtering if anyone hits the limit. |
| `JiraTracker` raw_graphql returns the tracker crate's stock "not supported" error since we moved it; sinfonia's old behavior was equivalent. No action needed unless someone files an issue. | `linear.rs::raw_graphql` vs `jira.rs::raw_graphql` | N/A — confirmed equivalent. |
| CI hookup for the bridge crate (`cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` as a docs-side gate) | repo CI config | When CI is being touched anyway — surface as a follow-up if not already in scope. |
| Manual end-to-end verification against a real Linear project + sandbox GitHub repo (`docs/v0.3-plan/01-bridge-mvp-VERIFY.md`, plan §9.3) | manual ops doc | Before tagging `v0.3.0-alpha.1` to an actual release. The 158 automated tests cover logic; manual run confirms the auth dance. |

### Found during P2 implementation

| Topic | Where | When to address |
|---|---|---|
| Vendor-CLI flag deltas (Phase 2 plan-doc surface didn't match `opencode.ai/docs`) | `opencode.rs` rustdoc + `docs/v0.3-plan/02-opencode-VERIFY.md` | ✅ resolved in P2; pattern documented in §5.10 for future phases. Generalized in Phase 3 §5.13 (OTel crate-version delta). The lesson is now phase-agnostic. |
| Manual end-to-end verification of OpenCode against a real Linear project | `docs/v0.3-plan/02-opencode-VERIFY.md` §5.3 | Before tagging `v0.3.0-alpha.x` to an actual release. The 9 new tests cover logic; manual run confirms the OpenCode auth dance and the per-state routing. |
| `cli.rs` preflight could migrate to `which` now that it's a workspace dep | `crates/sinfonia/src/agent/cli.rs` preflight helper | Optional; below threshold for Phase 3 (was not touched). Could fold into Phase 6 if `cli.rs` gets touched anyway for docker-image work. See §5.12. |

### Found during P3 implementation

| Topic | Where | When to address |
|---|---|---|
| OTel crate-version delta (~8 minors past the plan-doc proposal). The vendor-doc-spike lesson from P2 §5.10 generalizes — any plan-referenced vendor surface MUST be re-verified before writing code. | `Cargo.toml` workspace deps + `03-telemetry-VERIFY.md` §1 | ✅ resolved in P3; documented in §5.13. Forward-relevant for Phase 4 (Jira REST + ADF schema), Phase 5 (`inquire` API), Phase 6 (multi-arch Docker base images). |
| OTel metrics layer (9 instruments per plan §6) deferred to v0.3.1. The §8.2 dashboard SQL reads span attributes from the events table, NOT OTel metric points, so plan exit criteria met span-derived. | `03-telemetry-VERIFY.md` §2.1 (per-metric span-derived equivalent) | v0.3.1 — focused patch with `MeterProvider` setup + the 9 instruments. Future patches MUST NOT change span attribute names (the dashboards depend on them) — see §5.16. |
| Wire-level integration tests (`tests/telemetry_e2e.rs`, `tests/budget_e2e.rs`) deferred to v0.3.1. Algorithmic surface pinned by unit suite; the wire concern is covered by manual verification per Phase 3 plan §9.3 + the existing P1-H wiremock harness for the tracker-write side. | `03-telemetry-VERIFY.md` §2.5 (concern-to-test cross-reference table) | v0.3.1 — promote when a regression surfaces against the unit-tested surface, OR when manual verification runs for the first time and a wire bug is found. |
| Manual end-to-end verification against the reference Collector + Postgres stack + a `$0.01` cap-hit scenario | `docs/v0.3-plan/03-telemetry-VERIFY.md` §3 protocol | Before tagging `v0.3.0-alpha.x` to an actual release. The 183 automated tests cover logic; manual run confirms the OTLP gRPC handshake + the cap-hit transition end-to-end. |
| `bridge.pr_label` span not implemented; the existing `labels.rs` tracing logs cover the dashboard need. | `crates/sinfonia-bridge/src/labels.rs` | Re-evaluate if a `bridge.pr_label`-keyed dashboard query lands and the log-emission isn't enough. Below threshold otherwise. |
| `sinfonia.orchestrator.concurrent_runs` gauge has no span analog — it's a true gauge over `state.running.len()`. Lands with the OTel metrics layer. | `crates/sinfonia/src/orchestrator/state.rs` | v0.3.1 alongside the metrics layer. The metric site is on `state.running` mutations (claim / release). |

---

## 8. Pointer reference (paste-ready)

For the next agent's first message to itself when context is fresh:

```
Working directory: /Users/brettlee/work/sinfonia
Current branch: main (assumed; verify with `git branch --show-current`)
Last merged work: P4 Jira bridge support (PR #11, merge 17f5213)
                  — **Phase 4 of v0.3 is now complete.**
Earlier merges: P3 telemetry + budget enforcement (PR #10, merge 707a812);
                P2 OpenCode agent backend (PR #9, commit 3b84a20, merge f26aca7);
                P1-I Phase 1 docs (PR #8, commit a057218, merge e8f224a)
                  — closed out Phase 1 of v0.3;
                P1-H wiremock integration tests (PR #7, commit d7ad72d, merge 749c9c4);
                P1-G GitHub auth (PR #6, commit b0d7272, merge 8055659);
                P1-F feedback loop (PR #5, commit 9d33d51);
                P1-E webhook layer (PR #4, commit 69eb8e0);
                P1-D bridge skeleton (PR #3, commit 07c0381);
                Phase 1 foundation (PR #2: commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/05-skills-cli.md   (Phase 5 plan; next pickup)
     — and skim these before designing the skill UX:
     crates/sinfonia-bridge/src/config.rs (BRIDGE.md parser + validation rules,
       including the Phase 4 positive Jira rules — `endpoint` required,
       `email` required for *.atlassian.net),
     crates/sinfonia-bridge/src/selftest.rs (the existing PASS/FAIL/SKIP runner
       the skill will gate on),
     crates/sinfonia-tracker/src/jira.rs (the Jira write surface the skill
       can drive at first-run, especially `ensure_custom_field`'s screen-
       bind warn path),
     docs/JIRA-SCREEN-SCHEME.md (the operator-facing fallback the skill
       should surface when admin perms are absent).

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon; Phase 2 added src/agent/opencode.rs
                             as a sibling of cli.rs. Phase 3 added
                             src/telemetry/{mod,tenant,spans}.rs and
                             src/http/events.rs (subscriber-emitter +
                             registry + recent-buffer ring + endpoints).
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters,
                             custom_fields). Phase 3 added
                             sinfonia_budget_exhausted_at to
                             WELL_KNOWN_FIELDS. Phase 4 fills in the
                             five Jira write methods that currently
                             return default-impl errors.
  crates/sinfonia-bridge/  — bridge binary; Phase 3 added telemetry/,
                             events.rs (inbound typed events),
                             feedback/{cost,budget}.rs (cost table +
                             accumulator + 30s debounce). The
                             /api/v1/sinfonia-events route reuses
                             webhook::verify::verify_signature.

Test baseline: 183 passing, 0 failures (44 sinfonia unit + 14 conformance
+ 7 tracker + 107 bridge unit + 9 bridge integration + 2 sinfonia http
events). Phase 4 adds ~150 LOC of tests per 04-jira-bridge.md; do not
regress the existing 183.

Reference assets:
  config/cost_table.yaml                 — embedded; override via
                                            bridge.cost_table_path
  examples/telemetry/                    — Postgres schema + Collector
                                            config + 3 dashboard
                                            queries + README
  docs/v0.3-plan/03-telemetry-VERIFY.md  — Phase 3 verify doc:
                                            crate-version delta,
                                            metrics-deferral rationale,
                                            integration-test deferral
                                            cross-reference, manual
                                            verification protocol
```

---

## 9. What Phase 1 shipped (closing summary)

All success criteria from the Phase 1 deliverable checklist (`01-bridge-mvp.md` §12) are met:

- `crates/sinfonia-bridge/` is a complete, tested binary that:
  - Parses a `BRIDGE.md` config and exits cleanly on invalid input (P1-D).
  - Verifies GitHub webhook signatures with HMAC-SHA256 (P1-E).
  - Persists webhook delivery IDs in SQLite for idempotency (P1-E).
  - Maps `pull_request` / `check_suite` / `workflow_run` events to ticket-state transitions (P1-E + P1-F).
  - Routes by failure category when configured (P1-F).
  - Hits the attempt cap → moves to `blocked_state` (P1-F).
  - Manages PR labels under the `sinfonia:` prefix, with verbatim-alias semantics (P1-F).
  - Supports both GitHub PAT and App auth, with per-owner installation-scoped client cache (P1-G).
  - Exposes `sinfonia-bridge --self-test` with the canonical `PASS` / `FAIL` / `SKIP` output (P1-G).
- `BRIDGE.example.md` at the repo root documents every config field and parses cleanly under `--check` with no environment variables set (P1-I).
- `docs/SPEC.md` §11.6 draft describes the bridge-service extension contract in RFC-2119 voice (P1-I).
- `CHANGELOG.md` has a `[0.3.0-alpha.1]` entry (P1-I).
- README has a "What's new in v0.3 (preview)" stub (P1-I).
- `wiremock`-backed integration tests cover all nine scenarios in `01-bridge-mvp.md` §9.2 end-to-end (P1-H).
- The full `cargo test --workspace --no-fail-fast` count is **149 tests passing, zero failures** — 32 sinfonia + 12 conformance + 7 tracker + 89 bridge unit + 9 bridge integration.

Phase 1 of v0.3 is **shippable on its own** as `v0.3.0-alpha.1`. Phases 2–7 land on top of it.

---

**Phase 1 retrospective bullets** (for the v0.3 retro, when one is written):

- The CustomFieldValue serialization decision (§5.1) saved ~3 sub-phases of churn — it surfaced early in P1-B and was settled before P1-C had to layer template scope on top.
- The decision to ship `BRIDGE.example.md` as a real working config (parses under `--check`) rather than a cheat-sheet means schema drift between the parser and the example doc is a one-command CI gate, not a "did anyone re-read the docs after the last refactor?" review surface.
- Splitting STATUS doc updates into separate direct-to-main commits between PRs gave us nine clean inflection points in `git log` instead of one rolling doc commit per PR — every "what was this milestone like at week N" question can be answered from `git log docs/v0.3-plan/STATUS.md`.
- The `tests/bridge_e2e.rs` integration suite uses production constructors throughout (`OctocrabGhOps::from_octocrab(crab)`, `AppModeGhOps::new(crab)`, `LinearTracker::new(&cfg)`) — no test-only escape hatches in `github::auth`. P1-G's design specifically anticipated this by keeping those factories public, so the integration suite was a drop-in.

---

## 10. What Phase 2 shipped (closing summary)

All success criteria from the Phase 2 deliverable checklist (`02-opencode-backend.md` §8) are met:

- `crates/sinfonia/src/agent/opencode.rs` implements `CodingAgent` for `OpenCodeAgent` (686 LOC including rustdoc + 8 unit tests). Mirrors `cli.rs`'s parser + spawn + continuation pattern; does not refactor `cli.rs` or invent a new abstraction.
- `AgentProvider::OpenCode` variant in `crates/sinfonia/src/config/typed.rs` with `parse()` arm (`"opencode" => Ok(Self::OpenCode)`), `is_cli()` membership, default command (`"opencode run --format json"`), `default_llm_env()`, and `validate_for_dispatch()` hook.
- Factory wiring in `crates/sinfonia/src/agent/mod.rs` (`mod opencode;` declaration + the `AgentProvider::OpenCode => Arc::new(opencode::OpenCodeAgent::new(cfg, llm)?)` arm).
- `which = "8"` added to `[workspace.dependencies]` and consumed by `crates/sinfonia/Cargo.toml` for the preflight binary check.
- Unit tests per §5.1 of the Phase 2 plan, plus two beyond the minimum: `build_command_line_first_turn` / `build_command_line_pending_session` / `build_command_line_resumed_session` / `parse_init_event` / `parse_token_event` / `parse_full_stdout_picks_last_text` / `preflight_missing_binary` / `opencode_provider_is_cli_provider`.
- Integration parse-test `opencode_provider_parses` in `tests/spec_conformance.rs` covers all three §4 WORKFLOW.md example shapes (default-lane, state-machine routing, local-only Ollama).
- `WORKFLOW.example.md` gains a commented OpenCode states block with an inline note that OpenCode owns auth via its own CLI (`opencode auth login`) and Sinfonia does NOT pass an api_key.
- Module-level rustdoc on `opencode.rs` matches the comment density of `cli.rs`: doc comments on the struct, on `start_session` / `run_turn` / `stop_session`, on the `pending-` prefix convention, and on every non-obvious helper.
- README backend table adds OpenCode as a supported coding-agent CLI alongside `claude_code` / `codex`; the conformance scorecard marks OpenCode as a delivered SPEC §18.2 extension.
- `docs/SPEC.md` §18.2 lists `OpenCode backend` as a recommended extension.
- `CHANGELOG.md` `[Unreleased]` section carries the OpenCode entry (when v0.3.0-alpha.2 — or whatever the next tag is — gets cut, this is what the release notes will pull from).
- `docs/v0.3-plan/02-opencode-VERIFY.md` (204 LOC) captures the doc-spike findings (flag deltas table), the §5.3 manual-verification steps, and the "pending real-world run before v0.3.0-alpha.x tag" deferral.
- The full `cargo test --workspace --no-fail-fast` count is **158 tests passing, zero failures** — up from Phase 1's 149 by +9 (40 sinfonia + 13 conformance + 7 tracker + 89 bridge unit + 9 bridge integration).

Phase 2 of v0.3 is **shippable on top of Phase 1**. Phases 3–7 layer on top of both.

---

**Phase 2 retrospective bullets** (for the v0.3 retro, when one is written):

- The doc-spike-against-vendor-docs pattern (plan §7 open question 1) caught five concrete deltas in OpenCode's CLI surface before any code was written. Generalizing this for Phase 3: anywhere a plan doc references a vendor wire format (OTel exporter env vars, OTLP transport knobs, Postgres routing-processor config), verify it against upstream docs in the same ~30-minute spike before writing. The five-line "Deltas vs. plan" table at the top of `opencode.rs` is the model.
- Adding `which` to `[workspace.dependencies]` (vs. `crates/sinfonia/Cargo.toml`'s `[dependencies]`) cost nothing at this scale and gives future phases (Phase 5's setup skills, Phase 6's docker bootstrap, possibly a `cli.rs` cleanup) a free reuse path. The `inquire`-was-workspace-scoped lesson (§7 M-8) cuts the other way — workspace-scoping crates used by exactly one crate creates the impression of broader use. Phase 3 should put `tracing-opentelemetry` and `opentelemetry-otlp` at workspace scope (both binaries consume them) but anything single-crate (e.g. a Phase 3 budget-table-only helper) stays crate-scoped.
- Shipping Phase 2 as one PR (vs. Phase 1's nine) was right-sized — the work was naturally one unit (~400 LOC of Rust + ~150 LOC of tests + ~100 LOC of docs, all interdependent). The Phase 1 nine-way split made sense because each P1-x sub-task was independently shippable; Phase 2 wasn't. Phase 3 estimates ~1 100 LOC + 300 + 500 = ~1 900; that's plausibly two or three PRs (e.g. P3-A: subscriber init + tenant resolution, P3-B: spans + metrics + collector reference, P3-C: budget enforcement + typed event channel). Decide at plan-phase start, not partway through.
- The OpenCode `provider/model` wire format (§5.11) is the second instance of "the integration's wire format isn't Sinfonia's wire format" — the first was Linear's marker-comment envelope in P1-C. Both ended with "pass through verbatim; don't normalize." Phase 3's `tenant_id` resolution (config → env → "default") is on the other side of that line — it IS Sinfonia's concern because Sinfonia owns the multi-tenant taxonomy. Distinguishing "Sinfonia owns this taxonomy" vs. "Sinfonia consumes a vendor's taxonomy" is the load-bearing call.

---

---

## 11. What Phase 3 shipped (closing summary)

All success criteria from the Phase 3 deliverable checklist (`03-telemetry-budget.md` §12) are met, with two items explicitly deferred to v0.3.1 (OTel metrics layer + wire-level integration tests; see §7 "Found during P3 implementation" and `docs/v0.3-plan/03-telemetry-VERIFY.md` §2):

**OTel emission (both binaries):**
- OTel client crate set (`opentelemetry 0.32`, `opentelemetry_sdk 0.32`, `opentelemetry-otlp 0.32`, `opentelemetry-semantic-conventions 0.32` + `semconv_experimental`, `tracing-opentelemetry 0.33`) added to `[workspace.dependencies]`. Version set verified at impl time — the plan-doc proposed numbers were ~8 minors stale; feature-flag names held up. See §5.13.
- `crates/sinfonia/src/telemetry/` module with `mod.rs` (`init_observability(format, telemetry) -> ObservabilityGuard`), `tenant.rs` (`TenantId::resolve` precedence chain), `spans.rs` (span name + attribute key constants).
- `crates/sinfonia-bridge/src/telemetry/` mirror with the same shape; sibling not shared because span-name evolution is per-binary.
- Six daemon spans per §4: `orchestrator.tick`, `orchestrator.dispatch`, `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`. All carry the resolved `tenant_id`.
- Five bridge spans per §5: `bridge.webhook`, `bridge.ci_result`, `bridge.state_transition`, `bridge.cap_hit`, `bridge.events_receive`. Plus `bridge.cost_update` from `feedback::budget::flush_ticket`. `bridge.pr_label` deferred (covered by existing tracing logs).
- Metric sites per §6: deferred to v0.3.1; §8.2 dashboards work span-derived from the events table per `examples/telemetry/queries/*.sql`.

**Sinfonia → bridge typed event channel (§7.2):**
- `AgentEvent::SessionCompleted` variant added in `crates/sinfonia/src/agent/events.rs`; emitted from `crates/sinfonia/src/orchestrator/runner.rs` immediately after `agent.stop_session(...)` per N-3.
- `crates/sinfonia/src/http/events.rs` (~300 LOC): subscriber-emitter task consuming the existing `EventSender` channel via a fan-out tx, filtering for `SessionCompleted`, dispatching HMAC-SHA256-signed POSTs with retry-with-backoff (5 attempts, 250ms → 8s) and a 200-entry bounded ring buffer.
- `POST /api/v1/events/subscribers` and `GET /api/v1/events/recent` endpoints on the Sinfonia HTTP surface.
- `sinfonia_events_secret` plumbed through `ServiceConfig.telemetry` and used by the HMAC signer; the N-1 validation rule for "secret required when subscribe_url is set" already lived in BridgeConfig (P1-D prep).
- `POST /api/v1/sinfonia-events` endpoint on the bridge, verifying the HMAC via the existing `webhook::verify::verify_signature` helper (zero algorithm fork — see §5.18).

**Budget + cost pipeline (§7.3):**
- `crates/sinfonia-bridge/src/feedback/cost.rs` (~290 LOC) with the embedded `config/cost_table.yaml` via `include_str!`, the M-2 asymmetric freshness gate (warn at 90 days, refuse cost caps at 180 days; token caps unconditional), and the OpenCode `provider/model` lookup adaptation.
- `crates/sinfonia-bridge/src/feedback/budget.rs` (~480 LOC) with the per-process per-ticket accumulator, cap-detection (`Accumulated` / `CapHit { kind }`), 30s idle-flush debounce reconciler, and the cap-crossing path that flushes + transitions to `feedback_loop.budget_exceeded_state`.
- Terminal-state detection via the existing GitHub webhook `pull_request.closed.merged=true` path: a new branch in `handle_pull_request` flushes the accumulator + emits the transition log.

**Reference Collector → Postgres deployment:**
- `examples/telemetry/postgres-schema.sql` + `otel-collector-config.yaml` + three `queries/*.sql` + `README.md`. Three §8.2 dashboard queries verified against the span-attribute path.

**Tests + verification:**
- Workspace tests: **183 passing, 0 failed** (up from 158 by +25). Per-area breakdown in §1 "Test baseline."
- Manual verification protocol captured in `docs/v0.3-plan/03-telemetry-VERIFY.md` §3 (pending real-world run before tagging `v0.3.0-alpha.x`).

**Documentation:**
- `docs/SPEC.md` gains §11.6.11 (typed event channel contract — wire shape + HMAC contract) + §11.6.12 (budget enforcement surface — accumulator, debounce, freshness gates, per-ticket overrides) + a §18.2 RECOMMENDED entry for OpenTelemetry emission with `tenant_id`.
- `CHANGELOG.md` `[Unreleased]` section carries the Phase 3 Added / Changed / Deferred-to-v0.3.1 blocks.
- `README.md` gains the Phase 3 What's-new bullet + a new Observability section showing the env-var path + pointing at the Collector config + dashboard queries.
- `docs/v0.3-plan/03-telemetry-VERIFY.md` (new) captures the OTel crate-version delta, the SDK API rename, the `semconv_experimental` gating decision, the OTel-metrics + integration-test deferrals with span-derived equivalents / unit-coverage cross-references, the known-good integration points, and the §9.3 manual-verification protocol.

Phase 3 of v0.3 is **shippable on top of Phase 1 + Phase 2**. Phase 4 (Jira bridge), Phase 5 (skills + CLI), Phase 6 (Docker images), and Phase 7 (documentation) layer on top of all three.

---

**Phase 3 retrospective bullets** (for the v0.3 retro, when one is written):

- The vendor-doc-spike pattern from Phase 2 §5.10 generalized cleanly to Phase 3: the OTel crate set was ~8 minors past the plan doc, and a 5-minute hit-the-crates.io-API check during task #1 saved a downstream "why doesn't this compile" cycle. The lesson is now phase-agnostic — every plan-referenced vendor surface gets a re-verify before code is written. Phase 4 inherits the same expectation against the Jira REST + ADF surface.
- Deferring the OTel metrics layer to v0.3.1 was the right call. The §8.2 dashboard SQL queries genuinely read from span attributes via the events table — confirmed by reading the queries before deciding. The MeterProvider + 9-instrument plumbing is ~400 LOC of work whose primary consumer is data we already emit. Shipping spans-only got Phase 3 across the line without re-cutting the scope; metrics land additively in v0.3.1 alongside the one truly missing metric (`sinfonia.orchestrator.concurrent_runs`).
- The PR-shape call (one PR, five intermediate commits → squash merge) was a sweet spot between Phase 1's nine-PR fragmentation and a single mega-commit. Each commit was its own reviewable unit (foundation / bridge spans + event channel / cost + budget + terminal-state / docs + assets / VERIFY notes); the squash merge gives one Phase 2-style commit on main. For a phase materially larger than Phase 2 but with internally cohesive layers, this is the model.
- Choosing to reuse `webhook::verify::verify_signature` for the new typed event channel (§5.18) saved the algorithm-fork risk and means one place to fix any future HMAC issue. The header-name split (`X-Sinfonia-Signature-256` vs `X-Hub-Signature-256`) is the routing key — a reverse proxy can dispatch on header without inspecting the body. Pinned by an algorithm-format test that pins both directions.
- `TurnOutcome::Completed { usage }` (§5.19) was a tiny refactor with outsized impact: every backend already computed the value; surfacing it removed the need for the runner to listen on the event channel for its own session totals. Pattern: when the data already exists at a call site, surface it through the type system instead of plumbing a parallel observation path. Future agent backends (CodexAppServer is the holdout) MUST populate `usage` even if it's `TokenUsage::default()` — the type enforces the discipline.
- The 30s debounce + per-ticket accumulator (§5.23) is intentionally lossy on restart. The plan doc was explicit about this trade-off (budget caps as SLO, not billing); the implementation surfaces it in the budget.rs module-level rustdoc. If a future v0.5+ phase wants exact billing it'll need a different design (a SQLite write-ahead log keyed on issue_id) — but it should NOT try to make the accumulator durable while keeping the debounce.

---

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Phase 4 picks up from a fresh context with this doc as the entry point.
