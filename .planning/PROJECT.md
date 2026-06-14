# Project: Sinfonia

## What This Is

Sinfonia (codename "Symphony" in the spec) is a service that orchestrates coding
agents to get project work done. It is a long-running Rust daemon that reads work
from an issue tracker (Linear in the v0.3 line), creates an isolated per-issue
workspace, and runs a coding agent (Codex app-server) per issue. A companion
`sinfonia-bridge` service owns CI-result interpretation, attempt counters, and the
PR↔ticket mapping.

The system is a Cargo workspace (`crates/sinfonia`, `crates/sinfonia-bridge`,
`crates/sinfonia-tracker`), deployed via Docker / docker-compose, integrating with
Linear/Jira trackers and GitHub. The current shipped version line is **v0.3.0-alpha**.

This milestone (the first GSD-managed milestone, **v0.4**) closes out the harness
feedback-ingestion work (Proposal 0001, already reflected in the spec) and hardens
the agentic loop against three observed failure modes: ignored Linear dependencies,
merge conflicts, and duplicate/overlapping decomposition.

## Core Value

**Coding agents complete tracker work autonomously without merging broken, conflicting,
or duplicate code** — dependencies are respected, PRs land cleanly through a merge
queue, and the agent gets structured, scenario-level CI feedback when it fails.

## Milestone Scope (v0.4)

All 16 requirements across four themes:

- **Theme A — Harness feedback ingestion** (HARNESS-*): verify/close Proposal 0001's
  opt-in `bridge.json` ingestion path (already reflected in SPEC §11.6.13 / §12.5).
  Treated as explicit verification/closure, not net-new design.
- **Theme B — Dependency gating** (BLOCK-*): gate work on blocker PRs being *merged to
  main* (not merely terminal), keyed only on Linear `blocks` relations.
- **Theme C — Merge-conflict handling** (MERGE-*): pre-PR rebase, mergeability loop,
  merge queue + post-merge gate, conflict-phase concurrency policy.
- **Theme D — Decomposition + repository context graph** (CTXGRAPH-*): overlap checks,
  just-in-time read protocol, Repository Context Contract, invariant linters,
  decomposition consistency pass.

## Constraints (existing-system invariants — do NOT re-build)

These are the current/intended contract from `docs/SPEC.md` and `docs/HARNESS-SPEC.md`.
Downstream plans MUST NOT violate them. They are the baseline, not buildable scope.

