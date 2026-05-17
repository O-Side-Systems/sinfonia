---
# ============================================================================
# Boardgame Companion — Sinfonia automation contract
#
# This file is meant to live in the *boardgame companion repo root*, committed
# and reviewed via PR like any other code. Secrets are env-var references
# resolved at parse time; the real values live in the operator's shell / .env.
# ============================================================================

# ---- Tracker ----
tracker:
  kind: linear
  api_key: $LINEAR_API_KEY
  project_slug: board-game-companion-framework-3b7b5224d0cc
  active_states: ["Todo", "In Progress"]
  terminal_states: ["Done", "Cancelled"]
  # "In Review" is intentionally absent from both lists — it's the human gate.
  # Moving a ticket there stops the agent session but KEEPS the workspace.

polling:
  interval_ms: 10000           # 10s during testing; raise to 30000 for normal use

workspace:
  # Resolves to /home/dev/sinfonia-workspaces inside the docker container.
  root: ~/sinfonia-workspaces

# ---- Lifecycle hooks (run as `bash -lc`, cwd = workspace) ----
# IMPORTANT: hooks are NOT Liquid-rendered (only prompt bodies are). The cwd
# is named after the sanitized issue identifier, so derive everything you need
# from `$PWD` rather than `{{ issue.* }}`.
hooks:
  timeout_ms: 180000

  # First-time workspace bootstrap. `gh` uses $GH_TOKEN from the env.
  after_create: |
    gh repo clone O-Side-Systems/BCF .

  # Runs before every attempt — must be idempotent (retries re-run it).
  before_run: |
    branch="sinfonia/$(basename "$PWD" | tr '[:upper:]' '[:lower:]')"
    git fetch --all --quiet
    git switch -c "$branch" 2>/dev/null || git switch "$branch"

  after_run: |
    git status -s || true

# ---- Default agent (fallback for any state without an override) ----
agent:
  max_concurrent_agents: 2
  max_concurrent_agents_by_state:
    "In Progress": 1
  max_turns: 8
  max_retry_backoff_ms: 300000

  provider: claude_code         # `claude` CLI; auth via mounted ~/.claude
  model: claude-sonnet-4-6
  # command: defaults to "claude -p --output-format json --verbose --dangerously-skip-permissions"
  turn_timeout_ms: 3600000      # 60 min
  # NOTE: sinfonia only extracts `usage` from the terminal `result` event,
  # so `tokens` stays 0 mid-run and the stall detector false-positives. Raised
  # from 5min to 60min as a workaround until that parsing is fixed.
  stall_timeout_ms: 3600000     # 60 min

