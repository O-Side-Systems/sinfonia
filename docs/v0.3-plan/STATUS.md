# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix), the bridge skeleton (#3 — P1-D: `sinfonia-bridge` crate, BRIDGE.md parser, axum router with `/health` + stub `/webhook`), and the real webhook layer (#4 — P1-E: HMAC-SHA256 verification, SQLite idempotency, `pull_request` / `check_suite` / `workflow_run` dispatch).

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

Phase 1 *foundation* (P1-A / P1-B / P1-C), the bridge crate skeleton (P1-D), **and** the real webhook layer (P1-E — HMAC verify + SQLite idempotency + `pull_request` / `check_suite` / `workflow_run` dispatch) are merged to `main`. The remaining four Phase 1 deliverables (P1-F … P1-I) — feedback loop & categorization & labels, GitHub auth & `--self-test`, integration tests, and the Phase 1 docs — have not been started. Pick up at **P1-F: feedback loop + categorization + labels**. The phase plan in `01-bridge-mvp.md` (§5.2 CI evaluation, §6 categorization, §7 PR labels) is the source of truth; this STATUS doc captures the *implementation* deltas you need to know about that aren't in the plan.

The single most important non-obvious decision made during the foundation work: **`CustomFieldValue` was collapsed from four variants (`Null` / `Number` / `Decimal` / `LongText` / `Url`) to three (`Null` / `Number` / `String`)** because serde's `#[serde(untagged)]` deserializer can't distinguish multiple JSON-string variants. See §5 below.

The single most important non-obvious decision made during P1-E: **`check_suite` / `workflow_run` handlers stop at 202 \"queued\" and persist the `delivery_id`** — they do NOT yet run the CI evaluation. That's by design (see §2 below) so P1-F can land the loop in isolation. The `pull_request` handler is fully wired (extracts the tracker ID and upserts `pr_ticket_map`); only the CI-result-→-transition path is deferred.

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

### Phase 1 sub-task status

