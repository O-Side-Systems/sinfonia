---
# ---- Tracker ----
tracker:
  kind: linear
  api_key: $LINEAR_API_KEY
  project_slug: my-project
  active_states: ["Todo", "In Progress", "In Review"]
  terminal_states: ["Done", "Cancelled", "Duplicate"]

# Jira example (swap with the linear block above):
# tracker:
#   kind: jira
#   endpoint: https://acme.atlassian.net
#   api_key: $JIRA_API_TOKEN
#   email: $JIRA_EMAIL              # optional: Atlassian Cloud uses Basic auth
#   project_slug: ABC                # Jira project key
#   active_states: ["To Do", "In Progress", "In Review"]
#   terminal_states: ["Done", "Cancelled", "Duplicate"]

polling:
  interval_ms: 30000

workspace:
  root: ~/sinfonia-workspaces

hooks:
  after_create: |
    git init -q
    # `.sinfonia/` is a per-workspace scratchpad used by prompts (plans,
    # notes, intermediate state). Keeping it gitignored stops parallel
    # agent branches from merge-conflicting on a shared scratchpad file
    # when their PRs land on main.
    touch .gitignore
    grep -qxF '.sinfonia/' .gitignore || echo '.sinfonia/' >> .gitignore
  before_run: |
    git status >/dev/null 2>&1 || true
  after_run: |
    git status -s || true
  timeout_ms: 60000

agent:
  max_concurrent_agents: 3
  max_turns: 6
  max_retry_backoff_ms: 300000
  max_concurrent_agents_by_state:
    "In Progress": 2

  # ---- Default LLM backend ----
  # Used for any state without a `states:` override below.
  # Provider options: openai | anthropic | google | ollama | claude_code | codex | opencode
  provider: anthropic
  model: claude-sonnet-4-6
  # api_key resolves from ANTHROPIC_API_KEY / OPENAI_API_KEY / GOOGLE_API_KEY by default.
  # For CLI providers (claude_code / codex), no API key is read by Sinfonia — the CLI
  # itself handles auth (e.g. `claude` reads ~/.claude/credentials).
  temperature: 0.1
  turn_timeout_ms: 3600000
  stall_timeout_ms: 300000

# ---- State machine ----
# Per-tracker-state runner + prompt overrides. State names are matched case-insensitively
# against issue.state. Anything not listed here falls back to the `agent:` block above.
#
# Why this exists:
# - Different states benefit from different agents. Fresh investigation work plays to
#   Claude Code's strengths; a focused code-write pass might fit Codex CLI's strengths;
#   a quick review pass can run on a cheap raw-LLM provider with no tool loop overhead.
# - Each override can swap provider, model, command, temperature, turn_timeout_ms, and
#   the prompt body. Unset fields inherit from the global `agent:` block.
# - When the tracker state changes mid-run and routes to a different runner, the current
#   worker exits cleanly and the orchestrator re-dispatches under the new runner.
states:
  "Todo":
    # First pass: hand the issue to Claude Code so it can investigate and scaffold.
    provider: claude_code
    model: claude-sonnet-4-6
    # command: defaults to "claude -p --output-format json --verbose --dangerously-skip-permissions"
    prompt: |
      You are picking up a brand-new issue: **{{ issue.identifier }} — {{ issue.title }}**.

      ## Description

      {{ issue.description }}

      ## What to do

      1. Read the repo to understand the existing patterns.
      2. Sketch a plan in `.sinfonia/plans/{{ issue.identifier | downcase }}.md`
         (create the dir if needed). The `.sinfonia/` tree is a per-workspace
         scratchpad — do NOT commit it. If the repo's `.gitignore` doesn't
         already cover it, add a line for `.sinfonia/` and commit that change
         on its own. Per-issue filenames also keep parallel agent branches
         from merge-conflicting on a single shared plan file.
      3. Make a minimal first cut of the change.
      4. When you've made meaningful progress, transition the issue to "In Progress" in the
         tracker (use the `shell` tool with `linear-cli` / `gh` / `jira` as appropriate).

  "In Progress":
    # Implementation pass: longer turn timeout, optionally a stronger model.
    provider: claude_code
    model: claude-opus-4-7
    turn_timeout_ms: 5400000   # 90 minutes
    prompt: |
      Resume implementation of **{{ issue.identifier }}**.

      The previous turn left state in this workspace and in
      `.sinfonia/plans/{{ issue.identifier | downcase }}.md`. Pick up from there. Run
      the project's tests. When the change is complete and tests pass, transition the
      issue to "In Review" in the tracker.

  "In Review":
    # Review pass: small fast raw-LLM call, no tool loop subprocess overhead.
    provider: anthropic
    model: claude-haiku-4-5-20251001
    temperature: 0.0
    prompt: |
      Review the changes for **{{ issue.identifier }}** as if you were a senior engineer.

      Use `shell` to run `git diff main...HEAD` (or equivalent), read the touched files,
      and produce a short review. If the change looks good, leave a "ship it" comment and
      transition to "Done". If not, leave a comment explaining what to fix and transition
      back to "In Progress".

