# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21 (P2 merged — **Phase 2 complete**; Phase 3 is the next pickup)
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the **complete Phase 1 and Phase 2 of v0.3**. Phase 1's nine sub-tasks (P1-A through P1-I) ship as v0.3.0-alpha.1: the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix), the bridge skeleton (#3 — P1-D), the webhook layer (#4 — P1-E: HMAC + SQLite idempotency + event dispatch), the feedback loop (#5 — P1-F: categorize / attempts / transition + labels + PAT-mode `GhOps`), GitHub authentication + install gate (#6 — P1-G: PAT/App auth + `--self-test`), the wiremock-backed integration suite (#7 — P1-H: all nine §9.2 scenarios end-to-end), and the Phase 1 docs (#8 — P1-I: `BRIDGE.example.md`, `docs/SPEC.md` §11.6 draft, CHANGELOG, README stub). Phase 2 (#9 — P2: `provider: opencode` as a first-class CLI subprocess backend) lands the `OpenCodeAgent` next to `claude_code` / `codex`, the `which` workspace dep for preflight, the doc-spike-validated flag set (`--format json`, `--session <id>`), and the §8 doc deliverables (WORKFLOW example, README + SPEC §18.2 + CHANGELOG entries, `docs/v0.3-plan/02-opencode-VERIFY.md`).

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

**Phase 1 of v0.3 is complete.** All nine sub-tasks (P1-A through P1-I) are merged to `main`; the bridge MVP ships as v0.3.0-alpha.1. The bridge binary parses `BRIDGE.md`, verifies HMAC-signed GitHub webhooks, persists delivery-ID idempotency in SQLite, evaluates CI results, routes by failure category, applies the attempt cap, manages PR labels under the `sinfonia:` prefix, supports both PAT and App auth, and exposes `sinfonia-bridge --self-test` as the install gate. The full chain — bridge writes the marker comment → tracker fetch parses it into `Issue.fields` → prompt template renders `{{ issue.fields.sinfonia_last_ci_failure }}` into the agent's input — works end-to-end.

**Phase 2 of v0.3 is also complete.** `provider: opencode` is wired as a first-class CLI subprocess backend in `crates/sinfonia/src/agent/opencode.rs`, joining `claude_code` and `codex` as a sibling under `agent/cli.rs`'s pattern (parser + spawn + session continuation). OpenCode owns auth via its own CLI (`opencode auth login`) and routes internally to 75+ providers — including a local Ollama-with-LSP path that the raw `ollama` backend can't provide. The `which` crate is now a workspace-level dep used by the preflight binary check. Workspace test count: **158 passing** (40 sinfonia + 13 conformance + 7 tracker + 89 bridge unit + 9 bridge integration), zero failures — up from Phase 1's 149 by 8 new `agent::opencode::tests` unit tests + 1 new `spec_conformance::opencode_provider_parses` integration test.

The next pickup is **Phase 3 — Telemetry + budget enforcement** (`docs/v0.3-plan/03-telemetry-budget.md`). Phase 3's stated dependency was "Phase 1 AND Phase 2" — Phase 1 because the bridge owns the tracker write path the budget caps use, Phase 2 because every coding-agent backend (including OpenCode) needs to emit the same `runner.session` span shape. Both deps are now resolved. Phase 3 layers `tracing-opentelemetry` over the existing `tracing` macros (the json/pretty stdout subscribers stay unchanged), tags every span and metric with `tenant_id` from day one, defines a Sinfonia↔bridge typed HTTP event channel (replacing the dropped bridge-hosted OTLP receiver), and adds token + cost caps the bridge enforces at the tracker boundary. Plan §1-§7 is the source of truth for scope; §2's coexistence model with today's `tracing` subscribers is the key non-obvious design decision a fresh agent should anchor on first.

