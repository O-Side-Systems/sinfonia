# Proposal 0002 — Orchestrator Gating Ground Truth

- **Status:** Draft / Proposed
- **Author:** (orchestrator working group)
- **Date:** 2026-06-13
- **Affects:** `crates/sinfonia/src/orchestrator`, `crates/sinfonia-tracker`, `docs/SPEC.md §8.2`, `docs/HARNESS-SPEC.md §7.3`
- **Spec sections touched:** §8.2
- **Tracking milestone:** v0.4

---

## 1. Summary

Phase 1 verification establishes the **verified ground truth** for how the orchestrator gates
dispatch. Two gates exist inside a single predicate, `is_dispatch_eligible`
(`crates/sinfonia/src/orchestrator/dispatch.rs:6-69`):

1. **Blocker gate (intended mechanism):** keys on `issue.blocked_by`, derived solely from Linear
   `inverseRelations` of type `"blocks"`. Applies to `Todo` issues only. Opens when every blocker
   reaches a terminal state (today: state membership, not PR-merge-to-main).

2. **Parent-child hierarchy gate (CONFIRMED DEVIATION):** keys on `issue.children`
   (`dispatch.rs:36-48`), blocking a parent in any active state until every child is terminal.
   This is a live, production-active gate that the intended blocks-only invariant says should not
   exist. Flagged for the Phase 3 keep/remove decision.

**This proposal does not amend `docs/SPEC.md`, `docs/HARNESS-SPEC.md`, or DEC-003.** Those
amendments land in Phase 3 once the keep/remove decision for the hierarchy gate is ratified.

---

## 2. Ground Truth — Verified Predicate Table (§8.2)

All line numbers verified against source on 2026-06-13; paired with symbol names to survive small
code shifts.

### 2.1 Evaluation order in `is_dispatch_eligible` (`dispatch.rs:6-69`)

| Step | Lines | Guard / Symbol | Condition | Short-circuits on |
|------|-------|----------------|-----------|-------------------|
| 1 | 7–13 | Field-presence guard | `id`, `identifier`, `title`, `state` all non-empty | Any empty field |
| 2 | 14–27 | Active-and-not-terminal guard (`state_l`, `active`, `terminal`) | `state_l = issue.normalized_state()` (= `self.state.to_lowercase()`, `types.rs:69-71`); `active` = any `cfg.tracker.active_states` matches; `terminal` = any `terminal_states` matches; fail if `!active \|\| terminal` | Non-active or terminal |
| 3 | 36–48 | **Parent-child hierarchy gate** (loop on `issue.children`) | Empty child state → `false`; non-terminal child state → `false`; no children → passes through | Any non-terminal or unknown-state child |
| 4 | 50–67 | **Blocker gate** (loop on `issue.blocked_by`, **Todo only**) | `todo = active_states` contains "Todo" (case-insensitive) AND `state_l == "todo"`; only when `todo`: empty blocker state → `false`; non-terminal blocker state → `false` | Any non-terminal or unknown-state blocker (Todo issues only) |
| 5 | 68 | Fall-through | `return true` | n/a |

### 2.2 Which relations gate dispatch, in which states

| Gate | Relation source | Applies in states | Block condition | Opens when |
|------|-----------------|-------------------|-----------------|------------|
| **Parent-child hierarchy gate** (`dispatch.rs:36-48`) | `issue.children` — Linear `children(first: 100)` at `linear.rs:48`; Jira `subtasks` at `jira.rs:584,623` | **All active, non-terminal states** | Any child non-terminal OR state empty/unknown | Every child reaches a `terminal_states` value |
| **Blocker gate** (`dispatch.rs:50-67`) | `issue.blocked_by` — Linear `inverseRelations` where `type == "blocks"` at `linear.rs:527-538` | **`Todo` only** | Any blocker non-terminal OR state empty/unknown | Every blocker reaches a `terminal_states` value (**terminal state, NOT PR-merge-to-main**) |

### 2.3 Subtleties

- **Case-insensitivity:** `normalized_state()` (`types.rs:69-71`) and `.to_lowercase()` throughout.
  `[VERIFIED: dispatch.rs:14-24, 40, 59]`
