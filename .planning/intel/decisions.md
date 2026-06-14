# Decisions Intel

Synthesized from classified ADR-type docs. Precedence order: ADR > SPEC > PRD > DOC.

Only one doc classified as ADR-type in this ingest set: Proposal 0001. Its status is
**Draft / Proposed** (`locked: false`), so its decisions are *proposed*, not binding. They
are carried here as candidate decisions for the upcoming v0.3 bridge-extension milestone, not
as already-ratified architecture.

---

## DEC-0001-harness-feedback-ingestion — Opt-in harness `bridge.json` ingestion in the bridge

- **source:** /Users/brettlee/work/sinfonia/docs/proposals/0001-harness-feedback-ingestion.md
- **status:** PROPOSED (Draft) — `locked: false`
- **scope:** sinfonia-bridge feedback loop; `sinfonia_last_ci_failure` payload; retry prompt context
- **decision statement:** Add an optional, degrade-gracefully ingestion path in `sinfonia-bridge`
  that fetches and parses a CI harness `bridge.json` manifest (`schema_version: 2`), folds its
  structured per-scenario failures into the existing `sinfonia_last_ci_failure` string, and gates
  on a declared contract version. Keyed on the GitHub `workflow_run.completed` event (the Actions
  *artifacts* API is keyed by `workflow_run.id`). Changes no orchestrator trust boundary and
  requires no new orchestrator credentials.
- **sub-decisions:**
  - Ingestion trigger is `workflow_run.completed` (red + mapped PR); `check_suite`-only deployments
    fall back to the current check-name path.
  - Accepted-version gate via `SUPPORTED_BRIDGE_MANIFEST_VERSIONS = [2]`; newer → warn + best-effort
    forward-read of known fields; older/absent/unparseable → warn + check-name fallback. Mirrors the
    cost-table freshness gate (SPEC §11.6.12) warn-then-degrade precedent.
  - No new well-known custom field introduced — digest reuses `sinfonia_last_ci_failure` (String),
    so no coordinated orchestrator release is required.
  - `bridge.json` is treated as hostile (fork-PR) input: size caps, zip-bomb defense, in-memory parse,
    no `artifact_urls` server-side resolution, scalar (non-template) injection into rendering.
- **consequences:** Spec amendments §11.6.2/§11.6.3 (digest is opaque String), new §11.6.13 (consumed
  shape + retrieval + version gate + degradation matrix), §12 (failure field is primary retry
  diagnostic channel). Already reflected in the current SPEC.md text. Minor (`### Added`) bump within
  the v0.3 line; envelope stays `sinfonia_bridge_state_v1`.

---

## Candidate decision needing ratification (from action-plan, not yet an ADR)

## DEC-CANDIDATE-integration-model — Choose the integration model (merge queue vs stacked PRs vs serial)

- **source:** /Users/brettlee/work/sinfonia/.planning/intel/ingest-sources/sinfonia-harness-action-plan.md (Phase 0, item 0.3)
- **status:** OPEN — explicitly flagged "record a one-line decision in docs/SPEC.md or an ADR"
- **scope:** merge-conflict handling, branch protection, concurrency policy for the agentic loop
- **default recommendation in source:** GitHub native merge queue + serial foundational stories.
- **note:** This is forward-looking; surfaced here so the roadmapper can route it to an ADR decision
  before the merge-queue work (action-plan Phase 2) lands. Not locked.
