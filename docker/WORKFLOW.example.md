---
# ============================================================================
# Sinfonia automation contract — docker-friendly template
#
# HOW TO USE
#   Copy this file into your target repo's root as `WORKFLOW.md`, then:
#     1. Set `tracker.project_slug` to your Linear project's slug (the part
#        of the URL after `/project/`, including the trailing hash).
#     2. Update the `after_create` hook to clone your repo (or any other
#        bootstrap your project needs).
#     3. Add your `LINEAR_API_KEY` / `GH_TOKEN` to `.env` (the docker-compose
#        in this directory forwards them into the container automatically).
#
# Secrets are env-var references resolved at parse time; the real values
# live in the operator's shell / .env, never in this file.
#
# Owner/repo for the GitHub queries in the prompts are derived at runtime via
# `gh repo view` against the workspace cwd, so this file is portable across
# projects without editing the prompt bodies.
# ============================================================================

# ---- Tracker ----
tracker:
  kind: linear
  api_key: $LINEAR_API_KEY
  project_slug: your-linear-project-slug-here   # ← change me
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
    gh repo clone YOUR-ORG/YOUR-REPO .   # ← change me
    # Per-workspace scratchpad lives in .sinfonia/ — never commit it.
    # `.gitignore` write is idempotent across re-creations.
    touch .gitignore
    grep -qxF '.sinfonia/' .gitignore || echo '.sinfonia/' >> .gitignore

  # Runs before every attempt — must be idempotent (retries re-run it).
  before_run: |
    branch="sinfonia/$(basename "$PWD" | tr '[:upper:]' '[:lower:]')"
    # Heal a half-finished rebase from a prior turn (a conflict path may have
    # ended the turn mid-rebase); otherwise `git switch` below refuses and the
    # workspace wedges. No-op when no rebase is in progress.
    git rebase --abort 2>/dev/null || true
    git fetch --all --quiet
    git switch -c "$branch" 2>/dev/null || git switch "$branch"

  after_run: |
    git status -s || true

