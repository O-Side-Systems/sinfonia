# Changelog

All notable changes to Sinfonia are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0-alpha.3] — 2026-06-19

A bridge-reliability pre-release on the v0.4 line. Brings the bridge's merge gate and review-transition surface online: merges are now gated on the configured `required_checks` reporting green before the bridge acts, and the feedback loop owns the transition of an issue into review. Also wires the bridge into the dev Docker stack. No change to the custom-field envelope (`sinfonia_bridge_state_v1`), the well-known field set (SPEC §11.6.4), or the `bridge.json` contract (§7.1).

### Added

- **Green-CI merge gating on `required_checks`.** The bridge now confirms the configured required checks are green before merging, rather than acting on review state alone (`sinfonia-bridge`: `github/client.rs`, `merge/mod.rs`, `config.rs`).
- **Bridge-owned review transition.** The feedback loop transitions an issue into its review state under the bridge's ownership, with budget accounting (`sinfonia-bridge`: `feedback/mod.rs`, `feedback/transition.rs`, `feedback/budget.rs`).
- **Bridge service in the dev stack.** `docker-compose.dev.yml` gains the `bridge` service, with a `docker/BRIDGE.example.md` template and a `.gitignore` entry following the live-config/`.example` convention.

## [0.4.0-alpha.2] — 2026-06-17

A docs + tooling pre-release on the v0.4 line. Adds the **`.harness/` workspace convention** (HARNESS-SPEC §11) — the producer-repo's third durable layer alongside the test *sensor* and the `AGENTS.md` *map* — ships a deployable skeleton, and wires it into the WORKFLOW examples so a driven repo's agent reads its standards/criteria before building and compounds learnings back under the existing human-gated write protocol. Plus a containerized-agent Dockerfile fix. No orchestrator/bridge source-behavior change: the custom-field envelope (`sinfonia_bridge_state_v1`), the well-known field set (SPEC §11.6.4), and the `bridge.json` contract (§7.1) are unchanged, and the `.harness/` convention is RECOMMENDED-not-required so existing Sinfonia-ready repos remain conformant.

### Added

- **Harness `.harness/` workspace convention + deployable skeleton.** A prescribed, identical-in-every-repo directory the target repo is bootstrapped with, capturing the **how / what / why** that informs the agent before it writes code: `standards/` (coding, architecture + ADRs, documentation, compounding — *how* we build), `criteria/` (the per-step exit gates `plan` / `build` / `review`), and `knowledge/` (compounded learnings in an AI-indexable format). `templates/.harness/` ships the layout with fill-in stubs alongside `templates/{AGENTS.md,CODEOWNERS}`; the deployed `templates/AGENTS.md` now points agents at `.harness/` to read standards/criteria before building and to write learnings back. Knowledge/compounding writes are **human-gated** — they ride the same PR as the code change under CODEOWNERS, never an autonomous push, consistent with the doc-graph write protocol (DEC-004).

### Docs

- **HARNESS-SPEC §11 — the `.harness/` workspace.** New RECOMMENDED section specifying the workspace as the producer repo's third durable layer alongside the *sensor* (§4–§7) and the *map* (`CONTEXT-CONTRACT.md`): the prescribed directory layout (§11.1), the **execution loop** structure whose three exit gates are the `criteria/` files (§11.2 — the non-colliding name for the Plan→Build→Review flow; "harness" stays reserved for the sensor), the just-in-time read protocol (§11.3), the human-gated compounding write protocol cross-referenced to `CONTEXT-CONTRACT.md §6` / DEC-004 (§11.4), and bootstrapping from `templates/.harness/` (§11.5). The §1.1 prescribed/not-prescribed table, the §3 conformance overview, and the §9 checklist gain matching entries; `CONTEXT-CONTRACT.md §6.4` notes the same prohibition governs `.harness/knowledge/` writes.
- **WORKFLOW examples reference the `.harness/` workspace.** Both the generic `WORKFLOW.example.md` (conditionally — "if the repo has a `.harness/` workspace") and the fuller `docker/WORKFLOW.example.md` now point each state prompt at `.harness/standards/` + the step's `.harness/criteria/` gate, and add a human-gated compounding step (write learnings to `.harness/knowledge/` in the code PR). `docker/AGENTS.md` corrected: it no longer lists the operator-local, gitignored `docker/WORKFLOW.md` as a committed artifact (the committed template is `docker/WORKFLOW.example.md`), and references the `.harness/` convention.
- **Executive overview doc.** `docs/workflow-executive-overview.md` — a plain-language, mermaid-diagram walkthrough of the lifecycle (assigned → live) and the autonomy guardrails (attempt / cost / clarity gates) for non-implementer readers.

### Fixed

- **Docker: `IS_SANDBOX=1` baked into the claude-code agent image.** Claude Code refuses `--dangerously-skip-permissions` under root/sudo unless an explicit sandbox override is set; the agent images run as root, so without it the default `claude -p … --dangerously-skip-permissions` command exited 1 on every turn. Setting `ENV IS_SANDBOX=1` in the `sinfonia-with-claude-code` stage is Claude Code's supported opt-out for containerized use.

## [0.4.0-alpha.1] — 2026-06-15

First **v0.4** pre-release. A minor bump (`0.3 → 0.4`) opening the v0.4 line: this tag collects the v0.4 milestone work that landed since `[0.3.0-alpha.9]` — the orchestrator retry-storm fix, the dependency-gating and proactive-merge hardening of the workflow contract, the Repository Context Contract and its CI invariant linters, and the additive feedback-loop reliability + security + merge-coordinator proposals (0002–0005). Everything new in this release is either an orchestrator/bridge source fix, an additive opt-in capability (default off), a contract/spec amendment, or producer-side documentation; the custom-field envelope (`sinfonia_bridge_state_v1`), the well-known field set (SPEC §11.6.4), and the `bridge.json` contract (§7.1) are unchanged. Legacy defaults are preserved throughout — every new behavior ships dark behind an explicit flag.

### Added

