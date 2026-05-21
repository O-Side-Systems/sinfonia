# Phase 6 — Docker images

**Target:** v0.3.0
**Scope:** Six published Docker images covering the supported deployment topologies, plus a refactored `docker-compose.yml` that demonstrates the bridge + telemetry pieces.
**Estimated size:** ~200 LOC of Dockerfile + ~150 LOC of GitHub Actions + ~200 LOC of docs.
**Depends on:** Phase 1 (workspace + bridge binary), Phase 2 (OpenCode is a real backend choice), Phase 3 (telemetry pieces appear in the compose file).
**Unblocks:** Phase 7's deployment doc.

---

## 1. Status quo

Today's `Dockerfile` (read at planning time) is a **dev-shell** image: Node 22 + Rust toolchain + Claude Code CLI + `gh`, intended for the `--dangerously-skip-permissions` workflow under a bind mount. It's useful for local development, but it's not how we'd deploy Sinfonia for a client.

Today's `docker-compose.yml` is two services off the same image: a long-running `daemon` and an interactive `shell`. Both build from the local repo via `cargo build --release` at container start.

This is a fine dev experience and we keep it (`Dockerfile.dev`). Production images are new.

---

## 2. The image matrix

Six images, all published to GitHub Container Registry as `ghcr.io/o-side-systems/<name>`. Tagged with both the semver (`:0.3.0`) and `:latest`.

| Image | Contents | Audience | Size goal |
|---|---|---|---|
| `sinfonia` | The `sinfonia` binary, `bash`, `git`, `curl`, `gh` CLI | Anyone running just the daemon | < 100 MB |
| `sinfonia-bridge` | The `sinfonia-bridge` binary only | Bridge-as-standalone-host deployments | < 50 MB |
| `sinfonia-with-claude-code` | `sinfonia` + Node 22 + `@anthropic-ai/claude-code` | Users on Claude Code | < 600 MB |
| `sinfonia-with-codex` | `sinfonia` + `codex` CLI | Users on Codex | < 400 MB |
| `sinfonia-with-opencode` | `sinfonia` + `opencode` binary | Users on OpenCode | < 200 MB |
| `sinfonia-all-agents` | `sinfonia` + all three CLI agents | State-machine deployments that route across agents | < 800 MB |

Size goals are targets, not hard contracts — we measure at build time and document the actual sizes in the release notes.

### Why three "with one agent" images plus an "all-agents" image

The proposal answers this explicitly: a user who only uses Claude Code shouldn't pull a 2 GB image with `codex` and `opencode` they'll never run. A user with a state machine that routes across all three needs them all in one place because each ticket's runner is determined at dispatch time.

---

## 3. Dockerfile design

Single multi-stage `Dockerfile` at the repo root (the existing dev-shell Dockerfile moves to `Dockerfile.dev`). Each image is a separate target.

