#!/usr/bin/env bash
# scripts/lint-stale-nodes.sh
#
# Path-aware stale-node linter for AGENTS.md doc-graph nodes.
#
# Walks every real AGENTS.md node in the repo, reads last_verified_sha from
# front-matter, computes an effective git baseline (handles pre-merge feature-
# branch SHAs via git merge-base), counts commits on main that touch each
# node's OWNED paths since that baseline, and applies a tunable grace window:
#   within window  → warn (exit 0)
#   beyond window  → hard-fail (exit non-zero) locally and in CI
#
# Environment variables:
#   STALE_COMMIT_THRESHOLD  — number of owned-path commits before hard-fail
#                             (default: 5)
#   STALE_IGNORE_NODES      — comma-separated node paths to skip entirely
#                             (safety valve for known-high-churn nodes such as
#                             the root AGENTS.md; paths relative to repo root,
#                             e.g. "AGENTS.md,crates/sinfonia/AGENTS.md")
#
# Exit code: 0 if all nodes pass (with optional warnings); 1 on any hard-fail.

set -euo pipefail

cd "$(dirname "$0")/.."

# --- Tool preflight -----------------------------------------------------------
if ! command -v git >/dev/null 2>&1; then
  echo "ERROR: required tool not found on PATH: git" >&2
  exit 1
fi

# --- Configuration -----------------------------------------------------------
STALE_COMMIT_THRESHOLD="${STALE_COMMIT_THRESHOLD:-5}"
STALE_IGNORE_NODES="${STALE_IGNORE_NODES:-}"

ERRORS=0
WARNINGS=0

# --- Resolve the comparison ref once (CR-02) ---------------------------------
# A bare `main` only resolves via refs/heads/main, which does NOT exist under
# actions/checkout@v4 on a pull_request event (detached HEAD; only
# refs/remotes/origin/main is present). Resolve robustly, preferring a local
# main but falling back to the remote-tracking ref. A resolution failure must
# fail loud — never silently report "fresh" (WR-01/WR-02).
MAIN_REF=$(git rev-parse --verify --quiet refs/heads/main \
        || git rev-parse --verify --quiet refs/remotes/origin/main \
        || git rev-parse --verify --quiet origin/main) \
  || { echo "ERROR: cannot resolve 'main' ref (no refs/heads/main or origin/main)" >&2; exit 1; }

# --- Helpers -----------------------------------------------------------------

parse_frontmatter_field() {
  local file="$1" field="$2"
  grep -m1 "^${field}:" "$file" | sed "s/^${field}:[[:space:]]*//" | tr -d '"'
}

