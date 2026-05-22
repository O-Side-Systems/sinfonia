-- Sinfonia v0.3 — reference Postgres schema for the OTel Collector
-- → Postgres deployment (plan 03-telemetry-budget.md §8).
--
-- The Collector exporter (see `otel-collector-config.yaml`) routes:
--   - `runner.session` spans   → `sessions` table
--   - `bridge.ci_result` spans → `attempts` table
--   - everything else          → `events` table (JSONB catch-all)
--
-- Both binaries set `service.namespace = tenant_id` on the OTel
-- Resource so the routing-processor can split per-tenant exporters
-- WITHOUT touching emission code (plan §3.3). This schema lives in a
-- single namespace; multi-tenant deployments either run one DB per
-- tenant or partition with a Collector `routing_processor` before
-- this DB.

CREATE TABLE IF NOT EXISTS sessions (
    id                UUID PRIMARY KEY,
    tenant_id         TEXT        NOT NULL,
    issue_id          TEXT        NOT NULL,
    issue_ident       TEXT        NOT NULL,
    state             TEXT        NOT NULL,
    provider          TEXT        NOT NULL,
    model             TEXT        NOT NULL,
    turn_count        INTEGER     NOT NULL,
    total_tokens      BIGINT      NOT NULL,
    prompt_tokens     BIGINT      NOT NULL,
    completion_tokens BIGINT      NOT NULL,
    exit_reason       TEXT        NOT NULL,
    duration_ms       BIGINT      NOT NULL,
    started_at        TIMESTAMPTZ NOT NULL,
    ended_at          TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_tenant_started_idx
    ON sessions (tenant_id, started_at);
CREATE INDEX IF NOT EXISTS sessions_issue_started_idx
    ON sessions (issue_id, started_at);

CREATE TABLE IF NOT EXISTS attempts (
    id               UUID PRIMARY KEY,
    tenant_id        TEXT        NOT NULL,
    ticket_id        TEXT        NOT NULL,
    pr_number        INTEGER,
    repo             TEXT        NOT NULL,
    outcome          TEXT        NOT NULL,         -- green / red / cap_hit / pending / no_mapped_pr
    failure_category TEXT,
    attempt_number   INTEGER     NOT NULL,
    recorded_at      TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS attempts_tenant_recorded_idx
    ON attempts (tenant_id, recorded_at);
CREATE INDEX IF NOT EXISTS attempts_ticket_recorded_idx
    ON attempts (ticket_id, recorded_at);

-- Catch-all for spans not yet table-ized (workspace.hook, tracker.fetch,
-- bridge.cost_update, bridge.state_transition, etc.). The same shape
-- backs the §8.2 dashboard queries — query `attributes->>'…'` for any
-- attribute on any span.
CREATE TABLE IF NOT EXISTS events (
    id          UUID PRIMARY KEY,
    tenant_id   TEXT        NOT NULL,
    span_name   TEXT        NOT NULL,
    attributes  JSONB       NOT NULL,
    started_at  TIMESTAMPTZ NOT NULL,
    duration_ms BIGINT
);
CREATE INDEX IF NOT EXISTS events_tenant_span_started_idx
    ON events (tenant_id, span_name, started_at);
CREATE INDEX IF NOT EXISTS events_attributes_gin
    ON events USING gin (attributes);
