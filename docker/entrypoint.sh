#!/usr/bin/env bash
set -euo pipefail

# Wire git identity from env if provided — sinfonia's spawned agents make commits.
if [[ -n "${GIT_USER_NAME:-}" ]]; then
  git config --global user.name "${GIT_USER_NAME}"
fi
if [[ -n "${GIT_USER_EMAIL:-}" ]]; then
  git config --global user.email "${GIT_USER_EMAIL}"
fi
git config --global init.defaultBranch main
git config --global --add safe.directory /workspace

# If a GitHub token is present, set up gh's git credential helper so child
# processes can `git push` over HTTPS without further prompts.
if [[ -n "${GH_TOKEN:-}${GITHUB_TOKEN:-}" ]]; then
  gh auth setup-git >/dev/null 2>&1 || true
fi

exec "$@"
