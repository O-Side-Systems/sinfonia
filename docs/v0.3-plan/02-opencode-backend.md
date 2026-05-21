# Phase 2 — OpenCode agent backend

**Target:** v0.3.0
**Scope:** Add `provider: opencode` as a first-class subprocess-driven coding-agent backend, joining `claude_code` and `codex`. Raw LLM backends (`anthropic` / `openai` / `google` / `ollama`) stay.
**Estimated size:** ~400 LOC of Rust + ~150 LOC of tests + ~100 LOC of docs.
**Depends on:** Phase 1 (workspace conversion). Without the workspace move, `opencode.rs` adds noise to a single-crate `src/agent/` tree; with the workspace move it sits cleanly next to `cli.rs`.
**Unblocks:** Phase 5's `setup-agent-backend` skill needs a working OpenCode branch.

This document is an implementation plan, not the implementation.

---

## 1. Why OpenCode is worth a separate backend

The change proposal makes the case in three lines that's worth keeping prominent:

- OpenCode brings LSP integration, Plan/Build mode, and MCP support — features that would take significant effort to replicate inside Sinfonia's raw-LLM tool loop.
- It's the middle tier between raw-LLM and the Claude Code / Codex products.
- It adds an Ollama-with-LSP path without Sinfonia having to wire LSP support itself.

Practically: a team that wants local-model coding sessions (Ollama-backed) gets language-aware tooling for free by routing through OpenCode instead of through Sinfonia's raw `ollama` backend.

---

## 2. Surface area

A summary table that maps to the proposal text and to the existing `cli.rs` pattern:

| Aspect | Detail | Notes |
|---|---|---|
| Command | `opencode run --prompt-stdin --output-format json --quiet` | Verify against `opencode.ai/docs` at implementation time; `cli.rs` already abstracts the exact command line, so a future flag change is one-line. |
| Stdin | Prompt piped via stdin, matching `claude_code` and `codex`. | `child.stdin.write_all(prompt.as_bytes())` — same call site as in `cli.rs`. |
| Auth | Owned by the `opencode` CLI, configured via `opencode auth login`. | Sinfonia does NOT pass auth. Sinfonia doesn't have OpenCode credentials. |
| Continuation | `opencode run --continue <session_id>` on retry turns. | Matches the `--resume` / `--thread` pattern already present for Claude Code / Codex. |
| Working directory | Per-issue workspace (`Workspace.path` from `WorkspaceManager`). | Same as every other backend. |
| Provider selection | OpenCode internally supports 75+ providers (OpenAI, Anthropic, Ollama, etc.). User configures inside OpenCode's own config, not in `WORKFLOW.md`. | Means our `model:` field in WORKFLOW.md just gets passed through. |
| Output format | JSON event stream on stdout (one line per event). | `cli.rs`'s parser handles line-delimited JSON already; we copy that path. |
| Exit behavior | Process exits 0 on completion, non-zero on error. | Matches `cli.rs` expectations. |

---

## 3. Code changes

### 3.1 New file: `crates/sinfonia/src/agent/opencode.rs`

Modeled on `src/agent/cli.rs` (534 lines). We don't reuse `cli.rs` directly because the event payload shape, the `--continue` flag, and the model-pass-through behavior differ enough that a copy-and-adapt is cleaner than an abstraction.

Public type:

```rust
pub struct OpenCodeAgent {
    command: String,                       // configurable, default "opencode run"
    model: Option<String>,                 // pass-through to opencode
    turn_timeout_ms: Option<u64>,
}

#[async_trait::async_trait]
impl CodingAgent for OpenCodeAgent {
    async fn start_session(&self, issue: &Issue, workspace: PathBuf) -> Result<AgentSession>;
    async fn run_turn(&self, session: &mut AgentSession, prompt: &str, is_first_turn: bool, events: &EventSender) -> Result<TurnOutcome>;
    async fn stop_session(&self, _session: AgentSession) -> Result<()>;
}
```

The session id is discovered from the first JSON event line on stdout (OpenCode emits a session-init event). Until then, `AgentSession::thread_id` starts as `pending-<uuid>` — the same convention `cli.rs` uses today (see `cli.rs:193-198`).

Session continuation:

```rust
fn build_command_line(&self, session_id: Option<&str>) -> String {
    let mut s = self.command.clone();
    if let Some(model) = &self.model {
        s.push_str(&format!(" --model {}", shell_escape::escape(model.into())));
    }
    s.push_str(" --prompt-stdin --output-format json --quiet");
    if let Some(id) = session_id {
        if !id.starts_with("pending-") {
            s.push_str(&format!(" --continue {}", shell_escape::escape(id.into())));
        }
    }
    s
}
```

