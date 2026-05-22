# Phase 6 — VERIFY notes

**Status:** Phase 6 implementation landed on branch `v0.3-phase-6-docker` (delivered as P6-A through P6-I).

This file captures (a) plan-doc deltas surfaced during Phase 6 implementation, (b) the manual-verification matrix per plan §7.4, and (c) follow-up items.

---

## 1. Plan-doc deltas surfaced at impl time

### 1.1 `bash` is `gh`'s install prerequisite — we already had it

Plan §3 caveats note that the `gh` CLI install in the `sinfonia-base` layer is ~30 MB. We kept it: the production hooks documented in `WORKFLOW.example.md` reach for `gh pr create / gh pr view / gh pr comment` directly. Removing `gh` would force every operator to install it inside their own `after_create` hook, and the size hit is paid once on the base layer (shared across five of the six images). The dev compose smoke test does not exercise `gh`, but every published image carries it.

### 1.2 The bridge image is its own root, not a child of `sinfonia-base`

Plan §3's draft Dockerfile parents `sinfonia-bridge` off `sinfonia-base` (which already inherited bash / git / curl / gh). The bridge binary doesn't need any of that — it talks to GitHub + the tracker over HTTPS only. Implementation parents `sinfonia-bridge` directly off `debian:bookworm-slim` and adds only `ca-certificates`, keeping the standalone-bridge image far below the < 50 MB plan target. Documented in the in-Dockerfile comment.

### 1.3 `tags(name)` helper instead of literal arrays in `docker-bake.hcl`

Plan §3.2 sketches each target's `tags = [...]` literal. With three tags per image (`:VERSION`, `:VERSION_MINOR`, `:latest`) across six images, that's eighteen literals to keep in sync. Implementation pulls them through a `tags(name)` HCL function that branches on the dev placeholder (`VERSION="dev"` → one tag) vs a real semver (three tags). One source of truth; the publish workflow's `bake --print` dump confirms the expected fan-out.

### 1.4 `docker-compose.ci.yml` needs `!reset` on the volume list, not just an override

