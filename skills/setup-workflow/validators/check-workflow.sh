#!/usr/bin/env bash
# Wraps `sinfonia --check WORKFLOW.md` and translates the exit code into a
# remediation hint. Shared by `setup-workflow` and `migrate-from-symphony`.
#
# Usage: ./check-workflow.sh [PATH_TO_WORKFLOW]
# Default path: ./WORKFLOW.md
set -uo pipefail

WORKFLOW="${1:-./WORKFLOW.md}"

if ! command -v sinfonia >/dev/null 2>&1; then
    echo "error: 'sinfonia' binary not in PATH (build with: cargo build --release -p sinfonia)" >&2
    exit 127
fi

sinfonia "$WORKFLOW" --check
rc=$?

case $rc in
    0) echo "ok: $WORKFLOW validates" ;;
    2) echo "hint: YAML parse error — check the front-matter block for syntax errors" >&2 ;;
    3) echo "hint: schema validation failed — a required field is missing or malformed" >&2 ;;
    4) echo "hint: template compile error — a Liquid reference in the prompt body is invalid (often a missing | default: filter)" >&2 ;;
    5) echo "hint: tracker auth missing — set the env var referenced by tracker.api_key (e.g. LINEAR_API_KEY)" >&2 ;;
    *) echo "hint: unexpected exit code $rc — see error above" >&2 ;;
esac

exit $rc