```dockerfile
# syntax=docker/dockerfile:1.7
# Production image set for Sinfonia v0.3+.
# Build a specific target with:
#   docker build --target sinfonia -t ghcr.io/o-side-systems/sinfonia:0.3.0 .
# Build all targets with:
#   docker buildx bake --file docker-bake.hcl

# --- Build stage --------------------------------------------------------
FROM rust:1.78-bookworm AS builder
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --bin sinfonia --bin sinfonia-bridge && \
    cp target/release/sinfonia /usr/local/bin/sinfonia && \
    cp target/release/sinfonia-bridge /usr/local/bin/sinfonia-bridge

# --- Common runtime base ------------------------------------------------
FROM debian:bookworm-slim AS sinfonia-base
RUN apt-get update && apt-get install -y --no-install-recommends \
    bash git curl ca-certificates \
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
       | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
       > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y --no-install-recommends gh \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/sinfonia /usr/local/bin/
COPY --from=builder /usr/local/bin/sinfonia-bridge /usr/local/bin/

# --- Image: sinfonia (daemon only) --------------------------------------
FROM sinfonia-base AS sinfonia
WORKDIR /var/lib/sinfonia
ENTRYPOINT ["/usr/local/bin/sinfonia"]
CMD ["/etc/sinfonia/WORKFLOW.md", "--port", "8080"]
EXPOSE 8080
VOLUME ["/var/lib/sinfonia/workspaces", "/etc/sinfonia"]

# --- Image: sinfonia-bridge ---------------------------------------------
FROM debian:bookworm-slim AS sinfonia-bridge
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/sinfonia-bridge /usr/local/bin/
WORKDIR /var/lib/sinfonia
ENTRYPOINT ["/usr/local/bin/sinfonia-bridge"]
CMD ["/etc/sinfonia/BRIDGE.md", "--port", "8081"]
EXPOSE 8081
VOLUME ["/var/lib/sinfonia/bridge", "/etc/sinfonia"]

# --- Image: sinfonia + Claude Code --------------------------------------
FROM sinfonia AS sinfonia-with-claude-code
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    npm install -g @anthropic-ai/claude-code && \
    rm -rf /var/lib/apt/lists/*

# --- Image: sinfonia + Codex --------------------------------------------
FROM sinfonia AS sinfonia-with-codex
RUN curl -fsSL https://github.com/openai/codex/releases/latest/download/install.sh \
    | bash

# --- Image: sinfonia + OpenCode -----------------------------------------
FROM sinfonia AS sinfonia-with-opencode
RUN curl -fsSL https://opencode.ai/install | bash

# --- Image: sinfonia + all agents ---------------------------------------
FROM sinfonia-with-claude-code AS sinfonia-all-agents
RUN curl -fsSL https://github.com/openai/codex/releases/latest/download/install.sh | bash && \
    curl -fsSL https://opencode.ai/install | bash
```

**Caveats verified at implementation time:**

- The OpenCode and Codex install scripts may have changed by release time; pin to a specific version where possible.
- `cargo build --release` with cache mounts works under BuildKit (which is default with Docker Desktop and recent CI runners); document that explicitly.
- The `gh` CLI install in the base layer is heavyweight (~30 MB); if size becomes a problem, drop it and document that hooks that need `gh` should install it themselves in `after_create`.

### 3.1 Why each image is its own target, not its own Dockerfile

A single multi-stage Dockerfile + `docker buildx bake` is the convention for image families. Reviewers see all six images in one place, and the `builder` stage is shared so we don't rebuild the binary six times.

The downside is a longer Dockerfile. Mitigated by the comments above each target.

### 3.2 `docker-bake.hcl`

```hcl
variable "VERSION" { default = "dev" }
variable "REGISTRY" { default = "ghcr.io/o-side-systems" }

group "default" {
  targets = ["sinfonia", "sinfonia-bridge", "sinfonia-with-claude-code",
             "sinfonia-with-codex", "sinfonia-with-opencode", "sinfonia-all-agents"]
}

target "sinfonia" {
  dockerfile = "Dockerfile"
  target     = "sinfonia"
  tags       = ["${REGISTRY}/sinfonia:${VERSION}", "${REGISTRY}/sinfonia:latest"]
  platforms  = ["linux/amd64", "linux/arm64"]
}

target "sinfonia-bridge" {
  dockerfile = "Dockerfile"
  target     = "sinfonia-bridge"
  tags       = ["${REGISTRY}/sinfonia-bridge:${VERSION}", "${REGISTRY}/sinfonia-bridge:latest"]
  platforms  = ["linux/amd64", "linux/arm64"]
}
# ... and similarly for the other four targets ...
```

`linux/amd64` is mandatory. `linux/arm64` is best-effort — important for M-series Macs and Graviton hosts, but if a particular agent CLI lacks an arm64 binary, that image is amd64-only and we note it in the release.

---

## 4. The new `docker-compose.yml`

Renamed: the current dev-focused compose file moves to `docker-compose.dev.yml`. The new `docker-compose.yml` demonstrates a production-style topology.

