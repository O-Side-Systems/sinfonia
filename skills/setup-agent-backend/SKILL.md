---
name: setup-agent-backend
description: Configure a specific agent backend (claude_code, codex, opencode, anthropic, openai, google, ollama). Verifies the backend is reachable, runs a one-shot ping, and emits the `agent:` (+ optional `llm:`) block to inline into WORKFLOW.md. Called by other skills; rarely invoked directly.
version: 1.0.0
---

# setup-agent-backend

Configure a specific agent backend for Sinfonia. Returns a YAML snippet
that other skills (`setup-workflow`, `setup-state-machine`) inline into
the generated `WORKFLOW.md`.

## When to use

- Invoked by `setup-workflow` during fresh project setup.
- Invoked by `setup-state-machine` when a state needs a different backend
  from the global default.
- Invoked directly by an operator who wants to switch a deployed Sinfonia
  to a new backend.

## Procedure

### 1. Ask which backend

Offer:

| Backend | Best for |
|---|---|
| `claude_code` | Anthropic's Claude Code CLI is installed; want subscription pricing |
| `codex` | OpenAI's Codex CLI is installed |
| `opencode` | [sst/opencode](https://github.com/sst/opencode) is installed; want a multi-provider router (75+ providers, incl. local Ollama+LSP) |
| `anthropic` | direct Anthropic API; want raw control |
| `openai` | direct OpenAI API |
| `google` | direct Google AI / Gemini API |
| `ollama` | local Ollama daemon; no cloud costs |

### 2. Verify the backend is reachable

| Backend | Verification |
|---|---|
| `claude_code` | `claude --version` returns a version; `which claude` is non-empty |
| `codex` | `codex --version` returns a version |
| `opencode` | `opencode --version` returns a version (per `02-opencode-backend.md` §4) |
| `anthropic` | `$ANTHROPIC_API_KEY` is non-empty; ping `https://api.anthropic.com/v1/messages` with a 1-token prompt |
| `openai` | `$OPENAI_API_KEY` is non-empty; ping `/v1/chat/completions` |
| `google` | `$GOOGLE_API_KEY` is non-empty; ping `generativelanguage.googleapis.com/v1beta/models` |
| `ollama` | `curl -fsSL http://localhost:11434/api/tags` returns 200 |

If verification fails, stop and surface the error. Do NOT emit a YAML
block that points at an unreachable backend.

### 3. Auth setup

| Backend | Auth flow |
|---|---|
| `claude_code` | `claude /login` (interactive — opens a browser). Operator runs this themselves. |
| `codex` | `codex auth` (interactive). Operator runs this themselves. |
| `opencode` | `opencode auth login` (interactive — picks one of OpenCode's 75+ providers). Operator runs this themselves. |
| `anthropic` / `openai` / `google` | Set the corresponding API key env var. The skill prompts for the name + verifies it's non-empty. |
| `ollama` | No auth. |

### 4. One-shot ping (CLI backends only)

For `claude_code`, `codex`, `opencode`: run a one-shot test prompt and
assert a non-empty response:

```bash
echo 'echo hello' | claude --quiet --prompt-stdin 2>&1 | tee /tmp/agent-ping.log
test -s /tmp/agent-ping.log  # non-empty
```

Adapt for the chosen backend (OpenCode uses `--format json`; see
`02-opencode-backend.md` §5 for the actual flag set).

### 5. Emit the YAML block

Render the appropriate template:

- `templates/agent-claude-code.liquid`
- `templates/agent-codex.liquid`
- `templates/agent-opencode.liquid`
- `templates/agent-anthropic.liquid`
- `templates/agent-openai.liquid`
- `templates/agent-google.liquid`
- `templates/agent-ollama.liquid`

Return the rendered snippet to the calling skill.

## Templates

One template per backend, each emitting the `agent:` and (where
applicable) `llm:` block.

## See also

- `docs/v0.3-plan/02-opencode-VERIFY.md` — the OpenCode flag-set
  doc-spike findings (relevant if you're configuring `opencode`).
- `docs/SPEC.md` §18.2 — the canonical backend table.
