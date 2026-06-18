# Compounding standard

> HOW learnings are written back so each agent makes the next one smarter. The Plan
> gate requires a compounding step; this file defines what that step does and the
> hard rule it must obey.

## The rule (non-negotiable)

Every write to `.harness/` — and especially to [`../knowledge/`](../knowledge/) —
MUST ride the **same pull request** as the code change that produced the learning,
and is approved at the human review gate under `CODEOWNERS`.

**Autonomous, self-learning writes outside a human-reviewed PR are PROHIBITED.**
An agent proposes the diff; a human approves it. This is the same write protocol
the `AGENTS.md` doc-graph uses (see `docs/HARNESS-SPEC.md §11.4` and
`docs/CONTEXT-CONTRACT.md §6` in the Sinfonia repo).

## When to compound

Write a `knowledge/` entry when the work surfaced something a future agent would
otherwise have to rediscover: a non-obvious constraint, a failed approach and why,
a gotcha in a dependency, a decision and its rationale.

Do **not** compound what the code, tests, or git history already record.

## Entry format

See [`../knowledge/README.md`](../knowledge/README.md) for the entry template and
the AI-indexable front-matter.
