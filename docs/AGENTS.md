---
title: "docs — Spec Docs + Proposals"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "ee13a82"
derived_from: "docs/SPEC.md, docs/HARNESS-SPEC.md, docs/CONTEXT-CONTRACT.md"
---

# docs — Spec Docs + Proposals

Normative specifications and design proposals for Sinfonia. This file is NOT the
agent entry point — root [`AGENTS.md`](../AGENTS.md) is. Read root first, then
follow links to this node only when the issue acceptance criteria reference docs/.

## Module Ownership

| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
| Orchestrator + bridge spec | `docs/SPEC.md` | @osidesys @leebrett | Symphony spec conformance §18 |
| Harness authoring spec | `docs/HARNESS-SPEC.md` | @osidesys @leebrett | Four-artifact contract §5 |
| Repository Context Contract | `docs/CONTEXT-CONTRACT.md` | @osidesys @leebrett | AGENTS.md doc-graph shape |
| Deployment guide | `docs/DEPLOYMENT.md` | @osidesys @leebrett | Four topologies, credentials |
| Client setup guide | `docs/CLIENT_SETUP.md` | @osidesys @leebrett | Enterprise trust + audit |
| Harness feedback proposal | `docs/proposals/0001-harness-feedback-ingestion.md` | @osidesys @leebrett | bridge.json v2 ingestion |
| Reliability-seams proposal | `docs/proposals/0003-feedback-loop-reliability-seams.md` | @osidesys @leebrett | Write contention · missed-webhook sweep · restart workspace-reset |
| Tool-surface security proposal | `docs/proposals/0004-agent-tool-surface-hardening.md` | @osidesys @leebrett | Env scoping · permission opt-in · dispatch allowlist · documented posture |
| Merge-coordinator proposal | `docs/proposals/0005-merge-coordinator.md` | @osidesys @leebrett | Bridge-hosted rebase→re-test→merge; tier-independent native-queue substitute |
| Architecture / feedback loops | `docs/ARCHITECTURE.md` | @osidesys @leebrett | Non-normative; mermaid feedback-loop + HITL diagrams |

## See also

- [`docs/CONTEXT-CONTRACT.md`](CONTEXT-CONTRACT.md) — doc-graph contract (this area)
- [`docs/HARNESS-SPEC.md`](HARNESS-SPEC.md) — harness sensor spec
- [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) — feedback-loop + human-in-the-loop diagrams (non-normative)
- [`../AGENTS.md`](../AGENTS.md) — root entry point