# ---- OpenCode alternative (uncomment to use) ----
#
# Use this when:
# - You want LSP integration (OpenCode shells out to language servers for navigation /
#   completion / refactor — the raw `ollama` backend can't do this).
# - You want OpenCode's MCP support to plug in non-Anthropic / non-OpenAI tools.
# - You want a local Ollama with LSP for offline / air-gapped use.
#
# OpenCode (https://opencode.ai) is a coding-agent CLI with LSP, MCP, and 75+ provider
# backends. Like `claude_code` and `codex`, Sinfonia drives it as a subprocess in the
# workspace.
#
# Auth: OpenCode owns its own credentials — Sinfonia does NOT pass an api_key. Run
# `opencode auth login` once on the host before starting the daemon. The `model:` field
# is passed through verbatim as `--model <name>`; OpenCode's wire format is
# `provider/model` (e.g. `anthropic/claude-sonnet-4-6`, `ollama/qwen2.5-coder:32b`).
#
# Default-lane usage (replaces the `agent:` provider/model entries above):
#
# agent:
#   max_concurrent_agents: 3
#   provider: opencode
#   model: anthropic/claude-sonnet-4-6
#   turn_timeout_ms: 3600000
#
# State-machine usage (per-state overrides; mixes with the other backends):
#
# states:
#   Ready:
#     provider: opencode
#     model: anthropic/claude-sonnet-4-6
#     turn_timeout_ms: 3600000
#   "Needs Fixes":
#     provider: opencode
#     model: anthropic/claude-sonnet-4-6
#     prompt: |
#       Address the CI failures on {{ issue.identifier }}.
#       Last failure:
#       {{ issue.fields.sinfonia_last_ci_failure | default: "(see PR checks)" }}
#
# Air-gapped variant — Ollama with LSP:
#
# states:
#   Ready:
#     provider: opencode
#     model: ollama/qwen2.5-coder:32b   # local Ollama via opencode's `ollama/` route
#     turn_timeout_ms: 3600000

