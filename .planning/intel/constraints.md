# Constraints Intel

Synthesized from SPEC-type docs. These describe the **current / intended system contract**
(the floor downstream plans must not violate). Two SPEC-type docs:
- docs/SPEC.md — Symphony Service Specification (orchestrator + bridge contract; consumer side)
- docs/HARNESS-SPEC.md — Harness Authoring Specification (producer side of the feedback contract)

Both are "Draft v1" / "Recommended pattern" status (`locked: false`), but they are the
authoritative technical contract for the system as designed.

---

## From docs/SPEC.md (Symphony Service Specification)

### CON-spec-orchestrator-readonly
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §11.5, §11.6.1
- **type:** protocol
- **content:** The orchestrator MUST NOT write to the issue tracker and MUST NOT mutate `sinfonia_*`
  fields. It is a scheduler / runner / tracker *reader* only. Tracker writes are performed by the
  coding agent (via its tools) or by the companion bridge service. The bridge is the only writer of
  the `sinfonia_*` namespace.

### CON-spec-workflow-contract
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §5, §6
- **type:** schema
- **content:** Runtime behavior is loaded from a repo-owned `WORKFLOW.md` (YAML front matter + Markdown
  prompt body). Top-level config keys: `tracker`, `polling`, `workspace`, `hooks`, `agent`, `codex`.
  Dynamic reload is REQUIRED (detect changes, re-apply without restart, keep last-known-good on invalid
  reload). Defaults per §6.4 cheat sheet (e.g. `polling.interval_ms=30000`, `agent.max_concurrent_agents=10`,
  `agent.max_turns=20`, `agent.max_retry_backoff_ms=300000`).

### CON-spec-prompt-rendering-strict
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §5.4, §12.2
- **type:** protocol
- **content:** Prompt template uses strict (Liquid-compatible) rendering: unknown variables and unknown
  filters MUST fail rendering. Well-known `sinfonia_*` keys are pre-seeded with `Null` (§11.6.4) so
  `| default:` guards never raise strict-mode "Unknown index".

### CON-spec-orchestration-state-machine
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §7, §8
- **type:** protocol
- **content:** Single authoritative in-memory orchestrator state mutates all scheduling. Claim states:
  Unclaimed / Claimed / Running / RetryQueued / Released. `claimed` + `running` checks REQUIRED before
  launching any worker; reconciliation runs before dispatch every tick. Continuation retry ~1000ms after
  clean exit; failure backoff `min(10000 * 2^(attempt-1), max_retry_backoff_ms)`.

### CON-spec-candidate-eligibility-and-blocker-rule
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §8.2
- **type:** protocol
- **content:** An issue is dispatch-eligible only with id/identifier/title/state, state in `active_states`
  and not terminal, not already running/claimed, global + per-state slots available. Blocker rule applies
  **only to `Todo`** state: do not dispatch when any blocker is non-terminal. (Note: `In Progress` issues
  ignore blockers — this is the gap REQ-blocker-merged-guardrail / REQ-dependency-modeling-invariant target.)

### CON-spec-workspace-safety-invariants
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §9.5
- **type:** protocol
- **content:** Invariant 1: run the coding agent only with `cwd == workspace_path`. Invariant 2:
  workspace_path MUST stay inside (prefix of) workspace_root. Invariant 3: workspace key sanitized to
  `[A-Za-z0-9._-]`, all other chars → `_`.

### CON-spec-codex-agent-protocol
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §10
- **type:** protocol
- **content:** Agent runner launches `codex.command` (default `codex app-server`) via `bash -lc` in the
  workspace; the targeted Codex app-server protocol is source of truth for message shape/transport.
  `session_id = "<thread_id>-<turn_id>"`; reuse thread_id across continuation turns. Optional client-side
  tool `linear_graphql` (one operation per call, reuse configured Linear auth).

### CON-spec-linear-tracker-contract
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §11.1–§11.4
- **type:** api-contract
- **content:** Required tracker ops: `fetch_candidate_issues`, `fetch_issues_by_states`,
  `fetch_issue_states_by_ids`. Linear: GraphQL `https://api.linear.app/graphql`, auth in `Authorization`
  header, `project_slug` → `slugId`, pagination required (page size 50, 30s timeout). `blocked_by` derived
  from inverse `blocks` relations; labels lowercased; priority integer-only.

### CON-spec-bridge-envelope
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §11.6.2, §11.6.3, §11.6.4
- **type:** schema
- **content:** Bridge per-ticket state lives in a single versioned envelope `sinfonia_bridge_state_v1`.
  Field shapes serialize as raw JSON null/number/string (NOT tagged variants). Monetary values stringified
  to preserve precision. Linear stores envelope as a single bot-owned comment (rewrite whole comment atomically);
  Jira stores as real custom fields resolved via display name. Well-known set v0.3: `sinfonia_attempt_count`,
  `sinfonia_last_ci_failure`, `sinfonia_failure_category`, `sinfonia_max_attempts`, `sinfonia_tokens_consumed`,
  `sinfonia_cost_consumed_usd`, `sinfonia_max_cost_usd`.