```yaml
# docker-compose.yml — sample production topology for Sinfonia v0.3+.
# Adjust for your environment; this is the shape the `setup-bridge` and
# `setup-telemetry` skills generate.

services:
  sinfonia:
    image: ghcr.io/o-side-systems/sinfonia-all-agents:0.3.0
    restart: unless-stopped
    volumes:
      - ./WORKFLOW.md:/etc/sinfonia/WORKFLOW.md:ro
      - sinfonia-workspaces:/var/lib/sinfonia/workspaces
      - ${HOME}/.claude:/root/.claude:ro
      - ${HOME}/.codex:/root/.codex:ro
      - ${HOME}/.opencode:/root/.opencode:ro
    environment:
      - LINEAR_API_KEY=${LINEAR_API_KEY}
      - OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317
      - SINFONIA_TENANT_ID=${TENANT_ID:-default}
    ports:
      - "8080:8080"
    depends_on:
      - otel-collector

  sinfonia-bridge:
    image: ghcr.io/o-side-systems/sinfonia-bridge:0.3.0
    restart: unless-stopped
    volumes:
      - ./BRIDGE.md:/etc/sinfonia/BRIDGE.md:ro
      - bridge-state:/var/lib/sinfonia/bridge
    environment:
      - LINEAR_API_KEY=${LINEAR_API_KEY}
      - GH_WEBHOOK_SECRET=${GH_WEBHOOK_SECRET}
      - GH_APP_ID=${GH_APP_ID:-}
      - GH_APP_KEY=${GH_APP_KEY:-}
      - GH_PAT=${GH_PAT:-}
      - OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4317
      - SINFONIA_TENANT_ID=${TENANT_ID:-default}
    ports:
      - "8081:8081"
    depends_on:
      - otel-collector

  otel-collector:
    image: otel/opentelemetry-collector-contrib:0.103.0
    volumes:
      - ./examples/telemetry/otel-collector-config.yaml:/etc/otelcol/config.yaml:ro
    command: ["--config", "/etc/otelcol/config.yaml"]
    ports:
      - "4317:4317"   # OTLP gRPC
      - "4318:4318"   # OTLP HTTP

  postgres:
    image: postgres:16-alpine
    restart: unless-stopped
    environment:
      - POSTGRES_DB=sinfonia_telemetry
      - POSTGRES_USER=otel
      - POSTGRES_PASSWORD=${POSTGRES_PASSWORD}
    volumes:
      - sinfonia-telemetry-pg:/var/lib/postgresql/data
      - ./examples/telemetry/postgres-schema.sql:/docker-entrypoint-initdb.d/01-schema.sql:ro

volumes:
  sinfonia-workspaces:
  bridge-state:
  sinfonia-telemetry-pg:
```

Notes:

- `LINEAR_API_KEY`, `GH_WEBHOOK_SECRET`, etc. are read from a `.env` file or from the shell. The skill-generated version of this compose file references the literal env vars; the skill prompts the user to populate `.env` separately.
- Per-agent credential directories are bind-mounted read-only. This is the auth model the proposal documents: the `claude` / `codex` / `opencode` CLIs handle their own auth, and Sinfonia just inherits whatever's in those directories.
- Postgres ships with the telemetry schema pre-applied via `docker-entrypoint-initdb.d`.
- The Collector config is one of the files Phase 3 produces (`examples/telemetry/otel-collector-config.yaml`).

### 4.1 What changes about the existing dev workflow

Today's `docker-compose.yml` (the dev-shell flavor) becomes `docker-compose.dev.yml`. The README documentation gains a clear "Production vs development compose files" subsection explaining which to use when.

The behavior of `docker compose up -d daemon` from the dev file is unchanged — it still builds locally from source and runs the binary.

---

## 5. Publishing pipeline

GitHub Actions workflow at `.github/workflows/docker-publish.yml`:

```yaml
name: Publish Docker images

on:
  push:
    tags:
      - "v*"
  workflow_dispatch:

permissions:
  contents: read
  packages: write

jobs:
  build-and-push:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up QEMU
        uses: docker/setup-qemu-action@v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Log in to GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract version from tag
        id: version
        run: echo "value=${GITHUB_REF#refs/tags/v}" >> "$GITHUB_OUTPUT"

      - name: Build and push (all targets)
        env:
          VERSION: ${{ steps.version.outputs.value }}
        run: docker buildx bake --push
```

`docker buildx bake --push` reads `docker-bake.hcl` and pushes all six images with both `:0.3.0` and `:latest` tags.

### 5.1 Reproducible builds

Pin the Rust base image (`rust:1.78-bookworm`) and the runtime base image (`debian:bookworm-slim`) by digest at release time so re-builds produce byte-identical layers. The CI workflow includes a `docker buildx imagetools inspect` step on each pushed tag to print the digests into the workflow log for audit.

