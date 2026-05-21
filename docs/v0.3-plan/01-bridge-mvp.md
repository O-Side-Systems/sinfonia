# Phase 1 — `sinfonia-bridge` MVP

**Target:** v0.3.0
**Scope:** New companion daemon that closes the CI → tracker feedback loop. Linear tracker only in this phase; Jira lands in Phase 4.
**Estimated size:** ~2 400 LOC of Rust + ~500 LOC of tests + ~600 LOC of docs (total ~3 500). This is up from the original ~2 000 to absorb the custom-field-to-template plumbing that has to land in Phase 1 (see §4 and §5.3); the proposal's 2 000 estimate covered only the bridge binary and missed the small Sinfonia-side template work that's load-bearing for the loop.
**Depends on:** none (foundation phase)
**Unblocks:** Phase 3 (telemetry subscribes to bridge spans), Phase 4 (Jira), Phase 5 (`setup-bridge` skill), Phase 6 (`sinfonia-bridge` Docker image).

This document is an implementation plan, not the implementation. It says *what we'll build, where it'll live, and how we'll know it works*. No Rust gets written until this plan is approved.

---

## 1. Decisions locked by the proposal

The change-proposal text already settles these — listed here so reviewers don't re-litigate them.

1. **The bridge is a separate binary**, not a feature flag in `sinfonia`. Sinfonia stays free of tracker write credentials and webhook handling.
2. **Tracker writes go through the same adapter Sinfonia uses**, so we extract `src/tracker/` into a shared workspace member crate.
3. **Attempt counts live in tracker custom fields**, not in bridge-local state. Restarting the bridge can't lose loop progress.
4. **Idempotency uses GitHub delivery IDs** persisted in SQLite. No accidental double-increments on webhook redelivery.
5. **Two GitHub auth modes** (PAT and App). PAT is the default; the `setup-bridge` skill in Phase 5 routes to App when needed.
6. **Failure categorization is in scope for v0.3**, not deferred.
7. **PR labels are in scope for v0.3**, with `manage_labels: false` opt-out.

Open items at the end of this doc — none of them block starting Phase 1.

---

## 2. Repository restructure (workspace conversion)

`sinfonia` is currently a single crate (`Cargo.toml` declares one `[[bin]]` and one `[lib]`). The bridge needs to reuse tracker code without copy-paste, so we convert to a Cargo workspace.

### Target layout

```
sinfonia/                          # repo root
├── Cargo.toml                      # workspace manifest only
├── crates/
│   ├── sinfonia/                   # the daemon (current src/ moves here)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── lib.rs
│   │       ├── agent/
│   │       ├── config/
│   │       ├── domain.rs
│   │       ├── errors.rs
│   │       ├── http/
│   │       ├── orchestrator/
│   │       ├── template.rs
│   │       └── workspace/
│   ├── sinfonia-tracker/           # shared crate (new)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── linear.rs
│   │       ├── jira.rs
│   │       └── custom_fields.rs    # new — see §4
│   └── sinfonia-bridge/            # the bridge daemon (new)
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── lib.rs
│           ├── config.rs           # BRIDGE.md parser
│           ├── webhook/
│           │   ├── mod.rs
│           │   ├── verify.rs       # HMAC signature verification
│           │   └── handlers.rs     # axum routes
│           ├── feedback/
│           │   ├── mod.rs
│           │   ├── categorize.rs   # check-name → category
│           │   ├── transition.rs   # state transition logic
│           │   └── attempts.rs     # counter read/increment
│           ├── labels.rs           # PR label management
│           ├── github/
│           │   ├── mod.rs
│           │   ├── auth.rs         # PAT vs App selector
│           │   └── client.rs       # octocrab wrapper
│           ├── storage.rs          # rusqlite idempotency
│           └── selftest.rs         # --self-test command
└── tests/                          # workspace-wide integration tests
    ├── spec_conformance.rs         # existing; stays
    └── bridge_e2e.rs               # new
```

### What moves where

| Current path | Destination | Notes |
|---|---|---|
| `src/main.rs` | `crates/sinfonia/src/main.rs` | unchanged |
| `src/lib.rs` | `crates/sinfonia/src/lib.rs` | `pub use sinfonia_tracker::{IssueTracker, build_from_config}` re-exports kept for stability |
| `src/tracker/` | `crates/sinfonia-tracker/src/` | promoted to its own crate |
| `src/domain.rs` | `crates/sinfonia/src/domain.rs` | stays in sinfonia; re-exported types referenced by the tracker crate move to `sinfonia-tracker::types` |
| `src/agent/`, `src/config/`, `src/http/`, `src/orchestrator/`, `src/workspace/`, `src/template.rs`, `src/errors.rs` | `crates/sinfonia/src/...` | unchanged |
| `tests/spec_conformance.rs` | `crates/sinfonia/tests/` | scoped to the sinfonia crate |

