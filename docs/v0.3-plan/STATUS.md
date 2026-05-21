# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the Phase 1 foundation (#2: workspace conversion + tracker extensions + H-1 fix) plus the STATUS doc itself. The bridge-binary skeleton (P1-D) is on a branch `v0.3-phase-1-bridge-skeleton` awaiting PR.

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

Phase 1 *foundation* (P1-A / P1-B / P1-C) is merged to `main`. The remaining six Phase 1 deliverables (P1-D … P1-I) — the actual `sinfonia-bridge` binary, its tests, and the Phase 1 docs — have not been started. Pick up at **P1-D: bridge binary skeleton + BRIDGE.md config parser**. The phase plan in `01-bridge-mvp.md` is the source of truth; this STATUS doc captures the *implementation* deltas you need to know about that aren't in the plan.

The single most important non-obvious decision made during the foundation work: **`CustomFieldValue` was collapsed from four variants (`Null` / `Number` / `Decimal` / `LongText` / `Url`) to three (`Null` / `Number` / `String`)** because serde's `#[serde(untagged)]` deserializer can't distinguish multiple JSON-string variants. See §5 below.

---

## 1. What's merged to `main`

### Commits

| Commit | Title | Scope |
|---|---|---|
| `82d2d2f` | Add v0.3 implementation plan docs (eight phase plans + index) | Docs only — `docs/v0.3-plan/00..07.md` |
| `3f045e9` | Phase 1 foundation: workspace conversion + tracker extensions + H-1 fix | Code — P1-A / P1-B / P1-C |

### Phase 1 sub-task status

| Sub-task | Plan section | Status | Notes |
|---|---|---|---|
| **P1-A** workspace conversion + verify script | `01-bridge-mvp.md` §2 | ✅ merged | `crates/sinfonia/`, `crates/sinfonia-tracker/`, `scripts/verify-workspace-move.sh` |
| **P1-B** tracker trait extensions + `custom_fields` module | §4, §11 | ✅ merged | 5 new bridge-write methods on `IssueTracker`; Linear impls; Jira returns `NotImplemented` (Phase 4 fills) |
| **P1-C** `Issue.fields` + Linear marker-comment + template scope (H-1) | §4.2 | ✅ merged | `Issue.fields` populated by Linear fetch; `template.rs` pre-seeds well-known keys |
| **P1-D** bridge binary skeleton + BRIDGE.md config parser | §2, §3 | 🟡 PR open | `crates/sinfonia-bridge` crate scaffolded; BRIDGE.md parser + 9 validation rules + 16 unit tests; axum router with `/health` + stub `POST /webhook`; `--check` flag |
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

## 2. What's next: P1-D — bridge binary skeleton

The next concrete deliverable. Source of truth: `docs/v0.3-plan/01-bridge-mvp.md` §2 (target layout) and §3 (BRIDGE.md schema).

### Files to create (per the plan §2)

```
crates/sinfonia-bridge/
├── Cargo.toml
└── src/
    ├── main.rs                  # clap parsing, init_observability, run() entry
    ├── lib.rs                   # pub mod tree + Result/Error
    ├── config.rs                # BRIDGE.md front-matter parser + validation
    ├── webhook/
    │   ├── mod.rs
    │   ├── verify.rs            # HMAC-SHA256 + subtle::ConstantTimeEq
    │   └── handlers.rs          # axum routes (P1-E)
    ├── feedback/
    │   ├── mod.rs
    │   ├── categorize.rs        # check-name → category (P1-F)
    │   ├── transition.rs        # state transition logic (P1-F)
    │   └── attempts.rs          # counter read/increment (P1-F)
    ├── labels.rs                # PR label management (P1-F)
    ├── github/
    │   ├── mod.rs
    │   ├── auth.rs              # PAT vs App selector (P1-G)
    │   └── client.rs            # octocrab wrapper (P1-G)
    ├── storage.rs               # rusqlite idempotency (P1-E)
    └── selftest.rs              # --self-test command (P1-G)
```

