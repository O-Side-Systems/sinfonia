# Phase 7 — Documentation update

**Target:** v0.3.0
**Scope:** Make Sinfonia legible as a team-grade orchestrator, not just a single-user daemon. README rewrite, spec extensions, two new deployment guides, and the CHANGELOG entry.
**Estimated size:** ~3 000 lines of Markdown across new and updated files.
**Depends on:** Phases 1-6 all landed (or at least all locked behind feature flags) — docs that describe behavior that doesn't exist will mislead readers.
**Unblocks:** v0.3.0 release.

This phase exists because the user explicitly asked for "ample documentation at the quality one would expect for an open source project." That sets the bar:

- Every new concept has a one-paragraph "why this exists, in plain language" alongside the reference material.
- Every new config field has a documented default, type, and example value.
- Every recommended pattern (state machine, failure categorization, budget caps) has a worked example you can copy.
- Every doc file states its audience and prerequisites in the first 100 words.
- No content lives only in proposal-doc form — the doc set is self-contained.

---

## 1. Files touched in this phase

| File | Action | Owner |
|---|---|---|
| `README.md` | major rewrite of the front page | Phase 7 |
| `CHANGELOG.md` | new v0.3.0 section | Phase 7 |
| `docs/SPEC.md` | spec extensions (§11.6, §18.2) | Phase 7 |
| `docs/SKILLS.md` | new file | Phase 5 lands the stub; Phase 7 polishes |
| `docs/DEPLOYMENT.md` | new file | Phase 7 |
| `docs/CLIENT_SETUP.md` | new file | Phase 7 |
| `docs/JIRA-SCREEN-SCHEME.md` | new file | Phase 4 lands the stub; Phase 7 polishes |
| `docs/MIGRATION-v0.2-to-v0.3.md` | new file | Phase 7 |
| `WORKFLOW.example.md` | updated examples for OpenCode + state machine + telemetry | Phase 2 lands OpenCode; Phase 7 finalizes |
| `BRIDGE.example.md` | new file at repo root | Phase 1 lands the stub; Phase 7 polishes |
| `CONTRIBUTING.md` | update for workspace layout | Phase 7 |

---

## 2. README rewrite

### 2.1 Goals

The current README does a good job explaining what Sinfonia is for an individual user. After v0.3, the README has to answer three additional questions in its first screen:

