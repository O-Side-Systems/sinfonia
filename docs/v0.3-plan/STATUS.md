# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix), the bridge skeleton (#3 — P1-D: `sinfonia-bridge` crate, BRIDGE.md parser, axum router with `/health` + stub `/webhook`), the real webhook layer (#4 — P1-E: HMAC-SHA256 verification, SQLite idempotency, `pull_request` / `check_suite` / `workflow_run` dispatch), and the feedback loop (#5 — P1-F: `evaluate_ci` orchestrator, categorize / attempts / transition modules, `LabelManager` + `BridgeLabel`, PAT-mode `GhOps`).

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

Phase 1 *foundation* (P1-A / P1-B / P1-C), the bridge crate skeleton (P1-D), the real webhook layer (P1-E — HMAC verify + SQLite idempotency + `pull_request` / `check_suite` / `workflow_run` dispatch), **and** the feedback loop (P1-F — `evaluate_ci`, categorize, attempts counter, transition, `LabelManager`, PAT-only `GhOps`) are merged to `main`. The remaining three Phase 1 deliverables (P1-G / P1-H / P1-I) — GitHub auth & `--self-test`, integration tests, and the Phase 1 docs — have not been started. Pick up at **P1-G: GitHub auth (PAT + App) + `--self-test`**. The phase plan in `01-bridge-mvp.md` §8 is the source of truth; this STATUS doc captures the *implementation* deltas you need to know about that aren't in the plan.

The single most important non-obvious decision made during the foundation work: **`CustomFieldValue` was collapsed from four variants (`Null` / `Number` / `Decimal` / `LongText` / `Url`) to three (`Null` / `Number` / `String`)** because serde's `#[serde(untagged)]` deserializer can't distinguish multiple JSON-string variants. See §5 below.

The single most important non-obvious decision made during P1-F: the bridge's GitHub surface is **abstracted behind a narrow `GhOps` trait** (in `crates/sinfonia-bridge/src/github/client.rs`) so the feedback loop and the `LabelManager` can be unit-tested with counting fakes without bringing up an HTTP server. P1-G's job is to add a second implementation of that trait (App-mode + per-installation client cache) and the `--self-test` runner — `AppState`, `LabelManager`, `evaluate_ci`, and every handler already take `Arc<dyn GhOps>` and don't need to change.

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

### Phase 1 sub-task status

