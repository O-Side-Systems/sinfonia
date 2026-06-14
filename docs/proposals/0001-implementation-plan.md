# Implementation Plan — Proposal 0001 (Harness Feedback Ingestion)

- **Status:** Accepted (Closed — Phase 2, v0.4 milestone, 2026-06-13)
- **Companion:** [`0001-harness-feedback-ingestion.md`](./0001-harness-feedback-ingestion.md)
- **Scope constraint:** Changes are confined to **`sinfonia`**. The producer
  (BCF, or any conforming harness) is **not modified** — this plan consumes the
  `bridge.json` shape the reference producer already emits at `schema_version 2`.
- **Crates touched:** `crates/sinfonia-bridge` (all code changes), plus
  `docs/SPEC.md`, `BRIDGE.example.md`, `CHANGELOG.md` (docs).
- **Orchestrator (`crates/sinfonia`):** **unchanged.** Trust boundary §11.6.1
  preserved; no new GitHub credentials, no envelope migration, no new
  well-known field.

---

## 1. Verified starting point

Confirmed against the current tree (see Proposal 0001 §2 for the gap evidence):

| Element | Current state | File |
|---|---|---|
| `GhOps` trait | label/comment/check-summary/whoami only; **no artifact methods** | `crates/sinfonia-bridge/src/github/client.rs:64` |
| Event target extraction | pulls `(repo, head_sha, pr_numbers)`; **no `workflow_run.id`** | `crates/sinfonia-bridge/src/feedback/mod.rs:87` |
| Red-path failure text | hardcoded placeholder `"(log excerpt not yet fetched…)"` | `crates/sinfonia-bridge/src/feedback/mod.rs` (`evaluate_one_pr`) |
| Delivery field | `last_failure_log` → `sinfonia_last_ci_failure` **already exists & pre-seeded** | `config.rs:138`, SPEC §11.6.4 |
| Category routing | `failure_categories` regex **works; untouched by this plan** | `feedback/categorize.rs` |
| Deps | `octocrab 0.39` present (exposes Actions artifacts API); **no `zip` crate** | `crates/sinfonia-bridge/Cargo.toml` |

Everything else in this plan is net-new.

## 2. Principles

1. **Opt-in.** Gated by `feedback_loop.ingest_harness_manifest`, default `false`
   until the fixture suite (Task 7) is green, then `true`.
2. **Degrade, never fail.** Every miss (no event, no artifact, oversized,
   malformed, version-too-old) logs `warn` and falls back to today's check-name
   behavior. The current behavior is the floor.
3. **Trust-boundary-preserving.** All new I/O lives in the bridge, which already
   holds GitHub credentials. The orchestrator crate is not edited.
4. **No envelope/field churn.** The digest lands in the existing
   `sinfonia_last_ci_failure` string → no new well-known field, no `_v2`
   envelope, no coordinated orchestrator release (§11.6.4).
5. **Hostile input.** `bridge.json` may originate from a fork PR; it is parsed as
   untrusted data with size/count/length caps and no execution (Task 6).
6. **Each task is independently shippable** behind the default-off flag.

## 3. Task breakdown

Tasks are ordered by dependency. Each lands as its own atomic commit with tests.

### Task 1 — `GhOps`: GitHub Actions artifacts access

- **Files:** `crates/sinfonia-bridge/src/github/client.rs`,
  `crates/sinfonia-bridge/Cargo.toml`, the test mock (wherever `GhOps` is faked
  in `feedback` tests).
- **Change:** Add two trait methods:
  ```rust
  /// List artifacts for a completed workflow run.
  async fn list_run_artifacts(&self, repo: &str, run_id: u64)
      -> Result<Vec<ArtifactMeta>>;   // { id: u64, name: String, size_in_bytes: u64 }

  /// Download an artifact zip by id, capped at `max_bytes`. Returns the
  /// raw zip bytes. Errors (not Ok-empty) when the cap is exceeded so the
  /// caller can log + fall back distinctly from "empty artifact".
  async fn download_artifact(&self, repo: &str, artifact_id: u64, max_bytes: u64)
      -> Result<Vec<u8>>;
  ```
- **Impl:** `OctocrabGhOps` uses the Actions artifacts endpoints
  (`GET /repos/{repo}/actions/runs/{run_id}/artifacts`, then the artifact
  `archive_download_url`). Enforce `max_bytes` by checking `size_in_bytes`
  before download **and** bounding the streamed body.
- **Dep:** add `zip` (in-memory `ZipArchive` over `Cursor<Vec<u8>>`). Unzip is
  Task 4; the dep lands here with Task 1's plumbing.
