# Adopting Sinfonia in an enterprise context

**Audience:** an engineer at a company evaluating Sinfonia for use against
an existing codebase, owned by an existing team, with existing CI on
GitHub, against an existing Linear or Jira project. **Read time:** ~25 min.
**Prerequisites:** the README understood, [`docs/DEPLOYMENT.md`](DEPLOYMENT.md)
skimmed, and a draft answer to "who runs this once it's live?"

This document is the one we'd hand to a security reviewer, a CTO doing
buy / build, or an SRE team about to inherit operational ownership. It is
written for engineers, not executives.

Topics:

- [Security posture summary](#security-posture-summary)
- [Trust boundaries](#trust-boundaries)
- [GitHub App vs PAT](#github-app-vs-pat)
- [Budget controls](#budget-controls)
- [Audit trail](#audit-trail)
- [Failure handling](#failure-handling)
- [Handoff to the operating team](#handoff-to-the-operating-team)
- [Vendor evaluation worksheet](#vendor-evaluation-worksheet)

---

## Security posture summary

Sinfonia is **two processes that hold three classes of credential** and
**write to two external systems**.

| Process | What it holds | What it can do | What it canNOT do |
|---|---|---|---|
| `sinfonia` (daemon) | Tracker API key (read scope); agent backend credentials (env vars or CLI-owned files); HMAC secret for the typed event channel | Poll the tracker; spawn agent subprocesses; bind a loopback HTTP port for the dashboard | Write to the tracker; write to GitHub; reach outside its `workspace.root` |
| `sinfonia-bridge` | Tracker API key (read + write scope); GitHub PAT or App private key; HMAC secret for GitHub webhooks; HMAC secret for the typed event channel | Verify webhooks; transition tracker states; post comments; manage PR labels; mint App access tokens | Spawn agents; reach the agent's host filesystem; bind to a non-configured port |

The agent process is a *child* of the daemon, and it inherits the
daemon's process environment — so any env var the daemon sees, the agent
sees. This includes API keys. Mitigation: run the daemon under a
dedicated OS user, and only put credentials the agent needs in that user's
environment.

### One-paragraph summary for your security team

> Sinfonia is a Rust daemon that polls Linear or Jira every 30 seconds,
> looks for tickets in a configured "active" state, and spawns a
> coding-agent process (Claude Code, Codex CLI, OpenCode CLI, or a raw
> LLM API client) in a per-issue workspace directory. The daemon is
> read-only against the tracker; all tracker writes happen via the
> companion `sinfonia-bridge` process, which is the only thing reading
> GitHub webhooks. The agent operates under the daemon's user
> identity inside its workspace directory; outside the workspace, the
> agent has whatever permission the daemon has, which is whatever
> permission the OS user the daemon runs under has. Both processes
> emit OpenTelemetry traces that double as the audit log.

---

## Trust boundaries

```
┌──────────────────────────────────────────────────────────────────────────┐
│                       Trust boundary diagram                              │
│                                                                           │
│   ┌──────────┐                                                            │
│   │ Tracker  │◀────── read only (API key) ─────┐                          │
│   │ (Linear/ │                                  │                          │
│   │  Jira)   │◀── read + write (API key) ──┐    │                          │
│   └──────────┘                              │    │                          │
│        ▲                                    │    │                          │
│        │                          ┌─────────┴────┴──────────┐              │
│        │                          │       sinfonia-bridge   │              │
│        │                          │  - one OS user         │              │
│        │                          │  - bridge.db (SQLite)   │              │
│        │                          │  - PAT or App key       │              │
│        │                          └─────────┬───────────────┘              │
│        │                                    ▲                              │
│   ┌──────────┐                              │ HMAC-signed typed event      │
│   │  GitHub  │── webhook (HMAC-signed) ─────┘ channel (port 8081)          │
│   │          │◀── reads PR / branch / status ─┐                            │
│   └──────────┘                                 │                            │
│        ▲                                       │                            │
│        │ agent's shell-tool calls              │                            │
│        │ via `gh` (under agent's auth,         │                            │
│        │ NOT the bridge's)                     │                            │
│        │                                       │                            │
│        │                          ┌───────────┴────────────┐               │
│        │                          │         sinfonia       │               │
│        │                          │  - one OS user         │               │
│        │                          │  - workspace.root/     │               │
│        │                          │  - tracker read key    │               │
│        │                          │  - agent backend keys  │               │
│        │                          └──────────┬─────────────┘               │
│        │                                     │ spawn (PID-isolated)        │
│        │                                     ▼                              │
│        │                          ┌────────────────────────┐               │
│        │                          │     agent process      │               │
│        └─────────── shell ──────────│  inside workspace      │               │
│                                    │  dir; full repo perms   │               │
│                                    └────────────────────────┘               │
└──────────────────────────────────────────────────────────────────────────┘
```

Key invariants:

1. The daemon is the **only process that owns a workspace**. Files outside
   `workspace.root` are off-limits by sanitization (SPEC §9).
2. The bridge is the **only writer of `sinfonia_*` custom fields**
   (SPEC §11.6.1).
3. The agent has **whatever perms its OS user has** — most importantly,
   the agent's `shell` tool can run arbitrary commands. Mitigation: run
   the daemon under a dedicated, low-privilege user.
4. The bridge **never sees agent code execution**. It is purely a webhook
   receiver + tracker writer.
5. The bridge **never sees source code**. The daemon never sees GitHub
   credentials. The agent never sees the bridge's GitHub credential.

---

## GitHub App vs PAT

You will pick one of two GitHub auth modes for the bridge. Both are
documented in `BRIDGE.example.md`; this section is the trade-off summary.

### PAT mode

- **What it is.** A Personal Access Token (`ghp_…` or fine-grained
  `github_pat_…`) on a user account. The bridge uses it for every API
  call.
- **Required scopes.** `repo` (full) for label / comment writes,
  `read:org` for cross-repo PR resolution.
- **Pros.** Fastest install — three minutes from token creation to a
  working bridge. Single source of truth for the credential. Works on
  GitHub Enterprise Cloud + Server.
- **Cons.** Tied to a specific user; if that user leaves the org, the
  PAT dies. PAT scopes are coarse — `repo` grants write access to every
  repo the user can see. There's no token rotation built into the
  bridge; rotation = "stop, swap value in BRIDGE.md, start."
- **When to use it.** Evaluation, proof-of-concept, single-team installs,
  and any environment where the bridge's blast radius is acceptable.

### App mode

- **What it is.** A GitHub App with its own private key. The bridge mints
  per-installation access tokens via `POST /app/installations/{id}/access_tokens`
  on demand and caches per-owner `octocrab` instances. Tokens auto-rotate
  every hour.
- **Required permissions** (configured on the App in the GitHub UI):
  `Contents: read`, `Issues: write`, `Pull requests: write`,
  `Metadata: read`, `Checks: read`. Subscribe to events:
  `Pull request`, `Check suite`, `Workflow run`.
- **Pros.** Per-installation scope — each repo / org installs the App
  separately, and the bridge's blast radius is exactly the union of
  installations. Tokens auto-rotate. Survives any single user departure.
  Centralized install management via the App's GitHub page.
- **Cons.** Slower to set up — App registration + private key
  generation + per-repo install. Private key handling needs care (we
  recommend writing it to a file under `0600` perms and referencing it
  with `private_key: "@/etc/sinfonia/bridge-app.pem"`).
- **When to use it.** Production. Multi-org. Anywhere you want a clear
  audit trail of which repo opted in to which automation.

### Migration: PAT → App

You can switch modes at any time:

1. Register the App, generate the private key, install on the relevant
   repos.
2. In `BRIDGE.md`, comment out `github.pat`, set `github.app_id` and
   `github.private_key`.
3. Stop and restart the bridge. The webhook secret stays unchanged; the
   webhook URL doesn't move.

No tracker-side changes are required — the bridge identifies itself to
the tracker via its tracker API key, which is independent of the GitHub
credential mode.

---

## Budget controls

Three layers, used in combination.

### 1. Concurrency caps

In `WORKFLOW.md`:

```yaml
agent:
  max_concurrent_agents: 5
  max_concurrent_agents_by_state:
    "In Progress": 2     # heavy implementation lane
    "Triage": 5          # cheap raw-LLM lane
```

This bounds the daemon's parallelism. A single Claude Code session can
consume hundreds of thousands of tokens per hour at maximum effort; a
cap of 5 means at most 5 of those running at once.

### 2. Per-attempt turn caps

In `WORKFLOW.md`:

```yaml
agent:
  max_turns: 8
```

Each agent run gets at most `max_turns` interaction cycles. After that,
the runner exits cleanly and the orchestrator schedules a continuation
on the next poll. The continuation picks up where it left off (the
workspace persists across attempts). Effect: hard ceiling on per-attempt
cost, with naturally-bounded retries.

### 3. Per-ticket token + cost caps (bridge)

In `BRIDGE.md`:

```yaml
feedback_loop:
  max_attempts: 5
  max_tokens_per_ticket: 1_500_000
  max_cost_per_ticket_usd: "25.00"
  budget_exceeded_state: "Blocked - Budget Cap"
```

This is the *hard* ceiling per ticket across all attempts. When the
bridge sees a `runner.session.completed` event push a ticket's
accumulator past either cap, it:

1. Flushes the accumulator to the tracker
   (`sinfonia_tokens_consumed`, `sinfonia_cost_consumed_usd`,
   `sinfonia_budget_exhausted_at`).
2. Transitions the ticket to `feedback_loop.budget_exceeded_state`.
3. Applies the `<prefix>:budget-exceeded` PR label.

The daemon's next poll sees the new state, which (by your state-machine
config) is NOT in `active_states`, so the agent stops. A human can
reset the override via the `sinfonia_max_cost_usd` custom field and
manually transition the ticket back.

### Per-ticket overrides

Two custom fields let an operator widen the budget on a per-ticket
basis:

- `sinfonia_max_attempts` (Number) — overrides
  `feedback_loop.max_attempts`. Useful for a flaky but important ticket.
- `sinfonia_max_cost_usd` (String, decimal) — overrides
  `feedback_loop.max_cost_per_ticket_usd`. Useful for a research spike
  the team has decided to fund beyond the default.

Both fields are READ by the bridge on every cap-check. Writes to them
are operator-driven, NOT bridge-driven, so the override survives a
bridge restart.

### Cost-table freshness

The bridge computes costs from a versioned table at
`config/cost_table.yaml` (baked in via `include_str!`; overridable via
`bridge.cost_table_path`). The table carries a `verified_at` field. The
bridge:

- **Warns at startup** if the table is more than **90 days** stale.
- **Refuses to apply cost caps** (token caps stay enforced) when the
  table is more than **180 days** stale. Rationale: token caps degrade
  safely under stale pricing data, dollar caps don't (the dollar
  number stops being a meaningful upper bound).

To override, point `bridge.cost_table_path` at your own YAML file with
the same shape. The freshness gates apply to overrides too.

---

## Audit trail

Telemetry IS the audit log. Both binaries emit OpenTelemetry spans
through the standard OTLP exporter; route them through a Collector to a
backend of your choice (Honeycomb, Datadog, Grafana Cloud, or local
Postgres via the example Collector config at
[`examples/telemetry/`](../examples/telemetry/)).

Spans you care about for compliance:

| Span | Emitted by | Tells you |
|---|---|---|
| `runner.session` | daemon | "Sinfonia ran an agent on issue X starting at time T, using model M, for N turns." |
| `runner.turn` | daemon | "Each agent turn within a session: token count, duration, exit reason." |
| `workspace.hook` | daemon | "The `after_create` / `before_run` hook ran in workspace W with exit code C." |
| `bridge.webhook` | bridge | "GitHub delivered event E at time T; HMAC verification passed/failed." |
| `bridge.state_transition` | bridge | "Bridge moved ticket X from state A to state B because of reason R." |
| `bridge.cap_hit` | bridge | "Bridge detected per-ticket cap exhaustion for ticket X (which cap, what value)." |
| `bridge.cost_update` | bridge | "Bridge flushed the cost accumulator for ticket X (which fields, what values)." |

Every span carries `tenant_id`, so a multi-project install can answer
per-tenant questions cleanly.

### Reference queries

The three queries shipped at
[`examples/telemetry/queries/`](../examples/telemetry/queries/) cover
the most common audit questions:

```sql
-- "What did Sinfonia cost us per project per month?"
SELECT tenant_id,
       date_trunc('month', occurred_at) AS month,
       SUM(prompt_tokens + completion_tokens) AS tokens,
       SUM(cost_usd::numeric) AS cost
FROM events
WHERE event_type = 'runner.session.completed'
GROUP BY 1, 2
ORDER BY 1, 2;

-- "What fraction of tickets passed on the first attempt?"
-- ... see examples/telemetry/queries/first_try_rate.sql

-- "Which tickets hit the budget cap, and why?"
-- ... see examples/telemetry/queries/top_budget_tickets.sql
```

For "what tickets did agent X touch in Q1" — group by
`provider` + `model` over a `runner.session.completed` filter. The
schema at
[`examples/telemetry/postgres-schema.sql`](../examples/telemetry/postgres-schema.sql)
is the source of truth for column names.

### Tracker as a secondary audit log

The bridge writes the following per-ticket fields. Together with the
tracker's own activity log, they reconstruct the ticket's full
automation history:

- `sinfonia_attempt_count` — how many CI rounds this ticket has gone
  through.
- `sinfonia_last_ci_failure` — last 50 lines of the most-failed check
  (PII risk: failure logs can contain user-visible names if the test
  suite isn't careful).
- `sinfonia_failure_category` — `lint` / `e2e` / `…` per the bridge's
  failure-categorization rules.
- `sinfonia_tokens_consumed`, `sinfonia_cost_consumed_usd`,
  `sinfonia_budget_exhausted_at` — budget telemetry.

The Linear-side envelope at `sinfonia_bridge_state_v1` (SPEC §11.6.2)
is bot-owned and rewritten in place. Treat it as the authoritative
snapshot of bridge state for that ticket.

---

## Failure handling

### Cap-hit behavior

When `feedback_loop.max_attempts` is reached without CI passing, the
bridge:

1. Writes `sinfonia_attempt_count = max_attempts` and the latest
   `sinfonia_last_ci_failure`.
2. Transitions the ticket to `feedback_loop.blocked_state` (default
   "Blocked - Human Review").
3. Applies the `<prefix>:cap-hit` PR label.
4. Posts the failure comment (Liquid-rendered from
   `feedback_loop.failure_comment_template`).
5. Stops re-incrementing the counter. A human reset is required to
   resume.

The blocked-state ticket is OUT of the daemon's active set, so no
further agent work happens until a human moves it.

### Budget-cap behavior

Same flow as cap-hit but routes to `feedback_loop.budget_exceeded_state`
and uses the `<prefix>:budget-exceeded` label. The cost-table freshness
gate (above) may demote the cost cap to non-enforcing; tokens stay
enforced.

### Bridge label namespace

The bridge maintains six canonical labels under
`github.label_prefix` (default `sinfonia`):

- `<prefix>:in-progress` — applied when the agent starts a session.
- `<prefix>:awaiting-review` — applied when CI is green and the PR is
  open.
- `<prefix>:needs-fixes` — applied on red CI.
- `<prefix>:cap-hit` — applied at attempt-cap exhaustion.
- `<prefix>:budget-exceeded` — applied at budget-cap exhaustion.
- `<prefix>:failure:<category>` — applied per failed-check category
  (e.g. `sinfonia:failure:e2e`).

The bridge is the **single writer** of these labels; humans can add or
remove non-namespaced labels without interference. Operators with
pre-existing label schemes can override individual names via
`github.label_aliases` (verbatim semantics — the alias supplies the
FULL label name, no prefix is prepended).

### CODEOWNERS interaction

Sinfonia is designed to *coexist* with CODEOWNERS, not bypass it. The
recommended pattern:

1. Make Sinfonia-touched paths require human review via CODEOWNERS.
2. Configure your branch protection rule to require those CODEOWNERS
   reviews to merge.
3. Sinfonia agents can address review comments (by pushing follow-up
   commits) but cannot self-approve a PR — CODEOWNERS approvers are
   typically branch-protection-required reviewers, distinct from
   commit authors.

This makes the human checkpoint architectural, not aspirational. The
agent's "address the latest review comments" prompt template handles
the loop iteration cleanly.

---

## Handoff to the operating team

The operating team needs to know:

### What to monitor

- **`WARN orchestrator: candidate fetch failed`** lasting > 5 min →
  tracker outage or expired API key.
- **`seconds_running` aggregate** growing without `total_tokens` keeping
  pace → a stalled session is about to be killed by `stall_timeout_ms`.
- **`WARN bridge: webhook hmac mismatch`** → either secret rotation
  forgot one side, or someone is hitting `/webhook` from outside GitHub.
- **`WARN bridge: cost table is N days stale`** → time to refresh
  `config/cost_table.yaml` (or your override) before the 180-day
  no-enforce kick in.

### Routine ops

- **Rotate HMAC secrets quarterly.** Stop both processes, swap the value
  in both config files (and the GitHub webhook configuration), restart.
- **Rotate tracker API keys when a team member leaves** (PAT mode) or
  when org access changes (App mode).
- **Backup the bridge SQLite daily.** Not strictly required for
  correctness; see [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) Backup and
  recovery.
- **Watch the cost table.** The `WARN` at 90d is your reminder to refresh.

### Runbook template

Sinfonia does NOT ship a runbook in v0.3 — every team's incident-response
shape is different. The headers a useful runbook would cover:

1. **"Agent is making nonsense changes to a ticket."** Move the ticket
   to a terminal state. Daemon stops within one poll. Diagnose at
   leisure.
2. **"Bridge is not transitioning tickets after CI."** Run
   `sinfonia-bridge BRIDGE.md --self-test`. Most failures land in the
   GitHub auth or webhook-reachability lines.
3. **"Sinfonia is racking up cost."** Lower
   `agent.max_concurrent_agents` in `WORKFLOW.md` (the daemon
   hot-reloads). Tighten `feedback_loop.max_cost_per_ticket_usd` in
   `BRIDGE.md` (requires bridge restart).
4. **"A ticket is stuck in 'Blocked - Human Review'."** Reset the
   `sinfonia_attempt_count` to 0 (Linear: edit the marker comment; Jira:
   edit the field directly) and transition back to an active state.
5. **"How do I see what the agent did last night?"** Either the
   telemetry queries (`examples/telemetry/queries/`) or the
   tracker-side activity log + the `sinfonia_*` fields on each ticket.

### Documentation pointers

What to hand the operating team along with this doc:

- [`README.md`](../README.md) — getting started + the per-state runner
  reference.
- [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) — topologies, credentials,
  scaling, backup, upgrade.
- [`BRIDGE.example.md`](../BRIDGE.example.md) — every bridge config
  field with a worked default.
- [`WORKFLOW.example.md`](../WORKFLOW.example.md) — every daemon config
  field with a state-machine example.
- [`docs/SPEC.md`](SPEC.md) — the underlying spec. Read §11 (tracker
  + bridge contract) and §15 (trust posture) first.
- [`docs/SKILLS.md`](SKILLS.md) — the setup skills, for ongoing changes.
- [`docs/JIRA-SCREEN-SCHEME.md`](JIRA-SCREEN-SCHEME.md) — Jira admin
  reference for first-run custom-field binding.
- [`docs/MIGRATION-v0.2-to-v0.3.md`](MIGRATION-v0.2-to-v0.3.md) — for
  the next minor-version upgrade.

---

## Vendor evaluation worksheet

The checklist below is the one we'd hand a prospective adopter's
security review team. Answers are linked to the doc / config / source
file where the claim is verifiable.

### Trust and credentials

| Question | Answer | Verifiable at |
|---|---|---|
| Does Sinfonia execute code outside the per-issue workspace? | No. Agent processes operate inside `workspace.root/<ISSUE-ID>/`. The orchestrator is path-sanitized (SPEC §9.4). | `crates/sinfonia/src/workspace/` |
| What credentials does Sinfonia persist to disk? | None by default. Credentials are env vars on the daemon process. The bridge persists ONLY a SQLite of webhook delivery IDs + PR↔ticket mappings — no secrets. | `crates/sinfonia-bridge/src/storage.rs` |
| What credentials does the bridge persist to disk? | None. The bridge's SQLite (`storage.state_db_path`) contains delivery IDs and the PR↔ticket map. Both come from public webhook payloads. | `crates/sinfonia-bridge/src/storage.rs` schema |
| Does Sinfonia / the bridge ever transmit credentials in cleartext? | No, when configured with HTTPS endpoints. HMAC secrets are NEVER transmitted (they're hash inputs, not headers). | spec §11.6.5; bridge `webhook/verify.rs` |
| Can the agent escape its workspace? | The `shell` tool runs `bash -lc` in the workspace directory; an agent COULD `cd ..` and read outside. Mitigation: run the daemon under a dedicated low-privilege OS user. | README "Trust posture" |
| Can the bridge write tickets outside its configured project? | No — the bridge's tracker API key is scoped at the tracker layer (Linear team / Jira project). | bridge tracker config; tracker API docs |
| Can the bridge write PRs outside its configured repos? | App mode: scoped to installed repos. PAT mode: bounded by the PAT's repo scope. | `github.pat` scopes; App permissions |

### Operational posture

| Question | Answer | Verifiable at |
|---|---|---|
| What's the recovery point objective (RPO)? | Sub-minute. Workspace state is rebuildable; bridge SQLite is single-machine-loss tolerant; tracker is the source of truth. | [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) Backup |
| What's the recovery time objective (RTO)? | Seconds. Restart the daemon — it re-syncs from the tracker on next tick. Restart the bridge — webhook retries from GitHub fill the gap. | spec §14.3 |
| Does Sinfonia upgrade in place? | Yes within a minor. Cross-minor requires reading the migration doc. | [`docs/MIGRATION-v0.2-to-v0.3.md`](MIGRATION-v0.2-to-v0.3.md) |
| Is there an audit log? | Yes — OpenTelemetry spans from both binaries are the canonical audit log. The tracker's `sinfonia_*` fields are a secondary, per-ticket log. | `examples/telemetry/`, SPEC §11.6.11 |
| What happens when the LLM provider is down? | Daemon retries with exponential backoff (`agent.max_retry_backoff_ms`, default 5 min). After max retries, the session ends and the orchestrator picks up the ticket again on the next poll. | spec §8 |
| What happens when the tracker is down? | Daemon logs `WARN orchestrator: candidate fetch failed` and skips dispatch for the tick. Bridge logs an error and the webhook handler returns HTTP 500 (GitHub retries). | spec §11.4 |

### Compliance

| Question | Answer | Verifiable at |
|---|---|---|
| Does Sinfonia log PII? | Sinfonia logs tracker identifiers (`ENG-42`) and issue titles. The bridge logs PR titles. CI failure excerpts (50-line tail) may contain PII if the test suite includes it. Mitigation: scrub at the Collector layer if required. | `crates/sinfonia/src/`, `crates/sinfonia-bridge/src/feedback/` |
| Does Sinfonia have a SOC 2 / ISO 27001 report? | Sinfonia is open-source software you deploy. The compliance posture is YOUR deployment's posture; the code is licensed Apache-2.0 with no warranty. | LICENSE |
| Where is data stored? | The daemon's data lives on the host you run it on. The bridge's data lives on the host it runs on. Telemetry goes to the OTLP endpoint you configure. Tracker data lives in your tracker. There is no Sinfonia-side cloud component. | n/a — observable in your environment |
| Can I run Sinfonia air-gapped? | Yes, against an on-prem GitLab / Forgejo + a local-network tracker. Topology 1 (no bridge) is fully air-gappable if the agent backend runs locally (`ollama` + `opencode`). The bridge requires GitHub reachability. | see Topology 1 in [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) |

### Cost containment

| Question | Answer | Verifiable at |
|---|---|---|
| What's the cap on per-ticket cost? | Configurable via `feedback_loop.max_cost_per_ticket_usd` (USD). Override per-ticket via `sinfonia_max_cost_usd` custom field. | [`BRIDGE.example.md`](../BRIDGE.example.md) feedback_loop |
| What's the cap on total cost? | None directly — the bridge enforces per-ticket caps. Total cost is bounded by (max ticket cost) × (count of active tickets) × (max attempts). | derived |
| Can I see cost in real time? | Yes via the telemetry queries — `examples/telemetry/queries/tenant_monthly_cost.sql` aggregates from span attributes. | examples/telemetry/queries/ |
| How accurate is the cost table? | `config/cost_table.yaml` carries `verified_at`. Bridge warns at 90 days stale, refuses to enforce cost caps at 180 days stale. | `crates/sinfonia-bridge/src/feedback/cost.rs` |

---

## Where to go next

- [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) — pick a topology and execute.
- [`docs/SKILLS.md`](SKILLS.md) — hand the setup-bridge / setup-state-machine
  skills to your AI coding tool and let it scaffold the config.
- [`docs/MIGRATION-v0.2-to-v0.3.md`](MIGRATION-v0.2-to-v0.3.md) — if
  you're upgrading from v0.2.
- [`docs/SPEC.md`](SPEC.md) — the underlying spec. Read §11 (tracker
  + bridge) and §15 (trust posture) first.