The single most important non-obvious decision the Phase 1+2 work bequeaths to Phase 3: **the agent-side token-accounting plumbing is already in place across every CLI backend**. `OpenCodeAgent::run_turn` parses the `provider/usage` stream events and surfaces `(prompt_tokens, completion_tokens, total_tokens)` per turn; `cli.rs` does the same for `claude_code` and `codex`; and `crates/sinfonia/src/agent/turn.rs` already aggregates token counts into `TurnOutcome`. Phase 3's `runner.session` span attributes (per `03-telemetry-budget.md` §4) are a read of fields that already exist; the work is to emit them as OTel span attributes, not to instrument fresh code paths. Treat that as the savings line — Phase 3's "instrument every backend" surface is much smaller than it looks.

The single most important non-obvious decision made during Phase 1 itself (forward-relevant to every later phase): **`CustomFieldValue` is three variants (`Null` / `Number` / `String`), not five** (the plan-doc §4 draft showed `Decimal` / `LongText` / `Url` as separate variants). Cost values, URLs, and long-text fields all serialize as `String`. See §5.1 below. This is the foundation Phase 3's budget caps and Phase 4's Jira custom-field writes both build on — Phase 3 writes `sinfonia_session_cost_usd` and `sinfonia_total_cost_usd` as `CustomFieldValue::String("8.23")` (stringified for precision; never f64 for money).

