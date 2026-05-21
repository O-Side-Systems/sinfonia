# Phase 5 — Setup skills + new CLI flags

**Target:** v0.3.0
**Scope:** Six setup skills that AI coding tools (Claude Code, OpenCode, Codex, etc.) can invoke to scaffold a working Sinfonia deployment, plus the three CLI commands those skills need to be reliable.
**Estimated size:** ~800 LOC for CLI extensions + ~6 skill folders (~150 LOC of SKILL.md per skill, plus templates) + ~400 LOC of tests.
**Depends on:** Phase 1 (bridge exists), Phase 2 (OpenCode is a real provider). Phase 3's telemetry feeds into one skill but the skill catalog stays the same shape.
**Unblocks:** Phase 7 documentation cross-references the skills.

This phase is more documentation than code, by line count. That matches the user-facing goal: a new user should be able to go from `git clone` to a working deployment using only skills and the CLI flags below — no hand-editing of YAML.

---

## 1. What a skill is, in concrete terms

A skill is a folder under `skills/` containing:

```
skills/<skill-name>/
├── SKILL.md            # human-readable procedure with YAML front matter
├── templates/          # Liquid templates the procedure renders
│   └── *.liquid
└── validators/         # optional shell scripts that verify a step succeeded
    └── *.sh
```

Templates use the **Liquid** engine already in the workspace dependency tree (used by `crates/sinfonia/src/template.rs` for `WORKFLOW.md` prompt rendering). One templating engine, not two — keeps the contributor surface small.

The `SKILL.md` is tool-agnostic — it describes the procedure declaratively. An AI tool (Claude Code, OpenCode, Codex) reads it and either executes it directly or steps the user through it. A human can also read the same file and run it by hand.

**Distribution model (locked):**

- Sinfonia ships all six skills in the repo's `skills/` directory. They live alongside the code.
- Sinfonia does NOT auto-install skills into tool-specific directories. That convention is owned by each tool vendor.
- `docs/SKILLS.md` includes a short table linking to each vendor's current docs on installing skills.

This was decided in the proposal ("Resolved design decisions" §5) and is not up for re-litigation in Phase 5.

---

## 2. Skill catalog

Six skills. Listed in dependency order — the order a fresh user invokes them.

### 2.1 `skills/setup-workflow/`

**For:** a user who has Sinfonia installed and wants to create a `WORKFLOW.md` for a new project.

**Procedure** (this is the spine of `SKILL.md`):

1. Ask which tracker (`linear` / `jira-cloud` / `jira-server`).
2. Prompt for credentials and validate immediately via a tracker API call. Show the user what their project's actual states are.
3. Walk the user through which states are "active" (Sinfonia dispatches an agent) vs "terminal" (Sinfonia ignores).
4. Pick a default agent backend by invoking `setup-agent-backend` (§2.5).
5. Offer the recommended state-machine pattern (Triage → Ready → Needs Fixes → Human Review). If declined, fall back to a single-state config.
6. Ask for the workspace root.
7. Detect project language by looking for `package.json` / `pyproject.toml` / `Cargo.toml` / `go.mod`. Generate appropriate `hooks.after_create` (e.g., `npm install` / `pip install -r requirements.txt` / `cargo fetch`) and `hooks.before_run` (e.g., a test command).
8. Render `WORKFLOW.md` from `templates/workflow.md.liquid`.
9. Run `sinfonia --check WORKFLOW.md` to validate.
10. Commit the file.

**Templates:**

- `templates/workflow.md.liquid` — the full WORKFLOW.md template.
- `templates/hooks-node.liquid`, `templates/hooks-python.liquid`, `templates/hooks-rust.liquid`, `templates/hooks-go.liquid` — language-specific hook snippets.

**Validators:**

- `validators/check-workflow.sh` — invokes `sinfonia --check WORKFLOW.md`.

### 2.2 `skills/setup-bridge/`

**For:** setting up `sinfonia-bridge` for a project that already has a working `WORKFLOW.md`.

**Procedure:**

1. Confirm GitHub repo and ask "Do you need multi-repo or fine-grained permissions?" — routes to PAT (default) or App.
2. For PAT: prompt for token, validate via `gh api user`, confirm required scopes (`repo`, optionally `read:org`).
3. For App: walk through manifest creation, write the manifest file, give the user the install URL. Wait for the user to confirm install before continuing.
4. Confirm tracker is the same one configured in `WORKFLOW.md`. Read credentials from environment.
5. Create the bridge custom fields via `IssueTracker::ensure_custom_field`. For Linear, no-op (custom fields live in a marker comment). For Jira, real API calls.
6. Choose deployment topology:
   - **Sibling daemon** (same host as Sinfonia)
   - **Standalone host** (bridge runs separately, public webhook URL)
   - **Actions-only** (no public bridge; CI posts to bridge via a GitHub Actions workflow)
