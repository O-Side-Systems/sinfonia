# Phase 5: Repository Context Contract — Pattern Map

**Mapped:** 2026-06-14
**Files analyzed:** 13 (new: 11, modified: 4)
**Analogs found:** 13 / 13 (all files have at least a role-match analog)

---

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `docs/CONTEXT-CONTRACT.md` | spec-doc (new contract) | reference | `docs/HARNESS-SPEC.md` | role-match (same doc style, same §-numbered sections, same RFC 2119 normative language) |
| `AGENTS.md` (root) | dogfood node / entry-point | reference | `.planning/intel/ingest-sources/harness-improvement-analysis.md` §Context Contract concrete shape + RESEARCH.md §Code Examples | no exact analog — see "No Analog" table; pattern is net-new per RESEARCH.md |
| `crates/sinfonia/AGENTS.md` | dogfood node (crate) | reference | same as root | same — net-new pattern class |
| `crates/sinfonia-bridge/AGENTS.md` | dogfood node (crate) | reference | same | same |
| `crates/sinfonia-tracker/AGENTS.md` | dogfood node (crate) | reference | same | same |
| `docker/AGENTS.md` | dogfood node (area dir) | reference | same | same |
| `docs/AGENTS.md` | dogfood node (area dir) | reference | same | same |
| `templates/AGENTS.md` | template / downstream starter | reference | `RESEARCH.md §Code Examples` (front-matter schema, sizing rules) | role-match |
| `templates/CODEOWNERS` | ownership-config template | ownership | `.github/CODEOWNERS` (3 lines) | exact-template |
| `docs/HARNESS-SPEC.md` | spec amendment (§7 + §9) | reference | `docs/HARNESS-SPEC.md` §7.3 (existing bullet list) + §9 (existing checklist) | self-analog (edit in place) |
| `docker/WORKFLOW.md` | prompt amendment (fallback) | prompt-text | `docker/WORKFLOW.md:480–505` (fallback "What to do" section) | self-analog (targeted edit at line 499) |
| `docker/WORKFLOW.example.md` | prompt amendment (fallback) | prompt-text | `docker/WORKFLOW.example.md:720–743` (same fallback section) | self-analog (targeted edit at line 737) |
| `.github/CODEOWNERS` | ownership-config (extend) | ownership | `.github/CODEOWNERS:1–3` | self-analog (append entries) |

---

## Pattern Assignments

### `docs/CONTEXT-CONTRACT.md` (spec-doc, new contract)

**Analog:** `docs/HARNESS-SPEC.md`

**Doc header pattern** (`docs/HARNESS-SPEC.md` lines 1–19):
```markdown
# Harness Authoring Specification

- **Status:** *Recommended pattern.* This document specifies …
- **Audience:** authors bootstrapping …
- **Companion specs:** `docs/SPEC.md` …

## Normative Language

The key words MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, RECOMMENDED, MAY, and
OPTIONAL are to be interpreted as in RFC 2119.

A repository is **Sinfonia-ready** when it satisfies every MUST in §3 and §7.
```

Mirror this exactly for CONTEXT-CONTRACT.md: open with a bullet-header block (`Status`, `Audience`, `Companion specs`) then a Normative Language section. Swap in the correct companion refs (`docs/HARNESS-SPEC.md §7.3`, `docs/SPEC.md`).

**Numbered-section and REQUIRED/RECOMMENDED tagging pattern** (`docs/HARNESS-SPEC.md` lines 80–96):
```markdown
## 3. Conformance (overview)

A Sinfonia-ready harness MUST:

1. Author tests **outside-in** …  (§4.2)
2. Emit the **four-artifact contract** … (§5.1–§5.2)
…
It SHOULD additionally provide … (§5.5)
It MAY provide … (§4.1) … This is **OPTIONAL** …
```