The single most important non-obvious decision surfaced during Phase 2: **the OpenCode CLI's actual flag set differs from what the plan doc proposed**, and the doc-spike-against-vendor-docs pattern this exposed is reusable. The plan said `--prompt-stdin`, `--output-format json`, `--quiet`, `--continue <id>` — what `opencode.ai/docs` actually documents is `--format json` (no `--prompt-stdin` or `--quiet` needed; stdin auto-detected, JSON suppresses TUI) and `--session <id>` (bare `--continue` resumes only the *last* session globally — wrong for concurrent per-issue workspaces). Captured in `docs/v0.3-plan/02-opencode-VERIFY.md` and in the `opencode.rs` module rustdoc. The forward-relevant lesson for Phase 3: any time a plan doc references a vendor-CLI flag, OTel exporter env var, or third-party HTTP wire format, **verify it before writing code** — Phase 3 will need to do this for the `opentelemetry-otlp` crate's exporter config knobs.

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
| (this commit) | STATUS: mark Phase 2 merged, queue Phase 3 as next deliverable | Docs — this file |

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

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` → **158 tests pass, 0 failures** (up from Phase 1's 149 by +9)
  - **40** sinfonia unit tests (up from 32 by +8 in `agent::opencode::tests`: `build_command_line_first_turn`, `build_command_line_pending_session`, `build_command_line_resumed_session`, `parse_init_event`, `parse_token_event`, `parse_full_stdout_picks_last_text`, `preflight_missing_binary`, `opencode_provider_is_cli_provider`)
  - **13** `spec_conformance.rs` integration tests (up from 12 by +1: `opencode_provider_parses` covers all three §4 WORKFLOW shapes round-tripping through `ServiceConfig::from_workflow()`)
  - 7 sinfonia-tracker tests (1 base64 + 6 custom_fields) — unchanged
  - 89 sinfonia-bridge unit tests — unchanged (Phase 2 doesn't touch the bridge crate)
  - 9 sinfonia-bridge integration tests (`tests/bridge_e2e.rs`) — unchanged
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

## 2. What's next: Phase 3 — Telemetry + budget enforcement

Phase 1 + Phase 2 are shippable. The next pickup is **Phase 3 — Telemetry + budget enforcement** (`docs/v0.3-plan/03-telemetry-budget.md`). Phase 3's plan-doc-stated dependencies were "Phase 1 (bridge + custom-fields + `Issue.fields` template plumbing) AND Phase 2 (so the OpenCode backend emits the same spans)" — both are now resolved. Per `00-overview.md`'s suggested execution order, Phase 3 is the natural serial successor to Phase 2.

### Scope (per `03-telemetry-budget.md`)

Phase 3 is materially larger than Phase 2: **~1 100 LOC of Rust + ~300 LOC of tests + ~500 LOC of docs**, with a reference OpenTelemetry Collector → Postgres setup landing alongside the code. Four headline deliverables:

1. **OTel emission, opt-in by configuration.** `tracing-opentelemetry` layered onto the existing `tracing` subscriber in both binaries (the json/pretty stdout subscribers stay unchanged). When `OTEL_EXPORTER_OTLP_ENDPOINT` is unset and no `telemetry:` block is configured, the OTel layer is `None` and behavior matches today. See plan §2.
2. **Tenancy from day one.** Every span and every metric carries a `tenant_id` attribute. Resolution precedence: `telemetry.tenant_id` from config → `SINFONIA_TENANT_ID` env → literal `"default"`. Resource-level `service.namespace = tenant_id` lets a Collector routing-processor split per-tenant data without touching emission code. See plan §3.
3. **Sinfonia↔bridge typed HTTP event channel.** Replaces the original "bridge runs its own OTLP receiver" design (which would have re-implemented the OTel SDK). Sinfonia POSTs typed events (`session.token_usage`, `session.cost`, etc.) to the bridge over HTTPS, HMAC-signed with `telemetry.sinfonia_events_secret` (shared between the two configs; mismatch → bridge returns 401 + Sinfonia logs WARN on retry exhaustion). See plan §7.2.
4. **Budget caps.** Token + cost limits enforced by the bridge at the tracker write boundary. When a cap is hit, the bridge transitions the ticket to its configured `over_budget_state` (or `blocked_state` if unconfigured) and writes the `sinfonia_budget_exhausted_*` custom fields. Cost values write as `CustomFieldValue::String("8.23")` per the §5.1 decision (NEVER f64 for money). See plan §5 + §6.

### What's already prepared for Phase 3 (inherited from Phases 1 + 2)

- **Custom-field write path.** Phase 1's `IssueTracker` extension already exposes `update_issue_custom_field` (Linear impl) and `CustomFieldValue::String/Number/Null`. Phase 3's budget bookkeeping (`sinfonia_session_cost_usd`, `sinfonia_total_cost_usd`, `sinfonia_token_total`, `sinfonia_budget_exhausted_at`, etc.) just routes through these. Phase 3 needs to add the new keys to `WELL_KNOWN_FIELDS` in `custom_fields.rs` so template authors can use `| default:` against them (see §5.2 in this file).
- **Per-turn token accounting in every backend.** `OpenCodeAgent::run_turn` (Phase 2) parses `provider/usage` stream events and surfaces `(prompt_tokens, completion_tokens, total_tokens)` per turn; `cli.rs` does the same for `claude_code` and `codex`; raw-LLM backends already aggregate via `TurnOutcome`. The instrumentation surface for Phase 3's `runner.session` span is "read existing struct fields and emit as OTel attributes" — not "instrument fresh code paths."
- **The bridge tracker write path is exercised.** Phase 1's wiremock integration tests (`tests/bridge_e2e.rs`) cover the bridge's "fetch issue → mutate → transition → write custom fields" flow end-to-end for every §9.2 scenario. Phase 3's budget-cap-hit path layers on top of this without re-plumbing the GraphQL call shape.
- **HMAC verification helper exists.** `crates/sinfonia-bridge/src/webhook/verify.rs` (P1-E) already has the constant-time HMAC-SHA256 compare. Phase 3's typed event channel will share this helper (move it up to a shared module if a third caller materializes; one new caller is below the threshold for refactoring).
- **`server.public_url`** (added in P1-G for App-mode webhook delivery). Phase 3's `/events` endpoint on the bridge will land under the same URL prefix; no new public-URL plumbing is required.

### Phase 2 → Phase 3 hand-off pointer

Read these in this order before starting Phase 3:

1. `docs/v0.3-plan/03-telemetry-budget.md` — the Phase 3 plan. Source of truth for scope, OTel attribute shapes, the typed-event wire format, validation rules, test coverage.
2. `docs/v0.3-plan/00-overview.md` "Cross-cutting concerns" — Phase 3 sits at the cross-cutting OTel/Tenancy/Budget intersection; the overview is shorter than re-reading every per-phase doc.
3. `crates/sinfonia/src/main.rs` (subscriber init) — Phase 3 wraps this with the OTel layer.
4. `crates/sinfonia-bridge/src/main.rs` — same; the bridge gets the symmetric subscriber init plus a new `/events` route.
5. `crates/sinfonia/src/agent/turn.rs` + `crates/sinfonia/src/orchestrator/runner.rs` — where `runner.session` and `runner.turn` spans land (per plan §4).
6. `crates/sinfonia-tracker/src/custom_fields.rs` — Phase 3 adds new keys to `WELL_KNOWN_FIELDS` (see §5.2 below for why this is mandatory before the bridge can write them).
7. `crates/sinfonia-bridge/src/webhook/verify.rs` (HMAC helper) — the typed event channel reuses the same algorithm.

Then `git checkout -b v0.3-phase-3-telemetry` off `main` (158-test baseline) and start there.

### Phase 3 follow-up watch list

Items the plan doc surfaces that Phase 3 should explicitly resolve before merge — not Phase 1/2 hangover:

- **Deferred plan-checker finding M-2** (cost-table drift gate): refuse cost caps when the embedded cost table is >180 days old. Token caps stay accepted. Phase 3 implementation. See §7 below.
- **OTel exporter env-var verification spike.** Per Phase 2's lesson (the OpenCode flag-set spike found multiple plan-doc deltas), Phase 3 should verify the `opentelemetry-otlp` crate's actual env-var names and feature-flag knobs against the upstream crate docs *before* writing the exporter wrapper. Findings get pasted into the plan doc and the `init_observability` rustdoc.
- **Postgres schema for the reference Collector setup.** Plan §6 references a Postgres-backed cost ledger. Decide at implementation time whether to ship the schema in `docker/postgres/init.sql` (Phase 6 territory) or as `docs/v0.3-plan/03-telemetry-budget-EXAMPLES/` companion files (Phase 3 territory). Default: docs-side under `docs/v0.3-plan/03-...` so Phase 6 can pick it up wholesale without a re-spec.

### Other follow-up work (not blocking Phase 3)

These items surfaced during Phases 1 or 2 but were not in scope. They are not blockers; surface them at the right time:

- **CI for the bridge crate.** Wire `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` into CI so schema drift between `config.rs` and the example doc fails the build. Listed in §1 as one of the Phase 1 deliverables' implicit guarantees but not currently enforced by a CI job — add it when CI is being touched anyway.
- **Manual verification.** Two docs are pending real-world manual runs:
  - `docs/v0.3-plan/01-bridge-mvp-VERIFY.md` (NOT yet written; plan §9.3 calls for end-to-end verification against a real Linear project + sandbox GitHub repo before declaring v0.3.0-alpha.1 shippable).
  - `docs/v0.3-plan/02-opencode-VERIFY.md` (written in P2; manual run against a real OpenCode install + Linear project still pending). Both should be exercised before tagging `v0.3.0-alpha.x` to an actual release. The 158 automated tests cover the logic; manual verification confirms the auth/credential dance against real services.
- **STATUS doc retire path.** This file's "Phase 1 + 2" framing accretes per merged phase. Two options: (a) keep accreting new sections per phase, (b) freeze this as `docs/v0.3-plan/STATUS-phase-1-2.md` (or `STATUS-archive.md`) and start a fresh `STATUS.md` once Phase 3 is in flight. Decide if/when this file starts to feel noisy — current length (~390 lines) is still manageable.

---

## 3. Codebase orientation (current layout)

```
sinfonia/
├── Cargo.toml                   # workspace manifest; shared [workspace.dependencies]
├── Cargo.lock
├── crates/
│   ├── sinfonia/                # the daemon (existing)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── agent/           # raw + CLI agent backends (incl. opencode.rs from P2)
│   │   │   ├── config/          # WORKFLOW.md parser; typed config
│   │   │   ├── domain.rs        # orchestrator-only types + re-exports
│   │   │   ├── errors.rs        # sinfonia::Error (wraps tracker::Error)
│   │   │   ├── http/            # axum dashboard + /api/v1/state
│   │   │   ├── lib.rs
│   │   │   ├── main.rs          # clap entry
│   │   │   ├── orchestrator/    # polling, dispatch, runner, retries
│   │   │   ├── template.rs      # Liquid prompt rendering (H-1 well-known seed)
│   │   │   ├── tracker.rs       # shim re-exporting sinfonia_tracker + build_from_config
│   │   │   └── workspace/       # per-issue dir + hooks
│   │   └── tests/
│   │       └── spec_conformance.rs
│   ├── sinfonia-tracker/        # shared tracker abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs           # IssueTracker trait + re-exports
│   │       ├── config.rs        # TrackerKind, TrackerConfig
│   │       ├── custom_fields.rs # CustomFieldValue, MARKER, WELL_KNOWN_FIELDS, codec
│   │       ├── error.rs         # tracker-specific Error
│   │       ├── jira.rs          # JiraTracker (defaults for bridge-write methods)
│   │       ├── linear.rs        # LinearTracker (full bridge-write impls)
│   │       └── types.rs         # Issue (with .fields), IssueState, BlockerRef, ChildRef
│   └── sinfonia-bridge/         # the bridge daemon — Phase 1 complete (config + webhook + feedback loop + GitHub auth + --self-test + wiremock integration tests + Phase 1 docs all landed; v0.3.0-alpha.1)
├── docs/
│   ├── SPEC.md                  # Symphony spec; §11.6 (bridge extension contract) draft landed in P1-I
│   └── v0.3-plan/
│       ├── 00-overview.md       # milestone index + revision history
│       ├── 01-bridge-mvp.md     # Phase 1 plan (source of truth)
│       ├── 02-..07-…            # later-phase plans
│       └── STATUS.md            # this file
├── scripts/
│   └── verify-workspace-move.sh # one-shot gate for the P1-A commit
├── Dockerfile                   # current dev-shell image; refactored in Phase 6
├── docker-compose.yml           # current dev-shell compose; refactored in Phase 6
├── BRIDGE.example.md            # new in P1-I — fully-commented working bridge config (parses under `--check`)
├── README.md                    # "What's new in v0.3 (preview)" stub landed in P1-I; Phase 7 expands
├── CHANGELOG.md                 # [0.3.0-alpha.1] entry landed in P1-I
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