7. Render `BRIDGE.md` from `templates/bridge.md.liquid`.
8. For sibling-daemon: render `docker-compose.yml` snippet wiring Sinfonia + bridge.
9. For Actions-only: render `.github/workflows/sinfonia-ci-report.yml`.
10. Run `sinfonia-bridge --self-test` (§3.3) and require all checks pass.
11. Commit.

**Templates:**

- `templates/bridge.md.liquid`
- `templates/docker-compose-sibling.yml.liquid`
- `templates/gh-actions-ci-report.yml.liquid`

**Validators:**

- `validators/self-test.sh` — wraps `sinfonia-bridge --self-test BRIDGE.md`.

### 2.3 `skills/setup-state-machine/`

**For:** upgrading an existing `WORKFLOW.md` with the Triage / Ready / Needs Fixes / Human Review pattern.

**Procedure:**

1. Read existing `WORKFLOW.md`.
2. If `states:` block exists, preserve it. Otherwise, propose the four-state pattern with one-paragraph rationale per state.
3. Generate per-state prompts. Every reference to a bridge-written custom field uses a `| default:` filter — a human can manually drop a ticket into "Needs Fixes" with no prior bridge run, in which case `sinfonia_last_ci_failure` is unset. Liquid runs in strict mode in `crates/sinfonia/src/template.rs`; unguarded variable access against an absent field throws a render error instead of returning nil. Use:

   ```liquid
   {{ issue.fields.sinfonia_last_ci_failure | default: "(no prior CI failure recorded; check PR comments)" }}
   ```

   and the same `| default:` discipline for `sinfonia_failure_category`, `sinfonia_attempt_count`, etc. The skill's template generator includes a final pass that grep-asserts every `{{ issue.fields.* }}` is followed by `| default:`.

4. If the user enabled failure categorization in the bridge, also generate `Needs Fixes - Lint`, `Needs Fixes - E2E` per-state entries.
5. Lint with `sinfonia --check`.
6. Commit.

**Templates:**

- `templates/state-machine.liquid` — the full `states:` block.
- `templates/needs-fixes-prompt.liquid`, `templates/needs-fixes-e2e-prompt.liquid` — per-state prompts; every `issue.fields.*` reference includes a `| default:` filter.

### 2.4 `skills/setup-telemetry/`

**For:** wiring telemetry into an existing Sinfonia + bridge deployment.

**Procedure:**

1. Ask which sink:
   - **Honeycomb** — collect `HONEYCOMB_API_KEY`. Add the `telemetry:` block + headers.
   - **Datadog** — collect Datadog OTLP intake URL + API key.
   - **Grafana Cloud** — collect endpoint + auth.
   - **Local Collector → Postgres** — generate the full stack.
2. For local Collector + Postgres:
   - Render `otel-collector-config.yaml` from the example.
   - Render the `docker-compose.yml` Postgres + Collector services.
   - Apply `examples/telemetry/postgres-schema.sql` to the user's database.
   - Generate three starter SQL queries (the ones in `03-telemetry-budget.md` §8.2) as `examples/telemetry/queries/*.sql`.
3. Prompt the user to verify `config/cost_table.yaml` against their providers' current pricing.
4. Append the `telemetry:` blocks to `WORKFLOW.md` and `BRIDGE.md`.
5. Restart the daemons and run a synthetic ticket to confirm spans land in the sink.

**Templates:**

- `templates/telemetry-honeycomb.yml.liquid`
- `templates/telemetry-datadog.yml.liquid`
- `templates/telemetry-grafana.yml.liquid`
- `templates/telemetry-local.yml.liquid`
- `templates/otel-collector-config.yaml.liquid`

### 2.5 `skills/setup-agent-backend/`

**For:** configuring a specific agent backend correctly. Called by other skills; rarely invoked directly.

**Procedure** (sub-divided by backend):

| Backend | Verification | Auth |
|---|---|---|
| `claude_code` | `claude --version`; `which claude` | `claude /login` (interactive) |
| `codex` | `codex --version` | `codex auth` |
| `opencode` | `opencode --version` | `opencode auth login` (per §4 of `02-opencode-backend.md`) |
| `anthropic` | `ANTHROPIC_API_KEY` env present + quick API ping | env var |
| `openai` | `OPENAI_API_KEY` env present + quick API ping | env var |
| `google` | `GOOGLE_API_KEY` env present + quick API ping | env var |
| `ollama` | `curl http://localhost:11434/api/tags` | none |

The skill writes the appropriate `agent:` (or per-state) block. For CLI backends it also runs a one-shot test: `claude --quiet --prompt-stdin <<< 'echo hello'` and asserts a non-empty response.