# ---- Default agent (fallback for any state without an override) ----
agent:
  max_concurrent_agents: 2
  max_concurrent_agents_by_state:
    # serial-foundation convention (docs/HARNESS-SPEC.md §7.4): one foundational
    # story at a time — each lands on `main` before the next begins.
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
      You are picking up Linear issue **{{ issue.identifier }} — {{ issue.title }}**.

      ## Issue description

      {{ issue.description }}

      ## STEP 0 — Detect prior work (run this every time, before deciding anything)

      A human may have moved this ticket back to Todo to ask you to address PR
      feedback, fix merge conflicts, or respond to CI failures. Find that out
      first.

      ```bash
      set +e   # don't abort on missing PR / empty results
      BRANCH="sinfonia/{{ issue.identifier | downcase }}"

      # Derive owner/repo from the workspace's git remote so this prompt is portable.
      OWNER=$(gh repo view --json owner -q .owner.login)
      REPO=$(gh repo view --json name  -q .name)

      # --- BLOCK-01: Blocker-merged guardrail ---
      BLOCKERS_JSON=$(curl -sS \
        -H "Authorization: $LINEAR_API_KEY" \
        -H "Content-Type: application/json" \
        https://api.linear.app/graphql \
        -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { inverseRelations(first: 50) { nodes { type issue { identifier state { name } } } } } }\"}")
      BLOCKERS=$(echo "$BLOCKERS_JSON" \
        | jq -r '.data.issue.inverseRelations.nodes[] | select(.type == "blocks") | [.issue.identifier, (.issue.state.name // "")] | @tsv' 2>/dev/null || true)

      if [ -n "$BLOCKERS" ]; then
        UNMERGED=""
        while IFS=$'\t' read -r BLOCKER_ID BLOCKER_STATE; do
          [ -z "$BLOCKER_ID" ] && continue
          # Skip self/cyclic blocks
          if [ "$BLOCKER_ID" = "{{ issue.identifier }}" ]; then continue; fi
          # Validate identifier shape before interpolating into shell args
          if ! echo "$BLOCKER_ID" | grep -qE '^[A-Z]+-[0-9]+$'; then continue; fi
          BLOCKER_BRANCH="sinfonia/$(echo "$BLOCKER_ID" | tr '[:upper:]' '[:lower:]')"
          # NOTE: assumes the sinfonia/<id> branch convention; a blocker PR on another branch reads as unmerged (fails closed).
          MERGED_COUNT=$(gh pr list \
            --repo "$OWNER/$REPO" \
            --head "$BLOCKER_BRANCH" \
            --base main \
            --state merged \
            --json number \
            -q 'length' 2>/dev/null || echo "0")
          if [ "$MERGED_COUNT" = "0" ]; then
            UNMERGED="${UNMERGED:+$UNMERGED, }$BLOCKER_ID"
          fi
        done <<< "$BLOCKERS"

        if [ -n "$UNMERGED" ]; then
          MARKER="sinfonia-bot: blocked-by-unmerged"
          EXISTING_MARKERS=$(curl -sS \
            -H "Authorization: $LINEAR_API_KEY" \
            -H "Content-Type: application/json" \
            https://api.linear.app/graphql \
            -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { comments(first: 50) { nodes { body } } } }\"}" \
            | jq -r '.data.issue.comments.nodes[].body' 2>/dev/null || true)
          if ! printf '%s' "$EXISTING_MARKERS" | grep -qF "$MARKER"; then
            ISSUE_UUID=$(curl -sS \
              -H "Authorization: $LINEAR_API_KEY" \
              -H "Content-Type: application/json" \
              https://api.linear.app/graphql \
              -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { id } }\"}" \
              | jq -er '.data.issue.id')
            COMMENT_BODY="${MARKER}\\n\\nThis issue cannot start: the following blockers have not been merged to \`main\`: ${UNMERGED}\\n\\nRe-evaluating next poll. No code changes were made."
            curl -sS \
              -H "Authorization: $LINEAR_API_KEY" \
              -H "Content-Type: application/json" \
              https://api.linear.app/graphql \
              -d "{\"query\":\"mutation { commentCreate(input: { issueId: \\\"$ISSUE_UUID\\\", body: \\\"$COMMENT_BODY\\\" }) { success } }\"}"
          fi
          echo "BLOCKED: unmerged blockers: $UNMERGED — exiting without code changes."
          exit 0
        fi
      fi
      # --- END BLOCK-01 ---

      echo "=== git state on $BRANCH ==="
      git log --oneline main..HEAD 2>/dev/null | head -20 || echo "(no commits yet)"
      git status -s

      echo
      echo "=== existing PR for $BRANCH ==="
      PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number' 2>/dev/null)
      if [ -n "$PR_NUM" ]; then
        echo "Found PR #$PR_NUM"
        gh pr view "$PR_NUM" --json state,mergeable,mergeStateStatus,reviewDecision

        echo
        echo "=== unresolved review threads (via graphql) ==="
        gh api graphql -F owner="$OWNER" -F name="$REPO" -F num="$PR_NUM" -f query='
          query($owner:String!,$name:String!,$num:Int!){
            repository(owner:$owner,name:$name){
              pullRequest(number:$num){
                reviewThreads(first:50){
                  nodes{ isResolved path line
                    comments(first:10){ nodes{ author{login} body } }
                  }
                }
              }
            }
          }' \
          | jq '.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved == false) | {path, line, comments: [.comments.nodes[] | {author: .author.login, body}]}'

        echo
        echo "=== conversation comments (top-level) ==="
        gh pr view "$PR_NUM" --comments | tail -200

        echo
        echo "=== failing CI checks ==="
        gh pr checks "$PR_NUM" 2>/dev/null | grep -viE '^\s*(pass|skipping)' | head -20
      else
        echo "(no PR exists for this branch yet)"
      fi

      echo
      echo "=== Linear comments on {{ issue.identifier }} ==="
      curl -sS -H "Authorization: $LINEAR_API_KEY" -H "Content-Type: application/json" \
        https://api.linear.app/graphql \
        -d '{"query":"{ issue(id:\"{{ issue.identifier }}\"){ comments(orderBy:createdAt){ nodes{ user{ name } body createdAt } } } }"}' \
        | jq -r '.data.issue.comments.nodes[] | "[\(.createdAt) \(.user.name // "unknown")]\n\(.body)\n---"'
      set -e
      ```

      > ⛔ **If the STEP 0 output above contains `BLOCKED: unmerged blockers`, you are DONE for this turn.** Do NOT continue to STEP 1 or any later step. Make NO code changes, do NOT open or modify a PR, and do NOT transition the issue. The blocker comment (if needed) was already posted by the guardrail. End your turn now — the orchestrator will re-dispatch on the next poll once blockers merge.

      ## STEP 1 — Decide work mode from what STEP 0 showed

      **If `reviewDecision == "CHANGES_REQUESTED"` OR `mergeable ==
      "CONFLICTING"` OR there are unresolved review threads OR there are
      conversation comments asking for changes: YOU ARE NOT DONE.** Do not
      transition the issue forward (to In Progress or In Review) until you
      have actually addressed what was raised. Skipping straight to a state
      transition without code changes will be caught by the human and the
      ticket will come back to you.

      Pick the FIRST matching bullet — they are ranked by urgency:

      1. **Merge conflicts** (`mergeStateStatus` is `DIRTY` / `BEHIND` /
         `BLOCKED` with conflicts noted, or `mergeable == "CONFLICTING"`):
         your job is to resolve them.
         - Follow the **Mergeability procedure** below. Resolve conflicts
           touching only this issue's files. `git push --force-with-lease`.
         - Comment on the PR summarizing what you resolved (`gh pr comment $PR_NUM --body "..."`).
         - Re-check `mergeStateStatus`. If conflict-free (`BLOCKED`, `UNSTABLE`,
           or `CLEAN`), transition the issue to **In Review** (use the
           verify-then-claim pattern in step 4 below).
         - Do NOT touch unrelated code or open a new PR.

      2. **Unresolved review threads or conversation comments asking for changes**:
         your job is to address each one.
         - Read every unresolved thread. Make the requested change in code.
         - For each thread, reply explaining what you did:
           `gh api --method POST repos/{owner}/{repo}/pulls/$PR_NUM/comments/$COMMENT_ID/replies -f body="..."`
           (or `gh pr comment $PR_NUM --body "Re: <thread topic> — ..."` if you
           can't address the thread API).
         - Commit + push to the same branch. Do NOT create a new PR.
         - Transition the issue back to **In Review** when all unresolved
           threads have a reply explaining the fix.

      3. **Failing CI**: read the failing check, fix it, push. Then re-check.

      4. **PR exists, no feedback, no conflicts, CI green**: the human likely
         moved the ticket back by mistake. Leave a status comment on the PR
         (`gh pr comment $PR_NUM --body "No outstanding feedback detected;
         awaiting human review."`) and transition the issue back to **In Review**.
         Do not redo prior work.

      5. **No PR exists**: this is a genuinely fresh start. Proceed to the
         "Fresh work" section below.

      ## Fresh work (only when STEP 1 said "no PR exists")

      1. Sketch a short plan in `.sinfonia/plans/{{ issue.identifier | downcase }}.md`
         (~10 bullets, not an essay). Create the directory if it doesn't exist.
         The `.sinfonia/` tree is your per-workspace scratchpad — `after_create`
         already added it to `.gitignore`, so do NOT `git add .sinfonia/`. If
         the `.gitignore` entry got dropped, add it back as your first commit
         so it lands on the branch you push. Using a per-issue filename here
         is belt-and-suspenders: even if scratchpads accidentally land in git,
         per-issue paths don't merge-conflict against parallel agent branches.
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
      Resume work on **{{ issue.identifier }} — {{ issue.title }}**.

      ## STEP 0 — Detect prior work (run this every time)

      Same drill as the Todo prompt — the human may have moved this back to
      In Progress to ask you to fix something. Run all of these checks before
      assuming you're just continuing fresh implementation:

      ```bash
      set +e
      BRANCH="sinfonia/{{ issue.identifier | downcase }}"

      # Derive owner/repo from the workspace's git remote so this prompt is portable.
      OWNER=$(gh repo view --json owner -q .owner.login)
      REPO=$(gh repo view --json name  -q .name)

      # --- BLOCK-01: Blocker-merged guardrail ---
      BLOCKERS_JSON=$(curl -sS \
        -H "Authorization: $LINEAR_API_KEY" \
        -H "Content-Type: application/json" \
        https://api.linear.app/graphql \
        -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { inverseRelations(first: 50) { nodes { type issue { identifier state { name } } } } } }\"}")
      BLOCKERS=$(echo "$BLOCKERS_JSON" \
        | jq -r '.data.issue.inverseRelations.nodes[] | select(.type == "blocks") | [.issue.identifier, (.issue.state.name // "")] | @tsv' 2>/dev/null || true)

      if [ -n "$BLOCKERS" ]; then
        UNMERGED=""
        while IFS=$'\t' read -r BLOCKER_ID BLOCKER_STATE; do
          [ -z "$BLOCKER_ID" ] && continue
          # Skip self/cyclic blocks
          if [ "$BLOCKER_ID" = "{{ issue.identifier }}" ]; then continue; fi
          # Validate identifier shape before interpolating into shell args
          if ! echo "$BLOCKER_ID" | grep -qE '^[A-Z]+-[0-9]+$'; then continue; fi
          BLOCKER_BRANCH="sinfonia/$(echo "$BLOCKER_ID" | tr '[:upper:]' '[:lower:]')"
          # NOTE: assumes the sinfonia/<id> branch convention; a blocker PR on another branch reads as unmerged (fails closed).
          MERGED_COUNT=$(gh pr list \
            --repo "$OWNER/$REPO" \
            --head "$BLOCKER_BRANCH" \
            --base main \
            --state merged \
            --json number \
            -q 'length' 2>/dev/null || echo "0")
          if [ "$MERGED_COUNT" = "0" ]; then
            UNMERGED="${UNMERGED:+$UNMERGED, }$BLOCKER_ID"
          fi
        done <<< "$BLOCKERS"

        if [ -n "$UNMERGED" ]; then
          MARKER="sinfonia-bot: blocked-by-unmerged"
          EXISTING_MARKERS=$(curl -sS \
            -H "Authorization: $LINEAR_API_KEY" \
            -H "Content-Type: application/json" \
            https://api.linear.app/graphql \
            -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { comments(first: 50) { nodes { body } } } }\"}" \
            | jq -r '.data.issue.comments.nodes[].body' 2>/dev/null || true)
          if ! printf '%s' "$EXISTING_MARKERS" | grep -qF "$MARKER"; then
            ISSUE_UUID=$(curl -sS \
              -H "Authorization: $LINEAR_API_KEY" \
              -H "Content-Type: application/json" \
              https://api.linear.app/graphql \
              -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { id } }\"}" \
              | jq -er '.data.issue.id')
            COMMENT_BODY="${MARKER}\\n\\nThis issue cannot start: the following blockers have not been merged to \`main\`: ${UNMERGED}\\n\\nRe-evaluating next poll. No code changes were made."
            curl -sS \
              -H "Authorization: $LINEAR_API_KEY" \
              -H "Content-Type: application/json" \
              https://api.linear.app/graphql \
              -d "{\"query\":\"mutation { commentCreate(input: { issueId: \\\"$ISSUE_UUID\\\", body: \\\"$COMMENT_BODY\\\" }) { success } }\"}"
          fi
          echo "BLOCKED: unmerged blockers: $UNMERGED — exiting without code changes."
          exit 0
        fi
      fi
      # --- END BLOCK-01 ---

      echo "=== git state on $BRANCH ==="
      git log --oneline main..HEAD 2>/dev/null | head -20 || echo "(no commits yet)"
      git status -s

      echo
      echo "=== existing PR ==="
      PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number' 2>/dev/null)
      if [ -n "$PR_NUM" ]; then
        echo "Found PR #$PR_NUM"
        gh pr view "$PR_NUM" --json state,mergeable,mergeStateStatus,reviewDecision
        echo
        echo "=== unresolved review threads (via graphql) ==="
        gh api graphql -F owner="$OWNER" -F name="$REPO" -F num="$PR_NUM" -f query='
          query($owner:String!,$name:String!,$num:Int!){
            repository(owner:$owner,name:$name){
              pullRequest(number:$num){
                reviewThreads(first:50){
                  nodes{ isResolved path line
                    comments(first:10){ nodes{ author{login} body } }
                  }
                }
              }
            }
          }' \
          | jq '.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved == false) | {path, line, comments: [.comments.nodes[] | {author: .author.login, body}]}'
        echo
        echo "=== conversation comments ==="
        gh pr view "$PR_NUM" --comments | tail -200
        echo
        echo "=== failing CI checks ==="
        gh pr checks "$PR_NUM" 2>/dev/null | grep -viE '^\s*(pass|skipping)' | head -20
      fi

      echo
      echo "=== Linear comments on {{ issue.identifier }} ==="
      curl -sS -H "Authorization: $LINEAR_API_KEY" -H "Content-Type: application/json" \
        https://api.linear.app/graphql \
        -d '{"query":"{ issue(id:\"{{ issue.identifier }}\"){ comments(orderBy:createdAt){ nodes{ user{ name } body createdAt } } } }"}' \
        | jq -r '.data.issue.comments.nodes[] | "[\(.createdAt) \(.user.name // "unknown")]\n\(.body)\n---"'
      set -e
      ```

      > ⛔ **If the STEP 0 output above contains `BLOCKED: unmerged blockers`, you are DONE for this turn.** Do NOT continue to STEP 1 or any later step. Make NO code changes, do NOT open or modify a PR, and do NOT transition the issue. The blocker comment (if needed) was already posted by the guardrail. End your turn now — the orchestrator will re-dispatch on the next poll once blockers merge.

      ## STEP 1 — If STEP 0 surfaced any of these, handle them FIRST

      **If `reviewDecision == "CHANGES_REQUESTED"` OR `mergeable ==
      "CONFLICTING"` OR there are unresolved review threads OR comments
      asking for changes: YOU ARE NOT DONE.** Do not transition the issue to
      In Review or anywhere else until the requested changes are actually in
      code and pushed. Don't substitute a state transition for real work.

      - **Merge conflicts** → follow the **Mergeability procedure** below.
        Resolve conflicts in this issue's files only; comment on the PR summarizing
        the resolution. Do NOT touch unrelated code or open a new PR.
      - **Unresolved review threads** → address each comment in code, push,
        reply to each thread explaining what you did.
      - **Failing CI** → read the failing check, fix, push.
      - **Human comments on the Linear issue raising concerns** → respond to
        them with a Linear comment via the GraphQL `commentCreate` mutation,
        then act on whatever was raised.

      Only proceed to the implementation-continuation steps below if STEP 0
      showed clean state (no unresolved threads, mergeable, CI green) — in
      that case the human moved the ticket in to nudge progress.

      The previous turn (if any) left state in this workspace and possibly a
      plan in `.sinfonia/plans/{{ issue.identifier | downcase }}.md`. That tree is
      gitignored scratch — don't commit it.

      ## Implementation continuation (when STEP 1 had nothing to address)

      1. Pick up from `.sinfonia/plans/{{ issue.identifier | downcase }}.md` and
         complete the implementation.
      2. Run the project's tests + linters. Iterate until green.
      3. Commit cleanly. Reference `{{ issue.identifier }}` in the message.

      4. **Pre-PR gate (MERGE-01): follow the Mergeability procedure before pushing.**
         Fetch `origin/main`, rebase, resolve any conflicts (this issue's files only),
         re-run the repo-discovered gate. The gate MUST be green before you push.

         If the gate is still red after rebasing (conflicts unresolvable OR gate fails):

         ```bash
         set -e
         BRANCH="sinfonia/{{ issue.identifier | downcase }}"
         MARKER="sinfonia-bot: pre-pr-gate-red"
         AUTH="Authorization: $LINEAR_API_KEY"
         CT="Content-Type: application/json"
         API=https://api.linear.app/graphql

         EXISTING_MARKERS=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
           -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { comments(first: 50) { nodes { body } } } }\"}" \
           | jq -r '.data.issue.comments.nodes[].body' 2>/dev/null || true)
         if ! printf '%s' "$EXISTING_MARKERS" | grep -qF "$MARKER"; then
           ISSUE_UUID=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
             -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { id } }\"}" \
             | jq -er '.data.issue.id')
           COMMENT_BODY="${MARKER}\\n\\nPre-PR gate is red after rebasing on \`origin/main\`. The branch has not been pushed and no PR has been opened. Conflicts or gate failures must be resolved before pushing. Re-evaluating next poll."
           curl -sS -H "$AUTH" -H "$CT" "$API" \
             -d "{\"query\":\"mutation { commentCreate(input: { issueId: \\\"$ISSUE_UUID\\\", body: \\\"$COMMENT_BODY\\\" }) { success } }\"}"
         fi
         echo "GATE-RED: pre-PR gate failed — no push, no PR opened. Staying In Progress."
         exit 0
         ```

         > ⛔ **If the pre-PR gate is red, you are DONE for this turn.** Do NOT push the
         > branch. Do NOT open a PR. Do NOT transition the issue. The comment above
         > (if needed) was posted. End your turn — the orchestrator will re-dispatch.

      5. Push: `git push -u origin "sinfonia/{{ issue.identifier | downcase }}"`.

      6. Open a PR (or update the existing one). Verify the URL afterwards.

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
         PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number')
         ```

      7. **Mergeability loop (MERGE-02): gate the In Review transition.**
         Poll `mergeStateStatus` and loop up to 3 times using the Mergeability
         procedure on `DIRTY`/`BEHIND`; re-poll on `UNKNOWN`; proceed on
         `BLOCKED`/`UNSTABLE`/`CLEAN`.

         ```bash
         set -e
         BRANCH="sinfonia/{{ issue.identifier | downcase }}"
         AUTH="Authorization: $LINEAR_API_KEY"
         CT="Content-Type: application/json"
         API=https://api.linear.app/graphql
         MAX_ATTEMPTS=3
         ATTEMPT=0
         UNKNOWN_TRIES=0
         WEIRD_TRIES=0

         # Re-derive PR_NUM here so this block is self-contained (WR-03/WR-04).
         PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number')
         [ -n "$PR_NUM" ] || { echo "PR_NUM unset — cannot evaluate mergeability; staying In Progress."; exit 0; }

         while [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; do
           MSS=$(gh pr view "$PR_NUM" \
             --json state,mergeable,mergeStateStatus,reviewDecision \
             | jq -r '.mergeStateStatus')
           echo "mergeStateStatus (attempt $((ATTEMPT+1))/$MAX_ATTEMPTS): $MSS"

           case "$MSS" in
             DIRTY|BEHIND)
               ATTEMPT=$((ATTEMPT + 1))
               if [ "$ATTEMPT" -ge "$MAX_ATTEMPTS" ]; then break; fi
               echo "Conflict detected ($MSS) — running Mergeability procedure (attempt $ATTEMPT/$MAX_ATTEMPTS)..."
               git fetch origin
               # Guard the rebase: a DIRTY/BEHIND branch means real conflicts; set -e would
               # abort mid-rebase on failure, leaving a broken workspace.
               if ! git rebase origin/main; then
                 echo "Rebase hit conflicts — resolve THIS issue's files only, then:"
                 echo "  git add <file> && git rebase --continue"
                 echo "Re-run the repo gate; only force-push if it is green. Staying In Progress."
                 exit 0   # done for this turn — resolve conflicts, re-enter next poll
               fi
               # Gate BEFORE push, using the SAME procedure as the Mergeability procedure below.
               # Re-run the repo's own gate — the command its CI runs. Discover it from
               # .github/workflows/, README, or the harness's documented command. Do NOT
               # hardcode a stack-specific command, and do NOT eval a scraped YAML line
               # (run: | blocks and named jobs make that both wrong and unsafe).
               #   Examples: ./scripts/ci.sh | npm test | cargo test -- --test-threads=1
               # Run the gate now, then set GATE_OK=1 ONLY if it exited green. Safe by
               # default: if the gate is red or not yet run, GATE_OK stays empty and the
               # push below is skipped.
               GATE_OK=
               # <run the repo gate here; on green: GATE_OK=1>
               if [ "$GATE_OK" != "1" ]; then
                 echo "GATE not green (or not yet run) — NOT force-pushing. Staying In Progress."
                 exit 0   # done for this turn — do not transition with an unverified branch
               fi
               git push --force-with-lease origin "$BRANCH"
               ;;
             UNKNOWN)
               UNKNOWN_TRIES=$((UNKNOWN_TRIES + 1))
               if [ "$UNKNOWN_TRIES" -ge 5 ]; then
                 echo "mergeStateStatus stuck UNKNOWN after 5 polls — staying In Progress, re-evaluating next poll."
                 exit 0
               fi
               echo "GitHub still computing mergeStateStatus — sleeping 15s and re-polling ($UNKNOWN_TRIES/5)..."
               sleep 15
               ;;
             BLOCKED|UNSTABLE|CLEAN)
               echo "Conflict-free ($MSS) — proceeding to In Review transition."
               break
               ;;
             *)
               # Conservative catch-all: an unexpected or null mergeStateStatus is NOT
               # treated as conflict-free. Hold In Progress and re-poll once (WR-06).
               # Uses its own counter (WEIRD_TRIES) so its /5 budget is independent of
               # the genuine-UNKNOWN polls above.
               WEIRD_TRIES=$((WEIRD_TRIES + 1))
               if [ "$WEIRD_TRIES" -ge 5 ]; then
                 echo "Unexpected mergeStateStatus '$MSS' persists after 5 polls — staying In Progress, re-evaluating next poll."
                 exit 0
               fi
               echo "Unexpected mergeStateStatus: '$MSS' — holding In Progress, sleeping 15s and re-polling ($WEIRD_TRIES/5)..."
               sleep 15
               ;;
           esac
         done

         # Check if exhausted without becoming conflict-free
         MSS_FINAL=$(gh pr view "$PR_NUM" \
           --json mergeStateStatus | jq -r '.mergeStateStatus')
         if [ "$MSS_FINAL" = "DIRTY" ] || [ "$MSS_FINAL" = "BEHIND" ]; then
           MARKER="sinfonia-bot: mergeability-loop-exhausted"
           EXISTING_MARKERS=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
             -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { comments(first: 50) { nodes { body } } } }\"}" \
             | jq -r '.data.issue.comments.nodes[].body' 2>/dev/null || true)
           if ! printf '%s' "$EXISTING_MARKERS" | grep -qF "$MARKER"; then
             ISSUE_UUID=$(curl -sS -H "$AUTH" -H "$CT" "$API" \
               -d "{\"query\":\"{ issue(id:\\\"{{ issue.identifier }}\\\") { id } }\"}" \
               | jq -er '.data.issue.id')
             COMMENT_BODY="${MARKER}\\n\\nMergeability loop exhausted after ${MAX_ATTEMPTS} attempts. The PR is still \`${MSS_FINAL}\` against \`main\`. The issue stays In Progress — re-evaluating next poll once \`main\` stabilizes."
             curl -sS -H "$AUTH" -H "$CT" "$API" \
               -d "{\"query\":\"mutation { commentCreate(input: { issueId: \\\"$ISSUE_UUID\\\", body: \\\"$COMMENT_BODY\\\" }) { success } }\"}"
           fi
           echo "LOOP-EXHAUSTED: still $MSS_FINAL after $MAX_ATTEMPTS attempts — no transition. Staying In Progress."
           exit 0
         fi
         ```

         > ⛔ **If the loop is exhausted (still `DIRTY`/`BEHIND` after 3 attempts), you are DONE
         > for this turn.** Do NOT transition the issue to In Review. The comment above (if
         > needed) was posted. End your turn — the orchestrator will re-dispatch next poll.

      8. Transition the Linear issue to **In Review** using the same
         verify-then-claim pattern as the Todo prompt (swap `"In Progress"` for
         `"In Review"`). Do not claim success unless `OK:` prints. Then stop.

      ### Mergeability procedure

      The single source of truth for rebasing and getting the branch conflict-free
      against `main`. Force-push is safe here because `sinfonia/<id>` is an
      agent-owned branch, and the GitHub native merge queue rebases-and-tests each
      PR against `main` before merging.

      ```bash
      set -e
      BRANCH="sinfonia/{{ issue.identifier | downcase }}"

      # 0. Self-derive PR_NUM — callers from Todo STEP 1 or MERGE-01 may not have one
      #    yet. Steps 5+ (re-poll) are skipped when no PR exists.
      PR_NUM=$(gh pr list --head "$BRANCH" --state all --json number -q '.[0].number')

      # 1. Fetch latest main
      git fetch origin

      # 2. Rebase onto origin/main — resolve any conflicts.
      #    Resolve ONLY conflicts in files touched by this issue.
      #    Do NOT touch unrelated code, do NOT open a new PR.
      #    Guard the rebase: set -e would otherwise abort mid-rebase on a conflict,
      #    leaving a half-applied rebase and an un-pushed branch.
      if ! git rebase origin/main; then
        echo "Rebase hit conflicts — resolve THIS issue's files only, then:"
        echo "  git add <file> && git rebase --continue"
        echo "Re-run the repo gate; only force-push if it is green. Staying In Progress."
        exit 0   # done for this turn — resolve conflicts, re-enter next poll
      fi

      # 3. Gate BEFORE push. Re-run the repo's own gate — the command its CI runs.
      #    Discover it from the CI config (e.g. .github/workflows/), README, or the
      #    harness's documented command. Do NOT hardcode a stack-specific command,
      #    and do NOT eval a scraped YAML line.
      #    Examples: ./scripts/ci.sh | npm test | cargo test -- --test-threads=1
      #    Run the gate now, then set GATE_OK=1 ONLY if it exited green. Safe by
      #    default: if the gate is red or not yet run, the push below is skipped.
      GATE_OK=
      # <run the repo gate here; on green: GATE_OK=1>
      if [ "$GATE_OK" != "1" ]; then
        echo "GATE not green (or not yet run) — NOT force-pushing. Staying In Progress."
        exit 0
      fi

      # 4. Push (force-with-lease aborts safely if upstream moved unexpectedly)
      git push --force-with-lease origin "$BRANCH"

      # 5. Re-poll merge state (only meaningful once a PR exists)
      [ -n "$PR_NUM" ] || { echo "No PR yet — re-poll after the PR is created."; exit 0; }
      MSS=$(gh pr view "$PR_NUM" \
        --json state,mergeable,mergeStateStatus,reviewDecision \
        | jq -r '.mergeStateStatus')
      # Branch based on mergeStateStatus:
      #   DIRTY or BEHIND  → conflicts remain; loop / retry the procedure
      #   UNKNOWN          → GitHub still computing; sleep 15 and re-poll
      #   BLOCKED          → conflict-free; required review pending (proceed — human approves In Review)
      #   UNSTABLE         → conflict-free; non-required checks failing (proceed)
      #   CLEAN            → conflict-free and all checks passing (proceed)
      echo "mergeStateStatus after rebase: $MSS"
      ```

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
