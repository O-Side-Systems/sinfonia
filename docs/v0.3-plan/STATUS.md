# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix) and the bridge skeleton (#3 — P1-D: `sinfonia-bridge` crate, BRIDGE.md parser, axum router with `/health` + stub `/webhook`).

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

Phase 1 *foundation* (P1-A / P1-B / P1-C) **and** the bridge crate skeleton (P1-D) are merged to `main`. The remaining five Phase 1 deliverables (P1-E … P1-I) — runtime webhook handlers, the feedback loop, GitHub auth, integration tests, and the Phase 1 docs — have not been started. Pick up at **P1-E: webhook handlers + HMAC verify + SQLite idempotency**. The phase plan in `01-bridge-mvp.md` (§5 event flow, §9 test plan) is the source of truth; this STATUS doc captures the *implementation* deltas you need to know about that aren't in the plan.

The single most important non-obvious decision made during the foundation work: **`CustomFieldValue` was collapsed from four variants (`Null` / `Number` / `Decimal` / `LongText` / `Url`) to three (`Null` / `Number` / `String`)** because serde's `#[serde(untagged)]` deserializer can't distinguish multiple JSON-string variants. See §5 below.

---

## 1. What's merged to `main`

### Commits

| Commit / PR | Title | Scope |
|---|---|---|
| `82d2d2f` (#2) | Add v0.3 implementation plan docs (eight phase plans + index) | Docs only — `docs/v0.3-plan/00..07.md` |
| `3f045e9` (#2) | Phase 1 foundation: workspace conversion + tracker extensions + H-1 fix | Code — P1-A / P1-B / P1-C |
| `d6cd7ca` | Add v0.3 milestone STATUS doc | Docs — this file |
| `07c0381` (#3) | P1-D: sinfonia-bridge crate skeleton + BRIDGE.md config parser | Code — `crates/sinfonia-bridge/`, 16 unit tests |

### Phase 1 sub-task status

| Sub-task | Plan section | Status | Notes |
|---|---|---|---|
| **P1-A** workspace conversion + verify script | `01-bridge-mvp.md` §2 | ✅ merged | `crates/sinfonia/`, `crates/sinfonia-tracker/`, `scripts/verify-workspace-move.sh` |
| **P1-B** tracker trait extensions + `custom_fields` module | §4, §11 | ✅ merged | 5 new bridge-write methods on `IssueTracker`; Linear impls; Jira returns `NotImplemented` (Phase 4 fills) |
| **P1-C** `Issue.fields` + Linear marker-comment + template scope (H-1) | §4.2 | ✅ merged | `Issue.fields` populated by Linear fetch; `template.rs` pre-seeds well-known keys |
| **P1-D** bridge binary skeleton + BRIDGE.md config parser | §2, §3 | ✅ merged | `crates/sinfonia-bridge` crate scaffolded; BRIDGE.md parser + 9 validation rules + 16 unit tests; axum router with `/health` + stub `POST /webhook`; `--check` flag |
| **P1-E** webhook handlers + HMAC verify + SQLite idempotency | §5, §9 | ⬜ not started | Next deliverable |
| **P1-F** feedback loop + categorization + labels | §5, §6, §7 | ⬜ not started | |
| **P1-G** GitHub auth (PAT + App) + `--self-test` | §8 | ⬜ not started | |
| **P1-H** integration tests with `wiremock` | §9.2 | ⬜ not started | Nine scenarios specified |
| **P1-I** Phase 1 docs (BRIDGE.example.md, SPEC §11.6 draft, CHANGELOG, README stub) | §12 | ⬜ not started | |

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` → **51 tests pass, 0 failures**
  - 32 sinfonia unit tests (mostly under `crates/sinfonia/src/agent/cli/tests`, `config/`, `orchestrator/`, `template/`, `workspace/`)
  - 12 `spec_conformance.rs` integration tests
  - 7 sinfonia-tracker tests (1 base64 + 6 custom_fields)
- `scripts/verify-workspace-move.sh` → no longer applicable post-merge (the script's purpose was to gate the workspace-move commit; it can stay in-repo as a one-shot artifact)

### Test baseline on `v0.3-phase-1-bridge-skeleton`

- `cargo test --workspace --no-fail-fast` → **67 tests pass, 0 failures** (51 + 16 new bridge config tests)
- `cargo run -p sinfonia-bridge -- BRIDGE.md --check` → `ok` (exit 0) on valid, descriptive error (exit 1) on invalid
- `GET /health` returns `{"service":"sinfonia-bridge","status":"ok","tenant_id":<...>,"tracker":"linear"}`
- `POST /webhook` stub returns 200 (real handler logic lands in P1-E)

---

## 2. What's next: P1-E — webhook handlers + HMAC verify + SQLite idempotency

The next concrete deliverable. Source of truth: `docs/v0.3-plan/01-bridge-mvp.md` §5 (event flow) and §9.1 (storage / verify unit tests).

### Scope

Replace the P1-D `POST /webhook` stub with the real implementation. Three modules carry this:

- `crates/sinfonia-bridge/src/webhook/verify.rs` (new). HMAC-SHA256 over the raw request body using `github.webhook_secret`. Constant-time compare via `subtle::ConstantTimeEq`. Reject missing / malformed `X-Hub-Signature-256` headers with 401. Returns the verified body bytes so the handler can JSON-decode without re-reading.
- `crates/sinfonia-bridge/src/webhook/handlers.rs` (extend). Dispatch on the `X-GitHub-Event` header:
  - `pull_request` → parse PR body+title, run `feedback_loop.pr_link_pattern` to extract a tracker identifier, upsert the `pr_ticket_map` row.
  - `check_suite` (action=completed) → trigger CI evaluation.
  - `workflow_run` (action=completed) → same as check_suite.
  - Anything else → 200 + log at debug.
  - In P1-E the handlers stop short of doing the actual ticket-state transition; that's P1-F. P1-E just persists the mapping and returns 202 with a "queued" body so the bridge has visibility into what's been received.
- `crates/sinfonia-bridge/src/storage.rs` (new). `rusqlite::Connection` wrapped in `tokio::sync::Mutex`. Two tables:
  - `processed_deliveries(delivery_id TEXT PRIMARY KEY, processed_at INTEGER)` — idempotency. Insert returns `OK(())` if new, `Err(Storage("duplicate"))` on conflict; the handler treats duplicate as a 200 no-op.
  - `pr_ticket_map(repo TEXT, pr_number INTEGER, ticket_id TEXT, discovered_at INTEGER, PRIMARY KEY(repo, pr_number))` — PR↔ticket mapping. Upsert on `pull_request opened/synchronize`.
- `crates/sinfonia-bridge/src/webhook/mod.rs` (extend `AppState`). Add `store: Arc<storage::Store>` and `tracker: Arc<dyn IssueTracker>` so handlers have what they need without re-reading config.

Bridge's `main.rs::run()` must open the SQLite DB at `config.storage.state_db_path` before binding the listener — schema migration is a single `CREATE TABLE IF NOT EXISTS` per table.

### Unit tests (per plan §9.1)

| Module | Cases |
|---|---|
| `webhook::verify` | HMAC verify happy path; wrong-secret rejection; missing header rejection; tampered-body rejection |
| `storage` | Insert idempotency key (new + duplicate); upsert pr_ticket_map (insert + update); restart-replay reads same row |

### What's already prepared

- The bridge `AppState` already holds `Arc<BridgeConfig>` — extend it, don't replace.
- `octocrab`, `rusqlite`, `subtle`, `hmac`, `sha2` are already in `crates/sinfonia-bridge/Cargo.toml` from P1-D.
- `BridgeConfig.github.webhook_secret`, `BridgeConfig.storage.state_db_path`, and `BridgeConfig.feedback_loop.pr_link_pattern` are all parsed + validated in P1-D.
- The `IssueTracker` trait + `LinearTracker` implementations from P1-B already expose every read/write the bridge needs.
- `crates/sinfonia-tracker/src/custom_fields.rs` has `encode_marker()` / `decode_marker()` for Linear's bot-owned comment convention.

### Exit criteria for P1-E

- `cargo check --workspace` compiles clean.
- All P1-E unit tests pass (count: 4 + 3 = 7 new tests).
- `cargo test --workspace` zero regressions over the post-P1-D baseline (67 tests).
- `POST /webhook` with a correctly-signed `pull_request opened` payload upserts the `pr_ticket_map` row and returns 202.
- `POST /webhook` with the same `delivery_id` twice is idempotent (second response is 200 with body indicating the duplicate, no DB mutation).
- `POST /webhook` with a wrong HMAC returns 401 with no DB mutation.
- Manual: send a recorded `check_suite completed` payload, observe a row in `processed_deliveries`, observe the handler log it without dispatching a transition (that's P1-F).

### Files (still pending; P1-E owns the three marked with †)

```
crates/sinfonia-bridge/
├── Cargo.toml
└── src/
    ├── main.rs                  ← P1-D, extended in P1-E for storage init
    ├── lib.rs                   ← P1-D
    ├── config.rs                ← P1-D
    ├── webhook/
    │   ├── mod.rs               ← P1-D, AppState extended in P1-E
    │   ├── verify.rs            ← P1-E †  HMAC-SHA256 + subtle::ConstantTimeEq
    │   └── handlers.rs          ← P1-D stub, full impl in P1-E †
    ├── feedback/                ← P1-F
    │   ├── mod.rs
    │   ├── categorize.rs
    │   ├── transition.rs
    │   └── attempts.rs
    ├── labels.rs                ← P1-F
    ├── github/                  ← P1-G
    │   ├── mod.rs
    │   ├── auth.rs
    │   └── client.rs
    ├── storage.rs               ← P1-E †  rusqlite, idempotency + pr_ticket_map
    └── selftest.rs              ← P1-G
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
│   └── sinfonia-bridge/         # <-- P1-D creates this
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

---

## 6. Resume protocol — first commands a fresh agent should run

```bash
# 1. Land on a clean main with all merged work.
git checkout main
git pull --ff-only origin main

# 2. Confirm test baseline (should be 67 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file) and the Phase 1 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/01-bridge-mvp.md

# 4. Confirm the working tree shape matches §3 above.
ls crates/
ls crates/sinfonia-bridge/src/
ls crates/sinfonia-tracker/src/

# 5. Start a P1-E branch off main.
git checkout -b v0.3-phase-1-webhook-storage

# 6. Set up Phase 1 sub-task tracking. The original task IDs from the
#    completed-context conversation are not preserved across context
#    clears — TaskCreate a fresh set for P1-E..P1-I per the table in §1.
```

Previous sessions completed P1-A / P1-B / P1-C / P1-D. Five sub-tasks remain (P1-E … P1-I); recreate those as fresh TaskCreate entries.

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
Last merged work: P1-D bridge skeleton (PR #3, commit 07c0381)
Earlier merges: Phase 1 foundation (PR #2: commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/01-bridge-mvp.md   (Phase 1 plan; next deliverable is P1-E in §5 / §9)

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters, custom_fields)
  crates/sinfonia-bridge/  — bridge binary skeleton (config parser + /health + stub /webhook)

Test baseline: 67 passing, 0 failures. Maintain that.
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
- The full `cargo test --workspace` count should be roughly 51 + ~30 bridge unit tests + 9 integration tests = ~90 tests.

Phase 1 is then shippable on its own; Phases 2–7 land on top of it.

---

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Ready to resume from a fresh context with this doc as the entry point.