### Tracker crate API

The tracker crate exports exactly what both consumers need and nothing more:

```rust
// crates/sinfonia-tracker/src/lib.rs
pub mod custom_fields;
pub mod linear;
pub mod jira;
pub mod types;            // Issue, IssueState (moved from src/domain.rs)

pub use linear::LinearTracker;
pub use jira::JiraTracker;

#[async_trait::async_trait]
pub trait IssueTracker: Send + Sync {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>>;
    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>>;
    async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<Vec<IssueState>>;

    // NEW for bridge
    async fn transition_issue(&self, id: &str, target_state: &str) -> Result<()>;
    async fn read_custom_field(&self, id: &str, key: &str) -> Result<CustomFieldValue>;
    async fn write_custom_field(&self, id: &str, key: &str, value: CustomFieldValue) -> Result<()>;
    async fn ensure_custom_field(&self, schema: &CustomFieldSchema) -> Result<()>;
    async fn post_comment(&self, id: &str, body: &str) -> Result<()>;

    async fn raw_graphql(&self, q: &str, vars: Option<serde_json::Value>) -> Result<serde_json::Value>;
}
```

The first three methods are exactly what `crates/sinfonia/src/tracker/mod.rs:16-35` already defines today. The five new methods (`transition_issue`, `read_custom_field`, `write_custom_field`, `ensure_custom_field`, `post_comment`) are bridge-only — Sinfonia keeps not calling them per §11.5 of the spec.

Sinfonia consumes the trait via `sinfonia_tracker::IssueTracker`; the existing `src/tracker/mod.rs` thin shim file becomes:

```rust
// crates/sinfonia/src/tracker/mod.rs
pub use sinfonia_tracker::{IssueTracker, Issue, IssueState};

pub fn build_from_config(cfg: &crate::ServiceConfig) -> Result<std::sync::Arc<dyn IssueTracker>> {
    // unchanged; just delegates to sinfonia_tracker constructors
}
```

This means everywhere in Sinfonia that imports from `crate::tracker::` keeps compiling without source changes. Only the `Cargo.toml` files and the actual file moves are visible in the diff.

### Why this is the right time to extract

Doing the workspace conversion *as the first step of v0.3* avoids the alternative — having the bridge depend on a path in `sinfonia/src/tracker/`, which is messier than a proper workspace dependency and re-introduces the copy-paste risk this section is trying to avoid.

The downside is one large move commit. Mitigation: that commit changes paths only; logic is byte-for-byte preserved. Reviewers can verify with `git log --follow` and `cargo test` runs on each commit.

### Workspace-move verification script

"Paths only, no logic changes" is a process claim, not an artifact. A small script makes it a verifiable artifact:

```bash
# scripts/verify-workspace-move.sh
# Run from the workspace-move commit. Compares test output to the prior commit.
set -euo pipefail
before=$(git rev-parse HEAD~1)
after=$(git rev-parse HEAD)

run_tests () {
    git -c advice.detachedHead=false checkout --quiet "$1"
    cargo test --workspace --no-fail-fast 2>&1 \
        | grep -E '^test result:' \
        | sort > "/tmp/sinfonia-test-result.$1.txt"
}

trap 'git checkout --quiet -' EXIT

run_tests "$before"
run_tests "$after"

if diff -u "/tmp/sinfonia-test-result.$before.txt" "/tmp/sinfonia-test-result.$after.txt"; then
    echo "OK: workspace move preserved test set + pass-count."
else
    echo "FAIL: test outcome differs across the workspace move commit." >&2
    exit 1
fi
```

The script lives at `scripts/verify-workspace-move.sh` and runs as a Phase 1 CI step before any other Phase 1 PR is merged. If anyone questions the move later, the script answers them.

---

## 3. `BRIDGE.md` configuration file

Same YAML-front-matter + Markdown body convention as `WORKFLOW.md`. The parser lives in `crates/sinfonia-bridge/src/config.rs` and reuses the front-matter splitter currently in `crates/sinfonia/src/config/loader.rs` (we move that helper into a small `sinfonia-config-shared` crate, or copy it — see §10 open questions).

### Schema (canonical)

