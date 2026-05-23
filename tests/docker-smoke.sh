#!/usr/bin/env bash
# tests/docker-smoke.sh — image smoke test invoked after `bake --push`.
#
# Verifies every published image can at least show its `--help` text and
# exits 0. Runs the daemon's `--check` against
# `tests/fixtures/WORKFLOW.smoke.md` (a self-contained, env-var-free
# minimal config) to exercise the YAML + Liquid parsing path end-to-end.
# Runs `sinfonia-bridge --self-test` with `tests/fixtures/BRIDGE.smoke.md`
# — the binary returns a fatal error without real creds, which we suppress
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

# --- daemon --check accepts WORKFLOW.smoke.md ------------------------------
# Use the self-contained smoke fixture (no `$LINEAR_API_KEY` indirection)
# rather than the docs-facing WORKFLOW.example.md so the smoke harness
# stays env-var-free.
echo "  -- sinfonia --check WORKFLOW.smoke.md"
docker run --rm \
    -v "${REPO_ROOT}/tests/fixtures/WORKFLOW.smoke.md:/etc/sinfonia/WORKFLOW.md:ro" \
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
