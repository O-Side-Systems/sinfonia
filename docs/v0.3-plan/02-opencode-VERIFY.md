# Phase 2 — OpenCode backend manual verification

**Status:** **pending real-world manual run.** The code, unit tests, and
integration parse-test landed with the Phase 2 implementation commit
(`P2: OpenCode agent backend`), but the end-to-end manual run against the
real `opencode` CLI is a deferred follow-up. Complete this before tagging
`v0.3.0-alpha.x` if/when the alpha goes to an actual release.

This document captures the spike findings from the OpenCode CLI doc review
plus the manual-verification steps from the Phase 2 plan
(`docs/v0.3-plan/02-opencode-backend.md` §5.3) so the operator running this
step has everything in one place.

---

## OpenCode CLI flag deltas (vs. the Phase 2 plan §2)

The plan §2 surface-area table proposed
`opencode run --prompt-stdin --output-format json --quiet`. Verification
against <https://opencode.ai/docs/cli/> and the upstream source
(`sst/opencode` repo, `packages/opencode/src/cli/cmd/run.ts`) found:

| Plan-doc proposal | Actual flag in `opencode run` | Notes |
|---|---|---|
| `--prompt-stdin` | (no flag — stdin auto-detected) | OpenCode joins piped stdin into the message via `resolveRunInput()` when `process.stdin.isTTY` is false. Sinfonia pipes the prompt the same way `cli.rs` does for `claude_code` / `codex`, so this just works. |
| `--output-format json` | `--format json` | Different flag spelling from Claude Code's `--output-format stream-json`. |
| `--quiet` | (no flag) | OpenCode has no separate quiet mode. `--format json` already suppresses the TUI (the two are mutually exclusive in `run.ts`). |
| `--continue <session_id>` | `--session <session_id>` | The bare `--continue` boolean resumes the *last* session globally — not what we want for concurrent per-issue workspaces. `--session <id>` is the explicit-by-ID form, and that's what `OpenCodeAgent::build_command_line` uses. |
| `--model <name>` | `--model <name>` | Unchanged. OpenCode's value is `provider/model` (e.g. `anthropic/claude-sonnet-4-6`). |
| `session_id` field on init event | `sessionID` on every event | OpenCode emits camelCase. The parser tolerates both `sessionID` and `session_id` (`extract_session_id` in `opencode.rs`). |

The implementation reflects these deltas:

- Default `agent.command` for `provider: opencode` is
  `opencode run --format json` (see `config::typed::default_command`).
- `OpenCodeAgent::build_command_line` appends `--session <id>` (not
  `--continue <id>`) on every turn after the first.
- The streaming parser keys session-ID discovery off `sessionID` first,
  falling back to `session_id` for forward-compatibility.

The `02-opencode-backend.md` plan-doc text remains as authored; the deltas
above are the canonical source of truth and the top-of-file rustdoc in
`crates/sinfonia/src/agent/opencode.rs` reproduces the same table.

---

## Manual verification steps (per plan §5.3)

The Phase 2 proposal exit criteria, verbatim:

> A state-machine configuration that routes Triage to raw Haiku, Ready to
> OpenCode+Sonnet, and Needs Fixes to OpenCode+Sonnet runs successfully on
> a real project.

### Step 1 — Install OpenCode

Install per <https://opencode.ai/docs/> on the host that will run Sinfonia.
Confirm the binary is on `PATH`:

```bash
which opencode
opencode --version
```

If Sinfonia is configured with `provider: opencode` and `opencode` is not
on `PATH`, startup fails with a clear error pointing at the OpenCode docs
(`OpenCodeAgent::new` runs `which::which` in its preflight; see
`opencode.rs::preflight`).

### Step 2 — Authenticate OpenCode

OpenCode owns its own credentials. Sinfonia does not pass an API key.

```bash
opencode auth login
```

Smoke-test OpenCode in non-interactive JSON mode against a trivial prompt:

```bash
echo "echo hello" | opencode run --format json
```

You should see a stream of JSON events on stdout, one per line, each
carrying `sessionID`, `timestamp`, and a `type` field. Confirm:

