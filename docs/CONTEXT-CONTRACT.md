# Repository Context Contract

- **Status:** *Normative specification.* This document defines the **Repository
  Context Contract** — the shape, resolution semantics, and read/write protocol
  for the hierarchical `AGENTS.md` doc-graph used by Sinfonia's agentic loop.
  It is the **map**; `docs/HARNESS-SPEC.md` is the **sensor** (DEC-004).
- **Audience:** authors authoring or modifying `AGENTS.md` nodes in any repository
  driven by Sinfonia; agents reading context; Phase-6 linter designers.
- **Companion specs:** `docs/HARNESS-SPEC.md §7.3` (context-graph convention entry
  and conformance checklist line), `docs/SPEC.md` (orchestrator + bridge contract).

## Normative Language

The key words MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, RECOMMENDED, MAY, and
OPTIONAL are to be interpreted as in RFC 2119.

A repository is **contract-valid** when every `AGENTS.md` node it contains satisfies
every MUST in §2 and §4, and the write events in §6 are honored.

---

## 1. Purpose and Scope (REQUIRED)

This contract specifies the **shape** of the doc-graph: the node front-matter schema,
the nearest-wins resolution algorithm, the one-concern/one-screen sizing rule, the
just-in-time read protocol, and the write protocol. It contains **no concrete node
data** — no real path-globs, no real owner handles, no crate-specific tables. Those
belong in the `AGENTS.md` nodes themselves (§4) and the dogfood instances. (§4.4)

Per DEC-004 ("separate the sensor from the map"): `docs/HARNESS-SPEC.md` specifies
the harness contract (the sensor); this document specifies the context graph (the map).
Neither document contains the other's body.

| Prescribed (this contract) | NOT prescribed (node author's choice) |
|---|---|
| The six front-matter fields and their semantics (§2) | The prose wording inside a node's body |
| The status enum values `active`, `draft`, `deprecated` (§2.3) | Which nodes are linked from root vs. separate area nodes |
| The nearest-wins resolution algorithm (§3) | Whether a crate uses one node or several |
| The ≤ 50 rendered body-line sizing limit (§4) | The internal section headings of a node |
| The JIT read-order protocol (§5) | The grep/glob commands the agent uses to search |
| The write-protocol constraints: diff in PR, version bump, no autonomous mutation (§6) | The PR template, branch naming, or CI runner |

---

## 2. Node Front-Matter Schema (REQUIRED)

Every `AGENTS.md` node MUST open with a YAML front-matter block delimited by `---`
lines. The six fields MUST appear in this order:

```yaml
---
title: "<human-readable node name>"
version: "<semver string>"
status: <active | draft | deprecated>
owners: ["@<codeowners-handle>"]
last_verified_sha: "<7-char git SHA>"
derived_from: "<path or URL of authoritative source>"
---
```

### 2.1 `title` (REQUIRED)

A human-readable label for the node. Typically `"<repo or crate name> — <one-line scope>"`.
Consumed by agents as the node identity and by the Phase-6 stale linter for graph indexing.

### 2.2 `version` (REQUIRED)

A semver string (`MAJOR.MINOR.PATCH`). MUST be bumped on every node change that rides a
PR (§6). Patch bump for corrections; minor bump for new sections. Provides the Phase-6
linter a monotone staleness signal.

### 2.3 `status` (REQUIRED)

MUST be one of the three values:

- `active` — node is current and valid.
- `draft` — node is being authored; MAY be incomplete.
- `deprecated` — node is superseded; Phase-6 linter SHOULD skip stale-detection for
  deprecated nodes.

### 2.4 `owners` (REQUIRED)

A YAML array of CODEOWNERS-style handles (e.g. `["@handle1", "@handle2"]`). MUST include
at least one handle that matches a pattern in `.github/CODEOWNERS`. If the node-level
`owners` list and the CODEOWNERS file diverge, **CODEOWNERS is authoritative** (§6.3).

### 2.5 `last_verified_sha` (REQUIRED)

A 7-character git SHA that MUST be present in the repository's `main` branch ancestry.
This is the hook the Phase-6 stale linter (CTXGRAPH-04, out of scope here) will consume
to detect drift between the node's last verification and the current `main` HEAD. Authors
MUST NOT build the linter in Phase 5 — the field shape anticipates it.

Setting procedure (§6.2) and the squash-merge pitfall (§6.2) are specified in the Write
Protocol.

### 2.6 `derived_from` (REQUIRED)

A path or URL identifying the authoritative source this node distils. Examples:
`"docs/SPEC.md §11.6"`, `"README.md"`, `"https://example.com/spec"`. Supports DEC-004
traceability: the field records where to find the full context this node summarises.

### 2.7 `See also` — body bullet list (RECOMMENDED)

`See also` is **not a front-matter field**. It is a bullet list at the end of the node
body linking to related nodes and docs. Every contract-valid node SHOULD include a
`See also` section. (§Conformance Checklist item 7)

---

## 3. Nearest-Wins Resolution (REQUIRED)

The nearest-wins algorithm determines which `AGENTS.md` nodes an agent reads for a
given file path. It is **not** a recursive traversal of all nodes — it follows only
the path from root to the file being edited, plus acceptance-matching links.

**Resolution algorithm:**

1. Read the repository-root `AGENTS.md` — always, unconditionally. It is the single
   agent entry point per D-03.
2. Walk the directory tree from root toward the file being edited. Read each `AGENTS.md`
   found on that path. More-specific nodes override less-specific nodes; deeper nodes
   win. Root guidance applies everywhere unless a deeper node explicitly overrides it.
3. From the root node's module-ownership table, identify rows whose path-globs match
   files implicated by the current issue's acceptance criteria (by path, module name,
   or named capability). Follow ONLY those links. Do NOT follow links for paths not
   implicated by the issue.