### 5.2 Image scanning

Add a Trivy step:

```yaml
      - name: Scan images for vulnerabilities
        uses: aquasecurity/trivy-action@master
        with:
          image-ref: ghcr.io/o-side-systems/sinfonia:${{ steps.version.outputs.value }}
          format: sarif
          output: trivy-results.sarif
          severity: CRITICAL,HIGH
          exit-code: 1
```

Fail the publish on CRITICAL/HIGH CVEs in the base or in our binary. We accept LOW/MEDIUM — chasing every Debian curl CVE is not a good use of release time.

---

## 6. Authentication inside containers

Documented in §4 already, restated here for emphasis because it's the most common stumble:

- CLI agents (`claude`, `codex`, `opencode`) auth through credential files in `~/.claude/`, `~/.codex/`, `~/.opencode/`.
- For Docker, mount these as **read-only** volumes (`:ro`). The container needs to read your creds; it never needs to write them.
- The `setup-bridge` and `setup-telemetry` skills generate compose snippets that include these mounts.

For headless deployments where there's no human to run `claude /login`, the recommended path is:

1. Run the auth flow once on a workstation.
2. Copy the credential files to a secrets store (Kubernetes Secret, AWS Secrets Manager, etc.).
3. Mount them into the production container at the expected path.

This is documented in `docs/CLIENT_SETUP.md` (Phase 7).

---

## 7. Test plan

### 7.1 Build tests

CI invokes `docker buildx bake --no-cache` against each PR. All six targets must build. Build times are recorded.

### 7.2 Smoke tests

`tests/docker-smoke.sh` (a shell script, not Rust):

```bash
#!/usr/bin/env bash
set -euo pipefail

# Each image should at least run --help and exit 0.
docker run --rm ghcr.io/o-side-systems/sinfonia:latest --help
docker run --rm ghcr.io/o-side-systems/sinfonia-bridge:latest --help
docker run --rm ghcr.io/o-side-systems/sinfonia-with-claude-code:latest --help
docker run --rm ghcr.io/o-side-systems/sinfonia-with-codex:latest --help
docker run --rm ghcr.io/o-side-systems/sinfonia-with-opencode:latest --help
docker run --rm ghcr.io/o-side-systems/sinfonia-all-agents:latest --help

# Sinfonia --check should accept a known-good WORKFLOW.md.
docker run --rm -v "${PWD}/WORKFLOW.example.md:/etc/sinfonia/WORKFLOW.md:ro" \
    ghcr.io/o-side-systems/sinfonia:latest --check /etc/sinfonia/WORKFLOW.md

# Bridge --self-test should run without --self-test crashing (it'll FAIL most
# checks because we're not giving it real creds, but the binary itself works).
docker run --rm ghcr.io/o-side-systems/sinfonia-bridge:latest --self-test || true
```

Runs after `bake --push` in the CI pipeline.

### 7.3 Compose stack smoke test

The production `docker-compose.yml` bind-mounts `${HOME}/.claude`, `${HOME}/.codex`, `${HOME}/.opencode` so the in-container agents inherit user auth. In CI those directories don't exist; the mounts would fail or surface empty dirs and the smoke test would pass for the wrong reason.

Solution: ship a `docker-compose.ci.yml` overlay that disables the agent-credential mounts and points Sinfonia at a no-CLI-backend test config:

```yaml
# docker-compose.ci.yml — CI overlay; not for production use.
services:
  sinfonia:
    # Use the daemon-only image; we don't need claude/codex/opencode CLIs in CI.
    image: ghcr.io/o-side-systems/sinfonia:${VERSION:-latest}
    volumes:
      # Override the production user-cred mounts to bind nothing.
      - !reset []
      - ./tests/fixtures/WORKFLOW.smoke.md:/etc/sinfonia/WORKFLOW.md:ro
      - sinfonia-workspaces:/var/lib/sinfonia/workspaces
```