- **Bridge: Sinfonia-native merge coordinator (Proposal 0005).** A tier-independent substitute for a GitHub native merge queue (Enterprise-Cloud-only for private repos). When `feedback_loop.merge_coordinator.enabled` is set (default **false**), an approved + green `sinfonia/<id>` PR is enqueued in a durable `landing_queue` and the bridge serially drives it `update-branch (integrate latest base) → await CI on the new head → merge` (default method `rebase`), making it "green against the `main` it will actually land on" without an Enterprise tier. The landing queue row's existence *is* the human-approval marker — the coordinator never self-approves; it parks back to `needs_fixes_state` on conflict, a closed PR, or exhausted `max_update_cycles`, composing with the existing attempt caps rather than adding a new one. Boot-time reconciliation runs before the webhook server binds so an out-of-band human merge cannot cause a double-merge. Adds a `pull_request_review` webhook handler, `get_pull_request` / `update_pr_branch` / `merge_pr` to the bridge's GitHub client (compare-and-set on head SHA), and the `feedback_loop.merge_coordinator` config block (see `BRIDGE.example.md`). Lives entirely in `crates/sinfonia-bridge` — the daemon's zero-GitHub-access invariant (SPEC §11.6.1 / §15.1) is preserved. Additive SPEC §8.2 landing-lifecycle note and an HARNESS-SPEC §7.4 note recording the coordinator as an accepted native-queue substitute.
- **Bridge + daemon: agent tool-surface security hardening (Proposal 0004 subset, default-safe).** Symlink-resolving confinement for the agent file tools (an `..`/symlink escape out of the workspace root is now rejected), a pinned tool-catalog test, an opt-in `env_policy` for subprocess environment scoping (default `inherit` — legacy behavior preserved), a `dispatch_allowlist.require_labels` gate, and a startup WARN when the agent runs with `--dangerously-skip-permissions`. The documented mitigation for the retained permissive default is running Sinfonia in a container/VM (`SECURITY.md`), not flipping the default. Adds `SECURITY.md` and SPEC §15 posture notes.
- **Repository Context Contract + dogfooded context graph (v0.4 Phase 5).** `docs/CONTEXT-CONTRACT.md` defines a portable, agent-readable repository-context convention; the repo dogfoods it with a root `AGENTS.md`, per-area nodes (`crates/*`, `docker`, `docs`), `templates/{AGENTS.md,CODEOWNERS}` for downstream repos, and CODEOWNERS gating on all `**/AGENTS.md` edits. A just-in-time read protocol is applied across the `WORKFLOW.example.md` state prompts; HARNESS-SPEC §7.3/§9 record the convention.
- **CI invariant linters for the context graph (v0.4 Phase 5/6).** `scripts/scan-overlap.sh` (deterministic shared-surface overlap scan) + `lint-pr-overlap.sh`, `scripts/lint-stale-nodes.sh` (path-aware stale-node detection), and `scripts/decompose-consistency-check.sh`, wired into a new `.github/workflows/invariant-linters.yml` and a PR-triggered `overlap-linter.yml`. Seeded fixtures and runners under `tests/lint/`.
- **Daemon: continuous workspace disk reclamation (`workspace.cleanup`).** Previously workspaces were swept only for terminal-state issues and only at startup (SPEC §16.1), so a long-running daemon accumulated In Review / errored / unswept checkouts — each a full repo clone plus a multi-GB build `target/` — until the next restart. The terminal-state sweep now also runs periodically (`sweep_interval_secs`, default 600; additive-safe, removes only Done/Cancelled workspaces). A new opt-in age-based reaper (`max_age_hours`, default `0`/off) additionally removes any idle workspace not modified within the window, skipping issues currently running — reaped checkouts are re-created on demand. Adds `WorkspaceManager::{list_workspaces, remove_key}` and a pure, tested selection helper; see `WORKFLOW.example.md`.

### Changed

- **Daemon: dependency gating keys solely on `blocks` relations (v0.4 Phase 3).** Removed the parent-child (`children`) hierarchy dispatch gate; Linear/Jira fetches no longer request children. Dependency gating is now two complementary layers — a coarse orchestrator pre-filter (`Todo` only, non-terminal blocker → skip) and an authoritative workflow STEP 0 gate that verifies each blocker's PR is merged to `main` (not merely in a terminal state) before any code changes, posting an idempotent comment and stopping otherwise. A new BLOCK-01 guardrail enforces the merged-to-`main` check in both STEP 0 prompts. SPEC §8.2 amended to the two-layer model; ADR `0002-orchestrator-gating-ground-truth` records the verified predicate.
- **Workflow: proactive pre-PR merge gates + mergeability loop (v0.4 Phase 4).** Adds a proactive pre-PR gate (MERGE-01) and a mergeability-w.r.t.-`main` loop (MERGE-02) to `WORKFLOW.example.md`, refactoring STEP 1 into a dedicated Mergeability procedure. The gate is "no merge conflict against `main`" — looping only on `DIRTY`/`BEHIND` and treating `BLOCKED`/`UNSTABLE` as ready-for-human — correcting a literal-`CLEAN` reading that would deadlock a fresh agent PR awaiting required review. HARNESS-SPEC §7.4 + §9 and `DEPLOYMENT.md` document the merge queue, the post-merge `main` gate, and serial-foundation / leaf-fan-out concurrency (`max_concurrent_agents_by_state: "In Progress": 1`).

### Fixed

- **Daemon: stop retry-pending issues from being re-dispatched by the poll tick.** A failing issue entered a tight crash-retry loop, re-running every 3–7 s despite the 10 s backoff floor. The `claimed` check only gated slot acquisition, never skipping a *fresh* dispatch of an already-claimed issue, so each poll tick clobbered the pending `RetryEntry` (resetting `attempt` to 1) and backoff never escalated; the `fire_retry` requeue guard also fired only on `NoSlot`. A fresh poll dispatch (`attempt == None`) of a claimed issue now returns `Skipped` and lets the backoff schedule own the retry, so backoff escalates monotonically as intended. This is the retry-storm fix the branch is named for.

### Docs

- **Feedback-loop architecture + reliability/security proposals.** `docs/ARCHITECTURE.md` (mermaid diagrams of the CI→tracker→agent feedback loops and the human-in-the-loop gates) and four new proposals: `0002-orchestrator-gating-ground-truth` (the verified dispatch-gate predicate), `0003-feedback-loop-reliability-seams` (per-ticket envelope lock, compare-and-set transition, missed-webhook reconcile, restart resume-detection — proposed, not yet implemented), `0004-agent-tool-surface-hardening`, and `0005-merge-coordinator`. Proposal 0001 (harness ingestion) flipped to Accepted/Closed for the v0.4 line with golden-snapshot and no-disk-write regression tests added.
- **HARNESS-SPEC amendments from the wyrd-builder reference port (tech-stack-agnostic).** New §5.6 (gating tag separating merge-gating scenarios from authored-ahead RED ones; `bridge.json.failures` carries only gating failures — reconciles §4.2 with §7.4 and fixes a dangling §7.4→§5.5 reference) and §5.7 (non-vacuous gates: every required check must be guarded against a vacuous green that ran zero assertions). §7.4 now requires the PR gate to trigger on `synchronize` (so a merge-queue / merge-coordinator base-sync re-tests), §7.3 adds a RECOMMENDED local pre-push parity hook, §7.2 notes multi-substrate gating, and the §9 checklist gains the matching items. All inferred from `O-Side-Systems/wyrd-builder`'s harness; stack-specific details (in-process determinism, cargo/pgTAP/Playwright, in-repo `sinfonia.toml`) deliberately kept out.

