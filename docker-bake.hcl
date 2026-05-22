# docker-bake.hcl — multi-image build for Sinfonia v0.3+.
#
# Build everything locally (no push) for testing:
#   docker buildx bake
#
# Build and push to GHCR (CI):
#   VERSION=0.3.0 docker buildx bake --push
#
# Build a single image:
#   docker buildx bake sinfonia
#
# The `VERSION` variable is the full semver (e.g. `0.3.0` or `0.3.0-rc.1`).
# `VERSION_MINOR` is derived from it (`0.3`) and gets the moving "latest
# 0.3.x" tag. `latest` is always tagged on production builds.

variable "VERSION" {
  default = "dev"
}

variable "REGISTRY" {
  default = "ghcr.io/o-side-systems"
}

# Derive the X.Y portion of VERSION ("0.3.0-rc.1" → "0.3"). When VERSION is
# the placeholder "dev" the regex doesn't match and we fall back to "dev".
variable "VERSION_MINOR" {
  default = regex_replace(VERSION, "^(\\d+\\.\\d+)\\..*$", "$1")
}

# Common platform list. Both Codex and OpenCode upstream ship arm64-linux
# binaries as of the pinned `CODEX_VERSION` / `OPENCODE_VERSION` in the
# Dockerfile; bump those args (and re-verify arm64 tarballs exist on
# upstream) on each release.
variable "PLATFORMS" {
  default = "linux/amd64,linux/arm64"
}

group "default" {
  targets = [
    "sinfonia",
    "sinfonia-bridge",
    "sinfonia-with-claude-code",
    "sinfonia-with-codex",
    "sinfonia-with-opencode",
    "sinfonia-all-agents",
  ]
}

# Tag helper: each image gets `:VERSION`, `:VERSION_MINOR`, and `:latest`.
# When VERSION="dev" we skip the minor tag (it would collide with VERSION).
function "tags" {
  params = [name]
  result = VERSION == "dev" ? ["${REGISTRY}/${name}:dev"] : ["${REGISTRY}/${name}:${VERSION}", "${REGISTRY}/${name}:${VERSION_MINOR}", "${REGISTRY}/${name}:latest"]
}

target "_base" {
  context    = "."
  dockerfile = "Dockerfile"
  platforms  = split(",", PLATFORMS)
}

target "sinfonia" {
  inherits = ["_base"]
  target   = "sinfonia"
  tags     = tags("sinfonia")
}

target "sinfonia-bridge" {
  inherits = ["_base"]
  target   = "sinfonia-bridge"
  tags     = tags("sinfonia-bridge")
}

target "sinfonia-with-claude-code" {
  inherits = ["_base"]
  target   = "sinfonia-with-claude-code"
  tags     = tags("sinfonia-with-claude-code")
}

target "sinfonia-with-codex" {
  inherits = ["_base"]
  target   = "sinfonia-with-codex"
  tags     = tags("sinfonia-with-codex")
}

target "sinfonia-with-opencode" {
  inherits = ["_base"]
  target   = "sinfonia-with-opencode"
  tags     = tags("sinfonia-with-opencode")
}

target "sinfonia-all-agents" {
  inherits = ["_base"]
  target   = "sinfonia-all-agents"
  tags     = tags("sinfonia-all-agents")
}