(Exact flag names get verified at implementation time. If OpenCode renames `--continue` to `--resume`, this is the one line that changes.)

### 3.2 Factory wiring: `crates/sinfonia/src/agent/mod.rs`

Today's match (lines 79-88):

```rust
pub fn build_for(cfg: &ServiceConfig, llm: &LlmConfig) -> Result<Arc<dyn CodingAgent>> {
    Ok(match llm.provider {
        AgentProvider::OpenAi => Arc::new(openai::OpenAiAgent::new(cfg, llm)?),
        AgentProvider::Anthropic => Arc::new(anthropic::AnthropicAgent::new(cfg, llm)?),
        AgentProvider::Google => Arc::new(google::GoogleAgent::new(cfg, llm)?),
        AgentProvider::Ollama => Arc::new(ollama::OllamaAgent::new(cfg, llm)?),
        AgentProvider::ClaudeCode | AgentProvider::Codex => Arc::new(cli::build_for(llm)?),
        AgentProvider::CodexAppServer => Arc::new(codex_stub::CodexStubAgent::new(llm)?),
    })
}
```

After Phase 2:

```rust
        AgentProvider::OpenCode => Arc::new(opencode::OpenCodeAgent::new(cfg, llm)?),
```

That's it on the wiring side.

### 3.3 Provider enum: `crates/sinfonia/src/config/typed.rs`

The enum at `typed.rs:65-81` gains one variant:

```rust
pub enum AgentProvider {
    OpenAi,
    Anthropic,
    Google,
    Ollama,
    ClaudeCode,
    Codex,
    OpenCode,         // <- new
    CodexAppServer,
}
```

The `AgentProvider::parse()` function at `typed.rs:83-99` gains one arm:

```rust
        "opencode" => Ok(Self::OpenCode),
```

Schema docs (in `WORKFLOW.example.md`) get a one-paragraph note. See `docs/v0.3-plan/07-docs.md` for the wider doc update.

### 3.4 Preflight check

Sinfonia today has CLI-preflight checks for `claude_code` and `codex` that fail with a useful message when the binary isn't on PATH (existing logic in `cli.rs`). Mirror it for OpenCode:

```rust
fn preflight(command: &str) -> Result<()> {
    let bin = command.split_whitespace().next().unwrap_or("opencode");
    which::which(bin).map_err(|_| Error::missing_cli("opencode", bin))?;
    Ok(())
}
```

`which` is not currently a dep; we can either add it (~10 lines of work, very small crate) or use the `bash -lc "command -v opencode"` pattern that already exists elsewhere. Default: add `which` to the workspace deps; it's used by multiple backends.

---

## 4. Configuration examples

### 4.1 Default-lane usage

```yaml
agent:
  provider: opencode
  model: claude-sonnet-4-6
  command: opencode run             # override if installed elsewhere
  turn_timeout_ms: 1800000
```

### 4.2 State-machine routing (post Phase 1, with the bridge)

```yaml
states:
  Triage:
    provider: anthropic
    model: claude-haiku-4-5-20251001

  Ready:
    provider: opencode
    model: claude-sonnet-4-6
    turn_timeout_ms: 3600000

  "Needs Fixes":
    provider: opencode
    model: claude-sonnet-4-6
    prompt: |
      Address the CI failures on {{ issue.identifier }}.

      Last failure:
      {{ issue.fields.sinfonia_last_ci_failure | default: "(see PR checks)" }}

      Fix the failing tests, push to the same branch, and leave a comment
      summarizing what changed.

  "Needs Fixes - E2E":
    provider: opencode
    model: claude-opus-4-7
    turn_timeout_ms: 5400000
    prompt: |
      An e2e test failed. e2e failures can indicate real regressions or flaky
      tests. If you determine the test is flaky, document that in a comment
      and don't touch the test. Otherwise, fix the underlying issue.
```

### 4.3 Local-only / Ollama-with-LSP

The whole reason OpenCode is interesting for some users. `WORKFLOW.md`:

```yaml
agent:
  provider: opencode
  model: qwen2.5-coder:32b
```

OpenCode's own `~/.config/opencode/config.json`:

```json
{
  "provider": "ollama",
  "ollama": { "endpoint": "http://localhost:11434" }
}
```

This gives a fully-local coding loop with LSP support, which the raw `ollama` backend in Sinfonia doesn't provide.

---

## 5. Test plan

### 5.1 Unit tests (`crates/sinfonia/src/agent/opencode.rs::tests`)

| Test | What it covers |
|---|---|
| `build_command_line_first_turn` | No `--continue` flag when `session_id` is `None`. |
| `build_command_line_pending_session` | No `--continue` flag when `session_id` starts with `pending-`. |
| `build_command_line_resumed_session` | `--continue <id>` appended with proper shell escaping. |
| `parse_init_event` | First-line JSON `{"type":"init","session_id":"..."}` updates the session. |
| `parse_token_event` | Token-usage events update `total_tokens` in `TurnOutcome`. |
| `preflight_missing_binary` | When `which::which("opencode")` fails, returns a clear error. |

