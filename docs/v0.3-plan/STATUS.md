# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix), the bridge skeleton (#3 — P1-D: `sinfonia-bridge` crate, BRIDGE.md parser, axum router with `/health` + stub `/webhook`), the real webhook layer (#4 — P1-E: HMAC-SHA256 verification, SQLite idempotency, `pull_request` / `check_suite` / `workflow_run` dispatch), the feedback loop (#5 — P1-F: `evaluate_ci` orchestrator, categorize / attempts / transition modules, `LabelManager` + `BridgeLabel`, PAT-mode `GhOps`), and GitHub authentication + the install gate (#6 — P1-G: `github::auth` mode selector, `AppModeGhOps` with per-owner installation cache, `sinfonia-bridge --self-test` runner, `server.public_url` config field).

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

Phase 1 *foundation* (P1-A / P1-B / P1-C), the bridge crate skeleton (P1-D), the webhook layer (P1-E), the feedback loop (P1-F), **and** GitHub authentication + the install gate (P1-G — `BridgeAuthMode` mode selector, `AppModeGhOps` with per-owner installation cache, `sinfonia-bridge --self-test`, `server.public_url`) are merged to `main`. The remaining two Phase 1 deliverables (P1-H / P1-I) — integration tests with `wiremock` and the Phase 1 docs — have not been started. Pick up at **P1-H: integration tests with `wiremock`**. The phase plan in `01-bridge-mvp.md` §9.2 is the source of truth; this STATUS doc captures the *implementation* deltas you need to know about that aren't in the plan.

The single most important non-obvious decision made during the foundation work: **`CustomFieldValue` was collapsed from four variants (`Null` / `Number` / `Decimal` / `LongText` / `Url`) to three (`Null` / `Number` / `String`)** because serde's `#[serde(untagged)]` deserializer can't distinguish multiple JSON-string variants. See §5 below.