## [0.3.0-alpha.9] — 2026-05-29

First v0.3 alpha with a **source-code** payload change since `[0.3.0-alpha.4]` (alpha.5–8 were Docker-pipeline-only re-publishes of the alpha.4 source). Closes the harness feedback gap in the bridge: the red-CI retry turn now carries structured, scenario-level diagnostics sourced from the test harness instead of a comma-joined list of check names. Confined to `crates/sinfonia-bridge`; the orchestrator crate, the custom-field envelope (`sinfonia_bridge_state_v1`), and the well-known field set are unchanged. Ships with the producer-side `docs/HARNESS-SPEC.md` and the `docs/proposals/0001-*` design docs.

### Added

- **Bridge: optional ingestion of a harness `bridge.json` manifest (Proposal 0001).** When `feedback_loop.ingest_harness_manifest` is enabled — now the default — the bridge fetches the CI run's `bridge-*` artifact on a red `workflow_run`, parses the structured per-scenario failures (`scenario` / `feature_file` / `step` / `assertion`) and artifact references, and folds them into `sinfonia_last_ci_failure` for the retry prompt. Previously the bridge's red-CI feedback was built entirely from check-run *names* plus a hardcoded placeholder, so the inner agent loop retried against "the e2e suite failed" with nothing about *why*; the structured diagnostics the harness already emits one HTTP call away were dropped at the bridge boundary. Versioned against `bridge.json schema_version 2` with warn-and-degrade on mismatch (older/absent → check-name fallback; newer-additive → forward-read known fields). Treated as untrusted input (it may originate from a fork PR): size, count, and length caps; in-memory-only parsing with a per-entry decompressed (zip-bomb) cap; `artifact_urls` never fetched server-side; manifest text bound as a scalar, never evaluated as a template. Adds `list_run_artifacts` / `download_artifact` to the bridge's GitHub client and six optional `feedback_loop` keys (glob, filename, and the size/count/digest caps — see `BRIDGE.example.md`). No custom-field envelope migration and no orchestrator change: the digest lands in the existing `sinfonia_last_ci_failure` string, so the well-known field set (SPEC §11.6.4) is unchanged. New SPEC §11.6.13 documents the consumed shape, the `workflow_run`-keyed retrieval, the version gate, and the degradation matrix; §11.6.2 and §12.5 note the field's new role as the primary retry-turn diagnostic channel. A **minor**, additive, opt-out capability within the v0.3 line; the envelope stays `sinfonia_bridge_state_v1`.

### Docs

- **Producer-side `docs/HARNESS-SPEC.md`** — the authoring spec for a target repo that emits the `bridge.json` contract this release consumes. The natural-language → executable-specification step (§4.1) is **OPTIONAL**, not a conformance MUST: Sinfonia's agent takes its per-issue instructions from the tracker, so the spec generator is a bootstrapping convenience (used by the BCF reference harness), not a loop requirement. Adds `docs/proposals/0001-harness-feedback-ingestion.md` and its implementation plan.

## [0.3.0-alpha.8] — 2026-05-23

First v0.3 alpha with a fully-green Docker publish pipeline end-to-end (build → push → image smoke → compose stack smoke → per-image Trivy CVE scans). Closes out the four-tag iteration on the publish workflow that began at `[0.3.0-alpha.5]`. Source-code payload is unchanged from `[0.3.0-alpha.4]`; only `.github/workflows/docker-publish.yml` changes in this tag.

### Fixed

- **Split Trivy into two action invocations so the CRITICAL-only gate fires correctly.** `[0.3.0-alpha.7]` set `severity: CRITICAL` on the single existing Trivy step expecting it to gate only on CRITICAL findings — but `aquasecurity/trivy-action`, when `format: sarif`, internally overrides `TRIVY_SEVERITY` to ALL severities so the SARIF captures the full picture ("Building SARIF report with all severities" appears in the action log). That override also makes `exit-code: 1` trigger on any finding, ignoring the user-supplied `severity` input — so the alpha.7 gate change was effectively a no-op and the workflow still failed on HIGH findings despite zero CRITICAL findings. Fix splits into two steps: (1) `format: sarif`, `severity: CRITICAL,HIGH,MEDIUM,LOW,UNKNOWN`, `exit-code: "0"` — generates the SARIF report-only, never gates; (2) `format: table`, `severity: CRITICAL`, `exit-code: "1"` — actually gates strictly on CRITICAL. The second invocation reuses the vuln DB downloaded by the first via the shared workspace `cache-dir` (~5s incremental cost per matrix entry). Security tab still receives the full CRITICAL+HIGH+below picture per-image; HIGH findings no longer block a release.

## [0.3.0-alpha.7] — 2026-05-22

Re-publish of `[0.3.0-alpha.6]` addressing the real CVEs that alpha.6's scan surfaced. The Debian CRITICAL is fully resolved by this tag (every image dropped to 0 CRITICAL findings); the Trivy gate-relaxation attempted alongside it is a no-op due to an `aquasecurity/trivy-action` SARIF quirk and is corrected in `[0.3.0-alpha.8]`.

### Fixed