### 5.2 Integration test (`tests/spec_conformance.rs`, new section)

Add an `#[test] fn opencode_provider_parses()` that asserts `WORKFLOW.md` snippets like the ones in §4 round-trip cleanly through `ServiceConfig::from_workflow()`.

A full end-to-end test with an actual `opencode` subprocess running is **out of scope** for the unit/integration suite — same as today's `cli.rs` doesn't run `claude` for real in tests. Manual verification (§5.3) covers it.

### 5.3 Manual verification

The proposal exit criteria:

> A state-machine configuration that routes Triage to raw Haiku, Ready to OpenCode+Sonnet, and Needs Fixes to OpenCode+Sonnet runs successfully on a real project.

Steps:

1. Install `opencode` per `opencode.ai/docs`.
2. `opencode auth login` and verify with `opencode run --quiet --prompt-stdin <<< "echo hello"`.
3. Author a `WORKFLOW.md` per §4.2 (without the Phase-1 bridge fields, since we're testing Phase 2 in isolation).
4. Move three tickets through Triage → Ready → done. Verify Sinfonia dispatches the right backend at each state and the `opencode` subprocess receives the expected prompt.
5. Repeat with Ollama configured inside OpenCode (§4.3) on at least one ticket.

Result captured in `docs/v0.3-plan/02-opencode-VERIFY.md` during the implementation pass.

---

## 6. Documentation that lands with this phase

The phase isn't done until these are written:

- `WORKFLOW.example.md` — new `opencode` block in the `agent:` section example, plus an inline note about OpenCode owning auth via its own CLI.
- `README.md` — the bullet about supported backends gains `OpenCode`.
- `docs/SPEC.md` §18.2 — list `OpenCode backend` as a recommended extension.
- `crates/sinfonia/src/agent/opencode.rs` — module-level rustdoc summarizing: what OpenCode is, why it exists alongside `cli.rs`, what auth model it uses. Roughly the content of §1 of this plan, rewritten for in-source readers.

The user explicitly asked for "ample documentation at the quality one would expect for an open source project," so the inline rustdoc for `OpenCodeAgent` should match the comment density already present in `cli.rs` — i.e. doc comments on the struct, on `start_session` / `run_turn` / `stop_session`, and on any non-obvious helper (e.g. why the `pending-` prefix exists).

---

## 7. Open questions

1. **`opencode run` exact flags.** The proposal says "verify exact flags against `opencode.ai/docs` at implementation time." Do this in a separate ~30-minute spike before opening the implementation PR. The spike's findings get pasted into this plan and (after merge) into the code comments.

2. **OpenCode `--quiet` vs `--no-progress` vs structured events.** OpenCode's CLI is younger than `claude` and `codex`; the structured-event flag name might be `--output json` rather than `--output-format json`. Same spike resolves it.

3. **Cross-restart session resumption.** OpenCode sessions persist on disk; in principle we could resume across Sinfonia restarts. But Sinfonia's own spec §14.3 explicitly says in-flight sessions don't survive restart, and the proposal preserves that. So: we do nothing special, and the OpenCode session ID gets re-bootstrapped on restart. Document this in the rustdoc.

4. **Model name pass-through.** OpenCode interprets `claude-sonnet-4-6` differently depending on its internal provider config. Do we surface a warning when Sinfonia's `model:` doesn't match anything in the user's local OpenCode config? Probably not — that's OpenCode's job to error on, and we'd be inventing a coupling. Leave it alone.

---

## 8. Phase 2 deliverable checklist

- [ ] `crates/sinfonia/src/agent/opencode.rs` implementing `CodingAgent` for `OpenCodeAgent`.
- [ ] `AgentProvider::OpenCode` variant in `crates/sinfonia/src/config/typed.rs`, plus `parse()` arm.
- [ ] Factory wiring in `crates/sinfonia/src/agent/mod.rs`.
- [ ] `which` crate added to workspace deps (or equivalent presence check).
- [ ] Unit tests per §5.1.
- [ ] Integration parse-test in `tests/spec_conformance.rs`.
- [ ] Updated `WORKFLOW.example.md` with an OpenCode example block.
- [ ] Module-level rustdoc on `opencode.rs` summarizing the surface.
- [ ] README backend bullet updated.
- [ ] Manual verification recorded in `docs/v0.3-plan/02-opencode-VERIFY.md`.
- [ ] CHANGELOG entry mentioning the new backend.

When all boxes are checked, Phase 2 ships independently of Phases 3-7.
