---
title: "sinfonia-bridge — CI Feedback Bridge"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "b26bc50"
derived_from: "docs/SPEC.md §11.6, docs/proposals/0001-harness-feedback-ingestion.md"
---

# sinfonia-bridge — CI Feedback Bridge

Companion daemon that closes the CI → fix loop: receives GitHub webhooks, parses
`bridge.json` (schema_version 2), applies failure categorization, and transitions
tickets back to a "needs fixes" state with bounded attempt + budget enforcement.

## Module Ownership

| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
| Webhook receipt + HMAC verify | `crates/sinfonia-bridge/src/webhook/` | @osidesys @leebrett | SPEC §11.6.5–§11.6.9 |
| bridge.json manifest parsing | `crates/sinfonia-bridge/src/feedback/manifest.rs` | @osidesys @leebrett | schema_version 2 §11.6.13 |
| Attempt + budget enforcement | `crates/sinfonia-bridge/src/feedback/budget.rs` | @osidesys @leebrett | Per-ticket caps §11.6.12 |
| Failure categorization | `crates/sinfonia-bridge/src/feedback/categorize.rs` | @osidesys @leebrett | Lane routing §11.6.11 |
| GitHub client + auth | `crates/sinfonia-bridge/src/github/` | @osidesys @leebrett | PAT XOR App auth |
| OTel spans | `crates/sinfonia-bridge/src/telemetry/` | @osidesys @leebrett | Six bridge spans |

## See also

- [`docs/SPEC.md §11.6`](../../docs/SPEC.md) — bridge service contract
- [`docs/proposals/0001-harness-feedback-ingestion.md`](../../docs/proposals/0001-harness-feedback-ingestion.md) — Proposal 0001
- [`../../AGENTS.md`](../../AGENTS.md) — root entry point
