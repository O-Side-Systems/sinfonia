# Proposal 0001 — Harness Feedback Ingestion

- **Status:** Accepted (Closed — Phase 2, v0.4 milestone, 2026-06-13)
- **Author:** (harness-sync working group)
- **Date:** 2026-05-29
- **Affects:** `crates/sinfonia-bridge` (primary), `docs/SPEC.md` §11.6 / §12, `BRIDGE.example.md`
- **Spec sections touched:** §11.6.2 (envelope), §11.6.3 (field shapes), §11.6.5–§11.6.6 (webhook/response), §12 (prompt assembly)
- **Tracking milestone:** v0.3 bridge extension, post-`alpha.8`

> **Closure note (Phase 2, 2026-06-13):** All tasks (1–8) landed in
> `crates/sinfonia-bridge`. The implementation is fully verified end-to-end:
> `feedback/manifest.rs` owns the fetch + parse + digest pipeline;
> `HarnessManifestSection::default()` sets `ingest: true`;
> `tests/manifest_security.rs` closes the adversarial surface (12 tests, including
> `golden_snapshot_exact_field_rendering` and `ingestion_writes_no_files_to_disk`);
> `tests/bridge_e2e.rs` scenario_10 proves the end-to-end ingestion path.
> The full requirement→evidence map is in
> `.planning/phases/02-harness-manifest-ingestion-closure/02-CLOSURE.md`.

> This is the first of a planned pair. Proposal 0001 (this document) closes the
> feedback gap in Sinfonia's bridge. A companion effort extracts the producer
> side — the test harness pattern — into a tech-stack-neutral spec so any target
> repo can emit the contract this proposal consumes.

---

## 1. Summary

A conforming test harness (the reference producer is the BCF admin-UI harness)
emits, per CI run, a structured **`bridge.json`** failure manifest plus a
four-artifact bundle (`result.json` / `trace.zip` / `video.webm` / `a11y.json`)
per scenario. The manifest carries, for every failing scenario, the Gherkin
`feature_file`, a structured `step` and `assertion` string, and `artifact_urls`
pointing at the bundle.

**Sinfonia consumes none of it.** Today the bridge's red-CI path is built
entirely from GitHub Check Run *names* and a hardcoded placeholder. The
"diagnose a failure from artifacts without re-running locally" guarantee the
harness is designed to satisfy is dropped at the bridge boundary, so the inner
agent loop re-iterates against a comma-joined list of check names instead of the
structured diagnostics that already exist one HTTP call away.

This proposal adds an **optional, degrade-gracefully** ingestion path in the
bridge that fetches and parses `bridge.json`, threads its structured failures
into the existing `sinfonia_last_ci_failure` field and the retry prompt, and
gates on a declared contract version. It changes no orchestrator trust
boundaries and requires no harness changes beyond what the reference producer
already emits.

## 2. Problem Statement (verified)

The gap was confirmed against the code, not inferred:

**The bridge only ever sees check-run names.** `CheckRunSummary`
(`crates/sinfonia-bridge/src/github/client.rs:38`) is the complete surface the
feedback loop receives from GitHub:

```rust
pub struct CheckRunSummary {
    pub failed: Vec<String>,   // names of checks with a non-pass conclusion
    pub passed: Vec<String>,   // names of checks that passed
    pub any_pending: bool,
}
```

**The red-CI feedback is a placeholder.** In `feedback/mod.rs`
(`evaluate_one_pr`), the field that the spec (§11.6.2) advertises as
"…last 50 lines of the most-failed check…" is filled with literal placeholder
text, with an in-code admission that log fetching is unimplemented:

```rust
// Phase 1 limitation: we don't fetch check-run logs. The template
// sees a placeholder; ... P1-F scope — left as a follow-up alongside
// the rest of the budget/telemetry work in Phase 3.
let failure_log_excerpt = format!(
    "(log excerpt not yet fetched; see PR {pr_url}/checks for full logs)"
);
```

**Nothing references the harness artifacts.** A repository-wide search for
`bridge.json`, `result.json`, `artifact`, `four-artifact`, `feature_file`,
`trace.zip`, or `a11y` across `crates/` and `docs/` returns **zero** matches.

**Consequence.** The one interface point that *does* connect is check-name →
category routing (`failure_categories[].check_pattern`, e.g.
`(?i)(e2e|playwright|harness)` → `Needs Fixes - E2E`). That correctly *routes*
the failure to a state, but the *content* handed to the agent on retry — the
`sinfonia_last_ci_failure` field and the rendered failure comment — contains no
scenario, no failing step, no assertion, and no artifact reference. The agent is
told *that* the e2e suite failed and *nothing about why*.