```yaml
---
tracker:
  kind: linear                                 # linear | jira (Phase 4)
  api_key: $LINEAR_API_KEY                     # env var indirection like WORKFLOW.md
  project_slug: my-awesome-project-abc123def456

github:
  webhook_secret: $GH_WEBHOOK_SECRET
  # PAT mode:
  pat: $GH_PAT
  # OR App mode (mutually exclusive with pat):
  # app_id: $GH_APP_ID
  # private_key: $GH_APP_KEY                   # PEM contents or @/path/to/key.pem

  manage_labels: true
  label_prefix: "sinfonia"
  label_aliases: {}                            # override defaults; see §7

feedback_loop:
  max_attempts: 5
  needs_fixes_state: "Needs Fixes"
  blocked_state: "Blocked - Human Review"
  pr_link_pattern: '(?i)(?:closes|fixes|resolves)\s+([A-Z]+-\d+|[a-z]+-\d+)'
  required_checks: []                          # empty = all required checks must pass

  # Budget caps are scoped to Phase 3. The keys are accepted but no-op in Phase 1
  # so config files survive the upgrade without edits.
  max_tokens_per_ticket: null
  max_cost_per_ticket_usd: null
  budget_exceeded_state: "Blocked - Budget Cap"

  failure_comment_template: |
    CI failed on attempt {{ attempt }} of {{ max_attempts }}.
    ...

  failure_categories:                          # optional; absent = single-category routing
    - name: lint
      check_pattern: '(?i)(lint|prettier|eslint|rustfmt|clippy|black|ruff)'
      target_state: "Needs Fixes - Lint"
      priority: 10
    - name: default
      target_state: "Needs Fixes"
      priority: 0

custom_fields:
  attempt_count: sinfonia_attempt_count
  last_failure_log: sinfonia_last_ci_failure
  max_attempts_override: sinfonia_max_attempts
  failure_category: sinfonia_failure_category
  # Phase 3 fields (accepted, unused in Phase 1):
  tokens_consumed: sinfonia_tokens_consumed
  cost_consumed_usd: sinfonia_cost_consumed_usd
  max_cost_override_usd: sinfonia_max_cost_usd

server:
  bind: "0.0.0.0"
  port: 8081

storage:
  state_db_path: ~/.sinfonia/bridge.db

telemetry:                                     # Phase 3 will fill this in
  otlp_endpoint: null
  service_name: sinfonia-bridge
  tenant_id: null
  # Shared HMAC secret for the typed Sinfonia↔bridge event channel introduced
  # in Phase 3 (see 03-telemetry-budget.md §7.2). The same value must appear in
  # WORKFLOW.md's telemetry.sinfonia_events_secret, or the channel will reject
  # every event with HTTP 401. Accepts a literal value or $ENV_VAR. Required
  # only when the bridge subscribes to Sinfonia events; unset = no subscription.
  sinfonia_events_secret: $SINFONIA_EVENTS_SECRET
---

# Bridge for my-awesome-project

(Markdown body is for human notes — not parsed.)
```

### Validation rules (enforced at startup; tested in §9)

| Rule | Error message | Test |
|---|---|---|
| Exactly one of `github.pat` or `github.app_id` is set | `"BRIDGE.md github: must set either pat or app_id (mutually exclusive)"` | yes |
| `tracker.kind` is `linear` (Phase 1) — `jira` errors with "deferred to Phase 4" | `"BRIDGE.md tracker.kind 'jira' not supported until Phase 4"` | yes |
| `feedback_loop.max_attempts ≥ 1` | `"max_attempts must be >= 1"` | yes |
| `needs_fixes_state` / `blocked_state` are non-empty | typed error | yes |
| `pr_link_pattern` compiles as a regex | regex compile error wrapped | yes |
| Each `failure_categories[*].check_pattern` compiles as a regex | regex compile error wrapped | yes |
| `failure_categories[*].priority` is unique | `"duplicate priority N across categories X and Y"` | yes |
| `custom_fields.*` values are non-empty strings | typed error | yes |
| If `telemetry.sinfonia_events_secret` resolves to empty when `telemetry.sinfonia_event_subscribe_url` is set, error at startup | `"BRIDGE.md telemetry.sinfonia_events_secret is required when sinfonia_event_subscribe_url is configured"` | yes |

The validation surface is small enough that it fits in `config.rs` without a separate validator module.

### Reload behavior

Initial version: **no hot reload**. `sinfonia-bridge` re-reads `BRIDGE.md` only on process restart. The existing `notify`-based `WorkflowWatcher` (in `crates/sinfonia/src/config/watcher.rs`) is a future enhancement once we have evidence anyone wants it. Keeps the surface area down.

---

## 4. Custom fields — first-class concept

The current `IssueTracker` trait has no custom-fields concept. We add it as a new module in the shared tracker crate:

