---
# ============================================================================
# WORKFLOW.smoke.md — minimal config used by the Phase 6 compose smoke test.
#
# Goal: exercise the load → validate → bind path inside the `sinfonia`
# image without dispatching any agent or hitting any real tracker.
#
# - Linear credentials are deliberately fake. `fetch_candidate_issues` will
#   fail; the orchestrator logs a warning and continues, so the HTTP
#   surface still comes up cleanly on :8080.
# - The agent block declares the cheapest possible backend (Ollama at
#   localhost) so config validation passes. No subprocess will ever start
#   because polling never returns a candidate issue.
# ============================================================================

tracker:
  kind: linear
  api_key: SMOKE_FAKE_KEY
  project_slug: smoke-test-fixture
  active_states: ["Todo", "In Progress"]
  terminal_states: ["Done", "Cancelled"]

polling:
  interval_ms: 60000

workspace:
  root: /tmp/sinfonia-smoke-workspaces

agent:
  max_concurrent_agents: 1
  max_turns: 1
  max_retry_backoff_ms: 60000
  provider: ollama
  model: smoke-noop
  temperature: 0.0
  turn_timeout_ms: 60000
  stall_timeout_ms: 60000

server:
  bind: 0.0.0.0
  port: 8080
---

Smoke-test prompt body — never rendered because the smoke tracker config
never returns any candidate issues. Present so the file is a valid
WORKFLOW.md.

Issue: {{ issue.identifier | default: "smoke" }}.
