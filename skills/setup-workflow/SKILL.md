---
name: setup-workflow
description: Author a new WORKFLOW.md for a project. Walks the operator through tracker selection, credentials, state definitions, agent backend, workspace root, and language-aware hook generation. Produces a validated WORKFLOW.md.
version: 1.0.0
---

# setup-workflow

Author a new `WORKFLOW.md` for a project. This is the first skill a fresh
operator runs.

## When to use

- The operator has cloned Sinfonia and has no `WORKFLOW.md` yet.
- An existing `WORKFLOW.md` needs to be regenerated from scratch (e.g. the
  tracker changed).

If you're upgrading an existing workflow with the four-state pattern, use
`setup-state-machine` instead.

## Procedure

### 1. Tracker selection

Ask the operator: **"Which tracker do you use?"**

Offer three options: `linear`, `jira-cloud`, `jira-server`.

### 2. Credentials + live validation

Prompt for tracker credentials and validate them immediately via a tracker
API call. Show the operator their project's actual states so step 3 is
grounded in reality.

- **Linear**: prompt for `LINEAR_API_KEY` (env var name). Confirm
  `echo $LINEAR_API_KEY | head -c 10` is non-empty. Probe with
  `curl -s -H "Authorization: $LINEAR_API_KEY" -d
  '{"query":"{ viewer { id email } projects(first: 5) { nodes { name } } }"}'
  https://api.linear.app/graphql` and surface the project names + viewer
  email back to the operator.
- **Jira Cloud**: prompt for `JIRA_API_TOKEN`, `JIRA_EMAIL`, and the
  Atlassian endpoint (e.g. `https://yourorg.atlassian.net`). Probe with
  `curl -s -u "$JIRA_EMAIL:$JIRA_API_TOKEN"
  "$JIRA_BASE/rest/api/3/project"` and surface project keys.
- **Jira Server / DC**: prompt for `JIRA_PAT` and the endpoint. Probe with
  `curl -s -H "Authorization: Bearer $JIRA_PAT"
  "$JIRA_BASE/rest/api/3/project"`.

If any probe fails, stop and surface the error to the operator. Do not
proceed to step 3 — the rest of the flow depends on knowing the project's
real state list.

### 3. Active vs terminal states

Walk the operator through which states are **active** (Sinfonia dispatches
an agent on them) vs **terminal** (Sinfonia ignores them). Use the state
list returned in step 2 as the menu.

Recommend:
- Active: `Todo`, `In Progress` (and `Needs Fixes` if the state-machine
  pattern is enabled later).
- Terminal: `Done`, `Cancelled`, `Closed`.

### 4. Agent backend

Invoke the `setup-agent-backend` skill to choose and configure a default
agent backend. That skill returns the `agent:` (and optional `llm:`) block
to inline here.

### 5. State-machine pattern

Offer the recommended four-state pattern (Triage → Ready → Needs Fixes →
Human Review). If accepted, defer to the `setup-state-machine` skill for
state generation; if declined, fall back to a single-prompt config that
runs on every active state.

### 6. Workspace root

Ask for the path where Sinfonia will clone per-issue workspaces. Default:
`./workspaces`. Validate the path is writable.

### 7. Language detection + hooks

Inspect the project root for a recognizable language marker:

| File | Language | After-create hook | Before-run hook |
|---|---|---|---|
| `package.json` | Node | `npm install` | `npm test --silent` |
| `pyproject.toml` | Python | `pip install -r requirements.txt` (or `poetry install`) | `pytest -q` |
| `Cargo.toml` | Rust | `cargo fetch` | `cargo test --no-run` |
| `go.mod` | Go | `go mod download` | `go test ./...` |

If multiple markers exist, ask the operator which one to use. Render the
matching hooks template into the `hooks:` block.

### 8. Render

Render `templates/workflow.md.liquid` with the answers from steps 1–7.
Write the result to `./WORKFLOW.md` in the project root.

### 9. Validate

Run `sinfonia --check WORKFLOW.md`. Exit codes (per Phase 5 §3.1):

| Exit | Meaning | Remediation |
|---|---|---|
| 0 | All checks passed | Continue to step 10. |
| 2 | YAML parse error | Template bug — re-render and re-validate. |
| 3 | Schema validation | Likely a missing required field; re-prompt the operator. |
| 4 | Template compile error | The generated prompt body has a bad Liquid reference. Fix the template. |
| 5 | Tracker auth missing | The env var from step 2 isn't set in the current shell. |

Do not commit until validation passes.

### 10. Commit

`git add WORKFLOW.md && git commit -m "Add Sinfonia workflow"`.

## Next steps

Recommend the operator run:

1. `setup-bridge` — wire `sinfonia-bridge` to the same tracker for the CI
   feedback loop.
2. `setup-state-machine` — upgrade to the four-state pattern if not chosen
   in step 5.
3. `setup-telemetry` — wire OpenTelemetry into both binaries.

## Templates

- `templates/workflow.md.liquid` — the WORKFLOW.md skeleton.
- `templates/hooks-node.liquid`, `templates/hooks-python.liquid`,
  `templates/hooks-rust.liquid`, `templates/hooks-go.liquid` —
  language-specific snippets for step 7.

## Validators

- `validators/check-workflow.sh` — wraps `sinfonia --check WORKFLOW.md`
  and surfaces the exit code with a remediation hint.

## AI-tool-free fallback

If the operator doesn't have an AI tool installed, they can run
`sinfonia init` for an equivalent (slightly slimmer) REPL flow.