- **Tests:** mock returns a synthetic artifact list; `download_artifact` honors
  `max_bytes` (returns `Err` over cap). No live network in tests.
- **Risk:** octocrab's artifact-download ergonomics may require a raw request via
  the underlying client rather than a typed helper. Low; isolated to one method.

### Task 2 — Extract `workflow_run.id`

- **Files:** `crates/sinfonia-bridge/src/feedback/mod.rs`.
- **Change:** Replace the `(Option<String>, Option<String>, Vec<u64>)` tuple from
  `extract_targets` with a small struct that also carries
  `run_id: Option<u64>`. `workflow_run` events populate it from
  `workflow_run.id`; `check_suite` events leave it `None`.
- **Tests:** extend the existing `extract_targets_*` unit tests —
  `workflow_run` yields `Some(id)`; `check_suite` yields `None`; missing field
  tolerated.
- **Risk:** none; pure parsing, fully unit-testable.

### Task 3 — Manifest model + version gate

- **Files:** new `crates/sinfonia-bridge/src/feedback/manifest.rs` (+ `mod`
  declaration).
- **Change:** serde structs for the consumed `bridge.json` shape (Proposal 0001
  §10): `schema_version`, `run_url`, `artifact_bundle_name`, `failures[]`
  (`scenario`, `feature_file?`, `step?`, `assertion?`, `artifact_urls?`).
  Add:
  ```rust
  pub const SUPPORTED_BRIDGE_MANIFEST_VERSIONS: &[u32] = &[2];
  ```
  `parse_manifest(bytes, max_failures) -> ManifestOutcome` where `ManifestOutcome`
  distinguishes `Ingest(Manifest)`, `IngestForward(Manifest)` (newer-additive,
  warn), and `Fallback(reason)` (too old / malformed / absent version). Caps
  `failures` at `max_failures_parsed`.
- **Tests:** parse a v2 fixture; `version 1` → `Fallback`; `version 3` →
  `IngestForward` with known fields; malformed JSON → `Fallback`; `failures`
  truncated at the cap.

### Task 4 — Fetch → unzip → parse pipeline

- **Files:** `crates/sinfonia-bridge/src/feedback/manifest.rs` (or a sibling
  `ingest.rs`).
- **Change:** `try_fetch_manifest(gh, repo, run_id, cfg) -> Option<Manifest>`:
  1. `list_run_artifacts`, pick first matching `harness_manifest_artifact_glob`.
  2. `download_artifact(.., max_artifact_bytes)`.
  3. In-memory `ZipArchive`; read entry `harness_manifest_filename`, enforcing a
     per-entry decompressed cap (zip-bomb defense).
  4. `parse_manifest` (Task 3); map non-ingest outcomes to `None` + `warn`.
- **Tests:** happy path (zip containing `bridge.json`); no matching artifact →
  `None`; oversized download → `None` + warn; zip-bomb entry → `None`; missing
  `bridge.json` entry → `None`.

### Task 5 — Digest builder + wiring

- **Files:** `crates/sinfonia-bridge/src/feedback/manifest.rs` (builder),
  `crates/sinfonia-bridge/src/feedback/mod.rs` (`evaluate_one_pr` wiring).
- **Change:**
  - `build_failure_digest(&Manifest, run_url, cfg) -> String` per Proposal 0001
    §4.5: scenario-boundary truncation at `max_failure_digest_bytes`,
    `…(N more scenarios truncated)` marker, omit `null` lines, strip control
    chars, Markdown-fence-safe.
  - In `evaluate_one_pr`: when `ingest_harness_manifest` is on and a `run_id` is
    present, call `try_fetch_manifest`; on `Some`, replace the placeholder
    `failure_log_excerpt` with the digest. On `None`, keep the existing
    placeholder/fallback string. Categorization and transition paths are
    unchanged.
- **Tests:** digest rendering golden; truncation marker fires; `null` fields
  omitted; **injection fixture** — a `bridge.json` whose `assertion` contains
  Liquid (`{{ 7*7 }}`) and Markdown — asserts the value is rendered literally
  (no evaluation, no comment-escape) because it enters the template as a scalar,
  not as template source.

### Task 6 — Config surface

- **Files:** `crates/sinfonia-bridge/src/config.rs`, `BRIDGE.example.md`.
- **Change:** add to `FeedbackLoopSection` (all optional, defaults per
  Proposal 0001 §6):
  `ingest_harness_manifest` (bool), `harness_manifest_artifact_glob`,
  `harness_manifest_filename`, `max_artifact_bytes`, `max_failures_parsed`,
  `max_failure_digest_bytes`. Parser + defaults; an omitted block = disabled =
  today's behavior. Document the keys in `BRIDGE.example.md`.