# ---- State machine ----
# Todo        → scout + plan + first cut, then transition to In Progress
# In Progress → implement + test + push branch + open PR, then transition to In Review
# In Review   → (not active) human reviews the PR; merge is human-only
states:
  "Todo":
    provider: claude_code
    model: claude-sonnet-4-6
    prompt: |
      You are picking up a fresh Linear issue: **{{ issue.identifier }} — {{ issue.title }}**.

      ## Issue description

      {{ issue.description }}

      ## Orient first (do this every time)

      - Read `README.md` and `CLAUDE.md` (if present).
      - Skim `docs/` for architecture notes.
      - List the top of `src/` (or equivalent) to learn the module layout.
      - Confirm the test runner works (`npm test`, `cargo test`, `pytest`, etc.).
      {% if issue.children.size > 0 %}
      ## This issue is a PARENT — verify integration of all sub-issues first

      The orchestrator only dispatched this parent because every child issue
      reached a terminal state. Before doing your own work, confirm the
      children's contributions are actually integrated:

      {% for c in issue.children %}- `{{ c.identifier }}` ({{ c.state }})
      {% endfor %}
      Steps:
      - For each child, locate its branch (`sinfonia/{{ c.identifier | downcase }}` convention).
      - Confirm its PR is merged to `main` (or note which aren't and STOP — report back).
      - Pull `main`, run the full test suite, ensure children's work composes cleanly.
      - Only then continue with the parent-level work below.
      {% endif %}
      ## Then, on this pass

      1. Sketch a short plan in `.sinfonia/plan.md` (~10 bullets, not an essay).
         Create the directory if it doesn't exist.
      2. Make a minimal first cut — compiles/runs, may be incomplete.
      3. Commit work-in-progress on `sinfonia/{{ issue.identifier | downcase }}` with a
         descriptive message referencing `{{ issue.identifier }}`.
      4. Transition the Linear issue to **In Progress** using the GraphQL API,
         then **verify the state actually changed**. `jq` is installed.

         ```bash
         set -e
         AUTH="Authorization: $LINEAR_API_KEY"
         CT="Content-Type: application/json"
         API=https://api.linear.app/graphql

         # Resolve identifier → UUID + team
         ISSUE_JSON=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
           -d '{"query":"{ issue(id:\"{{ issue.identifier }}\"){ id team { key } state { name } } }"}')
         ISSUE_UUID=$(echo "$ISSUE_JSON" | jq -er '.data.issue.id')
         TEAM_KEY=$(echo "$ISSUE_JSON" | jq -er '.data.issue.team.key')

         # Resolve target state id for this team (state names repeat across teams)
         STATE_JSON=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
           -d "{\"query\":\"{ workflowStates(filter:{team:{key:{eq:\\\"$TEAM_KEY\\\"}},name:{eq:\\\"In Progress\\\"}}){ nodes { id } } }\"}")
         STATE_ID=$(echo "$STATE_JSON" | jq -er '.data.workflowStates.nodes[0].id')

         # Mutate
         MUT_JSON=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
           -d "{\"query\":\"mutation{ issueUpdate(id:\\\"$ISSUE_UUID\\\", input:{stateId:\\\"$STATE_ID\\\"}){ success } }\"}")
         echo "$MUT_JSON" | jq -e '.data.issueUpdate.success == true and (.errors // empty | length == 0)' \
           || { echo "MUTATION FAILED: $MUT_JSON"; exit 1; }

         # Verify by re-querying
         VERIFY=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
           -d "{\"query\":\"{ issue(id:\\\"$ISSUE_UUID\\\"){ state { name } } }\"}")
         NEW=$(echo "$VERIFY" | jq -er '.data.issue.state.name')
         [ "$NEW" = "In Progress" ] || { echo "VERIFY FAILED: state=$NEW"; exit 1; }
         echo "OK: {{ issue.identifier }} → In Progress"
         ```

         **Do not claim the transition succeeded in your final message unless the
         verification step above prints `OK:`.** If it failed, report what happened
         (the JSON response) instead of pretending it worked.

  "In Progress":
    provider: claude_code
    model: claude-opus-4-7      # stronger model for the implementation pass
    turn_timeout_ms: 5400000    # 90 min
    prompt: |
      Resume implementation of **{{ issue.identifier }} — {{ issue.title }}**.

      The previous turn left state in this workspace and a plan in `.sinfonia/plan.md`.

      ## What to do

      1. Pick up from `.sinfonia/plan.md` and complete the implementation.
      2. Run the project's tests + linters. Iterate until green.
      3. Commit cleanly. Reference `{{ issue.identifier }}` in the message.
      4. Push: `git push -u origin "sinfonia/{{ issue.identifier | downcase }}"`.
      5. Open a PR (or update the existing one). Verify the URL afterwards.

         ```bash
         set -e
         BRANCH="sinfonia/{{ issue.identifier | downcase }}"
         EXISTING=$(gh pr list --head "$BRANCH" --json url -q '.[0].url')
         if [ -z "$EXISTING" ]; then
           PR_URL=$(gh pr create \
             --title "{{ issue.identifier }}: {{ issue.title }}" \
             --body  $'Resolves {{ issue.identifier }}.\n\nGenerated by sinfonia.')
         else
           PR_URL="$EXISTING"
         fi
         echo "PR: $PR_URL"
         [ -n "$PR_URL" ] || { echo "PR CREATE FAILED"; exit 1; }
         ```

      6. Transition the Linear issue to **In Review** using the same
         verify-then-claim pattern as the Todo prompt (swap `"In Progress"` for
         `"In Review"`). Do not claim success unless `OK:` prints. Then stop.

      ## Don't

      - Don't merge the PR yourself.
      - Don't push to `main` directly.
      - Don't touch files unrelated to this issue.

# ---- HTTP dashboard ----
server:
  # 0.0.0.0 because sinfonia runs inside a docker container with port 8080 published.
  # If you run sinfonia directly on a workstation, switch this back to 127.0.0.1.
  bind: 0.0.0.0
  port: 8080
---

You are picking up Linear issue `{{ issue.identifier }}`: {{ issue.title }}.

This is the *default* prompt body — it runs only when an issue is in a state that has no
per-state `prompt:` override above. With the current state machine, every Todo or In Progress
issue uses its state-specific prompt; this fallback exists so a brand-new state we haven't
tuned for still behaves sensibly.

{% if attempt %}
This is attempt {{ attempt }} of this run. The previous attempt did not finish — inspect the
workspace before starting fresh.
{% endif %}

## Issue description

{{ issue.description }}

## What to do

1. Orient: `README.md`, `CLAUDE.md`, `docs/`.
2. Make focused changes — minimal diffs.
3. Run tests + linters.
4. Commit on `sinfonia/{{ issue.identifier | downcase }}` and push.
5. Open a PR with `gh` referencing `{{ issue.identifier }}`.
6. Stop. Don't transition the ticket from this fallback path.
