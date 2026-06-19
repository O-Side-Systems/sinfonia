---
# ============================================================================
# sinfonia-bridge config for the docker dev stack — EXAMPLE / TEMPLATE.
#
# Copy this to `docker/BRIDGE.md` (gitignored) and fill in the project-specific
# values (tracker.project_slug and feedback_loop.required_checks). The live
# `docker/BRIDGE.md` is bind-mounted into the `bridge` service exactly as
# `docker/WORKFLOW.md` is mounted into the `daemon` service.
#
# Companion to docker/WORKFLOW.md: the daemon polls the tracker and runs the
# agent; this bridge reacts to GitHub check_suite / workflow_run / pull_request
# webhooks and drives tracker state. Secrets are $ENV references resolved at
# parse time — supplied via .env / docker compose, never inline.
#
# Validate:  sinfonia-bridge docker/BRIDGE.md --check
# Self-test: sinfonia-bridge docker/BRIDGE.md --self-test
# See the repo-root BRIDGE.example.md for the full key reference.
# ============================================================================

# ---- Tracker ----  (use the same project as docker/WORKFLOW.md)
tracker:
  kind: linear                                # linear | jira
  api_key: $LINEAR_API_KEY
  project_slug: my-project-abc123def456       # <-- replace with your Linear project slug
  active_states: ["Todo", "In Progress"]
  terminal_states: ["Done", "Cancelled"]

# ---- GitHub ----
github:
  webhook_secret: $GH_WEBHOOK_SECRET          # must match the secret on the repo webhook
  pat: $GH_TOKEN                              # repo + read:org; reuses the daemon's token
  manage_labels: true
  label_prefix: "sinfonia"

# ---- Feedback loop ----
feedback_loop:
  max_attempts: 5

  # Red CI → route to an ACTIVE state (see tracker.active_states) so the daemon
  # re-dispatches the agent. If your workflow treats "moved back to In Progress"
  # as "please fix", point this at "In Progress" so red CI self-heals.
  needs_fixes_state: "In Progress"

  # Attempt cap exhausted → human gate (tagged sinfonia:cap-hit).
  blocked_state: "In Review"

  # Green CI (every required_check passed) → bridge promotes to human review.
  # OMIT to keep label-only behaviour (the agent owns the review transition).
  awaiting_review_state: "In Review"

  # Extract the tracker identifier from the PR title/body. The default matches
  # "Closes ABC-123"; this looser pattern matches a bare KEY-NUMBER token (e.g.
  # a "feat(ABC-123): …" title).
  pr_link_pattern: '(?i)\b([A-Z]{2,}-\d+)\b'

  # CLOSED green gate: every name here must report PASS on the PR head (none
  # failing, none pending) before the bridge calls the PR green. Use the EXACT
  # GitHub check-run names. Leave empty to fall back to "nothing failed + at
  # least one passed" (which can call a still-running suite green).
  required_checks:
    - "build"
    - "test"
    - "lint"

  max_tokens_per_ticket: null
  max_cost_per_ticket_usd: null
  budget_exceeded_state: "In Review"

  failure_comment_template: |
    CI failed on attempt {{ attempt }} of {{ max_attempts }} (category: {{ failure_category }}).

    Failed checks:
    {{ failed_checks }}

    ```
    {{ failure_log_excerpt }}
    ```

    The bridge moved {{ ticket_identifier }} back to "In Progress"; the agent
    will pick it up on the next poll. See {{ pr_url }}/checks for full logs.

  failure_categories:
    - name: default
      target_state: "In Progress"
      priority: 0

# ---- Custom fields ----  (Linear stores these in one bot-owned marker comment)
custom_fields:
  attempt_count:          sinfonia_attempt_count
  last_failure_log:       sinfonia_last_ci_failure
  max_attempts_override:  sinfonia_max_attempts
  failure_category:       sinfonia_failure_category
  tokens_consumed:        sinfonia_tokens_consumed
  cost_consumed_usd:      sinfonia_cost_consumed_usd
  max_cost_override_usd:  sinfonia_max_cost_usd

# ---- HTTP server ----
server:
  bind: "0.0.0.0"
  port: 8081
  # public_url left unset → --self-test SKIPs the external /health probe.
  # Set it to the externally reachable URL (e.g. a tunnel) once webhooks land.
  # public_url: https://<tunnel-host>

# ---- Storage ----  (persisted in the container; see docker-compose.dev.yml)
storage:
  state_db_path: ~/.sinfonia/bridge.db
---

# Bridge example (docker dev stack)

Copy to `docker/BRIDGE.md` and replace `tracker.project_slug` and
`feedback_loop.required_checks` with your project's values. The repo-root
`BRIDGE.example.md` documents every key in full.

The bridge only acts once GitHub can reach it: add a repo webhook (content type
`application/json`, secret = `$GH_WEBHOOK_SECRET`, events **Pull requests /
Check suites / Workflow runs**) pointing at a publicly reachable URL that
forwards to this container's port 8081.