- **Tests:** defaults applied when omitted; explicit block parses; disabled flag
  short-circuits ingestion (verified via the Task 5 wiring test).

### Task 7 — Security & fixture suite, then flip default

- **Files:** test fixtures under `crates/sinfonia-bridge/` test tree.
- **Change:** consolidate the adversarial fixtures (oversized, zip-bomb,
  malformed, version mismatch, injection, fork-PR-shaped manifest) into a
  named suite. Once green, flip `ingest_harness_manifest` default to `true`.
- **Exit criterion:** the §5 security matrix below is fully covered.

### Task 8 — Spec & docs

- **Files:** `docs/SPEC.md`, `CHANGELOG.md`.
- **Change:**
  - New **§11.6.13 "Harness Manifest Ingestion (OPTIONAL)"** — documents the
    consumed shape, `workflow_run`-keyed retrieval, the version gate, and the
    degradation matrix; marked OPTIONAL / Recommended-extension.
  - **§11.6.2** amendment — `sinfonia_last_ci_failure` MAY carry a structured
    multi-scenario digest.
  - **§12** note — when ingestion is on, that field is the primary retry-turn
    diagnostic channel.
  - `CHANGELOG.md` entries per §6.

## 4. Dependency graph

```
Task 1 (artifacts in GhOps) ─┐
Task 2 (run_id extraction) ──┼─► Task 4 (fetch+unzip) ─► Task 5 (digest+wiring) ─► Task 7 (sec suite → flip default)
Task 3 (manifest+version) ───┘                              ▲
Task 6 (config) ───────────────────────────────────────────┘
Task 8 (spec+docs) ── lands last, references the shipped behavior
```

Tasks 1, 2, 3, 6 are mutually independent and can land in any order. Task 4
needs 1+3; Task 5 needs 4 (+6 for the flag); Task 7 needs 5; Task 8 last.

## 5. Security test matrix (Task 7 exit criterion)

| Threat | Control | Test |
|---|---|---|
| Oversized artifact (resource exhaustion) | `max_artifact_bytes` pre- and mid-download | `download_artifact` returns `Err` over cap; pipeline → `None` |
| Zip bomb | per-entry decompressed cap | crafted entry → `None` + warn |
| Manifest flooding (huge `failures[]`) | `max_failures_parsed` + `max_failure_digest_bytes` | truncation marker; bounded output length |
| Template injection (fork-controlled `step`/`assertion`) | value-not-template binding | Liquid/markdown in `assertion` rendered literally |
| Path/exec via artifact content | in-memory only; no FS writes; `artifact_urls` never fetched server-side | code review + no-FS assertion in pipeline test |
| Version drift | `SUPPORTED_BRIDGE_MANIFEST_VERSIONS` gate | old → fallback; newer → forward-ingest known fields |

## 6. Changelog & release plan

Entries land under `[Unreleased]` → `### Added` as each step merges (the work is
additive and opt-in; no breaking change). Planned wording:

- *Added — Bridge: optional ingestion of a harness `bridge.json` manifest.* When
  `feedback_loop.ingest_harness_manifest` is enabled, the bridge fetches the
  CI run's `bridge-*` artifact on a red `workflow_run`, parses the structured
  per-scenario failures (`scenario` / `feature_file` / `step` / `assertion`) and
  artifact references, and folds them into `sinfonia_last_ci_failure` for the
  retry prompt. Versioned against `bridge.json schema_version 2` with
  warn-and-degrade on mismatch; treated as untrusted input with size, count, and
  length caps. Disabled by default until *(release)*; no envelope migration, no
  orchestrator change.

Versioning: a **minor** addition within the v0.3 line. The custom-field envelope
stays `sinfonia_bridge_state_v1` (no field-shape change → no `_v2` migration per
§11.6.2). No `MIGRATION-*` doc needed; the `BRIDGE.example.md` diff plus the
CHANGELOG entry are sufficient operator guidance.

## 7. Out of scope (this plan)

- **Generic check-run log fetching** (the original `P1-F` TODO) — a different
  GitHub endpoint, only relevant as a fallback for repos without a manifest.
- **A dedicated `sinfonia_failure_artifacts` context key** — would expand the
  well-known set and require a coordinated orchestrator release (§11.6.4);
  deferred (Proposal 0001 §9.2).
- **`check_suite`-only run-id resolution** (Proposal 0001 §9.1) — v1 keys on
  `workflow_run`.
- **Non-GitHub CI adapters** — noted for the harness spec's neutrality goals,
  not built here.
- **Any change to the BCF repo** — explicitly prohibited for this work.
