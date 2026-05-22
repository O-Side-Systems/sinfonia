# Skills

**Audience:** anyone setting up a Sinfonia deployment with an AI coding tool
(Claude Code, OpenCode, Codex, etc.) — or anyone preferring to follow a
written runbook by hand. **Read time:** ~5 min. **Prerequisites:** the
Sinfonia repo cloned (skills live in `skills/`), and either an AI coding
tool that supports skills, or a willingness to read `SKILL.md` files as
prose.

Sinfonia ships six skills in the `skills/` directory at the repo root. Each
skill is a self-contained procedure that helps you set up part of a Sinfonia
deployment. AI coding tools (Claude Code, OpenCode, Codex, …) can invoke
them directly; humans can also read each `SKILL.md` as a runbook and follow
the steps by hand.

## Available skills

| Skill | What it does |
|---|---|
| [`setup-workflow`](../skills/setup-workflow/SKILL.md) | Author a new `WORKFLOW.md` for a project. Walks tracker selection, credentials, state definitions, agent backend, and language-aware hooks. |
| [`setup-bridge`](../skills/setup-bridge/SKILL.md) | Set up `sinfonia-bridge` for an existing workflow. Configures GitHub auth, provisions custom fields, picks a deployment topology, and runs `--self-test`. |
| [`setup-state-machine`](../skills/setup-state-machine/SKILL.md) | Upgrade a workflow to the Triage / Ready / Needs Fixes / Human Review pattern. Generates per-state prompts that consume the bridge's custom fields safely. |
| [`setup-telemetry`](../skills/setup-telemetry/SKILL.md) | Wire OpenTelemetry into Sinfonia + the bridge. Supports Honeycomb, Datadog, Grafana Cloud, or local Collector → Postgres. |
| [`setup-agent-backend`](../skills/setup-agent-backend/SKILL.md) | Configure a specific agent backend (claude_code / codex / opencode / anthropic / openai / google / ollama). Called by other skills. |
| [`migrate-from-symphony`](../skills/migrate-from-symphony/SKILL.md) | Migrate from the OpenAI Symphony reference implementation or `skorokithakis/symphony` to Sinfonia. |

## Recommended order for a fresh deployment

1. `setup-workflow` — produces `WORKFLOW.md`.
2. `setup-bridge` — produces `BRIDGE.md` + GitHub auth + custom-field
   provisioning.
3. `setup-state-machine` — upgrades `WORKFLOW.md` with the four-state
   pattern + per-state prompts.
4. `setup-telemetry` — wires OpenTelemetry into both binaries.

## Installing skills in your AI tool

Each AI tool has its own convention for where skills live and how they're
discovered. Sinfonia ships the skills; the AI tool's documentation tells
you where to place them.

| Tool | How to install |
|---|---|
| Claude Code | See [Anthropic's skill installation docs](https://docs.claude.com/en/docs/claude-code/skills). |
| OpenCode | See [OpenCode extensibility docs](https://opencode.ai/docs). |
| Codex CLI | See [OpenAI Codex CLI repo](https://github.com/openai/codex). |
| Other tools | See your tool's documentation. |

The vendor URLs above point at current docs as of Sinfonia v0.3.0-alpha.1.
If a link 404s, search for the tool's "skills" or "extensions" section.

## Running skills without an AI tool

You can read every `SKILL.md` as a runbook and follow the steps by hand.
Sinfonia also ships two convenience CLIs that cover the most common path:

- `sinfonia init` — interactive REPL equivalent of `setup-workflow`. Asks
  the same questions and writes a validated `WORKFLOW.md`.
- `sinfonia-bridge --self-test` — non-AI bridge validation (the same
  check `setup-bridge` step 7 runs).

## Skill contract

Every `SKILL.md` carries YAML front matter with three required keys:

```yaml
---
name: setup-workflow
description: ...one-line summary...
version: 1.0.0
---
```

The integration test
`crates/sinfonia/tests/skills_integration.rs::every_skill_md_has_required_front_matter`
enforces this. AI tools that index skills by name or version can depend on
these keys being present.

## Strict-Liquid invariant

State-machine prompt templates (`skills/setup-state-machine/templates/*.liquid`)
must follow one rule: every `{{ issue.fields.X }}` reference is followed by
a `| default:` filter.

```liquid
{{ issue.fields.sinfonia_last_ci_failure | default: "(no prior CI failure recorded)" }}
```

Rationale: `crates/sinfonia/src/template.rs` runs Liquid in strict mode. A
human dragging a ticket into Needs Fixes without any prior bridge run
leaves bridge-managed custom fields absent; without `| default:` the
strict-mode renderer errors out. The
`state_machine_prompts_have_no_unguarded_issue_fields` integration test
greps for unguarded references and fails CI on a hit.

## Skill versioning

Skills carry a `version` field. v1.0 of every skill ships with Sinfonia
v0.3.0. When a future release changes a skill's contract, the version
bumps and the changelog calls it out. Skill cache invalidation in AI tools
is owned by the tool — Sinfonia doesn't ship a refresh mechanism.

## Where to go next

- [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) — production topologies (the skills
  scaffold the config; this doc tells you where to run it).
- [`docs/CLIENT_SETUP.md`](CLIENT_SETUP.md) — enterprise adoption (security
  posture, audit trail, handoff).
- [`docs/MIGRATION-v0.2-to-v0.3.md`](MIGRATION-v0.2-to-v0.3.md) — if you're
  coming from v0.2 and want to adopt the bridge / state machine
  incrementally.
- [`docs/SPEC.md`](SPEC.md) §18.2 — the recommended-extension entry the
  skills system formalizes.

## Reporting bugs

If a skill produces a `WORKFLOW.md` / `BRIDGE.md` / compose snippet that
`sinfonia --check` or `sinfonia-bridge --self-test` rejects, that's a
skill bug — open an issue against the repo with the rejected output
attached.
