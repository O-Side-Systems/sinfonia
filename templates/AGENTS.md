---
title: "<repo name> — Agent Entry Point"
version: "1.0.0"
status: draft
owners: ["@<owner1>"]
last_verified_sha: "<7-char git SHA of last verified commit>"
derived_from: "README.md"
---

# <!-- replace: one-sentence project orientation -->

## Module Ownership

| Capability | Path-glob | Owned By | Don't Roll Your Own |
|------------|-----------|----------|---------------------|
| <!-- replace: add one row per shared capability --> | | | |

## Before you build

Read [`.harness/`](.harness/) first — it holds how this repo builds and the gate
each step must meet:

- [`.harness/standards/`](.harness/standards/) — coding, architecture + ADRs,
  documentation, compounding. **How** we build.
- [`.harness/criteria/`](.harness/criteria/) — the exit gate for the step you're
  on: `plan`, `build`, `review`.
- [`.harness/knowledge/`](.harness/knowledge/) — learnings from prior work.

Write learnings back to [`.harness/knowledge/`](.harness/knowledge/) **in the same
PR as the code change** — never autonomously. See
[`.harness/standards/compounding.md`](.harness/standards/compounding.md).

## See also

- [`.harness/README.md`](.harness/README.md) — the workspace: standards, gates, memory
- <!-- replace: link to your spec docs + this template's source contract -->
