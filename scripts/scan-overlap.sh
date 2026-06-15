#!/usr/bin/env bash
# scripts/scan-overlap.sh — shared, deterministic, fixture-injectable overlap scan.
#
# Enumerates open sinfonia/* PRs, maps their changed files to owned modules via
# the root AGENTS.md module-ownership table, and detects cross-PR overlap.
#
# This script is the single deterministic code path serving:
#   - Agent pre-build sibling scan (CTXGRAPH-01): its stdout IS the sibling-scan log.
#   - CI overlap linter (plan 06-03 / CTXGRAPH-04): called by lint-pr-overlap.sh.
#
# Exit codes:
#   0 — no overlap (or fewer than 2 sinfonia/* PRs)
#   1 — overlap detected (two or more PRs touch the same owned module)
#   2 — error (unreadable fixture, missing tool, etc.)
#
# Injectable fixture for deterministic offline testing:
#   GH_OVERLAP_FIXTURE=tests/lint/overlap-fixture.json scripts/scan-overlap.sh
#
# Live usage (requires gh auth):
#   scripts/scan-overlap.sh
#
# Security notes:
#   - All PR data (branch names, file paths) is passed through jq + bash string
#     comparison only. Never eval/exec any PR field — attacker-controlled data
#     from fork PRs flows through here.
#   - GH_OVERLAP_FIXTURE is read-only; on unreadable path: error to stderr, exit 2.

set -euo pipefail
cd "$(dirname "$0")/.."

# ---------------------------------------------------------------------------
# Step 1: Load PR data (fixture or live gh call)
# ---------------------------------------------------------------------------
if [ -n "${GH_OVERLAP_FIXTURE:-}" ]; then
  # Security: validate the fixture path is readable before cat
  if [ ! -r "${GH_OVERLAP_FIXTURE}" ]; then
    echo "ERROR: GH_OVERLAP_FIXTURE is set but file is unreadable: ${GH_OVERLAP_FIXTURE}" >&2
    exit 2
  fi
  PR_JSON=$(cat "${GH_OVERLAP_FIXTURE}")
else
  PR_JSON=$(gh pr list --state open --limit 200 \
    --json number,headRefName,files 2>/dev/null || echo "[]")
fi

# ---------------------------------------------------------------------------
# Step 2: Filter to sinfonia/* PRs using jq client-side prefix filter.
# NOTE: gh pr list --search 'head:sinfonia/' does NOT work as a prefix filter
# (exact match only, returns empty). Always use startswith("sinfonia/") via jq.
# ---------------------------------------------------------------------------
SINFONIA_PRS=$(echo "$PR_JSON" \
  | jq '[.[] | select(.headRefName | startswith("sinfonia/"))]')

PR_COUNT=$(echo "$SINFONIA_PRS" | jq 'length')

echo "=== sibling sinfonia/* PRs: $PR_COUNT open ==="
echo "$SINFONIA_PRS" | jq -r '.[] | "  PR #\(.number) \(.headRefName)"'

if [ "$PR_COUNT" -lt 2 ]; then
  echo "=== no overlap possible (fewer than 2 sinfonia/* PRs) ==="
  exit 0
fi

# ---------------------------------------------------------------------------
# Step 3: Build owned-module map from root AGENTS.md ## Module Ownership table.
# awk range: from "## Module Ownership" until "## See also" (or end of file).
# Emits: <path-glob> <module-capability> (space-separated; module may contain spaces)
# ---------------------------------------------------------------------------
build_module_map() {
  awk '/## Module Ownership/,/## See also/' AGENTS.md \
    | grep '|' | grep -v 'Capability\|---' \
    | awk -F'|' '{
        capability = $2
        path = $3
        # Strip backticks and leading/trailing whitespace from path
        gsub(/`/, "", path)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", path)
        # Strip leading/trailing whitespace from capability
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", capability)
        if (path != "" && capability != "") print path " " capability
      }'
}

MODULE_MAP=$(build_module_map)

# ---------------------------------------------------------------------------
# Step 4 + 5: Map each PR's changed files to owned modules; record module:pr
# pairs in a temp file. Use temp file instead of bash arrays to avoid macOS
# bash 3.2 subshell-scope issues (Pitfall 6: array+=() in a while-read
# subshell loses accumulated state on exit in bash 3.2).
# ---------------------------------------------------------------------------
OVERLAP_TMP=$(mktemp -t scan-overlap.XXXXXX)
trap 'rm -f "${OVERLAP_TMP}"' EXIT

# Iterate PRs and files using jq to extract, then write module:pr lines to tmp file
# Use process substitution + read to avoid subshell variable loss
while IFS= read -r pr_json_line; do
  PR_NUM=$(echo "$pr_json_line" | jq -r '.number')
  # jq -r '.files[].path' emits one path per line; handle empty gracefully
  while IFS= read -r fpath; do
    [ -z "$fpath" ] && continue
    # Security: fpath is passed to bash string comparison only, never eval/exec
    # Find matching module via prefix match against MODULE_MAP
    MATCHED_MODULE=""
    while IFS= read -r map_line; do
      # Extract path_prefix as first space-delimited token; rest is module name
      path_prefix="${map_line%% *}"
      module_name="${map_line#* }"
      if [[ "$fpath" == "$path_prefix"* ]]; then
        MATCHED_MODULE="$module_name"
        break
      fi
    done <<< "$MODULE_MAP"
    [ -z "$MATCHED_MODULE" ] && continue
    # Write "module_name TAB pr_num" to temp file
    printf '%s\t%s\n' "$MATCHED_MODULE" "$PR_NUM" >> "${OVERLAP_TMP}"
  done < <(echo "$pr_json_line" | jq -r '.files[].path // empty')
done < <(echo "$SINFONIA_PRS" | jq -c '.[]')

# ---------------------------------------------------------------------------
# Step 6: Detect overlap — a module recorded against two DISTINCT PR numbers.
# Sort the temp file and use awk to find modules with >1 distinct PR.
# ---------------------------------------------------------------------------
OVERLAP_FOUND=0

if [ -s "${OVERLAP_TMP}" ]; then
  # Sort by module name (field 1), then pr (field 2); deduplicate
  sort -u "${OVERLAP_TMP}" | \
    awk -F'\t' '
      {
        module = $1
        pr     = $2
        if (module in seen_pr && seen_pr[module] != pr) {
          print "OVERLAP: PR #" pr " and PR #" seen_pr[module] " both modify '"'"'" module "'"'"'"
          overlap = 1
        } else {
          seen_pr[module] = pr
        }
      }
      END { exit (overlap ? 1 : 0) }
    ' && OVERLAP_FOUND=0 || OVERLAP_FOUND=$?
fi

if [ "$OVERLAP_FOUND" -ne 0 ]; then
  echo "=== OVERLAP DETECTED — add 'blocks' relation and stop ==="
  exit 1
fi
echo "=== no overlap detected ==="
exit 0
