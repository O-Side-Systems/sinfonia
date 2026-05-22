# Phase 3 — Telemetry + budget enforcement — VERIFY

This document captures the manual-verification status of Phase 3 and
the deliberate scope deferrals made during implementation. It mirrors
the structure of `02-opencode-VERIFY.md` — a sibling artifact that
holds what `gsd-plan-checker` would have caught after-the-fact.

**Status:** Code complete. Manual end-to-end verification against a
real Collector + Postgres stack + a budget-cap-hit scenario is pending
before `v0.3.0-alpha.x` ships.

---

## 1. Deltas vs. the plan-doc proposed surface

The Phase 2 §5.10 lesson generalizes — anywhere a plan doc references
a vendor / wire-format surface, verify it before writing code. Phase 3
turned up the following deltas:

### 1.1 OTel crate version set

The plan doc (`03-telemetry-budget.md` §10) proposed:

```text
opentelemetry         = "0.24"
opentelemetry_sdk     = "0.24" + rt-tokio
opentelemetry-otlp    = "0.17" + grpc-tonic + http-proto
tracing-opentelemetry = "0.25"
```

Verified against crates.io at impl time (2026-05-21): all four moved
roughly eight minors past the plan-doc numbers. Set in use:

```text
opentelemetry                       = "0.32"  (released 2026-05-08)
opentelemetry_sdk                   = "0.32"  (released 2026-05-08)
opentelemetry-otlp                  = "0.32"  (released 2026-05-08)
opentelemetry-semantic-conventions  = "0.32"  + semconv_experimental
tracing-opentelemetry               = "0.33"  (released 2026-05-18)
```

The `tracing-opentelemetry` minor leads `opentelemetry` by one (the
bridge crate pins `opentelemetry ^0.32` as a dependency). Feature-flag
names the plan doc proposed (`grpc-tonic`, `http-proto`, `rt-tokio`)
held up; only the version numbers needed re-pinning.

### 1.2 SDK API rename

The plan §2 code snippet used:

```rust
opentelemetry_sdk::trace::TracerProvider::builder()
    .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
```

In 0.32 the provider is `SdkTracerProvider` and `with_batch_exporter`
takes the exporter alone — the batch processor implicitly picks the
runtime from the SDK's feature flags. Updated in `telemetry/mod.rs` of
both crates.

### 1.3 Semantic-convention gating

`SERVICE_NAMESPACE` and `SERVICE_INSTANCE_ID` (used for plan §3.3
routing-processor splits) are gated behind the `semconv_experimental`
feature in `opentelemetry-semantic-conventions 0.32`. The crate's
"stable" subset is just `SERVICE_NAME`; the feature flag is a
versioning hedge, not a spec stability claim. Enabled in
`workspace.dependencies` with a comment.

---

## 2. Deliberate scope deferrals

### 2.1 OTel metrics layer (plan §6)

The plan doc lists nine OTel metric instruments. Setting up a
`MeterProvider` + 9 instruments threaded through every emission site
is substantial work whose primary consumer is the §8.2 dashboard
queries — but those queries (verified by reading them: see
`examples/telemetry/queries/*.sql`) read from span attributes via the
`events` table, NOT from OTel metric points. The exit criteria from
plan §9.3 are met span-derived.

**Disposition:** deferred to a Phase 3.1 enhancement. When users want
pure OTel metric points (e.g. for Prometheus scraping), the addition
is additive: the existing spans stay; new metric sites layer on top
without changing emission semantics.

Specific deferred metrics:
- `sinfonia.agent.tokens_total` — derivable from `SUM(total_tokens)` on `runner.session` spans.
- `sinfonia.agent.session_duration_ms` — derivable from `runner.session.duration_ms`.
- `sinfonia.agent.exit_reason` — derivable from `runner.session.exit_reason`.
- `sinfonia.orchestrator.tick_duration_ms` — derivable from `orchestrator.tick.tick_duration_ms`.
- `sinfonia.orchestrator.concurrent_runs` — NOT derivable from spans; this is genuinely
  missing in v0.3 and surfaces as a TODO for the Phase 3.1 patch.
- `bridge.ci_outcome` — derivable from `bridge.ci_result.outcome`.
- `bridge.attempts_to_close` — derivable from `bridge.ci_result` aggregations on close.
- `bridge.cap_hits` — derivable from `bridge.cap_hit` spans.
- `bridge.cost_per_ticket_usd` — derivable from sum-on-close of `bridge.cost_update.cost_total_usd`.

