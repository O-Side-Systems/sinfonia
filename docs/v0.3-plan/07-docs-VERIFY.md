# Phase 7 — Verify

**Status as of 2026-05-22:** Phase 7 branch (`v0.3-phase-7-docs`) lands the
v0.3.0 doc set in three reviewable commits (P7-A SPEC + stub polish; P7-B
new long-form guides; P7-C README rewrite + CHANGELOG promotion + CI).
The thirteen `07-docs.md` §13 deliverable boxes resolve as:

| Box | Status | Where |
|---|---|---|
| README rewrite per §2 | ✅ landed | `README.md` — "What's new in v0.3" rewritten as a five-item summary keyed on user questions; new "Where to go next" cross-link block above Getting Started; conformance scorecard §18.2 expanded for the six new recommended-extension bullets. |
| `CHANGELOG.md` v0.3.0 entry per §10 | ✅ landed | `CHANGELOG.md` — `[Unreleased]` promoted to `[0.3.0]` with a one-paragraph release summary and a P7 Added bullet covering the doc set. Migration section pointer. Compare links updated. Date placeholder `2026-MM-DD` to be filled at tag time. |
| `docs/SPEC.md` §11.5 / §11.6 / §11.7 / §18.2 per §3 | ✅ landed | §11.5 tightened ("orchestrator MUST NOT write"; pointer to §11.6); §11.6 "Draft" tag dropped (the contract is stable across P1+P3+P4); new §11.7 custom-field discovery (Linear marker-comment vs Jira `customfield_NNNNN` resolution); §18.2 grew six bullets (Jira tracker adapter, CI feedback bridge, failure categorization, budget enforcement, PR label management; alongside pre-existing OpenCode / OTel / setup-skills entries). Two bare-URL fixes folded in to satisfy markdownlint. |
| `docs/SKILLS.md` polished | ✅ landed | Audience / read-time / prereqs header added; "Where to go next" cross-link block at the bottom pointing at DEPLOYMENT / CLIENT_SETUP / MIGRATION / SPEC §18.2. (The P5 stub was already shipping-quality on content; this is a consistency pass.) |
| `docs/DEPLOYMENT.md` per §4 | ✅ landed | ~470 lines. At-a-glance table + four self-contained topology walkthroughs (daemon-only / daemon+bridge single-host / daemon+bridge separate hosts / bridge as GitHub Actions). Credential model. Webhook-reachability gotchas. Observability primer. Scaling / Backup / Upgrading. |
| `docs/CLIENT_SETUP.md` per §5 | ✅ landed | ~430 lines. Trust-boundary diagram + one-paragraph security summary. GitHub App vs PAT comparison. Three-layer budget controls (concurrency / per-attempt / per-ticket). Audit-trail queries. Failure handling. Handoff. Vendor-evaluation worksheet organized by trust / operational / compliance / cost-containment with source-pointer answers. |
| `docs/JIRA-SCREEN-SCHEME.md` polished | ✅ landed | "Where to go next" cross-link block added. (P4 stub was already shipping-quality on content.) |
| `docs/MIGRATION-v0.2-to-v0.3.md` per §6 | ✅ landed | ~150 lines. "What you DON'T need to do" leads. Required / optional / breaking changes (none expected) / compatibility notes sections. |
| `WORKFLOW.example.md` updated examples per §7 | ✅ landed | New `telemetry:` block walkthrough showing tenant_id precedence + opt-in default + sinfonia_events_secret wiring. OpenCode section expanded to three concrete usage variants (default-lane / state-machine / air-gapped Ollama-with-LSP). New full failure-categorization state-machine example wired to BRIDGE.md's `feedback_loop.failure_categories`. Every `{{ issue.fields.X }}` reference guarded with `\| default:` per the §8 box-2 grep invariant. |
| `BRIDGE.example.md` polished | ✅ landed | Budget-caps section shows realistic non-null values (was: "Phase 3 deferred"). Custom-fields header explains the Jira resolution path. Telemetry block explains tenant_id precedence + opt-in default + the typed event-channel URL semantics. New `cost_table_path` override block. "Where to go next" cross-link block at the bottom. `--check` still passes against the example. |
| `CONTRIBUTING.md` workspace update per §9 | ✅ landed | Three-crate table + "where to add new code" guidance. `cargo build/test/clippy` commands updated for `--workspace` and `-p crate` variants. "Adding a new tracker" reflects the shared tracker crate path AND the five bridge-write methods. "Adding a new agent backend" mentions opencode.rs as a third pattern. Release flow mentions the six-image Docker fan-out. |
| CI: link-rot, markdown lint, code-block syntax checks per §11 | ✅ landed | `.github/workflows/docs.yml` with three jobs: markdownlint-cli2 (DavidAnson action), lychee internal-link sweep on PR + push, lychee full sweep weekly via cron. `.markdownlint-cli2.yaml` config (relaxed style rules; active rules still catch bare URLs and structural issues). `lychee.toml` excludes placeholder hosts. `scripts/check-doc-code-blocks.sh` extracts every fenced YAML / JSON / TOML / bash block and runs each through its respective syntax checker — currently passes against every doc in the repo. |
| Manual readthrough by someone uninvolved in v0.3 implementation, with findings addressed | ⏳ deferred | Per user direction (`/start phase 7` confirmation: "Defer with VERIFY note"). The doc set is code-complete; the readthrough is the same pattern as Phase 4/5/6's pending verification matrices — to run against `v0.3.0-rc.x` before the GA tag. |