4. For anything not yet discoverable via a linked node, use `grep -r` or `git ls-files`
   to locate it. (§5)

This is not a recursive traversal. The agent reads the root node plus at most the
area/crate nodes whose acceptance criteria match — typically two to four nodes total.

---

## 4. One-Concern / One-Screen Sizing (REQUIRED)

### 4.1 Hard sizing limit

A node's **rendered body** (excluding the front-matter YAML block) MUST contain ≤ 50
lines. Total file length MUST be ≤ 58 lines (50 body + ~8 front-matter). Authors SHOULD
verify with `wc -l <node>` before committing.

### 4.2 One concern per node

Each node MUST cover exactly one concern: one crate, one top-level directory, or one
discrete capability area. The root node is a navigational index (links + module-ownership
table) — NOT a prose description of the entire codebase. (See Pitfall: §8.1)

### 4.3 Module-ownership table format

Nodes that carry an ownership index SHOULD use the four-column table format:

```
| Capability | Path-glob | Owned By | Notes |
|------------|-----------|----------|-------|
```

Root node: crate-level path-globs (coarse). Per-crate nodes: sub-module globs (fine).
This hierarchy preserves nearest-wins semantics: coarse at root, fine-grained at the
area node.

### 4.4 No concrete data in this contract

This document (`docs/CONTEXT-CONTRACT.md`) contains **no real path-globs, no real owner
handles, and no populated module-ownership tables**. Those belong in the `AGENTS.md`
node instances. Illustrative examples are confined to the non-normative appendix (§8).

---

## 5. Read Protocol (REQUIRED)

The just-in-time (JIT) read order an agent MUST follow at the start of any session:

1. **Read root `AGENTS.md`** — always the first step.
2. **Consult the module-ownership table.** Identify which path-globs match files the
   issue's acceptance criteria explicitly reference (by path, module name, or named
   capability).
3. **Read ONLY the `AGENTS.md` nodes** linked from root whose path-globs match step 2.
   Do NOT read nodes for areas not implicated by the issue.
4. **For anything not yet linked:** use `grep -r '<term>' crates/` or
   `git ls-files | grep '<pattern>'`. Do NOT read `docs/` wholesale.

Agents MUST NOT open `README.md`, `CLAUDE.md`, or browse `docs/` unless a link in an
`AGENTS.md` node specifically directs them there. The blanket-slurp pattern (`Orient:
README.md, CLAUDE.md, docs/`) is DEPRECATED as of Phase 5. WORKFLOW state prompts MUST
use the JIT protocol above in place of any blanket orient step. (§7.3 of HARNESS-SPEC)

---

## 6. Write Protocol (REQUIRED)

### 6.1 Diffs ride the code PR (REQUIRED)

Node edits MUST ride in the same pull request as the code change that makes them
necessary. `CODEOWNERS` gates the PR with required human review — the agent MUST NOT
be able to self-merge a node change. Standalone doc-only PRs for `AGENTS.md` are
OPTIONAL but permitted when no code changes accompany the node update.

### 6.2 Version bump and `last_verified_sha` (REQUIRED)

Every node change MUST:

