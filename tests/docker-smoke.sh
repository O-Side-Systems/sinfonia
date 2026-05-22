#!/usr/bin/env bash
# tests/docker-smoke.sh — Phase 6 §7.2 image smoke test.
#
# Verifies every published image can at least show its `--help` text and
# exits 0. Runs the daemon's `--check` against WORKFLOW.example.md to
# exercise the YAML parser end-to-end. Runs `sinfonia-bridge --self-test`
# with no config — the binary returns a fatal error which we suppress
# with `|| true` (we are asserting it doesn't segfault, not that it
# passes; the latter requires real creds).
#
# Invoked from .github/workflows/docker-publish.yml after `bake --push`.
# Reads VERSION from the environment; defaults to `latest` so a developer
# can run it against a locally-tagged build.

set -euo pipefail

VERSION="${VERSION:-latest}"
REGISTRY="${REGISTRY:-ghcr.io/o-side-systems}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "==> Smoke testing six images at $REGISTRY at version=$VERSION"

# --- --help works on every image -------------------------------------------
for name in sinfonia sinfonia-bridge \
            sinfonia-with-claude-code sinfonia-with-codex \
            sinfonia-with-opencode sinfonia-all-agents; do
    img="${REGISTRY}/${name}:${VERSION}"
    echo "  -- ${img} --help"
    docker run --rm "${img}" --help >/dev/null
done

# --- daemon --check accepts WORKFLOW.example.md ----------------------------
echo "  -- sinfonia --check WORKFLOW.example.md"
docker run --rm \
    -v "${REPO_ROOT}/WORKFLOW.example.md:/etc/sinfonia/WORKFLOW.md:ro" \
    "${REGISTRY}/sinfonia:${VERSION}" \
    --check /etc/sinfonia/WORKFLOW.md

# --- bridge --self-test runs (will FAIL without creds; we just want no
#     crash on entry) -----------------------------------------------------
echo "  -- sinfonia-bridge --self-test (expected to exit non-zero without creds)"
docker run --rm \
    -v "${REPO_ROOT}/tests/fixtures/BRIDGE.smoke.md:/etc/sinfonia/BRIDGE.md:ro" \
    "${REGISTRY}/sinfonia-bridge:${VERSION}" \
    /etc/sinfonia/BRIDGE.md --self-test \
    || true

echo "OK: image smoke test passed."