| Sub-task | Plan section | Status | Notes |
|---|---|---|---|
| **P1-A** workspace conversion + verify script | `01-bridge-mvp.md` §2 | ✅ merged | `crates/sinfonia/`, `crates/sinfonia-tracker/`, `scripts/verify-workspace-move.sh` |
| **P1-B** tracker trait extensions + `custom_fields` module | §4, §11 | ✅ merged | 5 new bridge-write methods on `IssueTracker`; Linear impls; Jira returns `NotImplemented` (Phase 4 fills) |
| **P1-C** `Issue.fields` + Linear marker-comment + template scope (H-1) | §4.2 | ✅ merged | `Issue.fields` populated by Linear fetch; `template.rs` pre-seeds well-known keys |
| **P1-D** bridge binary skeleton + BRIDGE.md config parser | §2, §3 | ✅ merged | `crates/sinfonia-bridge` crate scaffolded; BRIDGE.md parser + 9 validation rules + 16 unit tests; axum router with `/health` + stub `POST /webhook`; `--check` flag |
| **P1-E** webhook handlers + HMAC verify + SQLite idempotency | §5, §9 | ✅ merged | `storage.rs` (rusqlite + Mutex; `processed_deliveries` + `pr_ticket_map`); `webhook/verify.rs` (HMAC-SHA256, constant-time compare); full `POST /webhook` dispatch for `pull_request` / `check_suite` / `workflow_run`; `AppState` carries `Arc<Store>` + `Arc<dyn IssueTracker>`; +23 unit tests |
| **P1-F** feedback loop + categorization + labels | §5.2, §6, §7 | ✅ merged | `feedback/` (`evaluate_ci`, `categorize`, `attempts`, `transition`); `labels.rs` (`BridgeLabel` + `LabelManager`); `github/` (`GhOps` trait + PAT-mode `OctocrabGhOps`); `dispatch_ci_event` wired in `handle_check_suite` / `handle_workflow_run`; bridge crate now at 67 unit tests |
| **P1-G** GitHub auth (PAT + App) + `--self-test` | §8 | ⬜ not started | Next deliverable |
| **P1-H** integration tests with `wiremock` | §9.2 | ⬜ not started | Nine scenarios specified |
| **P1-I** Phase 1 docs (BRIDGE.example.md, SPEC §11.6 draft, CHANGELOG, README stub) | §12 | ⬜ not started | |

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` → **118 tests pass, 0 failures**
  - 32 sinfonia unit tests (mostly under `crates/sinfonia/src/agent/cli/tests`, `config/`, `orchestrator/`, `template/`, `workspace/`)
  - 12 `spec_conformance.rs` integration tests
  - 7 sinfonia-tracker tests (1 base64 + 6 custom_fields)
  - 67 sinfonia-bridge unit tests (16 config from P1-D + 23 from P1-E + 28 from P1-F covering `feedback::categorize`, `feedback::attempts`, `labels`, `feedback::transition`, and the wired-up `dispatch_ci_event` paths in `webhook::handlers`)
- `cargo run -p sinfonia-bridge -- BRIDGE.md --check` → `ok` (exit 0) on valid, descriptive error (exit 1) on invalid
- `GET /health` returns `{"service":"sinfonia-bridge","status":"ok","tenant_id":<...>,"tracker":"linear"}`
- `POST /webhook` flow (verified end-to-end with manual curl + sqlite3 row dump in P1-E; P1-F adds the action paths exercised by the 28 new unit tests; full wiremock coverage of the nine scenarios in §9.2 lands in P1-H):
  - Signed `pull_request opened` → 202 + row in `pr_ticket_map`.
  - Same `delivery_id` redelivered → 200 `{"status":"duplicate"}`, no DB mutation.
  - Wrong HMAC → 401, no DB mutation.
  - Signed `check_suite completed` with a mapped PR → 202 + per-PR `CiOutcome` (green / red / cap_hit / pending / no_mapped_pr) returned in the JSON body. PAT-mode `octocrab` calls reach the live GitHub API; App-mode auth is P1-G.
- `scripts/verify-workspace-move.sh` → no longer applicable post-merge (the script's purpose was to gate the workspace-move commit; it can stay in-repo as a one-shot artifact)

---

## 2. What's next: P1-G — GitHub auth (PAT + App) + `--self-test`

The next concrete deliverable. Source of truth: `docs/v0.3-plan/01-bridge-mvp.md` §8 (GitHub authentication + self-test). This is the deliverable that finishes the bridge's GitHub surface and gives the Phase 5 `setup-bridge` skill a deterministic install gate.

### Scope

P1-F landed `GhOps` as a trait with one implementation (`OctocrabGhOps::from_pat`) and a hand-rolled `match (&cfg.github.pat, &cfg.github.app_id)` branch in `main.rs::run` that explicitly rejects App-mode auth ("BRIDGE.md github.app_id auth is deferred to P1-G"). P1-G:

- Adds **App-mode auth** — JWT-authenticated `Octocrab` + per-installation client cache, so a single bridge process can serve multiple GitHub installations.
- Adds a **mode-selector** factory that consolidates the PAT branch and the App branch behind one entry point.
- Adds **`sinfonia-bridge --self-test`** — a serial runner that prints `PASS` / `FAIL` / `SKIP` per check and exits with the failure count, so install scripts can gate on it.
- Adds an optional **`server.public_url`** config field so the self-test can probe webhook reachability; absent → that check `SKIP`s.

Two modules carry this:

- `crates/sinfonia-bridge/src/github/auth.rs` (new). `BridgeAuthMode::{Pat, App}` selector; `load_private_key(s)` accepting inline PEM or `@/path/to/key.pem` (with `~` expansion via `shellexpand`); `build_gh_ops(&GitHubSection) -> Result<Arc<dyn GhOps>>` factory replacing the inline branch in `main.rs::run`. `AppModeGhOps` wraps a JWT-authenticated `Octocrab` plus `tokio::sync::RwLock<HashMap<String /* owner */, Arc<Octocrab>>>`. On the first GhOps method against a `repo`, the owner segment is resolved to an installation via `apps().get_repository_installation(owner, repo)`, the installation-scoped client is built via `crab.installation(id)` and cached under the owner key; subsequent calls hit the cache. (One installation per (App, owner) is the GitHub data model.) The trait methods on `AppModeGhOps` delegate every call to the per-owner installation client.
- `crates/sinfonia-bridge/src/selftest.rs` (new). `run_selftest(&BridgeConfig) -> i32` runs the checks serially, prints one labelled line each, returns the failure count for `std::process::exit`. Checks: (1) `config: BRIDGE.md parsed` (PASS by definition — we got here), (2) `github: authenticated as <login or app slug> (<mode>)` via `/user` (PAT) or `/app` (App), (3) `github: webhook endpoint reachable at <public_url>/health` — SKIP when `server.public_url` is unset, otherwise `reqwest::get` → expect 200, (4) `tracker: <kind> project '<slug>' accessible` — for Linear, an `IssueTracker::fetch_candidate_issues` round-trip is the cheapest "can we reach the API?" probe; for Jira (Phase 4) this is currently a `NotImplemented` no-op, (5) `custom fields: <MARKER> reserved` — verifies the `sinfonia_tracker::custom_fields::MARKER` constant; documentation-grade PASS.

The CLI gains a single new flag — `--self-test` — wired into the same `clap::Parser` struct already in `main.rs`. When set, `run()` builds `cfg` + `tracker` + `Arc<dyn GhOps>` exactly as the serve path does, then dispatches to `selftest::run_selftest` and exits without binding the listener.

### Config addition

`server.public_url: Option<String>` lands in `BridgeConfig::ServerSection` with one validation rule: when set, must parse as a `url::Url`. Backward-compatible — every existing test config + the example BRIDGE.md keeps working untouched.

### Unit tests

| Module | Cases |
|---|---|
| `github::auth` | `load_private_key`: inline-PEM passthrough; `@/abs/path` reads file; `@~/key.pem` expands `~`; missing-file errors with a path in the message. `BridgeAuthMode::from_github_section`: PAT-only → `Pat`; App-only (app_id + private_key) → `App`; both set / neither set → error (BridgeConfig validation already catches these; this is belt-and-braces against future drift). |
| `selftest` | Exit code = number of `FAIL` lines (not `SKIP`s); reachability check `SKIP`s cleanly when `server.public_url` is `None`; the formatted output starts each line with `PASS` / `FAIL` / `SKIP` and includes the check name. |
| `config` | `server.public_url` round-trips; absent → `None`; non-URL string → `Error::BridgeConfigInvalid`. |

App-mode integration (octocrab JWT → installation token → real REST call) is covered by the P1-H wiremock harness — scenario 8 specifically uses App credentials.

### What's already prepared

- `BridgeConfig.github.{pat,app_id,private_key,webhook_secret}` are all parsed in P1-D with env-var indirection (`$ENVVAR` substitution).
- Validation rule 1 in `config::validate` already enforces "exactly one of `pat` or `app_id`" and rule "App mode requires `private_key`."
- `GhOps` trait is repo-scoped — every method already receives `repo: &str` — so `AppModeGhOps` can extract the owner and route without a new trait method.
- `octocrab = "0.39"` is in `Cargo.toml`. `Octocrab::installation(InstallationId) -> Octocrab` is synchronous (no API call) — the API call happens once during installation discovery, then the scoped client is reusable.
- `shellexpand` is already in workspace deps; `~`-expansion for `@path` PEM loading is a one-line call.
- `reqwest` is in workspace deps for the public-URL probe.
- The `--check` flag's pattern in `main.rs` (build config, run a one-shot check, exit) is the template for `--self-test`'s wiring.

### Exit criteria for P1-G

- `cargo check --workspace` compiles clean.
- `cargo test --workspace` zero regressions over the post-P1-F baseline (118 tests).
- All new P1-G unit tests pass (count: ~5–7 across `github::auth`, `selftest`, and `config::server.public_url`).
- `sinfonia-bridge --self-test` against a valid PAT-mode `BRIDGE.md` prints PASS for each check and exits 0.
- `sinfonia-bridge --self-test` against a deliberately-broken `BRIDGE.md` (wrong PAT, unreachable public_url, wrong tracker project) prints one `FAIL` per broken check and exits with the failure count.
- App-mode `main.rs::run` no longer errors at startup — it instead constructs an `AppModeGhOps` with a working JWT and stuffs it into `AppState`. (Full live-traffic verification waits on P1-H; for P1-G, unit-test coverage of `auth.rs` plus successful workspace compilation is the gate.)

### Files (P1-G owns the three marked with †)

```
crates/sinfonia-bridge/
├── Cargo.toml
└── src/
    ├── main.rs                  ← P1-D / P1-E / P1-F. P1-G: + `--self-test` flag, swap inline auth branch for `auth::build_gh_ops`.
    ├── lib.rs                   ← P1-D / P1-F. P1-G: register `selftest`.
    ├── config.rs                ← P1-D. P1-G: add `server.public_url: Option<String>` + URL-syntax validation.
    ├── webhook/                 ← P1-D / P1-E / P1-F.
    ├── feedback/                ← P1-F.
    ├── labels.rs                ← P1-F.
    ├── github/
    │   ├── mod.rs               ← P1-F. P1-G: re-export `auth::build_gh_ops` + `BridgeAuthMode`.
    │   ├── client.rs            ← P1-F (`GhOps` trait + PAT-mode `OctocrabGhOps`).
    │   └── auth.rs              ← P1-G †  mode selector + `load_private_key` + `AppModeGhOps`.
    ├── storage.rs               ← P1-E.
    └── selftest.rs              ← P1-G †  serial runner with PASS/FAIL/SKIP per check.
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
│   └── sinfonia-bridge/         # the bridge daemon (config + webhook layer + feedback loop landed; GitHub App auth & --self-test are P1-G)
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