- **Conservative block on unknown/empty state (both gates):** `return false` on empty child state
  (`dispatch.rs:41-44`) or empty blocker state (`dispatch.rs:59-62`). This is the safety floor
  Phase 3 must preserve.
- **Todo-only scoping:** `In Progress` (and any non-`Todo` active state) skips the blocker loop
  entirely via the `if todo {` guard at `dispatch.rs:57`. In Progress issues with open blockers
  are eligible for dispatch. `[VERIFIED: dispatch.rs:50-57]`
- **Config-driven states:** `cfg.tracker.active_states` / `terminal_states` — not hardcoded.
  `[VERIFIED: dispatch.rs:15-24, 29-34]`
- **Gate opens on terminal STATE, not PR-merge:** both gates check `terminal_lc` membership;
  nothing in `dispatch.rs` consults PR or merge status. `[VERIFIED: dispatch.rs:45, 63]`

### 2.4 Blocks-only derivation evidence

`crates/sinfonia-tracker/src/linear.rs:527-538`:

```rust
// Blockers come from `inverseRelations` where type == "blocks". §11.3.
let blocked_by = n
    .get("inverseRelations")
    ...
    .filter_map(|rel| {
        let kind = rel.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if kind != "blocks" {
            return None;  // linear.rs:536-538: drops every other relation type
        }
```

The explicit `if kind != "blocks" { return None; }` at `linear.rs:536-538` is the concrete code
backing for "dependency gating keys only on `blocks`" — **for the blocker gate**. The hierarchy
gate keys on `children` (not `inverseRelations`), which is the confirmed deviation.

Corroborating SPEC refs: `docs/SPEC.md:1177` and `:2432` both assert "`blocked_by` derived from
inverse relations where relation type is `blocks`."

---

## 3. The Dependency-Gating Invariant

The following invariant text is carried verbatim in both this ADR and in
`.planning/phases/01-orchestrator-gating-verification/01-VERIFICATION.md`.

> **Dependency-gating invariant (BLOCK-02).** Orchestrator dependency gating SHOULD key only on
> Linear `blocks` relations (not hierarchy, "related", or prose). Today, two gates exist in
> `is_dispatch_eligible` (`dispatch.rs:6-69`):
>
> 1. **Blocker gate (intended mechanism):** keys on `blocked_by`, derived solely from Linear
>    `inverseRelations` of `type == "blocks"` (`linear.rs:527-555`). Applies to `Todo` issues
>    only; opens when every blocker reaches a terminal *state*. **Confirmed delta vs intent:**
>    `Done` SHOULD be set by PR-merge-to-`main`, but the gate opens on terminal state, not merge.
>    (Amendment deferred to Phase 3.)
> 2. **Parent-child hierarchy gate (CONFIRMED DEVIATION):** keys on `children`
>    (`dispatch.rs:36-48`), blocking a parent in any active state until every child is terminal.
>    This is a second, live dependency gate keyed on hierarchy — which the blocks-only invariant
>    says should not exist. **Flagged for the keep/remove decision in Phase 3.** Removing it
>    requires touching BOTH the predicate (`dispatch.rs:36-48`) AND the fetch
>    (`linear.rs:48`, `jira.rs:584-606`).
>
> This phase records the evidence and scoped delta. It does not decide keep-vs-remove and does
> not amend DEC-003 or SPEC §8.2.

---

## 4. The Confirmed Deviation — Parent-Child Hierarchy Gate

The parent-child hierarchy gate (`dispatch.rs:36-48`) is live and production-active. The
`children` field is genuinely populated by both tracker backends:

- **Linear:** `children(first: 100)` GraphQL fragment at `linear.rs:48-50`, mapped to `ChildRef`
  at `linear.rs:559-580`.
- **Jira:** `fields.subtasks` at `jira.rs:584-606` (classic sub-tasks only; team-managed
  Epic→Story NOT covered — see code comment at `jira.rs:580-582`).

Five existing unit tests pin the gate behavior (`dispatch.rs:171-205`):
`parent_with_open_child_is_not_eligible`, `parent_with_all_terminal_children_is_eligible`,
`parent_with_mixed_children_is_not_eligible_until_all_terminal`,
`issue_with_no_children_passes_parent_gate`, `parent_gating_is_case_insensitive`.