P1-D specifically only owns: `Cargo.toml`, `main.rs`, `lib.rs`, `config.rs`, an axum router with `/health`, and a stub for `webhook/handlers.rs` that returns 200 OK. The actual webhook logic is P1-E. The feedback/labels/auth/selftest pieces are later sub-tasks.

### What's already prepared

- Workspace `Cargo.toml` has `[workspace.dependencies]` with everything the bridge needs except its bridge-specific deps (`octocrab`, `rusqlite`, `subtle`, `hmac`, `sha2`, `jsonwebtoken`). Add those to **the bridge crate's** `Cargo.toml`, not the workspace, unless they end up shared.
- `sinfonia-tracker` exports `IssueTracker`, `LinearTracker`, `JiraTracker`, `TrackerConfig`, `CustomFieldValue`, `CustomFieldSchema`, `FieldsMap`, `WELL_KNOWN_FIELDS`, and its own `Error` type. The bridge depends on this crate.
- `crates/sinfonia-tracker/src/custom_fields.rs` has `encode_marker()` and `decode_marker()` plus the `MARKER` sentinel string; the bridge uses these to read/write the bot-owned Linear comment.

### Add to `crates/sinfonia-bridge/Cargo.toml`

```toml
[package]
name = "sinfonia-bridge"
description = "CI feedback bridge for Sinfonia — translates GitHub webhook events into tracker state transitions."
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
authors.workspace      = true
homepage.workspace     = true
repository.workspace   = true

[[bin]]
name = "sinfonia-bridge"
path = "src/main.rs"

[lib]
name = "sinfonia_bridge"
path = "src/lib.rs"

[dependencies]
sinfonia-tracker = { workspace = true }

tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
reqwest = { workspace = true }
clap = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
axum = { workspace = true }
tower = { workspace = true }
liquid = { workspace = true }      # for failure_comment_template
anyhow = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
async-trait = { workspace = true }
regex = { workspace = true }
url = { workspace = true }
shellexpand = { workspace = true }

# Bridge-specific (not workspace deps unless re-used)
octocrab = "0.39"                   # verify version at impl time
rusqlite = { version = "0.31", features = ["bundled"] }
subtle = "2.5"
hmac = "0.12"
sha2 = "0.10"

[dev-dependencies]
tempfile = { workspace = true }
wiremock = "0.6"
tokio = { version = "1.41", features = ["test-util", "macros", "rt-multi-thread"] }
```

Then add `"crates/sinfonia-bridge"` to the root `Cargo.toml` `[workspace] members`.

### Exit criteria for P1-D

- `cargo check -p sinfonia-bridge` compiles clean.
- `sinfonia-bridge BRIDGE.example.md --check` parses a sample config and exits 0.
- All schema validation rules in `01-bridge-mvp.md` §3 have a unit test (one test per rule).
- The skeleton axum server starts on the configured `server.port` and serves `GET /health` returning `{"status":"ok"}`.
- `cargo test --workspace` shows the existing 51 tests + however many new bridge unit tests; zero regressions.

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
# 1. Land on a clean main with all foundation work merged.
git checkout main
git pull --ff-only origin main

# 2. Confirm test baseline (should be 51 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file) and the Phase 1 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/01-bridge-mvp.md

# 4. Confirm the working tree shape matches §3 above.
ls crates/
ls crates/sinfonia-tracker/src/
ls crates/sinfonia/src/

# 5. Start a P1-D branch off main.
git checkout -b v0.3-phase-1-bridge-skeleton

# 6. Set up Phase 1 sub-task tracking. The original task IDs from the
#    completed-context conversation are not preserved across context
#    clears — TaskCreate a fresh set for P1-D..P1-I per the table in §1.
```

The previous agent had nine TaskCreate entries (P1-A through P1-I); they completed three (P1-A/B/C). On resume, recreate the remaining six tasks fresh.

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
Last merged work: Phase 1 foundation (commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/01-bridge-mvp.md   (Phase 1 plan; the next deliverable is P1-D in §2)

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon (existing code)
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters, custom_fields)
  crates/sinfonia-bridge/  — does not exist yet; P1-D creates it

Test baseline: 51 passing, 0 failures. Maintain that.
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
