# syntax=docker/dockerfile:1.7
# Production image set for Sinfonia v0.3+.
#
# This Dockerfile defines six build targets — see docker-bake.hcl for the
# full set. Build a single target with:
#
#   docker build --target sinfonia -t ghcr.io/o-side-systems/sinfonia:0.3.0 .
#
# Build all six at once (recommended for releases) with:
#
#   docker buildx bake
#
# The previous dev-shell Dockerfile (Node + Rust + Claude Code, intended
# for the `--dangerously-skip-permissions` bind-mount workflow) lives at
# `Dockerfile.dev` and is driven by `docker-compose.dev.yml`. That file
# is unchanged; this one is new.

# ============================================================================
# Build stage — shared across all production images. Compiles both binaries
# once with cargo's registry + target dirs mounted as BuildKit caches.
# ============================================================================
FROM rust:1.78-bookworm AS builder
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --bin sinfonia --bin sinfonia-bridge && \
    cp target/release/sinfonia        /usr/local/bin/sinfonia && \
    cp target/release/sinfonia-bridge /usr/local/bin/sinfonia-bridge

# ============================================================================
# sinfonia-base — Debian slim + the tools the daemon's hooks typically reach
# for (bash, git, curl, gh). Used as the parent for every `sinfonia-*` image
# except the bridge-only one.
# ============================================================================
FROM debian:bookworm-slim AS sinfonia-base
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        bash git curl ca-certificates gnupg \
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
         | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
         > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y --no-install-recommends gh \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/sinfonia        /usr/local/bin/sinfonia
COPY --from=builder /usr/local/bin/sinfonia-bridge /usr/local/bin/sinfonia-bridge

# ============================================================================
# Image: sinfonia — the daemon binary by itself (plus bash / git / curl / gh
# for hook scripts). Audience: operators running a non-CLI agent backend
# (raw Anthropic / OpenAI / Google API, or Ollama).
# ============================================================================
FROM sinfonia-base AS sinfonia
WORKDIR /var/lib/sinfonia
EXPOSE 8080
VOLUME ["/var/lib/sinfonia/workspaces", "/etc/sinfonia"]
ENTRYPOINT ["/usr/local/bin/sinfonia"]
CMD ["/etc/sinfonia/WORKFLOW.md", "--port", "8080"]

# ============================================================================
# Image: sinfonia-bridge — the CI-feedback bridge binary by itself. No
# tracker-CLI / git / gh dependencies; the bridge talks to GitHub + the
# tracker over HTTPS only. Kept deliberately small for sidecar / standalone
# deployments.
# ============================================================================
FROM debian:bookworm-slim AS sinfonia-bridge
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/sinfonia-bridge /usr/local/bin/sinfonia-bridge
WORKDIR /var/lib/sinfonia
EXPOSE 8081
VOLUME ["/var/lib/sinfonia/bridge", "/etc/sinfonia"]
ENTRYPOINT ["/usr/local/bin/sinfonia-bridge"]
CMD ["/etc/sinfonia/BRIDGE.md", "--port", "8081"]

# ============================================================================
# Image: sinfonia-with-claude-code — daemon + Node 22 + `claude` CLI.
# Audience: operators who want the Claude Code subprocess backend.
# Auth is owned by the `claude` CLI; mount your ~/.claude / ~/.claude.json
# into the container at runtime.
# ============================================================================
FROM sinfonia AS sinfonia-with-claude-code
USER root
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    npm install -g @anthropic-ai/claude-code && \
    rm -rf /var/lib/apt/lists/* /root/.npm

# ============================================================================
# Image: sinfonia-with-codex — daemon + `codex` CLI.
# Audience: operators who want the Codex CLI subprocess backend.
# The install script is upstream; pin the asset URL at release time if
# upstream changes the layout.
# ============================================================================
FROM sinfonia AS sinfonia-with-codex
USER root
RUN curl -fsSL https://github.com/openai/codex/releases/latest/download/install.sh \
        -o /tmp/install-codex.sh \
    && bash /tmp/install-codex.sh \
    && rm -f /tmp/install-codex.sh

# ============================================================================
# Image: sinfonia-with-opencode — daemon + `opencode` binary.
# Audience: operators who want the OpenCode subprocess backend (75+
# provider backends incl. Ollama-with-LSP).
# ============================================================================
FROM sinfonia AS sinfonia-with-opencode
USER root
RUN curl -fsSL https://opencode.ai/install \
        -o /tmp/install-opencode.sh \
    && bash /tmp/install-opencode.sh \
    && rm -f /tmp/install-opencode.sh

# ============================================================================
# Image: sinfonia-all-agents — daemon + all three CLI agents in one image.
# Audience: state-machine deployments that route across agents per state.
# Starts from the Claude Code image so we don't re-install Node.
# ============================================================================
FROM sinfonia-with-claude-code AS sinfonia-all-agents
USER root
RUN curl -fsSL https://github.com/openai/codex/releases/latest/download/install.sh \
        -o /tmp/install-codex.sh \
    && bash /tmp/install-codex.sh \
    && rm -f /tmp/install-codex.sh \
    && curl -fsSL https://opencode.ai/install \
        -o /tmp/install-opencode.sh \
    && bash /tmp/install-opencode.sh \
    && rm -f /tmp/install-opencode.sh
