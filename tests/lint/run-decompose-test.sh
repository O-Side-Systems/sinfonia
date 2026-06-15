#!/usr/bin/env bash
# tests/lint/run-decompose-test.sh — CTXGRAPH-05 acceptance test.
#
# Runs scripts/decompose-consistency-check.sh against the seeded stories fixture
# and asserts:
#   (1) OVERLAP-REPORT.md is created with all three required section headers.
#   (2) The Overlap Findings section names both LIN-10 and LIN-11.
#   (3) blocks-graph.json is valid JSON with schema_version == 1 and a blocks
#       entry with from=LIN-10, to=LIN-11, shared_module containing the bridge path.
#   (4) The emitter has no gh/curl calls (files-only, offline constraint).
#
# Exit codes: 0 = PASS, 1 = FAIL (prints PASS:/FAIL: lines)

set -euo pipefail
cd "$(dirname "$0")/../.."

FIXTURE="tests/lint/stories-fixture.json"
EMITTER="scripts/decompose-consistency-check.sh"
TESTS_PASS=0
TESTS_FAIL=0

fail() {
  echo "FAIL: $*" >&2
  TESTS_FAIL=$((TESTS_FAIL + 1))
}

pass() {
  echo "PASS: $*"
  TESTS_PASS=$((TESTS_PASS + 1))
}

echo "=== CTXGRAPH-05 decomposition consistency test ==="

# Preflight: check required tools
for tool in jq bash; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "ERROR: required tool not found: $tool" >&2
    exit 1
  fi
done

# Preflight: fixture and emitter must exist
if [ ! -f "$FIXTURE" ]; then
  echo "ERROR: stories fixture not found: $FIXTURE" >&2
  exit 1
fi

if [ ! -x "$EMITTER" ]; then
  echo "ERROR: emitter not found or not executable: $EMITTER" >&2
  exit 1
fi

# Create temp output dir; clean up on exit
OUTDIR=$(mktemp -d -t decompose-test.XXXXXX)
trap 'rm -rf "${OUTDIR}"' EXIT

OVERLAP_REPORT="${OUTDIR}/OVERLAP-REPORT.md"
BLOCKS_GRAPH="${OUTDIR}/blocks-graph.json"

# Run emitter
OVERLAP_REPORT="$OVERLAP_REPORT" \
BLOCKS_GRAPH="$BLOCKS_GRAPH" \
MILESTONE="test-milestone" \
  bash "$EMITTER" "$FIXTURE"

# ---- Assertion (1): OVERLAP-REPORT.md exists with all three section headers ----
if [ ! -f "$OVERLAP_REPORT" ]; then
  fail "OVERLAP-REPORT.md was not created at: $OVERLAP_REPORT"
else
  MISSING_HEADERS=0
  for header in "## Overlap Findings" "## Contradiction Findings" "## Proposed blocks ordering"; do
    if ! grep -qF "$header" "$OVERLAP_REPORT"; then
      fail "OVERLAP-REPORT.md missing required section: '$header'"
      MISSING_HEADERS=$((MISSING_HEADERS + 1))
    fi
  done
  if [ "$MISSING_HEADERS" -eq 0 ]; then
    pass "OVERLAP-REPORT.md contains all three required section headers"
  fi
fi

# ---- Assertion (2): Overlap Findings names both LIN-10 and LIN-11 ----
if [ -f "$OVERLAP_REPORT" ]; then
  if grep -qF "LIN-10" "$OVERLAP_REPORT" && grep -qF "LIN-11" "$OVERLAP_REPORT"; then
    pass "Overlap Findings section names both LIN-10 and LIN-11"
  else
    fail "Overlap Findings section does not reference both LIN-10 and LIN-11 in $OVERLAP_REPORT"
  fi
fi

# ---- Assertion (3): blocks-graph.json is valid JSON with schema_version 1 and a LIN-10→LIN-11 blocks edge ----
if [ ! -f "$BLOCKS_GRAPH" ]; then
  fail "blocks-graph.json was not created at: $BLOCKS_GRAPH"
else
  # Validate schema_version == 1
  if ! jq -e '.schema_version == 1' "$BLOCKS_GRAPH" >/dev/null 2>&1; then
    fail "blocks-graph.json does not have schema_version == 1"
  else
    pass "blocks-graph.json has schema_version == 1"
  fi

  # Validate at least one blocks entry with from=LIN-10, to=LIN-11
  EDGE_COUNT=$(jq -e '[.blocks[] | select(.from == "LIN-10" and .to == "LIN-11")] | length' "$BLOCKS_GRAPH" 2>/dev/null || echo 0)
  if [ "$EDGE_COUNT" -ge 1 ]; then
    pass "blocks-graph.json contains a LIN-10 -> LIN-11 blocks edge (foundation before leaf)"
  else
    fail "blocks-graph.json missing expected blocks edge from LIN-10 to LIN-11"
  fi

  # Validate shared_module references the bridge module
  BRIDGE_EDGE=$(jq -e '[.blocks[] | select(.from == "LIN-10" and .to == "LIN-11" and (.shared_module | contains("sinfonia-bridge")))] | length' "$BLOCKS_GRAPH" 2>/dev/null || echo 0)
  if [ "$BRIDGE_EDGE" -ge 1 ]; then
    pass "blocks-graph.json LIN-10->LIN-11 edge correctly names the bridge shared_module"
  else
    fail "blocks-graph.json LIN-10->LIN-11 edge is missing shared_module with 'sinfonia-bridge'"
  fi

  # Validate at least one blocks entry total
  TOTAL_BLOCKS=$(jq -e '.blocks | length' "$BLOCKS_GRAPH" 2>/dev/null || echo 0)
  if [ "$TOTAL_BLOCKS" -ge 1 ]; then
    pass "blocks-graph.json has at least one blocks entry (total: $TOTAL_BLOCKS)"
  else
    fail "blocks-graph.json has no blocks entries"
  fi
fi

# ---- Assertion (4): emitter makes no gh/curl calls (files-only, offline constraint) ----
if grep -nE '\bgh \b|\bcurl\b|git push' "$EMITTER" >/dev/null 2>&1; then
  fail "emitter $EMITTER contains forbidden tracker-mutation commands (gh/curl/git push)"
else
  pass "emitter $EMITTER has no gh/curl/git push calls (files-only constraint satisfied)"
fi

# ---- Final result ----
echo ""
echo "=== Results: $TESTS_PASS passed, $TESTS_FAIL failed ==="

if [ "$TESTS_FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