`tests/docker-compose-smoke.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Bring the full stack up with the CI overlay. The smoke test asserts each
# binary boots and exposes its HTTP surface; it does NOT exercise real
# agent backends.
LINEAR_API_KEY=test GH_WEBHOOK_SECRET=test POSTGRES_PASSWORD=test \
    docker compose \
      -f docker-compose.yml \
      -f docker-compose.ci.yml \
      up -d

# Wait for each service's healthz endpoint with a bounded retry, not a sleep.
for url in http://localhost:8080/api/v1/state http://localhost:8081/health; do
    for i in $(seq 1 30); do
        if curl --fail --silent --max-time 2 "$url" >/dev/null 2>&1; then
            break
        fi
        sleep 1
        [ "$i" -eq 30 ] && { echo "FAIL: $url never came up" >&2; exit 1; }
    done
done

docker compose -f docker-compose.yml -f docker-compose.ci.yml down -v
echo "OK: smoke test passed."
```

`tests/fixtures/WORKFLOW.smoke.md` is a minimal workflow that exercises Sinfonia's HTTP surface without dispatching any agent — it uses a stub tracker config that returns no candidate issues.

A real production deployment requires real credentials and a webhook URL reachable from GitHub — out of scope for CI smoke tests. The skill-driven manual verification in `docs/v0.3-plan/06-docker-VERIFY.md` covers that.

### 7.4 Manual verification

Per proposal exit criteria:

> `docker compose up` brings up Sinfonia + bridge + OTel Collector + Postgres in one command from a generated `docker-compose.yml`.

Steps:

1. From a fresh checkout, run `skills/setup-bridge` (Phase 5).
2. Set the required env vars in `.env`.
3. `docker compose up -d`.
4. Verify all four services are running.
5. Trigger a real ticket cycle.
6. Confirm telemetry shows up in Postgres via the queries from `03-telemetry-budget.md` §8.2.

Captured in `docs/v0.3-plan/06-docker-VERIFY.md`.

---

## 8. Open questions

1. **Multi-arch availability.** Codex and OpenCode may not publish arm64 binaries. Decision: build amd64 always, build arm64 where the agent has arm64 support, label clearly in the release notes when an image is amd64-only.

2. **Image versioning.** `:0.3.0` and `:latest` is the proposal text. Add `:0.3` (a moving "latest 0.3.x") as well? Default: yes, also tag `:0.3` and `:0.3.0`. Three tags per image is fine.

3. **`docker-bake.hcl` vs `--bake` flag vs separate `Dockerfile.<image>`.** Bake is the lowest-friction multi-image path and we use it. If reviewers prefer separate Dockerfiles for clarity, we can split — but the upside is small and the duplication is real.

4. **Base image hardening.** `debian:bookworm-slim` is the easy default. Distroless (`gcr.io/distroless/cc-debian12`) would be smaller and have fewer CVEs, but it lacks bash, git, curl, gh — which we need for hooks. Sticking with Debian slim, but it's worth a v0.4+ revisit.

5. **GHCR vs Docker Hub.** GHCR-first per the proposal. We can mirror to Docker Hub via a CI step if there's demand; for v0.3, GHCR-only.

---

## 9. Phase 6 deliverable checklist

- [ ] Refactored `Dockerfile` with all six build targets.
- [ ] `docker-bake.hcl` defining the targets, registry, and platforms.
- [ ] Existing `Dockerfile` renamed to `Dockerfile.dev`.
- [ ] Existing `docker-compose.yml` renamed to `docker-compose.dev.yml`.
- [ ] New `docker-compose.yml` showing the production topology.
- [ ] `.github/workflows/docker-publish.yml` building + pushing all six on tag.
- [ ] Trivy scan in the publish workflow with `severity: CRITICAL,HIGH` fail policy.
- [ ] `tests/docker-smoke.sh` invoked after publish.
- [ ] `tests/docker-compose-smoke.sh` invoked in CI with `docker-compose.ci.yml` overlay (handles absent user-cred mounts).
- [ ] `tests/fixtures/WORKFLOW.smoke.md` stub for the compose smoke test.
- [ ] `docs/DEPLOYMENT.md` (Phase 7) referencing the image matrix.
- [ ] `README.md` "Docker" section updated.
- [ ] Manual verification recorded in `docs/v0.3-plan/06-docker-VERIFY.md`.
- [ ] CHANGELOG entry.

Phase 6 ships independently of Phase 7 documentation polish — the images are usable from the release notes alone.
