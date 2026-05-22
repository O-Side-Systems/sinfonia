# Phase 5 — VERIFY notes

**Status:** Phase 5 implementation landed on branch `v0.3-phase-5-skills-cli`.

This file captures (a) the plan-doc deltas that surfaced during Phase 5 implementation, (b) the manual-verification matrix per plan §5.3, and (c) follow-up items.

---

## 1. Plan-doc deltas surfaced at impl time

### 1.1 `env:NAME` vs `$NAME` env-var syntax

**Plan-doc references** (`05-skills-cli.md` §3.1 pseudo-code) suggested:

```rust
let wf = sinfonia::config::read_workflow_file(&workflow_path)?;
let cfg = ServiceConfig::from_workflow(&wf)?;
cfg.validate_for_dispatch()?;
```

without specifying the env-var resolution syntax used inside the YAML. The canonical Sinfonia syntax is **`$NAME`**, not `env:NAME` — `crates/sinfonia/src/config/typed.rs::resolve_var_string` only recognizes the leading-`$` form (and treats anything else as a literal). Both `WORKFLOW.example.md` and `BRIDGE.example.md` use `$VAR`.

**Captured in:** `init_repl.rs::render_workflow` and the six skill Liquid templates all emit `$VAR`. The early draft of `check.rs` test fixtures used `env:VAR` and gave a false-positive on the auth-missing test (literal string `env:NAME` resolved to a non-empty value); fixed before commit. Worth a one-liner in the plan-doc.

### 1.2 `sinfonia::template::compile` doesn't exist

Plan-doc §3.1 pseudo-code shows `sinfonia::template::compile(prompt)`. The actual `template.rs` exposes `render_prompt(template, &issue, attempt)` — there's no separate compile/render split. Liquid does parse the template inside `render_prompt`, so calling `render_prompt` against a stub Issue catches the same template-author errors a separate `compile` would have. Phase 5's `--check` does exactly this.

### 1.3 Clap default-subcommand

Plan-doc §3.2 shows:

```rust
enum Cli {
    Init(InitArgs),
    Run(RunArgs),
}
```

with the comment "default subcommand is `Run`." Clap doesn't natively support "default subcommand on no-subcommand-given." The workable shape: a top-level `Cli` with `Option<Subcommand>` + `#[command(flatten)] RunArgs` + `#[command(args_conflicts_with_subcommands = true)]`. That preserves the legacy `sinfonia <path>` invocation untouched while exposing `sinfonia run <path>` and `sinfonia init` as siblings. Documented in `crates/sinfonia/src/main.rs` rustdoc.

### 1.4 `sinfonia-bridge --self-test` already shipped in P1-G

Plan §3.3 describes the `--self-test` output format. The actual implementation landed in Phase 1 (P1-G, `crates/sinfonia-bridge/src/selftest.rs`) and matches the plan §3.3 format exactly. Phase 5 §3 work was: nothing on the bridge side. Phase 4 wired the Jira probe through the same shape; no further deltas.

---

## 2. Test coverage

### 2.1 Unit tests (plan §5.1)

| Area | File | Tests | Status |
|---|---|---|---|
| `sinfonia --check` exit codes | `crates/sinfonia/src/check.rs::tests` | 6 (one per exit code + ok + a second template-error variant) | ✅ |
| `sinfonia init` REPL — render→parse→validate | `crates/sinfonia/src/init_repl.rs::tests` | 5 (Linear / Jira-Cloud / Jira-Server rendering, CLI backend omits API env, helper) | ✅ |
| `sinfonia-bridge --self-test` per-check | `crates/sinfonia-bridge/src/selftest.rs::tests` (from P1-G) | inherited from Phase 1 | ✅ |
| Liquid template parsing | `crates/sinfonia/tests/skills_integration.rs::every_liquid_template_parses` | 1 (walks every `skills/*/templates/*.liquid`) | ✅ |

### 2.2 Integration tests (plan §5.2)

| Test | What it pins |
|---|---|
| `all_six_skills_are_present` | The six v1.0 skills exist by name (catches an accidental delete or typo) |
| `every_skill_md_has_required_front_matter` | Every `SKILL.md` has `name` / `description` / `version` keys + a non-empty body |
| `every_liquid_template_parses` | Every `*.liquid` parses under the daemon's Liquid configuration |
| `state_machine_prompts_have_no_unguarded_issue_fields` | The §8 box-2 grep invariant — no unguarded `{{ issue.fields.* }}` references in state-machine prompts |
| `validators_are_executable` | Every `validators/*.sh` carries `chmod +x` |
| `workflow_template_round_trips_through_check` | The `setup-workflow` template renders against a realistic context, and the result passes `validate_for_dispatch` (i.e. the rendered file is a valid WORKFLOW.md) |