| Sub-task | Plan section | Status | Notes |
|---|---|---|---|
| **P1-A** workspace conversion + verify script | `01-bridge-mvp.md` §2 | ✅ merged | `crates/sinfonia/`, `crates/sinfonia-tracker/`, `scripts/verify-workspace-move.sh` |
| **P1-B** tracker trait extensions + `custom_fields` module | §4, §11 | ✅ merged | 5 new bridge-write methods on `IssueTracker`; Linear impls; Jira returns `NotImplemented` (Phase 4 fills) |
| **P1-C** `Issue.fields` + Linear marker-comment + template scope (H-1) | §4.2 | ✅ merged | `Issue.fields` populated by Linear fetch; `template.rs` pre-seeds well-known keys |
| **P1-D** bridge binary skeleton + BRIDGE.md config parser | §2, §3 | ✅ merged | `crates/sinfonia-bridge` crate scaffolded; BRIDGE.md parser + 9 validation rules + 16 unit tests; axum router with `/health` + stub `POST /webhook`; `--check` flag |
| **P1-E** webhook handlers + HMAC verify + SQLite idempotency | §5, §9 | ✅ merged | `storage.rs` (rusqlite + Mutex; `processed_deliveries` + `pr_ticket_map`); `webhook/verify.rs` (HMAC-SHA256, constant-time compare); full `POST /webhook` dispatch for `pull_request` / `check_suite` / `workflow_run`; `AppState` carries `Arc<Store>` + `Arc<dyn IssueTracker>`; +23 unit tests |
| **P1-F** feedback loop + categorization + labels | §5.2, §6, §7 | ⬜ not started | Next deliverable |
| **P1-G** GitHub auth (PAT + App) + `--self-test` | §8 | ⬜ not started | |
| **P1-H** integration tests with `wiremock` | §9.2 | ⬜ not started | Nine scenarios specified |
| **P1-I** Phase 1 docs (BRIDGE.example.md, SPEC §11.6 draft, CHANGELOG, README stub) | §12 | ⬜ not started | |

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` → **90 tests pass, 0 failures**
  - 32 sinfonia unit tests (mostly under `crates/sinfonia/src/agent/cli/tests`, `config/`, `orchestrator/`, `template/`, `workspace/`)
  - 12 `spec_conformance.rs` integration tests
  - 7 sinfonia-tracker tests (1 base64 + 6 custom_fields)
  - 39 sinfonia-bridge unit tests (16 config from P1-D + 23 from P1-E: 6 verify, 6 storage, 11 handlers)
- `cargo run -p sinfonia-bridge -- BRIDGE.md --check` → `ok` (exit 0) on valid, descriptive error (exit 1) on invalid
- `GET /health` returns `{"service":"sinfonia-bridge","status":"ok","tenant_id":<...>,"tracker":"linear"}`
- `POST /webhook` flow (verified end-to-end with manual curl + sqlite3 row dump):
  - Signed `pull_request opened` → 202 + row in `pr_ticket_map`.
  - Same `delivery_id` redelivered → 200 `{"status":"duplicate"}`, no DB mutation.
  - Wrong HMAC → 401, no DB mutation.
  - Signed `check_suite completed` → 202 + row in `processed_deliveries` (no transition; that's P1-F).
- `scripts/verify-workspace-move.sh` → no longer applicable post-merge (the script's purpose was to gate the workspace-move commit; it can stay in-repo as a one-shot artifact)

---

## 2. What's next: P1-F — feedback loop + categorization + labels

The next concrete deliverable. Source of truth: `docs/v0.3-plan/01-bridge-mvp.md` §5.2 (CI evaluation), §6 (categorization), §7 (PR labels). This is the deliverable that turns the bridge from "accepts webhooks" into "closes the loop."

### Scope

P1-E left `check_suite` / `workflow_run` handlers acknowledging at 202 "queued" without doing the actual work. P1-F replaces that with the real CI-result-→-tracker-state path, plus the label management that goes with it, plus the failure categorization that routes to the right `Needs Fixes - X` state.

Five modules carry this:

- `crates/sinfonia-bridge/src/feedback/mod.rs` (new). Front door for the loop. Exposes `evaluate_ci(state, payload)` called from `webhook/handlers::handle_check_suite` and `handle_workflow_run`. Looks up the PR from the event payload, resolves the ticket via the P1-E `pr_ticket_map`, fetches check runs through the github client (P1-G), aggregates outcome (green / red / wait), and dispatches to the appropriate transition path.
- `crates/sinfonia-bridge/src/feedback/categorize.rs` (new). Pure function `categorize(failed_checks, categories) -> &FailureCategory`. Edge cases per plan §6: empty `failed_checks` (debug-assert, return default in release); multi-match across categories (highest priority wins); equal priorities (unreachable — config validation rejects them); no `failure_categories:` configured (synthetic default routes to `needs_fixes_state`). The synthetic-default-on-empty path is already in place from `BridgeConfig` parsing — categorize just consumes the list.
- `crates/sinfonia-bridge/src/feedback/attempts.rs` (new). Read / increment / cap-check for the `sinfonia_attempt_count` custom field on the ticket. Reads via `IssueTracker::read_custom_field`; writes the incremented value via `write_custom_field`. Honors `sinfonia_max_attempts` per-ticket override before falling back to `feedback_loop.max_attempts`. Cap detection returns an enum (`AttemptDecision::Continue { next } | ::CapHit`) so the transition layer can pick the right state and label.
- `crates/sinfonia-bridge/src/feedback/transition.rs` (new). State-transition logic per plan §5.2. On green: apply `awaiting-review` label, remove `in-progress` / `needs-fixes`. On red below cap: write `sinfonia_last_ci_failure` + `sinfonia_failure_category`, increment counter, transition to category target state, apply `needs-fixes` + `failure:<category>` labels, post the rendered `failure_comment_template` to the PR. On red at cap: transition to `blocked_state`, apply `cap-hit`, post the cap-explanation comment.
- `crates/sinfonia-bridge/src/labels.rs` (new). `BridgeLabel` enum (`InProgress` / `AwaitingReview` / `NeedsFixes` / `CapHit` / `BudgetExceeded` (Phase 3) / `Failure(String)`). `BridgeLabel::full_name(prefix, aliases)` resolves the verbatim-semantics alias from `BridgeConfig.github.label_aliases` (already parsed in P1-D). `ensure_labels(client, repo, prefix, aliases)` is idempotent label creation per repo, called lazily on first event for a (repo). `apply(client, repo, pr, label)` / `remove(client, repo, pr, label)` short-circuit when `manage_labels: false`.

Note on the github client surface: P1-F needs to fetch check runs, post comments, and create/apply/remove labels. The cleanest split with P1-G is for P1-F to introduce `crates/sinfonia-bridge/src/github/client.rs` with **PAT-mode-only** auth (one `Octocrab::builder().personal_token(...)` instance built in `main.rs::run` and stuffed into `AppState`), and let P1-G add App-mode and `--self-test`. The `auth.rs` mode-selector and the per-installation `RwLock<HashMap<…>>` are P1-G's job. `BridgeConfig.github.pat` and `BridgeConfig.github.webhook_secret` are already parsed in P1-D.

### Unit tests (per plan §9.1)

| Module | Cases |
|---|---|
| `feedback::categorize` | Priority ordering (high beats low); multi-match across categories; empty `failed_checks` returns default; `failure_categories:` absent in config still returns default. |
| `feedback::attempts` | Increment from 0 → 1; increment respects per-ticket `sinfonia_max_attempts` override; cap-hit detection on the final attempt; cap-hit does NOT increment past the cap. |
| `labels` | Label name composition (prefix + base); alias supplies full name verbatim (prefix ignored); `manage_labels: false` short-circuits both `apply` and `ensure_labels`. |

Mock the `IssueTracker` and the github client for these — they're pure dispatch tests, no live HTTP. The full wiremock-backed integration coverage is P1-H's responsibility (nine scenarios in plan §9.2).

### What's already prepared

- The P1-E `webhook/handlers::handle_check_suite` / `handle_workflow_run` stubs are the call sites that P1-F replaces. They already log and 202 — P1-F swaps in `feedback::evaluate_ci(&state, &payload, &delivery_id).await` and returns based on the result.
- `AppState` already carries `Arc<Store>` and `Arc<dyn IssueTracker>` from P1-E. P1-F adds the github client to it.
- `Store::lookup_pr_ticket(repo, pr_number)` exists — that's the PR-→-ticket step on the read side.
- `BridgeConfig.feedback_loop` is fully parsed: `max_attempts`, `needs_fixes_state`, `blocked_state`, `failure_categories` (with the synthetic-default already injected when the user omits them), `failure_comment_template`, `pr_link_pattern`.
- `BridgeConfig.github.label_prefix` / `label_aliases` / `manage_labels` are parsed with verbatim-alias semantics in `parse_github`.
- `BridgeConfig.custom_fields` resolves the four loop-side field names (`attempt_count`, `last_failure_log`, `max_attempts_override`, `failure_category`); P1-F just reads them off `state.config.custom_fields`.
- `octocrab` and `liquid` are already in `crates/sinfonia-bridge/Cargo.toml` from P1-D.
- `sinfonia_tracker::IssueTracker::{transition_issue, read_custom_field, write_custom_field, post_comment}` are all implemented on `LinearTracker` from P1-B. P1-F calls them directly — no new tracker methods needed.

### Exit criteria for P1-F

- `cargo check --workspace` compiles clean.
- All P1-F unit tests pass (count: ~4 + ~4 + 3 = ~11 new tests minimum).
- `cargo test --workspace` zero regressions over the post-P1-E baseline (90 tests).
- A signed `check_suite completed` with all-pass conclusions applies the `awaiting-review` label and does NOT transition the ticket.
- A signed `check_suite completed` with at least one failing conclusion increments `sinfonia_attempt_count` on the ticket, writes `sinfonia_last_ci_failure`, transitions to the configured `needs_fixes_state` (or the matched category's `target_state`), applies `needs-fixes` + `failure:<category>` labels, and posts the rendered `failure_comment_template` to the PR.
- The (`max_attempts`+1)-th red run transitions to `blocked_state` and applies `cap-hit` — counter does NOT advance past the cap.
- `manage_labels: false` short-circuits every label call; state transitions still happen.

### Files (still pending; P1-F owns the five marked with †)

```
crates/sinfonia-bridge/
├── Cargo.toml
└── src/
    ├── main.rs                  ← P1-D / P1-E, P1-F adds github client to AppState
    ├── lib.rs                   ← P1-D, P1-F registers feedback/ + labels + github
    ├── config.rs                ← P1-D
    ├── webhook/
    │   ├── mod.rs               ← P1-D / P1-E
    │   ├── verify.rs            ← P1-E
    │   └── handlers.rs          ← P1-E (handle_check_suite / handle_workflow_run swap in P1-F)
    ├── feedback/                ← P1-F †
    │   ├── mod.rs               ← P1-F †  evaluate_ci entry point
    │   ├── categorize.rs        ← P1-F †  pure categorize() function
    │   ├── transition.rs        ← P1-F †  green/red/cap state logic
    │   └── attempts.rs          ← P1-F †  counter read/increment via custom_fields
    ├── labels.rs                ← P1-F †  BridgeLabel + ensure/apply/remove
    ├── github/                  ← P1-F (client.rs PAT-only) / P1-G (auth.rs App + selector)
    │   ├── mod.rs               ← P1-F
    │   ├── auth.rs              ← P1-G  (App mode + mode selection)
    │   └── client.rs            ← P1-F  (PAT-mode octocrab wrapper)
    ├── storage.rs               ← P1-E
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
│   └── sinfonia-bridge/         # the bridge daemon (config + webhook layer landed; feedback loop is P1-F)
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