Plan §7.3 shows a CI overlay that overrides the production `volumes:` block. YAML list merging in Compose is additive by default — without `!reset` the overlay would *append* the smoke-fixture mount to the production user-credential mounts and the CI container would still fail to start (the bind sources don't exist). Implementation uses the Compose v2.24+ `!reset` tag so the list is cleared before the overlay re-populates it.

Documented as a one-liner in `docker-compose.ci.yml`'s top comment.

### 1.5 `sinfonia-bridge --self-test` requires a config to run

Plan §7.2's smoke step `docker run --rm sinfonia-bridge:latest --self-test || true` assumes the binary boots far enough to run its checks without `BRIDGE.md`. It doesn't — `run()` errors out at "BRIDGE.md not found at './BRIDGE.md'" before reaching `--self-test`. Implementation mounts `tests/fixtures/BRIDGE.smoke.md` for the smoke and asserts the bridge exits non-zero (it does — the smoke creds are fake, every self-test probe FAILs by design). The actual smoke claim is "the binary doesn't segfault on entry," not "self-test passes."

### 1.6 Smoke `WORKFLOW.smoke.md` needs a real tracker `kind`, not a stub

Plan §7.3 mentions "a stub tracker config that returns no candidate issues." Sinfonia doesn't have a stub tracker kind — `kind:` is `linear` or `jira`. The smoke fixture uses `kind: linear` with a fake `api_key` / `project_slug`; the orchestrator's `candidate fetch failed; skipping dispatch` path (`crates/sinfonia/src/orchestrator/mod.rs:243`) swallows the resulting Linear error and the daemon's HTTP `/api/v1/state` surface still binds. The smoke asserts the HTTP surface comes up; it does not assert successful polling.

---

## 2. Manual verification matrix

Per plan §7.4, the following checks run against a real release tag (`v0.3.0` or a release-candidate). The CI workflow at `.github/workflows/docker-publish.yml` automates §7.1, §7.2, and §7.3; §2 below is the human pass.

| Check | How to run | Pass criteria |
|---|---|---|
| All six images build clean | `docker buildx bake` from a clean checkout | All six targets succeed; no missing platform errors |
| Image sizes match plan targets | `docker images ghcr.io/o-side-systems/*` | `sinfonia` < 100 MB, `sinfonia-bridge` < 50 MB, `with-claude-code` < 600 MB, `with-codex` < 400 MB, `with-opencode` < 200 MB, `all-agents` < 800 MB. Target deltas are released-notes material, not blockers. |
| `--help` works on every image | `docker run --rm ghcr.io/o-side-systems/<img>:0.3.0 --help` for each of the six | Exit 0; help text printed |
| Daemon `--check` accepts `WORKFLOW.example.md` | See `tests/docker-smoke.sh` lines for the bind-mount form | Exit 0, prints `ok` |
| Bridge `--check` accepts `BRIDGE.example.md` | Equivalent with `sinfonia-bridge` image and bind mount | Exit 0, prints `ok` |
| Production compose comes up with real creds | `LINEAR_API_KEY=… GH_WEBHOOK_SECRET=… POSTGRES_PASSWORD=… docker compose up -d` (no overlay) | All four services in `Up` state; Postgres init logs show the §3 telemetry schema applied; `curl http://localhost:8080/api/v1/state` returns `200`; `curl http://localhost:8081/health` returns `200`. |
| End-to-end ticket cycle | Move a Linear test ticket into an active state, watch a real agent run, confirm a PR opens and CI feedback returns | Pre-existing Phase 1–5 acceptance criteria; bridge writes the §3 marker comment with attempt counters; `sessions` table in Postgres gets at least one row. |
| Trivy scan passes | Watch the publish workflow on the release tag | The `scan` matrix job exits 0 for each image (no CRITICAL/HIGH unfixed CVEs). LOW/MEDIUM SARIF uploads to the Security tab are informational, not blocking. |

---

## 3. Image-size measurements at release time

Recorded for `v0.3.0` (replace with real numbers post-build):

| Image | linux/amd64 | linux/arm64 | Plan target |
|---|---|---|---|
| `sinfonia` | TBD | TBD | < 100 MB |
| `sinfonia-bridge` | TBD | TBD | < 50 MB |
| `sinfonia-with-claude-code` | TBD | TBD | < 600 MB |
| `sinfonia-with-codex` | TBD | (may be amd64-only) | < 400 MB |
| `sinfonia-with-opencode` | TBD | TBD | < 200 MB |
| `sinfonia-all-agents` | TBD | (may be amd64-only) | < 800 MB |

The release-notes line per plan §3 ("we measure at build time and document the actual sizes") goes here.

---

## 4. Multi-arch availability

Per plan §6 open question 1:

- `linux/amd64` is mandatory for every image. CI fails the publish if amd64 doesn't build.
- `linux/arm64` is best-effort. The base + daemon + bridge are pure-Rust and build cleanly for arm64. The CLI-agent images depend on upstream arm64 binaries:
  - `claude-code` ships arm64 (Node 22 + an npm package — both arch-agnostic).
  - `codex` arm64 availability depends on the upstream `install.sh`; if it lacks arm64 the build fails. CI handles this via `docker buildx bake --set sinfonia-with-codex.platform=linux/amd64` when needed.
  - `opencode` similarly — the install script is upstream's call.

When a release downgrades an image to amd64-only, the release notes call it out and the GHCR manifest lists only the one platform.

---

## 5. Follow-ups

- **Distroless variant for the bridge image.** `sinfonia-bridge` doesn't need bash / glibc utilities — `gcr.io/distroless/cc-debian12` would cut another ~20 MB and shrink the CVE surface. Deferred to v0.4 per plan §8 open question 4.
- **Docker Hub mirror.** GHCR-only for v0.3.0 per plan §8 open question 5. Mirror via CI `docker push` step if demand surfaces; no work needed in v0.3.
- **Pin base image digests at release time.** The `docker-publish.yml` workflow prints digests via `docker buildx imagetools inspect` for audit. The actual Dockerfile still uses tag-based bases (`rust:1.78-bookworm`, `debian:bookworm-slim`); pinning by digest is a separate PR before each release tag so the digest dump in the workflow log captures the reproducible build inputs.
- **`setup-bridge` / `setup-telemetry` skills** (Phase 5) reference the new `docker-compose.yml` shape. No skill changes needed for Phase 6 — the shape they already generate matches what Phase 6 ships — but a release-notes cross-link is recorded in `docs/SKILLS.md` (Phase 7).
