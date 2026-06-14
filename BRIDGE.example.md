---
# ---- Tracker ----
# The bridge reads the tracker's project state, comment threads, and (for
# Linear) the per-issue marker comment that stores the feedback-loop
# counters. The schema mirrors WORKFLOW.md's tracker block so a single
# project can share credentials between Sinfonia (the daemon) and the
# bridge. In a real install, swap each literal value below for an
# `$ENV_VAR` reference — see the "Production note" at the bottom of
# this file.
tracker:
  kind: linear                          # linear | jira  (both supported as of Phase 4)
  # endpoint: https://api.linear.app/graphql   # optional; default shown
  api_key: lin_api_REPLACE_ME
  project_slug: my-awesome-project-abc123def456
  # active_states / terminal_states are optional in BRIDGE.md — the bridge
  # mostly defers to WORKFLOW.md for "what counts as active." They are
  # accepted here so a single project file can be re-pointed at the bridge
  # without surgery.
  active_states: ["Todo", "In Progress", "In Review"]
  terminal_states: ["Done", "Cancelled", "Duplicate"]

# Jira Cloud example (active as of Phase 4):
# tracker:
#   kind: jira
#   endpoint: https://acme.atlassian.net         # required — no per-tenant default
#   api_key: ATATT_REPLACE_ME                    # Atlassian API token
#   email: you@example.com                       # required for Cloud Basic auth
#   project_slug: ABC                            # Jira project key (the prefix in ABC-123)
#   active_states: ["To Do", "In Progress", "In Review"]
#   terminal_states: ["Done", "Cancelled"]
#
# Jira Server / Data Center (self-hosted) — omit `email` and put a PAT
# in `api_key` to switch the adapter to Bearer auth:
# tracker:
#   kind: jira
#   endpoint: https://jira.internal.example.com
#   api_key: $JIRA_PAT
#   project_slug: ABC
#   active_states: ["To Do", "In Progress", "In Review"]
#   terminal_states: ["Done", "Cancelled"]
#
# First-run note: on a fresh Jira project the bridge creates the
# `sinfonia_*` custom fields automatically and tries to bind them to a
# screen so they show up in the UI. The bind call requires admin perms;
# if it fails the bridge logs a WARN and continues (reads + writes via
# REST still work). See `docs/JIRA-SCREEN-SCHEME.md` for the manual bind.

# ---- GitHub ----
# The bridge subscribes to webhooks from one or more GitHub repositories
# and writes back PR labels, status transitions, and failure comments.
github:
  webhook_secret: a-shared-hmac-secret   # HMAC-SHA256 shared secret; required.

  # ONE of `pat` or `app_id` must be set; setting both is a startup error.
  # PAT mode — quickest path to a working install. Required scopes:
  # `repo` (full) and `read:org` (for multi-repo PR resolution).
  pat: ghp_REPLACE_ME

  # App mode — recommended for production. Set `app_id` AND `private_key`;
  # comment out the `pat:` line above. The bridge mints per-installation
  # access tokens automatically via `octocrab::Octocrab::installation(id)`.
  # private_key accepts inline PEM contents OR an `@/path/to/key.pem`
  # filesystem indirection.
  # app_id: 123456
  # private_key: |
  #   -----BEGIN RSA PRIVATE KEY-----
  #   ...PEM contents...
  #   -----END RSA PRIVATE KEY-----
  # private_key: "@/etc/sinfonia/bridge-app.pem"   # OR file reference

  # PR label management. The bridge maintains six canonical labels:
  #   <prefix>:in-progress, <prefix>:awaiting-review, <prefix>:needs-fixes,
  #   <prefix>:cap-hit,     <prefix>:budget-exceeded, <prefix>:failure:<category>
  manage_labels: true                   # set false to disable label writes entirely
  label_prefix: "sinfonia"              # leading namespace; default "sinfonia"

  # Aliases — verbatim semantics (H-4). Each value, when set, supplies
  # the FULL label name; `label_prefix` is NOT prepended. Useful when
  # you already have an existing label scheme you want the bridge to
  # adopt instead of creating a parallel set.
  label_aliases:
    # in_progress:     "ai:working"
    # awaiting_review: "ai:ready-for-review"
    # needs_fixes:     "ai:needs-fixes"
    # cap_hit:         "ai:cap-hit"
    # budget_exceeded: "ai:budget-cap"
    # failure_prefix:  "ai:failure"     # the bridge appends ":<category>" to this

