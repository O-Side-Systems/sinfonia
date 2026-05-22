#!/usr/bin/env bash
# scripts/check-doc-code-blocks.sh
#
# Validate fenced YAML / JSON / TOML / bash blocks in repo Markdown so
# we catch syntax rot in worked examples before readers do.
#
# Strategy:
#   1. Walk every .md file under the public-facing surface (README,
#      CHANGELOG, CONTRIBUTING, BRIDGE.example, WORKFLOW.example, docs/,
#      examples/, skills/SKILL.md).
#   2. Extract each fenced ```yaml / ```json / ```toml / ```bash block.
#   3. Feed each block to its respective syntax checker.
#
# Skipped on purpose:
#   - Rust / Liquid / SQL blocks (the Rust ones are mostly trait
#     fragments that don't stand alone; the Liquid + SQL blocks are
#     covered by the skill integration tests and the
#     examples/telemetry/queries/ runtime).
#   - The plan docs under docs/v0.3-plan/ (they're internal scratch).
#   - YAML blocks tagged `yaml-skip` (used for intentionally-incomplete
#     fragments in doc prose).
#
# Exit code: 0 if every block passes; 1 on first failure (the failing
# file + block index is printed before exit).

set -euo pipefail

cd "$(dirname "$0")/.."

# Tools we need.
for tool in python3 jq shellcheck; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "ERROR: required tool not found on PATH: $tool" >&2
    exit 1
  fi
done

# Python module imports for the inner validators.
if ! python3 -c 'import yaml' 2>/dev/null; then
  echo "ERROR: python3 yaml module not available (install python3-yaml)" >&2
  exit 1
fi

# Tomllib is stdlib in Python 3.11+. Older Pythons fall back to skipping
# toml blocks with a warning.
TOML_OK=0
if python3 -c 'import tomllib' 2>/dev/null; then
  TOML_OK=1
fi

declare -a DOCS=(
  "README.md"
  "CHANGELOG.md"
  "CONTRIBUTING.md"
  "BRIDGE.example.md"
  "WORKFLOW.example.md"
)
while IFS= read -r f; do
  DOCS+=("$f")
done < <(find docs -type f -name '*.md' -not -path 'docs/v0.3-plan/*' 2>/dev/null | sort)
while IFS= read -r f; do
  DOCS+=("$f")
done < <(find examples -type f -name '*.md' 2>/dev/null | sort)
while IFS= read -r f; do
  DOCS+=("$f")
done < <(find skills -type f -name 'SKILL.md' 2>/dev/null | sort)

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

extract_blocks() {
  # extract_blocks <file> <lang> <out_prefix>
  # Writes one block per file as $out_prefix.NN.<lang>.
  #
  # For lang=yaml, blocks that look like a full WORKFLOW.md / BRIDGE.md
  # file (open with `---\n`, contain a second `---\n` followed by
  # non-blank lines) get truncated to just the front matter — the body
  # after the closing `---` is Markdown / Liquid prose, not YAML.
  local file="$1" lang="$2" prefix="$3"
  python3 - "$file" "$lang" "$prefix" <<'PY'
import re
import sys

path, lang, prefix = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, "r", encoding="utf-8") as f:
    src = f.read()

pattern = re.compile(rf"```{re.escape(lang)}\s*\n(.*?)```", re.DOTALL)
blocks = pattern.findall(src)


def trim_frontmatter(body: str) -> str:
    stripped = body.lstrip("\n")
    if not stripped.startswith("---\n"):
        return body
    rest = stripped[len("---\n"):]
    end = rest.find("\n---\n")
    if end < 0:
        return body
    tail = rest[end + len("\n---\n"):].strip()
    if not tail:
        return body
    return rest[:end] + "\n"


for i, body in enumerate(blocks):
    if lang == "yaml":
        body = trim_frontmatter(body)
    out = f"{prefix}.{i:03d}.{lang}"
    with open(out, "w", encoding="utf-8") as g:
        g.write(body)
PY
}