# Returns true (0) if the node path matches an entry in STALE_IGNORE_NODES.
is_ignored_node() {
  local node="$1"
  # Normalise: strip leading ./ for comparison
  local norm="${node#./}"
  if [ -z "$STALE_IGNORE_NODES" ]; then
    return 1
  fi
  local IFS_ORIG="$IFS"
  IFS=","
  for entry in $STALE_IGNORE_NODES; do
    # WR-05: strip ALL leading/trailing whitespace (not just one space), so
    # entries like "AGENTS.md,  crates/x/AGENTS.md" (multiple spaces after the
    # comma) still match instead of silently failing the safety valve.
    entry="${entry#"${entry%%[![:space:]]*}"}"
    entry="${entry%"${entry##*[![:space:]]}"}"
    if [ "$norm" = "$entry" ] || [ "$node" = "$entry" ]; then
      IFS="$IFS_ORIG"
      return 0
    fi
  done
  IFS="$IFS_ORIG"
  return 1
}

# --- Main walk ---------------------------------------------------------------

while IFS= read -r node; do

  # --- Skip conditions (Pitfall 4: template/draft/deprecated nodes) ----------
  STATUS=$(parse_frontmatter_field "$node" "status")
  if [[ "$STATUS" == "draft" || "$STATUS" == "deprecated" ]]; then
    continue
  fi
  # Skip nodes under templates/ directory
  if [[ "$node" == "./templates/"* || "$node" == "templates/"* ]]; then
    continue
  fi
  # Skip nodes listed in STALE_IGNORE_NODES (safety valve)
  if is_ignored_node "$node"; then
    echo "SKIP:  $node (in STALE_IGNORE_NODES)" >&2
    continue
  fi

  # --- Parse last_verified_sha -----------------------------------------------
  LAST_SHA=$(parse_frontmatter_field "$node" "last_verified_sha")

  # Validate SHA format — security: prevent git argument injection (T-06-04)
  if ! [[ "$LAST_SHA" =~ ^[0-9a-f]{7,40}$ ]]; then
    echo "ERROR: $node: invalid last_verified_sha format '${LAST_SHA}'" >&2
    ERRORS=$((ERRORS + 1))
    continue
  fi

  # Validate SHA exists in the repo (Pitfall 2: do NOT use --is-ancestor)
  if ! git cat-file -e "${LAST_SHA}" 2>/dev/null; then
    echo "ERROR: $node: last_verified_sha '${LAST_SHA}' not found in repo" >&2
    ERRORS=$((ERRORS + 1))
    continue
  fi

  # --- Compute effective baseline (handles pre-merge branch SHAs) -----------
  # git merge-base returns the branching point when SHA is not yet on main.
  # This avoids counting all of main's history as "new" for in-flight PRs.
  #
  # WR-02: if LAST_SHA shares NO common ancestor with main (force-pushed /
  # rebased-away branch, unrelated graft), merge-base fails. Do NOT silently
  # fall back to main HEAD (which would make the node read as "always fresh").
  # Instead warn and baseline from the root commit so all owned commits count.
  if ! EFFECTIVE=$(git merge-base "${LAST_SHA}" "${MAIN_REF}" 2>/dev/null); then
    echo "WARN:  $node — last_verified_sha ${LAST_SHA} has no common ancestor with main; treating as stale baseline" >&2
    WARNINGS=$((WARNINGS + 1))
    EFFECTIVE=$(git rev-list --max-parents=0 "${MAIN_REF}" | tail -1)
  fi

  # --- Extract owned path-globs from the node's Module Ownership table --------
  # Falls back to the node's own directory when no ownership table is found.
  # WR-03: trim only SURROUNDING whitespace per line (not all spaces) so a
  # legitimate path containing a space is not silently corrupted into tokens.
  PATHS=$(awk '/## Module Ownership/,/## See also/' "$node" \
    | grep '|' | grep -v 'Capability\|---' \
    | awk -F'|' '{print $3}' \
    | sed 's/`//g; s/^[[:space:]]*//; s/[[:space:]]*$//' | grep -v '^$' || true)

  if [ -z "$PATHS" ]; then
    # Fallback: use the node's own directory as the owned path
    PATHS=$(dirname "$node")
    PATHS="${PATHS#./}"
  fi

  # --- Check each owned path-glob for stale commits on main ------------------
  # WR-03: iterate via `while read` with no unquoted word-splitting and
  # globbing disabled so untrusted front-matter path-globs containing shell
  # metacharacters (*, ?, [) are NOT expanded against the working directory.
  # WR-01: drop the `2>/dev/null || echo 0` masking — a genuine git/ref error
  # must fail loud (counted as an ERROR), not be reinterpreted as "fresh".
  set -f  # disable pathname expansion for the duration of the loop
  while IFS= read -r path_glob; do
    [ -z "$path_glob" ] && continue
    if ! COUNT=$(git rev-list --count "${EFFECTIVE}..${MAIN_REF}" -- "$path_glob"); then
      echo "ERROR: $node — git rev-list failed for $path_glob (baseline ${EFFECTIVE}..${MAIN_REF})" >&2
      ERRORS=$((ERRORS + 1))
      continue
    fi
    if [ "$COUNT" -gt "$STALE_COMMIT_THRESHOLD" ]; then
      echo "ERROR: $node STALE — $COUNT commits on main touch $path_glob since ${LAST_SHA} (threshold: ${STALE_COMMIT_THRESHOLD})" >&2
      ERRORS=$((ERRORS + 1))
    elif [ "$COUNT" -gt 0 ]; then
      echo "WARN:  $node — $COUNT commits on main touch $path_glob since ${LAST_SHA} (within grace window of ${STALE_COMMIT_THRESHOLD})" >&2
      WARNINGS=$((WARNINGS + 1))
    fi
  done <<< "$PATHS"
  set +f

done < <(find . -name "AGENTS.md" -not -path "./.planning/*" | sort)

# --- Final result ------------------------------------------------------------
if [ "$ERRORS" -gt 0 ]; then
  echo "FAIL: $ERRORS stale node(s) exceed grace window — restamp before merging" >&2
  exit 1
fi

echo "ok: stale-node check passed ($WARNINGS warning(s))"
