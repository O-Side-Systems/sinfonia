#!/usr/bin/env bash
# scripts/decompose-consistency-check.sh — decomposition consistency pass.
#
# Reads a stories JSON input, runs an overlap/contradict pass and an ordering
# pass, and writes two artifacts:
#
#   OVERLAP-REPORT.md  — overlap + contradiction findings + proposed ordering
#   blocks-graph.json  — blocks-relation dependency graph (serial foundation →
#                        parallel fan-out), schema_version: 1
#
# This script writes FILES ONLY.  It MUST NOT call gh, curl, git push, or any
# tracker API.  Applying the blocks graph to the tracker is a separate, explicit
# step (D-08, HARNESS-SPEC §4.1).
#
# Usage:
#   scripts/decompose-consistency-check.sh <stories.json>
#   STORIES_INPUT=<path> scripts/decompose-consistency-check.sh
#
# Optional environment:
#   STORIES_INPUT=<path>          Input stories JSON (overridden by positional $1)
#   OVERLAP_REPORT=<path>         Output path for OVERLAP-REPORT.md  (default: OVERLAP-REPORT.md)
#   BLOCKS_GRAPH=<path>           Output path for blocks-graph.json   (default: blocks-graph.json)
#   MILESTONE=<label>             Milestone label embedded in blocks-graph.json (default: unknown)
#
# Stories JSON schema (minimum):
#   [ { "id": "LIN-10", "description": "...", "modules": ["crates/sinfonia-bridge/src/"], "foundation": true } ]
#
# Security: all story fields flow through jq only — no dynamic execution of story data.
#
# Exit codes:
#   0 — artifacts emitted (findings may include overlaps/contradictions)
#   1 — usage / input error (missing or invalid stories input)
#   2 — missing required tool (jq)

set -euo pipefail
cd "$(dirname "$0")/.."

