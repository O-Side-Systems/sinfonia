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
hooks:
  timeout_ms: 180000

  # First-time workspace bootstrap. `gh` uses $GH_TOKEN from the env.
  after_create: |
    gh repo clone O-Side-Systems/BCF .

  # Runs before every attempt — must be idempotent (retries re-run it).
  before_run: |
    git fetch --all --quiet
    git switch -c "sinfonia/{{ issue.identifier | downcase }}" 2>/dev/null \
      || git switch "sinfonia/{{ issue.identifier | downcase }}"

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
  stall_timeout_ms: 300000      # 5 min idle kill

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

      ## Then, on this pass

      1. Sketch a short plan in `.sinfonia/plan.md` (~10 bullets, not an essay).
         Create the directory if it doesn't exist.
      2. Make a minimal first cut — compiles/runs, may be incomplete.
      3. Commit work-in-progress on `sinfonia/{{ issue.identifier | downcase }}` with a
         descriptive message referencing `{{ issue.identifier }}`.
      4. Transition the Linear issue to **In Progress** using the GraphQL API.
         Token is in `$LINEAR_API_KEY`. Two-step pattern:

         ```bash
         # Find the workflow state id (cache it after the first lookup):
         curl -sS https://api.linear.app/graphql \
           -H "Authorization: $LINEAR_API_KEY" -H "Content-Type: application/json" \
           -d '{"query":"{ workflowStates(filter:{name:{eq:\"In Progress\"}}) { nodes { id name team { key } } } }"}'

         # Then move the issue (use the issue's UUID, not its identifier):
         curl -sS https://api.linear.app/graphql \
           -H "Authorization: $LINEAR_API_KEY" -H "Content-Type: application/json" \
           -d '{"query":"mutation($id:String!,$s:String!){issueUpdate(id:$id,input:{stateId:$s}){success}}","variables":{"id":"<ISSUE_UUID>","s":"<STATE_ID>"}}'
         ```

         To resolve `{{ issue.identifier }}` → UUID:

         ```bash
         curl -sS https://api.linear.app/graphql \
           -H "Authorization: $LINEAR_API_KEY" -H "Content-Type: application/json" \
           -d '{"query":"{ issue(id:\"{{ issue.identifier }}\") { id } }"}'
         ```

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
      5. Open a PR with `gh`. Linear auto-links when the identifier appears in the body:

         ```bash
         gh pr create \
           --title "{{ issue.identifier }}: {{ issue.title }}" \
           --body  $'Resolves {{ issue.identifier }}.\n\nGenerated by sinfonia.' \
           --label bot:sinfonia
         ```

      6. Transition the Linear issue to **In Review** (same GraphQL pattern as Todo).
         Then stop — your job is done.

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
