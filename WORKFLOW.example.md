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
      2. Sketch a plan in `.sinfonia/plan.md` (create the dir if needed).
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

      The previous turn left state in this workspace and in `.sinfonia/plan.md`. Pick up
      from there. Run the project's tests. When the change is complete and tests pass,
      transition the issue to "In Review" in the tracker.

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
# OpenCode (https://opencode.ai) is a coding-agent CLI with LSP, MCP, and 75+ provider
# backends. Like `claude_code` and `codex`, Sinfonia drives it as a subprocess in the
# workspace.
#
# Auth: OpenCode owns its own credentials — Sinfonia does NOT pass an api_key. Run
# `opencode auth login` once on the host before starting the daemon. The `model:` field
# is passed through verbatim as `--model <name>`; OpenCode's wire format is
# `provider/model` (e.g. `anthropic/claude-sonnet-4-6`, `ollama/qwen2.5-coder:32b`).
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

# ---- Optional HTTP dashboard ----
server:
  port: 8080
  bind: 127.0.0.1
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
