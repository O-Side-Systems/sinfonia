#!/usr/bin/env bash
# tests/docker-compose-smoke.sh — Phase 6 §7.3 compose-stack smoke test.
#
# Brings the production compose stack up with the CI overlay
# (`docker-compose.ci.yml`) so the user-credential bind mounts that don't
# exist in CI are stripped, and the smoke fixtures replace WORKFLOW.md /
# BRIDGE.md. The smoke asserts that each binary's HTTP surface comes up;
# it does NOT exercise any real agent backend or webhook flow.
#
# Required env vars (the production compose references them; smoke values
# are fine — see the docker-publish workflow for what we pass in CI):
#   LINEAR_API_KEY, GH_WEBHOOK_SECRET, POSTGRES_PASSWORD
#
# Optional:
#   VERSION   image tag suffix. Defaults to `latest`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

: "${LINEAR_API_KEY:?missing}"
: "${GH_WEBHOOK_SECRET:?missing}"
: "${POSTGRES_PASSWORD:?missing}"

export VERSION="${VERSION:-latest}"

cleanup() {
    echo "==> tearing down compose stack"
    docker compose -f docker-compose.yml -f docker-compose.ci.yml down -v --remove-orphans \
        || true
}
trap cleanup EXIT

echo "==> bringing up compose stack at VERSION=$VERSION"
docker compose -f docker-compose.yml -f docker-compose.ci.yml up -d

# Bounded retry per URL — 30s of 1-second polls. Long enough to absorb
# Postgres initdb + Sinfonia startup, short enough that a hung container
# fails CI within a minute.
wait_for() {
    local url="$1"
    local i
    for i in $(seq 1 30); do
        if curl --fail --silent --max-time 2 "$url" >/dev/null 2>&1; then
            echo "    OK: $url responding (after ${i}s)"
            return 0
        fi
        sleep 1
    done
    echo "FAIL: $url never came up in 30s" >&2
    echo "==> last logs:" >&2
    docker compose -f docker-compose.yml -f docker-compose.ci.yml logs --tail 50 >&2 || true
    return 1
}

echo "==> waiting for HTTP surfaces"
wait_for http://localhost:8080/api/v1/state
wait_for http://localhost:8081/health

echo "OK: compose smoke test passed."
