# Context Intel

Running notes from DOC-type docs, keyed by topic, appended with source attribution. These are
operator guides, runbooks, migration notes, and forward-looking analysis — background/context,
not normative requirements.

---

## Topic: Product identity & trust posture

- Sinfonia (codename Symphony in the spec) is a long-running daemon that reads work from an issue
  tracker (Linear in v0.3), creates an isolated per-issue workspace, and runs a coding agent
  (Codex app-server) per issue. Companion `sinfonia-bridge` owns CI-result interpretation, attempt
  counters, and PR↔ticket mapping. **source:** docs/SPEC.md §1; docs/CLIENT_SETUP.md
- Enterprise trust posture: orchestrator needs no GitHub credentials (bridge holds them); GitHub App
  vs PAT auth trade-offs; budget controls; audit trail via OpenTelemetry; CODEOWNERS interaction; a
  vendor-evaluation worksheet for security review. **source:** docs/CLIENT_SETUP.md

## Topic: Deployment & operations

- Four deployment topologies, credential model, GitHub webhooks + HMAC secrets, OpenTelemetry
  observability, scaling, backup/recovery, upgrade. **source:** docs/DEPLOYMENT.md
- v0.2 → v0.3 migration: required/optional/breaking changes; Docker images, docker-compose topology,
  state-machine pattern, OpenTelemetry, OpenCode backend; setup-skill changes. **source:** docs/MIGRATION-v0.2-to-v0.3.md

## Topic: Setup skills

- Six setup skills: setup-workflow, setup-bridge, setup-state-machine, setup-telemetry,
  setup-agent-backend, migrate-from-symphony. SKILL.md contract, recommended order, Strict-Liquid
  invariant, skill versioning. The `setup-bridge` skill uses the bridge's `--self-test` (SPEC §11.6.10)
  as its install gate. **source:** docs/SKILLS.md; docs/SPEC.md §11.6.10

## Topic: Jira custom-field binding

- Admin runbook to bind the seven `sinfonia_*` Jira custom fields to screens so they appear in the UI
  (Cloud + Server/DC). The bridge writes fields via REST regardless of UI visibility; binding is for
  human visibility. **source:** docs/JIRA-SCREEN-SCHEME.md; docs/SPEC.md §11.7.2

## Topic: Proposal 0001 implementation plan (engineering execution context)

- Task-by-task plan for the harness `bridge.json` ingestion work: add GitHub Actions *artifacts* access
  to the `GhOps` trait, extract `workflow_run.id`, build the bridge.json model + version gate, fetch/unzip/
  parse pipeline, failure-digest builder, `feedback_loop` config surface, security/adversarial fixture suite,
  `sinfonia_last_ci_failure` delivery. Classified DOC because it is an execution plan that defers the formal
  requirements to the companion Proposal 0001 (see decisions.md / requirements.md Theme A). Note: this doc
  and the proposal cross-reference each other (companion docs). **source:** docs/proposals/0001-implementation-plan.md
- Staged rollout: (1) artifact-fetch foundation shipped dark behind `ingest_harness_manifest`; (2) digest +
  version gate + security tests, flip default on once fixtures green; (3) spec + docs (§11.6.13, §11.6.2,
  BRIDGE.example keys). Minor `### Added` bump in v0.3 line. **source:** docs/proposals/0001-harness-feedback-ingestion.md §8

## Topic: Forward-looking improvement analysis (2026-06-13) — rationale for the upcoming milestone

These two docs are recent (2026-06-13) analysis/action-plan documents. Their proposed changes are captured
as **candidate requirements** in requirements.md (Themes B/C/D), not as shipped facts. Context/rationale here:

- The three observed problems live at three layers: (1) Linear dependencies ignored — orchestrator/tracker
  modeling; (2) merge conflicts — workflow loop + merge queue; (3) two stories building the same thing —
  decomposition + repo context graph. Only #3 is mostly a harness-docs problem. **source:** harness-improvement-analysis.md (TL;DR)
- Empirical backing cited: agent-PR merge-conflict rate ~26.9% (AgenticFlict); "Resolved by Another PR"
  ~22% of unmerged agent PRs (arXiv 2602.00164); serialize merges to avoid silent feature loss (Autonoma).
  **source:** harness-improvement-analysis.md (Problems 2 & 3)
- Doc-graph / "just enough context" design: adopt hierarchical nearest-wins hyperlinked AGENTS.md (don't
  invent a parallel convention); separate the sensor (HARNESS-SPEC) from the map (Repository Context Contract);
  reject autonomous "self-learning" doc generation (ETH study: LLM-written context files reduced task success
  in 5/8 settings) in favor of reviewed surgical doc-diffs riding the code PR. **source:** harness-improvement-analysis.md (doc-graph)
- Phased actionable backlog with file targets + done-checks: Phase 0 verify (orchestrator gating behavior),
  Phase 1 WORKFLOW.example.md changes, Phase 2 HARNESS-SPEC.md changes, Phase 3 new bootstrap templates
  (AGENTS.md, CODEOWNERS, CONTEXT-CONTRACT, integration-setup note), Phase 4 decomposition discipline.
  Suggested order: 1.1–1.3 first, then 2.2 + 3.3/3.4, then context graph, then linters + discipline.
  **source:** sinfonia-harness-action-plan.md (all phases + Suggested execution order)
- Phase 0 explicitly flags an UNVERIFIED assumption: WORKFLOW.example.md's `{% if issue.children %}` block
  asserts parent-child gating, but the README documents gating only on `blocks` relations. Must confirm in
  `src/orchestrator/` before trusting it. **source:** sinfonia-harness-action-plan.md (0.1); harness-improvement-analysis.md (Problem 1)
