#!/usr/bin/env bash
# Wraps `sinfonia-bridge --self-test BRIDGE.md`. Exit code = number of
# failed checks (matches the `--self-test` contract from P1-G).
#
# Usage: ./self-test.sh [PATH_TO_BRIDGE_CONFIG]
# Default path: ./BRIDGE.md
set -uo pipefail

BRIDGE_CFG="${1:-./BRIDGE.md}"

if ! command -v sinfonia-bridge >/dev/null 2>&1; then
    echo "error: 'sinfonia-bridge' binary not in PATH (build with: cargo build --release -p sinfonia-bridge)" >&2
    exit 127
fi

sinfonia-bridge "$BRIDGE_CFG" --self-test
rc=$?

if [ $rc -eq 0 ]; then
    echo "ok: every self-test check passed"
else
    echo "hint: $rc check(s) failed — review the FAIL lines above and re-run setup-bridge for the affected step" >&2
fi

exit $rc