The single most important non-obvious decision made during P1-G: the GitHub mock surface for P1-H *doesn't need any new escape hatches* in the bridge. `OctocrabGhOps::from_octocrab(Arc<Octocrab>)` and `AppModeGhOps::new(Arc<Octocrab>)` are already public — so the integration suite constructs a wiremock-pointed `Octocrab` via `Octocrab::builder().base_uri(mock.uri())?.build()` and wraps it directly, without touching `github::auth::build_gh_ops`. App-mode installation-scoped clients inherit the configured `BaseUriLayer` automatically because `Octocrab::installation(id)` clones the inner `client`; the JWT-mode installation-discovery and access-token endpoints just need to be mocked alongside the per-repo REST routes.

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
| **P1-H** integration tests with `wiremock` | §9.2 | ⬜ not started | Next deliverable — nine scenarios |
| **P1-I** Phase 1 docs (BRIDGE.example.md, SPEC §11.6 draft, CHANGELOG, README stub) | §12 | ⬜ not started | |

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` → **140 tests pass, 0 failures**
  - 32 sinfonia unit tests (mostly under `crates/sinfonia/src/agent/cli/tests`, `config/`, `orchestrator/`, `template/`, `workspace/`)
  - 12 `spec_conformance.rs` integration tests
  - 7 sinfonia-tracker tests (1 base64 + 6 custom_fields)
  - 89 sinfonia-bridge unit tests (67 from P1-D+P1-E+P1-F plus 22 from P1-G covering `github::auth`, `selftest`, and the new `config::server.public_url` round-trips)
- `cargo run -p sinfonia-bridge -- BRIDGE.md --check` → `ok` (exit 0) on valid, descriptive error (exit 1) on invalid
- `cargo run -p sinfonia-bridge -- BRIDGE.md --self-test` → one labelled `PASS`/`FAIL`/`SKIP` line per check; exit code = number of `FAIL` lines (SKIPs don't count). Wiremock-driven coverage of App-mode token-mint + REST round-trip lands in P1-H scenario 8.
- `GET /health` returns `{"service":"sinfonia-bridge","status":"ok","tenant_id":<...>,"tracker":"linear"}`
- `POST /webhook` flow (verified end-to-end with manual curl + sqlite3 row dump in P1-E; P1-F adds the action paths exercised by the 28 unit tests; P1-G adds the App-mode auth path; full wiremock coverage of the nine scenarios in §9.2 lands in P1-H):
  - Signed `pull_request opened` → 202 + row in `pr_ticket_map`.
  - Same `delivery_id` redelivered → 200 `{"status":"duplicate"}`, no DB mutation.
  - Wrong HMAC → 401, no DB mutation.
  - Signed `check_suite completed` with a mapped PR → 202 + per-PR `CiOutcome` (green / red / cap_hit / pending / no_mapped_pr) returned in the JSON body. PAT-mode `octocrab` calls reach the live GitHub API; App-mode constructs an `AppModeGhOps` whose installation-discovery + token-mint flow is exercised by P1-H scenario 8.
- `scripts/verify-workspace-move.sh` → no longer applicable post-merge (the script's purpose was to gate the workspace-move commit; it can stay in-repo as a one-shot artifact)

---

## 2. What's next: P1-H — Integration tests with `wiremock`

The next concrete deliverable. Source of truth: `docs/v0.3-plan/01-bridge-mvp.md` §9.2 (Integration tests). This is the deliverable that proves the full webhook → feedback-loop → tracker-write chain works against real HTTP — every prior sub-task has been covered by unit tests with trait fakes; P1-H exercises the production `OctocrabGhOps` / `AppModeGhOps` and the production `LinearTracker` against canned mock servers.

### Scope

A new `crates/sinfonia-bridge/tests/bridge_e2e.rs` integration suite that:

- Boots the full bridge daemon under `tokio::test` on a random local port (`tokio::net::TcpListener::bind("127.0.0.1:0")` → `axum::serve(...)` → `Router` built from `webhook::router(state)`).
- Stands up a `wiremock::MockServer` for GitHub. The bridge's `OctocrabGhOps::from_octocrab(crab)` and `AppModeGhOps::new(crab)` accept a pre-built `Octocrab`, so the test builds one via `Octocrab::builder().base_uri(mock.uri())?.{personal_token | app}(…).build()?` and wraps it directly — *no new escape hatches in `github::auth`*.
- Stands up a second `wiremock::MockServer` for Linear. `tracker.endpoint` is already config-driven (P1-D), so `BRIDGE.md` simply points at `<mock_linear.uri()>/`. A single `Respond` impl (`LinearGraphqlMock`) dispatches by GraphQL query keyword (`comments(first:` → load marker, `team {` `states(first:` → resolve state id, `commentCreate(` / `commentUpdate(` → store marker, `issueUpdate(` → record transition).
- Has a `post_webhook_signed` helper that HMAC-SHA256-signs the body with the configured `webhook_secret` and POSTs to the bridge's `/webhook` — i.e. exactly the bytes GitHub would send.
- Has a per-scenario `LinearMockState` (`Arc<Mutex<…>>`) so a sequence of write_custom_field / load_marker_comment calls round-trips correctly across the bridge's load-modify-store path (the marker is rewritten on every counter increment).

### Scenario coverage (plan §9.2)

| # | Name | Asserts |
|---|---|---|
| 1 | Green PR | `awaiting-review` label POST'd; no Linear transitions; no PR comment |
| 2 | One red CI run | Counter 0→1; `Needs Fixes` transition; `needs-fixes` label apply; failure comment posted |
| 3 | Three red runs with category routing | Counter 1→2→3; category-specific transitions (`Needs Lint Fixes` / `Needs E2E Fixes` / `Needs Lint Fixes`); `sinfonia:failure:lint` vs `sinfonia:failure:e2e` labels each iteration |
| 4 | Cap hit | Pre-seeded marker at `attempt_count=3`; one red → `Blocked - Human Review` transition; counter does NOT advance past `max_attempts`; `cap-hit` label apply |
| 5 | Webhook redelivery | Second POST with same `X-GitHub-Delivery` → 200 `{"status":"duplicate"}`; no downstream calls |
| 6 | Signature failure | 401; zero outbound calls to either mock |
| 7 | PR without a tracker link | 200 ignored; no mapping row; no outbound calls |
| 8 | GitHub App auth | Same as #2 but via `AppModeGhOps` — exercises `/repos/{owner}/{repo}/installation` discovery + `POST /app/installations/{id}/access_tokens` token mint before the labels/comments REST calls |
| 9 | `manage_labels: false` | Transition + failure comment still fire; zero label endpoint calls |

### What's already prepared

- `Store::open_in_memory()` exists for unit tests; P1-H promotes it out of `#[cfg(test)]` so the integration suite can use it without writing to a shared file path. One-line edit in `storage.rs`.
- `OctocrabGhOps::from_octocrab(Arc<Octocrab>)` and `AppModeGhOps::new(Arc<Octocrab>)` are already public (P1-F / P1-G).
- `LinearTracker::new(&TrackerConfig)` already takes the endpoint from config.
- The handler unit-test `ScriptedGh` pattern (in `webhook/handlers.rs::tests`) is the blueprint for assertion style — each test inspects which label/comment endpoints the mock saw.
- `hmac` and `sha2` are already in `[dependencies]`, so the signed-webhook helper compiles without dev-dep additions.
- `octocrab::OctocrabBuilder::base_uri` is the canonical mock-injection point — verified by reading the octocrab 0.39 source.

### Cargo additions

- `wiremock = "0.6"` to `[dev-dependencies]` of `sinfonia-bridge` (already left a TODO comment for it in P1-D).

### Unit tests / structure

The integration suite is a single file (`tests/bridge_e2e.rs`) totalling ~9 `#[tokio::test]`s plus ~600 lines of fixtures + mock helpers. Each scenario constructs its own GitHub and Linear `MockServer`, so tests are fully isolated and can run in parallel.

### Exit criteria for P1-H

- `cargo check --workspace --tests` compiles clean (no new warnings beyond pre-existing ones).
- `cargo test --workspace --no-fail-fast` → at least 149 tests pass (140 post-P1-G + 9 new integration scenarios), zero failures.
- Each of the nine scenarios named in §9.2 maps to a `#[tokio::test]` that asserts on both the wiremock call set and the bridge's HTTP response body — not just "test compiles."

### Files (P1-H owns the file marked with †)

```
crates/sinfonia-bridge/
├── Cargo.toml                   ← P1-D … P1-G. P1-H: +`wiremock = "0.6"` dev-dep.
├── src/
│   ├── storage.rs               ← P1-E. P1-H: drop `#[cfg(test)]` gate on `open_in_memory` so integration tests can use it.
│   ├── … (everything else unchanged)
└── tests/
    └── bridge_e2e.rs            ← P1-H †  nine scenarios + shared GitHub/Linear mock helpers + RSA test key for App-mode.
```

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
│   └── sinfonia-bridge/         # the bridge daemon (config + webhook layer + feedback loop + GitHub auth + --self-test all landed; integration tests are P1-H)
├── docs/
│   ├── SPEC.md                  # Symphony spec; §11.6 draft lands in P1-I
│   └── v0.3-plan/
│       ├── 00-overview.md       # milestone index + revision history
│       ├── 01-bridge-mvp.md     # Phase 1 plan (source of truth)
│       ├── 02-..07-…            # later-phase plans
│       └── STATUS.md            # this file
├── scripts/
│   └── verify-workspace-move.sh # one-shot gate for the P1-A commit
├── Dockerfile                   # current dev-shell image; refactored in Phase 6
├── docker-compose.yml           # current dev-shell compose; refactored in Phase 6
├── README.md                    # updated in Phase 7
├── CHANGELOG.md                 # v0.3.0-alpha.1 entry lands with P1-I
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

**Implication:** Linear API payload per ticket is larger now. For tickets with >100 comments where the marker arrived early, the marker WILL still be in the first 100 (Linear's `comments` ordering is creation-ascending by default; the bridge creates the marker on first interaction, so it's always near the front). For tickets with >100 *bot interactions*, the marker would scroll off — document that boundary in `docs/SPEC.md` §11.6 when P1-I writes it.

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
# 1. Land on a clean main with all merged work.
git checkout main
git pull --ff-only origin main

# 2. Confirm test baseline (should be 140 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file) and the Phase 1 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/01-bridge-mvp.md

# 4. Confirm the working tree shape matches §3 above.
ls crates/
ls crates/sinfonia-bridge/src/        # expect: config.rs, lib.rs, main.rs, storage.rs, labels.rs, selftest.rs, webhook/, feedback/, github/
ls crates/sinfonia-bridge/src/webhook/  # expect: handlers.rs, mod.rs, verify.rs
ls crates/sinfonia-bridge/src/feedback/ # expect: attempts.rs, categorize.rs, mod.rs, transition.rs
ls crates/sinfonia-bridge/src/github/   # expect: auth.rs, client.rs, mod.rs
ls crates/sinfonia-tracker/src/

# 5. Start a P1-H branch off main.
git checkout -b v0.3-phase-1-integration-tests

# 6. Set up Phase 1 sub-task tracking. The original task IDs from the
#    completed-context conversation are not preserved across context
#    clears — TaskCreate a fresh set for P1-H..P1-I per the table in §1.
```

Previous sessions completed P1-A / P1-B / P1-C / P1-D / P1-E / P1-F / P1-G. Two sub-tasks remain (P1-H, P1-I); recreate those as fresh TaskCreate entries.

---

## 7. Open issues to track across phases

From the second-pass `gsd-plan-checker` review. The originals are in `docs/v0.3-plan/00-overview.md` "Plan revision history" — repeated here as a checklist so they're picked up at the right phase.

### Deferred from the plan-checker review

| ID | Gist | Resolve when |
|---|---|---|
| **M-1** | Phase 2 depends only on Phase 1's workspace conversion (now landed), not the rest of P1. Parallelism unlock. | Already unlocked — Phase 2 work can start any time. |
| **M-2** | Cost-table drift gate is asymmetric. Refuse cost caps (not token caps) when table >180 days old. | Phase 3 implementation. |
| **M-4** | (Closed) §6's "tracker poll every 60s" was rewritten to webhook-driven. | N/A — done. |
| **M-8** | `inquire` should be `crates/sinfonia/Cargo.toml`-scoped, not workspace-scoped. | Phase 5. |

### Found during P1 foundation implementation

| Topic | Where | When to address |
|---|---|---|
| `WELL_KNOWN_FIELDS` boundary semantics — what happens when the bridge writes a key outside the list | `custom_fields.rs` | When writing the `docs/SPEC.md` §11.6 draft (P1-I): make this a contract item. |
| `comments(first: 100)` upper limit when bot has >100 interactions | `linear.rs` ISSUE_FRAGMENT | Document in §11.6 (P1-I). Move toward `comments(orderBy: createdAt, first: 5)` or marker-by-author filtering if anyone hits the limit. |
| `JiraTracker` raw_graphql returns the tracker crate's stock "not supported" error since we moved it; sinfonia's old behavior was equivalent. No action needed unless someone files an issue. | `linear.rs::raw_graphql` vs `jira.rs::raw_graphql` | N/A — confirmed equivalent. |

---

## 8. Pointer reference (paste-ready)

For the next agent's first message to itself when context is fresh:

```
Working directory: /Users/brettlee/work/sinfonia
Current branch: main (assumed; verify with `git branch --show-current`)
Last merged work: P1-G GitHub auth (PAT + App) + `--self-test` (PR #6, commit b0d7272, merge 8055659)
Earlier merges: P1-F feedback loop (PR #5, commit 9d33d51);
                P1-E webhook layer (PR #4, commit 69eb8e0);
                P1-D bridge skeleton (PR #3, commit 07c0381);
                Phase 1 foundation (PR #2: commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/01-bridge-mvp.md   (Phase 1 plan; next deliverable is P1-H in §9.2)

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters, custom_fields)
  crates/sinfonia-bridge/  — bridge binary; config + storage + webhook dispatch + feedback loop + PAT/App auth + --self-test landed,
                             integration tests (P1-H) and Phase 1 docs (P1-I) still to come

Test baseline: 140 passing, 0 failures. Maintain that.
```

---

## 9. What success looks like at the end of Phase 1

When P1-D through P1-I are all merged:

- `crates/sinfonia-bridge/` is a complete, tested binary that:
  - Parses a `BRIDGE.md` config and exits cleanly on invalid input.
  - Verifies GitHub webhook signatures with HMAC-SHA256.
  - Persists webhook delivery IDs in SQLite for idempotency.
  - Maps `pull_request` / `check_suite` / `workflow_run` events to ticket-state transitions.
  - Routes by failure category when configured.
  - Hits the attempt cap → moves to `blocked_state`.
  - Manages PR labels under the `sinfonia:` prefix.
  - Supports both GitHub PAT and App auth.
  - Exposes `sinfonia-bridge --self-test` with the canonical PASS/FAIL output.
- `BRIDGE.example.md` documents every config field.
- `docs/SPEC.md` §11.6 draft describes the bridge-service extension contract.
- `CHANGELOG.md` has a v0.3.0-alpha.1 entry.
- README has a "What's new in v0.3" stub.
- `wiremock`-backed integration tests cover the nine scenarios in `01-bridge-mvp.md` §9.2.
- The full `cargo test --workspace` count is 51 sinfonia/tracker + 89 bridge unit tests (67 from P1-D+P1-E+P1-F plus 22 from P1-G) + 9 integration tests = 149 tests as of P1-H, growing modestly as P1-I lands docs-only additions.

Phase 1 is then shippable on its own; Phases 2–7 land on top of it.

---

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Ready to resume from a fresh context with this doc as the entry point.