1. "Can a team adopt this?" (Yes — there's a bridge that closes the feedback loop, observability, and Docker images.)
2. "Can I deploy this without writing config by hand?" (Yes — there are setup skills.)
3. "What does v0.3 add that wasn't there before?" (Bridge, OpenCode, telemetry, skills, Docker.)

### 2.2 Structure (proposed)

```markdown
# Sinfonia

> Sinfonia (n.) — Spanish/Italian for "symphony."

Sinfonia is a Rust implementation of the **Symphony Service Specification**
([`docs/SPEC.md`](docs/SPEC.md)). It runs as a long-lived daemon that polls
an issue tracker, isolates each issue in a per-issue workspace, and runs a
coding-agent session for that issue under a repo-owned `WORKFLOW.md`.

## Why v0.3 matters

(Existing content for v0.1/v0.2.)

In v0.3, Sinfonia also ships:

- **`sinfonia-bridge`** — a companion daemon that closes the CI → fix loop.
  When tests fail on the PR, the bridge transitions the ticket back to a
  configurable "needs fixes" state and lets Sinfonia run the agent again,
  with bounded attempt counts, optional failure categorization, and token /
  cost budget caps.
- **OpenCode as a first-class agent backend**, alongside Claude Code and
  Codex, and alongside the raw LLM backends.
- **OpenTelemetry emission**, tenant-tagged from day one — for answering
  "what did this cost us last month?" and similar questions with SQL.
- **Setup skills** that AI coding tools (Claude Code, OpenCode, Codex, etc.)
  can run to scaffold a working deployment without hand-editing YAML.
- **Docker images** for the supported topologies, published to
  `ghcr.io/o-side-systems`.

[Read the v0.3 deployment guide →](docs/DEPLOYMENT.md)

## Getting started

(Existing four-step tutorial, lightly updated for the new options.)

## Sinfonia vs. Symphony

(Existing table, with a row added for the bridge.)

...
```

### 2.3 What we cut

- Detailed CLI reference moves to `docs/CLI.md` (new in Phase 7 if the README needs to shrink).
- The deep config-schema table moves to `docs/CONFIG.md` (or just an in-repo `WORKFLOW.example.md` reference).
- Long debugging tips move to `docs/TROUBLESHOOTING.md` if any exist today.

The README's job is "convince a reader to keep reading and run the tutorial." Reference material lives in `docs/`.

---

## 3. `docs/SPEC.md` extensions

The current SPEC.md is the authoritative description of the Symphony spec as Sinfonia implements it. The proposal calls for two specific edits:

### 3.1 §11.5 clarification

Today §11.5 likely says "the orchestrator does not write to the tracker." We extend it:

> §11.5 — The orchestrator MUST NOT write to the issue tracker. State
> transitions, comment posting, custom-field writes, and PR-to-ticket
> mapping are NON-orchestrator responsibilities.
>
> Companion services MAY write to the tracker on the orchestrator's behalf.
> See §11.6 for the recommended bridge-service contract.

### 3.2 New §11.6 — Bridge services

A full subsection (~400 lines of Markdown) describing:

- The role of a bridge service in the Symphony architecture.
- The recommended custom-field convention (`sinfonia_attempt_count`, etc.).
- The state-transition contract (what a "Needs Fixes" state means, how cap-hit is communicated).
- The reference Sinfonia implementation (`sinfonia-bridge`).
- The minimum contract a non-Sinfonia implementation must honor.

Because §11.6 is a recommended extension, other Symphony implementations are free to ignore it. But if they do implement it, they should follow this contract so their bridge is interoperable.

### 3.3 New §11.7 — Custom-field discovery

A short section on how each tracker exposes custom fields:

- **Linear:** comment-marker payload (the v0.3 Sinfonia convention).
- **Jira:** `customfield_NNNNN` IDs resolved by display name; created via `/rest/api/3/field`.

### 3.4 §18.2 Recommended Extensions

Six new bullets per the proposal:

- CI feedback bridge with attempt-count, token, and cost custom fields
- Failure categorization with priority-based state routing
- Budget enforcement (token + cost caps) with separate `budget_exceeded_state`
- PR label management convention (`sinfonia:*` namespace)
- OpenTelemetry emission with `tenant_id` resource attribute
- OpenCode backend (alongside Codex CLI and Claude Code)

Each bullet links to the full subsection that describes it.

The SPEC stays language-agnostic. These are recommended extensions, not required conformance items.

---

## 4. `docs/DEPLOYMENT.md` (new)

The deployment guide. Audience: an engineer who's evaluated Sinfonia and wants to put it into production. About 800-1200 lines of Markdown.

### 4.1 Outline

```markdown
# Deploying Sinfonia

## Audience and prerequisites

You're evaluating Sinfonia for production use. You've read the README, run
the tutorial locally, and decided to deploy it for a team.

This guide covers:
- The supported deployment topologies (and what they trade off)
- The credential model
- Day-one operability — logs, metrics, scaling

It does not cover the spec — see [`docs/SPEC.md`](SPEC.md).

## Topology 1 — Sinfonia only, no bridge

Smallest deployment. One host. Polls the tracker every 30 s. No CI integration.
Use when:
- You only need agent automation, not the feedback loop.
- You have no GitHub repo or you're using a non-GitHub VCS.

### Diagram
(ASCII or Mermaid)

### Steps
...

## Topology 2 — Sinfonia + bridge, single host

The expected starting point for most teams. Sibling daemons.
Use when:
- You want the CI → fix loop.
- You have a public-facing webhook URL (or use ngrok / tunnel.io / Cloudflare Tunnel).

### Diagram

### Steps

### Notes on webhook reachability
...

## Topology 3 — Sinfonia + bridge, separate hosts

For shared bridges across multiple Sinfonia instances.

## Topology 4 — Sinfonia + bridge via GitHub Actions

When the bridge can't be internet-reachable.

## Credential model

Sinfonia and the bridge use different credentials. Lay them out clearly.

### Sinfonia credentials
- Tracker API key (Linear or Jira)
- Agent backend credentials (per backend)

### Bridge credentials
- Tracker API key (same as Sinfonia, but bridge needs WRITE scope)
- GitHub PAT or App credentials
- GitHub webhook secret

## Observability

Telemetry primer. Pointers to `setup-telemetry` (Phase 5).

## Scaling considerations

One Sinfonia process = one project. Run multiple processes for multiple
projects. Bridge can serve multiple projects.

## Backup and recovery

Idempotency DB backup (small). Workspace state is rebuildable.

## Upgrading

Pointers to `MIGRATION-v0.2-to-v0.3.md`.
```

Each topology section is self-contained — a reader can follow Topology 2 without reading Topology 1 first.

---

## 5. `docs/CLIENT_SETUP.md` (new)

The "you're an enterprise team adopting this" guide. About 600-800 lines.

### 5.1 Outline

```markdown
# Adopting Sinfonia in an enterprise context

## Audience and prerequisites

You're at a company evaluating Sinfonia for use against an existing
codebase, owned by an existing team, with existing CI, on existing GitHub
or Jira. You need to answer:
- Can our security team approve this?
- How do we control spend?
- How do we audit what the agent did?
- How do we hand the system off to the team that runs it day-to-day?

## Security posture summary

What credentials Sinfonia holds. What credentials the bridge holds. What
they CAN do and what they CAN'T do.

A clear "trust boundary" diagram showing:
- Sinfonia: reads tracker, writes nothing.
- Bridge: reads + writes tracker, reads PR / CI from GitHub, reacts to webhooks.
- Agent: full repo permissions inside the per-issue workspace.

## GitHub App vs PAT

When to use which. (App is recommended for enterprise; PAT is fine for a
proof of concept.)

## Budget controls

Token caps. Cost caps. Per-ticket overrides. The `budget_exceeded_state`.

## Audit trail

Telemetry as the audit log. SQL queries for "what tickets did agent X
touch in Q1?" etc.

## Failure handling

Cap-hit behavior. Bridge label namespace. CODEOWNERS interaction.

## Handoff to the operating team

What the operating team needs to know. Documentation pointers. On-call
runbook template (probably ships as `examples/runbook.md`).

## Vendor evaluation worksheet

A checklist the prospective adopter can hand to their security review team:
- "Does this tool ever execute code outside the per-issue workspace?" (No.)
- "What credentials does it persist to disk?" (Listed.)
- ...
```

This doc is the document we'd hand to a Kyros or SoFi security review. It also doubles as a marketing piece — but written for engineers, not for executives.

---

## 6. `docs/MIGRATION-v0.2-to-v0.3.md` (new)

Short, mechanical. About 300 lines.

```markdown
# Migrating from Sinfonia v0.2 to v0.3

v0.3 introduces a workspace layout, a companion bridge, a new agent backend,
telemetry, skills, and Docker images. Most existing v0.2 deployments need
zero config changes to keep working — but there are a few things to watch.

## Required changes (v0.2 → v0.3)

### `cargo install sinfonia`

The crate now installs both binaries (`sinfonia` and `sinfonia-bridge`).
If your install scripts pin to `--bin sinfonia`, they still work.

### Docker image rename

`sinfonia-claude:latest` is no longer published. Use one of:
- `ghcr.io/o-side-systems/sinfonia:0.3.0` — daemon only.
- `ghcr.io/o-side-systems/sinfonia-with-claude-code:0.3.0` — closest to the v0.2 image.

The old image stays in the registry but is frozen.

## Optional upgrades

### Adopt the state-machine pattern

(Pointer to `setup-state-machine`.)

### Add the bridge

(Pointer to `setup-bridge`.)

### Enable telemetry

(Pointer to `setup-telemetry`.)

## Breaking changes

None expected. Any breaking changes get listed here at release time.

## Compatibility notes

- v0.2 `WORKFLOW.md` files parse cleanly under v0.3.
- The `provider: codex_app_server` stub remains accepted; behavior unchanged.
- The HTTP API surface (`/api/v1/state`) is unchanged.
```

---

## 7. `WORKFLOW.example.md` — updated examples

Three new sections appended to the existing file:

1. **OpenCode example** — both a default-lane and a state-machine usage.
2. **Telemetry block** — what the `telemetry:` keys mean.
3. **State machine with failure categories** — the full pattern (Triage → Ready → Needs Fixes → category states → Human Review).

Each new section has:
- A "use this when" sentence.
- A worked YAML example.
- A short paragraph explaining each non-obvious field.

---

## 8. `BRIDGE.example.md` — new file

Sits at the repo root, mirroring `WORKFLOW.example.md`'s shape. Phase 1 lands a stub; Phase 7 turns it into a complete, copy-pasteable reference with every config field documented.

Same convention as `WORKFLOW.example.md`:

- YAML front matter with every documented field.
- Markdown body with one paragraph per field group explaining purpose and trade-offs.

---

## 9. `CONTRIBUTING.md` — update

The current `CONTRIBUTING.md` (read at planning time) documents a single-crate layout. Update for:

- The new `crates/` directory.
- Building with `cargo build --workspace` vs `cargo build -p sinfonia-bridge`.
- Running workspace-wide tests with `cargo test --workspace`.
- Where to add new code: `crates/sinfonia/` for daemon work, `crates/sinfonia-bridge/` for bridge work, `crates/sinfonia-tracker/` for shared tracker code.

---

## 10. `CHANGELOG.md` — v0.3.0 entry

```markdown
## v0.3.0 — 2026-MM-DD

### Added
- `sinfonia-bridge` companion daemon that closes the CI → tracker feedback
  loop with bounded attempt counts, optional failure categorization, and
  token/cost budget caps. ([#NNN](https://github.com/...))
- OpenCode as a first-class agent backend (`provider: opencode`).
- OpenTelemetry emission from Sinfonia and the bridge. Every span and metric
  is tagged with `tenant_id` for multi-project segmentation.
- Six setup skills under `skills/` for `WORKFLOW.md` authoring, bridge
  setup, state-machine adoption, telemetry wiring, agent backend setup,
  and Symphony migration.
- New CLI commands: `sinfonia init`, `sinfonia --check`,
  `sinfonia-bridge --self-test`.
- Six Docker images published to `ghcr.io/o-side-systems`.
- `docs/DEPLOYMENT.md`, `docs/CLIENT_SETUP.md`, `docs/SKILLS.md`,
  `docs/MIGRATION-v0.2-to-v0.3.md`.

### Changed
- Repository converted to a Cargo workspace. `src/tracker/` moved to a
  shared `crates/sinfonia-tracker/` crate so `sinfonia-bridge` can reuse
  it. Existing v0.2 `WORKFLOW.md` files parse unchanged.
- `Dockerfile` now produces six target images via `docker buildx bake`.
  The previous dev-shell Dockerfile moved to `Dockerfile.dev`; the
  previous `docker-compose.yml` moved to `docker-compose.dev.yml`.
- `docs/SPEC.md` gained §11.6 (bridge services), §11.7 (custom-field
  discovery), and six entries in §18.2 (recommended extensions).

### Fixed
- (List discovered during implementation.)

### Migration
- See [`docs/MIGRATION-v0.2-to-v0.3.md`](docs/MIGRATION-v0.2-to-v0.3.md).
```

The dates and PR numbers get filled in at release tag time.

---

## 11. Test plan

Documentation has no unit tests in the traditional sense. We do have:

### 11.1 Link-rot check

CI step that runs `lychee` (or similar) over the `docs/` directory and fails on broken internal links. External links checked weekly via a scheduled workflow, not on every PR.

### 11.2 Markdown lint

CI step that runs `markdownlint` to enforce a small style guide:
- ATX headings (`#`, not underlines).
- Reference-style links allowed but consistent within each file.
- No bare URLs.

### 11.3 Code-block validation

For every code block tagged `yaml`, `rust`, `toml`, `sql`, `bash`: run a syntax check. The proposal's `cost_table.yaml` example, the `docker-compose.yml`, etc. — all should parse cleanly with their respective tools.

### 11.4 Spec-conformance test

`tests/spec_conformance.rs` (already exists) gains a test that asserts every recommended-extension bullet in §18.2 is implemented in the codebase. Stops `docs/SPEC.md` and the code from drifting.

### 11.5 Manual verification

A reader who's never seen Sinfonia before should be able to:
1. Read the README and want to try it.
2. Run the README tutorial in under 30 minutes.
3. Follow `DEPLOYMENT.md` to a working deployment in under a day.
4. Hand `CLIENT_SETUP.md` to a security reviewer without embarrassment.

These are subjective tests. We do them by asking someone who hasn't been involved in v0.3 to read the docs and report sticking points. Findings go into a follow-up doc patch.

---

## 12. Open questions

1. **Do we ship a docs site?** Not in v0.3. Markdown in the repo is good enough. A `docs.rs`-style hosted site is a v0.4+ project.

2. **Versioning of docs?** Each markdown file has the version it applies to in its header. We don't maintain separate trees per version; users on v0.2 read v0.2 docs from `git checkout v0.2`.

3. **Translation.** English only. Punt to v0.5+.

4. **API reference?** Sinfonia is a daemon, not a library — there's no "users of the library" audience to write rustdoc for. Module-level rustdoc stays because it helps contributors; we don't publish rustdoc as a separate doc artifact.

5. **The `examples/` directory.** Phase 3 adds `examples/telemetry/`. Phase 5's skill templates also have copy-pasteable example output. Do we promote an `examples/runbook.md`, `examples/incident-response.md`, etc.? Yes — but only when we have real-world content, not speculative content.

---

## 13. Phase 7 deliverable checklist

- [ ] README rewrite per §2.
- [ ] `CHANGELOG.md` v0.3.0 entry per §10.
- [ ] `docs/SPEC.md` §11.5 / §11.6 / §11.7 / §18.2 per §3.
- [ ] `docs/SKILLS.md` polished (Phase 5 stub).
- [ ] `docs/DEPLOYMENT.md` per §4.
- [ ] `docs/CLIENT_SETUP.md` per §5.
- [ ] `docs/JIRA-SCREEN-SCHEME.md` polished (Phase 4 stub).
- [ ] `docs/MIGRATION-v0.2-to-v0.3.md` per §6.
- [ ] `WORKFLOW.example.md` updated examples per §7.
- [ ] `BRIDGE.example.md` polished (Phase 1 stub).
- [ ] `CONTRIBUTING.md` workspace update per §9.
- [ ] CI: link-rot, markdown lint, code-block syntax checks per §11.
- [ ] Manual readthrough by someone uninvolved in v0.3 implementation, with findings addressed.

When all boxes are checked, the v0.3.0 doc set is shipping-quality. Phase 7 is the final phase — once done, v0.3.0 is releasable.
