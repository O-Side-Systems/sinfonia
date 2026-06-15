#!/usr/bin/env bash
# scripts/lint-pr-overlap.sh
#
# CI overlap linter — thin wrapper around scripts/scan-overlap.sh.
#
# Fails loud (exit non-zero) when two open sinfonia/* PRs touch the same
# owned module per the root AGENTS.md module-ownership table. All detection
# logic lives in scan-overlap.sh so the CI gate and the agent pre-build scan
# share one deterministic code path.
#
# Usage:
#   scripts/lint-pr-overlap.sh                         # live (requires gh auth)
#   GH_OVERLAP_FIXTURE=tests/lint/overlap-fixture.json scripts/lint-pr-overlap.sh
#
# Exit codes:
#   0 — no PR overlap detected
#   1 — overlap detected (CI should fail)

set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== overlap linter: scanning open sinfonia/* PRs ==="
if ! scripts/scan-overlap.sh; then
  echo "FAIL: PR overlap detected — see scan output above" >&2
  echo "Resolution: add a 'blocks' relation so foundation lands before leaves." >&2
  exit 1
fi
echo "ok: no PR overlap detected"