- **`apt-get upgrade -y` in `sinfonia-base` and `sinfonia-bridge` RUN blocks.** `[0.3.0-alpha.6]`'s Trivy scan surfaced 2 CRITICAL + 9 HIGH real CVEs across the six published images. The CRITICAL — `CVE-2026-42010`, a gnutls authentication bypass via NUL character in username — came from the `libgnutls30` package in `debian:bookworm-slim`. Adding `apt-get upgrade -y` after the install line in both base layers (so each rebuild picks up Debian security backports without bumping the base tag) eliminates `CVE-2026-42010` along with the four other gnutls HIGHs. The `sinfonia-bridge` image — the only one not depending on `sinfonia-base` and the only one without `gh` baked in — returns a fully clean scan after this change, confirming Debian's backport is in `bookworm-security`. Remaining HIGHs (`gh`'s Go-stdlib CVEs at `usr/bin/gh`, and npm/picomatch transitively under `@anthropic-ai/claude-code` in claude-code-bearing variants) persist until the upstream packages rebuild against patched dependencies.

### Changed

- **Attempted to relax the Trivy gate from `CRITICAL,HIGH` to `CRITICAL` only.** Set `severity: CRITICAL` on `.github/workflows/docker-publish.yml`'s Trivy scan step with the goal of letting HIGH findings appear in the Security tab without blocking the release. Change is functionally a no-op due to the `aquasecurity/trivy-action` SARIF behavior described in `[0.3.0-alpha.8]`'s entry — corrected there.

## [0.3.0-alpha.6] — 2026-05-22

Re-publish of `[0.3.0-alpha.5]` with the Trivy GHCR-auth bug fixed. First tag in the v0.3 series where the post-publish CVE scan ran end-to-end and produced meaningful findings rather than failing on tooling.

### Fixed

- **Add `docker/login-action` to the Trivy `scan` job in `.github/workflows/docker-publish.yml`.** The matrix `scan` job had no GHCR-login step (the existing `docker/login-action` was scoped to the sibling `build-and-push` job), so Trivy could neither inspect the freshly-pushed image via the local Docker daemon (never pulled into the scan runner) nor fall back to its `remote` source — the latter returned `GET https://ghcr.io/token?...&service=ghcr.io: UNAUTHORIZED: authentication required`. Each scan exited ~18s with a `unable to find the specified image` fatal, no SARIF produced, and `Upload SARIF` then failed with `Path does not exist: trivy-<image>.sarif`. Fix re-uses the same `docker/login-action@v3` block `build-and-push` already uses, scoped to `ghcr.io` with `github.actor` + `secrets.GITHUB_TOKEN`. First surfaced on `[0.3.0-alpha.5]` because that was the first tag whose `build-and-push` job got far enough to reach the scan stage; the bug had been latent since Phase 6 added the Trivy matrix.

### Surfaced (not blocking; informational)

- **First real CVE picture for the published images.** With the gate now actually running, six per-image Trivy analyses uploaded SARIF to the Security tab: 13–26 findings per image total, of which 2 CRITICAL + 9 HIGH in the base `sinfonia` image alone. CRITICAL was `CVE-2026-42010` (gnutls auth bypass) in the Debian 12 base; HIGHs distributed across `usr/bin/gh` (5 Go-stdlib CVEs) and gnutls (4 more). The claude-code-bearing variants additionally carried `CVE-2026-33671` (npm `picomatch` ReDoS) under `usr/lib/node_modules/npm/node_modules/picomatch`. CRITICAL is patched in `[0.3.0-alpha.7]`; HIGHs are documented but not gated as of `[0.3.0-alpha.8]`.

## [0.3.0-alpha.5] — 2026-05-22

Re-publish of `[0.3.0-alpha.4]` with one Docker publish pipeline fix. First v0.3 tag whose `build-and-push` job ran to completion (alpha.2 → alpha.4 each failed earlier in the pipeline).

### Fixed

- **Point `sinfonia --check` smoke at the env-var-free fixture in `tests/docker-smoke.sh`.** The smoke step was mounting `WORKFLOW.example.md` into the `sinfonia --check` invocation, but that file references `$LINEAR_API_KEY` via Liquid and the smoke runner has no credentials — `--check` failed with an unresolved-variable error. Fix repoints the mount at `tests/fixtures/WORKFLOW.smoke.md`, the self-contained smoke fixture committed in `[0.3.0-alpha.2]` alongside `tests/fixtures/BRIDGE.smoke.md` (which the bridge `--self-test` step had already been using). The smoke harness header comment is refreshed to describe the env-var-free design intent so the next person to edit `docker-smoke.sh` doesn't reintroduce the env-coupled fixture. With this fix `build-and-push` passed (1h12m); the Trivy `scan` matrix still failed on a separate, latent GHCR-auth bug fixed in `[0.3.0-alpha.6]`.

## [0.3.0-alpha.4] — 2026-05-22

Re-publish of `[0.3.0-alpha.3]`. The Rust toolchain bump in alpha.3 fixed the cargo build, but the Docker publish pipeline still failed on a second, independent layer: the upstream Codex CLI install script (`https://github.com/openai/codex/releases/latest/download/install.sh`) errors with `Could not find SHA-256 digest for codex-package-x86_64-unknown-linux-musl.tar.gz in codex-package_SHA256SUMS` when invoked from inside the slim builder. The same `awk` lookup against the same file succeeds locally and resolves the expected digest cleanly — the failure is reproducible inside the `debian:bookworm-slim`-based build environment and resists remote debugging.

### Changed

- **Skip upstream install scripts for Codex and OpenCode in the Docker images.** Both `sinfonia-with-codex` and `sinfonia-with-opencode` (and the combined `sinfonia-all-agents`) now download the bare-binary tarballs directly (`codex-${triple}.tar.gz` from `openai/codex` releases; `opencode-linux-${arch}.tar.gz` from `sst/opencode` releases), extract, install to `/usr/local/bin/`, and self-verify with a `--version` smoke. Versions are pinned via Dockerfile `ARG CODEX_VERSION=rust-v0.133.0` / `ARG OPENCODE_VERSION=v1.15.9`; bump on each release after confirming the upstream tarballs exist for both linux/amd64 + linux/arm64. Bypasses the install scripts' SHA256SUMS-verification dance entirely. `curl --retry 3 --retry-delay 5` provides transient-network resilience. The pinned versions are operator-overridable via `docker buildx bake --set sinfonia-with-codex.args.CODEX_VERSION=<tag>`.
- **`docker-bake.hcl` comments updated** — both pinned upstreams now ship arm64-linux binaries, so the prior "MAY not publish linux/arm64" caveat is removed from the `PLATFORMS` and per-target comments.

## [0.3.0-alpha.3] — 2026-05-22

Re-publish of `[0.3.0-alpha.2]` with the toolchain fix below. The
`v0.3.0-alpha.2` tag and GitHub Release remain visible as a "did not
produce artifacts" marker — the Docker publish workflow failed on that
tag because the pinned `rust:1.78-bookworm` builder image is two stable
releases behind what current transitive dependencies need.

### Changed

- **Bump workspace MSRV from `1.78` to `1.88`.** Two stacking
  transitive-dep constraints surfaced when the Docker publish pipeline
  rebuilt against current `Cargo.lock` on the v0.3.0-alpha.2 tag:
  `hashbrown 0.17.1` requires the `edition2024` Cargo feature
  (stabilized in Rust 1.85), and `tonic 0.14` + `time-macros 0.2.27`
  require Rust 1.88. `Dockerfile` updated from `rust:1.78-bookworm` to
  `rust:1.88-bookworm`; `Cargo.toml`'s `workspace.package.rust-version`
  bumped to match; README MSRV badge and prerequisites line updated.
  `Dockerfile.dev` is unaffected — it installs via `rustup` and tracks
  stable. Existing v0.2 / v0.3.0-alpha.1 deployments running compiled
  binaries are unaffected.

## [0.3.0-alpha.2] — 2026-05-22

Second v0.3 preview. Makes Sinfonia legible as a team-grade orchestrator alongside its existing single-user shape. Six additions over `[0.3.0-alpha.1]`, all opt-in: OpenTelemetry emission tenant-tagged from day one (Phase 3), the Jira bridge write surface (Phase 4), six setup skills + `sinfonia --check` / `sinfonia init` CLIs for AI-coding-tool-driven scaffolding (Phase 5), six published Docker images (Phase 6), and the finalized doc set with `docs/DEPLOYMENT.md` + `docs/CLIENT_SETUP.md` + `docs/MIGRATION-v0.2-to-v0.3.md` (Phase 7). The daemon's behaviour against an unchanged v0.2 `WORKFLOW.md` is preserved — see [`docs/MIGRATION-v0.2-to-v0.3.md`](docs/MIGRATION-v0.2-to-v0.3.md). v0.3.0 GA waits on a manual readthrough of the doc set and the carried-forward manual-verification debts (Linear bridge end-to-end, OpenCode + Linear, Collector + Postgres cap-hit cycle, real Jira sandbox).

### Added

- **Finalized documentation (Phase 7).** SPEC §11.5 tightened ("orchestrator MUST NOT write"; pointer to §11.6) and §11.6 promoted from Draft to Recommended Extension; new §11.7 documents custom-field discovery per tracker (Linear marker-comment vs Jira `customfield_NNNNN` resolution); §18.2 grew bullets for the Jira tracker adapter, the CI feedback bridge, failure categorization, budget enforcement, and PR label management (alongside the OpenCode + OpenTelemetry + setup-skills entries from earlier phases). New guides: `docs/DEPLOYMENT.md` (four topologies + credential model + scaling + backup + upgrading), `docs/CLIENT_SETUP.md` (enterprise adoption checklist with trust-boundary diagram, security posture, budget controls, audit-trail queries, handoff runbook headers, and a vendor-evaluation worksheet), `docs/MIGRATION-v0.2-to-v0.3.md` (required + optional + breaking-changes sections). `WORKFLOW.example.md` gains a `telemetry:` block walkthrough, three OpenCode usage variants (default-lane / state-machine / air-gapped Ollama-with-LSP), and a full failure-categorization state-machine example wired to `BRIDGE.md`'s `feedback_loop.failure_categories`. `BRIDGE.example.md` budget-caps section ships realistic non-null example values; new cost_table_path override block; cross-link block at the bottom. `CONTRIBUTING.md` updated for the Cargo workspace layout (three-crate table; `--workspace` / `-p crate` commands; "where to add new code" guidance). Docs CI: link-rot via lychee (internal-only on PR, full-link weekly), Markdown lint via markdownlint-cli2, fenced YAML/TOML/JSON code-block syntax checks.

- **Docker images + production compose (Phase 6).** Six production images publish to `ghcr.io/o-side-systems/` from a single multi-stage `Dockerfile` driven by `docker-bake.hcl`: `sinfonia` (daemon only, Debian slim + bash/git/curl/gh), `sinfonia-bridge` (bridge only, no agent or git tooling), three single-agent variants (`sinfonia-with-claude-code` adds Node 22 + `@anthropic-ai/claude-code`; `sinfonia-with-codex` and `sinfonia-with-opencode` install the respective CLIs via upstream install scripts), and `sinfonia-all-agents` for state-machine deployments that route across agents. Each image is tagged `:VERSION`, `:VERSION_MINOR`, and `:latest`; built for `linux/amd64` and `linux/arm64` where the underlying CLI provides arm64. The build stage shares one `cargo build --release` across all six targets via BuildKit registry + target cache mounts. The new root `docker-compose.yml` demonstrates a production topology (daemon + bridge + OTel Collector + Postgres) with read-only mounts for the per-agent credential directories (`~/.claude`, `~/.codex`, `~/.opencode`) and the Phase 3 telemetry schema applied to Postgres via `docker-entrypoint-initdb.d`. The pre-existing dev-shell Dockerfile + compose move to `Dockerfile.dev` / `docker-compose.dev.yml` with their behaviour unchanged. Publishing runs through `.github/workflows/docker-publish.yml` on every `v*` tag — `docker buildx bake --push`, image-digest inspection, image / compose smoke tests, then a per-image Trivy scan with `severity: CRITICAL,HIGH` and `exit-code: 1` (CRITICAL/HIGH CVEs fail the publish; LOW/MEDIUM upload SARIF to the Security tab without blocking).

- **Compose smoke harness.** `tests/docker-compose-smoke.sh` brings the full production compose stack up with the `docker-compose.ci.yml` overlay (which strips the user-credential bind mounts that don't exist in CI and swaps in `tests/fixtures/WORKFLOW.smoke.md` + `tests/fixtures/BRIDGE.smoke.md`), polls `http://localhost:8080/api/v1/state` and `http://localhost:8081/health` on a bounded 30-second retry, and tears the stack back down. The `!reset` Compose-spec tag clears the inherited volume list so the overlay merges cleanly on Compose v2.24+. `tests/docker-smoke.sh` is the lighter per-image variant — `docker run --rm <img> --help` against all six images plus `sinfonia --check WORKFLOW.example.md` against the daemon image.

- **`docker-bake.hcl`** as the single source of truth for the production image matrix — targets, registry, platforms, and the `tags(name)` helper that fans `${VERSION}` out into the three-tag set per image.

- **Setup skills + CLI extensions (Phase 5).** Six setup skills ship at `skills/` in the repo root: `setup-workflow`, `setup-bridge`, `setup-state-machine`, `setup-telemetry`, `setup-agent-backend`, `migrate-from-symphony`. Each is a self-contained folder with a `SKILL.md` runbook (YAML front-matter format with `name`/`description`/`version` keys), Liquid templates for the artifacts the skill produces (WORKFLOW.md, BRIDGE.md, docker-compose snippets, GitHub Actions workflow, per-state prompts, per-backend agent blocks), and optional shell-script validators wrapping the CLI gates. AI tools (Claude Code, OpenCode, Codex) invoke the skills directly; humans can read each `SKILL.md` as a runbook. `docs/SKILLS.md` is the cross-vendor pointer table. Phase 5 §1 distribution model is locked: Sinfonia ships the skills; auto-install into AI-tool-specific directories is owned by each tool vendor.

- **`sinfonia --check <WORKFLOW.md>`** (Phase 5 §3.1). Validates a workflow without running. Loads YAML, runs `validate_for_dispatch`, then renders every prompt template (workflow body + per-state overrides) against a stub Issue to catch strict-Liquid errors before the operator hits "go." Exit codes: `0` ok / `2` YAML parse / `3` schema / `4` template / `5` tracker auth — skills branch on the exit code to give specific remediation prompts.

- **`sinfonia init`** (Phase 5 §3.2). Interactive `inquire`-driven REPL that scaffolds a `WORKFLOW.md`. The AI-tool-free equivalent of the `setup-workflow` skill. Walks tracker selection, project slug, endpoint/email when applicable, active/terminal states, default agent backend, and workspace root. Linear flow with abort-on-error (re-run to start over).

- **State-machine prompt invariant.** `skills/setup-state-machine/templates/*.liquid` enforce the strict-Liquid contract by construction — every `{{ issue.fields.X }}` reference is followed by `| default: "…"`. The `state_machine_prompts_have_no_unguarded_issue_fields` integration test greps for unguarded references and fails CI on a hit. A human can drag a ticket into Needs Fixes without any prior bridge run; the prompt renders cleanly because the `| default:` filter handles absent fields.

- **`docs/SKILLS.md`** with the cross-vendor pointer table for skill installation (Claude Code, OpenCode, Codex CLI) and the skill-contract documentation (front-matter keys + strict-Liquid invariant).

- **Jira bridge support (Phase 4).** The five `IssueTracker` bridge-write methods (`transition_issue`, `read_custom_field`, `write_custom_field`, `ensure_custom_field`, `post_comment`) are implemented for `JiraTracker` against the Atlassian Cloud REST API v3. Custom-field IDs (`customfield_NNNNN`) are resolved once per bridge-stable key and cached for the process lifetime via a `tokio::sync::RwLock<HashMap>`. State transitions go through `GET /transitions` to resolve the target state to a transition id and `POST /transitions` with that id. Comments are emitted as Atlassian Document Format (ADF) via a narrow-scope hand-rolled Markdown→ADF converter (paragraphs, fenced code blocks, lists, inline strong/em/code/link) in `crates/sinfonia-tracker/src/jira_adf.rs`. `ensure_custom_field` creates missing fields via `POST /rest/api/3/field` and attempts a best-effort screen-scheme bind so the field shows up in the Jira UI; failures log a WARN pointing to `docs/JIRA-SCREEN-SCHEME.md` for manual setup. Self-hosted Jira Server / Data Center is supported via PAT-only auth (omit `tracker.email`, put the token in `tracker.api_key` — the adapter switches from Basic to Bearer auth).

- **Bridge config validation accepts `tracker.kind: jira`** (was: rejected with "deferred to Phase 4"). Two new positive rules: `tracker.endpoint` is required for Jira (no sensible per-tenant default), and `tracker.email` is required when the endpoint matches `*.atlassian.net` (Cloud uses Basic auth with email + token). Self-hosted endpoints proceed without an email.

- **Jira self-test probe.** `sinfonia-bridge --self-test` now exercises the Jira candidate-fetch path (`POST /rest/api/3/search`) when `tracker.kind: jira` — auth + project visibility surface as the `tracker` check line (replaces the previous "unimplemented (Phase 4)" stub).

- **OpenCode coding-agent backend.** `provider: opencode` in `WORKFLOW.md` (and inside `states:` blocks) now drives the `opencode` CLI (<https://opencode.ai>) as a subprocess in the per-issue workspace, joining `claude_code` and `codex` as a sibling subprocess-driven backend. The prompt is piped on stdin, OpenCode events are read one JSON object per line from stdout (`--format json`), and the per-session ID is resumed on retry turns via `--session <id>`. Auth is owned by the `opencode` CLI itself (`opencode auth login`) — Sinfonia does NOT pass an api_key. The `model:` field is passed through verbatim with OpenCode's `provider/model` wire format (e.g. `anthropic/claude-sonnet-4-6`, `ollama/qwen2.5-coder:32b`). OpenCode adds LSP integration, MCP support, and 75+ provider backends — most notably an Ollama-with-LSP path that the raw `ollama` backend can't provide. Implementation lives in `crates/sinfonia/src/agent/opencode.rs`; the workspace gains the `which` crate as a workspace-level dependency for the preflight binary check. See `WORKFLOW.example.md`, the README backend table, and `docs/SPEC.md` §18.2 for usage.

- **OpenTelemetry emission (Phase 3).** Both binaries layer an optional OTLP exporter over the existing `tracing` subscribers. When `OTEL_EXPORTER_OTLP_ENDPOINT` is unset and no `telemetry:` block is configured, behaviour matches today — the OTel layer is `None` and the binaries run stdout-only. When configured, Sinfonia emits six spans (`orchestrator.tick`, `orchestrator.dispatch`, `runner.session`, `runner.turn`, `workspace.hook`, `tracker.fetch`) and the bridge emits six more (`bridge.webhook`, `bridge.ci_result`, `bridge.state_transition`, `bridge.cap_hit`, `bridge.cost_update`, `bridge.events_receive`). Every span carries the resolved `tenant_id` (precedence: `telemetry.tenant_id` → `SINFONIA_TENANT_ID` env → `"default"`); resource-level `service.namespace = tenant_id` lets a Collector routing-processor split per-tenant data without touching emission code. Crate set: `opentelemetry 0.32` / `opentelemetry_sdk 0.32` / `opentelemetry-otlp 0.32` / `tracing-opentelemetry 0.33`.

- **Typed Sinfonia↔bridge event channel.** The bridge no longer requires running an OTLP receiver (the original H-2 plan-review fix). Sinfonia POSTs typed events (`runner.session.completed`) to subscribers registered via `POST /api/v1/events/subscribers` — HMAC-SHA256 signed (header `X-Sinfonia-Signature-256`, same scheme as the GitHub webhook) with a `telemetry.sinfonia_events_secret` shared between `WORKFLOW.md` and `BRIDGE.md`. Mismatch returns HTTP 401 and the Sinfonia retry loop logs `WARN` on exhaustion. The bridge handler at `POST /api/v1/sinfonia-events` parses the body, feeds it into the budget pipeline, and (on cap-hit) transitions the ticket to `feedback_loop.budget_exceeded_state`. Diagnostic surface: `GET /api/v1/events/recent` returns the last 200 emitted events.

- **Budget enforcement.** `crates/sinfonia-bridge/src/feedback/budget.rs` adds a per-process per-ticket cost / token accumulator. Token + cost caps from `BRIDGE.md` (`max_tokens_per_ticket`, `max_cost_per_ticket_usd`) are enforced at the tracker write boundary; cap-crossings transition the ticket to `budget_exceeded_state` and write the `sinfonia_tokens_consumed` / `sinfonia_cost_consumed_usd` / `sinfonia_budget_exhausted_at` custom fields. Cost values are stringified via `rust_decimal::Decimal` per STATUS §5.1 (never f64 on the wire). A 30 s idle-debounce reconciler coalesces under-cap writes so a busy ticket emits one Linear API hit per quiet window instead of one per session.

- **Cost table** (`config/cost_table.yaml`) embedded into the bridge via `include_str!`, overridable at runtime via `bridge.cost_table_path`. Includes Anthropic, OpenAI, Google, and Ollama (zero-cost local) entries verified against provider pricing pages on 2026-05-21. Two freshness gates: `WARN` log at startup if `verified_at` is more than 90 days stale, and the M-2 plan-checker fix — the bridge refuses to apply COST caps (token caps stay enforced) when the table is more than 180 days stale.

- **`AgentEvent::SessionCompleted`** variant emitted by the runner immediately after `agent.stop_session(...)` per the N-3 plan-checker fix. Carries the per-session token totals, exit reason, and provider/model the bridge needs without re-parsing the event stream.

- **`WELL_KNOWN_FIELDS`** registry gains `sinfonia_budget_exhausted_at` so templates referencing it via `| default: …` don't trip strict-mode Liquid.

- **Reference Collector + Postgres assets** at `examples/telemetry/`:
  - `postgres-schema.sql` — sessions / attempts / events tables with the indexes the §8.2 dashboard queries expect.
  - `otel-collector-config.yaml` — receiver + routing-by-tenant processor + Postgres exporter starter.
  - `queries/*.sql` — the three reference dashboard queries: tenant monthly cost, first-try rate, top-budget tickets.
  - `README.md` — wiring guide + span / attribute reference + multi-tenant notes.

### Changed

- **`Dockerfile` and `docker-compose.yml` are now production-shaped (Phase 6).** The previous dev-shell `Dockerfile` (Node 22 + Rust + Claude Code, bind-mounted repo, `--dangerously-skip-permissions` entrypoint) moves verbatim to `Dockerfile.dev`. The previous `docker-compose.yml` (two services, both built from source) moves verbatim to `docker-compose.dev.yml` and its `build.dockerfile` value is updated to `Dockerfile.dev`. Local dev workflows continue to work — just pass `-f docker-compose.dev.yml` to compose commands.
- `TurnOutcome::Completed` now carries a `usage: TokenUsage` field so the runner aggregates session totals without re-parsing the event channel. All four implementers (`turn.rs`, `cli.rs`, `opencode.rs`) emit the same field they were already passing into `AgentEvent::TurnCompleted`.
- `Orchestrator::dispatch_one` returns a `DispatchOutcome::{Dispatched, Skipped, NoSlot}` enum instead of a boolean. `retries::tick_retries` uses the new `continue_loop()` helper to preserve its existing "no slot → requeue" semantics.
- `AppState::with_default_budget(...)` constructor added on the bridge side for tests / fixtures — production wires the `BudgetManager` explicitly so the embedded cost table can be overridden via `bridge.cost_table_path`.

### Deferred to v0.3.1

The 9-instrument OTel metrics layer (`sinfonia.agent.tokens_total`, `bridge.ci_outcome`, etc.) is deferred. The reference dashboard SQL in `examples/telemetry/queries/*.sql` reads from span attributes via the `events` table, not from OTel metric points, so the dashboards work span-derived. Filing this here so a future maintainer knows the metrics layer was a deliberate scope cut, not an oversight. The bridge `--once` single-shot mode described as one of the Topology 4 options in `docs/DEPLOYMENT.md` is also a v0.3.1 candidate — as of v0.3.0 the bridge always runs as a server. The four-topology guide in `docs/DEPLOYMENT.md` documents a working v0.3.0 alternative (POST to the existing `/webhook` handler from within the Action, then kill the bridge).

### Migration

- See [`docs/MIGRATION-v0.2-to-v0.3.md`](docs/MIGRATION-v0.2-to-v0.3.md).

## [0.3.0-alpha.1] — 2026-05-21

First v0.3 preview. Adds the `sinfonia-bridge` binary alongside the existing daemon; the daemon's behaviour is unchanged.

### Added

- **Workspace conversion.** The single-crate layout is now a Cargo workspace with three members:
  - `crates/sinfonia/` — the daemon (unchanged in behaviour).
  - `crates/sinfonia-tracker/` — the shared `IssueTracker` trait, Linear and Jira adapters, and the new `custom_fields` module.
  - `crates/sinfonia-bridge/` — the new bridge binary.
- **Custom-field plumbing** (`sinfonia-tracker::custom_fields`):
  - `CustomFieldValue` enum (`Null` / `Number` / `String`) with hand-written `Serialize` so values flatten to JSON primitives in the Liquid template scope.
  - `MARKER = "sinfonia_bridge_state_v1"` sentinel for the bridge's per-ticket envelope (`docs/SPEC.md` §11.6).
  - `WELL_KNOWN_FIELDS` registry consumed by `crates/sinfonia/src/template.rs` to pre-seed missing keys as `Null`, so templates using `{{ issue.fields.X | default: "…" }}` no longer trip strict-mode "Unknown index" errors.
  - `IssueTracker` gains five bridge-write methods: `ensure_custom_field`, `write_custom_field`, `transition_to_state`, `add_comment`, `apply_labels`. Linear implementations land in this release; Jira returns `NotImplemented` until a later milestone.
- **`Issue.fields`** map populated by the Linear adapter from the bridge's marker comment (single GraphQL hop via `comments(first: 100)`).
- **New `sinfonia-bridge` binary** (`crates/sinfonia-bridge/`):
  - `BRIDGE.md` config file (YAML front matter, mirrors `WORKFLOW.md` style) with a strict parser, nine validation rules, and a `--check` flag for config-only verification.
  - `POST /webhook` endpoint with HMAC-SHA256 signature verification (constant-time compare), SQLite-backed delivery-ID idempotency, and dispatch on `pull_request` / `check_suite` / `workflow_run`.
  - Feedback-loop orchestrator (`feedback::evaluate_ci`): categorizes failed checks, increments per-ticket attempt counters, routes to category-specific "needs fixes" states, applies the attempt cap, and posts a Liquid-rendered failure comment to the PR.
  - PR label management (`labels::LabelManager`): six canonical labels under a configurable prefix, with verbatim-alias semantics for installs that already have a competing label scheme.
  - GitHub authentication via either Personal Access Token or GitHub App (per-owner installation-scoped client cache); both modes exercised by integration tests.
  - `sinfonia-bridge --self-test` install gate: serial `PASS` / `FAIL` / `SKIP` lines per check, exit code = number of `FAIL` lines.
- **Tests.** The bridge crate ships 89 unit tests (config validation, webhook verify, storage, feedback loop, labels, GitHub auth, self-test, config round-trips) plus 9 `wiremock`-backed integration tests in `tests/bridge_e2e.rs` covering all nine scenarios from the Phase 1 plan §9.2 end-to-end. Workspace test count: 149 passing.
- **New docs.**
  - `BRIDGE.example.md` at the repo root — fully-commented working config, validated by `sinfonia-bridge BRIDGE.example.md --check` with no environment variables set.
  - `docs/SPEC.md` §11.6 — draft of the recommended bridge-service extension contract.

### Changed

- `LinearTracker::new` / `JiraTracker::new` now take a `&TrackerConfig` instead of `&ServiceConfig`. Existing callers go through `crates/sinfonia/src/tracker.rs`, so no migration is needed.
- `sinfonia::Error` gains a `Tracker` variant (`#[from] sinfonia_tracker::Error`); direct constructors of formerly-bare variants in `crates/sinfonia/src/config/typed.rs` now route through the wrap.

### Known limitations

- Phase 1 supports Linear only on the bridge side. `tracker.kind: jira` in `BRIDGE.md` is rejected at startup with a friendly "deferred to a later milestone" message.
- Budget caps (`max_tokens_per_ticket`, `max_cost_per_ticket_usd`) and the `telemetry.otlp_*` fields are accepted by the parser but unused in this release — they are scoped to a later milestone.
- The bridge does not hot-reload `BRIDGE.md`; configuration changes require a process restart.
- Linear marker comments are fetched via `comments(first: 100)`; tickets with more than 100 bot interactions may scroll the marker out of the window. See `docs/SPEC.md` §11.6.7 for RECOMMENDED mitigations.

## [0.1.0] — 2026-05-16

Initial public release.

### Added

- Rust implementation of the Symphony Service Specification (Draft v1, `docs/SPEC.md`):
  - `WORKFLOW.md` loader with YAML front matter + Liquid prompt body and `$VAR` resolution.
  - Single-authority orchestrator with poll loop, dispatch, reconciliation, exponential retries, continuation retries, and stall detection.
  - Per-issue workspace manager with sanitized identifiers, lifecycle hooks (`after_create`, `before_run`, `after_run`, `before_remove`), and root-containment safety invariants.
  - Strict prompt templating with `issue` + `attempt` variables.
  - Structured logs with `issue_id` / `issue_identifier` / `session_id` context.
  - Dynamic `WORKFLOW.md` reload via filesystem watcher.
- Issue tracker adapters:
  - **Linear** (GraphQL, paginated, blocker normalization from `inverseRelations`).
  - **Jira** (Cloud + self-hosted, REST + JQL, Basic-or-Bearer auth, "is blocked by" link normalization).
- Coding-agent backends:
  - **Raw LLM** with built-in tool loop (`shell`, `read_file`, `write_file`, `edit_file`, `list_dir`, `finish`) targeting OpenAI, Anthropic, Google Gemini, and locally hosted Ollama.
  - **CLI subprocess** drivers for Anthropic's `claude` (Claude Code) and OpenAI's `codex` (Codex CLI), with session resume via `--resume` / `--thread`.
- Configurable per-state runner overrides (`states:` block in `WORKFLOW.md`). Each tracker state can route to a different provider, model, command, prompt, temperature, and turn timeout.
- Optional HTTP server (axum): dashboard at `/`, JSON API at `/api/v1/state`, `/api/v1/<issue_identifier>`, `POST /api/v1/refresh`. Loopback bind by default.
- CLI: positional `WORKFLOW.md`, `--port`, `--log-format pretty|json`.

### Known limitations

- Retry queue and session metadata are in-memory only and do not survive process restart (per spec §14.3).
- The `linear_graphql` client-side tool is wired on the tracker trait but not exposed in the agent tool catalog yet.
- The Codex app-server stdio protocol backend is stubbed; this release targets the `codex exec` CLI surface instead.
- One project per running daemon. Multi-project deployments use one daemon per project.

[Unreleased]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.8...HEAD
[0.3.0-alpha.8]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.7...v0.3.0-alpha.8
[0.3.0-alpha.7]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.6...v0.3.0-alpha.7
[0.3.0-alpha.6]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.5...v0.3.0-alpha.6
[0.3.0-alpha.5]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.4...v0.3.0-alpha.5
[0.3.0-alpha.4]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.3...v0.3.0-alpha.4
[0.3.0-alpha.3]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.2...v0.3.0-alpha.3
[0.3.0-alpha.2]: https://github.com/O-Side-Systems/sinfonia/compare/v0.3.0-alpha.1...v0.3.0-alpha.2
[0.3.0-alpha.1]: https://github.com/O-Side-Systems/sinfonia/compare/v0.1.0...v0.3.0-alpha.1
[0.1.0]: https://github.com/O-Side-Systems/sinfonia/releases/tag/v0.1.0