All six tests run via `cargo test -p sinfonia --test skills_integration` (6/6 pass).

### 2.3 Total test baseline

Pre-Phase-5: 210 workspace tests on `main` (Phase 4 baseline).
Post-Phase-5 (this branch): **227 workspace tests, 0 failures** (+17 from Phase 5 — 6 `check::tests` + 5 `init_repl::tests` + 6 `skills_integration`).

---

## 3. Manual verification (plan §5.3)

The plan §5.3 exit criterion: *"A fresh user can go from `git clone` to a running deployment using only skills, no manual config-file editing."* The matrix:

| ID | Scenario | Status |
|---|---|---|
| V-1 | Fresh laptop. `git clone` + `cargo build --release -p sinfonia`. Invoke `setup-workflow` skill via Claude Code → `WORKFLOW.md` written → `sinfonia --check` passes. | ⏳ pending manual run |
| V-2 | Same flow but using `sinfonia init` (AI-tool-free). | ⏳ pending manual run |
| V-3 | Invoke `setup-agent-backend` for `claude_code`. Verify the rendered `agent:` block + the one-shot `claude --quiet --prompt-stdin` ping. | ⏳ pending manual run |
| V-4 | Run Sinfonia for a few ticks against a sandbox Linear project; observe a real issue picked up. | ⏳ pending manual run |
| V-5 | Invoke `setup-bridge` for the same project → `BRIDGE.md` written → `sinfonia-bridge --self-test` passes all checks. Sandbox GitHub PR triggers a CI run → the bridge transitions the ticket. | ⏳ pending manual run |
| V-6 | Invoke `setup-state-machine` against the existing workflow → per-state prompts generated with `| default:` guards → `sinfonia --check` passes. | ⏳ pending manual run |
| V-7 | Invoke `setup-telemetry` (Honeycomb sink) → spans land in the Honeycomb dataset (verify the `tenant_id` resource attribute). | ⏳ pending manual run |

V-1 through V-7 will be exercised before tagging `v0.3.0-alpha.x` to an actual release. The 227 automated tests cover the logic; manual verification confirms the AI-tool integration story works end-to-end with a real tracker + real GitHub + real OTel sink.

---

## 4. Plan §7 open-question resolutions

| Open question | Plan default | Resolved at | Notes |
|---|---|---|---|
| #1 `inquire` vs `dialoguer` | `inquire` | impl | `inquire = "0.7"` added to `[workspace.dependencies]`. Pulls in `crossterm` + `signal-hook` + `fuzzy-matcher` transitively — bearable. |
| #2 `sinfonia init` UX (linear vs back-step) | linear with abort | impl | `init_repl::run` is single-pass; the operator re-runs the command to restart. `--no-validate` is a hidden escape hatch for the unusual case where the operator wants the file written even though their env-vars aren't set yet. |
| #3 Skill versioning | punt to v0.4 | impl | All six skills ship at `version: 1.0.0`. AI-tool cache invalidation is the tool's concern, not Sinfonia's. |
| #4 Localization | punt | impl | English-only. |
| #5 `migrate-from-symphony` audience | land anyway | impl | Shipped at v1.0 as the migration-path marker. The "we have a path if you're upgrading" message has value even if zero v0.3.x users invoke it. |

---

## 5. Follow-up items (not blocking Phase 5)

- **Live tracker probe in `sinfonia init`.** The skill version (driven by an AI tool) performs a live API call to validate credentials at the credential-input step. The CLI version (`sinfonia init`) does not — the questions mirror the skill but the live probe is deferred. Adding it would require synchronous reqwest inside the REPL, which is doable; punt unless an operator surfaces a need.

- **Skill-runner reference implementation.** Sinfonia doesn't ship a runner that "executes a SKILL.md" — AI tools do that. If a generic open-source skill runner emerges (or a Sinfonia-specific one is needed for CI testing), it'd live in a separate crate.

- **Per-state failure-category prompts.** Plan §2.3 step 4 mentions generating `Needs Fixes - Lint`, `Needs Fixes - E2E`, etc. per-state entries when the bridge has failure categorization enabled. The skill describes this; the templates only ship the generic `needs-fixes-prompt.liquid` + the E2E variant. Adding per-category templates (`needs-fixes-lint-prompt.liquid`, `needs-fixes-build-prompt.liquid`, `needs-fixes-unit-test-prompt.liquid`) is straightforward; deferred until a user surfaces the need.

- **CI gate on the `--check` exit codes.** `cargo run -p sinfonia -- WORKFLOW.example.md --check` should run in CI alongside the existing `sinfonia-bridge BRIDGE.example.md --check` gate (STATUS Phase-3-followup §3). Add when CI is being touched anyway.