Use the same three-tier (MUST / SHOULD / MAY) structure inside CONTEXT-CONTRACT.md §3 (Conformance overview) and §4 (Node Shape). Parenthetical back-references like `(§4.2)` are the established convention.

**Conformance-checklist pattern** (`docs/HARNESS-SPEC.md` lines 416–439, §9):
```markdown
## 9. Conformance Checklist

A repo is **Sinfonia-ready** when:

- [ ] Scenarios are authored outside-in … (§4.2)
- [ ] Every scenario emits four artifacts … (§5.1)
…
- [ ] *(RECOMMENDED)* architectural-invariant gating (§5.5) …
- [ ] *(OPTIONAL)* a natural-language → executable-specification step … (§4.1)
```

CONTEXT-CONTRACT.md §Conformance Checklist (listed in RESEARCH.md §Code Examples) copies this pattern exactly — `- [ ]` bullets, back-reference in parens, RECOMMENDED/OPTIONAL in italics at end. The nine checklist items from RESEARCH.md §Manual Conformance Checklist should use this exact bullet style.

**Two-column comparison table pattern** (`docs/HARNESS-SPEC.md` lines 39–46):
```markdown
| Prescribed (this spec) | NOT prescribed (target repo's choice) |
|---|---|
| The loop *structure* … | The spec language … |
```

Use this table style (two-col, `|---|---|` separator row) in CONTEXT-CONTRACT.md whenever contrasting "what the contract mandates" vs. "what the node author decides."

**Worked example / non-normative appendix pattern** (`docs/HARNESS-SPEC.md` lines 383–412, §8):
Use `## N. Section Name (non-normative)` heading for any illustrative examples in CONTEXT-CONTRACT.md, matching the HARNESS-SPEC convention.

---

### `AGENTS.md` (root dogfood node) and all five crate/area AGENTS.md nodes

**Analog:** RESEARCH.md §Code Examples (front-matter schema + root node body) — no real-codebase file to mirror; all AGENTS.md nodes are net-new. The RESEARCH.md code examples are the authoritative pattern to copy.

**Front-matter schema** (RESEARCH.md lines 180–189):
```yaml
---
title: "sinfonia-bridge — CI Feedback Crate"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "6598fe4"
derived_from: "docs/SPEC.md §11.6, docs/proposals/0001-harness-feedback-ingestion.md"
---
```

All six dogfood nodes and the template MUST open with this YAML block. Fields are required in this order: `title`, `version`, `status`, `owners`, `last_verified_sha`, `derived_from`. For the initial authoring pass, use the current HEAD short SHA (from `git rev-parse --short HEAD`) as `last_verified_sha`.

**Root node body pattern** (RESEARCH.md lines 413–445):
```markdown
# Sinfonia — Agent Context

Sinfonia is a polling orchestrator that turns Linear/Jira issues into
agent sessions. Three Rust crates: `sinfonia` (orchestrator), `sinfonia-bridge`
(CI-feedback loop), `sinfonia-tracker` (Linear/Jira adapters).

## Module Ownership

| Capability | Path-glob | Owned By | Don't Roll Your Own |
|------------|-----------|----------|---------------------|
| Issue dispatch / orchestration | `crates/sinfonia/src/orchestrator/` | @osidesys @leebrett | See crates/sinfonia/AGENTS.md |
| Agent runners (Anthropic/OpenAI/CLI) | `crates/sinfonia/src/agent/` | @osidesys @leebrett | See crates/sinfonia/AGENTS.md |
| CI-to-issue feedback bridge | `crates/sinfonia-bridge/src/` | @osidesys @leebrett | See crates/sinfonia-bridge/AGENTS.md |
| Linear/Jira adapters | `crates/sinfonia-tracker/src/` | @osidesys @leebrett | See crates/sinfonia-tracker/AGENTS.md |
| Docker topology + WORKFLOW templates | `docker/` | @osidesys @leebrett | See docker/AGENTS.md |
| Spec docs + proposals | `docs/` | @osidesys @leebrett | See docs/AGENTS.md |

## See also
- [`docs/SPEC.md`](docs/SPEC.md) — orchestrator + bridge contract
- [`docs/HARNESS-SPEC.md`](docs/HARNESS-SPEC.md) — harness authoring spec
- [`docs/CONTEXT-CONTRACT.md`](docs/CONTEXT-CONTRACT.md) — this doc-graph's contract
```

