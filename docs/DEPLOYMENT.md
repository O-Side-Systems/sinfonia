# Deploying Sinfonia

**Audience:** an engineer who has evaluated Sinfonia, run the README tutorial
locally, and now wants to deploy it for a team. **Read time:** ~25 min start
to finish; you can skip to the topology that matches your install.
**Prerequisites:** the README understood, a tracker (Linear or Jira) with API
access, and a target host (or container platform) to run the daemon on.

This guide covers:

- The supported deployment topologies and what they trade off.
- The credential model — who holds what, why, and which scopes are required.
- Day-one operability — logs, metrics, scaling, backup, and upgrade.

It does NOT cover the spec — see [`docs/SPEC.md`](SPEC.md). It does not
cover writing `WORKFLOW.md` from scratch — see
[`WORKFLOW.example.md`](../WORKFLOW.example.md) and
[`docs/SKILLS.md`](SKILLS.md) (the `setup-workflow` skill).

---

## At a glance

| Topology | Hosts | Webhook needed? | Best for |
|---|---|---|---|
| [1 — daemon only](#topology-1--sinfonia-only-no-bridge) | 1 | No | Solo / proof of concept, or non-GitHub VCS |
| [2 — daemon + bridge, single host](#topology-2--sinfonia--bridge-single-host) | 1 | Yes | The expected starting point for most teams |
| [3 — daemon + bridge, separate hosts](#topology-3--sinfonia--bridge-separate-hosts) | 2+ | Yes | Shared bridge across multiple Sinfonia instances |
| [4 — bridge as GitHub Actions](#topology-4--sinfonia--bridge-via-github-actions) | 1 daemon + GHA | No | Bridge cannot be internet-reachable |

Each section below is self-contained — you can skip straight to the
topology that matches your situation.

---

## Topology 1 — Sinfonia only, no bridge

The smallest possible deployment. One host. One daemon process polling the
tracker every `polling.interval_ms` (default 30 s). No GitHub integration
on the Sinfonia side — your `WORKFLOW.md` prompt is responsible for any
PR / branch / comment writes via the agent's `shell` tool (typically `gh`,
`linear-cli`, `jira`, or `curl`).

Use when:

- You only want agent automation, not the closed CI → fix loop.
- You have no GitHub repo, or you use a non-GitHub VCS (GitLab / Bitbucket /
  Gitea / Forgejo).
- You are proving the concept on a single project before scaling up.

### Diagram

```
            ┌─────────────────────────┐
            │      Tracker            │
            │ (Linear / Jira)         │
            └──────────┬──────────────┘
                       │ poll every 30 s
                       │ (HTTPS, read-only API key)
                       ▼
            ┌─────────────────────────┐
            │     sinfonia            │
            │   (one process)         │
            ├─────────────────────────┤
            │  workspace.root/        │
            │    ENG-101/             │
            │    ENG-104/             │
            │    ...                  │
            └─────────────────────────┘
```

### Steps

1. **Build the binary** (or pull `ghcr.io/o-side-systems/sinfonia:0.3.0`):

   ```bash
   git clone https://github.com/O-Side-Systems/sinfonia
   cd sinfonia
   cargo build --release -p sinfonia
   # binary: target/release/sinfonia
   ```

2. **Author `WORKFLOW.md`.** Either run `sinfonia init` for the interactive
   REPL, run the `setup-workflow` skill from your AI coding tool, or copy
   `WORKFLOW.example.md` and edit. Validate before running:

   ```bash
   sinfonia --check WORKFLOW.md
   ```

3. **Export credentials.** The daemon reads its tracker key from
   `tracker.api_key` (which typically reads `$LINEAR_API_KEY` or
   `$JIRA_API_TOKEN` via `$VAR` substitution). For CLI agent backends
   (`claude_code` / `codex` / `opencode`), auth is owned by the CLI itself
   — run `claude` / `codex` / `opencode auth login` once on the host so the
   credential file lands in `~/.claude/` / `~/.codex/` / `~/.opencode/`.

4. **Run under a process supervisor.** Recommended layout for systemd:

   ```ini
   # /etc/systemd/system/sinfonia.service
   [Unit]
   Description=Sinfonia
   After=network-online.target
   Wants=network-online.target

   [Service]
   Type=simple
   User=sinfonia
   WorkingDirectory=/etc/sinfonia
   Environment=LINEAR_API_KEY=lin_api_…
   Environment=ANTHROPIC_API_KEY=sk-ant-…
   Environment=RUST_LOG=sinfonia=info
   ExecStart=/usr/local/bin/sinfonia /etc/sinfonia/WORKFLOW.md --port 8080 --log-format json
   Restart=on-failure
   RestartSec=5

   [Install]
   WantedBy=multi-user.target
   ```

   Equivalent macOS launchd / Windows service / Kubernetes Deployment are
   one-liners — the binary takes the `WORKFLOW.md` path as a positional
   argument and listens on `--port` for the dashboard. The `Restart=on-failure`
   directive is important: in-flight session state is in-memory only (spec
   §14.3), and restarts re-sync from the tracker on the next tick.

5. **Verify.** Point your browser at `http://<host>:8080/` for the
   dashboard. Move a sandbox ticket into one of your `active_states`. Within
   `polling.interval_ms` the dashboard should show one running session and
   `~/sinfonia-workspaces/<ISSUE-ID>/` should be populated.

### Notes

- One Sinfonia process = one tracker project. Run multiple processes for
  multiple projects (each with its own port and workspace root).
- No webhook is required. Sinfonia is fully pull-based; the tracker doesn't
  need to know anything about Sinfonia's network location.
- For lower latency than `polling.interval_ms`, you can run a tiny webhook
  receiver that calls `POST /api/v1/refresh` — but most teams don't need
  this for v0.3.

---

## Topology 2 — Sinfonia + bridge, single host

The expected starting point for most teams. Two sibling processes on one
host: the daemon polls the tracker, the bridge consumes GitHub webhooks
and writes feedback back into the tracker so the daemon picks up the
"needs fixes" state on the next poll.

Use when:

- You want the closed CI → fix loop.
- You have a public-facing webhook URL (or a tunneling service —
  ngrok / Cloudflare Tunnel / Tailscale Funnel work, with the caveats
  documented in the webhook-reachability section).

### Diagram

```
   GitHub                     ┌─── Tracker (Linear / Jira) ───┐
     │                        │                                │
     │ webhook (HTTPS,         │   poll (HTTPS, read-only)      │
     │  HMAC-SHA256 signed)    │                                │
     ▼                        ▼                                ▼
  ┌──────────────────┐    ┌──────────────────┐
  │  sinfonia-bridge │◀──▶│      sinfonia    │
  │   (port 8081)    │    │   (port 8080)    │
  └──────────────────┘    └──────────────────┘
        │     ▲
        │     │ typed event channel (POST /api/v1/sinfonia-events,
        │     │ HMAC-SHA256 signed; same scheme as GitHub webhooks)
        │     │
        ▼     │
   GitHub PR / labels / status checks
```

### Steps

1. **Build / pull both binaries.**

   ```bash
   # cargo
   cargo build --release --workspace
   # binaries: target/release/{sinfonia, sinfonia-bridge}

   # OR docker
   docker pull ghcr.io/o-side-systems/sinfonia-all-agents:0.3.0
   docker pull ghcr.io/o-side-systems/sinfonia-bridge:0.3.0
   ```

   For docker, the production `docker-compose.yml` at the repo root
   demonstrates the full topology (daemon + bridge + OTel Collector +
   Postgres). See the [Docker section of the README](../README.md#docker)
   for the launch sequence.

2. **Author both config files.**

   - `WORKFLOW.md` — same as Topology 1, plus a `telemetry:` block if you
     want the typed event channel (recommended) and a `states:` block that
     routes to your bridge's `needs_fixes_state`. The `setup-state-machine`
     skill (or `WORKFLOW.example.md`'s state-machine section) gives you a
     working template.
   - `BRIDGE.md` — copy `BRIDGE.example.md` and edit. The minimum bridge
     config needs the tracker block (same shape as `WORKFLOW.md`'s),
     a GitHub credential block (PAT or App — see step 4), an HMAC
     `webhook_secret`, and the `feedback_loop` block (state names and the
     attempt cap).

   Validate both:

   ```bash
   sinfonia --check WORKFLOW.md
   sinfonia-bridge BRIDGE.md --check
   ```

3. **Decide on webhook reachability.** GitHub needs to POST to your bridge.
   Three common patterns:

   - **Direct public DNS + TLS.** The bridge listens on
     `server.bind: 0.0.0.0`, `server.port: 8081`, fronted by nginx /
     Caddy / a load balancer terminating TLS. Set
     `server.public_url: https://bridge.example.com` so `--self-test`
     can probe `/health` from the outside.
   - **Tunneling service** (Cloudflare Tunnel, Tailscale Funnel, ngrok,
     etc.). Useful for evaluation, single-developer installs, and
     proof-of-concept demos. Set `server.public_url` to the tunnel's
     hostname. The bridge does NOT change behaviour; the tunnel terminates
     TLS for you.
   - **GitHub Actions as the bridge runner** (Topology 4) when none of the
     above is acceptable.

4. **Pick a GitHub auth mode.** Two modes are mutually exclusive per bridge
   install:

   - **PAT mode** — fastest path to a working install. Required scopes:
     `repo` (full) for label / comment writes, `read:org` for cross-repo
     PR resolution. Recommended for evaluation and single-org installs.
   - **App mode** — recommended for production. The bridge mints
     per-installation access tokens, scoped to the orgs / repos that
     installed the App. Tokens auto-rotate. Set `github.app_id` AND
     `github.private_key`; comment out `github.pat`.

   See [`docs/CLIENT_SETUP.md` § "GitHub App vs PAT"](CLIENT_SETUP.md)
   for the longer comparison.

5. **Configure the GitHub webhook.** In your repo (or org), Settings →
   Webhooks → Add webhook:

   - Payload URL: `https://<server.public_url>/webhook`
   - Content type: `application/json`
   - Secret: the same value you set for `github.webhook_secret` in
     `BRIDGE.md`.
   - Events: at minimum, `Pull requests`, `Check suites`, `Workflow runs`.
     (`Pushes` are NOT needed — the bridge keys off PR-to-ticket mapping
     from the PR body.)

   The bridge verifies the HMAC-SHA256 signature on every inbound webhook
   (SPEC §11.6.5) and rejects mismatches with HTTP 401.

6. **Run `--self-test` BEFORE going live.** The install gate:

   ```bash
   sinfonia-bridge BRIDGE.md --self-test
   ```

   Returns one `PASS` / `FAIL` / `SKIP` line per check (config, GitHub auth,
   webhook reachability, tracker, custom fields). Exit code = number of
   `FAIL` lines. Fix any `FAIL` before starting the bridge.

7. **Provision Jira custom fields (Jira only).** On first start against a
   Jira project, the bridge creates the seven `sinfonia_*` custom fields
   automatically via `POST /rest/api/3/field`. The screen-scheme bind step
   requires admin perms; if it fails, the bridge logs a `WARN` and the
   fields are still writable via REST. Follow
   [`docs/JIRA-SCREEN-SCHEME.md`](JIRA-SCREEN-SCHEME.md) for the manual
   bind (one-time per project).

8. **Run both processes under your process supervisor.** Same pattern as
   Topology 1; just add a second `sinfonia-bridge.service` unit. The two
   processes share no state — the daemon writes nothing to the bridge's
   SQLite, and the bridge writes nothing to the daemon's in-memory state.
   The typed event channel is the ONLY runtime coupling.

### Webhook reachability — gotchas

- **Local-only deployments** (the bridge listening on `127.0.0.1` because
  there is no public URL) can still test the wiring with `gh webhook
  forward` or `smee.io`. Production should not use these.
- **Self-hosted GitHub Enterprise** works the same way; the bridge does
  not depend on `github.com` specifically.
- **HTTPS is mandatory** for any internet-reachable webhook endpoint.
  GitHub will accept HTTP URLs but the secret is then transmitted in
  cleartext on every delivery.
- **Burstiness.** GitHub can deliver dozens of webhooks within a few
  seconds when a CI pipeline finishes (`check_suite` + `workflow_run` +
  one `pull_request` for the status synchronize). The bridge's SQLite
  idempotency layer (delivery-ID dedupe; SPEC §11.6.5) handles this
  cleanly — pin the bridge to a host with reasonable IO.

---

## Topology 3 — Sinfonia + bridge, separate hosts

Two (or more) hosts: the bridge on a public-facing webhook host, one or
more Sinfonia daemons elsewhere. Useful when:

- You want a single bridge serving multiple Sinfonia instances (e.g.
  three Linear projects all on the same GitHub org).
- The Sinfonia daemons run on machines that *can* reach the bridge over
  the network but *cannot* be public themselves (e.g. behind a corp
  firewall, or on developer workstations).
- You want the GitHub-credential blast radius (the bridge's PAT or App
  private key) on a separate machine from the agent backends and the
  tracker key.

### Diagram

```
   GitHub                          ┌─── Tracker ───┐
     │                             │               │
     │ webhook (HTTPS)             │ poll          │ poll
     ▼                             ▼               ▼
   ┌──────────────────┐    ┌─────────────┐  ┌─────────────┐
   │  sinfonia-bridge │◀──▶│  sinfonia A │  │  sinfonia B │
   │  (public host)   │    │  (project A)│  │  (project B)│
   └──────────────────┘    └─────────────┘  └─────────────┘
       single-writer of            │
       sinfonia_* fields           │
       on tracker                  │ typed event channel,
       (per tracker project)       │ HMAC-SHA256 signed,
                                   │ over private network
                                   ▼
                            POST /api/v1/sinfonia-events
                            on the bridge
```

### Wiring notes

- **One bridge per tracker project.** Each `BRIDGE.md` is scoped to one
  tracker project (just like `WORKFLOW.md`). If you have three projects,
  run three bridge instances. The bridges can share the same host (just
  bind different ports) or be split.
- **Multiple Sinfonia daemons can register against one bridge** via the
  typed event channel. Each daemon's `telemetry.sinfonia_events_secret`
  MUST match the bridge's. The bridge's
  `POST /api/v1/sinfonia-events` is the receiver; the daemon's
  `POST /api/v1/events/subscribers` registers the bridge's callback URL.
- **Network rules.** Inbound to the bridge from GitHub (one CIDR — see
  `https://api.github.com/meta`) and from each daemon's IP. Outbound
  from the bridge to the tracker and to GitHub. Outbound from each
  daemon to the bridge and to the tracker.
- **Storage.** Each daemon owns its own workspace root; the bridge owns
  its SQLite (delivery-ID dedupe + PR↔ticket map). These should NOT be
  shared across machines — the SQLite is single-writer.

The webhook-reachability and GitHub-auth notes from Topology 2 carry
over unchanged.

---

## Topology 4 — Sinfonia + bridge via GitHub Actions

When the bridge CANNOT be internet-reachable (corp policy forbids
inbound HTTPS, no available public DNS, no admin willing to provision a
load balancer), an alternative is to run the bridge as a one-shot GitHub
Actions job triggered by `workflow_run` and `check_suite` events.

This trades latency (a GitHub Actions cold start takes ~30-60s) for
reachability (zero inbound networking required).

### Diagram

```
              ┌──────────────── GitHub ────────────────┐
              │                                          │
              │ check_suite / workflow_run events        │
              │       trigger:                           │
              │  .github/workflows/sinfonia-bridge.yml   │
              │                                          │
              │     ┌──────────────────────────┐        │
              │     │  sinfonia-bridge --once  │        │
              │     │  (GitHub-hosted runner   │        │
              │     │   or self-hosted)        │        │
              │     └──────────────────────────┘        │
              └────────────────┬─────────────────────────┘
                               │ writes (state transitions,
                               │  marker comment, labels) over
                               │  HTTPS to the tracker
                               ▼
                          ┌────────────┐
                          │  Tracker   │
                          └────────────┘
```

### Trade-offs

- **Latency.** A red CI run → bridge action triggers → ~30-60s cold-start
  → bridge writes back to the tracker. The Sinfonia daemon picks up the
  new tracker state on its next poll. End-to-end latency is roughly
  `cold_start_s + polling.interval_ms`.
- **State.** The bridge's SQLite idempotency layer doesn't carry across
  workflow runs. Webhook duplicate-delivery dedupe is not a concern (GHA
  fires once per event); the PR↔ticket map is recomputed from the PR
  body each run (the regex-based mapping is stateless).
- **Cost.** A few minutes of GitHub-hosted runner time per CI completion.
  For a busy repo, consider a self-hosted runner.

### Skeleton workflow

```yaml
# .github/workflows/sinfonia-bridge.yml
name: sinfonia-bridge
on:
  check_suite:
    types: [completed]
  workflow_run:
    types: [completed]
  pull_request:
    types: [opened, synchronize, reopened, closed]

jobs:
  bridge:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write
      issues: write
    steps:
      - uses: actions/checkout@v4
      - name: Run bridge once
        env:
          LINEAR_API_KEY: ${{ secrets.LINEAR_API_KEY }}
          GH_WEBHOOK_SECRET: not-used-in-once-mode
          GH_PAT: ${{ secrets.GITHUB_TOKEN }}
        run: |
          docker run --rm \
            -v ${{ github.workspace }}/.sinfonia:/cfg \
            -e LINEAR_API_KEY -e GH_PAT \
            ghcr.io/o-side-systems/sinfonia-bridge:0.3.0 \
            /cfg/BRIDGE.md --once \
            --event-name "${{ github.event_name }}" \
            --event-payload "${{ toJson(github.event) }}"
```

> Note: `--once` mode is a v0.3.1 candidate. As of v0.3.0 the bridge
> always runs as a server. If you need this topology before v0.3.1,
> run the bridge as a one-shot via the existing webhook handler:
> POST the event to `http://localhost:8081/webhook` from within the
> Action, then `kill` the bridge.

---

## Credential model

Four credential categories. Sinfonia and the bridge use different keys
even when they target the same external system.

### Tracker credentials

| Process | Why | Required scope |
|---|---|---|
| Daemon (Sinfonia) | Poll candidate issues, fetch state, fetch terminal issues. **Read-only.** | Linear: any user-scoped API key. Jira: project-read on Cloud or `read` on self-hosted. |
| Bridge | Read counter / marker; write state transitions, marker comment, custom fields. **Read + write.** | Linear: same user-scoped key with comment-create allowed. Jira: `Edit Issues` + `Manage Workflow Transitions` + `Add Comments` + admin-on-project for first-run field provisioning. |

You CAN share a single tracker key across daemon + bridge if the key has
both reader and writer perms — but you SHOULD use two keys so revoking
the bridge doesn't take down the daemon (and vice versa).

### GitHub credentials

| Process | Why | Mode |
|---|---|---|
| Daemon (Sinfonia) | None — the daemon does not call GitHub. The *agent inside the session* calls GitHub via `shell` + `gh`, using whatever credential `gh` is configured with on the host. | n/a |
| Bridge | Verify webhooks (HMAC, shared secret); apply PR labels; post failure comments to PRs; (App mode) mint per-installation access tokens. | PAT (`repo` + `read:org`) OR App (`Contents: read`, `Issues: write`, `Pull requests: write`, `Metadata: read`, `Checks: read`). |

### Agent-backend credentials

| Backend | Where the key lives |
|---|---|
| `openai` / `anthropic` / `google` | Env var read by the daemon process at startup (`OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `GOOGLE_API_KEY`), OR explicit `agent.api_key` in `WORKFLOW.md`. |
| `ollama` | None — local model server, no key. |
| `claude_code` / `codex` / `opencode` | The CLI itself owns auth. Run `claude` / `codex` / `opencode auth login` once on the host so the credential file lands in `~/.claude/` / `~/.codex/` / `~/.opencode/`. The daemon does NOT pass an `api_key`. |

### HMAC shared secrets

Two HMAC secrets, both shared between processes, NEVER between
environments:

- `github.webhook_secret` — bridge ↔ GitHub. Set in `BRIDGE.md` AND in
  the GitHub webhook configuration.
- `telemetry.sinfonia_events_secret` — daemon ↔ bridge for the typed
  event channel. Set in `WORKFLOW.md` AND in `BRIDGE.md`.

Generate both with `openssl rand -hex 32`. Rotate independently. There
is no key-rollover mechanism in v0.3 — to rotate, set the new value on
both sides and restart both processes within the same window. Webhooks
delivered during the swap will get HTTP 401 and GitHub will retry
(GitHub's standard 8-attempt backoff handles this).

---

## Observability

Set the standard OpenTelemetry endpoint env var to wire structured trace
emission to a Collector / Honeycomb / Datadog / etc.:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export SINFONIA_TENANT_ID=acme-web-app
sinfonia /etc/sinfonia/WORKFLOW.md
sinfonia-bridge /etc/sinfonia/BRIDGE.md
```

Or set the same values inside the `telemetry:` block of `WORKFLOW.md` /
`BRIDGE.md`. When neither is set, the OTel layer is disabled and both
binaries run stdout-only — identical behaviour to v0.3.0-alpha.1.

What you get:

- **Daemon spans:** `orchestrator.tick`, `orchestrator.dispatch`,
  `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`.
- **Bridge spans:** `bridge.webhook`, `bridge.ci_result`,
  `bridge.state_transition`, `bridge.cap_hit`, `bridge.cost_update`,
  `bridge.events_receive`.
- Every span carries the resolved `tenant_id`. Resource-level
  `service.namespace = tenant_id` lets a Collector routing-processor
  split per-tenant data without touching emission code.

A reference Collector + Postgres deployment ships under
[`examples/telemetry/`](../examples/telemetry/) — the Postgres schema,
the Collector config, and three dashboard SQL queries (tenant monthly
cost, first-try rate, top-budget tickets). The `setup-telemetry` skill
generates the same wiring from a guided REPL.

For the typed Sinfonia↔bridge event channel that drives budget
enforcement, see [`docs/SPEC.md` §11.6.11](SPEC.md). The budget
enforcement surface itself is §11.6.12.

---

## Scaling

**One Sinfonia daemon = one tracker project.** Run multiple daemons for
multiple projects (each with its own port, workspace root, and
`telemetry.tenant_id`). The orchestrator's mutable state is NOT shared
across daemons — that's deliberate; cross-project coordination would
require distributed locking that isn't worth it for a polling loop.

**One bridge = one tracker project.** Same reasoning. The bridge's
SQLite is single-writer.

A team running 10 projects would typically have:

- 10 Sinfonia daemons (one per project, each on a different port).
- 10 BRIDGE.md files. Whether you run 10 bridge processes or co-locate
  them on one host is a packaging choice — each `--config` flag points
  at a different `BRIDGE.md` and binds a different port.
- One OTel Collector (and one Postgres) aggregating telemetry from all
  20 processes, with `service.namespace = <project>` doing the
  per-project segmentation.

**Per-daemon concurrency** is controlled by `agent.max_concurrent_agents`
in `WORKFLOW.md` and per-state caps via
`agent.max_concurrent_agents_by_state`. Tune these based on:

- Your tracker's API rate limits (Linear: 1500 req/h on a personal API
  key; Jira Cloud: varies by plan).
- Your LLM provider's rate limits (Anthropic, OpenAI, Google publish
  per-org and per-key limits separately).
- Host CPU / RAM / disk if running CLI subprocess agents — each
  `claude_code` / `codex` / `opencode` subprocess is several hundred MB
  of Node / Bun / Go process plus the agent's actual workload.

---

## Backup and recovery

**Workspaces** are rebuildable. The `after_create` hook bootstraps them
from a clean clone; the `before_run` hook brings them current. If you
lose a workspace, the next tick re-runs `after_create` and re-bootstraps.
Backup is optional and primarily useful when an in-flight `.sinfonia/`
notebook (a state-machine working scratchpad) is non-trivial to recreate.

**Bridge SQLite** (`storage.state_db_path`, default
`~/.sinfonia/bridge.db`) contains the `processed_deliveries` idempotency
log + the `pr_ticket_map`. Losing it means:

- GitHub may retry a webhook delivery the bridge previously processed,
  causing one duplicate state transition / comment. The transitions are
  idempotent (transitioning to the same state is a no-op).
- The PR↔ticket map gets recomputed on next webhook from the PR body
  regex match. Recomputation is silent and stateless.

So: bridge SQLite backup is **nice to have, not required for
correctness**. A daily `cp` to remote storage is sufficient. Restore is
file copy.

**Tracker** is the source of truth for everything that matters. The
`sinfonia_bridge_state_v1` marker comment (Linear) and the `sinfonia_*`
custom fields (Jira) ARE the durable state.

---

## Merge queue & branch protection

For Sinfonia agent PRs to land cleanly under the GitHub native merge queue
integration model, the target repo requires specific branch protection and CI
configuration. No Sinfonia code changes are needed — this is configuration
in the target repo and GitHub settings.

### Branch protection rule

Enable the following settings on the `main` branch:

| Setting | Value |
|---------|-------|
| Require a pull request before merging | Enabled |
| Require status checks to pass | Enabled — include the harness gate check name (§7.2) |
| Require merge queue | Enabled |

> **Linear history:** do not also enable "Require linear history" — GitHub's UI
> treats it as mutually exclusive with a merge queue. Linear history is already
> produced by the merge queue's "Rebase and merge" method (see
> [Merge queue settings](#merge-queue-settings) below); no separate toggle is needed.

### Merge queue settings

| Setting | Value |
|---------|-------|
| Merge method | Rebase and merge |
| Required status checks to pass before merging | All required checks (rebase-and-test) |

This ensures every PR is rebased against the latest `main` and tested before
merging. A PR that was green when submitted is validated against concurrent work
before it lands.

### Post-merge harness gate

Add a CI workflow triggered on `push` to `main` that runs the harness gate
(the same check that runs on PRs). When the gate fails after a merge, operators
should receive an alert — a green-at-PR-time change that breaks once integrated
is caught before the next agent dispatch sees a broken base.

The post-merge gate's role is described normatively in
[`docs/HARNESS-SPEC.md` §7.4](HARNESS-SPEC.md); this section is the
operator-actionable counterpart.

### Serial-foundation convention

Foundational or cross-cutting stories in a milestone run serially (one lands on
`main` before the next begins). This is enforced at the dispatch layer by
`agent.max_concurrent_agents_by_state: "In Progress": 1` in `WORKFLOW.md`.
The convention and its rationale are defined in
[`docs/HARNESS-SPEC.md` §7.4](HARNESS-SPEC.md).

---

## Upgrading

Sinfonia follows [SemVer](https://semver.org/). Within a minor (e.g.
0.3.0 → 0.3.1), upgrades are drop-in: stop the old process, start the
new one. Across minors (e.g. 0.2.x → 0.3.0), see
[`docs/MIGRATION-v0.2-to-v0.3.md`](MIGRATION-v0.2-to-v0.3.md).

The bridge does NOT hot-reload `BRIDGE.md` in v0.3 — config changes
require a process restart. The daemon DOES hot-reload `WORKFLOW.md` via
the filesystem watcher; invalid changes are rejected and the previous
config keeps serving traffic.

---

## Where to go next

- [`docs/CLIENT_SETUP.md`](CLIENT_SETUP.md) — enterprise adoption
  perspective: security review, audit trail, handoff to an operating
  team.
- [`docs/SKILLS.md`](SKILLS.md) — the six setup skills that scaffold a
  working deployment without hand-editing YAML.
- [`docs/SPEC.md`](SPEC.md) §11 — the underlying spec for the tracker
  and bridge contract.
- [`examples/telemetry/`](../examples/telemetry/) — reference Collector +
  Postgres + dashboard SQL.