| ID | Invariant | Source |
|----|-----------|--------|
| CON-orchestrator-readonly | Orchestrator MUST NOT write the tracker or mutate `sinfonia_*` fields; it is reader/scheduler only. Bridge is the sole writer of the `sinfonia_*` namespace. | SPEC §11.5, §11.6.1 |
| CON-workflow-contract | Runtime behavior loaded from repo-owned `WORKFLOW.md` (YAML front matter + Markdown prompt); dynamic reload REQUIRED with last-known-good on invalid reload; §6.4 defaults. | SPEC §5, §6 |
| CON-prompt-rendering-strict | Strict Liquid rendering: unknown variables/filters MUST fail rendering; `sinfonia_*` keys pre-seeded with `Null`. | SPEC §5.4, §12.2 |
| CON-state-machine | Single authoritative in-memory state machine (Unclaimed/Claimed/Running/RetryQueued/Released); reconciliation before dispatch every tick; backoff `min(10000*2^(attempt-1), max_retry_backoff_ms)`. | SPEC §7, §8 |
| CON-candidate-eligibility | §8.2 dispatch eligibility + blocker rule. **Current contract: blocker rule applies only to `Todo`; `In Progress` ignores blockers; gate opens on terminal state, not PR-merge.** (Theme B amends this.) | SPEC §8.2 |
| CON-workspace-safety | cwd == workspace_path; workspace_path stays inside workspace_root; workspace key sanitized to `[A-Za-z0-9._-]`. | SPEC §9.5 |
| CON-codex-protocol | Agent runner launches `codex app-server` via `bash -lc`; `session_id = "<thread_id>-<turn_id>"`; thread reuse across continuations. | SPEC §10 |
| CON-linear-contract | Required tracker ops; Linear GraphQL contract; `blocked_by` derived from inverse `blocks` relations; pagination required. | SPEC §11.1–§11.4 |
| CON-bridge-envelope | Bridge state in single versioned envelope `sinfonia_bridge_state_v1`; raw JSON shapes; v0.3 well-known field set. | SPEC §11.6.2–§11.6.4 |
| CON-bridge-webhook-auth | Bridge accepts `pull_request`/`check_suite.completed`/`workflow_run.completed`; HMAC-SHA256 constant-time verify; durable `X-GitHub-Delivery` dedupe; PAT XOR App auth. | SPEC §11.6.5–§11.6.9 |
| CON-bridge-events-budget | OPTIONAL typed event channel + per-ticket budget caps with versioned cost-table freshness gate. | SPEC §11.6.11, §11.6.12 |
| CON-harness-manifest-shape | Consumed `bridge.json` at `schema_version: 2`; `workflow_run`-keyed; version gate; degradation matrix (check-name path is floor); untrusted-input handling; digest folds into `sinfonia_last_ci_failure` as opaque scalar. | SPEC §11.6.13, §12.5 |
| CON-harness-conformance | Harness MUST author outside-in, emit four-artifact contract, structured failure strings, determinism NFR, `bridge.json` v2, repository conventions. | HARNESS-SPEC §3 |
| CON-harness-four-artifact | Four artifacts per scenario (`result`/`trace`/`video`/`a11y`); `result.json` required fields; additive bumps. | HARNESS-SPEC §5.1, §5.2 |
| CON-harness-determinism | N-of-N identical verdict for fixed code state (RECOMMENDED N≥20); environment-keyed values MUST NOT affect verdict. | HARNESS-SPEC §5.4 |
| CON-harness-producer-schema | Producer `bridge.json` v2 fields; artifact default-named `bridge-<run-id>`; empty `failures` when green. | HARNESS-SPEC §7.1 |
| CON-harness-repo-conventions | CI check-name routing; `sinfonia/<issue-id>` branches; PR `Resolves <ID>` line; `sinfonia:*` labels bridge-owned; CODEOWNERS human gate is terminal (agent MUST NOT self-merge). | HARNESS-SPEC §7.2–§7.4 |

## Key Decisions

| ID | Decision | Status | Notes |
|----|----------|--------|-------|
| DEC-001-integration-model | **GitHub native merge queue + serial foundational stories** is the integration model. | **LOCKED** (ratified for this milestone) | Resolves the open DEC-CANDIDATE-integration-model. Unblocks all Theme C (MERGE-*) work and the concurrency/decomposition policy. To be recorded as a one-line decision in `docs/SPEC.md` (or an ADR) during Phase 3. |
| DEC-002-milestone-version | Milestone version is **v0.4**. | LOCKED | Repo is mid v0.3.0-alpha; the forward work (Themes B/C/D) is the next minor line. Proposal 0001 closure is an additive `### Added` bump that lands within this milestone. |
| DEC-0001-harness-feedback-ingestion | Opt-in, degrade-gracefully `bridge.json` ingestion in `sinfonia-bridge` (`schema_version: 2`, version-gated, untrusted-input handling). | Reflected in SPEC §11.6.13/§12.5; treated as VALIDATED baseline; this milestone verifies/closes it. | Source: Proposal 0001 (Draft). Changes no orchestrator trust boundary; no new credentials. |
| DEC-003-dependency-gating-on-blocks | Dependency gating keys ONLY on Linear `blocks` relations (not hierarchy/"related"/prose); `Done` SHOULD be set by PR-merge-to-main. | Candidate (Theme B); amends SPEC §8.2. | The WORKFLOW.example.md `{% if issue.children %}` parent-child gating assumption is **UNVERIFIED** and must be confirmed against `crates/sinfonia/src/orchestrator/` (Phase 1) before dependent work. |
| DEC-004-context-graph-convention | Adopt hierarchical nearest-wins hyperlinked `AGENTS.md` doc-graph; reject autonomous self-learning doc generation in favor of reviewed surgical doc-diffs riding the code PR. | Candidate (Theme D). | Separate the sensor (HARNESS-SPEC) from the map (Repository Context Contract). |

## Baseline Status

The current `docs/SPEC.md` + `docs/HARNESS-SPEC.md` contract is treated as **Validated /
existing**. Buildable scope for this milestone = forward work (Themes B, C, D) plus the
Theme A verification/closure. Existing-system invariants above are the floor, not tasks.
