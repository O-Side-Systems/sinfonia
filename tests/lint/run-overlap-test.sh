#!/usr/bin/env bash
# tests/lint/run-overlap-test.sh — CTXGRAPH-01 acceptance test.
#
# Asserts that scripts/scan-overlap.sh:
#   1. Detects overlap in the seeded two-PR fixture (both touch crates/sinfonia-bridge/src/)
#      and logs the OVERLAP token in stdout.
#   2. Exits non-zero when overlap is detected (the reuse-or-block signal).
#   3. Exits 0 and does NOT log OVERLAP when given the no-overlap fixture.
#
# Runs fully offline via GH_OVERLAP_FIXTURE injection — no live gh call required.
#
# Usage:
#   tests/lint/run-overlap-test.sh
#   bash tests/lint/run-overlap-test.sh

set -euo pipefail
cd "$(dirname "$0")/../.."

OVERLAP_FIXTURE="tests/lint/overlap-fixture.json"
NO_OVERLAP_FIXTURE="tests/lint/no-overlap-fixture.json"
SCAN="scripts/scan-overlap.sh"

echo "=== CTXGRAPH-01 seeded overlap test ==="

# Pre-flight: confirm fixtures and scan script exist
for f in "$OVERLAP_FIXTURE" "$NO_OVERLAP_FIXTURE" "$SCAN"; do
  if [ ! -f "$f" ]; then
    echo "FAIL: required file not found: $f" >&2
    exit 1
  fi
done

# -----------------------------------------------------------------------
# Assertion 1: overlap fixture → stdout contains OVERLAP token + both PR numbers
# -----------------------------------------------------------------------
echo "--- [1/3] overlap fixture: expect OVERLAP in stdout and PR 101, 102 ---"

OUTPUT=$(GH_OVERLAP_FIXTURE="$OVERLAP_FIXTURE" "$SCAN" 2>&1 || true)
echo "$OUTPUT"

if ! echo "$OUTPUT" | grep -q "OVERLAP"; then
  echo "FAIL: scan-overlap.sh did not log an OVERLAP line for the seeded fixture" >&2
  exit 1
fi

if ! echo "$OUTPUT" | grep -q "101"; then
  echo "FAIL: scan-overlap.sh output did not reference PR #101" >&2
  exit 1
fi

if ! echo "$OUTPUT" | grep -q "102"; then
  echo "FAIL: scan-overlap.sh output did not reference PR #102" >&2
  exit 1
fi

# -----------------------------------------------------------------------
# Assertion 2: overlap fixture → exit non-zero (reuse-or-block signal)
# -----------------------------------------------------------------------
echo "--- [2/3] overlap fixture: expect non-zero exit code ---"

set +e
GH_OVERLAP_FIXTURE="$OVERLAP_FIXTURE" "$SCAN" >/dev/null 2>&1
RC=$?
set -e

if [ "$RC" -eq 0 ]; then
  echo "FAIL: scan-overlap.sh should exit non-zero when overlap detected (got exit 0)" >&2
  exit 1
fi

echo "  exit code: $RC (non-zero, as expected)"

# -----------------------------------------------------------------------
# Assertion 3: no-overlap fixture → exit 0 and no OVERLAP in stdout
# -----------------------------------------------------------------------
echo "--- [3/3] no-overlap fixture: expect exit 0 and no OVERLAP in stdout ---"

set +e
NO_OVERLAP_OUTPUT=$(GH_OVERLAP_FIXTURE="$NO_OVERLAP_FIXTURE" "$SCAN" 2>&1)
NO_OVERLAP_RC=$?
set -e

echo "$NO_OVERLAP_OUTPUT"

if [ "$NO_OVERLAP_RC" -ne 0 ]; then
  echo "FAIL: scan-overlap.sh should exit 0 when no overlap detected (got exit $NO_OVERLAP_RC)" >&2
  exit 1
fi

if echo "$NO_OVERLAP_OUTPUT" | grep -q "OVERLAP"; then
  echo "FAIL: scan-overlap.sh logged OVERLAP for the no-overlap fixture (false positive)" >&2
  exit 1
fi

# -----------------------------------------------------------------------
# All assertions passed
# -----------------------------------------------------------------------
echo ""
echo "PASS: CTXGRAPH-01 — sibling-scan logged overlap and returned exit 1 (reuse-or-block signal)"
echo "PASS: no false positive on the negative fixture (exit 0, no OVERLAP token)"