### CON-spec-bridge-webhook-and-auth
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §11.6.5, §11.6.6, §11.6.9
- **type:** api-contract
- **content:** Bridge accepts GitHub `pull_request` (opened/synchronize/reopened/closed), `check_suite.completed`,
  `workflow_run.completed`. MUST verify HMAC-SHA256 with constant-time compare; MUST dedupe `X-GitHub-Delivery`
  durably. JSON response contract (200 duplicate/ignored, 202 queued, 401 error). GitHub auth: PAT or App mode,
  mutually exclusive, exactly one configured at startup.

### CON-spec-bridge-events-and-budget
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §11.6.11, §11.6.12
- **type:** api-contract
- **content:** OPTIONAL typed event channel: daemon POSTs `runner.session.completed` (version 1) to subscribers,
  HMAC-SHA256 signed in `X-Sinfonia-Signature-256`; unknown event types ACK 200 ignored. OPTIONAL per-ticket
  budget caps from `feedback_loop.max_tokens_per_ticket` / `max_cost_per_ticket_usd`; cost from a versioned
  cost table (warn >90d stale, refuse cost caps >180d stale; token caps stay enforced).

### CON-spec-harness-manifest-consumed-shape
- **source:** /Users/brettlee/work/sinfonia/docs/SPEC.md §11.6.13, §12.5
- **type:** schema
- **content:** (Reflects Proposal 0001, OPTIONAL extension.) Consumed `bridge.json` at `schema_version: 2`:
  `run_url`, `artifact_bundle_name`, `failures[]` of `{scenario (required), feature_file?, step?, assertion?,
  artifact_urls?}`. `workflow_run`-keyed retrieval; version gate; degradation matrix (check-name path is the
  floor). Untrusted-input handling required. Digest folds into `sinfonia_last_ci_failure` as opaque scalar text.

---

## From docs/HARNESS-SPEC.md (Harness Authoring Specification — producer side)

### CON-harness-conformance-musts
- **source:** /Users/brettlee/work/sinfonia/docs/HARNESS-SPEC.md §3
- **type:** nfr
- **content:** A Sinfonia-ready harness MUST: author tests outside-in (§4.2); emit the four-artifact contract
  per scenario, pass or fail (§5.1–§5.2); express failures as structured `step`/`assertion` strings (§5.3);
  satisfy the determinism NFR (§5.4); assemble a `bridge.json` manifest at `schema_version 2` (§7.1); honor
  repository conventions (§7.3).

### CON-harness-four-artifact-contract
- **source:** /Users/brettlee/work/sinfonia/docs/HARNESS-SPEC.md §5.1, §5.2
- **type:** schema
- **content:** Every scenario MUST emit four artifacts unconditionally with stable role names: `result`
  (result.json), `trace` (trace.zip), `video` (video.webm), `a11y` (a11y.json), laid out one dir per scenario
  `runs/<run-id>/<scenario-slug>/`. `result.json` MUST carry `schema_version`, `scenario`, `passed`,
  `duration_s`, and structured `failed_step`/`assertion` on failure; bumps MUST be additive.

### CON-harness-determinism-nfr
- **source:** /Users/brettlee/work/sinfonia/docs/HARNESS-SPEC.md §5.4
- **type:** nfr
- **content:** For a fixed code state, the harness MUST return an identical pass/fail verdict across repeated
  runs (N-of-N identical, RECOMMENDED N≥20). Storage/environment-keyed values MUST NOT influence the verdict.

### CON-harness-bridge-json-producer-schema
- **source:** /Users/brettlee/work/sinfonia/docs/HARNESS-SPEC.md §7.1
- **type:** schema
- **content:** Producer-side `bridge.json` at `schema_version 2` (pinned to match consumer's
  `SUPPORTED_BRIDGE_MANIFEST_VERSIONS`): `schema_version`, `pr_number?`, `branch`, `commit_sha`, `run_url`,
  `artifact_bundle_name`, `failures[]` of `{scenario, feature_file?, step?, assertion?, artifact_urls?}`.
  Uploaded as artifact default-named `bridge-<run-id>` (matches consumer glob). Empty `failures` array when green.

### CON-harness-repository-conventions
- **source:** /Users/brettlee/work/sinfonia/docs/HARNESS-SPEC.md §7.2, §7.3, §7.4
- **type:** protocol
- **content:** CI check name MUST match the consumer's `failure_categories` routing pattern (reference:
  `(?i)(e2e|playwright|harness)` → `Needs Fixes - E2E`). Agent work lands on `sinfonia/<issue-id>` branches;
  PR body MUST contain a tracker-identifier line matching `pr_link_pattern` (default `Resolves <ID>`);
  `sinfonia:*` labels are bridge-owned; CODEOWNERS human-review gate MUST cover Sinfonia-touched paths (agent
  MUST NOT self-merge); harness + invariant gates MUST block merge on failure. Green CI is necessary but not
  sufficient — the CODEOWNERS human gate is terminal authority.

### CON-harness-observability-feedback
- **source:** /Users/brettlee/work/sinfonia/docs/HARNESS-SPEC.md §6
- **type:** nfr
- **content:** (RECOMMENDED) Per-workspace isolated observability stack; telemetry tagged with a workspace-id
  resource attribute; standard vendor-neutral query endpoints surfaced in `result.json.obs_endpoints`. The
  *agent* consumes these endpoints, not the orchestrator. Isolation invariants: loopback bind only,
  deterministic port allocation from workspace id, workspace-scoped lockfile for endpoints/pids.
