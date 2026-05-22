---
name: setup-state-machine
description: Upgrade an existing WORKFLOW.md to the recommended Triage / Ready / Needs Fixes / Human Review state-machine pattern. Generates per-state prompts that consume the bridge's custom fields safely (every issue.fields.* reference uses `| default:` so a human can drag a ticket between states without breaking strict Liquid).
version: 1.0.0
---

# setup-state-machine

Upgrade an existing `WORKFLOW.md` with the four-state pattern that
Sinfonia is designed around:

| State | Who moves the ticket here | What Sinfonia does |
|---|---|---|
| **Triage** | Reporter | Agent reads, drafts a plan, asks clarifying questions in a comment, moves to **Ready** |
| **Ready** | Triage agent or human | Agent picks up the work, opens a PR |
| **Needs Fixes** | Bridge (after CI failure) or human | Agent reads `sinfonia_last_ci_failure`, fixes, re-pushes |
| **Human Review** | Bridge (max-attempts hit) or human | Sinfonia does nothing — a person decides |

## When to use

- A `WORKFLOW.md` exists but uses a single global prompt.
- The operator wants per-state behavior + the failure feedback loop wired up.

Prerequisite: `setup-bridge` should already be run so the bridge is actually
writing the custom fields the Needs-Fixes prompts read.

## Procedure

### 1. Read existing WORKFLOW.md

Load the file. If a `states:` block is already present and the operator
isn't sure why it's there, ask before overwriting.

### 2. Confirm state names

Surface the operator's existing `tracker.active_states`. Map each to one
of the four canonical roles:

- `Triage` → role: `triage`
- `Ready` → role: `ready`
- `Needs Fixes` → role: `needs_fixes`
- `Human Review` → role: `human_review`

If the operator's tracker uses different state names ("Backlog" ≈ Triage,
etc.), let them map by hand. Persist the mapping for step 3.

### 3. Generate per-state prompts

Render `templates/state-machine.liquid` (the full `states:` block) and
`templates/needs-fixes-prompt.liquid` / `templates/needs-fixes-e2e-prompt.liquid`
(per-state prompt bodies).

**Critical invariant:** every `{{ "{{ issue.fields.X }}" }}` reference in
a generated prompt MUST be followed by `| default:`. Strict Liquid (used
by `crates/sinfonia/src/template.rs`) errors on absent fields; the
`| default:` filter is what makes the prompt safe when a human drops a
ticket into Needs Fixes without any prior bridge run.

Run this grep against every rendered prompt and fail loudly on a hit:

```bash
grep -nE '\{\{[^}]*issue\.fields\.[^|]*\}\}' rendered/*.liquid
```

A non-empty result means a template emitted an unguarded reference and
needs to be fixed before writing the result to disk.

### 4. Failure-category split (optional)

If the operator enabled failure categorization in the bridge (`feedback_loop.failure_categories`
in `BRIDGE.md`), generate per-category Needs-Fixes states:

- `Needs Fixes - Lint`
- `Needs Fixes - Unit Test`
- `Needs Fixes - E2E`
- `Needs Fixes - Build`

Each gets its own prompt template (use `templates/needs-fixes-e2e-prompt.liquid`
as the model — adapt the failure-summary framing for the category).

### 5. Validate

Run `sinfonia --check WORKFLOW.md` (per `setup-workflow` step 9).

### 6. Commit

`git add WORKFLOW.md && git commit -m "Add state-machine pattern"`.

## Templates

- `templates/state-machine.liquid` — the `states:` block (front-matter
  inset, references the per-state prompt files).
- `templates/needs-fixes-prompt.liquid` — generic Needs-Fixes prompt.
  Reads `issue.fields.sinfonia_last_ci_failure | default: ...` and
  `issue.fields.sinfonia_attempt_count | default: 0`.
- `templates/needs-fixes-e2e-prompt.liquid` — E2E-specific variant.

## Verification

The §8 deliverable checklist (`docs/v0.3-plan/05-skills-cli.md`) includes
a grep check that the rendered templates contain no unguarded
`issue.fields.*` references. The skill's own templates pass this check;
the operator's edits to the rendered output do not.
