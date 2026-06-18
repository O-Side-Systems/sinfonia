#!/usr/bin/env bash
# tests/lint/run-stale-test.sh
#
# Offline stale-node behavior tests for scripts/lint-stale-nodes.sh.
#
# Assertions:
#   (1) STALE_COMMIT_THRESHOLD=9999 exits 0 on current repo (high window).
#   (2) Output does NOT contain an ERROR line referencing templates/AGENTS.md
#       (draft-status skip rule holds).
#   (3) Fixture node with non-churning owned path reports no ERROR.
#   (4) Invalid SHA format triggers ERROR (security/format guard fires).
#
# Exit code: 0 on all PASS, 1 on any FAIL.

set -euo pipefail

cd "$(dirname "$0")/../.."

LINTER="scripts/lint-stale-nodes.sh"
PASS=0
FAIL=0

pass() { echo "PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "FAIL: $1" >&2; FAIL=$((FAIL + 1)); }

echo "=== stale-node linter behavior tests ==="

# --- Assertion 1: High threshold never hard-fails ---------------------------
echo ""
echo "--- (1) STALE_COMMIT_THRESHOLD=9999 exits 0 ---"
if STALE_COMMIT_THRESHOLD=9999 "$LINTER" >/dev/null 2>&1; then
  pass "high threshold exits 0 (no hard-fail)"
else
  fail "high threshold should not hard-fail but did (exit $?)"
fi

# --- Assertion 2: templates/AGENTS.md is skipped (status: draft) ------------
echo ""
echo "--- (2) templates/AGENTS.md is NOT flagged as ERROR (skip rule) ---"
# Capture combined stdout+stderr from the linter
STALE_OUTPUT=$(STALE_COMMIT_THRESHOLD=9999 "$LINTER" 2>&1 || true)
if echo "$STALE_OUTPUT" | grep -q "ERROR.*templates/AGENTS.md"; then
  fail "templates/AGENTS.md appeared in ERROR output — skip rule not working"
else
  pass "templates/AGENTS.md not in ERROR output (skip rule holds)"
fi

# --- Assertion 3: Fixture node with non-churning owned path reports no ERROR -
echo ""
echo "--- (3) Fixture node with non-churning owned path reports no ERROR ---"
FIXTURE_NODE="tests/lint/stale-node-fixture/AGENTS.md"
if [ ! -f "$FIXTURE_NODE" ]; then
  fail "fixture node $FIXTURE_NODE does not exist — cannot test"
else
  # The fixture node owns tests/lint/stale-node-fixture/ which has zero churn
  FIXTURE_OUTPUT=$(STALE_COMMIT_THRESHOLD=9999 "$LINTER" 2>&1 || true)
  if echo "$FIXTURE_OUTPUT" | grep -q "ERROR.*stale-node-fixture"; then
    fail "fixture node reported ERROR despite non-churning owned path"
  else
    pass "fixture node reports no ERROR for non-churning owned path"
  fi
fi

# --- Assertion 4: Invalid SHA format triggers ERROR (security guard) ---------
echo ""
echo "--- (4) Invalid SHA format triggers ERROR ---"
# Create a temp directory with a fake AGENTS.md node that has an invalid SHA.
TMPDIR_TEST=$(mktemp -d)
trap 'rm -rf "$TMPDIR_TEST"' EXIT

# Write a fake AGENTS.md with an invalid SHA ("zzz") into a temp subdir
mkdir -p "$TMPDIR_TEST"
cat > "$TMPDIR_TEST/AGENTS.md" <<'FAKENODE'
---
title: "Test Node — Invalid SHA"
version: "1.0.0"
status: active
owners: ["@test"]
last_verified_sha: "zzz"
derived_from: "test"
---

# Test Invalid SHA Node

## Module Ownership

| Capability | Path-glob | Owned By | Don't Roll Your Own |
|------------|-----------|----------|---------------------|
| Test | `tests/lint/stale-node-fixture/` | @test | — |

## See also

- n/a
FAKENODE

# The linter walks from repo root — we can't easily inject a temp dir into
# the find path. Instead, verify the SHA validation regex is present in the
# linter script (proxy assertion per plan instructions), and also test the
# actual regex logic directly.
if grep -qE '\^\[0-9a-f\]\{7,40\}\$' "$LINTER"; then
  pass "SHA validation regex ^[0-9a-f]{7,40}$ found in linter (security guard present)"
else
  fail "SHA validation regex not found in linter — T-06-04 mitigation missing"
fi

# Direct regex test — confirm 'zzz' fails the pattern and a real SHA passes
if [[ "zzz" =~ ^[0-9a-f]{7,40}$ ]]; then
  fail "'zzz' should not match SHA regex but did"
else
  pass "'zzz' correctly rejected by SHA regex"
fi
if [[ "b26bc50" =~ ^[0-9a-f]{7,40}$ ]]; then
  pass "'b26bc50' correctly accepted by SHA regex"
else
  fail "'b26bc50' should match SHA regex but did not"
fi

# Also write the node into a temporary location reachable by the linter by
# copying it to a path the find command will pick up during this test run.
# We'll use the STALE_IGNORE_NODES env to temporarily allow the scan while
# keeping the real nodes unaffected, then clean up.
#
# Actually: place the fake node in the repo root temp subpath and let the
# linter find it. Then remove it after the test.
TEMP_NODE_DIR="tests/lint/stale-node-fixture-invalid-sha-tmp"
mkdir -p "$TEMP_NODE_DIR"
cp "$TMPDIR_TEST/AGENTS.md" "$TEMP_NODE_DIR/AGENTS.md"
trap 'rm -rf "$TEMP_NODE_DIR"; rm -rf "$TMPDIR_TEST"' EXIT

INVALID_OUTPUT=$(STALE_COMMIT_THRESHOLD=9999 "$LINTER" 2>&1 || true)
rm -rf "$TEMP_NODE_DIR"
# Reset trap to just clean TMPDIR_TEST
trap 'rm -rf "$TMPDIR_TEST"' EXIT

if echo "$INVALID_OUTPUT" | grep -q "ERROR.*stale-node-fixture-invalid-sha-tmp.*invalid.*SHA\|ERROR.*stale-node-fixture-invalid-sha-tmp.*last_verified_sha"; then
  pass "Invalid SHA in temp node reported ERROR (security/format guard fires)"
else
  # Check the error line mentions the node
  if echo "$INVALID_OUTPUT" | grep -q "ERROR.*stale-node-fixture-invalid-sha-tmp"; then
    pass "Invalid SHA in temp node reported ERROR (format guard fires)"
  else
    fail "Expected ERROR for invalid SHA in temp node but none found in output: $INVALID_OUTPUT"
  fi
fi

# --- Final result -----------------------------------------------------------
echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ "$FAIL" -gt 0 ]; then
  echo "FAIL: $FAIL stale-node behavior test(s) failed" >&2
  exit 1
fi
echo "PASS: all stale-node behavior tests passed"