```rust
// crates/sinfonia-tracker/src/custom_fields.rs

pub enum CustomFieldValue {
    Null,
    Number(f64),
    Decimal(rust_decimal::Decimal),    // for cost in USD
    LongText(String),
    Url(String),
}

pub struct CustomFieldSchema {
    pub key: String,                   // e.g. "sinfonia_attempt_count"
    pub display_name: String,          // e.g. "Sinfonia Attempt Count"
    pub kind: CustomFieldKind,
    pub description: Option<String>,
}

pub enum CustomFieldKind {
    Number,
    Decimal,
    LongText,
    Url,
}
```

Linear and Jira diverge here:

- **Linear** doesn't have arbitrary custom fields the way Jira does. We model "custom fields" on Linear as a structured comment thread the bridge owns (one comment per ticket, JSON body), keyed by a sentinel header. The implementation reads and re-writes that single comment on every update. Tradeoff: a single Linear bot comment per ticket is mildly ugly but it's the cleanest path. Documented in `docs/SPEC.md` §11.6.
- **Jira** uses real custom fields via `/rest/api/3/field`. We create them at startup via `ensure_custom_field`. Lands in Phase 4.

In Phase 1, only the Linear path is implemented. The trait method exists for both, but Jira returns `Err(NotImplemented)` until Phase 4.

### 4.1 Why the Linear-as-comment approach

Considered alternatives:

1. **Linear "extension fields" (private API).** Not documented; would break on Linear's whim.
2. **Linear `description` munging.** Steps on user content; rejected.
3. **Bot-owned comment with structured payload.** Visible, debuggable, queryable via Linear's GraphQL `comments` field. Chosen.

The comment body is JSON with a sentinel:

```json
{
  "sinfonia_bridge_state_v1": {
    "attempt_count": 3,
    "last_ci_failure": "...",
    "failure_category": "e2e",
    "tokens_consumed": 412053,
    "cost_consumed_usd": "8.23"
  }
}
```

The marker `sinfonia_bridge_state_v1` lets us migrate the schema later without ambiguity.

### 4.2 Threading custom fields into the agent prompt — Phase 1 owns this

The whole feedback loop's value depends on the agent prompt being able to read what the bridge wrote. The proposal example (lines 230-245) uses `{{ issue.fields.sinfonia_last_ci_failure }}` in the per-state Liquid prompt. That works only if three Sinfonia-side changes land in Phase 1 (this is not Phase 5's job — Phase 5 just authors skills that produce templates referencing the field, and those templates need the field to exist at render time):

1. **Extend the `Issue` domain type** with a `fields: HashMap<String, CustomFieldValue>` map.

   ```rust
   // crates/sinfonia-tracker/src/types.rs (moved from crates/sinfonia/src/domain.rs)
   pub struct Issue {
       pub id: String,
       pub identifier: String,
       // ... existing fields ...
       pub fields: std::collections::HashMap<String, CustomFieldValue>,
   }
   ```

   New empty by default — pre-bridge deployments and non-tracked tickets simply have an empty map.

2. **Populate the map on fetch.** `LinearTracker::fetch_candidate_issues` (and `fetch_issues_by_states`) parse the bridge's `sinfonia_bridge_state_v1` marker comment if present, and unfold its JSON keys into `Issue.fields` with the bridge's `custom_fields:` config providing the key → field-name mapping. Jira does the equivalent via real custom-field reads in Phase 4.

3. **Expose `Issue.fields` in the Liquid scope.** `crates/sinfonia/src/template.rs` today does `liquid::model::to_value(issue)` (lines 25-32 in the existing single-crate layout). For the existing path to keep working, `CustomFieldValue` must have a **hand-written** `Serialize` impl that flattens to the underlying primitive — NOT the derived one, which would emit tagged JSON like `{"Number": 3}` and render as the wrong string in the prompt:

   ```rust
   // crates/sinfonia-tracker/src/custom_fields.rs
   impl serde::Serialize for CustomFieldValue {
       fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
           match self {
               Self::Null         => ser.serialize_none(),
               Self::Number(n)    => n.serialize(ser),
               Self::Decimal(d)   => d.to_string().serialize(ser),  // %.6f stable; not f64
               Self::LongText(s)  => s.serialize(ser),
               Self::Url(s)       => s.serialize(ser),
           }
       }
   }
   ```

   Do NOT add `#[derive(Serialize)]` to the enum — the round-trip test in step (4) will fail, but the failure is easier to spot if the stub is written up front. With this impl in place, `{{ issue.fields.sinfonia_last_ci_failure }}` Just Works and `Issue` keeps its existing `#[derive(Serialize)]` unchanged.

