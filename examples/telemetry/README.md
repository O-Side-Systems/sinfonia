# Reference OpenTelemetry → Postgres deployment

Sinfonia v0.3 ships an opt-in OpenTelemetry emission layer (`docs/SPEC.md`
§18.2) plus a typed Sinfonia↔bridge event channel for the cost / budget
pipeline (§11.6.11). This directory holds a reference Collector + Postgres
configuration — the v0.3 "starter kit" for operators who want to answer
the three reference dashboard questions (tenant monthly cost, first-try
rate, top-budget tickets) without standing up Honeycomb / Datadog.

## Files

| File | Purpose |
|---|---|
| `otel-collector-config.yaml` | Collector receivers + routing-by-tenant + Postgres exporters. Starter only — adjust the DSNs for your deployment. |
| `postgres-schema.sql` | The three tables the queries below read from: `sessions`, `attempts`, `events`. |
| `queries/*.sql` | The §8.2 dashboard queries (tenant monthly cost, first-try rate, budget-heavy tickets). |

## Wiring it up

1. Apply the schema:

   ```bash
   psql sinfonia -f postgres-schema.sql
   ```

2. Start a Collector with this config:

   ```bash
   otelcol-contrib --config otel-collector-config.yaml
   ```

3. Point both binaries at the Collector:

   ```bash
   export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
   sinfonia
   sinfonia-bridge
   ```

   Or set it in `WORKFLOW.md` / `BRIDGE.md` under the `telemetry:` block.

## Span / attribute reference

Sinfonia emits (plan §4):

| Span | Attributes |
|---|---|
| `orchestrator.tick` | tenant_id, candidates_count, dispatched_count, tick_duration_ms |
| `orchestrator.dispatch` | tenant_id, issue_id, issue_identifier, state, provider, model |
| `runner.session` | tenant_id, issue_id, issue_identifier, state, provider, model, turn_count, total_tokens, prompt_tokens, completion_tokens, exit_reason, duration_ms |
| `runner.turn` | tenant_id, turn_index, tokens, duration_ms |
| `workspace.hook` | hook_name, duration_ms, exit_code |
| `tracker.fetch` | tracker_kind, request_kind, result_count, duration_ms |

The bridge emits (plan §5):

| Span | Attributes |
|---|---|
| `bridge.webhook` | tenant_id, event_type, delivery_id, repo, duration_ms |
| `bridge.ci_result` | tenant_id, event_type, repo, outcome, attempt_count, failure_category |
| `bridge.state_transition` | ticket_id, to_state, reason (ci_failure / cap_hit / budget_exceeded), attempt_count |
| `bridge.cap_hit` | ticket_id, cap_kind (attempts / tokens / cost), final_attempt_count |
| `bridge.cost_update` | ticket_id, tokens_delta, tokens_total, cost_delta_usd, cost_total_usd |
| `bridge.events_receive` | tenant_id, event_type |

All resource-level attributes (per plan §3.3):
- `service.name` — `sinfonia` or `sinfonia-bridge`
- `service.namespace` — the resolved `tenant_id` (the routing-processor key)
- `service.version` — `CARGO_PKG_VERSION`
- `service.instance.id` — fresh UUID per process start

## Multi-tenant routing

The Collector config includes a `routing/by_tenant` processor keyed on
`service.namespace`. Add one entry per tenant; each gets its own
Postgres exporter and (in production) its own database. The
single-tenant default exporter catches anything not in the lookup
table.

## What's NOT in v0.3

The 9-instrument OTel metrics layer (`sinfonia.agent.tokens_total`,
`bridge.cost_per_ticket_usd`, etc.) is deferred to a follow-up release.
The dashboard SQL above reads from span attributes via the `events`
table, not from OTel metric points, so the reference dashboards work
span-derived as-is.
