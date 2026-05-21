# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21 (P1-I merged — **Phase 1 complete**; Phase 2 is the next pickup)
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the **complete Phase 1 of v0.3** — all nine sub-tasks (P1-A through P1-I) are merged. The merged work, in order: the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix), the bridge skeleton (#3 — P1-D: `sinfonia-bridge` crate, BRIDGE.md parser, axum router with `/health` + stub `/webhook`), the real webhook layer (#4 — P1-E: HMAC-SHA256 verification, SQLite idempotency, `pull_request` / `check_suite` / `workflow_run` dispatch), the feedback loop (#5 — P1-F: `evaluate_ci` orchestrator, categorize / attempts / transition modules, `LabelManager` + `BridgeLabel`, PAT-mode `GhOps`), GitHub authentication + the install gate (#6 — P1-G: `github::auth` mode selector, `AppModeGhOps` with per-owner installation cache, `sinfonia-bridge --self-test` runner, `server.public_url` config field), the wiremock-backed integration suite (#7 — P1-H: `tests/bridge_e2e.rs` boots the full daemon against per-test GitHub + Linear `MockServer`s and asserts on all nine §9.2 scenarios end-to-end), **and** the Phase 1 docs (#8 — P1-I: `BRIDGE.example.md` at the repo root, `docs/SPEC.md` §11.6 draft of the bridge extension contract, `CHANGELOG.md` entry for v0.3.0-alpha.1, README "What's new in v0.3 (preview)" stub).

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

**Phase 1 of v0.3 is complete.** All nine sub-tasks (P1-A through P1-I) are merged to `main`; the bridge MVP ships as v0.3.0-alpha.1. The bridge binary parses `BRIDGE.md`, verifies HMAC-signed GitHub webhooks, persists delivery-ID idempotency in SQLite, evaluates CI results, routes by failure category, applies the attempt cap, manages PR labels under the `sinfonia:` prefix, supports both PAT and App auth, and exposes `sinfonia-bridge --self-test` as the install gate. The full chain — bridge writes the marker comment → tracker fetch parses it into `Issue.fields` → prompt template renders `{{ issue.fields.sinfonia_last_ci_failure }}` into the agent's input — works end-to-end. Workspace test count: 149 passing (32 sinfonia + 12 conformance + 7 tracker + 89 bridge unit + 9 bridge integration), zero failures.

The next pickup is **Phase 2 — OpenCode agent backend** (`docs/v0.3-plan/02-opencode-backend.md`). Phase 2 depends only on Phase 1's workspace conversion (long since merged) — that dependency unlocked back at the P1-A merge. The remaining Phase 1 commits (P1-D..P1-I) were all in the bridge crate, which Phase 2 doesn't touch. Per `00-overview.md`'s deferred-finding M-1, Phase 2 has been runnable in parallel with later Phase 1 work for months; it's just been queued behind the v0.3 milestone's natural left-to-right ordering. Pick it up from `02-opencode-backend.md` §1.

The single most important non-obvious decision the foundation work bequeaths to Phase 2: **`crates/sinfonia/src/agent/cli/` is the canonical model** for any subprocess-driven backend. OpenCode joins `claude_code` and `codex` as a sibling under `agent/cli.rs`'s parser + spawn pattern; do not invent a new abstraction. The `template.rs` Liquid scope (with the H-1 well-known field pre-seed from §5.2) is also part of what Phase 2 inherits — any new backend automatically gets `{{ issue.fields.* }}` access for the bridge-written counters with no extra wiring.

The single most important non-obvious decision made during Phase 1 itself (forward-relevant to every later phase): **`CustomFieldValue` is three variants (`Null` / `Number` / `String`), not five** (the plan-doc §4 draft showed `Decimal` / `LongText` / `Url` as separate variants). Cost values, URLs, and long-text fields all serialize as `String`. See §5.1 below. This is the foundation Phase 3's budget caps and Phase 4's Jira custom-field writes both build on.

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
| (this commit) | STATUS: mark P1-I merged, Phase 1 complete | Docs — this file |

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

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` → **149 tests pass, 0 failures**
  - 32 sinfonia unit tests (mostly under `crates/sinfonia/src/agent/cli/tests`, `config/`, `orchestrator/`, `template/`, `workspace/`)
  - 12 `spec_conformance.rs` integration tests
  - 7 sinfonia-tracker tests (1 base64 + 6 custom_fields)
  - 89 sinfonia-bridge unit tests (67 from P1-D+P1-E+P1-F plus 22 from P1-G covering `github::auth`, `selftest`, and the `config::server.public_url` round-trips)
  - 9 sinfonia-bridge integration tests (`tests/bridge_e2e.rs`, one per §9.2 scenario)
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

## 2. What's next: Phase 2 — OpenCode agent backend

Phase 1 is shippable. The natural next pickup is **Phase 2 — OpenCode agent backend** (`docs/v0.3-plan/02-opencode-backend.md`). Phase 2 has been runnable in parallel since the P1-A workspace conversion landed (deferred plan-checker finding M-1, now in §7 below) — the v0.3 milestone just queued it behind Phase 1's natural left-to-right ordering.

### Scope (per `02-opencode-backend.md`)

Add `provider: opencode` as a first-class subprocess-driven coding-agent backend, joining `claude_code` and `codex`. Raw LLM backends (`anthropic` / `openai` / `google` / `ollama`) stay. The plan estimates **~400 LOC of Rust + ~150 LOC of tests + ~100 LOC of docs**.

The work is naturally a sibling of `crates/sinfonia/src/agent/cli.rs` — the existing CLI-subprocess pattern that drives `claude_code` and `codex` is the blueprint. OpenCode joins that family rather than inventing a new abstraction. The OpenCode CLI owns auth and provider selection internally (75+ providers including Ollama), so Sinfonia's surface is just: spawn, pipe prompt via stdin, parse the line-delimited JSON event stream on stdout, support `--continue <session_id>` for retry turns.

### What's already prepared for Phase 2 (inherited from Phase 1)

- **Workspace shape.** `crates/sinfonia/src/agent/` is the right home for `opencode.rs`. The workspace conversion (P1-A) was Phase 2's blocking dependency.
- **CLI backend pattern.** `crates/sinfonia/src/agent/cli/` already has the parser + spawn + continuation infrastructure that OpenCode reuses. Read `cli.rs` end-to-end before writing `opencode.rs`; the goal is to drop a sibling backend in, not to refactor the surrounding module.
- **Liquid template scope with `issue.fields`.** Any new backend automatically gets `{{ issue.fields.sinfonia_* }}` access in its per-state prompt — wired in P1-C, well-known keys pre-seeded by `template.rs` (§5.2). Phase 2 doesn't need to touch this.
- **Per-state runner overrides.** `WORKFLOW.md`'s `states:` block (and the `StateOverride` parser in `config/typed.rs`) already lets users route specific tracker states at OpenCode. No state-machine change is required for Phase 2 routing; the wiring is already there.

### Phase 1 → Phase 2 hand-off pointer

Read these in this order before starting Phase 2:

1. `docs/v0.3-plan/02-opencode-backend.md` — the Phase 2 plan. Source of truth for scope, validation rules, test coverage.
2. `crates/sinfonia/src/agent/cli.rs` (and `cli/`) — the existing subprocess-backend pattern OpenCode mirrors.
3. `crates/sinfonia/src/agent/mod.rs` — provider enumeration and dispatch.
4. `WORKFLOW.example.md` — the `provider: claude_code` / `provider: codex` examples are what `provider: opencode` should slot next to.

Then `git checkout -b v0.3-phase-2-opencode` off `main` (149-test baseline) and start there.

### Other Phase 1 follow-up work (not blocking Phase 2)

These items surfaced during Phase 1 but were not in P1-A..P1-I scope. They are not blockers; surface them at the right time:

- **CI for the bridge crate.** Wire `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` into CI so schema drift between `config.rs` and the example doc fails the build. Listed in §1 as one of the Phase 1 deliverables' implicit guarantees but not currently enforced by a CI job — add it when CI is being touched anyway.
- **Manual verification.** The Phase 1 plan §9.3 calls for end-to-end verification against a real Linear project + sandbox GitHub repo before declaring the milestone shippable. The 149 automated tests cover the logic; manual verification confirms the credentials/auth dance against real services. Plan §9.3 says this is documented in `docs/v0.3-plan/01-bridge-mvp-VERIFY.md` (not yet written; do this before tagging v0.3.0-alpha.1 if it goes to actual release).
- **STATUS doc retire path.** Once Phase 2..7 work begins, this file's "Phase 1" framing becomes archaeological. Two options: (a) keep accreting new sections per phase, (b) freeze this as `docs/v0.3-plan/STATUS-phase-1.md` and start a fresh `STATUS.md` for the active phase. Decide at the start of whichever phase first feels noisy in this layout.

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
│   │   │   ├── agent/           # raw + CLI agent backends
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

These are deltas from the plan docs — choices made during P1-A/B/C implementation that future work needs to respect.

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

---

## 6. Resume protocol — first commands a fresh agent should run

```bash
# 1. Land on a clean main with the full Phase 1 of v0.3.
git checkout main
git pull --ff-only origin main

# 2. Confirm test baseline (should be 149 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file), the milestone overview, and the
#    Phase 2 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/00-overview.md
cat docs/v0.3-plan/02-opencode-backend.md

# 4. Spot-check Phase 1's deliverables on disk (none of these should error).
ls crates/sinfonia-bridge/src/        # expect: config.rs, lib.rs, main.rs, storage.rs, labels.rs, selftest.rs, webhook/, feedback/, github/
ls crates/sinfonia-bridge/tests/      # expect: bridge_e2e.rs
ls BRIDGE.example.md                  # Phase 1 docs, P1-I
cargo run -q -p sinfonia-bridge -- BRIDGE.example.md --check  # expect: ok

# 5. Read the Phase-2 hand-off blueprint (the existing CLI backend OpenCode
#    mirrors) before writing any code.
cat crates/sinfonia/src/agent/cli.rs

# 6. Start a Phase 2 branch off main and TaskCreate a fresh sub-task list
#    per `02-opencode-backend.md` §2 (the surface-area table is the natural
#    seed for tasks).
git checkout -b v0.3-phase-2-opencode
```

Phase 1 of v0.3 is complete: P1-A through P1-I are all merged. Phase 2 (OpenCode agent backend) is the next pickup; Phases 3..7 follow in plan-doc order unless a parallel run is desired (see §7 below for which inter-phase dependencies are real).

---

## 7. Open issues to track across phases

From the second-pass `gsd-plan-checker` review. The originals are in `docs/v0.3-plan/00-overview.md` "Plan revision history" — repeated here as a checklist so they're picked up at the right phase.

### Deferred from the plan-checker review

| ID | Gist | Resolve when |
|---|---|---|
| **M-1** | Phase 2 depends only on Phase 1's workspace conversion, not the rest of P1. Parallelism unlock. | Already unlocked since P1-A; with Phase 1 now fully complete this is moot — Phase 2 is simply the next phase to start. |
| **M-2** | Cost-table drift gate is asymmetric. Refuse cost caps (not token caps) when table >180 days old. | Phase 3 implementation. |
| **M-4** | (Closed) §6's "tracker poll every 60s" was rewritten to webhook-driven. | N/A — done. |
| **M-8** | `inquire` should be `crates/sinfonia/Cargo.toml`-scoped, not workspace-scoped. | Phase 5. |

### Found during P1 foundation implementation

| Topic | Where | When to address |
|---|---|---|
| `WELL_KNOWN_FIELDS` boundary semantics — what happens when the bridge writes a key outside the list | `custom_fields.rs` | ✅ documented in `docs/SPEC.md` §11.6.4 (P1-I). |
| `comments(first: 100)` upper limit when bot has >100 interactions | `linear.rs` ISSUE_FRAGMENT | ✅ documented in `docs/SPEC.md` §11.6.7 (P1-I) with RECOMMENDED mitigations. Open: implement `comments(orderBy: createdAt, first: 5)` or marker-by-author filtering if anyone hits the limit. |
| `JiraTracker` raw_graphql returns the tracker crate's stock "not supported" error since we moved it; sinfonia's old behavior was equivalent. No action needed unless someone files an issue. | `linear.rs::raw_graphql` vs `jira.rs::raw_graphql` | N/A — confirmed equivalent. |
| CI hookup for the bridge crate (`cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` as a docs-side gate) | repo CI config | When CI is being touched anyway — surface as a follow-up if not already in scope. |
| Manual end-to-end verification against a real Linear project + sandbox GitHub repo (`docs/v0.3-plan/01-bridge-mvp-VERIFY.md`, plan §9.3) | manual ops doc | Before tagging `v0.3.0-alpha.1` to an actual release. The 149 automated tests cover logic; manual run confirms the auth dance. |

---

## 8. Pointer reference (paste-ready)

For the next agent's first message to itself when context is fresh:

```
Working directory: /Users/brettlee/work/sinfonia
Current branch: main (assumed; verify with `git branch --show-current`)
Last merged work: P1-I Phase 1 documentation (PR #8, commit a057218, merge e8f224a)
                  — **Phase 1 of v0.3 is now complete.**
Earlier merges: P1-H wiremock integration tests (PR #7, commit d7ad72d, merge 749c9c4);
                P1-G GitHub auth (PR #6, commit b0d7272, merge 8055659);
                P1-F feedback loop (PR #5, commit 9d33d51);
                P1-E webhook layer (PR #4, commit 69eb8e0);
                P1-D bridge skeleton (PR #3, commit 07c0381);
                Phase 1 foundation (PR #2: commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/02-opencode-backend.md   (Phase 2 plan; next pickup)
     — and skim crates/sinfonia/src/agent/cli.rs (the existing CLI backend
     pattern OpenCode mirrors) before writing code.

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon (Phase 2 lives under src/agent/opencode.rs as a sibling of cli.rs)
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters, custom_fields) — Phase 1 complete
  crates/sinfonia-bridge/  — bridge binary — Phase 1 complete; ships as v0.3.0-alpha.1

Test baseline: 149 passing, 0 failures. Phase 2 adds ~150 LOC of tests
(per 02-opencode-backend.md); do not regress the existing 149.
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

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Phase 2 picks up from a fresh context with this doc as the entry point.
