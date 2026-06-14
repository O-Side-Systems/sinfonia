---
title: "sinfonia-tracker — Linear/Jira Adapters"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "b26bc50"
derived_from: "docs/SPEC.md §8, §11.1–§11.4"
---

# sinfonia-tracker — Linear/Jira Adapters

Provides the `IssueTracker` trait and two concrete adapters (Linear via GraphQL,
Jira via REST Cloud/DC). Owns the `blocked_by` derivation logic (inverse `blocks`
relations) and the `sinfonia_*` custom-field read/write surface.

## Module Ownership

| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
| IssueTracker trait + shared types | `crates/sinfonia-tracker/src/types.rs` | @osidesys @leebrett | Canonical issue/state types |
| Linear GraphQL adapter | `crates/sinfonia-tracker/src/linear.rs` | @osidesys @leebrett | SPEC §11.1–§11.4, pagination required |
| Jira REST adapter (Cloud + DC) | `crates/sinfonia-tracker/src/jira.rs` | @osidesys @leebrett | Basic + Bearer auth |
| Jira ADF rendering | `crates/sinfonia-tracker/src/jira_adf.rs` | @osidesys @leebrett | Description format conversion |
| sinfonia_* custom fields | `crates/sinfonia-tracker/src/custom_fields.rs` | @osidesys @leebrett | Bridge-written namespace |
| Tracker config schema | `crates/sinfonia-tracker/src/config.rs` | @osidesys @leebrett | Coupled to WORKFLOW.md §5 |

## See also

- [`docs/SPEC.md §8, §11`](../../docs/SPEC.md) — dispatch eligibility + tracker contract
- [`../../AGENTS.md`](../../AGENTS.md) — root entry point