1. Increment the `version` field (§2.2) by at least a patch bump.
2. Update `last_verified_sha` (§2.5) to a short SHA present in `main` ancestry.

Procedure: after the PR is approved and ready to merge, run `git rev-parse --short HEAD`
on the branch tip and set `last_verified_sha` to that value. If the PR is squash-merged,
use the pre-merge branch-tip SHA — the Phase-6 linter treats any SHA reachable from
`main` ancestry as valid. Do NOT use an ephemeral SHA from a branch that is later
force-pushed.

### 6.3 CODEOWNERS is authoritative (REQUIRED)

The module-ownership table in each `AGENTS.md` node mirrors `.github/CODEOWNERS`. If
they diverge, `.github/CODEOWNERS` is authoritative. The `AGENTS.md` table is the
*agent-readable* representation; CODEOWNERS is the *enforced* mechanism. Authors MUST
keep them consistent; the Conformance Checklist (§Conformance Checklist item 9) gates
this.

### 6.4 No autonomous mutation (REQUIRED — DEC-004)

An agent that discovers a stale or incorrect node MUST NOT commit or push a node edit
outside a human-reviewed PR. The agent MUST propose a draft diff and stop. Autonomous
doc generation or self-learning node updates are PROHIBITED. This prohibition is
enforced by the CODEOWNERS human gate (§6.1) and is restated here as an explicit
invariant so that agents reading this contract see it directly.

---

## 7. Conformance (overview) (REQUIRED)

A repository is **contract-valid** when every `AGENTS.md` node it contains:

1. Carries all six front-matter fields in the required order. (§2)
2. Has `status` in `{active, draft, deprecated}`. (§2.3)
3. Lists at least one `owners` handle matching a CODEOWNERS entry. (§2.4)
4. Has `last_verified_sha` set to a 7-char SHA in `main` ancestry. (§2.5)
5. Has a rendered body ≤ 50 lines. (§4.1)
6. Covers exactly one concern. (§4.2)
7. Includes a `See also` bullet list. (§2.7)
8. Does not duplicate content available in a linked spec doc. (§4.4)
9. Has module-ownership path-globs that match `.github/CODEOWNERS` patterns. (§6.3)

And when write events (§6) are honored: diff in PR, version bump, CODEOWNERS gate,
no autonomous mutation.

---

## Conformance Checklist

A node is **contract-valid** when:

- [ ] Front-matter YAML block is present with all six required fields:
      `title`, `version`, `status`, `owners`, `last_verified_sha`, `derived_from`
- [ ] `status` is one of: `active`, `draft`, `deprecated`
- [ ] `owners` lists at least one owner matching a CODEOWNERS entry
- [ ] `last_verified_sha` is a 7-char git SHA present in `main` history
- [ ] Node body fits in one terminal screen (≤ 50 lines rendered, excluding front-matter)
- [ ] Body covers exactly one concern (one crate, one dir, one capability area)
- [ ] A "See also" link list is present at the end of the body
- [ ] The node does NOT duplicate content available in a linked spec doc
- [ ] If the node contains a module-ownership table, every row's path-glob
      matches a pattern present in `.github/CODEOWNERS`

---

## 8. Non-Normative Examples (non-normative)

### 8.1 Root node pitfall: prose instead of index

A root `AGENTS.md` that contains 200 lines of architectural overview violates §4.2.
The root node body MUST be: one orientation sentence, the module-ownership table, and
`See also` links. All prose belongs in the linked docs the table points to.

### 8.2 Illustrative front-matter (placeholder values only)

```yaml
---
title: "<crate name> — <one-line scope>"
version: "1.0.0"
status: active
owners: ["@<owner-handle>"]
last_verified_sha: "<7-char SHA>"
derived_from: "<path-to-spec §N>"
---
```

Replace all `<…>` placeholders with real values. Do NOT copy placeholder text into
production nodes. See `templates/AGENTS.md` for a ready-to-fill starter.

### 8.3 Prescribed vs. not-prescribed (summary)

| This contract mandates | Node author decides |
|---|---|
| Six front-matter fields, their order, their types | Prose wording in the node body |
| `status` ∈ `{active, draft, deprecated}` | Which area nodes link from root |
| Body ≤ 50 rendered lines | Number of rows in the ownership table |
| One concern per node | Internal headings beyond the ownership table |
| Write events: diff in PR, version bump, no self-merge | PR template, branch name, CI runner |
| CODEOWNERS is authoritative when tables diverge | Frequency of minor vs. patch bumps |
