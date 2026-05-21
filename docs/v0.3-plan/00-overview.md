# Sinfonia v0.3.0 — implementation plan index

**Status:** Draft (planning only — no implementation has started)
**Source proposal:** [`sinfonia-change-proposal.md`](../../sinfonia-change-proposal.md) (Brett, O'Side Systems, 2026-05-21)
**Scope:** Make Sinfonia deployable as a team-grade AI coding orchestrator for client engagements, not just a personal tool.

This directory contains seven implementation-plan documents (one per phase) and this index. Together they specify *what we're going to build, where it goes, how we'll know it works,* and *in what order*. None of the code has been written yet — that's deliberate. Plans get reviewed before implementation begins.

---

## Phase docs

| # | Phase | LOC est. (Rust + tests + docs) | Doc |
|---|---|---|---|
| 1 | `sinfonia-bridge` MVP (Linear only) | ~2 400 + 500 + 600 = **~3 500** | [01-bridge-mvp.md](01-bridge-mvp.md) |
| 2 | OpenCode agent backend | ~400 + 150 + 100 = **~650** | [02-opencode-backend.md](02-opencode-backend.md) |
| 3 | Telemetry + budget enforcement | ~1 100 + 300 + 500 = **~1 900** | [03-telemetry-budget.md](03-telemetry-budget.md) |
| 4 | Jira bridge support | ~200 + 150 + 150 = **~500** | [04-jira-bridge.md](04-jira-bridge.md) |
| 5 | Setup skills + new CLI flags | ~800 + 400 + 900 skill prose = **~2 100** | [05-skills-cli.md](05-skills-cli.md) |
| 6 | Docker images | ~200 Dockerfile + 150 CI + 200 docs = **~550** | [06-docker.md](06-docker.md) |
| 7 | Documentation update | **~3 000 Markdown** | [07-docs.md](07-docs.md) |

**Totals:** ~5 100 lines of Rust + ~1 500 lines of tests + ~5 450 lines of documentation/skill prose, six new Docker images, six new skill folders.

Numbers updated after the post-planning review pass: Phase 1 grew to absorb the custom-field-to-template plumbing (originally punted to Phase 5); Phase 3 grew to ~1 900 after replacing the dropped OTLP-receiver design with a typed HTTP event channel; Phase 5 grew to ~2 100 after reckoning with skill SKILL.md prose budget. Per-phase docs show the breakdown in their leading metadata block.

---

## Phase dependency graph

```
                ┌────────────────┐
                │ Phase 1: bridge│
                │   MVP (Linear) │
                └──┬──────┬──────┘
                   │      │
            ┌──────▼┐ ┌───▼──────┐
            │ P2:   │ │ P4: Jira │
            │ Open- │ │  bridge  │
            │ Code  │ └───┬──────┘
            └──┬────┘     │
               │          │
            ┌──▼──────────▼──┐
            │ P3: Telemetry  │
            │   + budget     │
            └──┬─────────────┘
               │
            ┌──▼────────────────┐
            │ P5: Skills + CLI  │
            └──┬────────────────┘
               │
            ┌──▼────────────────┐
            │ P6: Docker images │
            └──┬────────────────┘
               │
            ┌──▼────────────────┐
            │ P7: Documentation │
            └───────────────────┘
```

Concrete dependencies:

- **Phase 2 depends on Phase 1** because the workspace conversion in Phase 1 is what lets `opencode.rs` sit cleanly next to `cli.rs` in the new crate layout. Phase 2 could land before Phase 1 in a single-crate world, but the proposal commits us to the workspace structure.
- **Phase 4 depends on Phase 1** because Phase 1 lands the `IssueTracker` trait extension; Phase 4 fills in the Jira side of those methods.
- **Phase 3 depends on Phase 1 and Phase 2** because the budget enforcement needs the bridge's tracker write path AND needs every backend (including OpenCode) to emit the same `runner.session` span shape.
- **Phase 5 depends on Phase 1, 2, 3** because the skills reference real-world deployment configurations that only exist after those phases land.
- **Phase 6 depends on Phase 1, 2, 3** for the same reason — the `docker-compose.yml` example references the bridge, OpenCode, and the Collector + Postgres stack.
- **Phase 7 depends on every other phase** — docs that describe behavior that doesn't exist will mislead readers.

### Suggested execution order

Strictly serial: 1 → 2 → 3 → 4 → 5 → 6 → 7. Each phase ships a usable feature, so partial-completion of the v0.3 scope still gives something deployable.

If team capacity allows two parallel tracks, Phase 2 and Phase 4 can run in parallel after Phase 1 lands. They touch disjoint code paths (`crates/sinfonia/src/agent/opencode.rs` vs `crates/sinfonia-tracker/src/jira.rs`).

Phase 7 (docs) gets a *draft* stub from every phase as it lands — the spec extensions, the example YAML blocks, the README rewrites. The Phase 7 dedicated pass is the polish + cross-cutting consistency check, not from-scratch authoring.

---

## Cross-cutting concerns

Things that aren't owned by any one phase but affect every one. Tracked here so they don't fall through cracks.

### A. The workspace conversion is a load-bearing commit

Phase 1's first commit moves files. Logic doesn't change; paths do. Every subsequent phase depends on the new layout. This is the single highest-risk operation in the v0.3 plan because of the diff size and the rebase pain it creates for any in-flight branches.

Mitigation:

- Land Phase 1's workspace move on `new-features` before any v0.3 implementation work starts on side branches.
- The workspace-conversion commit changes paths only — use `git log --follow` and per-commit `cargo test` to verify logic is byte-identical.
- Announce ahead of time so anyone with pending Sinfonia work rebases before the cut.

### B. Custom-field semantics across trackers

Linear and Jira represent custom fields differently (comment marker payload vs real REST fields). The trait abstraction in `crates/sinfonia-tracker/src/custom_fields.rs` hides this from the bridge's business logic — but anyone debugging "where did this value get written?" needs to know about the per-tracker mapping.

Documented:
- In the SPEC §11.7 (Phase 7).
- In `crates/sinfonia-tracker/src/custom_fields.rs` module-level rustdoc.
- In the `setup-bridge` skill's narration (so users see it during setup, not while debugging).

### C. Cost-table currency

The cost table is in-repo, versioned, and overridable. It drifts. Mitigation:

- Phase 3 includes a `verified_at:` date and a startup warning if the table is more than 90 days stale.
- Phase 5's `setup-telemetry` skill prompts the user to verify the table at install time.
- Long term, a future v0.4+ enhancement could pull prices from provider APIs. Not in v0.3 scope.

### D. Tenant ID threading

The proposal's tenancy design adds `tenant_id` to every span and metric in Phase 3. But `tenant_id` is read from config at the same place as other config (Phase 1 already has a config loader). Decision: thread `tenant_id` through `ServiceConfig` from Phase 1, even though no code reads it until Phase 3. Avoids a Phase-3 plumbing patch across already-shipped code.

### E. Single-tenant per process for v0.3

The `tenant_id` attribute is per-process. Running multiple tenants in one Sinfonia process is **out of scope** for v0.3 — and a future `setup-bridge`-style skill for multi-tenant deployments is a v0.4+ idea. Per-tenant resource limits (per-tenant token caps, per-tenant attempt caps) are also v0.4+.

### F. Test data fixtures

Multiple phases need shared test fixtures: a fake Linear API response, a fake GitHub webhook payload, a fake Jira REST response. Default: one fixture directory at `tests/fixtures/` shared across phases. Don't proliferate per-phase fixture trees.

### G. CHANGELOG hygiene

Each phase appends to a `CHANGELOG.md` "Unreleased" section as it lands. Phase 7's final pass moves the Unreleased block under the `v0.3.0 — YYYY-MM-DD` header. This way we never lose change-log entries to a missed final pass.

---

## What's deliberately out of scope for v0.3

Recorded here so we don't accidentally drift into them mid-implementation. From the proposal "Resolved design decisions" and "Remaining open questions":

1. **Webhook-driven orchestration inside Sinfonia itself.** Polling stays. Webhook handling lives in the bridge.
2. **Replacing the raw LLM backends.** They remain valid for cheap-lane states (Triage, In Review).
3. **Multi-project per daemon.** One `WORKFLOW.md` = one project.
4. **Cross-restart durability of in-flight agent sessions.** Sinfonia spec §14.3 stays.
5. **Per-tenant resource limits in self-hosted mode.** v0.4+.
6. **Automated cost-table updates from provider APIs.** v0.4+.
7. **Failure-category extensibility via structured CI output.** v0.4+.
8. **Hot-reload of `BRIDGE.md`.** Process restart only in v0.3.
9. **Docs site / hosted docs.** Repo Markdown only.
10. **Translation / non-English docs.** v0.5+.

If a feature isn't in a phase deliverable checklist, it's out of v0.3. Surprises during implementation become a tagged v0.4 issue, not a scope expansion.

---

## Open cross-cutting questions

Compiled from the per-phase docs. Each one has a default answer in the phase doc; resolve before the relevant phase starts implementation.

1. **Tracker crate name** (Phase 1 §11.1). Default: `sinfonia-tracker`.
2. **Front-matter parser sharing** (Phase 1 §11.2). Default: small `sinfonia-frontmatter` crate.
3. **`octocrab` version pin** (Phase 1 §11.3). Default: `0.39`; re-verify at impl time.
4. **In-memory mapping cache vs SQLite-only** (Phase 1 §11.4). **RESOLVED:** SQLite-only.
5. **`opencode` CLI exact flag names** (Phase 2 §7.1, §7.2). Resolve via a ~30-minute spike at impl time.
6. **OTel version compatibility** (Phase 3 §10). Resolve at impl time; pick a mutually-compatible set.
7. **Bridge OTLP intake vs typed event channel** (Phase 3 §11.1). **RESOLVED:** typed JSON event channel from Sinfonia to bridge over the existing HTTP surface. OTLP receiver dropped.
8. **Token attribution at turn vs session level** (Phase 3 §11.2). Default: session-level.
9. **Markdown→ADF library vs hand-roll** (Phase 4 §3.5). Default: hand-roll.
10. **`inquire` vs `dialoguer` for `sinfonia init`** (Phase 5 §7.1). Default: `inquire`.
11. **Multi-arch image availability** (Phase 6 §8.1). Resolve per-image.
12. **GHCR vs Docker Hub** (Phase 6 §8.5). Default: GHCR-only for v0.3.
13. **Label aliases — full name vs prefix-applied** (Phase 1 §7). **RESOLVED:** aliases supply the full label name verbatim; the configured `label_prefix` is NOT prepended.

---

## Plan revision history

The plan-doc set was reviewed by `gsd-plan-checker` before any implementation began. The review surfaced one blocker and three high-severity issues at phase seams. The plan docs were revised to address them.

| Date | Revision | Files touched |
|---|---|---|
| 2026-05-21 | Initial plan set written. | `00`–`07` |
| 2026-05-21 | **H-1 fix:** moved the `Issue.fields` + tracker-populate + template-scope plumbing into Phase 1. Originally punted to Phase 5; that path made the feedback loop unrenderable end-to-end in Phase 1. Added four sub-tasks to Phase 1's deliverable checklist and a `template.rs::tests` round-trip test requirement. | `01` |
| 2026-05-21 | **H-2 fix:** dropped the bridge-side OTLP receiver. Replaced with a typed JSON event stream Sinfonia POSTs to subscriber URLs. `opentelemetry-otlp` is a client crate; standing up a server would have eaten Phase 3's LOC budget. New endpoints: `/api/v1/events/subscribers` on Sinfonia, `/api/v1/sinfonia-events` on the bridge, HMAC-signed. | `03` |
| 2026-05-21 | **H-3 fix:** addressed Linear write-amplification risk by coalescing cost-update writes via a 30 s per-ticket debounce. Cap-crossing still flushes immediately. Drops Linear API write rate by ~10×. | `03` |
| 2026-05-21 | **H-4 fix:** resolved label-alias semantics — aliases supply the full label name verbatim, no prefix prepended. Unit test in `labels::tests`. | `01` |
| 2026-05-21 | **M-5 fix:** added `scripts/verify-workspace-move.sh` requirement so the workspace-conversion commit has a verifiable "logic unchanged" artifact. | `01` |
| 2026-05-21 | **M-3 fix:** every bridge-written custom-field reference in generated skill templates uses `\| default:` filter. Strict-mode Liquid would otherwise error on absent fields. | `05` |
| 2026-05-21 | **M-6 fix:** Docker compose smoke test gets a `docker-compose.ci.yml` overlay so user-cred bind mounts don't trip CI. | `06` |
| 2026-05-21 | **M-7 fix:** reconciled per-phase LOC estimate breakdowns between this index and per-phase doc metadata. | `00`, `01`, `03` |
| 2026-05-21 | **L-2 + L-8 fixes:** consistency cleanup — SQLite-only mapping (no in-memory tier), Liquid-only skill templates (no Handlebars option). | `01`, `05` |
| 2026-05-21 | **Second-pass review** (`gsd-plan-checker` run #2) confirmed H-1..H-4 + the called-out medium/low fixes resolved. Surfaced four new items from the revisions themselves: N-1, N-2, N-3, N-4. | n/a |
| 2026-05-21 | **N-1 fix:** added `sinfonia_events_secret` config key to both BRIDGE.md (`01` §3) and WORKFLOW.md (`03` §3.1, §7.2) telemetry blocks, plus a startup validation rule for when subscribe URL is set but secret is empty. | `01`, `03` |
| 2026-05-21 | **N-2 fix:** tightened §7.3 restart-recovery wording so it doesn't read as accumulator durability. | `03` |
| 2026-05-21 | **N-3 fix:** named the Sinfonia emission integration point (`runner.rs:154`, after `stop_session`) and committed to extending the existing `AgentEvent` enum + `EventSender` channel rather than building a parallel one. Added `AgentEvent::SessionCompleted` to the Phase 3 checklist. | `03` |
| 2026-05-21 | **N-4 fix:** inlined the hand-written `impl Serialize for CustomFieldValue` stub in `01` §4.2 so an implementer doesn't reflex-add `#[derive(Serialize)]` and break the template render path. | `01` |

## Deferred plan-checker findings

These items came from the first review pass but were deliberately not addressed in the revision pass. Recording them here so they aren't silently dropped between now and Phase implementation. Each one is either a phase-local detail that's addressed implicitly, or an explicit v0.4 deferral.

| Finding | Gist | Disposition |
|---|---|---|
| **M-1** | Phase 2 (OpenCode) only depends on Phase 1's workspace conversion landing, not on Phase 1's bridge code. Could run more parallel than the dep graph implies. | **Accepted as a scheduling note.** Phase 2 implementers may begin work the moment the workspace-conversion commit lands on `main`, without waiting for the rest of Phase 1. `02-opencode-backend.md` already says "Depends on Phase 1 (workspace conversion)" — the line is correct; M-1 is a tactical observation about parallelism, not a plan defect. |
| **M-2** | Cost-table drift gate is asymmetric — `WARN` at 90 days, but no operational gate when the table is months stale. Suggested: refuse cost caps (not token caps) when >180 days old. | **Adopt in Phase 3 implementation.** Small defensive change inside `crates/sinfonia-bridge/src/feedback/cost.rs`. Adding this to Phase 3's planned cost-pipeline work, not as a separate item. Tracked here so it isn't forgotten. |
| **M-4** | Phase 3 §6 originally said "polls the tracker every 60s" for terminal-state detection — clashed with the bridge's webhook posture. | **Already addressed by the H-2/H-3 rewrite.** The Phase 3 §6 revision pivoted terminal-state detection to ride the existing GitHub `pull_request.closed.merged=true` webhook. No tracker polling loop is added. M-4 is now closed. |
| **M-8** | `inquire` should be scoped to `crates/sinfonia/Cargo.toml`, not the workspace `[workspace.dependencies]`. | **Acknowledged as a Phase 5 implementation detail.** The plan doc (`05-skills-cli.md` §6) lists `inquire` under crate-local deps; the workspace-level vs crate-local distinction is settled at impl time. No plan-doc change needed. |

All other open questions from the first review remain as listed in the per-phase "Open questions" sections (Phase 1 §11, Phase 2 §7, Phase 3 §11, Phase 4 §7, Phase 5 §7, Phase 6 §8) — they're phase-local defaults that resolve at implementation time.

---

## How to use these documents

Reviewing the plan:

1. Read this overview first.
2. Read `01-bridge-mvp.md` in full — it's the foundational phase and sets the patterns the others follow.
3. Skim the other phase docs for the per-phase decisions and open questions. The deliverable checklist at the end of each phase doc is the most actionable artifact.
4. Comment / push back in the channel(s) the team uses for design review.

Approving a phase plan unblocks implementation of that phase:

- Mark the corresponding plan section ✅ in this overview when approval lands.
- Open a tracking GitHub Issue per phase (or a single milestone issue with sub-checkboxes).
- Start implementation on a phase branch off `new-features`.

Implementation produces:

- Code (the substantive output).
- A `<NN>-<phase>-VERIFY.md` sibling next to each plan doc, capturing manual-verification output. Replaces the stub at the deliverable-checklist line "Manual verification recorded in ...".
- Updated CHANGELOG entries in `crates/sinfonia/CHANGELOG.md` or the root `CHANGELOG.md` (whichever convention we settle on in Phase 7).

When all seven phase checklists are complete, v0.3.0 is releasable.

---

## Approval status

| Phase | Plan reviewed | Plan approved | Implementation started | Implementation merged |
|---|---|---|---|---|
| 1 — Bridge MVP | ☐ | ☐ | ☐ | ☐ |
| 2 — OpenCode | ☐ | ☐ | ☐ | ☐ |
| 3 — Telemetry | ☐ | ☐ | ☐ | ☐ |
| 4 — Jira | ☐ | ☐ | ☐ | ☐ |
| 5 — Skills + CLI | ☐ | ☐ | ☐ | ☐ |
| 6 — Docker | ☐ | ☐ | ☐ | ☐ |
| 7 — Docs | ☐ | ☐ | ☐ | ☐ |

When the first column flips to ✅ for a phase, implementation can begin.