# 2. Confirm test baseline (should be 118 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file) and the Phase 1 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/01-bridge-mvp.md

# 4. Confirm the working tree shape matches §3 above.
ls crates/
ls crates/sinfonia-bridge/src/        # expect: config.rs, lib.rs, main.rs, storage.rs, labels.rs, webhook/, feedback/, github/
ls crates/sinfonia-bridge/src/webhook/  # expect: handlers.rs, mod.rs, verify.rs
ls crates/sinfonia-bridge/src/feedback/ # expect: attempts.rs, categorize.rs, mod.rs, transition.rs
ls crates/sinfonia-bridge/src/github/   # expect: client.rs, mod.rs   (P1-G adds auth.rs + selftest.rs at crate root)
ls crates/sinfonia-tracker/src/

# 5. Start a P1-G branch off main.
git checkout -b v0.3-phase-1-github-auth

# 6. Set up Phase 1 sub-task tracking. The original task IDs from the
#    completed-context conversation are not preserved across context
#    clears — TaskCreate a fresh set for P1-G..P1-I per the table in §1.
```

Previous sessions completed P1-A / P1-B / P1-C / P1-D / P1-E / P1-F. Three sub-tasks remain (P1-G … P1-I); recreate those as fresh TaskCreate entries.

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
Last merged work: P1-F feedback loop + categorization + labels (PR #5, commit 9d33d51, merge 1c2c14f)
Earlier merges: P1-E webhook layer (PR #4, commit 69eb8e0);
                P1-D bridge skeleton (PR #3, commit 07c0381);
                Phase 1 foundation (PR #2: commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/01-bridge-mvp.md   (Phase 1 plan; next deliverable is P1-G in §8)

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters, custom_fields)
  crates/sinfonia-bridge/  — bridge binary; config + storage + verify + webhook dispatch landed,
                             feedback loop (P1-F) and github client (P1-F/P1-G) still to come

Test baseline: 90 passing, 0 failures. Maintain that.
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
- The full `cargo test --workspace` count should be roughly 51 sinfonia/tracker + ~75 bridge unit tests (67 from P1-D+P1-E+P1-F plus P1-G's ~5–7) + 9 integration tests ≈ ~130 tests.

Phase 1 is then shippable on its own; Phases 2–7 land on top of it.

---

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Ready to resume from a fresh context with this doc as the entry point.