4. **Round-trip test.** A new test in `template.rs::tests` renders a prompt containing `{{ issue.fields.sinfonia_last_ci_failure | default: "(none)" }}` against an `Issue` with that field populated and asserts the rendered string. A second test against an `Issue` with the field absent asserts the `default:` filter fallback fires (rather than a strict-mode template error).

This is ~80–120 LOC of Sinfonia-side change plus the two tests. It is non-negotiable Phase 1 work — without it, no skill template in any subsequent phase can rely on the field syntax, and Phase 1's exit criterion ("the agent runs with the failure log in its prompt") isn't actually achievable.

---

## 5. Event flow (canonical sequence)

Reproducing the proposal text, expanded with implementation-level detail and the failure modes the proposal glosses over.

### 5.1 Webhook arrives

1. GitHub POST → `POST /bridge/webhook` (axum route in `webhook/handlers.rs`).
2. `webhook/verify.rs` validates the HMAC-SHA256 signature against `github.webhook_secret`. Constant-time comparison via `subtle::ConstantTimeEq`.
3. Idempotency check (`storage.rs`): does `delivery_id` exist in `processed_deliveries`?
   - **Yes** → return 200 OK without processing. We've already handled this.
   - **No** → insert the row with `processed_at = NOW()`, continue.
4. The handler dispatches on `X-GitHub-Event`:
   - `pull_request` (action `opened` / `synchronize` / `closed`) → update `pr_id → ticket_id` mapping (in-memory cache, see §5.5).
   - `check_suite` (action `completed`) → trigger CI evaluation.
   - `workflow_run` (action `completed`) → same as `check_suite`.

### 5.2 CI evaluation

1. Resolve PR number from event payload.
2. Look up linked ticket via `pr_id → ticket_id` cache; if missing, scan the PR body for `pr_link_pattern` against the configured tracker project.
3. Read all check runs for the PR via `octocrab`. Collect the non-skipped ones.
4. Aggregate outcome:
   - All passed → **green**.
   - At least one failed/timed_out/cancelled → **red**.
   - Some still pending → **wait** (return 200, no action; the next `check_suite` will retrigger).
5. On green:
   - Apply `sinfonia:awaiting-review` label, remove `sinfonia:in-progress` / `sinfonia:needs-fixes`.
   - Emit `bridge.ci_result` span (Phase 3 detail; in Phase 1 just `tracing::info!`).
   - Return.
6. On red:
   - Read `sinfonia_attempt_count` from the ticket (default 0 if missing).
   - Read `sinfonia_max_attempts` override (default `feedback_loop.max_attempts`).
   - If `attempt_count + 1 > max_attempts`:
     - Transition ticket to `blocked_state`.
     - Apply `sinfonia:cap-hit` label.
     - Post failure comment to the PR explaining the cap.
   - Else:
     - Determine `failure_category` (see §6).
     - Increment `sinfonia_attempt_count` to `N+1`.
     - Write `sinfonia_last_ci_failure` with the comment-template-rendered failure summary.
     - Write `sinfonia_failure_category` (e.g. "e2e").
     - Transition ticket to the category's `target_state` (default = `needs_fixes_state`).
     - Apply `sinfonia:needs-fixes` + `sinfonia:failure:<category>` labels.
     - Post failure comment to the PR.

### 5.3 Sinfonia picks it up

Out of scope for this binary, but documented here for completeness: Sinfonia's next poll sees the ticket in `Needs Fixes` (or `Needs Fixes - E2E`, etc.), which is in `active_states`, and dispatches the configured runner. The prompt template references `{{ issue.fields.sinfonia_last_ci_failure }}`, which works because the `Issue.fields` map + tracker population + template scope are all introduced in this same Phase 1 (see §4.2). The full chain — bridge writes the marker comment → tracker fetch parses it into `Issue.fields` → prompt template renders the value into the agent's input — is end-to-end deliverable in Phase 1 with no Phase 5 dependency.

### 5.4 Failure-comment template rendering

We reuse Liquid for the bridge's `failure_comment_template`. Available variables:

| Variable | Source |
|---|---|
| `{{ attempt }}` | current attempt count post-increment |
| `{{ max_attempts }}` | from config (or per-ticket override) |
| `{{ failed_checks }}` | comma-separated list of failed check names |
| `{{ failure_log_excerpt }}` | last 50 lines of the most-failed check's logs, fetched via the GitHub API |
| `{{ failure_category }}` | category name (or `"default"`) |
| `{{ pr_url }}` | GitHub PR URL |
| `{{ ticket_identifier }}` | tracker identifier, e.g. `ENG-42` |

### 5.5 PR ↔ ticket mapping

Stored in `storage.rs` as a second SQLite table — SQLite-only, no in-memory cache layer in Phase 1. (See §11 question 4 for why; a cache adds invalidation complexity for negligible latency wins at v0.3 traffic volumes.)