### 2.2 Bridge `pr_label` span

Plan §5 lists `bridge.pr_label` (add / remove). The `labels.rs`
helpers already emit structured `tracing` log lines on each apply /
remove with the same attribute set the span would have carried.
Adding a dedicated span name on top is low-value compared to the
remaining Phase 3 surface. **Disposition:** deferred; existing
tracing log emissions cover the dashboard need.

### 2.3 Concurrent-runs gauge

The `sinfonia.orchestrator.concurrent_runs` gauge from §6 would track
"how many workers are running right now." It's a genuinely
stateful metric (current count, not aggregable from per-event spans).
**Disposition:** v0.3.1 — add the metric AND the corresponding
`MeterProvider` wiring at the same time so the metrics-layer
deferral closes in one patch.

### 2.4 Phase 5 `setup-telemetry` skill templates

The reference Collector config + Postgres schema ship in
`examples/telemetry/` per plan §8. Phase 5's `setup-telemetry` skill
will templatize them (per-deployment endpoints, per-tenant DSNs).
**Disposition:** Phase 5 work — not in scope for Phase 3.

---

## 3. Manual verification — pending

Per plan §9.3, the v0.3 exit criterion is:

> The question "how much did this tenant's project cost us this week?"
> is answerable with a SQL query. A ticket configured with a $5 cap
> correctly transitions to `budget_exceeded_state` when crossed,
> independently of attempt count.

To run before `v0.3.0-alpha.x` tag:

1. **Bring up the reference stack.**
   ```bash
   docker run -d --name postgres -p 5432:5432 \
     -e POSTGRES_USER=otel -e POSTGRES_PASSWORD=otel \
     -e POSTGRES_DB=sinfonia postgres:16
   psql -h localhost -U otel sinfonia -f examples/telemetry/postgres-schema.sql
   otelcol-contrib --config examples/telemetry/otel-collector-config.yaml
   ```

2. **Start both binaries with OTel pointed at the Collector.**
   ```bash
   export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
   sinfonia &
   sinfonia-bridge &
   ```

3. **Drive one ticket cycle.** Open a PR linked to a Linear ticket,
   let CI run, observe the bridge → tracker transition.

4. **Run query 01 from `examples/telemetry/queries/`.** Expect a
   non-zero numeric `total_usd` covering the session's cost.

5. **Configure a $0.01 cap and verify the transition.** Set
   `feedback_loop.max_cost_per_ticket_usd: 0.01` in `BRIDGE.md`,
   drive a new session, observe the ticket transition to
   `feedback_loop.budget_exceeded_state` after the first
   session-completed event.

Record the captured timings, span snapshots, and SQL outputs in
section 4 below. Until that's filled in, the v0.3 release notes
should call this verification path out as "post-merge follow-up."

---

## 4. Captured verification output

(empty — fill in during manual verification)

---

## 5. Known-good integration points

These are not deferrals — they're verified by the unit and existing
P1-H integration suites:

- The HMAC algorithm + header format match across the Sinfonia signer
  (`crates/sinfonia/src/http/events.rs::sign`) and the bridge verifier
  (reuses `crates/sinfonia-bridge/src/webhook/verify.rs::verify_signature`).
  Pinned by the `hmac_signature_format_matches_sinfonia` test in
  `crates/sinfonia-bridge/src/events.rs::tests`.

- `BudgetManager::flush_ticket` writes `cost_consumed_usd` as a
  `CustomFieldValue::String` (per STATUS §5.1, money is NEVER f64 on
  the wire). Pinned by
  `feedback::budget::tests::flush_writes_to_tracker_and_clears_pending`.

- The M-2 cost-cap freshness gate (180 days) is asymmetric — token
  caps still fire when cost caps are blocked. Pinned by
  `feedback::cost::tests::freshness_gate_warn_and_block`.

- The OpenCode `provider/model` model-name format (STATUS §5.11)
  looks up correctly against the cost table — pinned by
  `feedback::cost::tests::lookup_handles_opencode_provider_slash_model`.

- The OTel layer is opt-in. When no `telemetry.otlp_endpoint` is
  configured and no `OTEL_EXPORTER_OTLP_ENDPOINT` env var is set, the
  layer is `None` and both binaries run with stdout-only logging.
  Verified at startup with the
  `telemetry initialized otel_enabled=false` log line.