---

## 1. Plan-doc deltas surfaced at implementation time

### 1.1 The `[Unreleased]` block stayed in place

Plan §10 specified a single `[0.3.0]` heading replacing `[Unreleased]`. The
shipped CHANGELOG keeps `[Unreleased]` (now empty) ABOVE `[0.3.0]` so that
in-flight commits between v0.3.0 tag and v0.3.1 have somewhere to go.
This matches the Keep-a-Changelog convention.

### 1.2 §11.6 lost the "Draft" tag

Plan §3.2 wrote "Status: *Draft.*" The Draft language made sense in Phase 1
when only the Linear adapter was wired up. By Phase 7 the contract is
stable across P1+P3+P4 (Linear marker comments + typed event channel +
budget surface + Jira customfield resolution). §11.6 is now annotated
"Recommended Extension" without the Draft caveat, in keeping with the
other §18.2-class extensions in the same SPEC.

### 1.3 The four-topology guide added a fourth shape (GHA bridge)

Plan §4.1 listed three topologies. The shipped DEPLOYMENT.md adds Topology
4 (bridge as GitHub Actions) because two of the discuss-phase user
questions surfaced "the bridge can't be internet-reachable" as a real
deployment constraint. The Topology 4 section documents the trade-off
(latency vs reachability) and ships a skeleton workflow + a note that
`--once` mode is a v0.3.1 candidate; the v0.3.0 path is to POST to the
existing `/webhook` handler from within the Action and `kill` the bridge.

### 1.4 The vendor-evaluation worksheet is a four-table block

Plan §5.1 outlined "Vendor evaluation worksheet" as a checklist. The
shipped CLIENT_SETUP.md splits the checklist into four tables by axis
(Trust and credentials / Operational posture / Compliance / Cost
containment) so a security reviewer can pull the relevant rows for
their review without reading the whole doc.

### 1.5 Docs CI tool selection