```sql
CREATE TABLE pr_ticket_map (
    repo TEXT NOT NULL,
    pr_number INTEGER NOT NULL,
    ticket_id TEXT NOT NULL,
    discovered_at INTEGER NOT NULL,
    PRIMARY KEY (repo, pr_number)
);
```

On `pull_request opened` / `synchronize`: parse the PR body and title with `pr_link_pattern`. On match, upsert the row. If no match, the bridge skips this PR — no ticket means no work to do.

The SQLite row IS the source of truth for the mapping during the bridge's lifetime, but it's re-derivable from PR bodies on any restart (the bridge replays open PRs once at startup to rebuild rows that were lost). No durability guarantee is needed beyond that.

---

## 6. Failure categorization

Logic in `crates/sinfonia-bridge/src/feedback/categorize.rs`. Pure function, easy to unit-test.

```rust
pub fn categorize(
    failed_checks: &[String],
    categories: &[FailureCategory],
) -> &FailureCategory {
    let mut matched: Vec<&FailureCategory> = categories
        .iter()
        .filter(|c| match &c.check_pattern {
            None => false,
            Some(re) => failed_checks.iter().any(|name| re.is_match(name)),
        })
        .collect();
    matched.sort_by_key(|c| std::cmp::Reverse(c.priority));
    matched.first().copied()
        .unwrap_or_else(|| default_category(categories))
}
```

Edge cases (tested):

- Empty `failed_checks` slice → caller bug; assert in debug, return default in release.
- Multiple checks failing, each matching a different category → highest-priority wins.
- Equal priorities → forbidden by config validation, so unreachable.
- `failure_categories:` absent in config → in-memory representation has a single synthetic `default` entry pointing at `needs_fixes_state`.

### Per-state runner config (Sinfonia side)

Once the ticket reaches `Needs Fixes - E2E`, Sinfonia's existing per-state override (`StateOverride` in `crates/sinfonia/src/config/typed.rs:150-161`) handles routing. **No Sinfonia code change is required for category routing** — the state-machine support is already there. Phase 5's `setup-state-machine` skill is what helps users author the per-category state overrides.

---

## 7. PR label management

Code in `crates/sinfonia-bridge/src/labels.rs`. The six label kinds correspond to enum variants:

```rust
pub enum BridgeLabel {
    InProgress,
    AwaitingReview,
    NeedsFixes,
    CapHit,
    BudgetExceeded,                       // Phase 3; struct exists, never applied in Phase 1
    Failure(String),                      // category name
}
```

### On startup (per repo seen)

```rust
async fn ensure_labels(client: &Client, repo: &str, prefix: &str, aliases: &LabelAliases) {
    for label in BridgeLabel::all_known() {
        let name = label.full_name(prefix, aliases);
        let color = label.default_color();
        client.labels(repo).get(&name).or_else(|| {
            client.labels(repo).create(name, color, label.description())
        });
    }
}
```

Idempotent. Color defaults per the proposal: in-progress blue (`0e8a16`-ish), awaiting-review green, needs-fixes yellow, cap-hit / budget-exceeded red. Failure-category labels (`sinfonia:failure:e2e`) are created lazily when first encountered.

### `manage_labels: false`

Short-circuits `ensure_labels` and every `apply` / `remove` call. The bridge logs `"label management disabled"` at startup.

### Aliases — full-name verbatim

Resolved: `label_aliases.in_progress = "ai:working"` supplies the **full label name verbatim**. The prefix from `label_prefix` is NOT prepended. This matches the proposal's literal example (lines 268-274 use values like `"ai:working"` that read as full names, not suffixes).

In practice:

| Config | Resulting GitHub label |
|---|---|
| default (no aliases) | `sinfonia:in-progress`, `sinfonia:needs-fixes`, … |
| `label_prefix: "automation"`, no aliases | `automation:in-progress`, … |
| `label_aliases.in_progress: "ai:working"` (with any prefix) | `ai:working` (verbatim — prefix ignored for this label) |

Implementation: `BridgeLabel::full_name(prefix, aliases)` checks `aliases` first; if a name is supplied there, return it as-is, else return `format!("{prefix}:{base}")`.

This is the semantic the `setup-bridge` skill (Phase 5) prompts for: "Enter the full label name you want, including the namespace prefix" — not "Enter the suffix."

Unit test in `labels::tests`:

```rust
#[test]
fn alias_supplies_full_name_verbatim() {
    let aliases = LabelAliases { in_progress: Some("ai:working".to_owned()), .. };
    assert_eq!(BridgeLabel::InProgress.full_name("sinfonia", &aliases), "ai:working");
    // Without alias, prefix applies:
    let no_aliases = LabelAliases::default();
    assert_eq!(BridgeLabel::InProgress.full_name("sinfonia", &no_aliases), "sinfonia:in-progress");
}
```