---

## 6. Resume protocol — first commands a fresh agent should run

```bash
# 1. Land on a clean main with the full Phase 1 of v0.3.
git checkout main
git pull --ff-only origin main

# 2. Confirm test baseline (should be 158 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file), the milestone overview, and the
#    Phase 3 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/00-overview.md
cat docs/v0.3-plan/03-telemetry-budget.md

# 4. Spot-check Phase 1 + 2 deliverables on disk (none of these should error).
ls crates/sinfonia-bridge/src/        # expect: config.rs, lib.rs, main.rs, storage.rs, labels.rs, selftest.rs, webhook/, feedback/, github/
ls crates/sinfonia-bridge/tests/      # expect: bridge_e2e.rs
ls crates/sinfonia/src/agent/opencode.rs    # Phase 2: the OpenCode CLI backend
ls BRIDGE.example.md                  # Phase 1 docs, P1-I
cargo run -q -p sinfonia-bridge -- BRIDGE.example.md --check  # expect: ok

# 5. Read the Phase-3 hand-off blueprint before writing any code.
#    Phase 3 layers tracing-opentelemetry over the existing subscribers and
#    adds a typed Sinfonia→bridge event channel; the existing tracing wiring
#    in both main.rs files is what gets wrapped.
cat crates/sinfonia/src/main.rs            # subscriber init
cat crates/sinfonia-bridge/src/main.rs     # symmetric init; will gain /events route
cat crates/sinfonia/src/agent/turn.rs      # where runner.turn / runner.session spans land
cat crates/sinfonia/src/orchestrator/runner.rs

# 6. Start a Phase 3 branch off main.
git checkout -b v0.3-phase-3-telemetry
```

