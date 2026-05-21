#!/usr/bin/env bash
#
# verify-workspace-move.sh — confirms the v0.3 workspace-conversion commit
# preserved every test outcome from the prior commit.
#
# Per `docs/v0.3-plan/01-bridge-mvp.md` §2, the workspace move is supposed to
# be a *path-only* change — files relocate, logic is byte-for-byte preserved.
# That's a process claim. This script makes it a verifiable artifact: it
# checks out the prior commit, captures the workspace test summary, checks
# out the move commit, captures the same summary, and exits non-zero if the
# two summaries differ.
#
# Usage:
#   scripts/verify-workspace-move.sh
#
# Optional environment:
#   VERIFY_BEFORE=<git ref>   defaults to HEAD~1
#   VERIFY_AFTER=<git ref>    defaults to HEAD
#
# Requirements:
#   - Working tree must be clean (the script does `git checkout`).
#   - Cargo must be on PATH.

set -euo pipefail

before="${VERIFY_BEFORE:-HEAD~1}"
after="${VERIFY_AFTER:-HEAD}"

# Pre-flight: refuse to run on a dirty working tree — we use git checkout.
if [[ -n "$(git status --porcelain)" ]]; then
    echo "error: working tree has uncommitted changes; refusing to run" >&2
    git status --short >&2
    exit 1
fi

original_ref="$(git symbolic-ref --quiet --short HEAD || git rev-parse HEAD)"
tmpdir="$(mktemp -d -t sinfonia-workspace-verify.XXXXXX)"
trap 'git checkout --quiet "${original_ref}" 2>/dev/null || true; rm -rf "${tmpdir}"' EXIT

run_tests () {
    local ref="$1"
    local out="$2"
    git -c advice.detachedHead=false checkout --quiet "${ref}"
    # `cargo test --workspace` exits nonzero if a test fails; we capture the
    # full output and post-process. `--no-fail-fast` so one failing crate
    # doesn't hide others.
    cargo test --workspace --no-fail-fast 2>&1 \
        | grep -E '^test result:' \
        | sort > "${out}"
}

before_out="${tmpdir}/before.txt"
after_out="${tmpdir}/after.txt"

echo "Running workspace tests at ${before} (before workspace move)…"
run_tests "${before}" "${before_out}"

echo "Running workspace tests at ${after}  (after workspace move)…"
run_tests "${after}" "${after_out}"

echo
echo "Before summary:"
sed 's/^/    /' "${before_out}"
echo "After summary:"
sed 's/^/    /' "${after_out}"
echo

# Diff allows the AFTER side to be a strict superset of BEFORE (the workspace
# split surfaces previously-private unit tests in the tracker crate as a new
# binary, which is gain, not regression). We require that every BEFORE line
# is still present in AFTER.
missing_lines="$(comm -23 "${before_out}" "${after_out}" || true)"
if [[ -n "${missing_lines}" ]]; then
    echo "FAIL: the workspace move dropped test outcomes that were present in ${before}:" >&2
    echo "${missing_lines}" | sed 's/^/    /' >&2
    exit 1
fi

# Refuse to bless a move that turned passing tests into failures.
if grep -Eq 'FAILED|failed' "${after_out}"; then
    echo "FAIL: workspace tests fail at ${after}:" >&2
    sed 's/^/    /' "${after_out}" >&2
    exit 1
fi

echo "OK: workspace move at ${after} preserves every test outcome from ${before}."