# ---- Feedback loop ----
# How the bridge interprets CI results and drives state transitions.
feedback_loop:
  max_attempts: 5                       # cap on Sinfonia retries per ticket; default 5
  needs_fixes_state: "Needs Fixes"      # state to route to on red CI
  blocked_state: "Blocked - Human Review"   # state to route to once max_attempts is hit

  # Regex applied to PR title + body to discover the linked tracker
  # identifier (e.g. "Closes ENG-42"). Default shown.
  pr_link_pattern: '(?i)(?:closes|fixes|resolves)\s+([A-Z]+-\d+|[a-z]+-\d+)'

  # Empty = all required checks (as configured in the repo's branch
  # protection) must pass. Populate to override.
  required_checks: []

  # ---- Budget caps ----
  # Per-ticket token and dollar ceilings enforced at the tracker-write
  # boundary. Cap-crossings flush the accumulator, transition the ticket
  # to `budget_exceeded_state`, and stop the bridge from re-driving more
  # work on that ticket. Per-ticket overrides come from the
  # `sinfonia_max_attempts` / `sinfonia_max_cost_usd` custom fields.
  # Leave `null` to disable the corresponding cap.
  max_tokens_per_ticket: 1_500_000
  max_cost_per_ticket_usd: "25.00"
  budget_exceeded_state: "Blocked - Budget Cap"

  # Liquid template rendered into a PR comment on each red CI run.
  # Available variables: attempt, max_attempts, failed_checks,
  # failure_log_excerpt, failure_category, pr_url, ticket_identifier.
  failure_comment_template: |
    CI failed on attempt {{ attempt }} of {{ max_attempts }} (category: {{ failure_category }}).

    Failed checks:
    {{ failed_checks }}

    Last 50 lines of the most-failed check:
    ```
    {{ failure_log_excerpt }}
    ```

    The bridge has moved {{ ticket_identifier }} to the appropriate
    "needs fixes" state; Sinfonia will pick it back up on the next poll.

  # Optional category routing. The bridge inspects the failed check
  # names, matches each `check_pattern` regex, and routes to the
  # highest-priority match's `target_state`. Priorities MUST be unique
  # across categories. If absent, all failures route to
  # `needs_fixes_state`. A synthetic `default` (priority 0, no pattern)
  # is always added if you don't supply one.
  failure_categories:
    - name: lint
      check_pattern: '(?i)(lint|prettier|eslint|rustfmt|clippy|black|ruff)'
      target_state: "Needs Fixes - Lint"
      priority: 20
    - name: e2e
      check_pattern: '(?i)(e2e|playwright|cypress|selenium)'
      target_state: "Needs Fixes - E2E"
      priority: 10
    - name: default
      target_state: "Needs Fixes"      # no check_pattern → catches everything else
      priority: 0

  # ---- Harness manifest ingestion (Proposal 0001; OPTIONAL) ----
  # When a conforming test harness publishes a `bridge.json` failure
  # manifest as a run artifact, the bridge can fetch it on a red
  # `workflow_run`, parse the structured per-scenario failures
  # (scenario / feature_file / step / assertion + artifact references),
  # and fold them into `sinfonia_last_ci_failure` (and the
  # `failure_log_excerpt` template variable) so the retry turn sees *why*
  # CI failed, not just which check did. Best-effort and degrade-only:
  # any miss (no artifact, oversize, malformed, version too old) logs a
  # warning and falls back to the check-name behavior. The manifest is
  # treated as untrusted input (it may come from a fork PR) with size,
  # count, and length caps. Ingestion is ON by default; set the switch to
  # false to force the legacy check-name-only feedback.
  # When absent or set to false, the bridge falls back to the check-name path
  # exactly as in versions prior to Proposal 0001 — no change to WORKFLOW.md
  # is required.
  ingest_harness_manifest: true                  # master switch; default true
  harness_manifest_artifact_glob: "bridge-*"     # run artifact holding bridge.json (one `*` wildcard)
  harness_manifest_filename: "bridge.json"        # entry name inside the artifact zip
  max_artifact_bytes: 5_242_880                   # 5 MiB download cap (zip-bomb / exhaustion defense)
  max_failures_parsed: 20                         # max scenarios folded into the digest
  max_failure_digest_bytes: 8_192                 # cap on the sinfonia_last_ci_failure digest text