---

## 8. GitHub authentication

Code in `crates/sinfonia-bridge/src/github/auth.rs`.

### PAT mode

```rust
let token = resolve_env(&cfg.github.pat)?;     // env-var substitution
let octocrab = Octocrab::builder().personal_token(token).build()?;
```

Required PAT scopes: `repo` (full), `read:org` (only if multi-repo PR resolution is desired). Documented in `docs/v0.3-plan/05-skills-cli.md` so the `setup-bridge` skill can tell the user up front.

### App mode

```rust
let app_id = parse_u64(&cfg.github.app_id)?;
let private_key = load_private_key(&cfg.github.private_key)?;   // accepts inline PEM or @path
let octocrab = Octocrab::builder()
    .app(AppAuth::new(app_id, private_key))
    .build()?;
```

The bridge stores per-installation `octocrab` instances in a `RwLock<HashMap<InstallationId, Octocrab>>` so each webhook event uses the right scoped client.

### Self-test (`sinfonia-bridge --self-test`)

Implemented in `selftest.rs`. Runs each check serially and reports `PASS` / `FAIL` per line:

```
PASS  config: BRIDGE.md parsed
PASS  github: authenticated as octocat (PAT mode)
PASS  github: webhook endpoint reachable at https://...
PASS  tracker: linear project 'my-awesome-project-abc...' accessible
PASS  custom fields: sinfonia_bridge_state_v1 comment marker reserved
FAIL  webhook reachability: GET https://bridge.example.com/health returned 404
```

Exit code = number of failed checks. Used by Phase 5's `setup-bridge` skill as the gate before declaring the install complete.

---

## 9. Test plan

### 9.1 Unit tests (`crates/sinfonia-bridge/src/.../tests`)

| Module | What it covers |
|---|---|
| `config::tests` | Schema validation rules from §3 (one test per rule). |
| `feedback::categorize::tests` | Priority ordering, multi-match resolution, empty `failed_checks`, `failure_categories` absent. |
| `feedback::attempts::tests` | Increment from 0, increment with override, cap-hit detection. |
| `labels::tests` | Label name composition (prefix + base + alias), `manage_labels: false` short-circuits. |
| `webhook::verify::tests` | HMAC verify happy path, wrong-secret rejection, missing-header rejection. |
| `storage::tests` | Idempotency key insert/select, PR-ticket-map upsert. |

### 9.2 Integration tests (`tests/bridge_e2e.rs`)

Run the full daemon under `tokio::test`, with:

- A `wiremock`-backed fake GitHub API mounted on a random port. Octocrab gets its base URL pointed at the mock.
- A fake Linear API likewise on `wiremock`.
- A fake webhook sender that delivers signed payloads to the bridge's `/bridge/webhook`.

Scenarios:

1. **Green PR** → no state transition, `awaiting-review` label applied.
2. **One red CI run** → counter goes 0 → 1, ticket transitions to `Needs Fixes`, `needs-fixes` label applied, PR comment posted.
3. **Three red CI runs** (mix of lint and e2e) with category routing configured → counter goes 0 → 1 → 2 → 3, each transition lands in the correct category state, correct `sinfonia:failure:lint` vs `sinfonia:failure:e2e` labels.
4. **Cap hit** (`max_attempts: 3`, fourth red run) → ticket transitions to `blocked_state`, `cap-hit` label applied, counter NOT incremented past cap.
5. **Webhook redelivery** → second delivery with same ID is a no-op.
6. **Webhook signature failure** → 401 response, no state changes.
7. **PR without ticket link** → 200 response, no state changes (logged at debug).
8. **GitHub App auth** mode → same scenario as (2) with App credentials instead of PAT.
9. **`manage_labels: false`** → state transitions still happen, but no labels touched.

### 9.3 Manual verification

End-to-end against a real Linear project + a sandbox GitHub repo. Documented in `docs/v0.3-plan/01-bridge-mvp-VERIFY.md` (added in the actual implementation pass).

### 9.4 Exit criteria (per proposal)

> A ticket that fails CI 3 times (mix of lint and e2e failures) correctly increments the counter, routes to the right category-specific state each time, applies the correct PR labels, and ends up in the blocked state on the 4th failure. Validated with both PAT and App auth.

Test 3 + test 4 in §9.2 are this, with `wiremock` standing in for live services. Manual verification reproduces it on real services before the phase is marked complete.

---

## 10. Dependencies (new Cargo deps)