Per user direction (`/start phase 7` confirmation: "lychee +
markdownlint-cli2"). The implementation lands:

- **markdownlint-cli2** via `DavidAnson/markdownlint-cli2-action@v16`. The
  `.markdownlint-cli2.yaml` config disables nine rules that would force
  gratuitous churn on already-shipped content; the active rule set still
  catches MD004 (list style), MD034 (bare URLs), and the structural
  defaults that protect long-form readability.
- **lychee** via `lycheeverse/lychee-action@v2`. Two jobs: internal-only
  on PR (offline mode, fast feedback), external sweep weekly via cron
  (to catch upstream rot without blocking PRs on transient outages).
- **Code-block syntax** via `scripts/check-doc-code-blocks.sh` (new). Runs
  PyYAML / jq / Python tomllib / shellcheck-or-bash-n against every
  fenced ``` ```yaml / ```json / ```toml / ```bash ``` block in the
  public-facing Markdown surface. YAML blocks that look like a full
  `WORKFLOW.md` / `BRIDGE.md` sample (open with `---\n`, contain a second
  `---\n` followed by Markdown prose) get truncated to the front matter.

The plan §11.4 spec-conformance test (asserting every §18.2 bullet is
implemented in the codebase) is deferred to v0.3.1 — most §18.2 bullets
ARE implemented in v0.3.0, but writing the cross-reference test
mechanically would require parsing SPEC.md prose into a structured
assertion catalog, which is out of scope for a docs phase.

---

## 2. Verification matrix (V-1 through V-5)

Per plan §11.5, the readthrough is the canonical manual check. Until
that happens against a real v0.3.0-rc.x build, the rows below capture
the gate each item passed at implementation time.

| Id | Item | Status | Evidence |
|---|---|---|---|
| V-1 | README rewrite reads like a v0.3.0 release announcement, not a v0.3.0-alpha.1 status update | ✅ self-check | The five "What's new in v0.3" items each have a concrete config example + cross-link to the deeper doc; the "preview" / "currently being landed" tags from the alpha block are gone. |
| V-2 | `sinfonia-bridge BRIDGE.example.md --check` continues to pass | ✅ verified | Run during P7-A polish; exit 0. |
| V-3 | `sinfonia --check WORKFLOW.example.md` continues to pass | ✅ verified | Same as main: needs `LINEAR_API_KEY` (placeholder env var); with that set, exit 0. |
| V-4 | Docs CI tooling passes locally against the new doc set | ✅ verified | `markdownlint-cli2`: 18 files, 0 errors. `lychee --offline --exclude-path docs/v0.3-plan`: 141 links, 0 errors. `scripts/check-doc-code-blocks.sh`: ok. |
| V-5 | Manual readthrough by someone uninvolved in v0.3 | ⏳ deferred | Pre-v0.3.0-rc.x. Findings are folded into a follow-up doc-patch commit; no STATUS bump on findings (they're considered part of the same Phase 7 deliverable). |

---

## 3. Where things landed (file count)

- 8 modified docs across the repo root (`README`, `CHANGELOG`,
  `CONTRIBUTING`, `BRIDGE.example`, `WORKFLOW.example`).
- 5 modified docs under `docs/` (`SPEC`, `SKILLS`, `JIRA-SCREEN-SCHEME`,
  the new `DEPLOYMENT` / `CLIENT_SETUP` / `MIGRATION-v0.2-to-v0.3`).
- 4 new CI / tooling files (`.github/workflows/docs.yml`,
  `.markdownlint-cli2.yaml`, `lychee.toml`,
  `scripts/check-doc-code-blocks.sh`).
- 1 minor SKILL.md touch (`skills/setup-bridge/SKILL.md` — two bare URLs
  wrapped to satisfy markdownlint).
- Two intermediate test runs to confirm `--check` and the doc CI all
  pass.

Total: ~2700 lines added (within the plan's ~3000-line estimate).

---

## 4. What's NOT shipped in Phase 7

- `docs/CLI.md`, `docs/CONFIG.md`, `docs/TROUBLESHOOTING.md` (plan §2.3
  contingency files). The README didn't grow enough to need cutting —
  it's at ~530 lines after the rewrite, and the §2.3 "what we cut"
  section was explicitly contingent on needing to shrink the README.
- `examples/runbook.md` (plan §12 open question 5: "promote when we
  have real-world content"). Phase 7 doesn't synthesize speculative
  runbook content; the existing CLIENT_SETUP.md handoff section + the
  §6.5 runbook-template headers in there are the v0.3.0 answer.
- The plan §11.4 SPEC-conformance test cross-checking §18.2 against the
  codebase (deferred to v0.3.1; rationale above).
- The bridge `--once` single-shot mode for Topology 4 (deferred to
  v0.3.1; rationale in CHANGELOG `Deferred to v0.3.1`).

---

## 5. Open after Phase 7

- Manual readthrough (V-5) before `v0.3.0` tag.
- Fill the `2026-MM-DD` date placeholder in `CHANGELOG.md` at tag time.
- Fill the `Fixed:` section in the `[0.3.0]` block with any fix-class
  commits that land between Phase 7 merge and the v0.3.0 tag (currently
  empty by design).
