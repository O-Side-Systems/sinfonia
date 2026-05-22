---
# ============================================================================
# BRIDGE.smoke.md — minimal config used by the Phase 6 compose smoke test.
#
# Goal: exercise the load → validate → bind path inside the
# `sinfonia-bridge` image without exercising any tracker or webhook.
# `/health` should respond on :8081.
# ============================================================================

tracker:
  kind: linear
  api_key: SMOKE_FAKE_KEY
  project_slug: smoke-test-fixture
  active_states: ["Todo", "In Progress", "In Review"]
  terminal_states: ["Done", "Cancelled"]

github:
  webhook_secret: smoke-fake-shared-secret
  pat: ghp_SMOKE_FAKE
  manage_labels: false
  label_prefix: "sinfonia"

feedback_loop:
  max_attempts: 1
  needs_fixes_state: "Needs Fixes"
  blocked_state: "Blocked - Human Review"
  pr_link_pattern: '(?i)(?:closes|fixes|resolves)\s+([A-Z]+-\d+|[a-z]+-\d+)'
  required_checks: []
  max_tokens_per_ticket: null
  max_cost_per_ticket_usd: null
  budget_exceeded_state: "Blocked - Budget Cap"
  failure_comment_template: |
    Smoke test — never rendered.
  failure_categories:
    - name: default
      target_state: "Needs Fixes"
      priority: 0

custom_fields:
  attempt_count:          sinfonia_attempt_count
  last_failure_log:       sinfonia_last_ci_failure
  max_attempts_override:  sinfonia_max_attempts
  failure_category:       sinfonia_failure_category
  tokens_consumed:        sinfonia_tokens_consumed
  cost_consumed_usd:      sinfonia_cost_consumed_usd
  max_cost_override_usd:  sinfonia_max_cost_usd

server:
  bind: "0.0.0.0"
  port: 8081

storage:
  state_db_path: /tmp/sinfonia-smoke-bridge.db
---

Smoke bridge body — content unused (the bridge does not Liquid-render its
own front matter body the way the daemon does). Present so the parser
treats the file as well-formed.