- The first event includes a `sessionID` value.
- Exit code is 0.
- There's at least one event with `"type":"text"` carrying the model's reply.

### Step 3 — Author a Phase-2-only `WORKFLOW.md`

Create a `WORKFLOW.md` for the test project that routes three states at
three different backends (no Phase-1 bridge fields, so this exercises
Phase 2 in isolation):

```yaml
---
tracker:
  kind: linear
  api_key: $LINEAR_API_KEY
  project_slug: <your-project-slug>
  active_states: ["Triage", "Ready", "Needs Fixes"]

agent:
  provider: anthropic
  model: claude-haiku-4-5-20251001
  max_concurrent_agents: 2

states:
  Triage:
    provider: anthropic
    model: claude-haiku-4-5-20251001
    prompt: |
      Read {{ issue.identifier }} and decide if it's ready for implementation.
      If yes, transition to "Ready". If not, leave a comment and transition
      to "Needs Clarification".
  Ready:
    provider: opencode
    model: anthropic/claude-sonnet-4-6
    turn_timeout_ms: 3600000
    prompt: |
      Implement {{ issue.identifier }}.
  "Needs Fixes":
    provider: opencode
    model: anthropic/claude-sonnet-4-6
    prompt: |
      The CI on the PR for {{ issue.identifier }} failed. Pull the latest
      branch, fix the failures, push, and leave a status comment.
---

You are picking up issue `{{ issue.identifier }}`: {{ issue.title }}.
{{ issue.description }}
```

### Step 4 — Move three tickets through the state machine

Pick three Linear tickets in the test project. For each:

1. Move it to `Triage`. Wait one poll cycle. Confirm Sinfonia logs show
   the **raw `anthropic` backend** dispatched and the agent transitioned
   the ticket to `Ready`.
2. Confirm Sinfonia logs then show the **`opencode` backend** dispatched
   for the `Ready` state — look for `bash -lc 'opencode run --format json
   ...'` in the spawn trace.
3. Confirm the per-issue workspace gets the same OpenCode `sessionID`
   across turns within the same worker — the `thread_id` in subsequent
   `TurnStarted` / `TurnCompleted` events should not start with `pending-`
   after the first turn, and the second turn's command line should
   include `--session <real-id>`.
4. If CI fails on the PR, move the ticket to `Needs Fixes` and confirm
   OpenCode picks up again with the "Needs Fixes" prompt.

### Step 5 — Local Ollama with LSP

To exercise the Phase 2 §4.3 path (the original rationale for adding
OpenCode), reconfigure OpenCode itself for an Ollama provider:

```jsonc
// ~/.config/opencode/config.json
{
  "provider": "ollama",
  "ollama": { "endpoint": "http://localhost:11434" }
}
```

Update `WORKFLOW.md`'s `Ready` state to use an Ollama model:

```yaml
  Ready:
    provider: opencode
    model: ollama/qwen2.5-coder:32b
    turn_timeout_ms: 3600000
```

Repeat Step 4 for at least one ticket. Confirm:

- No outbound calls go to Anthropic / OpenAI (Sinfonia sets no api_key for
  CLI providers; OpenCode picks the provider from its own config).
- The agent uses LSP tools (visible in OpenCode's JSON event stream).

### Step 6 — Record the result

When the steps above pass on a real project, replace this document's
**Status** line at the top with `passed YYYY-MM-DD on <host/OS>` plus a
two-line note on:

- Which OpenCode CLI version was tested.
- Any flag deltas observed since the table above was last updated.

If anything fails, file an issue tagged `phase-2-opencode` and link it
here.

---

## Out of scope for the unit/integration suite

A full end-to-end test that actually spawns `opencode` is **out of scope**
for `cargo test` — same as `cli.rs` doesn't spawn `claude` for real in
tests today. The unit tests in `crates/sinfonia/src/agent/opencode.rs::tests`
cover the parsing logic, command-line assembly, and the preflight failure
path. The integration test in `crates/sinfonia/tests/spec_conformance.rs`
(`opencode_provider_parses`) covers WORKFLOW.md round-tripping for all
three §4 example shapes. Everything else lives in this document.