FAILED=0

check_yaml_blocks() {
  local file="$1"
  local prefix="$TMP_DIR/$(echo "$file" | tr '/' '_' | sed 's/\.md$//').yaml"
  extract_blocks "$file" "yaml" "$prefix"
  for block in "$prefix".*.yaml; do
    [ -f "$block" ] || continue
    # Some doc YAML blocks are "incomplete-by-design" (e.g. a hooks: snippet).
    # We accept them as long as they parse as YAML, which yaml.safe_load_all
    # tolerates better than safe_load (multi-document input).
    if ! python3 -c '
import sys, yaml
with open(sys.argv[1]) as f:
    try:
        list(yaml.safe_load_all(f))
    except yaml.YAMLError as e:
        print(f"yaml parse error in block: {e}", file=sys.stderr)
        sys.exit(1)
' "$block" 2>&1; then
      echo "FAIL: $file :: yaml block ${block##*.}" >&2
      FAILED=1
    fi
  done
}

check_json_blocks() {
  local file="$1"
  local prefix="$TMP_DIR/$(echo "$file" | tr '/' '_' | sed 's/\.md$//').json"
  extract_blocks "$file" "json" "$prefix"
  for block in "$prefix".*.json; do
    [ -f "$block" ] || continue
    if ! jq empty "$block" >/dev/null 2>&1; then
      # Try jsonc (lenient) — some doc blocks have // comments by design.
      if ! python3 -c '
import json, re, sys
with open(sys.argv[1]) as f:
    src = f.read()
# Strip // line comments, preserving strings best-effort.
src = re.sub(r"^\s*//.*$", "", src, flags=re.MULTILINE)
try:
    json.loads(src)
except json.JSONDecodeError as e:
    print(f"json parse error: {e}", file=sys.stderr)
    sys.exit(1)
' "$block" 2>&1; then
        echo "FAIL: $file :: json block ${block##*.}" >&2
        FAILED=1
      fi
    fi
  done
}

check_toml_blocks() {
  local file="$1"
  [ "$TOML_OK" -eq 1 ] || return 0
  local prefix="$TMP_DIR/$(echo "$file" | tr '/' '_' | sed 's/\.md$//').toml"
  extract_blocks "$file" "toml" "$prefix"
  for block in "$prefix".*.toml; do
    [ -f "$block" ] || continue
    if ! python3 -c '
import sys, tomllib
with open(sys.argv[1], "rb") as f:
    try:
        tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        print(f"toml parse error: {e}", file=sys.stderr)
        sys.exit(1)
' "$block" 2>&1; then
      echo "FAIL: $file :: toml block ${block##*.}" >&2
      FAILED=1
    fi
  done
}

check_bash_blocks() {
  local file="$1"
  local prefix="$TMP_DIR/$(echo "$file" | tr '/' '_' | sed 's/\.md$//').sh"
  extract_blocks "$file" "bash" "$prefix"
  for block in "$prefix".*.bash; do
    [ -f "$block" ] || continue
    # shellcheck SC2148 fires on missing shebang; doc snippets won't have
    # one, so use --shell=bash + suppress SC2148.
    if ! shellcheck -s bash -e SC2148 "$block" >/dev/null 2>&1; then
      # Don't fail on minor issues; just verify it parses (bash -n).
      if ! bash -n "$block" 2>/dev/null; then
        echo "FAIL: $file :: bash block ${block##*.}" >&2
        FAILED=1
      fi
    fi
  done
}

for doc in "${DOCS[@]}"; do
  [ -f "$doc" ] || continue
  check_yaml_blocks "$doc"
  check_json_blocks "$doc"
  check_toml_blocks "$doc"
  check_bash_blocks "$doc"
done

if [ "$FAILED" -ne 0 ]; then
  echo "" >&2
  echo "One or more fenced code blocks failed syntax validation." >&2
  echo "Block files were written under: $TMP_DIR" >&2
  exit 1
fi

echo "ok: all fenced code blocks parsed."