# ---- Failure-categorization state machine (works with sinfonia-bridge) ----
#
# Use this when:
# - You're running sinfonia-bridge (with `feedback_loop.failure_categories` configured
#   in BRIDGE.md to route lint / e2e / unit-test / type-check failures to distinct
#   "Needs Fixes - X" states).
# - You want a cheap raw-LLM lane for lint fixes (fast turnaround) and a heavier agent
#   for e2e or type-check failures (more context required).
# - You want the bridge to drive the loop and the daemon's state machine to route
#   each retry to the most cost-effective runner for that failure class.
#
# Pattern: Triage → Ready → (Needs Fixes - Lint | Needs Fixes - E2E | Needs Fixes) →
# Human Review. Each "Needs Fixes" variant pulls a different runner based on the cost /
# context the failure class needs.
#
# Every {{ issue.fields.X }} reference in the prompts below is guarded with `| default:`
# so a human dragging a ticket into Needs Fixes without any prior bridge run renders
# cleanly — strict-mode Liquid would otherwise reject absent fields (see SPEC §11.6.4).
#
# states:
#   "Triage":
#     # Cheap raw-LLM pass: scope, risk, suggested approach. Comment back on the
#     # ticket and transition to "Ready" if the request is clear, or
#     # "Needs Clarification" if not.
#     provider: anthropic
#     model: claude-haiku-4-5-20251001
#     prompt: |
#       Read {{ issue.identifier }} ({{ issue.title }}). Comment your scope assessment
#       and approach sketch on the ticket. Transition to "Ready" or "Needs Clarification".
#
#   "Ready":
#     # Full implementation pass: Claude Code with Opus and a long turn timeout.
#     provider: claude_code
#     model: claude-opus-4-7
#     turn_timeout_ms: 5400000
#     prompt: |
#       Implement {{ issue.identifier }}. Push to a branch named
#       sinfonia/{{ issue.identifier | downcase }} and open a PR with body
#       "Closes {{ issue.identifier }}" so the bridge can map the PR back to the ticket.
#       Transition to "In Review" when the PR exists and CI is green.
#
#   "Needs Fixes - Lint":
#     # Cheap fast lane: lint fixes are mechanical; a raw-LLM can do them.
#     # The bridge routes here when the failing check name matches the lint pattern
#     # (see BRIDGE.example.md `feedback_loop.failure_categories`).
#     provider: anthropic
#     model: claude-haiku-4-5-20251001
#     prompt: |
#       The PR for {{ issue.identifier }} is failing a lint check.
#
#       Last failure log:
#       {{ issue.fields.sinfonia_last_ci_failure | default: "(see PR checks)" }}
#
#       Apply the fix, push, and transition back to "In Review".
#
#   "Needs Fixes - E2E":
#     # Heavier lane: e2e failures usually need investigation and context.
#     provider: claude_code
#     model: claude-opus-4-7
#     turn_timeout_ms: 3600000
#     prompt: |
#       The PR for {{ issue.identifier }} is failing an e2e check (attempt
#       {{ issue.fields.sinfonia_attempt_count | default: "?" }} of
#       {{ issue.fields.sinfonia_max_attempts | default: "5" }}).
#
#       Failure category: {{ issue.fields.sinfonia_failure_category | default: "e2e" }}
#       Last failure log:
#       {{ issue.fields.sinfonia_last_ci_failure | default: "(see PR checks)" }}
#
#       Investigate the failing test, fix the underlying issue (don't just stub the
#       assertion), push, and transition back to "In Review".
#
#   "Needs Fixes":
#     # Catchall: the bridge routes here when no failure-category pattern matched.
#     provider: claude_code
#     model: claude-sonnet-4-6
#     prompt: |
#       The PR for {{ issue.identifier }} is failing CI (attempt
#       {{ issue.fields.sinfonia_attempt_count | default: "?" }}).
#
#       Last failure log:
#       {{ issue.fields.sinfonia_last_ci_failure | default: "(see PR checks)" }}
#
#       Fix the failure, push, and transition back to "In Review".
#
#   "In Review":
#     # Optional: a small raw-LLM pass that responds to human review comments.
#     # Skip if your team prefers the agent to stop once CI is green.
#     provider: anthropic
#     model: claude-sonnet-4-6
#     prompt: |
#       Address the latest review comments on the PR for {{ issue.identifier }}.
#       Don't change anything that wasn't called out. When all comments are addressed,
#       leave a status comment and stop.

# ---- Optional HTTP dashboard ----
server:
  port: 8080
  bind: 127.0.0.1

# ---- Optional OpenTelemetry emission ----
# OPT-IN. When `otlp_endpoint` is unset AND `OTEL_EXPORTER_OTLP_ENDPOINT`
# is unset in the environment, the OTel layer is disabled and the daemon
# runs stdout-only — identical to v0.3.0-alpha.1 behaviour.
#
# When configured, the daemon emits six spans (orchestrator.tick,
# orchestrator.dispatch, runner.session, runner.turn, workspace.hook,
# tracker.fetch). Every span carries the resolved tenant_id (precedence:
# this field → SINFONIA_TENANT_ID env → "default"). Resource-level
# `service.namespace = tenant_id` lets a Collector routing-processor
# split per-tenant data without touching emission code.
#
# `sinfonia_events_secret` enables the typed Sinfonia↔bridge event channel
# (SPEC §11.6.11). The bridge subscribes via POST /api/v1/events/subscribers
# at startup; the daemon HMAC-signs each runner.session.completed event and
# POSTs it to every registered subscriber.
#
# telemetry:
#   service_name: sinfonia
#   tenant_id: acme-web-app
#   otlp_endpoint: http://otel-collector:4318
#   sinfonia_events_secret: $SINFONIA_EVENTS_SECRET
---

You are picking up issue `{{ issue.identifier }}`: {{ issue.title }}.

This is the *default* prompt body — it runs for any state without a per-state `prompt:` in
the `states:` block above.

{% if attempt %}
This is attempt {{ attempt }} of this run. The previous attempt did not finish.
{% endif %}

## Issue description

{{ issue.description }}

## What to do

1. Inspect the workspace.
2. Make focused changes to address the issue. Prefer minimal diffs.
3. Run the project's tests and linters if they exist.
4. Use the `finish` tool (raw-LLM backends) or transition the ticket (CLI backends) when done.
