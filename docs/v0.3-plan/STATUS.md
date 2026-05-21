# v0.3.0 milestone — status & handoff

**Last updated:** 2026-05-21 (P1-H merged; P1-I queued)
**Updated by:** Brett (orchestrated via Claude Opus 4.7)
**Branch state:** `main` contains the Phase 1 foundation (#2 — workspace conversion + tracker extensions + H-1 fix), the bridge skeleton (#3 — P1-D: `sinfonia-bridge` crate, BRIDGE.md parser, axum router with `/health` + stub `/webhook`), the real webhook layer (#4 — P1-E: HMAC-SHA256 verification, SQLite idempotency, `pull_request` / `check_suite` / `workflow_run` dispatch), the feedback loop (#5 — P1-F: `evaluate_ci` orchestrator, categorize / attempts / transition modules, `LabelManager` + `BridgeLabel`, PAT-mode `GhOps`), GitHub authentication + the install gate (#6 — P1-G: `github::auth` mode selector, `AppModeGhOps` with per-owner installation cache, `sinfonia-bridge --self-test` runner, `server.public_url` config field), **and** the wiremock-backed integration suite (#7 — P1-H: `tests/bridge_e2e.rs` boots the full daemon against per-test GitHub + Linear `MockServer`s and asserts on all nine §9.2 scenarios end-to-end).

This file is the **rolling milestone status**. Future agents resuming work on v0.3.0 should read this *before* the per-phase plans — it tells you what's done, what's next, and the decisions that aren't obvious from the code alone.

---

## TL;DR for the next agent

Phase 1 *foundation* (P1-A / P1-B / P1-C), the bridge crate skeleton (P1-D), the webhook layer (P1-E), the feedback loop (P1-F), GitHub authentication + the install gate (P1-G), **and** the wiremock-backed integration tests (P1-H — `tests/bridge_e2e.rs` boots the full daemon and exercises all nine §9.2 scenarios end-to-end) are merged to `main`. The single remaining Phase 1 deliverable is **P1-I: the Phase 1 docs** — `BRIDGE.example.md` at the repo root, the `docs/SPEC.md` §11.6 draft (recommended bridge extension contract), a CHANGELOG entry for v0.3.0-alpha.1, and a README "What's new in v0.3" stub. The plan in `01-bridge-mvp.md` §12 is the deliverable checklist; this STATUS doc captures the *implementation* deltas the docs need to reflect that aren't in the plan.

The single most important non-obvious decision made during the foundation work: **`CustomFieldValue` was collapsed from four variants (`Null` / `Number` / `Decimal` / `LongText` / `Url`) to three (`Null` / `Number` / `String`)** because serde's `#[serde(untagged)]` deserializer can't distinguish multiple JSON-string variants. See §5 below. This is the foundational decision the §11.6 draft has to document — the bridge writes cost values, URLs, and long-text fields all as `CustomFieldValue::String(...)`.

The single most important non-obvious decision pending for P1-I: `BRIDGE.example.md` is a **real working config that `sinfonia-bridge --check` parses cleanly**, not a YAML cheat-sheet of every imaginable field. Same convention as `WORKFLOW.example.md`. The `--check` flag from P1-D already supports this verification, so the example doc is its own CI gate — running `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` in CI catches schema drift between the parser and the doc. The example must cover every section in `crates/sinfonia-bridge/src/config.rs::BridgeConfig` (the P1-D..P1-G code, NOT the §3 plan-doc draft — the implementation has accumulated drift from the draft schema, the most important pieces of which are listed in §5 below: three-variant `CustomFieldValue` (§5.1), `LinearTracker::new` taking a `TrackerConfig` (§5.3), `server.public_url` (added in P1-G), the verbatim label-alias semantics (H-4, §7 of the plan), and the `failure_prefix` alias field in `LabelAliases` that the plan §3 doesn't show but `config.rs` accepts).

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
| **P1-I** Phase 1 docs (BRIDGE.example.md, SPEC §11.6 draft, CHANGELOG, README stub) | §12 | ⬜ not started | Next (and final) Phase 1 deliverable |

### Test baseline on `main`

- `cargo test --workspace --no-fail-fast` → **149 tests pass, 0 failures**
  - 32 sinfonia unit tests (mostly under `crates/sinfonia/src/agent/cli/tests`, `config/`, `orchestrator/`, `template/`, `workspace/`)
  - 12 `spec_conformance.rs` integration tests
  - 7 sinfonia-tracker tests (1 base64 + 6 custom_fields)
  - 89 sinfonia-bridge unit tests (67 from P1-D+P1-E+P1-F plus 22 from P1-G covering `github::auth`, `selftest`, and the `config::server.public_url` round-trips)
  - 9 sinfonia-bridge integration tests (`tests/bridge_e2e.rs`, one per §9.2 scenario)
- `cargo run -p sinfonia-bridge -- BRIDGE.md --check` → `ok` (exit 0) on valid, descriptive error (exit 1) on invalid. Phase P1-I should add `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` as a docs-side smoke check (CI gates schema drift between the parser and the example).
- `cargo run -p sinfonia-bridge -- BRIDGE.md --self-test` → one labelled `PASS`/`FAIL`/`SKIP` line per check; exit code = number of `FAIL` lines (SKIPs don't count). App-mode token-mint + REST round-trip now covered by `bridge_e2e.rs` scenario 8.
- `GET /health` returns `{"service":"sinfonia-bridge","status":"ok","tenant_id":<...>,"tracker":"linear"}`.
- `POST /webhook` flow (now exercised end-to-end against wiremock for every §9.2 scenario):
  - Signed `pull_request opened` → 202 + row in `pr_ticket_map`.
  - Same `delivery_id` redelivered → 200 `{"status":"duplicate"}`, no DB mutation.
  - Wrong HMAC → 401, no DB mutation.
  - Signed `check_suite completed` with a mapped PR → 202 + per-PR `CiOutcome` (green / red / cap_hit / pending / no_mapped_pr) returned in the JSON body. PAT-mode and App-mode auth paths both wired through `OctocrabGhOps` / `AppModeGhOps` against the mock.
- `scripts/verify-workspace-move.sh` → no longer applicable post-merge (the script's purpose was to gate the workspace-move commit; it can stay in-repo as a one-shot artifact).

---

## 2. What's next: P1-I — Phase 1 documentation

The final Phase 1 deliverable, and the only docs-only one. Source of truth: `docs/v0.3-plan/01-bridge-mvp.md` §12 (the deliverable checklist's **Documentation** sub-list). Four files; no production-code change; the existing 149-test baseline must stay green (this is a docs PR — any regression is your bug).

### Scope

Four artifacts:

| Artifact | Path | Style anchor |
|---|---|---|
| Bridge config example | `BRIDGE.example.md` (repo root) | `WORKFLOW.example.md` (same dir) |
| Bridge extension contract | `docs/SPEC.md` §11.6 — insert after §11.5 "Tracker Writes (Important Boundary)" | The other §11 subsections; keep RFC-2119 "MUST/SHOULD/MAY" voice |
| Changelog entry | `CHANGELOG.md` (top, under `## [Unreleased]`) | The existing `## [0.1.0]` entry |
| README stub | `README.md` (a short section right above `## Sinfonia vs. Symphony`) | The existing intro paragraphs |

### `BRIDGE.example.md` — content requirements

Mirror `WORKFLOW.example.md`'s style verbatim: YAML front matter inside a `---` fence, then a short Markdown body. Cover **every section** the parser in `crates/sinfonia-bridge/src/config.rs` reads, with inline `# comments` explaining each field. Must include:

- `tracker:` block (Linear), with the optional `endpoint:` / `active_states:` / `terminal_states:` fields that `parse_tracker` reads (these are NOT in the §3 plan-doc draft but ARE in the parser). A commented-out Jira block ("Phase 4 — not supported yet") for completeness.
- `github:` block, with BOTH a PAT example and a commented-out App-mode example (`app_id` + `private_key:` with both inline-PEM and `@/path/to/key.pem` syntax shown). Show `manage_labels: false` as a commented alternative. Show `label_prefix:` and a `label_aliases:` block including the verbatim-semantics note from H-4 (`in_progress: "ai:working"` → final label is `ai:working`, prefix ignored). Don't forget `failure_prefix:` — `LabelAliases` has this field, plan §3 omits it.
- `feedback_loop:` block with `max_attempts`, `needs_fixes_state`, `blocked_state`, `pr_link_pattern` (show the default verbatim), `required_checks: []`, the budget-cap fields (`max_tokens_per_ticket: null` etc., commented as "Phase 3 — accepted but unused in Phase 1"), `failure_comment_template:` (a working multi-line `|` literal), and a `failure_categories:` block showing two real entries plus the implicit synthetic `default`.
- `custom_fields:` block listing all seven keys with their `sinfonia_*` defaults. The Phase 3 keys (`tokens_consumed`, `cost_consumed_usd`, `max_cost_override_usd`) are required even in Phase 1 (see §5 / config.rs Rule 8).
- `server:` block with `bind`, `port`, and `public_url` (P1-G addition; show with a real-looking https URL).
- `storage:` block with `state_db_path` (use `~/.sinfonia/bridge.db` to show shellexpand works).
- `telemetry:` block with `service_name`, the optional Phase 3 fields commented out (`otlp_endpoint`, `tenant_id`, `sinfonia_events_secret`, `sinfonia_event_subscribe_url`, `sinfonia_event_callback_url`).

Verification: `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` MUST exit 0. Run it locally before committing. (The file's purpose is dual: documentation for humans + a parser-side smoke test.)

### `docs/SPEC.md` §11.6 — content requirements

A **draft** of the recommended bridge-service extension contract. Spec voice (RFC-2119). Insert as a new subsection between §11.5 and §12. The exit text "Phase 7 finalizes this; a draft lands with Phase 1" appears at the top so readers know this is provisional. Cover:

- **Bridge service definition**: an OPTIONAL out-of-band component that owns CI-result interpretation, attempt-counter management, and PR↔ticket mapping. Sinfonia itself remains a polling reader; the bridge is the only writer of `sinfonia_*` custom fields.
- **Custom-field marker envelope** (`sinfonia_bridge_state_v1`): single JSON object stored as a tracker-owned comment (Linear) or real custom field (Jira, Phase 4). Document the field set: `attempt_count`, `last_ci_failure`, `failure_category`, `tokens_consumed`, `cost_consumed_usd`. Versioned via the envelope key for future schema migration.
- **WELL_KNOWN_FIELDS registration**: implementations writing a NEW `sinfonia_*` key MUST add it to `WELL_KNOWN_FIELDS`. Without that, `template.rs`'s `| default:` filter raises "Unknown index" against missing values in strict Liquid mode (§5.2). Quote the well-known list.
- **Webhook surface**: SHOULD accept `pull_request` (`opened` / `synchronize` / `reopened` / `closed`), `check_suite.completed`, `workflow_run.completed`. MUST verify HMAC-SHA256 against a shared secret using constant-time compare. MUST persist `X-GitHub-Delivery` IDs for idempotency. Note that `reopened` is treated as a mapping-update event (§5.9) so contributors can re-point a tracker link between close and reopen.
- **HTTP response contract**: every non-200 response is `application/json` with an `error` field; 200/202 responses include `status` and (for queued events) the event-specific fields documented in §5.8. Quote the four canonical shapes.
- **PR↔ticket mapping**: implementations MAY persist the mapping (Sinfonia's bridge uses SQLite) or recompute on every poll. Source of truth is the PR body matched against a regex; the persistent table is a cache only. Document the default regex `(?i)(?:closes|fixes|resolves)\s+([A-Z]+-\d+|[a-z]+-\d+)`.
- **Linear comment-boundary note**: `LinearTracker` fetches `comments(first: 100)` per ticket (§5.4). For tickets with >100 bot interactions the marker would scroll off. RECOMMENDED mitigation: filter by author at the GraphQL layer, or migrate the marker to a Linear custom-field once Linear ships a stable custom-field API. Implementations MUST document the boundary if they exceed 100 marker rewrites per ticket.
- **GitHub auth**: PAT and App modes are both REQUIRED for a conforming bridge. For App mode, `octocrab::Octocrab::installation(InstallationId)` produces a per-installation scoped client; the bridge SHOULD cache these per `owner` to avoid re-minting tokens.
- **Self-test contract**: every conforming bridge SHOULD expose a `--self-test` mode that emits one `PASS` / `FAIL` / `SKIP` line per check, with exit code = number of `FAIL`s (SKIPs do not count). Quote the canonical line format from `01-bridge-mvp.md` §8.

Length target: 150–300 lines (similar density to §11.5 + §11.4 combined). This is a draft, not a finalized contract; mark every Phase 3 / Phase 4 / Phase 7 future-work item explicitly.

### `CHANGELOG.md` — content requirements

A new section at the top under `## [Unreleased]` (or replace `[Unreleased]` if the project prefers per-pre-release entries — current style has `[Unreleased]` as a placeholder, so add `## [0.3.0-alpha.1] — YYYY-MM-DD` below it). Mirror the existing `## [0.1.0]` entry's tone — `### Added`, then optional `### Changed` / `### Known limitations`. Cover:

- Workspace conversion: `crates/sinfonia/`, `crates/sinfonia-tracker/`, `crates/sinfonia-bridge/`.
- `IssueTracker` trait extensions for custom fields (Linear via marker-comment, Jira stubbed).
- `Issue.fields` + Liquid scope wiring (`{{ issue.fields.sinfonia_* }}`).
- New `sinfonia-bridge` binary: `BRIDGE.md` config + `--check` + `--self-test` flags; HMAC-verified webhook; SQLite idempotency; CI-result interpretation with category routing; PR label management; PAT + App GitHub auth.
- Tests: bridge crate gains 89 unit + 9 integration tests.
- New repo-root docs: `BRIDGE.example.md`, `docs/SPEC.md` §11.6.

Don't claim Phase 2-7 features — those land in their own milestones.

Add the new version's compare link at the bottom alongside the existing ones (`[0.3.0-alpha.1]: https://github.com/O-Side-Systems/sinfonia/compare/v0.1.0...v0.3.0-alpha.1`).

### `README.md` — content requirements

A short "**What's new in v0.3 (preview)**" section, ~10–15 lines, inserted right before the existing `## Sinfonia vs. Symphony` heading. Phase 7 expands this; the stub:

- Names the new `sinfonia-bridge` binary in one sentence.
- Points at `BRIDGE.example.md` for config and `docs/SPEC.md` §11.6 for the contract draft.
- Calls out that Sinfonia (the daemon) is unchanged — Phase 1 adds the bridge alongside, not in place of, the existing polling loop. (This matters because a reader who's already running v0.1.0 shouldn't think the binary they're running just changed semantics.)
- One-line "still in alpha; Phases 2-7 land budget/telemetry, Jira bridge writes, skills CLI, Docker, and final docs."

Do NOT rewrite "Getting started" or any existing section. The stub is additive.

### Verification before opening the PR

1. `cargo check --workspace` — should still build clean.
2. `cargo test --workspace --no-fail-fast` — 149 tests passing, zero failures.
3. `cargo run -p sinfonia-bridge -- BRIDGE.example.md --check` — must exit 0. If it errors, the example doc has drifted from the parser; fix the example, not the parser.
4. Visually re-read `docs/SPEC.md` end-to-end to confirm §11.6 doesn't break the numbering of §12 onward.

### Exit criteria for P1-I

- All four artifacts land in a single PR titled `P1-I: Phase 1 documentation (BRIDGE.example.md, SPEC §11.6 draft, CHANGELOG, README stub)`.
- `BRIDGE.example.md` parses cleanly via `--check`.
- The Phase 1 deliverable checklist in `01-bridge-mvp.md` §12 has every box checked.
- A follow-up STATUS commit on `main` ("STATUS: mark P1-I merged, Phase 1 complete") flips the last sub-task row to ✅ and adds a closing note that Phase 1 is shippable on its own — Phases 2-7 land on top of it.

### Files (P1-I owns the files marked with †)

```
sinfonia/
├── BRIDGE.example.md            ← P1-I †  new file, sibling to WORKFLOW.example.md
├── CHANGELOG.md                 ← P1-I †  add v0.3.0-alpha.1 entry under [Unreleased]
├── README.md                    ← P1-I †  add "What's new in v0.3 (preview)" stub
└── docs/
    └── SPEC.md                  ← P1-I †  insert §11.6 between §11.5 and §12
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
│   └── sinfonia-bridge/         # the bridge daemon (config + webhook + feedback loop + GitHub auth + --self-test + wiremock integration tests all landed; only Phase 1 docs P1-I remain)
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

# 2. Confirm test baseline (should be 149 passing tests, zero failures).
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
ls crates/sinfonia-bridge/tests/        # expect: bridge_e2e.rs
ls crates/sinfonia-tracker/src/

# 5. Start a P1-I branch off main.
git checkout -b v0.3-phase-1-docs

# 6. Set up Phase 1 sub-task tracking. The original task IDs from the
#    completed-context conversation are not preserved across context
#    clears — TaskCreate a fresh set for the four P1-I artifacts per §2 above.
```

Previous sessions completed P1-A / P1-B / P1-C / P1-D / P1-E / P1-F / P1-G / P1-H. One sub-task remains (P1-I — Phase 1 docs); recreate as fresh TaskCreate entries.

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
Last merged work: P1-H wiremock integration tests (PR #7, commit d7ad72d, merge 749c9c4)
Earlier merges: P1-G GitHub auth (PR #6, commit b0d7272, merge 8055659);
                P1-F feedback loop (PR #5, commit 9d33d51);
                P1-E webhook layer (PR #4, commit 69eb8e0);
                P1-D bridge skeleton (PR #3, commit 07c0381);
                Phase 1 foundation (PR #2: commits 82d2d2f + 3f045e9)

Read these in this order:
  1. docs/v0.3-plan/STATUS.md   (this file — rolling milestone status)
  2. docs/v0.3-plan/00-overview.md   (milestone index, phase deps)
  3. docs/v0.3-plan/01-bridge-mvp.md   (Phase 1 plan; next (and final) deliverable is P1-I in §12)

Source of truth for the underlying change set:
  /Users/brettlee/Downloads/sinfonia-change-proposal.md

Workspace shape:
  crates/sinfonia/         — the daemon
  crates/sinfonia-tracker/ — shared tracker (Linear + Jira adapters, custom_fields)
  crates/sinfonia-bridge/  — bridge binary; config + storage + webhook dispatch + feedback loop + PAT/App auth + --self-test
                             + wiremock integration tests all landed; only Phase 1 docs (P1-I) still to come

Test baseline: 149 passing, 0 failures. P1-I is docs-only; do not regress this.
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
- The full `cargo test --workspace` count is 51 sinfonia/tracker + 89 bridge unit tests (67 from P1-D+P1-E+P1-F plus 22 from P1-G) + 9 integration tests = 149 tests as of P1-H. P1-I is docs-only and is not expected to change this number.

Phase 1 is then shippable on its own; Phases 2–7 land on top of it.

---

**Last actionable step before clearing context:** check `docs/v0.3-plan/STATUS.md` reads cleanly (this file), then `git push` if any local-only changes remain. Ready to resume from a fresh context with this doc as the entry point.
