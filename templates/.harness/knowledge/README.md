# `knowledge/` — compounded learnings

Where the compounding step writes learnings for future agents, in a consistent,
AI-indexable format. One learning per file.

**These writes are human-gated.** A `knowledge/` entry rides the same PR as the
code change that produced it and is approved at the review gate — never an
autonomous push. See [`../standards/compounding.md`](../standards/compounding.md).

## When to add an entry

Add one when the work surfaced something a future agent would otherwise rediscover:
a non-obvious constraint, a failed approach and why it failed, a dependency gotcha,
or a decision and its rationale. Do **not** record what the code, tests, or git
history already capture.

## Entry format

One file per learning, named `<short-kebab-slug>.md`, with this front-matter:

```markdown
---
title: <one line>
date: <YYYY-MM-DD>
tags: [<area>, <topic>]        # for retrieval — keep them stable
source_pr: <#NN or URL>        # the PR this learning rode in on
---

## What we learned

<the fact, stated so an agent can act on it without re-deriving it>

## Why it matters

<the cost of not knowing this — the failure it prevents>

## How to apply

<what to do differently next time; link related entries and standards>
```

Keep entries short and single-fact. Cross-link related entries and the relevant
`../standards/` file.