### 2.6 `skills/migrate-from-symphony/`

**For:** users migrating from the OpenAI Symphony reference implementation, or from `skorokithakis/symphony`.

**Procedure:**

1. Ask the user to point at their existing config directory / file.
2. Detect the Symphony variant (OpenAI's `app-server` shape vs the simpler `skorokithakis` Python shape).
3. Map fields:
   - Tracker config → Sinfonia `tracker:` block.
   - Agent config → Sinfonia `agent:` block (with the `codex_app_server` provider stubbed if they were using it).
   - Workspace config → Sinfonia `workspace:` block.
4. Generate the equivalent `WORKFLOW.md`.
5. Flag features without a direct equivalent (e.g., Symphony's webhook listener — point them at the new bridge from Phase 1).
6. Lint.

**Templates:**

- `templates/workflow-from-symphony.liquid`

**Validators:**

- `validators/check-workflow.sh` — shared with `setup-workflow`.

---

## 3. CLI changes

The skills depend on three new CLI capabilities. These are also useful for users not running an AI tool.

### 3.1 `sinfonia --check <WORKFLOW.md>`

Validates a workflow file without running. Returns non-zero on schema or template errors. Used by every skill that produces a `WORKFLOW.md`.

Implementation:

```rust
#[derive(Parser, Debug)]
struct Args {
    workflow: Option<PathBuf>,
    #[arg(long)] port: Option<u16>,
    #[arg(long, default_value = "pretty")] log_format: String,
    #[arg(long)] check: bool,                  // <-- new
}
```

When `--check` is set:

```rust
let wf = sinfonia::config::read_workflow_file(&workflow_path)?;
let cfg = ServiceConfig::from_workflow(&wf)?;
cfg.validate_for_dispatch()?;
// Additionally: render every state's prompt template with a stub Issue and confirm it parses.
for (state, prompt) in cfg.all_prompts() {
    sinfonia::template::compile(prompt).map_err(|e| format!("state {state}: {e}"))?;
}
println!("ok");
std::process::exit(0);
```

Returns specific error codes:

| Exit code | Meaning |
|---|---|
| 0 | All checks passed |
| 2 | YAML parse error |
| 3 | Schema validation error |
| 4 | Template compile error |
| 5 | Tracker auth missing |

Skills can branch on the exit code to give specific remediation prompts.

### 3.2 `sinfonia init`

Interactive bootstrap that essentially runs the `setup-workflow` skill without requiring an external AI tool. Useful when the user is bootstrapping Sinfonia itself, before they have a coding agent configured.

Implementation: a `clap` subcommand that runs an inquire/dialoguer-driven REPL. The questions mirror §2.1 step-by-step. Output: a `WORKFLOW.md` in the current directory.

```rust
// crates/sinfonia/src/main.rs
#[derive(Parser, Debug)]
#[command(name = "sinfonia")]
enum Cli {
    Init(InitArgs),
    Run(RunArgs),                              // default; what today's `sinfonia` does
}
```

The default subcommand is `Run` to preserve backwards compatibility — `sinfonia WORKFLOW.md` still works exactly as today, parsed as `sinfonia run WORKFLOW.md`. The `init` subcommand is explicit.

Dependencies:

- `inquire = "0.7"` for the REPL — small, no async dep.

### 3.3 `sinfonia-bridge --self-test`

Validates GitHub credentials, tracker credentials, and webhook reachability. Output format per `01-bridge-mvp.md` §8:

```
PASS  config: BRIDGE.md parsed
PASS  github: authenticated as octocat (PAT mode)
PASS  github: webhook endpoint reachable at https://...
PASS  tracker: linear project 'my-awesome-project-abc...' accessible
PASS  custom fields: sinfonia_bridge_state_v1 comment marker reserved
FAIL  webhook reachability: GET https://bridge.example.com/health returned 404
```

Exit code = number of failed checks. The `setup-bridge` skill requires zero failed checks before declaring the install complete.

Implementation in `crates/sinfonia-bridge/src/selftest.rs`.

---

## 4. The `docs/SKILLS.md` cross-vendor table

This is the user-facing pointer to where to install each skill. It looks like:

```markdown
# Skills

Sinfonia ships six skills in the `skills/` directory at the repo root. Each
skill is a self-contained procedure that helps you set up part of a Sinfonia
deployment.

## Available skills

| Skill | What it does |
|---|---|
| `setup-workflow` | Author a new `WORKFLOW.md` for a project. |
| `setup-bridge` | Set up `sinfonia-bridge` for an existing workflow. |
| `setup-state-machine` | Upgrade a workflow to the recommended state-machine pattern. |
| `setup-telemetry` | Wire OpenTelemetry into Sinfonia + the bridge. |
| `setup-agent-backend` | Configure a specific agent backend. |
| `migrate-from-symphony` | Migrate from the upstream Symphony implementations. |

## Installing skills in your AI tool

| Tool | How to install |
|---|---|
| Claude Code | [Anthropic's skill installation docs](https://docs.claude.com/en/docs/claude-code/skills) |
| OpenCode | [OpenCode extensibility docs](https://opencode.ai/docs/extensibility) |
| Codex | [OpenAI Codex CLI docs](https://github.com/openai/codex) |
| Other tools | See your tool's documentation. |

## Running skills without an AI tool

Each `SKILL.md` file is written as a readable runbook. You can also use:

- `sinfonia init` — non-AI equivalent of `setup-workflow`.
- `sinfonia-bridge --self-test` — non-AI bridge validation.
```

The vendor URLs are placeholders; verify each one points to a current page before the release.

---

## 5. Test plan

### 5.1 Unit tests

| Area | What it covers |
|---|---|
| `sinfonia --check` exit codes | Each error class triggers the right exit code. |
| `sinfonia init` REPL flow | A scripted set of inputs produces the expected `WORKFLOW.md` (use `inquire`'s test harness or stub the prompts). |
| `sinfonia-bridge --self-test` | Each check kind passes / fails as expected in isolation. |
| Skill template rendering | Each `templates/*.liquid` renders against a stub context (covered as a small Rust test using the `liquid` crate that's already in the workspace). |

### 5.2 Integration tests

`tests/skills_integration.rs`:

1. For each skill, parse the `SKILL.md` front matter and assert it has `name`, `description`, `version`.
2. For each skill with templates, render them against a known-good context and assert the output matches a snapshot.

We do NOT test "Claude Code runs the skill end-to-end" — that's a vendor integration test, out of our scope.

### 5.3 Manual verification

Per the proposal exit criteria:

> A fresh user can go from `git clone` to a running deployment using only skills, no manual config-file editing.

Procedure:

1. Fresh laptop, no Sinfonia installed.
2. `git clone <repo>`.
3. `cargo build --release`.
4. Invoke `setup-workflow` skill via Claude Code (or `sinfonia init` for the AI-tool-free path).
5. Invoke `setup-agent-backend`.
6. Run Sinfonia for a few ticks against a sandbox project.
7. Invoke `setup-bridge`, then `setup-state-machine`, then `setup-telemetry`.
8. Confirm everything works end-to-end.

Captured in `docs/v0.3-plan/05-skills-VERIFY.md`.

---

## 6. Dependencies

```toml
inquire = "0.7"                # for `sinfonia init`
```

The Liquid crate is already in the workspace; no addition needed for skill templates.

---

## 7. Open questions

1. **`inquire` vs `dialoguer`.** Both are popular Rust REPL crates. `inquire` has cleaner async support; `dialoguer` is more mature. The skills don't need async — pick whichever has lower dep weight. Default: `inquire`.

2. **`sinfonia init` UX.** Should it be a single linear flow, or should it allow back-stepping? Default: linear with the ability to abort and restart. Back-stepping is complexity we don't need yet.

3. **Skill versioning.** Each skill's front matter has a `version` field. When we ship `setup-bridge v1.1` with new behavior, AI tools that cached `v1.0` won't know to refresh. Punt this to v0.4 — for v0.3 we ship v1.0 of every skill.

4. **Localization.** Skills are English-only. Punt.

5. **The `migrate-from-symphony` audience.** Are there actually users who'd benefit? If the OpenAI Symphony project hasn't shipped a v1, this skill is preemptive. Land it anyway — the user-facing message "we have a migration path if you're coming from upstream" is worth the small effort, even if zero people use it in v0.3's lifetime.

---

## 8. Phase 5 deliverable checklist

- [ ] `skills/setup-workflow/`, `skills/setup-bridge/`, `skills/setup-state-machine/`, `skills/setup-telemetry/`, `skills/setup-agent-backend/`, `skills/migrate-from-symphony/` — each with `SKILL.md`, `templates/` (Liquid only), optional `validators/`.
- [ ] Generated templates pass a final `grep -E '\{\{[^}]*issue\.fields\.[^|]*\}\}'` check that finds no unguarded references (every `issue.fields.*` is followed by `| default:`).
- [ ] `sinfonia --check` with the documented exit codes.
- [ ] `sinfonia init` subcommand with the REPL flow.
- [ ] `sinfonia-bridge --self-test` with the canonical output format.
- [ ] `docs/SKILLS.md` with the vendor pointer table.
- [ ] Tests per §5.1 and §5.2.
- [ ] Manual verification recorded in `docs/v0.3-plan/05-skills-VERIFY.md`.
- [ ] CHANGELOG entry.

Phase 5 ships independently of Phases 6-7.