Phase 1 + Phase 2 of v0.3 are complete (P1-A..P1-I + P2 all merged). Phase 3 (Telemetry + budget enforcement) is the next pickup; Phases 4..7 follow in plan-doc order unless a parallel run is desired (see §7 below for which inter-phase dependencies are real — Phase 4 is the next plausible parallel candidate after Phase 3 starts, since it only depends on Phase 1 and touches disjoint code).

---

## 7. Open issues to track across phases

From the second-pass `gsd-plan-checker` review. The originals are in `docs/v0.3-plan/00-overview.md` "Plan revision history" — repeated here as a checklist so they're picked up at the right phase.

### Deferred from the plan-checker review

| ID | Gist | Resolve when |
|---|---|---|
| **M-1** | Phase 2 depends only on Phase 1's workspace conversion, not the rest of P1. Parallelism unlock. | ✅ Closed by Phase 2 merge (PR #9, commit `3b84a20`, merge `f26aca7`). No longer applicable. |
| **M-2** | Cost-table drift gate is asymmetric. Refuse cost caps (not token caps) when table >180 days old. | Phase 3 implementation (next pickup). The embedded cost table lives in the budget-enforcement module per `03-telemetry-budget.md`; the drift gate fires at config-load time. |
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
| Vendor-CLI flag deltas (Phase 2 plan-doc surface didn't match `opencode.ai/docs`) | `opencode.rs` rustdoc + `docs/v0.3-plan/02-opencode-VERIFY.md` | ✅ resolved in P2; pattern documented in §5.10 for future phases (especially Phase 3's OTel exporter env vars). |
| Manual end-to-end verification of OpenCode against a real Linear project | `docs/v0.3-plan/02-opencode-VERIFY.md` §5.3 | Before tagging `v0.3.0-alpha.x` to an actual release. The 9 new tests cover logic; manual run confirms the OpenCode auth dance and the per-state routing. |
| `cli.rs` preflight could migrate to `which` now that it's a workspace dep | `crates/sinfonia/src/agent/cli.rs` preflight helper | Optional; below threshold for Phase 3. Could fold into Phase 6 if `cli.rs` gets touched anyway for docker-image work. See §5.12. |