This is the **complete body structure** for `AGENTS.md` (root). The four-column module-ownership table (`Capability | Path-glob | Owned By | Don't Roll Your Own`) is the canonical format — use it in all six dogfood nodes (sub-tables in crate nodes cover that crate's internal modules).

**Sizing constraint from RESEARCH.md Pattern 3:** Node body MUST be ≤ 50 rendered lines (excluding the 8-line front-matter block, so total file ≤ 58 lines). Verify with `wc -l <node>` before committing.

**Per-crate node structure** (derived from RESEARCH.md Pattern 1 + Pattern 3):
Each crate AGENTS.md follows this skeleton (adapt paths/capability names):
```markdown
---
title: "sinfonia-<crate> — <one-line scope>"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "<7-char SHA>"
derived_from: "docs/SPEC.md §<relevant section>"
---

# sinfonia-<crate> — <one-line scope>

<1–3 sentence scope summary>. Do not edit `<key file>` without also
checking `<coupled file>` coupling.

## Module Ownership

| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
| <sub-module 1> | `crates/sinfonia-<x>/src/<sub>/` | @osidesys @leebrett | — |
| <sub-module 2> | `crates/sinfonia-<x>/src/<sub2>/` | @osidesys @leebrett | — |

## See also
- [`docs/SPEC.md §<N>`](docs/SPEC.md) — <what it describes>
- [`AGENTS.md`](../../AGENTS.md) — root entry point
```

---

### `templates/AGENTS.md` (template / downstream starter)

**Analog:** RESEARCH.md §Code Examples (front-matter schema) + RESEARCH.md Pitfall 6 (template sizing constraint)

**Template front-matter pattern** — same YAML schema as dogfood nodes, with placeholder values:
```yaml
---
title: "<repo name> — Agent Entry Point"
version: "1.0.0"
status: draft
owners: ["@<owner1>"]
last_verified_sha: "<7-char git SHA of last verified commit>"
derived_from: "README.md"
---
```

**Template comment style** (RESEARCH.md Pitfall 6): Use `<!-- replace: ... -->` markers only, NOT prose explanations. All guidance for filling the template lives in `docs/CONTEXT-CONTRACT.md`, not in the template file itself.

**Module-ownership table** (RESEARCH.md §Code Examples): Include the four-column table from the root node pattern. Replace all capability rows with `<!-- replace: add one row per shared capability -->` placeholder.

**Sizing gate:** `wc -l templates/AGENTS.md` MUST return ≤ 58. If replacing `<!-- replace: ... -->` markers with real content would exceed 50 body lines, the template is too large.

---

### `templates/CODEOWNERS` (ownership-config template)

**Analog:** `.github/CODEOWNERS` lines 1–3 (exact content) + RESEARCH.md Pattern 5 (extension entries)

**Existing CODEOWNERS** (`.github/CODEOWNERS` lines 1–3):
```gitignore
# Default owners for everything in the repo.
# These users are requested for review when someone opens a PR.
* @osidesys @leebrett
```

**Template CODEOWNERS pattern** (RESEARCH.md Pattern 5 + §Code Examples):
```gitignore
# Default owners for everything in the repo.
# These users are requested for review when someone opens a PR.
* @<owner1> @<owner2>

# Context graph nodes — explicit gate on all AGENTS.md edits (last-match-wins)
**/AGENTS.md @<owner1> @<owner2>

# Per-area ownership (mirrors AGENTS.md module-ownership table)
/crates/<crate-1>/  @<owner1> @<owner2>
/crates/<crate-2>/  @<owner1> @<owner2>
/docker/            @<owner1> @<owner2>
/docs/              @<owner1> @<owner2>
/templates/         @<owner1> @<owner2>
```

The `**/AGENTS.md` line MUST come AFTER the `*` catch-all (last-match-wins precedence). The per-area entries are documentation-in-enforcement and anticipate future differentiated ownership. Replace all `@<ownerN>` and `/crates/<crate-N>/` with `<!-- replace: ... -->` markers following the same comment style as `templates/AGENTS.md`.

---

### `docs/HARNESS-SPEC.md` — §7.3 amendment + §9 checklist line

**Analog:** `docs/HARNESS-SPEC.md` §7.3 existing bullet list (lines 329–339) + §9 checklist (lines 416–439)

**§7.3 existing bullet pattern** (lines 329–339) — the five current bullets to mirror for the sixth:
```markdown
### 7.3 Repository conventions (REQUIRED)

- **Branch:** agent work lands on `sinfonia/<issue-id>` branches.
- **PR body:** MUST contain a tracker-identifier line …
- **Labels:** the `sinfonia:*` label namespace is **bridge-owned** …
- **CODEOWNERS:** a human-review gate MUST cover Sinfonia-touched paths …
- **CI gates:** the harness gate … MUST block merge on failure.
```

**New §7.3 bullet to insert** (RESEARCH.md Pattern 7):
```markdown
- **Context graph:** The repo MUST maintain a hierarchical `AGENTS.md` doc-graph
  conforming to `docs/CONTEXT-CONTRACT.md`. The root `AGENTS.md` is the agent
  entry point; all node edits ride in the code PR under CODEOWNERS. See
  [`docs/CONTEXT-CONTRACT.md`](docs/CONTEXT-CONTRACT.md) for the full contract.
```

Insert after the existing **CI gates** bullet (currently the last item in §7.3). Bold key term, MUST in caps, cross-link to the contract in markdown link format — consistent with all existing §7.3 bullets.

**§9 existing checklist pattern** (lines 416–439):
```markdown
- [ ] Scenarios are authored outside-in … (§4.2)
- [ ] Every scenario emits four artifacts … (§5.1)
…
- [ ] *(RECOMMENDED)* architectural-invariant gating (§5.5) …
```

**New §9 checklist line to append** (RESEARCH.md Pattern 7):
```markdown
- [ ] A root `AGENTS.md` exists and conforms to `docs/CONTEXT-CONTRACT.md`;
      CODEOWNERS gates all `**/AGENTS.md` edits. (§7.3)
```

Insert before the `*(RECOMMENDED)*` lines so it sits with the REQUIRED items. Back-reference `(§7.3)` in parens, consistent with the existing pattern.

---

### `docker/WORKFLOW.md` — fallback prompt edit (line 499)

**Analog:** `docker/WORKFLOW.md` lines 480–505 (the fallback "What to do" section)

**Current state — full fallback block** (lines 480–505):
```
You are picking up Linear issue `{{ issue.identifier }}`: {{ issue.title }}.

This is the *default* prompt body …

{% if attempt %}
This is attempt {{ attempt }} of this run. …
{% endif %}

## Issue description

{{ issue.description }}

## What to do

1. Orient: `README.md`, `CLAUDE.md`, `docs/`.
2. Make focused changes — minimal diffs.
3. Run tests + linters.
4. Commit on `sinfonia/{{ issue.identifier | downcase }}` and push.
5. Open a PR with `gh` referencing `{{ issue.identifier }}`.
6. Stop. Don't transition the ticket from this fallback path.
```

**Targeted edit:** Replace only line 499. The surrounding numbered list (items 2–6) is unchanged.

**Replacement text** (RESEARCH.md Pattern 6 + §Code Examples):
```
1. Orient (just-in-time — follow this order exactly):
   a. Read root `AGENTS.md`.
   b. In its module-ownership table, find rows whose path-globs match files
      this issue's acceptance criteria mention or imply.
   c. Read ONLY the `AGENTS.md` nodes linked from those matching rows.
   d. For anything not yet found: `grep -r '<term>' crates/` or
      `git ls-files | grep '<pattern>'`.
   Do NOT open `docs/` wholesale. Do NOT read `README.md` unless a node
   links to it for a specific reason.
```

**Non-disruption constraint:** The In Progress prompt (~lines 294–475) has no explicit orient step — it goes directly to STEP 0 scripting. The RESEARCH.md confirms this (Assumption A4, Pattern 6). Do NOT insert an orient preamble before STEP 0 in the In Progress prompt unless the planner explicitly scopes it. Only the fallback at line 499 is a locked edit.

---

### `docker/WORKFLOW.example.md` — fallback prompt edit (line 737)

**Analog:** `docker/WORKFLOW.example.md` lines 720–743 (identical structure to WORKFLOW.md fallback)

**Current state — full fallback block** (lines 720–743):
```
This is the *default* prompt body …

{% if attempt %}
This is attempt {{ attempt }} of this run. …
{% endif %}

## Issue description

{{ issue.description }}

## What to do

1. Orient: `README.md`, `CLAUDE.md`, `docs/`.
2. Make focused changes — minimal diffs.
3. Run tests + linters.
4. Commit on `sinfonia/{{ issue.identifier | downcase }}` and push.
5. Open a PR with `gh` referencing `{{ issue.identifier }}`.
6. Stop. Don't transition the ticket from this fallback path.
```

**Targeted edit:** Replace only line 737 with the identical JIT orient text as WORKFLOW.md. Items 2–6 are unchanged.

**Post-edit verification smoke command** (RESEARCH.md §Validation):
```bash
grep -n "Orient.*docs/" docker/WORKFLOW.md docker/WORKFLOW.example.md   # expect 0 hits
grep -n "CLAUDE.md"    docker/WORKFLOW.md docker/WORKFLOW.example.md   # expect 0 hits
grep -c "AGENTS.md"    docker/WORKFLOW.md                              # expect >= 1
grep -c "AGENTS.md"    docker/WORKFLOW.example.md                      # expect >= 1
```

---

### `.github/CODEOWNERS` — append new entries

**Analog:** `.github/CODEOWNERS` lines 1–3 (existing 3-line file)

**Current file** (lines 1–3):
```gitignore
# Default owners for everything in the repo.
# These users are requested for review when someone opens a PR.
* @osidesys @leebrett
```

**Append pattern** (RESEARCH.md Pattern 5 + §Code Examples):
```gitignore
# Context graph nodes — explicit gate on all AGENTS.md edits (last-match-wins)
**/AGENTS.md @osidesys @leebrett

# Per-area ownership (mirrors AGENTS.md module-ownership table)
/crates/sinfonia/         @osidesys @leebrett
/crates/sinfonia-bridge/  @osidesys @leebrett
/crates/sinfonia-tracker/ @osidesys @leebrett
/docker/                  @osidesys @leebrett
/docs/                    @osidesys @leebrett
/templates/               @osidesys @leebrett
```

Append all entries AFTER the existing `*` line. Last-match-wins means `**/AGENTS.md` appearing after `*` gives it explicit visibility without changing the effective owner (same pair). The per-area entries are anticipatory for future ownership differentiation and mirror the module-ownership table in root `AGENTS.md`.

---

## Shared Patterns

### YAML Front-Matter Block

**Source:** RESEARCH.md lines 180–189 (Schema definition) + `.planning/STATE.md` lines 1–15 (style analog for YAML front-matter in this repo's markdown files)

**Apply to:** All six dogfood AGENTS.md nodes + `templates/AGENTS.md`

**Repo's existing front-matter style** (`.planning/STATE.md` lines 1–15):
```yaml
---
gsd_state_version: 1.0
milestone: v0.4
status: planning
last_updated: "2026-06-14T20:30:57.881Z"
---
```

**Phase 5 node front-matter style** (RESEARCH.md, net-new convention on top of this):
```yaml
---
title: "<name> — <scope>"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "<7-char SHA>"
derived_from: "<source doc>"
---
```

The project already uses `---` delimiters and scalar + quoted-string YAML values. Phase 5 nodes adopt the same delimiter style; `owners` is a YAML array (square-bracket notation).

---

### HARNESS-SPEC §-Numbered Section Style

**Source:** `docs/HARNESS-SPEC.md` lines 1–19, 80–96, 329–339, 416–439

**Apply to:** `docs/CONTEXT-CONTRACT.md` (all sections)

Section headings use `## N. Section Name (REQUIRED)` or `### N.M Sub-section (RECOMMENDED)` with normative qualifier in the heading. Body uses RFC 2119 caps (MUST, SHOULD, MAY). Back-references are `(§N.M)` in parens at end of sentence.

---

### CODEOWNERS Last-Match-Wins Ordering

**Source:** `.github/CODEOWNERS` lines 1–3 + RESEARCH.md Pattern 5

**Apply to:** `.github/CODEOWNERS` (append) + `templates/CODEOWNERS` (template)

Specific patterns always appear AFTER the `*` catch-all. Entries are grouped with a comment header explaining the group. Alignment of `@owner` tokens with spaces is not required (existing file uses none).

---

### Fallback Prompt List Item Style

**Source:** `docker/WORKFLOW.md` lines 499–504 (existing fallback numbered list)

**Apply to:** `docker/WORKFLOW.md:499` and `docker/WORKFLOW.example.md:737` (replacement text only)

Existing items use plain `N. Verb noun` format with backtick code references. The JIT orient replacement uses a lettered sub-list (`a.`, `b.`, `c.`, `d.`) for the four steps, matching the multi-step pattern already established in the In Progress prompt (lines 201–243 use this `a.`/`b.`/`c.` sub-step style). A `Do NOT` warning paragraph follows, consistent with the existing `> ⛔ **If …**` warning style elsewhere in the prompts.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `AGENTS.md` (root) | dogfood agent node | reference | No AGENTS.md files exist anywhere in the repo yet; the format is a net-new project-level convention. RESEARCH.md §Code Examples is the only pattern source. |
| `crates/sinfonia/AGENTS.md` | dogfood node (crate) | reference | Same — no prior per-directory AGENTS.md nodes to mirror. |
| `crates/sinfonia-bridge/AGENTS.md` | dogfood node (crate) | reference | Same. |
| `crates/sinfonia-tracker/AGENTS.md` | dogfood node (crate) | reference | Same. |
| `docker/AGENTS.md` | dogfood node (area) | reference | Same. |
| `docs/AGENTS.md` | dogfood node (area) | reference | Same. |

**Planner note for all six:** Use RESEARCH.md Pattern 1 (front-matter schema), Pattern 3 (one-screen sizing), and the §Code Examples root-node body as the authoritative template. The `.planning/STATE.md` YAML front-matter establishes that `---` delimiters are used in this repo's markdown files.

---

## Metadata

**Analog search scope:** `/Users/brettlee/work/sinfonia/docs/`, `/Users/brettlee/work/sinfonia/.github/`, `/Users/brettlee/work/sinfonia/docker/`, `/Users/brettlee/work/sinfonia/.planning/`
**Files read:** `docs/HARNESS-SPEC.md`, `docs/SPEC.md`, `.github/CODEOWNERS`, `docker/WORKFLOW.md` (lines 1–505), `docker/WORKFLOW.example.md` (lines 720–743), `.planning/STATE.md`, `.planning/phases/04-merge-conflict-handling-merge-queue/04-02-PLAN.md`, `.planning/intel/ingest-sources/harness-improvement-analysis.md`
**Pattern extraction date:** 2026-06-14
