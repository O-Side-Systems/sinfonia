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
#   2 — scan error (e.g. gh failure); CI must fail loud, not pass

set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== overlap linter: scanning open sinfonia/* PRs ==="
# Capture scan-overlap.sh's exit code explicitly so we distinguish:
#   1 — overlap detected (resolution: add a 'blocks' relation)
#   2 — scan error (e.g. gh failure; CR-01). Must surface as a hard CI
#       failure, NOT be reinterpreted as "overlap detected".
set +e
scripts/scan-overlap.sh
SCAN_RC=$?
set -e

if [ "$SCAN_RC" -eq 1 ]; then
  echo "FAIL: PR overlap detected — see scan output above" >&2
  echo "Resolution: add a 'blocks' relation so foundation lands before leaves." >&2
  exit 1
elif [ "$SCAN_RC" -ne 0 ]; then
  echo "FAIL: overlap scan errored (exit ${SCAN_RC}) — see output above; gate cannot verify" >&2
  exit "$SCAN_RC"
fi
echo "ok: no PR overlap detected"