---

## 8. Pointer reference (paste-ready)

For the next agent's first message to itself when context is fresh:

```
Working directory: /Users/brettlee/work/sinfonia
Current branch: main (assumed; verify with `git branch --show-current`)
Last merged work: P2 OpenCode agent backend (PR #9, commit 3b84a20, merge f26aca7)
                  — **Phase 2 of v0.3 is now complete.**
Earlier merges: P1-I Phase 1 docs (PR #8, commit a057218, merge e8f224a)
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
  3. docs/v0.3-plan/03-telemetry-budget.md   (Phase 3 plan; next pickup)
     — and skim the four hand-off files in §2 above before writing code:
     crates/sinfonia/src/main.rs (subscriber init that Phase 3 wraps),
     crates/sinfonia-bridge/src/main.rs (symmetric init + future /events),
     crates/sinfonia/src/agent/turn.rs and
     crates/sinfonia/src/orchestrator/runner.rs (where the runner.session
     / runner.turn spans land).

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon; Phase 2 added src/agent/opencode.rs
                             as a sibling of cli.rs. Phase 3 wraps
                             src/main.rs subscriber init and adds an OTel
                             layer.
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters,
                             custom_fields). Phase 3 adds new
                             budget-related keys to WELL_KNOWN_FIELDS
                             (see §5.2 and the §7 "Found during P1"
                             checklist).
  crates/sinfonia-bridge/  — bridge binary; Phase 1 complete, ships as
                             v0.3.0-alpha.1. Phase 3 adds the /events
                             route + budget-enforcement module.

Test baseline: 158 passing, 0 failures (40 sinfonia unit + 13 conformance
+ 7 tracker + 89 bridge unit + 9 bridge integration). Phase 3 adds
~300 LOC of tests per 03-telemetry-budget.md; do not regress the
existing 158.
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

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Phase 3 picks up from a fresh context with this doc as the entry point.