# 2. Confirm test baseline (should be 90 passing tests, zero failures).
cargo test --workspace --no-fail-fast 2>&1 | grep -E "test result"

# 3. Read the rolling status (this file) and the Phase 1 plan.
cat docs/v0.3-plan/STATUS.md
cat docs/v0.3-plan/01-bridge-mvp.md

# 4. Confirm the working tree shape matches §3 above.
ls crates/
ls crates/sinfonia-bridge/src/        # expect: config.rs, lib.rs, main.rs, storage.rs, webhook/
ls crates/sinfonia-bridge/src/webhook/  # expect: handlers.rs, mod.rs, verify.rs
ls crates/sinfonia-tracker/src/

# 5. Start a P1-F branch off main.
git checkout -b v0.3-phase-1-feedback-loop

# 6. Set up Phase 1 sub-task tracking. The original task IDs from the
#    completed-context conversation are not preserved across context
#    clears — TaskCreate a fresh set for P1-F..P1-I per the table in §1.
```

Previous sessions completed P1-A / P1-B / P1-C / P1-D / P1-E. Four sub-tasks remain (P1-F … P1-I); recreate those as fresh TaskCreate entries.

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
Last merged work: P1-E webhook layer (PR #4, commit 69eb8e0)
Earlier merges: P1-D bridge skeleton (PR #3, commit 07c0381);
                Phase 1 foundation (PR #2: commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/01-bridge-mvp.md   (Phase 1 plan; next deliverable is P1-F in §5.2 / §6 / §7)

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
- The full `cargo test --workspace` count should be roughly 51 sinfonia/tracker + ~50 bridge unit tests (39 from P1-D+P1-E plus P1-F's ~11 + P1-G's ~5) + 9 integration tests ≈ ~110 tests.

Phase 1 is then shippable on its own; Phases 2–7 land on top of it.

---

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Ready to resume from a fresh context with this doc as the entry point.