**Why this is a deviation:** DEC-003 (`PROJECT.md`) establishes that dependency gating keys ONLY
on Linear `blocks` relations. The hierarchy gate keys on `children` — which is hierarchy, not
`blocks`. There is no SPEC §8.2 contract for this gate (§8.2 is silent on it).

**What removal requires (Phase 3 scope):**
1. Remove or condition the predicate: `crates/sinfonia/src/orchestrator/dispatch.rs:36-48`
   (the `for c in &issue.children { ... }` loop in `is_dispatch_eligible`)
2. Remove the `children` fetch: `crates/sinfonia-tracker/src/linear.rs:48`
   (`children(first: 100)` from `ISSUE_FRAGMENT`)
3. Remove the Jira children population: `crates/sinfonia-tracker/src/jira.rs:584,623`

**No keep/remove recommendation is made in this proposal.** That decision is deferred to Phase 3.

---

## 5. The §8.2 Delta for Phase 3

`docs/SPEC.md` §8.2 "Candidate Selection Rules" (lines 717–734) matches the code on the blocker
rule but diverges on two points:

| Aspect | SPEC §8.2 today | Code today | Phase 3 amendment |
|--------|-----------------|------------|-------------------|
| Blocker rule scope | `Todo` only (matches) | `Todo` only (`dispatch.rs:50-57`) | unchanged direction |
| Blocker "non-terminal" | terminal *state* | terminal *state* (`dispatch.rs:63`) | **change to PR-merge-to-`main`** (DEC-003: `Done` SHOULD be set by merge) |
| Parent-child hierarchy gate | **NOT documented** | LIVE (`dispatch.rs:36-48`) | **resolve** — §8.2 must either document or drop the gate (keep/remove decision) |

**§8.2 is silent on the parent-child gate** — a documentation gap this proposal surfaces.

Phase 3 work items:
1. Amend §8.2: gate-on-PR-merge-to-main (replaces gate-on-terminal-state for the blocker rule).
2. Resolve the hierarchy gate: §8.2 + code must be consistent (either both document it or both
   omit it).
3. Cross-reference from `docs/HARNESS-SPEC.md §7.3` once §8.2 is amended.

**`docs/SPEC.md §8.2` is NOT amended in this proposal.** The amendment lands in Phase 3.

### Criterion #4 baseline (confirmed by Wave 1 tests)

The following blocker-rule behavior is confirmed by executable pinning tests added in plan 01-01:

| Claim | Code evidence | Pinning test |
|-------|---------------|-------------|
| Blocker rule applies only to `Todo` | `dispatch.rs:50-57` | `todo_with_open_blocker_is_not_eligible` |
| `In Progress` ignores blockers | `dispatch.rs:57` | `in_progress_ignores_blockers` |
| Gate opens on terminal state, not PR-merge | `dispatch.rs:63` | Both Wave 1 tests implicitly confirm |

---

## 6. Cross-References

| Reference | Relevance |
|-----------|-----------|
| **BLOCK-02** (`REQUIREMENTS.md:85-96`) | The requirement this proposal partially satisfies (verification half); SPEC/HARNESS-SPEC amendment lands in Phase 3 |
| **DEC-003** (`PROJECT.md`) — dependency-gating-on-blocks | The decision this phase verifies against; left intact in this phase |
| **CON-candidate-eligibility** (`PROJECT.md`) | Current contract: "blocker rule applies only to `Todo`; `In Progress` ignores blockers; gate opens on terminal state, not PR-merge" — confirmed matching code |
| `.planning/phases/01-orchestrator-gating-verification/01-VERIFICATION.md` | Phase 1 findings doc (disk-only; same invariant + §8.2 ground truth as this ADR) |
| `docs/proposals/0001-harness-feedback-ingestion.md` | Numbering and format precedent for this ADR |
| `crates/sinfonia/src/orchestrator/dispatch.rs:6-69` | Primary source: `is_dispatch_eligible` predicate |
| `crates/sinfonia-tracker/src/linear.rs:527-538` | `blocked_by` derivation from `blocks` inverse relations |
| `crates/sinfonia-tracker/src/linear.rs:48` | `children` fetch (to be removed if hierarchy gate is dropped) |
| `crates/sinfonia-tracker/src/jira.rs:584,623` | Jira `children` population (same scope as linear.rs:48 if removed) |
