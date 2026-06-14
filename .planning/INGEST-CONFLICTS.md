## Conflict Detection Report

Mode: new (fresh .planning bootstrap)
Precedence: ADR > SPEC > PRD > DOC
Docs synthesized: 11 (1 ADR-type [Draft], 2 SPEC, 8 DOC). No PRD-type. No LOCKED ADRs.
No UNKNOWN/low-confidence classifications.

### BLOCKERS (0)

None.

- No two LOCKED ADRs contradict (there are no LOCKED ADRs; Proposal 0001 is Draft/Proposed).
- No UNKNOWN-confidence-low docs requiring re-tag.
- No synthesis-breaking reference cycle (see INFO for the benign companion-doc citation).
- Mode is `new`, so no existing-CONTEXT.md locked-decision conflicts apply.

### WARNINGS (3)

[WARNING] Candidate milestone changes the §8.2 blocker-gating contract
  Found: docs/SPEC.md §8.2 — blocker rule applies ONLY to `Todo` state; `In Progress` issues ignore
         blockers; gate opens on terminal state (e.g. `Done`), not on PR-merge-to-main.
  Found: .planning/intel/ingest-sources/sinfonia-harness-action-plan.md (1.1, 2.4) +
         harness-improvement-analysis.md (Problem 1) — propose verifying blocker PRs are MERGED to
         `main` (not merely terminal) before work, modeling dependencies only as Linear `blocks`
         relations, and setting `Done` on PR-merge.
  Impact: This is a forward-looking delta to the current contract, NOT a same-scope contradiction to
          auto-resolve by precedence (the action-plan is a DOC, lower precedence than the SPEC, and is
          explicitly pre-implementation). Captured as candidate requirements REQ-blocker-merged-guardrail
          and REQ-dependency-modeling-invariant.
  → Roadmapper: scope these as an explicit milestone delta against docs/SPEC.md §8.2 / WORKFLOW.example.md,
    not as a silent merge into the current contract. Resolve action-plan item 0.1 (verify parent/child
    gating in src/orchestrator/) first — the WORKFLOW.example.md `{% if issue.children %}` assumption is
    flagged UNVERIFIED by the source.

[WARNING] Candidate milestone tightens HARNESS-SPEC §7.4 merge gating
  Found: docs/HARNESS-SPEC.md §7.4 — green CI necessary-not-sufficient; CODEOWNERS human gate is terminal;
         no merge-queue or post-merge harness-gate requirement.
  Found: sinfonia-harness-action-plan.md (2.2, 3.4) + harness-improvement-analysis.md (Problem 2) —
         add SHOULD/MUST language for a rebase-and-test merge queue and a post-merge harness gate on `main`.
  Impact: Additive tightening of an existing §, not a contradiction. Captured as REQ-merge-queue-and-post-merge-gate
          plus REQ-pre-pr-rebase / REQ-mergeability-loop (workflow-layer).
  → Roadmapper: treat as a HARNESS-SPEC §7.4 + §9 amendment milestone; depends on open integration-model
    decision (see below), not auto-applied.

[WARNING] Unratified integration-model decision blocks dependent scope
  Found: sinfonia-harness-action-plan.md (0.3) — "Decide the integration model: merge queue vs stacked
         PRs vs serial" is an OPEN decision; Phase 2 items branch on it. Default recommendation: GitHub
         native merge queue + serial foundational stories.
  Impact: Several candidate requirements (REQ-merge-queue-and-post-merge-gate, REQ-concurrency-policy-for-
          conflict-phases, REQ-decomposition-consistency-pass) depend on this choice. No ADR records it yet.
  → Roadmapper/User: ratify the integration model as an ADR (or one-line docs/SPEC.md decision) before
    sequencing the Problem-2 / Problem-3 work. Tracked as DEC-CANDIDATE-integration-model in decisions.md.

### INFO (2)

[INFO] Companion-doc citation cycle is benign (no synthesis loop)
  Note: docs/proposals/0001-harness-feedback-ingestion.md and docs/proposals/0001-implementation-plan.md
        cross-reference each other (proposal ↔ implementation plan). DFS finds a 2-node cycle in the raw
        cross_ref graph, but the two docs classify to different buckets (ADR-type requirements vs DOC-type
        execution context) and do not contradict — this is an ordinary bidirectional citation between
        companion docs, not a contradiction that would loop synthesis. Both were synthesized normally.
        No traversal depth-cap issue (graph well under the 50-depth cap).

[INFO] SPEC.md already incorporates Draft Proposal 0001's outcome
  Note: Proposal 0001 is Status "Draft / Proposed" (locked=false), yet docs/SPEC.md already contains its
        result as normative text: §11.6.2/§11.6.3 (digest-as-opaque-String amendment), §11.6.13 (Harness
        Manifest Ingestion, OPTIONAL), and §12.5 (failure-diagnostics channel). The proposal (ADR-type)
        and SPEC (SPEC-type) AGREE on the same scope, so there is no precedence conflict to resolve —
        recorded only for transparency that a "Draft" proposal's content is already reflected in the spec.
        Downstream should treat the §11.6.13 extension as OPTIONAL/Recommended (the floor is the existing
        check-name path), consistent with both docs.
