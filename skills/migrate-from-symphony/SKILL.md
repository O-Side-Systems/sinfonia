---
name: migrate-from-symphony
description: Migrate from the OpenAI Symphony reference implementation or skorokithakis/symphony to Sinfonia. Detects the source variant, maps tracker/agent/workspace config, generates an equivalent WORKFLOW.md, and flags features without a direct equivalent.
version: 1.0.0
---

# migrate-from-symphony

Move an existing Symphony deployment to Sinfonia. Two upstream
implementations exist; the skill detects which one and maps fields
accordingly.

## When to use

- The operator is running an existing Symphony deployment (OpenAI's
  reference implementation or the upstream Python implementation by
  [skorokithakis](https://github.com/skorokithakis/symphony)) and wants
  to move to Sinfonia.

## Procedure

### 1. Locate the source config

Ask the operator to point at their existing Symphony config — either:

- A directory (`app-server` style — OpenAI's reference impl), or
- A single config file (skorokithakis-style — usually `symphony.yaml` or
  similar).

### 2. Detect the variant

Heuristics:

- **OpenAI `app-server`**: directory contains `agent.yaml` +
  `workflows/`. Tracker config has `linear:` or `jira:` blocks with
  schema close to Sinfonia's but with `app-server`-specific keys
  (e.g. `codex_app_server: {...}`).
- **`skorokithakis/symphony`**: a single Python-shape YAML with `agents:`,
  `prompts:`, `triggers:` blocks.

If neither matches, stop and ask the operator to copy a sample config
into a fresh path.

### 3. Map fields

| Symphony field | Sinfonia field |
|---|---|
| Tracker block (`linear:` / `jira:`) | `tracker:` block |
| Agent block (`codex` / `claude_code` / `openai` / etc.) | `agent:` + `llm:` blocks |
| `codex_app_server: {...}` | `agent.provider: codex_app_server` (stubbed — see SPEC §18.2) |
| Workspace block (`workspace_root:`, `hooks:`) | `workspace:` + `hooks:` blocks |
| Webhook listener config (OpenAI's app-server only) | Flag this — Sinfonia uses `sinfonia-bridge` for webhooks. Run `setup-bridge` after this skill. |
| Prompt template files (`prompts/*.md`) | Inline into per-state `prompt_template` (see `setup-state-machine` for the four-state pattern) |
| Skorokithakis `triggers:` | Map to active/terminal states in `tracker:` |

### 4. Render

Render `templates/workflow-from-symphony.liquid` with the mapped values.
Write to `./WORKFLOW.md` (default) or the path the operator specifies.

### 5. Flag features without a direct equivalent

Surface to the operator in a "compatibility notes" section:

- **Webhook listener**: Symphony's built-in webhook listener becomes
  `sinfonia-bridge`. The bridge ships separately and is configured via
  `BRIDGE.md` (run `setup-bridge` next).
- **`codex_app_server` provider**: stubbed in Sinfonia for parity but the
  full feature surface (sandbox execution, multi-step planning) is
  partially implemented. See `docs/SPEC.md` §18.2.
- **Native ADR/decision-doc generation** (some Symphony forks): not
  ported; use the bridge's marker-comment pattern + tracker comments
  instead.

### 6. Validate

Run `validators/check-workflow.sh` (shared with `setup-workflow`).

### 7. Commit

`git add WORKFLOW.md && git commit -m "Migrate from Symphony"`.

## Audience

If the upstream OpenAI Symphony project hasn't shipped a v1 by the time
you're reading this, this skill is preemptive. We ship it as a migration
path for the upstream implementations that DO exist (`openai/symphony`
reference + `skorokithakis/symphony` Python port); the user-facing
message "we have a migration path if you're coming from upstream" is
worth the small effort even if zero people use it in v0.3's lifetime.

## Templates

- `templates/workflow-from-symphony.liquid` — Sinfonia WORKFLOW.md
  rendered from mapped Symphony fields.

## Validators

- `validators/check-workflow.sh` — shared with `setup-workflow`.
