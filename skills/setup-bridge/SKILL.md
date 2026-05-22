---
name: setup-bridge
description: Set up sinfonia-bridge for a project that already has a working WORKFLOW.md. Configures GitHub auth (PAT or App), provisions tracker custom fields, picks a deployment topology (sibling daemon / standalone host / Actions-only), renders BRIDGE.md, and runs `sinfonia-bridge --self-test`.
version: 1.0.0
---

# setup-bridge

Wire `sinfonia-bridge` to the project. Assumes `setup-workflow` has already
produced a `WORKFLOW.md`.

## When to use

- A working `WORKFLOW.md` exists, and the operator wants Sinfonia to react
  to GitHub CI results (PR opened, check_suite completed, workflow_run
  completed).

If no `WORKFLOW.md` exists yet, run `setup-workflow` first.

## Procedure

### 1. GitHub auth mode

Ask the operator:

> Do you need the bridge to act across multiple repos, or do you need
> fine-grained permissions (read:org, etc.)?

- **No** (single repo, no special scopes): use **PAT mode**. Simpler, fewer
  moving parts.
- **Yes**: use **App mode**. The GitHub App's bot account becomes the bridge
  identity.

### 2a. PAT mode

1. Direct the operator to https://github.com/settings/tokens to create a
   classic PAT with `repo` (and optionally `read:org`) scope.
2. Have them set `GITHUB_TOKEN` in their shell.
3. Validate: `gh api user` should succeed and return their handle.

### 2b. App mode

1. Walk the operator through GitHub App manifest creation. Write a manifest
   JSON to `./bridge-app-manifest.json` with the bridge's required
   permissions (pull_requests: write, issues: write, checks: read, actions:
   read, contents: read).
2. Direct them to https://github.com/settings/apps/new with the manifest
   URL.
3. After creation, have them install the App on the target repo(s).
4. Collect: App ID, Client ID, private key path, installation ID. Surface
   these as `BRIDGE_APP_ID`, `BRIDGE_APP_INSTALLATION_ID`,
   `BRIDGE_APP_PRIVATE_KEY_PATH` env vars.
5. Wait for the operator to confirm the App is installed before proceeding.

### 3. Tracker confirmation

Read `WORKFLOW.md` and surface the tracker config. Ask:

> The bridge will write to this tracker. Same credentials as the daemon —
> already in your env. Proceed?

If the operator wants the bridge to use *different* credentials (rare —
typically a separate service account for the bridge writes), prompt for
the override and write it as `tracker.api_key: $BRIDGE_LINEAR_API_KEY`
(or similar) in `BRIDGE.md`.

### 4. Custom field provisioning

Call `IssueTracker::ensure_custom_field` for each well-known bridge field.

- **Linear**: no-op. Linear custom fields live in a marker comment
  (`sinfonia_bridge_state_v1`); no schema changes needed.
- **Jira**: real REST API calls to `/rest/api/3/field`. The bridge will
  resolve display names (e.g. `Sinfonia Attempt Count`) to
  `customfield_NNNNN` IDs and create absent fields. This requires admin
  permissions on the Jira project.

If admin perms are missing on Jira, point the operator at
`docs/JIRA-SCREEN-SCHEME.md` for the manual pre-create flow.

### 5. Deployment topology

Ask the operator to pick one:

- **Sibling daemon** — bridge runs on the same host as Sinfonia. Webhook
  served at `http://sinfonia-host:<bridge-port>/webhook`. Use when the
  operator has a single VM or container host.
- **Standalone host** — bridge runs on its own host with a public webhook
  URL. Use when GitHub needs to reach the bridge over the public internet
  and the Sinfonia host is firewalled.
- **Actions-only** — no public bridge. CI posts to the bridge via a
  GitHub Actions workflow. Use when the operator cannot expose a public
  HTTP endpoint.

### 6. Render

Render `templates/bridge.md.liquid` with the answers. Additional renders
depending on topology:

- Sibling daemon: render `templates/docker-compose-sibling.yml.liquid`
  wiring Sinfonia + bridge as services in the same compose file.
- Actions-only: render `templates/gh-actions-ci-report.yml.liquid` into
  `.github/workflows/sinfonia-ci-report.yml`.
- Standalone host: no extra files — the operator is responsible for the
  reverse proxy / hosting choice.

### 7. Validate

Run `validators/self-test.sh` (which wraps `sinfonia-bridge --self-test
BRIDGE.md`). Every check must return `PASS`. Format:

```
PASS  config: BRIDGE.md parsed
PASS  github: authenticated as octocat (PAT mode)
PASS  github: webhook endpoint reachable at https://...
PASS  tracker: linear project 'my-awesome-project-abc...' accessible
PASS  custom fields: sinfonia_bridge_state_v1 comment marker reserved
```

If any check is `FAIL`, surface the line to the operator and do not
proceed.

### 8. Commit

`git add BRIDGE.md docker-compose.yml .github/workflows/sinfonia-ci-report.yml`
(omit the ones that weren't rendered). Commit.

### 9. Webhook secret rotation

If a webhook secret was newly minted in step 6, remind the operator to set
the secret in the GitHub webhook config (Settings → Webhooks → Edit) to
match `webhook.secret` in `BRIDGE.md`.

## Templates

- `templates/bridge.md.liquid` — the BRIDGE.md skeleton.
- `templates/docker-compose-sibling.yml.liquid` — Compose wiring for the
  sibling-daemon topology.
- `templates/gh-actions-ci-report.yml.liquid` — the Actions-only workflow.

## Validators

- `validators/self-test.sh` — wraps `sinfonia-bridge --self-test BRIDGE.md`
  with PASS/FAIL output.

## See also

- `setup-state-machine` — adds the Needs-Fixes states that consume the
  bridge's `sinfonia_last_ci_failure` field.
- `setup-telemetry` — instruments both binaries with OpenTelemetry once
  the bridge is live.
