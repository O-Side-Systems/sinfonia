# Sinfonia

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.78%2B-orange.svg)](https://www.rust-lang.org)
[![Spec](https://img.shields.io/badge/spec-Symphony%20Draft%20v1-informational)](docs/SPEC.md)

> *Sinfonia (n.) — Spanish/Italian for "symphony." Pronounced sin-FOH-nee-ah.*

Sinfonia is a Rust implementation of the **Symphony Service Specification** ([`docs/SPEC.md`](docs/SPEC.md), originally published at [openai/symphony](https://github.com/openai/symphony)). It is an independent, community-maintained build — not affiliated with OpenAI.

It's a long-running daemon that polls an issue tracker, isolates each issue in a per-issue workspace, and runs a coding-agent session for that issue under a repo-owned `WORKFLOW.md` contract.

This implementation conforms to the Symphony spec (Draft v1) with three added pluggability points:

- **Issue trackers**: Linear and Jira (Cloud + self-hosted PAT).
- **Coding agents**: OpenAI, Anthropic, Google Gemini, Ollama (locally hosted) over raw LLM APIs with a built-in tool loop, **plus** the `claude` CLI (Claude Code), `codex` CLI (Codex CLI), and `opencode` CLI (OpenCode — adds LSP, MCP, and 75+ provider backends, including local Ollama with LSP) driven as subprocesses so you can leverage those products' full capabilities instead of a hand-rolled loop.
- **Per-state runner overrides**: a `states:` block in `WORKFLOW.md` routes each tracker state to a different agent + prompt — e.g. `Todo` → Claude Code, `In Progress` → Claude Code with Opus, `In Review` → raw Haiku for a quick pass.

## What's new in v0.3 (preview)

**v0.3.0-alpha.1** adds a separate `sinfonia-bridge` binary that turns GitHub CI results into tracker state transitions for the daemon to pick up — a CI failure on a PR moves the linked ticket back to a "needs fixes" state, Sinfonia's next poll routes the agent at the failure, and a per-ticket attempt counter caps the loop before runaway retries. The bridge runs alongside the existing `sinfonia` daemon; nothing about the v0.1 polling-loop behaviour has changed.

If you're upgrading from v0.1 and don't want the bridge, you don't have to do anything — `sinfonia` still runs the same way against the same `WORKFLOW.md`. If you do want the bridge:

- Start with [`BRIDGE.example.md`](BRIDGE.example.md) — fully-commented config for the new binary, parses cleanly under `sinfonia-bridge BRIDGE.example.md --check`.
- The recommended extension contract for any compatible bridge implementation is drafted in [`docs/SPEC.md` §11.6](docs/SPEC.md).
- Run `sinfonia-bridge BRIDGE.md --self-test` once you've filled in real credentials — it returns one `PASS` / `FAIL` / `SKIP` line per install-gate check.
- See [`CHANGELOG.md`](CHANGELOG.md) for the full Added / Changed / Known limitations list.

**Phase 3 (telemetry + budget enforcement, currently being landed)** layers an OPT-IN OpenTelemetry exporter over both binaries' existing `tracing` subscribers, adds a typed Sinfonia→bridge event channel for the cost / budget pipeline, and enforces per-ticket token + cost caps at the bridge's tracker-write boundary. When `OTEL_EXPORTER_OTLP_ENDPOINT` is unset and no `telemetry:` block is configured in `WORKFLOW.md` / `BRIDGE.md`, behaviour matches v0.3.0-alpha.1 — the OTel layer is disabled and the binaries run stdout-only.

**Phase 4 (Jira bridge support, currently being landed)** fills in the five `IssueTracker` bridge-write methods on the Jira side — state transitions via `POST /rest/api/3/issue/{id}/transitions`, custom-field reads/writes via cached `customfield_NNNNN` resolution, and ADF-rendered comments. Bridge config no longer rejects `tracker.kind: jira`. Self-hosted Jira (Server / Data Center) is supported via PAT-only auth. See `docs/v0.3-plan/04-jira-bridge.md` and `docs/v0.3-plan/04-jira-VERIFY.md`.

**Phase 5 (setup skills + CLI extensions, currently being landed)** ships six setup skills at `skills/` (`setup-workflow`, `setup-bridge`, `setup-state-machine`, `setup-telemetry`, `setup-agent-backend`, `migrate-from-symphony`) that AI coding tools can invoke to scaffold a working deployment without hand-editing YAML. Two CLI extensions back the skills: `sinfonia --check <WORKFLOW.md>` validates a workflow file (exit codes per failure class) and `sinfonia init` is the AI-tool-free interactive REPL. `docs/SKILLS.md` is the cross-vendor pointer table.

**Phase 6 (Docker images, currently being landed)** publishes six production images to `ghcr.io/o-side-systems/`: `sinfonia` (daemon only), `sinfonia-bridge` (bridge only), three single-agent variants (`sinfonia-with-claude-code`, `sinfonia-with-codex`, `sinfonia-with-opencode`), and the combined `sinfonia-all-agents`. A new root `docker-compose.yml` demonstrates the production topology (daemon + bridge + OTel Collector + Postgres). The pre-existing dev-shell image moves to `Dockerfile.dev` / `docker-compose.dev.yml`. See [the Docker section below](#docker).

Still alpha — Phase 7 lands finalized docs.

## Observability (Phase 3 preview)

Set the standard OTel endpoint env var to enable structured trace emission to a Collector / Honeycomb / Datadog / etc.:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export SINFONIA_TENANT_ID=kyros-web-app   # or set telemetry.tenant_id in YAML
sinfonia
sinfonia-bridge
```

Or set the same values inside the `telemetry:` block of `WORKFLOW.md` / `BRIDGE.md` — see [`BRIDGE.example.md`](BRIDGE.example.md) for the schema.

Six daemon-side spans (`orchestrator.tick`, `orchestrator.dispatch`, `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`) and six bridge-side spans (`bridge.webhook`, `bridge.ci_result`, `bridge.state_transition`, `bridge.cap_hit`, `bridge.cost_update`, `bridge.events_receive`) carry the resolved `tenant_id` as a per-span attribute. Resource-level `service.namespace = tenant_id` lets a Collector routing-processor split per-tenant data without touching emission code.

A reference Collector + Postgres deployment ships under [`examples/telemetry/`](examples/telemetry/) — schema, Collector config, and three dashboard SQL queries (tenant monthly cost, first-try rate, top-budget tickets). See [`docs/SPEC.md` §11.6.11](docs/SPEC.md) for the typed Sinfonia↔bridge event-channel contract and §11.6.12 for the budget-enforcement surface.

## Sinfonia vs. Symphony

| | Symphony | Sinfonia |
|---|---|---|
| What it is | The language-agnostic **specification** at [openai/symphony](https://github.com/openai/symphony) | An independent **Rust implementation** of that spec |
| License | (see upstream) | [Apache-2.0](LICENSE) |
| Trackers | Linear (spec required) | Linear **+ Jira** |
| Agents | Codex app-server (spec required) | OpenAI / Anthropic / Google / Ollama (raw API + built-in tool loop) plus `claude` and `codex` CLIs as subprocess backends |
| State machine | Fixed claim states | Configurable per-tracker-state runner overrides |

If you came here looking for the spec itself, jump straight to [`docs/SPEC.md`](docs/SPEC.md). If you came here looking for a daemon you can run today, keep reading.

---

## Getting started

### Mental model in one paragraph

Sinfonia is a **polling daemon**, not a webhook listener. Once running, it asks your tracker every `polling.interval_ms` (default 30 s) "which issues in this project are currently in one of these states?" Every matching issue that isn't already running gets a per-issue workspace directory and a coding-agent session. **Moving a ticket into an active state is the trigger; moving it out is the stop signal.** There is no webhook to register, no callback URL to expose, and Sinfonia doesn't need to be reachable from the internet. The tracker keeps the source of truth; Sinfonia reacts to its state.

### Prerequisites

- Rust toolchain 1.78+ (uses 2021 edition). Build with `cargo build --release`; binary lands at `target/release/sinfonia`.
- A `bash` available on `PATH` — Sinfonia uses `bash -lc` for hooks, the `shell` tool, and CLI agent subprocesses.
- One of:
  - **A direct LLM API key** (`ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `GOOGLE_API_KEY`) or a local **Ollama** at `127.0.0.1:11434`, **or**
  - **The `claude` CLI** (Claude Code) and/or **the `codex` CLI** installed and authenticated on the host.
- A Linear or Jira account with API access for the project you want to automate.

### Step 1 — Set up your tracker

Sinfonia talks to **one project per `WORKFLOW.md`**. If you want to automate three projects, run three Sinfonia processes (each with its own workflow file, workspace root, and HTTP port). The orchestrator state isn't shared across instances — that's by design.

#### Linear

1. **Get an API key.** Linear → **Settings → My Account → API → Personal API keys → Create new**. Scope it to read+write. Export it:
   ```bash
   export LINEAR_API_KEY="lin_api_…"
   ```
2. **Find your project's `slugId`.** Open the project in Linear and look at the URL:
   ```
   https://linear.app/<team>/project/my-awesome-project-abc123def456
                                       └──────────── slugId ────────┘
   ```
   The slugId is the slug after the last `/` (Linear includes a short hash at the end). Use that as `tracker.project_slug`.
3. **Decide which workflow states count as "active."** Linear's defaults are `Backlog`, `Todo`, `In Progress`, `In Review`, `Done`, `Cancelled`. The spec default is `["Todo", "In Progress"]`; widen or narrow that based on what you want Sinfonia to pick up:
   ```yaml
   tracker:
     kind: linear
     api_key: $LINEAR_API_KEY
     project_slug: my-awesome-project-abc123def456
     active_states: ["Todo", "In Progress", "In Review"]
     terminal_states: ["Done", "Cancelled", "Duplicate"]
   ```
4. **Blocker handling.** A Linear issue in `Todo` is skipped while any "blocks" relation points to a non-terminal issue. Once the blocker is closed, the next poll picks the unblocker up. (`In Progress` issues ignore blockers — the assumption is someone has decided to start them anyway.)
5. **Sanity-check the credentials** without running a full daemon:
   ```bash
   LINEAR_API_KEY=$LINEAR_API_KEY ./target/release/sinfonia WORKFLOW.md --port 0
   ```
   Check the first log line; a 401 from Linear means the key/slug is wrong.

#### Jira (Cloud)

1. **Create an API token.** id.atlassian.com → **Security → Create and manage API tokens → Create API token**.
   ```bash
   export JIRA_API_TOKEN="ATATT…"
   export JIRA_EMAIL="you@example.com"     # required for Atlassian Cloud Basic auth
   ```
2. **Identify your site URL and project key.** The site URL is your Jira instance (e.g. `https://acme.atlassian.net`). The project key is the short prefix in issue IDs — `ABC-123` → key `ABC`.
3. **Workflow states.** Use the exact state names from your Jira workflow ("To Do", "In Progress", "In Review", "Done"). State names are case-insensitive in Sinfonia's matching but should match what JQL would return.
   ```yaml
   tracker:
     kind: jira
     endpoint: https://acme.atlassian.net
     api_key: $JIRA_API_TOKEN
     email: $JIRA_EMAIL
     project_slug: ABC
     active_states: ["To Do", "In Progress", "In Review"]
     terminal_states: ["Done", "Cancelled", "Won't Do"]
   ```
4. **Blocker handling.** Jira's "is blocked by" link (issue link type `Blocks`, direction `inwardIssue`) is normalized into the same blocker check the Linear adapter uses.

#### Jira (self-hosted / DC)

Drop `email:` and set `api_key:` to a Personal Access Token. Sinfonia will use `Authorization: Bearer <token>` instead of Basic auth.

#### What Sinfonia doesn't do

- **No webhook endpoint.** There is nothing to register in Linear/Jira beyond an API key. Sinfonia asks; the tracker answers. The polling interval is the upper bound on pickup latency.
- **No multi-project per daemon.** One workflow file = one project. Run multiple daemons for multiple projects.
- **No tracker writes from Sinfonia itself.** Closing tickets, posting comments, moving states — all of that is done by the agent inside its session via `gh`, `curl`, `linear-cli`, the Linear or Jira REST API, etc. The orchestrator stays out of the ticket-write business by design (spec §11.5).
- **No team-aware ACLs.** A single API key controls what Sinfonia can see. Use a tracker user that's scoped to the project(s) you want automated.

### Step 2 — Pick an agent backend

Two flavors, mix them per state:

| When to use… | Pick this | What you need |
|---|---|---|
| You want zero install, full control over the tool loop, cheapest cost | `openai`, `anthropic`, `google`, or `ollama` | Just an API key (or a running Ollama) |
| You want Claude Code's planning, file edits, MCP tools, etc. | `claude_code` | `claude` CLI installed and logged in |
| You want Codex CLI's behavior | `codex` | `codex` CLI installed and authenticated |
| You want OpenCode's LSP integration, MCP support, and 75+ provider backends (incl. local Ollama-with-LSP) | `opencode` | `opencode` CLI installed and authenticated (`opencode auth login`) |

Auth lives where each backend expects it:

- **Raw LLM backends** read `agent.api_key` from `WORKFLOW.md`, or default to `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `GOOGLE_API_KEY` env vars. Ollama needs no key.
- **CLI backends** read no key from Sinfonia. The CLI itself owns auth — run `claude` or `codex` once interactively on the host and let it write its credential file (`~/.claude/credentials`, `~/.codex/auth.json`, etc.).

### Step 3 — Author your `WORKFLOW.md`

The file is YAML front matter + a Liquid prompt body. It lives **in your repo**, version-controlled with the code. The simplest possible one:

```yaml
---
tracker:
  kind: linear
  api_key: $LINEAR_API_KEY
  project_slug: my-awesome-project-abc123def456

agent:
  provider: anthropic
  model: claude-sonnet-4-6

workspace:
  root: ~/sinfonia-workspaces

hooks:
  after_create: |
    git clone git@github.com:my-org/my-repo.git .
  before_run: |
    git fetch --all --quiet
    git switch -c "sinfonia/{{ issue.identifier | downcase }}" 2>/dev/null || git switch "sinfonia/{{ issue.identifier | downcase }}"
---

You are picking up issue `{{ issue.identifier }}`: {{ issue.title }}.

{{ issue.description }}

When done, push your branch and open a PR. Mention the issue identifier in the PR body so
the tracker auto-links it.
```

Three things to be deliberate about:

1. **`workspace.root`** — Sinfonia reuses workspaces across runs for the same issue. Put it on a fast disk with room for cloned repos. Don't put it inside the Sinfonia source tree.
2. **`hooks.after_create`** — only runs the *first* time a workspace is created. This is where `git clone` belongs (and `npm install`, `bundle install`, etc.). If `after_create` fails, the workspace is left in a broken state — the next attempt re-runs it.
3. **`hooks.before_run`** — runs before *every* attempt. Put `git fetch`, branch switching, and pre-flight checks here. Failure aborts the current attempt; the orchestrator will retry with backoff.

Add a state machine if you want different agents for different parts of the lifecycle — see [`WORKFLOW.example.md`](WORKFLOW.example.md) for the full pattern, and the "State machine" reference section below for what each override accepts.

### Step 4 — First run

```bash
export LINEAR_API_KEY=lin_api_…
export ANTHROPIC_API_KEY=sk-ant-…
./target/release/sinfonia WORKFLOW.md --port 8080 --log-format pretty
```

You should see:

```
INFO main: sinfonia started path=WORKFLOW.md
INFO http: listening actual=127.0.0.1:8080
DEBUG orchestrator: tick start
DEBUG orchestrator: tick end
```

Open <http://127.0.0.1:8080/> for the dashboard. The page lists running issues, retry queue, token totals, and last-event timestamps. The same data is available as JSON at `/api/v1/state`.

To trigger work *right now* instead of waiting for the next poll, hit `/api/v1/refresh`:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/refresh
```

To send a test issue through the loop, in your tracker move a ticket into one of your `active_states`. Within one poll cycle, Sinfonia will create `~/sinfonia-workspaces/<ISSUE-ID>/`, run your `after_create` hook, then start an agent session.

### Step 5 — How work gets triggered (in detail)

There is exactly one trigger pattern: **"is this issue in an active state right now?"**

| Action in tracker | What Sinfonia does (within ≤ `interval_ms`) |
|---|---|
| Move ticket → `Todo` (with no open blockers) | Picks it up, runs the agent |
| Move ticket → `In Progress` | Picks it up, runs the agent (blocker rule ignored) |
| Move ticket → `Done` / `Cancelled` / any `terminal_states` value | Stops any running session, deletes the workspace |
| Move ticket → some non-active / non-terminal state | Stops any running session, **keeps** the workspace |
| Add a "blocks" relation that's not closed | A future pickup is paused; an already-running session is **not** preempted |
| Close a blocker | The blocked ticket becomes pickup-eligible on the next poll |
| Edit ticket title / description / labels | Reflected in the prompt for the **next** turn (current turn keeps its rendered prompt) |

If you need lower latency than `polling.interval_ms`, three options:

- Lower the interval (`polling.interval_ms: 10000`). Fine, costs ~3× more tracker API calls.
- Hit `POST /api/v1/refresh` from a tiny webhook receiver you maintain (Linear webhooks → curl).
- Edit `WORKFLOW.md` — the file watcher triggers an immediate re-poll on any change.

The orchestrator's per-tick budget is bounded by tracker latency. With ~30 active issues on Linear and a healthy network, a tick is well under a second.

### Step 6 — Operating the daemon

- **Logs.** `tracing-subscriber` writes structured logs to stdout. Tune with `RUST_LOG=sinfonia=info,orchestrator=debug,runner=info`. JSON format for log aggregators via `--log-format json`.
- **Reload without restart.** Edit `WORKFLOW.md`. The `notify`-backed watcher debounces and re-applies config atomically. Invalid changes are rejected loudly and the last known good config keeps serving traffic.
- **Stop a runaway session.** Move the ticket to a terminal/non-active state. The next reconciliation tick will stop the agent and (for terminal) clean the workspace.
- **Crash recovery.** State is in-memory. On restart, Sinfonia does a one-shot terminal-state sweep to clear stale workspaces, then begins polling normally. In-flight sessions and retry timers don't survive a restart (spec §14.3).
- **Concurrency caps.** `agent.max_concurrent_agents` is the global ceiling. `agent.max_concurrent_agents_by_state` lets you cap individual states (e.g. allow 5 `Todo` runs but only 2 `In Progress` to avoid stomping on each other).
- **HTTP surface.** Loopback by default. Don't expose it directly; put a reverse proxy in front if you need remote visibility.

### Team workflow patterns

#### Recommended: triage → autopilot → human-review handoff

Set up three states and route each to the right runner:

```yaml
states:
  Triage:
    provider: anthropic           # cheap raw LLM; just labels / scopes the work
    model: claude-haiku-4-5-20251001
    prompt: |
      Read the issue and assess: scope, risk, suggested approach. Comment back on the
      ticket and transition to "Ready" if the request is clear, or "Needs Clarification"
      if not.
  Ready:
    provider: claude_code         # full agent loop for implementation
    model: claude-opus-4-7
    turn_timeout_ms: 5400000
    prompt: |
      Implement {{ issue.identifier }}. Push to a branch named sinfonia/{{ issue.identifier }}
      and open a PR. Transition to "Human Review" when the PR exists and CI is green.
  "Human Review":
    provider: codex               # let codex pick at lint/style issues while you sleep
    model: gpt-5-mini             # use whatever your `codex` install supports
    prompt: |
      Address the latest review comments on the PR for {{ issue.identifier }}. Don't change
      anything that wasn't called out. When all comments are addressed, leave a status comment
      and stop.
```

**Why this shape works:**

- Triage is a cheap pass that filters underspecified tickets out of the expensive lane.
- "Ready" is the only state that does heavy implementation work — predictable budget.
- "Human Review" never bypasses a human. The agent only pushes follow-up commits in response to comments and stops; humans still merge.
- A human moving the ticket to `Cancelled` or `Done` stops Sinfonia immediately — no orphan sessions.

#### Repo conventions to adopt

- **`WORKFLOW.md` lives in the repo's root**, alongside `README.md`. Reviewed via PR like any other code.
- **Branch naming.** Have the prompt mandate `sinfonia/<issue-id>` (or your equivalent) so it's obvious which branches came from automation.
- **PR labels.** Add a `bot:sinfonia` label in the PR body — useful for CODEOWNERS rules, automerge gates, and dashboards.
- **CODEOWNERS.** Make Sinfonia-touched paths require human review. The agent can address comments but cannot merge.
- **CI gates.** Tests + linters block merge. Sinfonia agents are good at writing code; they're not infallible.
- **Idempotent hooks.** `after_create`/`before_run` need to be safe to re-run because retries will re-run them. `git switch -c X 2>/dev/null || git switch X` is the canonical pattern.

#### What to *not* let it do

- **Don't let it merge its own PRs.** Even with a CODEOWNERS rule, don't give the bot user merge permission. The whole value is the human checkpoint.
- **Don't put secrets in the workspace.** Hooks and the `shell` tool run as your user. Mount real secrets through env vars set on the Sinfonia process, not on disk inside the workspace.
- **Don't share a workspace across tickets.** Sanitization makes that impossible by construction, but if you write a custom hook that does `cp -r ../shared/* .` you're back in trouble. Bootstrap fresh.
- **Don't enable `codex` or `claude_code` on a workstation you also use interactively.** The agents run unattended. Give them a dedicated user account or a container.

#### Multi-repo, multi-project layout

Run one Sinfonia process per project. Typical systemd / launchd / Docker layout:

```
/etc/sinfonia/web-app/WORKFLOW.md          (Linear project: Web App, port 8081)
/etc/sinfonia/mobile/WORKFLOW.md           (Linear project: Mobile,  port 8082)
/etc/sinfonia/infra/WORKFLOW.md            (Jira project: INFRA,     port 8083)
```

```bash
sinfonia /etc/sinfonia/web-app/WORKFLOW.md --port 8081 --log-format json
sinfonia /etc/sinfonia/mobile/WORKFLOW.md  --port 8082 --log-format json
sinfonia /etc/sinfonia/infra/WORKFLOW.md   --port 8083 --log-format json
```

Each process has its own `workspace.root` and its own concurrency budget. Aggregate the JSON `/api/v1/state` endpoints into one dashboard if you want a unified view.

#### Observability for a team

- Forward JSON logs to your aggregator (Datadog, Loki, etc.). Filter on `target=runner` for per-issue lifecycle, `target=orchestrator` for tick-level scheduling decisions.
- Alert on `WARN orchestrator: candidate fetch failed` lasting more than 5 minutes — usually a tracker outage or expired credential.
- Alert on the `seconds_running` aggregate growing without `total_tokens` keeping pace — usually a stalled session that's about to be killed by `stall_timeout_ms`.

#### Cost controls

- Set `agent.max_concurrent_agents` deliberately. Each running session is paying for tokens or for CLI subprocesses.
- Set `agent.max_turns` low (4–8). The continuation-retry loop will pick up where it left off on the next active poll, so a low per-worker turn cap is a natural circuit breaker.
- Use a state machine to route the bulk of polls to a cheap raw-LLM pass, and only escalate the few that need full Claude Code / Codex CLI runs.

---

## Build

```
cargo build --release
```

The binary lands at `target/release/sinfonia`.

## Run

```
sinfonia [WORKFLOW.md] [--port 8080] [--log-format pretty|json]
```

If you omit the path, Sinfonia reads `./WORKFLOW.md` from the cwd (spec §5.1).

See [`WORKFLOW.example.md`](WORKFLOW.example.md) for a complete example with both a Linear and Jira tracker block and an LLM block you can edit in place.

### Environment variables

- `LINEAR_API_KEY` — default value for `tracker.api_key` when `tracker.kind: linear`.
- `JIRA_API_TOKEN` and `JIRA_EMAIL` — default value for Jira auth.
- `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `GOOGLE_API_KEY` — default value for `agent.api_key` when the matching provider is selected. Ollama needs no key.
- `RUST_LOG=sinfonia=info,orchestrator=debug,…` — standard `tracing-subscriber` filter.

The `$VAR` shorthand inside `WORKFLOW.md` is resolved at parse time (spec §6.1).

## Docker

Phase 6 publishes six production images to `ghcr.io/o-side-systems/`:

| Image                          | Contents                                       | Audience                                                |
|--------------------------------|------------------------------------------------|---------------------------------------------------------|
| `sinfonia`                     | The `sinfonia` binary + bash / git / curl / gh | Anyone running just the daemon                          |
| `sinfonia-bridge`              | The `sinfonia-bridge` binary only              | Bridge-as-standalone-host deployments                   |
| `sinfonia-with-claude-code`    | `sinfonia` + Node 22 + `@anthropic-ai/claude-code` | Users on Claude Code                                |
| `sinfonia-with-codex`          | `sinfonia` + `codex` CLI                       | Users on Codex                                          |
| `sinfonia-with-opencode`       | `sinfonia` + `opencode` binary                 | Users on OpenCode                                       |
| `sinfonia-all-agents`          | `sinfonia` + all three CLI agents              | State-machine deployments routing across agents         |

Each is tagged with the full semver (`:0.3.0`), the moving minor (`:0.3`), and `:latest`. Built for `linux/amd64` and `linux/arm64` where the underlying CLI supports it.

### Production compose

The new root `docker-compose.yml` demonstrates the production topology: `sinfonia` (using `sinfonia-all-agents`) + `sinfonia-bridge` + an OTel Collector + Postgres. It bind-mounts your local `WORKFLOW.md` / `BRIDGE.md` and per-agent credential directories (`~/.claude`, `~/.codex`, `~/.opencode`) read-only — the in-container CLIs auth through your existing local credentials.

```bash
# Required (the bridge HMAC and Postgres passwords are user-chosen).
export LINEAR_API_KEY="lin_api_…"
export GH_WEBHOOK_SECRET="$(openssl rand -hex 32)"
export POSTGRES_PASSWORD="$(openssl rand -hex 16)"
docker compose up -d
```

The `setup-bridge` and `setup-telemetry` skills (Phase 5) generate the same compose shape from a guided REPL.

### Dev compose

The pre-existing dev-shell image (Node + Rust toolchain + Claude Code + `gh`, intended for the `--dangerously-skip-permissions` bind-mount workflow) is unchanged — it lives at `Dockerfile.dev` / `docker-compose.dev.yml`:

```bash
docker compose -f docker-compose.dev.yml up -d daemon
# interactive shell:
docker compose -f docker-compose.dev.yml run --rm shell
```

### Building images locally

```bash
# build all six targets:
docker buildx bake

# or one at a time:
docker build --target sinfonia -t sinfonia:dev .
```

`docker-bake.hcl` is the source of truth for which targets exist and which tags they get. The publish pipeline at `.github/workflows/docker-publish.yml` runs `docker buildx bake --push` on every `v*` tag, then runs Trivy with `severity: CRITICAL,HIGH` against each pushed image.

## What's where

| Module                       | What it does                                                                 |
|------------------------------|-------------------------------------------------------------------------------|
| `src/config/`                | `WORKFLOW.md` loader + typed config + file watcher (spec §5, §6).            |
| `src/template.rs`            | Strict Liquid prompt rendering (spec §12).                                   |
| `src/tracker/`               | `IssueTracker` trait + Linear (GraphQL) + Jira (REST) adapters (spec §11).   |
| `src/agent/`                 | `CodingAgent` trait + OpenAI/Anthropic/Google/Ollama tool-using backends.    |
| `src/workspace/`             | Per-issue dirs + lifecycle hooks with timeouts (spec §9).                    |
| `src/orchestrator/`          | Single-authority state machine, polling, retries, reconciliation (spec §7-8).|
| `src/http/`                  | Optional dashboard + `/api/v1/{state,refresh,<identifier>}` (spec §13.7).    |
| `src/main.rs`                | CLI binary, logging, watcher wiring.                                         |

## State machine (per-state runners)

The `states:` block in `WORKFLOW.md` lets you swap the runner, model, command, prompt, and turn timeout per tracker state. Anything you don't set falls back to the global `agent:` defaults. Example:

```yaml
agent:
  provider: anthropic
  model: claude-sonnet-4-6     # default for any state not listed below

states:
  Todo:
    provider: claude_code      # use the `claude` CLI for initial investigation
    model: claude-sonnet-4-6
    prompt: |
      Investigate {{ issue.identifier }} and sketch a plan in .sinfonia/plan.md
  "In Progress":
    provider: claude_code      # implementation pass on a stronger model
    model: claude-opus-4-7
    turn_timeout_ms: 5400000
  "In Review":
    provider: anthropic        # cheap raw-LLM review pass, no subprocess
    model: claude-haiku-4-5-20251001
```

Provider values:

| `provider:` value | What it drives | Authentication |
|---|---|---|
| `openai` / `anthropic` / `google` / `ollama` | Raw LLM HTTP API + built-in tool loop (`shell`, `read_file`, `write_file`, `edit_file`, `list_dir`, `finish`). | Provider env var (`OPENAI_API_KEY`, etc.) or explicit `api_key:` |
| `claude_code` | Subprocess: `claude -p --output-format json --verbose --dangerously-skip-permissions` in the workspace, with `--resume <session_id>` on continuation turns. | Handled by the `claude` CLI itself |
| `codex` | Subprocess: `codex exec --json` in the workspace, with `--thread <id>` on continuation turns where supported. | Handled by the `codex` CLI itself |
| `opencode` | Subprocess: `opencode run --format json` in the workspace, prompt piped over stdin, with `--session <id>` on continuation turns. OpenCode adds LSP integration and MCP, and routes internally to 75+ providers (use `model: provider/model` syntax, e.g. `anthropic/claude-sonnet-4-6` or `ollama/qwen2.5-coder:32b`). | Handled by the `opencode` CLI itself (run `opencode auth login` once on the host) |
| `codex_app_server` | Stub for the original Codex app-server protocol (not implemented). | n/a |

The default `command:` for `claude_code` and `codex` is set automatically; override `command:` to add flags or pick a different binary. The prompt is delivered via stdin so length/escaping doesn't matter.

When the tracker state changes mid-run and routes to a different runner, the current worker exits cleanly and the orchestrator re-dispatches under the new runner.

## Trust posture

Hooks and any `shell` tool calls execute under your user with `bash -lc` inside the workspace directory. **Treat `WORKFLOW.md` and the LLM as fully trusted** — they can run arbitrary commands in the workspace. Run Sinfonia under a dedicated user or inside an OS sandbox if either is in doubt (spec §15.5).

## Tracker writes

Sinfonia does not move tickets, post comments, or update fields itself (spec §11.5). When the LLM decides to communicate back to Linear/Jira, it does so by calling the `shell` tool (e.g. `gh`, `curl`, `linear-cli`, `jira`). This keeps the orchestrator out of the ticket-write business and lets each team's workflow prompt encode its own conventions.

## Conformance scorecard

Of spec §18.1 "Required for Conformance":

- ✅ Workflow path selection (explicit + cwd default)
- ✅ `WORKFLOW.md` loader (YAML front matter + body split)
- ✅ Typed config layer (defaults + `$VAR` resolution)
- ✅ Dynamic workflow watch + reload
- ✅ Polling orchestrator with single-authority state
- ✅ Tracker client (candidate / state refresh / terminal fetch)
- ✅ Workspace manager with sanitized per-issue dirs
- ✅ Workspace lifecycle hooks with timeout config
- ✅ Strict prompt rendering (`issue`, `attempt`)
- ✅ Exponential retry queue + continuation retries after normal exit
- ✅ Configurable retry backoff cap
- ✅ Reconciliation that stops runs on terminal/non-active states
- ✅ Workspace cleanup for terminal issues
- ✅ Structured logs (`issue_id`, `issue_identifier`, `session_id`)
- ✅ Operator-visible observability (logs + optional HTTP)
- ⚠️ Coding-agent app-server client: this build ships OpenAI/Anthropic/Google/Ollama backends and a `codex` stub. If you target the original Codex app-server protocol, wire it up in `src/agent/codex_stub.rs`.

Spec §18.2 "Recommended Extensions":

- ✅ HTTP dashboard + JSON API
- ✅ Jira tracker adapter
- ✅ OpenCode CLI backend (`provider: opencode`) — driven as a subprocess like `claude_code` / `codex`; brings LSP integration, MCP support, and 75+ provider routes (including local Ollama-with-LSP).
- ⏳ `linear_graphql` client-side tool — wiring exists on the tracker trait but the LLM tool catalog does not currently expose it.
- ⏳ Persistent retry queue across restarts.