Added to `crates/sinfonia-bridge/Cargo.toml`:

```toml
[dependencies]
sinfonia-tracker = { path = "../sinfonia-tracker" }

# Already in the workspace, listed for inheritance:
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
reqwest = { workspace = true }
axum = { workspace = true }
clap = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
async-trait = { workspace = true }
regex = { workspace = true }
chrono = { workspace = true }
url = { workspace = true }
shellexpand = { workspace = true }

# New for the bridge:
octocrab = "0.39"
rusqlite = { version = "0.31", features = ["bundled"] }
subtle = "2.5"
hmac = "0.12"
sha2 = "0.10"
liquid = { workspace = true }
rust_decimal = "1.35"           # only needed for Phase 3 cost-table, but cheaper to land now
jsonwebtoken = "9"              # required by octocrab App auth; verify if octocrab re-exports
```

The workspace `Cargo.toml` declares all shared deps under `[workspace.dependencies]` so versions stay in lockstep.

---

## 11. Open questions

1. **Tracker crate name.** `sinfonia-tracker` reads naturally but takes the namespace. Alternative: `sinfonia-tracker-core` to leave room for `sinfonia-tracker-linear` / `-jira` split crates later. Phase 1 default: `sinfonia-tracker`. Revisit at Phase 4 if Jira's footprint argues otherwise.

2. **Front-matter parser sharing.** The Markdown+YAML splitter is small. Options: (a) move it into `sinfonia-tracker` (semantically wrong but convenient), (b) make a `sinfonia-frontmatter` micro-crate, (c) copy-paste 30 lines. Lean toward (b); cheap to land.

3. **`octocrab` version pin.** `0.39` is current at time of writing. The plan should be verified against `crates.io` at implementation time and updated if a breaking 0.40 has shipped.

4. **In-memory mapping cache vs always-SQLite.** Resolved: **SQLite only**, no in-memory layer. Caching adds invalidation complexity for negligible latency wins at v0.3 traffic volumes (the bridge handles tens of webhook events per minute at worst). §5.5 reflects this. Revisit only if `bridge.ci_result` span latency is observably an issue.

5. **Reload of `BRIDGE.md`.** Not implemented in Phase 1 (process restart only). Document this explicitly in `docs/SKILLS.md` so the `setup-bridge` skill doesn't suggest hot-reload.

6. **Failure-log fetch size.** `failure_log_excerpt` defaults to the last 50 lines of the most-failed check's logs. Configurable? Not for Phase 1; revisit if users complain.

---

## 12. Phase 1 deliverable checklist

**Workspace + tracker crate:**

- [ ] Workspace conversion (move existing code into `crates/sinfonia/`, extract `sinfonia-tracker`).
- [ ] `scripts/verify-workspace-move.sh` per §2, run in CI on the workspace-move commit.
- [ ] `crates/sinfonia-tracker` exporting the extended `IssueTracker` trait and a Linear implementation with the comment-marker custom-field path.

**Custom-field-to-template plumbing (§4.2 — blocks the loop end-to-end):**

- [ ] `Issue.fields: HashMap<String, CustomFieldValue>` added to `crates/sinfonia-tracker/src/types.rs`.
- [ ] `LinearTracker::fetch_candidate_issues` and `fetch_issues_by_states` parse the `sinfonia_bridge_state_v1` marker comment and populate `Issue.fields`.
- [ ] `crates/sinfonia/src/template.rs` Liquid scope exposes `issue.fields` (via `CustomFieldValue: Serialize` flattening to primitives).
- [ ] `template.rs::tests` round-trip test: `{{ issue.fields.sinfonia_last_ci_failure }}` populated case + `| default:` fallback case.

**Bridge binary:**

- [ ] `crates/sinfonia-bridge` binary + library, with all modules listed in §2.
- [ ] `BRIDGE.example.md` at the repo root, mirroring `WORKFLOW.example.md` style.
- [ ] Label aliases verbatim semantics implemented + unit test per §7.
- [ ] Unit tests per §9.1.
- [ ] Integration test suite per §9.2 using `wiremock` for both GitHub and Linear fakes.
- [ ] `sinfonia-bridge --self-test` command with the output format from §8.

**Documentation:**

- [ ] `docs/SPEC.md` §11.6 — recommended bridge extension contract (Phase 7 finalizes this; a draft lands with Phase 1).
- [ ] CHANGELOG entry for v0.3.0-alpha.1 noting the workspace conversion and the bridge MVP.
- [ ] README "What's new in v0.3" paragraph (Phase 7 expands this; Phase 1 lands the stub).

When the box for every item is checked, Phase 1 is shippable on its own — even before Phases 2-7 land.