## 3. Goals and Non-Goals

### 3.1 Goals

1. Make `sinfonia_last_ci_failure` carry **structured, scenario-level
   diagnostics** sourced from `bridge.json` when present.
2. Surface **artifact references** (`artifact_urls`) to the inner agent loop so
   it can pull `trace.zip` / `a11y.json` on demand.
3. **Version the harness contract** the bridge accepts, with a warn/fall-back
   gate on mismatch — mirroring the existing cost-table freshness gate
   (SPEC §11.6.12).
4. Keep the whole path **optional**: a repo that emits no `bridge.json` behaves
   exactly as today.

### 3.2 Non-Goals

- **No new orchestrator GitHub credentials.** The trust boundary in §11.6.1
  stands: only the bridge talks to GitHub. (See §5.)
- **No harness/producer changes.** This proposal is the consumer half only; the
  producer contract is the companion proposal's concern.
- **No server-side artifact rendering.** The bridge does not download
  `trace.zip` / `video.webm`; it passes their *references* to the agent. Only
  the small `bridge.json` manifest is fetched and parsed.
- **No change to category routing.** §11.6 failure categorization is unchanged;
  this enriches the *payload*, not the *routing*.
- **No bridge.json schema authorship here.** This document pins the *consumed*
  shape at `schema_version: 2` (the reference producer's current version); the
  normative producer schema is defined by the companion spec.

## 4. Design

### 4.1 Where it runs: the bridge, keyed on `workflow_run`

Artifact retrieval requires GitHub credentials, which only the bridge holds
(§11.6.1). The bridge already receives `workflow_run` *and* `check_suite`
`completed` events (§11.6.5). The GitHub **Actions artifacts** API is keyed by
*workflow run id*, which is present on the `workflow_run` payload
(`workflow_run.id`) but not directly on `check_suite`. Therefore:

- **`workflow_run.completed` is the ingestion trigger.** When the run is red and
  the PR is mapped, the bridge resolves the run's artifacts.
- `check_suite.completed` continues to drive routing as today; when only a
  `check_suite` event is available (no resolvable run id), the bridge falls back
  to the current check-name behavior (§4.4). Operators wanting rich feedback
  ensure their CI emits `workflow_run` (the reference `pr-check.yml` does).

### 4.2 Fetch + parse pipeline

On a red `workflow_run` with a mapped ticket, the bridge:

1. `GET /repos/{repo}/actions/runs/{run_id}/artifacts`, finds the artifact whose
   name matches a configured glob (default `bridge-*`, see §6).
2. Downloads the artifact zip (a single `GET` to the `archive_download_url`),
   enforcing **`max_artifact_bytes`** (default 5 MiB) — abort and fall back if
   exceeded.
3. Extracts `bridge.json` *in memory* (no temp files), enforcing a per-entry
   decompressed-size cap to defend against zip bombs.
4. Validates `schema_version` against the supported set (§4.3).
5. Builds a **structured digest** (§4.5) and writes it to
   `sinfonia_last_ci_failure`; passes `artifact_urls` through to the prompt
   context (§4.6).

Any failure in steps 1–4 logs at `warn` and **falls back** to the current
check-name path. Ingestion is best-effort enrichment, never a hard dependency.

### 4.3 Contract versioning gate

A new bridge constant declares the accepted manifest versions:

```rust
/// bridge.json schema versions this bridge knows how to read.
pub const SUPPORTED_BRIDGE_MANIFEST_VERSIONS: &[u32] = &[2];
```

- `schema_version` in the supported set → ingest.
- `schema_version` newer than any supported → `warn` ("bridge.json
  schema_version {n} is newer than supported {max}; ingesting best-effort by
  known fields, unknown fields ignored") and ingest the known subset. Manifests
  are additive by convention, so forward-reading the known fields is safe.
- `schema_version` older than the minimum, or absent/unparseable → `warn` and
  fall back to check-name behavior.

This mirrors the precedent set by the cost-table freshness gate (warn-then-
degrade rather than hard-fail), keeping the bridge resilient to producer drift.

### 4.4 Graceful degradation matrix

| Condition | Behavior |
|---|---|
| No `workflow_run` event (only `check_suite`) | Current check-name path |
| No artifact matching the glob | Current check-name path |
| Artifact over `max_artifact_bytes` | `warn`; current check-name path |
| `bridge.json` missing/unparseable in the zip | `warn`; current check-name path |
| `schema_version` unsupported (too old) | `warn`; current check-name path |
| `schema_version` supported / newer-additive | **Structured digest path** |
| Green CI | Unchanged (apply `awaiting-review`, no tracker writes) |

The current behavior is the floor; ingestion is strictly additive.

### 4.5 Structured digest → `sinfonia_last_ci_failure`

`sinfonia_last_ci_failure` is a `String` (§11.6.3), already pre-seeded in the
prompt context (§11.6.4), so **no new well-known field is required** for the v1
of this work — a deliberate choice to avoid the packaging contract in §11.6.4
(a new key needs a coordinated orchestrator release). The bridge folds the
structured failures into that one string, budget-bounded:

```
e2e harness (@smoke gate): 2 scenario(s) failed (attempt 3/5)

1. "Create tenant persists across reload"
   feature: requirements/features/tenant/create-tenant.feature
   step:      Then the tenant list shows "Acme"
   assertion: Expected element [data-testid='tenant-row-acme'] to be visible;
              was not present in DOM
   artifacts: trace.zip · a11y.json · video.webm  (bundle: harness-runs-1820934)

2. "Tenant slug is server-validated"
   feature: requirements/features/tenant/create-tenant.feature
   step:      When I submit slug "ACME!"
   assertion: Expected 422 response; got 500

(diagnostics from bridge.json schema_version=2; full bundle at <run_url>)
```

Construction rules:

- Cap total length at **`max_failure_digest_bytes`** (default 8 KiB), truncating
  at a scenario boundary with an explicit `…(N more scenarios truncated)` marker
  rather than mid-string.
- Cap parsed scenarios at **`max_failures_parsed`** (default 20).
- `step` / `assertion` are emitted verbatim as the **structured strings** the
  producer guarantees — never re-interpreted, never parsed as templates (§5).
- When a field is absent (`null`), omit its line; never print `null`.
- `artifact_urls` render as bundle-relative names plus the bundle artifact name,
  not as fetched content.

### 4.6 Prompt context

The retry prompt already reads `issue.fields.sinfonia_last_ci_failure` (§12).
Because the digest lands in that existing field, **most prompts get richer
feedback with no template change.** Optionally, a follow-up may expose
`artifact_urls` as a distinct context key (`issue.fields.sinfonia_failure_artifacts`)
— but that *would* expand the well-known set and therefore requires a
coordinated orchestrator release per §11.6.4, so it is deferred out of v1 and
called out in §9.

## 5. Security Considerations

`bridge.json` originates from a CI run that may have been triggered by an
**untrusted fork PR**. It is treated as hostile input end to end:

- **Size bounds.** `max_artifact_bytes` on download; per-entry decompressed cap
  on unzip (zip-bomb defense); `max_failure_digest_bytes` and
  `max_failures_parsed` on parse.
- **No execution, no path use.** Parsed entirely in memory as data; the bridge
  never writes artifact content to disk, never executes it, and never resolves
  or fetches `artifact_urls` server-side (they are opaque reference strings
  handed downstream).
- **No template injection.** Failure text enters Liquid rendering as a *scalar
  value*, not as template source — consistent with how `failed_checks` is
  already bound in `render_failure_comment`. The bridge MUST NOT parse field
  content as a template. (Worth an explicit test, since the content is now
  attacker-influenced.)
- **Comment/field hygiene.** The digest is the only thing written to the
  bot-owned marker comment and the failure PR comment; both are already
  bot-authored surfaces. Control characters are stripped; the string is fenced
  when embedded in Markdown comments.
- **Trust boundary preserved.** All of the above runs in the bridge, which
  already holds GitHub credentials (§11.6.1). The orchestrator gains no new
  capability or secret.

## 6. Configuration Surface (`BRIDGE.md`)

New optional `feedback_loop` keys, all with safe defaults so existing configs
keep working unchanged:

```yaml
feedback_loop:
  # ... existing keys ...

  # Harness feedback ingestion (Proposal 0001). All optional.
  harness_manifest_artifact_glob: "bridge-*"   # which run artifact holds bridge.json
  harness_manifest_filename: "bridge.json"     # entry name inside the artifact zip
  max_artifact_bytes: 5_242_880                # 5 MiB download cap
  max_failures_parsed: 20                      # scenarios folded into the digest
  max_failure_digest_bytes: 8_192              # cap on sinfonia_last_ci_failure text
  ingest_harness_manifest: true                # master switch; false = today's behavior
```

When `ingest_harness_manifest` is absent or `false`, the bridge behaves exactly
as it does today (the §4.4 floor). No `WORKFLOW.md` change is required.

## 7. Spec Changes (`docs/SPEC.md`)

- **§11.6.2 / §11.6.3** — note that `sinfonia_last_ci_failure` MAY carry a
  structured multi-scenario digest sourced from a harness manifest, and define
  the digest as an opaque human/agent-readable `String` (no shape guarantees for
  consumers beyond "diagnostic text").
- **New §11.6.13 "Harness Manifest Ingestion (OPTIONAL)"** — document the
  `bridge.json` consumed shape at `schema_version: 2`, the
  `workflow_run`-keyed retrieval, the version gate, and the degradation matrix.
  Marked OPTIONAL and Recommended-extension, consistent with §11.6's status.
- **§12** — note that the failure field, when ingestion is enabled, is the
  primary diagnostic channel for the retry turn.

The producer-side normative schema is owned by the companion spec; §11.6.13
references it rather than re-specifying it.

## 8. Rollout & Changelog Plan

Staged so each step is independently shippable and reversible:

1. **Artifact-fetch foundation**: add GitHub Actions *artifacts* access to the
   `GhOps` trait and the `workflow_run.id` extraction, then the download +
   in-memory unzip plumbing behind `ingest_harness_manifest`, defaulted off.
   Ship dark. (Note: this is the **Actions artifacts** API — a *different*
   endpoint from check-run **logs**. Generic check-run log fetching, the
   original in-code `P1-F` TODO, is NOT a prerequisite for this work; it is only
   the fallback for repos that emit no harness manifest, and is out of scope
   here — see §3.2.)
2. **Digest + version gate**: build the structured digest and the
   `SUPPORTED_BRIDGE_MANIFEST_VERSIONS` gate; add the security tests (size caps,
   injection, fork-PR fixture). Flip the default on once the fixture suite is
   green.
3. **Spec + docs**: land §11.6.13, the §11.6.2 amendment, and the
   `BRIDGE.example.md` keys.

A task-level breakdown of these steps lives in
[`0001-implementation-plan.md`](./0001-implementation-plan.md).

Each merged step gets a Keep-a-Changelog entry under `[Unreleased]`. Because
this is an additive, opt-in bridge capability with no breaking change to the
custom-field envelope or the orchestrator, it is a **minor** bump within the
v0.3 line (`### Added`), not a new alpha of the publish pipeline. The envelope
stays `sinfonia_bridge_state_v1` (no field-shape change → no `_v2` migration per
§11.6.2). A one-paragraph note in `MIGRATION-*` is unnecessary; the
`BRIDGE.example.md` diff plus the CHANGELOG entry are sufficient operator
guidance.

## 9. Open Questions

1. **`check_suite`-only deployments.** Should the bridge resolve a workflow run
   id from a `check_suite` event (extra `GET /repos/{repo}/commits/{sha}/check-runs`
   → run linkage) to support CI that doesn't emit `workflow_run`? Deferred;
   reference CI emits `workflow_run`, so v1 keys on it and documents the
   requirement.
2. **Dedicated artifact context key.** Expose `artifact_urls` as
   `sinfonia_failure_artifacts` (expands the well-known set, needs a coordinated
   orchestrator release per §11.6.4) vs. keeping them folded into the digest
   string (no release coupling)? v1 folds; revisit if prompts need them
   separable.
3. **Producer discovery vs. convention.** Pin the artifact name by glob
   (this proposal) or have the harness advertise the artifact name in a check-run
   output/annotation the bridge reads first? Glob is simplest and matches the
   reference producer; revisit if producers diverge on naming.
4. **Non-GitHub CI.** The retrieval path is GitHub-Actions-specific. A future
   producer on other CI would need an analogous "fetch the manifest" adapter;
   out of scope here, noted for the companion spec's neutrality goals.

## 10. Appendix — Consumed `bridge.json` shape (schema_version 2)

For reference, the reference producer (`.github/workflows/pr-check.yml`) emits:

```json
{
  "schema_version": 2,
  "pr_number": 42,
  "branch": "sinfonia/eng-42",
  "commit_sha": "…",
  "linear_story_id": null,
  "run_url": "https://github.com/owner/name/actions/runs/1820934",
  "artifact_bundle_name": "harness-runs-1820934",
  "failures": [
    {
      "scenario": "Create tenant persists across reload",
      "feature_file": "requirements/features/tenant/create-tenant.feature",
      "step": "Then the tenant list shows \"Acme\"",
      "assertion": "Expected element [data-testid='tenant-row-acme'] to be visible; was not present in DOM",
      "artifact_urls": {
        "result": "<scenario-dir>/result.json",
        "trace":  "<scenario-dir>/trace.zip",
        "video":  "<scenario-dir>/video.webm",
        "a11y":   "<scenario-dir>/a11y.json"
      }
    }
  ]
}
```

The bridge reads `schema_version`, `run_url`, `artifact_bundle_name`, and
`failures[]` (`scenario`, `feature_file`, `step`, `assertion`, `artifact_urls`).
All other fields are ignored forward-compatibly.
