# Migrating from Sinfonia v0.2 to v0.3

**Audience:** anyone running a Sinfonia v0.2 deployment and considering
v0.3. **Read time:** ~10 min. **Prerequisites:** a working v0.2
install, and a calm hour with `git` and your config files.

v0.3 introduces a workspace layout, a companion `sinfonia-bridge` binary,
a new OpenCode agent backend, OpenTelemetry emission, six setup skills,
six published Docker images, and finalized documentation.

**Most existing v0.2 deployments need zero config changes to keep
working.** The daemon's behaviour against an unchanged `WORKFLOW.md` is
preserved — your tutorial install will pick up v0.3 and run identically.

This guide covers:

- [What you DON'T need to do](#what-you-dont-need-to-do)
- [Required changes](#required-changes-v02--v03)
- [Optional upgrades](#optional-upgrades)
- [Breaking changes](#breaking-changes)
- [Compatibility notes](#compatibility-notes)

---

## What you DON'T need to do

If you have a v0.2 install and you want to "stay on the polling-daemon
shape but get the bugfixes," do nothing except update the binary:

- `cargo install sinfonia` continues to install the daemon (now installs
  the bridge alongside it as a second binary; if your scripts pin to
  `--bin sinfonia`, they still work).
- Existing `WORKFLOW.md` files parse unchanged.
- The HTTP API surface (`/api/v1/state`, `/api/v1/refresh`,
  `/api/v1/<identifier>`) is unchanged.
- Existing systemd / launchd / Docker units that invoke
  `sinfonia /path/to/WORKFLOW.md --port 8080` continue to work.
- The `provider: codex_app_server` stub remains accepted with the same
  behaviour as v0.2.

If you do nothing else, v0.3 is a drop-in bugfix-and-features upgrade.

---

## Required changes (v0.2 → v0.3)

### `cargo install sinfonia` installs two binaries

The crate now produces both `sinfonia` and `sinfonia-bridge`. If your
install scripts pin to `--bin sinfonia`, they still work (the daemon
binary is unchanged). If you want only the daemon and want to keep your
bin dir clean:

```bash
cargo install sinfonia --bin sinfonia
```

To install only the bridge:

```bash
cargo install sinfonia --bin sinfonia-bridge
```

Default behaviour (both binaries) is what most users want.

### Docker image rename

`sinfonia-claude:latest` is no longer published. Replace with one of:

- `ghcr.io/o-side-systems/sinfonia:0.3.0` — daemon only.
- `ghcr.io/o-side-systems/sinfonia-with-claude-code:0.3.0` — closest to
  the v0.2 image (daemon + Node 22 + `@anthropic-ai/claude-code`).
- `ghcr.io/o-side-systems/sinfonia-all-agents:0.3.0` — daemon + all
  three CLI agents (Claude Code, Codex, OpenCode); useful if you adopt
  the state-machine pattern with different runners per state.

The old `sinfonia-claude` tag stays in the registry but is frozen at
v0.2. Migration is a single image-tag swap in your Dockerfile /
docker-compose.yml / Kubernetes Deployment.

### `Dockerfile` and `docker-compose.yml` at the repo root are now production

If you build images from the repo, the file layout changed:

- The pre-existing dev-shell `Dockerfile` (Node + Rust toolchain + Claude
  Code, bind-mount workflow) moves to `Dockerfile.dev`.
- The pre-existing dev `docker-compose.yml` moves to
  `docker-compose.dev.yml` (and its `build.dockerfile` reference is
  updated to point at `Dockerfile.dev`).
- The new root `Dockerfile` is the production multi-stage file that
  `docker-bake.hcl` drives.
- The new root `docker-compose.yml` demonstrates the production topology
  (daemon + bridge + OTel Collector + Postgres).

If you were doing `docker compose up -d`, switch to:

```bash
# old behaviour (dev shell):
docker compose -f docker-compose.dev.yml up -d

# new behaviour (production):
docker compose up -d
```

---

## Optional upgrades

None of the items below are required. Each is a discrete piece of v0.3
you can adopt independently, when it makes sense for your install.

### Adopt the state-machine pattern

If your v0.2 install uses a single global `agent:` block for every
state, v0.3's `states:` block lets you route different states to
different runners (cheap raw LLM for triage, Claude Code for
implementation, raw Haiku for review).

The `setup-state-machine` skill scaffolds the canonical pattern. See
[`skills/setup-state-machine/SKILL.md`](../skills/setup-state-machine/SKILL.md)
or run it from your AI coding tool.

The state-machine pattern is fully backward-compatible: any state not
listed in `states:` falls back to the global `agent:` block.

### Add the bridge

If you want to close the CI → fix loop, add `sinfonia-bridge`. The
canonical install path:

1. Author `BRIDGE.md` — copy [`BRIDGE.example.md`](../BRIDGE.example.md)
   and edit, OR run the `setup-bridge` skill from your AI coding tool.
2. Configure the GitHub webhook against your repo (or org).
3. Run `sinfonia-bridge BRIDGE.md --self-test` to validate.
4. Run `sinfonia-bridge BRIDGE.md` alongside your existing daemon.

[`docs/DEPLOYMENT.md`](DEPLOYMENT.md) covers the four topologies
(single-host, separate hosts, GitHub Actions, no-bridge) and the
credential model. [`docs/CLIENT_SETUP.md`](CLIENT_SETUP.md) covers the
security posture if you're rolling this out at an enterprise.

### Enable OpenTelemetry

Two env vars wire up trace emission to a Collector / Honeycomb / Datadog
/ etc.:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export SINFONIA_TENANT_ID=my-project
```

Or set the same values inside the `telemetry:` block of `WORKFLOW.md`
/ `BRIDGE.md`. When neither is set, behaviour matches v0.3.0-alpha.1
(stdout-only logging, no exporter overhead).

The `setup-telemetry` skill scaffolds a working Collector + Postgres
deployment from a guided REPL. See
[`examples/telemetry/`](../examples/telemetry/) for the reference
schema and dashboard SQL.

### Switch to the OpenCode backend

If you're running Claude Code, Codex CLI, or a raw LLM, OpenCode is now
a sibling option (`provider: opencode`). OpenCode brings LSP integration,
MCP support, and 75+ provider backends including a local-Ollama-with-LSP
path that the raw `ollama` backend can't provide.

Run `opencode auth login` once on the host, then:

```yaml
agent:
  provider: opencode
  model: anthropic/claude-sonnet-4-6   # OpenCode's provider/model wire format
```

The `setup-agent-backend` skill walks the per-backend setup.

---

## Breaking changes

**None expected in v0.3.0.** This section is the place to record any
that surface during the release-candidate window.

If you encounter a behavioural change in v0.3 that we haven't called
out here, file an issue at
<https://github.com/O-Side-Systems/sinfonia/issues> — the v0.2 → v0.3
upgrade is intended to be drop-in for the daemon, and we want to know
about any regression.

---

## Compatibility notes

- **v0.2 `WORKFLOW.md` files parse cleanly under v0.3.** The schema is
  additive: new keys (`telemetry:`, `states:`, OpenCode under `agent:`)
  are optional with sensible defaults.
- **The `provider: codex_app_server` stub remains accepted.** Behaviour
  is unchanged from v0.2.
- **The HTTP API surface is unchanged.** Existing dashboards and
  scrapers continue to work.
- **The CLI surface is additive.** `--check`, `init`, and `--self-test`
  are new. `sinfonia WORKFLOW.md --port 8080 --log-format json` is
  unchanged.
- **The internal workspace conversion to a Cargo workspace** does NOT
  affect runtime behaviour. Source-level contributors will see the
  `crates/` layout (see [`CONTRIBUTING.md`](../CONTRIBUTING.md)) but
  end users see two binaries that behave the same as v0.2's one.
- **The bridge is OPTIONAL.** If you don't want it, don't run it. Its
  config file (`BRIDGE.md`) is independent of `WORKFLOW.md`; the daemon
  does not require a bridge to function.
- **OpenTelemetry emission is OPT-IN.** If you don't configure an OTLP
  endpoint (and don't set `telemetry.otlp_endpoint`), the OTel layer is
  disabled and you pay no overhead.
- **Setup skills are OPT-IN.** You can hand-author `WORKFLOW.md` and
  `BRIDGE.md` from the example files. The skills are a convenience for
  AI-coding-tool-based scaffolding.
- **Docker images are OPT-IN.** `cargo build --release --workspace`
  remains the canonical build path; the images are for users who'd
  rather pull a tag than build.

---

## Where to go next

- [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) — pick a topology and execute.
- [`docs/CLIENT_SETUP.md`](CLIENT_SETUP.md) — enterprise adoption.
- [`docs/SKILLS.md`](SKILLS.md) — setup skills.
- [`CHANGELOG.md`](../CHANGELOG.md) — the full Added / Changed list per
  release.
