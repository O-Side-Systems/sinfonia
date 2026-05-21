# Phase 3 — Telemetry + budget enforcement

**Target:** v0.3.0
**Scope:** OpenTelemetry emission from Sinfonia and the bridge, tenant-tagging from day one, and budget caps (token + cost) enforced by the bridge — fed by a small typed HTTP event stream from Sinfonia, NOT by a bridge-hosted OTLP receiver.
**Estimated size:** ~1 100 LOC across Sinfonia + bridge + a reference Collector → Postgres setup, + ~300 LOC of tests + ~500 LOC of docs. (Up from the original 900 to absorb the typed-event HTTP path that replaces the dropped OTLP receiver — see §7.2.)
**Depends on:** Phase 1 (bridge exists, custom-fields trait exists, `Issue.fields` template plumbing exists), Phase 2 (so the OpenCode backend emits the same spans).
**Unblocks:** Phase 5's `setup-telemetry` skill needs the OTel surface to be stable; Phase 6's `docker-compose` reference includes the Collector + Postgres pieces.

---

## 1. What telemetry answers that logs don't

Sinfonia already emits structured `tracing` logs (see `src/main.rs:113-123` for the subscriber setup). Those answer "what did this one agent do?" The proposal adds a separate event stream that answers "is the system working?" — a different consumer with different retention needs.

Concrete questions Phase 3 has to make answerable:

- "What did this tenant cost us last month?"
- "How often does the agent actually close a ticket without human intervention?"
- "Which tickets are eating disproportionate budget?"
- "Is Haiku triage actually cheap enough? Is Opus implementation actually higher-success?"

None of these are answerable today. Each one drives a span or metric below.

---

## 2. Two consumer roles, one emission

Same `tracing` macros stay where they are. We add `tracing-opentelemetry` so each `info!` / `instrument` ALSO emits as an OTel span when the OTLP exporter is configured. The pretty/json stdout subscriber stays unchanged for log consumers.

```rust
// updated init_logging in crates/sinfonia/src/main.rs and crates/sinfonia-bridge/src/main.rs
fn init_observability(format: &str, otel: &TelemetryConfig) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let stdout_layer = if format == "json" {
        tracing_subscriber::fmt::layer().json().boxed()
    } else {
        tracing_subscriber::fmt::layer().pretty().boxed()
    };

    let otel_layer = otel.exporter().map(|exporter| {
        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(otel.resource())
            .build();
        let tracer = provider.tracer("sinfonia");
        tracing_opentelemetry::layer().with_tracer(tracer)
    });

    let subscriber = tracing_subscriber::registry().with(env_filter).with(stdout_layer);
    match otel_layer {
        Some(l) => subscriber.with(l).init(),
        None => subscriber.init(),
    }
}
```

When `OTEL_EXPORTER_OTLP_ENDPOINT` is unset and no `telemetry:` block is configured in WORKFLOW.md / BRIDGE.md, the OTel layer is `None` and we're back to today's behavior. The feature is **opt-in by configuration**; the binary still works without it.

---

## 3. Tenancy — the data model

Every span and every metric carries a `tenant_id` attribute. This is non-negotiable per the proposal §Tenancy. The mechanism:

### 3.1 Configuration surface

`WORKFLOW.md`:

```yaml
telemetry:
  otlp_endpoint: $OTEL_EXPORTER_OTLP_ENDPOINT
  service_name: sinfonia                      # defaults to "sinfonia"
  tenant_id: kyros-web-app                    # optional; see precedence below
  headers:                                    # for SaaS sinks (Honeycomb x-honeycomb-team, etc.)
    x-honeycomb-team: $HONEYCOMB_API_KEY
  # Shared HMAC secret for the typed Sinfonia↔bridge event channel (see §7.2).
  # Sinfonia signs every outbound POST with HMAC-SHA256(body, secret). Must
  # match the same key in BRIDGE.md's telemetry.sinfonia_events_secret.
  # Unset = Sinfonia accepts no subscribers (the bridge can't consume events).
  sinfonia_events_secret: $SINFONIA_EVENTS_SECRET
```

