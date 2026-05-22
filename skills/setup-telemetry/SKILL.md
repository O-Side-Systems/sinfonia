---
name: setup-telemetry
description: Wire OpenTelemetry into an existing Sinfonia + bridge deployment. Supports Honeycomb, Datadog, Grafana Cloud, or a local Collector → Postgres stack. Appends `telemetry:` blocks to WORKFLOW.md and BRIDGE.md, generates the Collector config when self-hosting, and validates spans land in the sink with a synthetic ticket.
version: 1.0.0
---

# setup-telemetry

Layer OpenTelemetry over an existing Sinfonia + bridge deployment.

## When to use

- Both `WORKFLOW.md` and `BRIDGE.md` exist and validate (via
  `setup-workflow` + `setup-bridge`).
- The operator wants to ship spans to an observability sink for cost
  tracking, dashboard-building, or alerting.

If only one of the two binaries is deployed, the skill still works — just
skip the half that doesn't apply.

## Procedure

### 1. Pick a sink

Ask the operator:

> Where should spans go?

Offer four options:

- **Honeycomb** — managed; small free tier. Best for "I just want to see
  my traces" with minimal infra.
- **Datadog** — managed; if the operator already pays for Datadog APM.
- **Grafana Cloud** — managed; Tempo trace store + Grafana dashboards.
- **Local Collector → Postgres** — self-hosted reference stack. Best for
  "I want full ownership + SQL queries" — matches `examples/telemetry/`.

### 2a. Honeycomb

1. Collect `HONEYCOMB_API_KEY`.
2. Render `templates/telemetry-honeycomb.yml.liquid` into a snippet.
3. Append to both `WORKFLOW.md` and `BRIDGE.md` under their respective
   `telemetry:` blocks.

### 2b. Datadog

1. Collect the Datadog OTLP intake URL (depends on the operator's Datadog
   region — `https://api.datadoghq.com/api/intake/otel/...`) and an API
   key with `traces:write` scope.
2. Render `templates/telemetry-datadog.yml.liquid`.
3. Append to both files.

### 2c. Grafana Cloud

1. Collect the Tempo endpoint and the basic-auth header (Grafana Cloud
   uses the `<instance-id>:<api-key>` base64-encoded form).
2. Render `templates/telemetry-grafana.yml.liquid`.
3. Append to both files.

### 2d. Local Collector + Postgres

The richest setup; mirrors `examples/telemetry/` in the Sinfonia repo.

1. Render `templates/otel-collector-config.yaml.liquid` to
   `./otel-collector-config.yaml`. Copy `examples/telemetry/postgres-schema.sql`
   into the project root and apply it to the operator's database:
   ```bash
   psql "$DATABASE_URL" < postgres-schema.sql
   ```
2. Render `templates/telemetry-local.yml.liquid` (the
   Sinfonia/bridge-side block).
3. Render the Postgres + Collector services into the existing
   `docker-compose.yml` (skill must merge, not overwrite — preserve any
   sinfonia/bridge services from `setup-bridge`).
4. Generate three starter SQL queries (`tenant_monthly_cost.sql`,
   `first_try_rate.sql`, `budget_heavy_tickets.sql`) — copy verbatim from
   `examples/telemetry/queries/`.

### 3. Verify the cost table

Tell the operator:

> The bridge uses `config/cost_table.yaml` (embedded in the binary, but
> overridable via `bridge.cost_table_path`) to compute per-session USD
> costs. Confirm the rates for your providers are current. The bundled
> table is `verified_at: 2026-05-21`; if you're running a newer Claude /
> GPT / Gemini model whose price changed, override the table.

Surface the current per-provider entries and ask the operator to confirm.

### 4. Append `telemetry:` blocks

For each rendered snippet from step 2, append to:
- `WORKFLOW.md` — the daemon-side `telemetry:` block.
- `BRIDGE.md` — the bridge-side `telemetry:` block. The bridge has two
  extra fields (`sinfonia_event_subscribe_url`,
  `sinfonia_event_callback_url`) for the typed Sinfonia↔bridge event
  channel; render them if the operator wants the bridge to subscribe.

### 5. Restart the daemons

```bash
docker compose down && docker compose up -d
```

### 6. Synthetic ticket

Drive a synthetic ticket through the daemon and confirm spans land in the
sink:

1. Create a test issue in the operator's tracker (e.g. `STUB-1`).
2. Watch the orchestrator pick it up. The first span you should see is
   `orchestrator.tick`, followed by `orchestrator.dispatch`,
   `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`.
3. Confirm the `tenant_id` resource attribute is set correctly (the
   Phase 3 routing-processor key).

If no spans appear, common causes:
- `telemetry.otel_enabled` is false or unset.
- The sink endpoint is wrong / blocked by firewall.
- The auth header in `telemetry.headers` is malformed.

## Templates

- `templates/telemetry-honeycomb.yml.liquid`
- `templates/telemetry-datadog.yml.liquid`
- `templates/telemetry-grafana.yml.liquid`
- `templates/telemetry-local.yml.liquid`
- `templates/otel-collector-config.yaml.liquid`

## See also

- `examples/telemetry/` in the Sinfonia repo for the full reference
  Collector + Postgres setup, schema, and three dashboard queries.
- `docs/SPEC.md` §11.6.11 (typed event channel) and §11.6.12 (budget
  enforcement surface) for the wire shapes that spans carry.
- `docs/v0.3-plan/03-telemetry-VERIFY.md` for crate-version pinning + the
  metrics-layer deferral notes.