# ---------------------------------------------------------------------------
# Preflight: jq required
# ---------------------------------------------------------------------------
if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq is required but not found on PATH" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Resolve input path
# ---------------------------------------------------------------------------
if [ $# -ge 1 ]; then
  STORIES_INPUT="$1"
elif [ -z "${STORIES_INPUT:-}" ]; then
  echo "ERROR: stories input path required — pass as \$1 or set STORIES_INPUT" >&2
  echo "Usage: $0 <stories.json>" >&2
  exit 1
fi

if [ ! -r "${STORIES_INPUT}" ]; then
  echo "ERROR: stories input not readable: ${STORIES_INPUT}" >&2
  exit 1
fi

# Validate it's an array
if ! jq -e 'type == "array"' "${STORIES_INPUT}" >/dev/null 2>&1; then
  echo "ERROR: stories input is not a JSON array: ${STORIES_INPUT}" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Resolve output paths and milestone
# ---------------------------------------------------------------------------
OVERLAP_REPORT="${OVERLAP_REPORT:-OVERLAP-REPORT.md}"
BLOCKS_GRAPH="${BLOCKS_GRAPH:-blocks-graph.json}"
MILESTONE="${MILESTONE:-unknown}"
GENERATED_AT=$(date -u +%FT%TZ 2>/dev/null || date -u +"%Y-%m-%dT%H:%M:%SZ")

# ---------------------------------------------------------------------------
# Scratch dir for intermediate work
# ---------------------------------------------------------------------------
SCRATCH=$(mktemp -d -t decompose-check.XXXXXX)
trap 'rm -rf "${SCRATCH}"' EXIT

OVERLAPS_FILE="${SCRATCH}/overlaps.json"
BLOCKS_FILE="${SCRATCH}/blocks.json"
WARNINGS_FILE="${SCRATCH}/warnings.json"

# ---------------------------------------------------------------------------
# Pass 1: Overlap detection
# For every pair of stories sharing at least one module, record (idA, idB, shared_module).
# All field access goes through jq — story data is never eval'd or exec'd.
# ---------------------------------------------------------------------------
jq -r '
  # Emit one line per (story_index_i, story_index_j, shared_module) where i < j
  # and the two stories share at least one module.
  [
    range(length) as $i |
    range(length) as $j |
    select($i < $j) |
    . as $stories |
    ($stories[$i].modules // []) as $mi |
    ($stories[$j].modules // []) as $mj |
    ($mi | map(. as $m | $mj | map(select(. == $m))) | add // []) as $shared |
    select($shared | length > 0) |
    $shared[] |
    {
      idA: $stories[$i].id,
      idB: $stories[$j].id,
      foundationA: ($stories[$i].foundation // false),
      foundationB: ($stories[$j].foundation // false),
      shared_module: .
    }
  ]
' "${STORIES_INPUT}" > "${OVERLAPS_FILE}"

# ---------------------------------------------------------------------------
# Pass 2: Classify each overlap as "overlap" or "contradict"
# Heuristic: a pair is a "contradict" when both stories own the same module
# AND neither is marked foundation: true (two leaves both claim the same
# surface with no designated owner).  Overlaps where exactly one or both are
# foundation are classified as "overlap" (the ordering pass resolves these).
# This is a structural heuristic; true behavioral contradiction requires human
# triage (documented in SUMMARY).
# ---------------------------------------------------------------------------
jq -r '
  map(
    if (.foundationA == false and .foundationB == false)
    then . + {warning_type: "contradict"}
    else . + {warning_type: "overlap"}
    end
  )
' "${OVERLAPS_FILE}" > "${SCRATCH}/classified.json"

# ---------------------------------------------------------------------------
# Pass 3: Ordering pass
# For each overlap where exactly ONE story is foundation:true, emit a blocks
# edge: from=<foundation_id>, to=<leaf_id>.
# If both are foundation (rare), emit no blocks edge (two co-owned foundations
# is an editorial decision, not a sequencing one).
# ---------------------------------------------------------------------------
jq -r '
  map(
    select(.foundationA != .foundationB) |  # exactly one foundation
    if .foundationA == true
    then { from: .idA, to: .idB, reason: ("Foundation story \(.idA) must land before leaf \(.idB) — both own shared module"), shared_module: .shared_module }
    else { from: .idB, to: .idA, reason: ("Foundation story \(.idB) must land before leaf \(.idA) — both own shared module"), shared_module: .shared_module }
    end
  ) | unique_by([.from, .to, .shared_module])
' "${SCRATCH}/classified.json" > "${BLOCKS_FILE}"

# ---------------------------------------------------------------------------
# Build warnings array (for blocks-graph.json)
# ---------------------------------------------------------------------------
jq -r '
  map({
    type: .warning_type,
    issues: [.idA, .idB],
    detail: ("Stories \(.idA) and \(.idB) both claim module \(.shared_module)")
  })
' "${SCRATCH}/classified.json" > "${WARNINGS_FILE}"

# ---------------------------------------------------------------------------
# Emit blocks-graph.json
# ---------------------------------------------------------------------------
jq -n \
  --arg generated_at "${GENERATED_AT}" \
  --arg milestone "${MILESTONE}" \
  --slurpfile blocks "${BLOCKS_FILE}" \
  --slurpfile warnings "${WARNINGS_FILE}" \
  '{
    schema_version: 1,
    generated_at: $generated_at,
    milestone: $milestone,
    blocks: $blocks[0],
    warnings: $warnings[0]
  }' > "${BLOCKS_GRAPH}"

echo "Emitted: ${BLOCKS_GRAPH} ($(jq '.blocks | length' "${BLOCKS_GRAPH}") blocks edge(s), $(jq '.warnings | length' "${BLOCKS_GRAPH}") warning(s))"

# ---------------------------------------------------------------------------
# Emit OVERLAP-REPORT.md
# ---------------------------------------------------------------------------
{
  echo "# Decomposition Consistency Report"
  echo ""
  echo "Generated: ${GENERATED_AT}"
  echo "Milestone: ${MILESTONE}"
  echo "Input: ${STORIES_INPUT}"
  echo ""
  echo "---"
  echo ""

  # ---- § Overlap Findings ----
  echo "## Overlap Findings"
  echo ""
  OVERLAP_COUNT=$(jq '[.[] | select(.warning_type == "overlap")] | length' "${SCRATCH}/classified.json")
  if [ "${OVERLAP_COUNT}" -eq 0 ]; then
    echo "_No overlapping story pairs found._"
  else
    echo "_${OVERLAP_COUNT} pair(s) of stories share owned modules. One story should be designated foundation to establish ordering._"
    echo ""
    jq -r '
      .[] | select(.warning_type == "overlap") |
      "- **\(.idA)** and **\(.idB)** both claim `\(.shared_module)`"
    ' "${SCRATCH}/classified.json"
  fi
  echo ""

  # ---- § Contradiction Findings ----
  echo "## Contradiction Findings"
  echo ""
  CONTRADICT_COUNT=$(jq '[.[] | select(.warning_type == "contradict")] | length' "${SCRATCH}/classified.json")
  if [ "${CONTRADICT_COUNT}" -eq 0 ]; then
    echo "_No contradiction pairs found._"
  else
    echo "_${CONTRADICT_COUNT} pair(s) of leaf stories (neither marked foundation) claim the same owned module. These require human triage to determine ownership and sequencing._"
    echo ""
    jq -r '
      .[] | select(.warning_type == "contradict") |
      "- **\(.idA)** and **\(.idB)** both claim `\(.shared_module)` — neither is designated foundation"
    ' "${SCRATCH}/classified.json"
  fi
  echo ""

  # ---- § Proposed blocks ordering ----
  echo "## Proposed blocks ordering"
  echo ""
  BLOCKS_COUNT=$(jq '.blocks | length' "${BLOCKS_GRAPH}")
  if [ "${BLOCKS_COUNT}" -eq 0 ]; then
    echo "_No ordering constraints derived. All stories may run in parallel (no shared modules with a designated foundation)._"
  else
    echo "_Serial foundation → parallel fan-out derived from the blocks graph:_"
    echo ""
    echo "**Foundation stories (must land first):**"
    echo ""
    jq -r '
      .blocks | map(.from) | unique[] |
      "- \(.)"
    ' "${BLOCKS_GRAPH}"
    echo ""
    echo "**Leaf stories (parallel fan-out, after foundations):**"
    echo ""
    jq -r '
      .blocks | map(.to) | unique[] |
      "- \(.)"
    ' "${BLOCKS_GRAPH}"
    echo ""
    echo "**Blocks edges:**"
    echo ""
    jq -r '
      .blocks[] |
      "- `\(.from)` blocks `\(.to)` — shared module: `\(.shared_module)`"
    ' "${BLOCKS_GRAPH}"
    echo ""
    echo '> **Note:** Apply these edges to the tracker as a separate, explicit step.'
    echo '> The `blocks-graph.json` artifact encodes these relations for that step.'
  fi
  echo ""

} > "${OVERLAP_REPORT}"

echo "Emitted: ${OVERLAP_REPORT}"