`BRIDGE.md` has the same `telemetry:` block, with the same `sinfonia_events_secret` key. The two values must match; if they don't, the bridge rejects every event with HTTP 401 and Sinfonia logs a `WARN` on retry exhaustion.

### 3.2 Tenant ID resolution precedence

1. `telemetry.tenant_id` from the config file (literal or `$ENV_VAR`).
2. `SINFONIA_TENANT_ID` env var.
3. The literal string `default`.

Documented in code as a single constant + resolver, used by both binaries.

### 3.3 Resource-level attributes

In addition to the per-span `tenant_id`, the OTel `Resource` carries:

| Attribute | Value | Why |
|---|---|---|
| `service.name` | `sinfonia` or `sinfonia-bridge` | Standard OTel; lets collectors route by component. |
| `service.namespace` | `tenant_id` | Lets a Collector config split per-tenant exporters cleanly (`routing_processor`). |
| `service.version` | `env!("CARGO_PKG_VERSION")` | Standard OTel; for dashboards filtering by release. |
| `service.instance.id` | `Uuid::new_v4()` per process start | Standard OTel. |

Resource-level `service.namespace = tenant_id` means an OTel Collector can use the [routing processor](https://github.com/open-telemetry/opentelemetry-collector-contrib/tree/main/processor/routingprocessor) to send tenant A's data to one Postgres schema and tenant B's data to another, without ever touching span-emission code.

---

## 4. Sinfonia spans

All carry `tenant_id` (omitted from the table for brevity).

| Span | Attributes | Where emitted |
|---|---|---|
| `orchestrator.tick` | `candidates_count`, `dispatched_count`, `tick_duration_ms` | wraps `Orchestrator::tick()` body in `crates/sinfonia/src/orchestrator/mod.rs:173+` |
| `orchestrator.dispatch` | `issue_id`, `issue_identifier`, `state`, `provider`, `model` | `crates/sinfonia/src/orchestrator/dispatch.rs` |
| `runner.session` | `issue_id`, `provider`, `model`, `turn_count`, `total_tokens`, `prompt_tokens`, `completion_tokens`, `exit_reason`, `duration_ms` | `crates/sinfonia/src/orchestrator/runner.rs` |
| `runner.turn` | `turn_index`, `tool_calls_count`, `tokens`, `duration_ms` | `crates/sinfonia/src/agent/turn.rs` |
| `workspace.hook` | `hook_name`, `duration_ms`, `exit_code` | `crates/sinfonia/src/workspace/hooks.rs:32+` |
| `tracker.fetch` | `tracker_kind`, `request_kind`, `duration_ms`, `result_count` | `crates/sinfonia-tracker/src/{linear,jira}.rs` |

Each is created with `tracing::info_span!()` and attached attributes via `.record()` / structured fields, so the same span is visible to both the stdout subscriber and the OTel exporter.

### 4.1 Concretely how a span gets emitted

```rust
// inside Orchestrator::tick()
let span = tracing::info_span!(
    "orchestrator.tick",
    tenant_id = %self.tenant_id.as_str(),
    candidates_count = tracing::field::Empty,
    dispatched_count = tracing::field::Empty,
    tick_duration_ms = tracing::field::Empty,
);
let _enter = span.enter();
let started = std::time::Instant::now();
// ... existing tick logic ...
span.record("candidates_count", candidates.len() as i64);
span.record("dispatched_count", dispatched as i64);
span.record("tick_duration_ms", started.elapsed().as_millis() as i64);
```

The `Empty` field placeholders are required so `tracing-opentelemetry` emits attributes with proper types when they're later set.

---

## 5. Bridge spans

| Span | Attributes |
|---|---|
| `bridge.webhook` | `event_type`, `delivery_id`, `repo`, `duration_ms` |
| `bridge.ci_result` | `pr_number`, `ticket_id`, `outcome` (green/red), `attempt_count`, `failure_category` |
| `bridge.state_transition` | `ticket_id`, `from_state`, `to_state`, `reason` (ci_failure / cap_hit / budget_exceeded / manual) |
| `bridge.cap_hit` | `ticket_id`, `final_attempt_count`, `cap_kind` (attempts / tokens / cost) |
| `bridge.cost_update` | `ticket_id`, `tokens_delta`, `cost_delta_usd`, `tokens_total`, `cost_total_usd` |
| `bridge.pr_label` | `pr_number`, `label`, `action` (add / remove) |

All carry `tenant_id`. Emission sites map 1-to-1 with the modules planned in `01-bridge-mvp.md` §2 — for example, every state transition in `feedback/transition.rs` wraps the call in `bridge.state_transition`.

---

## 6. Metrics

OTel metrics, separate from the spans. Each carries `tenant_id` as a label.

| Metric | Type | Labels | Where emitted |
|---|---|---|---|
| `sinfonia.agent.tokens_total` | Counter | `provider`, `model`, `state`, `issue_id` | `runner.rs` on session end |
| `sinfonia.agent.session_duration_ms` | Histogram | `provider`, `model`, `state` | `runner.rs` on session end |
| `sinfonia.agent.exit_reason` | Counter | `provider`, `reason` (completed / turn_limit / stall / error) | `runner.rs` on session end |
| `sinfonia.orchestrator.tick_duration_ms` | Histogram | — | `orchestrator.tick` span exit |
| `sinfonia.orchestrator.concurrent_runs` | Gauge | `state` | `orchestrator/state.rs` (claim / release) |
| `bridge.ci_outcome` | Counter | `outcome`, `project`, `failure_category` | `feedback/transition.rs` |
| `bridge.attempts_to_close` | Histogram | `project` (fires on ticket close) | watcher on ticket terminal-state transition |
| `bridge.cap_hits` | Counter | `project`, `cap_kind` | `feedback/transition.rs` |
| `bridge.cost_per_ticket_usd` | Histogram | `project` (fires on ticket close) | same place as `attempts_to_close` |

`bridge.attempts_to_close` and `bridge.cost_per_ticket_usd` need a "ticket reached a terminal state" detection path that didn't exist in Phase 1. Phase 3 reuses the GitHub webhook events the bridge already receives — specifically `pull_request closed` with `merged: true`, which the bridge correlates to a tracked ticket via the existing PR ↔ ticket map. When a merge happens, the bridge transitions to "this ticket is done" and emits the two metrics. No tracker polling loop is added; the bridge stays webhook-driven for GitHub and pull-driven for tracker writes only.

This intentionally skips the case where a ticket reaches a terminal state without a merged PR (e.g., a human manually marked it Done). Those tickets won't generate the close-time histogram entries. Acceptable for v0.3 — the metrics are about "what does the closing-via-our-pipeline path look like," not "what does every ticket close look like." Documented in `docs/SPEC.md` §11.6.

---

## 7. Cost table and budget enforcement

Budget caps are why Phase 3 is the right time to do telemetry — they're the consumer that proves the telemetry stream is correct.

### 7.1 Cost table

Ships in the repo at `config/cost_table.yaml`:

```yaml
# Per-provider, per-model pricing in USD per 1M tokens.
# Source: provider public pricing pages. Last verified 2026-05-21.
#
# Override with bridge.cost_table_path if a provider's pricing changes
# between Sinfonia releases.
providers:
  anthropic:
    claude-haiku-4-5-20251001:
      input_per_million_usd: 1.00
      output_per_million_usd: 5.00
    claude-sonnet-4-6:
      input_per_million_usd: 3.00
      output_per_million_usd: 15.00
    claude-opus-4-7:
      input_per_million_usd: 15.00
      output_per_million_usd: 75.00
  openai:
    gpt-4o:
      input_per_million_usd: 2.50
      output_per_million_usd: 10.00
  # ...
```

Schema struct in `crates/sinfonia-bridge/src/feedback/cost.rs`:

```rust
pub struct CostTable {
    providers: HashMap<String, HashMap<String, ModelCost>>,
    verified_at: chrono::NaiveDate,
}

pub struct ModelCost {
    input_per_million_usd: rust_decimal::Decimal,
    output_per_million_usd: rust_decimal::Decimal,
}
```

Loaded once at bridge startup. Logs a `WARN` if `verified_at` is more than 90 days old.

### 7.2 The bridge subscribes to a typed Sinfonia event stream

**Design change from the original proposal:** the original plan had the bridge run an OTLP/HTTP receiver to subscribe to Sinfonia's span stream. That path requires implementing the OTLP server protocol — `opentelemetry-otlp` is a *client* crate, and standing up a server from the protobuf schema is hundreds of LOC of plumbing for one consumer. We replace it with a small typed HTTP event channel.

Sinfonia already exposes an axum HTTP API (today's `/api/v1/state`, `/api/v1/refresh`, `/api/v1/:identifier`). Phase 3 adds one new endpoint and one new background emitter:

```
POST  /api/v1/events/subscribers     # bridge registers itself with a callback URL
GET   /api/v1/events/recent          # diagnostic; recent events for debugging
```

When a `runner.session` ends in Sinfonia, the runner POSTs a typed event to every registered subscriber (today: just the bridge) with retry-with-backoff and a small bounded in-memory replay buffer in case the bridge is briefly down:

```jsonc
// Body of POST <subscriber.callback_url>
{
  "type": "runner.session.completed",
  "version": 1,
  "tenant_id": "kyros-web-app",
  "issue_id": "lin_iss_abc123",
  "issue_identifier": "ENG-42",
  "state": "Needs Fixes - E2E",
  "provider": "claude_code",
  "model": "claude-opus-4-7",
  "turn_count": 8,
  "prompt_tokens": 318404,
  "completion_tokens": 12892,
  "duration_ms": 542113,
  "exit_reason": "completed",
  "occurred_at": "2026-05-21T17:42:11Z"
}
```

`BRIDGE.md` config:

```yaml
telemetry:
  otlp_endpoint: $OTEL_EXPORTER_OTLP_ENDPOINT       # where the bridge SENDS its own spans
  sinfonia_event_subscribe_url: http://sinfonia:8080/api/v1/events/subscribers
  sinfonia_event_callback_url:  http://sinfonia-bridge:8081/api/v1/sinfonia-events
  sinfonia_events_secret: $SINFONIA_EVENTS_SECRET   # shared HMAC; must match WORKFLOW.md
  service_name: sinfonia-bridge
```

The `sinfonia_events_secret` is the symmetric key Sinfonia uses to sign each outbound POST and the bridge uses to verify it. Both sides MUST be configured with the same value (env var or literal). The `setup-bridge` skill (Phase 5) generates a random 32-byte value when neither side has one set, and writes it to both files plus a `.env` entry.

The bridge handler at `POST /api/v1/sinfonia-events`:

1. Verifies the request came from a known Sinfonia (shared-secret HMAC header, mirroring the GitHub-webhook verification pattern from Phase 1 §5.1).
2. For `runner.session.completed` events: feeds into the cost pipeline (§7.3).
3. Other event types are ignored for forward-compatibility.

**Why this beats OTLP-as-receiver:** typed, versioned JSON shape that we control. Trivial to mock in tests (it's just an HTTP POST). One bridge dep removed (`opentelemetry-otlp` server-side machinery wasn't going to materialize anyway). Sinfonia's OTel emission to the Collector / Honeycomb / Datadog stays exactly as designed — the typed HTTP stream is an ADDITIONAL out-of-band channel for the bridge specifically, not a replacement for OTel.

#### Sinfonia emission — integration with the existing `AgentEvent` channel

`crates/sinfonia/src/agent/events.rs` already defines an `AgentEvent` enum with variants like `SessionStarted`, `TurnCompleted`, `TurnFailed`, etc., dispatched through an `EventSender` channel by the runner. Phase 3 extends rather than parallels this:

1. **Add `AgentEvent::SessionCompleted`** to the enum in `events.rs`, alongside the existing variants. Fields: `thread_id`, `issue_id`, `state`, `provider`, `model`, `turn_count`, `prompt_tokens`, `completion_tokens`, `duration_ms`, `exit_reason`. (`tenant_id` is process-wide and supplied by the subscriber-emitter task, not per-event.)

2. **Emit it from the runner** at the natural hook point — `crates/sinfonia/src/orchestrator/runner.rs:154`, immediately after `agent.stop_session(session).await` and before `run_after_run_best_effort(...)`. The runner already owns all the fields and is the canonical place a session lifecycle ends.

3. **A new subscriber-emitter task** consumes the `EventSender` channel alongside today's HTTP dashboard consumer, filters for `SessionCompleted`, serializes to the typed POST body shown in §7.2, and dispatches to every registered subscriber URL with HMAC signing + retry-with-backoff. The task lives in `crates/sinfonia/src/http/events.rs` (new), owned by the same `Orchestrator` Arc that owns the existing event channel.

Rationale for extending the existing channel rather than building a parallel one: the runner already has exactly one event-emission point and one channel; adding a second emission path doubles the failure surface and creates two places future contributors have to remember to update. The existing channel's `mpsc::UnboundedSender` already gives the subscriber-emitter all the buffering it needs.

What does NOT change: the existing `AgentEvent` consumers (HTTP dashboard via `src/http/mod.rs`) keep working untouched — they just ignore the new variant. The HTTP dashboard's `/api/v1/state` payload doesn't expose `SessionCompleted` (it'd be redundant with the per-session token totals already there).

### 7.3 The cost pipeline + Linear write coalescing

For each `runner.session.completed` event the bridge:

1. Looks up the model in the cost table (`config/cost_table.yaml`).
2. Computes `delta_cost_usd = (input * input_rate + output * output_rate) / 1_000_000`.
3. Adds the deltas to an **in-process per-ticket accumulator** (NOT a tracker write yet) keyed by `issue_id`.
4. Checks the running total against `max_tokens_per_ticket` and `max_cost_per_ticket_usd` (with per-ticket overrides applied).
5. If a cap is crossed: immediately flush the accumulator to the tracker AND transition to `budget_exceeded_state`. This is the only path where the bridge writes per-session to the tracker.
6. If no cap is crossed: schedule (or refresh) a debounce timer that flushes the accumulator to the tracker after **30 s of no further activity for that ticket**, or on ticket close (whichever comes first).

**Why the debounce.** Linear's custom-field-equivalent is a single bot-owned comment. Every "write a custom field" is a GraphQL read-modify-write of that comment. At Sinfonia's traffic rates a busy ticket can produce 3-5 `runner.session.completed` events per minute; writing the comment each time burns Linear API budget on values that nobody will read until the next agent dispatch. Coalescing to a 30 s debounce drops write rate by an order of magnitude with no observable user impact.

The accumulator is in-process state — it does NOT survive bridge restart. On restart the bridge re-reads the last *persisted* totals from the tracker (whatever was on disk at the last flush) as the starting point for new accumulation; any in-flight deltas not yet flushed are lost. The accumulator itself is not reconstructed. This is acceptable: the budget cap is the SLO, not "exact-to-the-cent accounting." The proposal's stated goal is to prevent runaway spend, not to be a billing system.

After flush the bridge emits `bridge.cost_update` with the deltas + new totals.

The implementation lives in `crates/sinfonia-bridge/src/feedback/budget.rs`. Pure-function helpers (`compute_cost`, `cap_status`) are unit-testable independently of the debounce machinery. The debounce itself is a small `tokio::time::Interval`-backed reconciler that runs every 5 s and flushes any ticket-accumulator that's been idle for ≥30 s.

### 7.4 Budget vs attempts — independent caps

Per the proposal:

- `max_attempts` — incremented on each red CI run; cap hits transition to `blocked_state`.
- `max_tokens_per_ticket` — accumulated across all sessions; cap hits transition to `budget_exceeded_state`.
- `max_cost_per_ticket_usd` — accumulated; cap hits transition to `budget_exceeded_state`.

A budget cap firing does NOT increment the attempt counter. They're different cap kinds. This matters because dashboards distinguish "the agent gave up after N attempts" from "we ran out of money before the agent finished."

### 7.5 Per-ticket overrides

Read from custom fields if present:

| Override | Field |
|---|---|
| `sinfonia_max_attempts` | int, overrides `feedback_loop.max_attempts` |
| `sinfonia_max_cost_usd` | decimal, overrides `feedback_loop.max_cost_per_ticket_usd` |

A ticket green-lit for $100 of work sets `sinfonia_max_cost_usd = 100` without raising the team default.

---

## 8. Sink — what does the user point this at?

The proposal calls out three validation targets:

1. **Honeycomb** — set `OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io:443` and `telemetry.headers.x-honeycomb-team=$HONEYCOMB_API_KEY`. Done.
2. **Datadog OTLP intake** — set the agent's OTLP endpoint and the API key header. Done.
3. **Local Collector → Postgres** — the reference deployment. Includes `otel-collector-config.yaml` + a Postgres schema, both produced by Phase 5's `setup-telemetry` skill.

### 8.1 Reference Postgres schema

Ships in `examples/telemetry/postgres-schema.sql`:

```sql
CREATE TABLE sessions (
    id            UUID PRIMARY KEY,
    tenant_id     TEXT NOT NULL,
    issue_id      TEXT NOT NULL,
    issue_ident   TEXT NOT NULL,
    state         TEXT NOT NULL,
    provider      TEXT NOT NULL,
    model         TEXT NOT NULL,
    turn_count    INTEGER NOT NULL,
    total_tokens  BIGINT NOT NULL,
    prompt_tokens BIGINT NOT NULL,
    completion_tokens BIGINT NOT NULL,
    exit_reason   TEXT NOT NULL,
    duration_ms   BIGINT NOT NULL,
    started_at    TIMESTAMPTZ NOT NULL,
    ended_at      TIMESTAMPTZ NOT NULL
);
CREATE INDEX ON sessions (tenant_id, started_at);
CREATE INDEX ON sessions (issue_id, started_at);

CREATE TABLE attempts (
    id              UUID PRIMARY KEY,
    tenant_id       TEXT NOT NULL,
    ticket_id       TEXT NOT NULL,
    pr_number       INTEGER,
    repo            TEXT NOT NULL,
    outcome         TEXT NOT NULL,           -- green / red
    failure_category TEXT,
    attempt_number  INTEGER NOT NULL,
    recorded_at     TIMESTAMPTZ NOT NULL
);

CREATE TABLE events (                        -- catch-all for spans we haven't tableized
    id          UUID PRIMARY KEY,
    tenant_id   TEXT NOT NULL,
    span_name   TEXT NOT NULL,
    attributes  JSONB NOT NULL,
    started_at  TIMESTAMPTZ NOT NULL,
    duration_ms BIGINT
);
CREATE INDEX ON events (tenant_id, span_name, started_at);
```

The Collector config (`examples/telemetry/otel-collector-config.yaml`) routes `runner.session` spans into `sessions`, `bridge.ci_result` spans into `attempts`, and everything else into `events`. Both files ship in-repo and are referenced by the Phase 5 `setup-telemetry` skill and the Phase 6 `docker-compose.yml`.

### 8.2 The sample queries that justify Phase 3

`docs/v0.3-plan/03-telemetry-budget.md` (this doc, when published) closes with:

```sql
-- What did this tenant cost us this month?
SELECT
    SUM((attributes->>'cost_delta_usd')::numeric) AS total_usd
FROM events
WHERE tenant_id = 'kyros-web-app'
  AND span_name = 'bridge.cost_update'
  AND started_at >= date_trunc('month', NOW());

-- How often does the agent close a ticket without human intervention?
-- (proxy: attempts_to_close = 1 and final state is terminal-success)
SELECT
    COUNT(*) FILTER (WHERE attempt_number = 1)::float
  / NULLIF(COUNT(*), 0) AS first_try_rate
FROM attempts
WHERE tenant_id = 'kyros-web-app'
  AND outcome = 'green'
  AND recorded_at >= NOW() - INTERVAL '30 days';

-- Which tickets are eating disproportionate budget?
SELECT
    s.issue_id,
    s.issue_ident,
    SUM((s.prompt_tokens + s.completion_tokens))::bigint AS tokens,
    COUNT(*) AS sessions
FROM sessions s
WHERE s.tenant_id = 'kyros-web-app'
GROUP BY s.issue_id, s.issue_ident
ORDER BY tokens DESC
LIMIT 20;
```

These three queries are also the smoke tests for the Phase 3 integration test — if they don't return sensible data against a populated test DB, something is wrong with the emission pipeline.

---

## 9. Test plan

### 9.1 Unit tests

| Module | What it covers |
|---|---|
| `feedback::cost::tests` | `compute_cost()` happy path, unknown-provider error, missing-model warning + fallback to zero. |
| `feedback::budget::tests` | Cap detection (under / at / over), per-ticket override application, separate cap kinds (attempts vs tokens vs cost). |
| `telemetry::tenant::tests` | Tenant resolution precedence (config > env > "default"). |
| `telemetry::span_attrs::tests` | Helper that constructs attribute maps; numeric vs string types. |

### 9.2 Integration tests

`tests/telemetry_e2e.rs`:

1. Spin up an in-process OTLP receiver (`opentelemetry-otlp` has a test harness; or roll a tiny axum receiver).
2. Run Sinfonia against a mocked tracker + agent for a few ticks.
3. Assert that the captured spans contain the expected attribute shapes.

`tests/budget_e2e.rs`:

1. Wire bridge + a mocked tracker.
2. Feed synthesized `runner.session` spans (via the OTLP intake) totalling more than `max_cost_per_ticket_usd`.
3. Assert the ticket transitions to `budget_exceeded_state` and gets the `sinfonia:budget-exceeded` label.
4. Assert that a subsequent CI failure on the same ticket does NOT transition it back to a needs-fixes state (it's blocked on budget; bridge has to back off).

### 9.3 Manual verification

Per proposal exit criteria:

> The question "how much did this tenant's project cost us this week?" is answerable with a SQL query. A ticket configured with a $5 cap correctly transitions to `budget_exceeded_state` when crossed, independently of attempt count.

Run the local Collector + Postgres stack from §8, complete one ticket cycle, run the SQL query from §8.2. Result captured in `docs/v0.3-plan/03-telemetry-VERIFY.md`.

---

## 10. Dependencies

Added to workspace `Cargo.toml`:

```toml
[workspace.dependencies]
opentelemetry         = "0.24"
opentelemetry_sdk     = { version = "0.24", features = ["rt-tokio"] }
opentelemetry-otlp    = { version = "0.17", features = ["grpc-tonic", "http-proto"] }
tracing-opentelemetry = "0.25"
```

(Versions need verification at implementation time. The OTel Rust crates moved relatively fast through 2025; pick a set of mutually-compatible versions.)

`opentelemetry-otlp` is used **as a client** by both binaries (exports spans to the user-configured OTLP endpoint). Phase 3 does NOT include a server-side OTLP receiver inside the bridge — the bridge consumes a typed JSON event stream from Sinfonia over HTTP instead (see §7.2). Both binaries' HTTP surfaces are served by the existing `axum` dep; no new HTTP framework is introduced.

The new event-subscriber endpoints on the Sinfonia side use the same `axum` patterns as the existing `/api/v1/*` routes — see `src/http/mod.rs:19-27`. Sinfonia's emit path uses `reqwest` (already in the dep tree) with a small retry loop.

---

## 11. Open questions

1. **Bridge OTLP intake — RESOLVED, dropped.** Original design had the bridge running an OTLP receiver. Implementing that from `opentelemetry-otlp` (a client crate) would have eaten the Phase 3 LOC budget on its own. Resolved per the plan-review pass: bridge subscribes to a small typed JSON event stream Sinfonia emits over its existing HTTP surface (§7.2). User-facing OTel emission is unchanged — both binaries still export to a configured OTLP endpoint. The bridge-to-Sinfonia channel is a separate concern, decoupled from OTel.

2. **Token-attribution at the turn level vs session level.** Today `cli.rs` (and presumably the new `opencode.rs`) report a cumulative token total at session end. For per-turn cost histograms, we'd need provider-by-provider parsing of mid-stream JSON events. Default: stick with session-level for v0.3 and revisit if users want per-turn granularity.

3. **Where does the cost-table version live?** Embedded in the binary (`include_str!`) so the binary always has a default, then overridable via `bridge.cost_table_path`. The `setup-telemetry` skill prompts users to verify the default at install time.

4. **Sampling.** Should we sample low-frequency spans (e.g. `orchestrator.tick` once per minute is fine, every tick is overkill)? Default: no sampling in v0.3; rely on the Collector's tail-sampling processor for users who care.

5. **Metric naming convention.** `sinfonia.agent.tokens_total` vs `agent.tokens_total` with `service.name` doing the namespacing. The proposal uses the prefixed form. Stick with it for clarity in raw OTel dashboards.

---

## 12. Phase 3 deliverable checklist

**OTel emission (both binaries):**

- [ ] OTel client crates added to workspace deps.
- [ ] `crates/sinfonia/src/telemetry/` module: `mod.rs` (init, resource), `tenant.rs`, `spans.rs` (constructors with `tracing::field::Empty` placeholders).
- [ ] Same module replicated in `crates/sinfonia-bridge/src/telemetry/`.
- [ ] Span sites added per §4 and §5.
- [ ] Metric sites added per §6.

**Sinfonia → bridge typed event channel (§7.2):**

- [ ] `AgentEvent::SessionCompleted` variant added in `crates/sinfonia/src/agent/events.rs` with the fields listed in §7.2.
- [ ] `crates/sinfonia/src/orchestrator/runner.rs` emits it immediately after the existing `agent.stop_session(session).await` call (today `runner.rs:154`).
- [ ] `crates/sinfonia/src/http/events.rs` (new): subscriber-emitter task consuming the existing `EventSender` channel, filtering for `SessionCompleted`, dispatching HMAC-signed POSTs with retry-with-backoff and a bounded replay buffer.
- [ ] `POST /api/v1/events/subscribers` and `GET /api/v1/events/recent` endpoints on the Sinfonia HTTP surface.
- [ ] `sinfonia_events_secret` plumbed through `ServiceConfig.telemetry` and used by the HMAC signer; startup error when subscribe URL is set but secret is empty.
- [ ] `POST /api/v1/sinfonia-events` endpoint on the bridge, verifying the HMAC with the matching secret from BRIDGE.md.

**Budget + cost pipeline (§7.3):**

- [ ] `crates/sinfonia-bridge/src/feedback/{cost,budget}.rs`.
- [ ] `config/cost_table.yaml` baked into the binary via `include_str!`, with `bridge.cost_table_path` override.
- [ ] In-process per-ticket cost accumulator with 30 s debounce.
- [ ] Cap-crossing path that flushes immediately and transitions to `budget_exceeded_state`.
- [ ] Terminal-state detection via the existing GitHub webhook `pull_request.closed.merged=true` path (§6).

**Reference Collector → Postgres deployment:**

- [ ] `examples/telemetry/postgres-schema.sql` and `otel-collector-config.yaml`.
- [ ] Sample SQL queries from §8.2 in `examples/telemetry/queries/`.

**Tests + verification:**

- [ ] Tests per §9.1 and §9.2.
- [ ] Manual verification recorded in `docs/v0.3-plan/03-telemetry-VERIFY.md`.

**Documentation:**

- [ ] `docs/SPEC.md` §18.2 entry: OTel emission with `tenant_id`.
- [ ] `docs/SPEC.md` §11.6 update: typed Sinfonia→bridge event contract documented as part of the recommended bridge extension.
- [ ] CHANGELOG entry.
- [ ] README section: "Observability."

When all boxes are checked, Phase 3 ships independently of Phases 4-7. Phases 1-3 together get a client engagement off the ground; 4-7 are polish, breadth, and ergonomics.