# ---- Custom fields ----
# Tracker-side field names the bridge reads and writes. The Linear
# adapter stores the entire envelope in a single bot-owned comment
# (the `sinfonia_bridge_state_v1` marker — see docs/SPEC.md §11.6).
# The Jira adapter resolves each name to a real `customfield_NNNNN`
# id at startup and caches it for the process lifetime (§11.7.2).
#
# All seven keys are REQUIRED and MUST be non-empty (parser rule 8).
# Rename only if the displayed name conflicts with an existing field —
# the Phase 5 skill templates reference these defaults.
custom_fields:
  attempt_count:          sinfonia_attempt_count
  last_failure_log:       sinfonia_last_ci_failure
  max_attempts_override:  sinfonia_max_attempts
  failure_category:       sinfonia_failure_category
  tokens_consumed:        sinfonia_tokens_consumed
  cost_consumed_usd:      sinfonia_cost_consumed_usd
  max_cost_override_usd:  sinfonia_max_cost_usd

# ---- HTTP server ----
# The bridge listens for inbound GitHub webhooks here.
server:
  bind: "0.0.0.0"                       # default 0.0.0.0
  port: 8081                            # default 8081
  # public_url is the externally reachable URL of THIS bridge instance.
  # Used by `sinfonia-bridge --self-test` to probe /health from the
  # outside. When unset, the reachability check SKIPs (not a failure).
  public_url: https://bridge.example.com

# ---- Storage ----
# SQLite-backed idempotency + PR↔ticket mapping. `~` is shell-expanded.
storage:
  state_db_path: ~/.sinfonia/bridge.db

# ---- Telemetry ----
# Opt-in OpenTelemetry emission + the typed Sinfonia↔bridge event
# channel. When `otlp_endpoint` is unset (and OTEL_EXPORTER_OTLP_ENDPOINT
# is unset in the environment) the OTel layer is disabled and the bridge
# runs stdout-only — same behaviour as v0.3.0-alpha.1.
#
# tenant_id (precedence: this field → SINFONIA_TENANT_ID env → "default")
# tags every span as a per-span attribute AND is surfaced as the
# resource-level `service.namespace`, so a Collector routing-processor
# can split per-tenant data without touching emission code.
#
# `sinfonia_event_*` keys wire up the typed event channel (SPEC §11.6.11).
# When the bridge is set to receive runner-session events from Sinfonia,
# `sinfonia_event_callback_url` is the externally-reachable URL where
# Sinfonia POSTs events (HMAC-SHA256 signed); `sinfonia_event_subscribe_url`
# is the Sinfonia endpoint the bridge calls at startup to register itself.
# `sinfonia_events_secret` is REQUIRED when either subscribe / callback URL
# is set — the value MUST match Sinfonia's `telemetry.sinfonia_events_secret`.
telemetry:
  service_name: sinfonia-bridge
  # tenant_id: acme
  # otlp_endpoint: http://otel-collector:4318
  # # Required when sinfonia_event_subscribe_url is set:
  # sinfonia_events_secret: a-shared-hmac-secret
  # sinfonia_event_subscribe_url: http://sinfonia:8080/api/v1/events/subscribers
  # sinfonia_event_callback_url: http://bridge.local:8081/api/v1/sinfonia-events

# ---- Cost table override (optional) ----
# The bridge embeds `config/cost_table.yaml` at compile time via
# `include_str!`. Override at runtime by pointing this key at an
# external YAML file with the same shape — useful when provider pricing
# moves faster than the Sinfonia release cadence. The freshness gates
# documented in SPEC §11.6.12 apply to overrides too: WARN at 90 days
# stale, refuse to apply cost caps (token caps stay enforced) at 180
# days stale.
# cost_table_path: /etc/sinfonia/cost_table.yaml
---

# Bridge for my-awesome-project

This Markdown body is for human notes — the bridge parser ignores everything
below the closing `---` fence.

## Quick reference

- Parse + validate this file: `sinfonia-bridge BRIDGE.example.md --check`
- Run the install gate: `sinfonia-bridge BRIDGE.example.md --self-test`
- Start the bridge: `sinfonia-bridge BRIDGE.example.md`

## How the example maps to a running install

1. The `tracker:` block reuses the same Linear API key as your
   `WORKFLOW.md`. The bridge reads the tracker only on demand (to look
   up counters and resolve state IDs); it does not own the polling
   loop — that's still Sinfonia's job.
2. The `github:` block decides whether the bridge runs as a single-repo
   PAT install (fast to set up, scoped to one user) or as a multi-repo
   GitHub App install (recommended for production). Exactly one of
   `pat` / `app_id` must be set.
3. The `feedback_loop:` block is where you decide what "too many tries"
   means and which states your tracker uses for the routed failure
   buckets. Cap-hit moves the ticket to `blocked_state` and stops the
   bridge from re-incrementing the counter; this is the human-review
   handoff.
4. `custom_fields:` lets you rename the marker keys if you already have
   competing field names on the tracker. Don't rename them lightly —
   Phase 5 skill templates reference the defaults.
5. `server.public_url` is the externally reachable URL where GitHub
   webhooks land. `--self-test` uses it to probe `/health` from the
   outside.

## Production note: env-var indirection

The example above uses literal placeholder values (`ghp_REPLACE_ME`,
`lin_api_REPLACE_ME`, `a-shared-hmac-secret`) so this file can be
validated by `sinfonia-bridge BRIDGE.example.md --check` with no
environment variables set.

In a real install, swap each secret for an `$ENV_VAR` reference — the
same indirection `WORKFLOW.example.md` uses. The bridge parser
substitutes `$NAME` against `std::env` at startup; an unset or empty
variable is treated as "field not set," which surfaces as a
descriptive startup error rather than silently sending an empty
credential.

Recommended environment variables:

| Env var | Field it backs |
|---|---|
| `LINEAR_API_KEY` | `tracker.api_key` |
| `GH_WEBHOOK_SECRET` | `github.webhook_secret` |
| `GH_PAT` | `github.pat` (PAT mode only) |
| `GH_APP_ID` + `GH_APP_KEY` | `github.app_id` + `github.private_key` (App mode only) |

A production `BRIDGE.md` derived from this template would read e.g.:

```yaml
tracker:
  api_key: $LINEAR_API_KEY
github:
  webhook_secret: $GH_WEBHOOK_SECRET
  pat: $GH_PAT
telemetry:
  sinfonia_events_secret: $SINFONIA_EVENTS_SECRET
```

## Where to go next

- [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) — production topologies (single-host, separate
  hosts, GitHub-Actions bridge) and the credential model.
- [`docs/CLIENT_SETUP.md`](docs/CLIENT_SETUP.md) — enterprise adoption checklist, trust
  boundaries, security posture, vendor evaluation worksheet.
- [`docs/SPEC.md`](docs/SPEC.md) §11.6 / §11.7 — the underlying recommended-extension contract
  this file's keys implement.
- [`skills/setup-bridge/SKILL.md`](skills/setup-bridge/SKILL.md) — the AI-tool-driven REPL
  that scaffolds a `BRIDGE.md` from a fresh project.
