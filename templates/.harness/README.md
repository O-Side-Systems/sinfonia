# `.harness/` — the workspace

This directory is the repo's **durable, agent-readable memory**: how this project
builds, what each step of the execution loop must satisfy, and why prior work went
the way it did. It informs the agent *before* it writes code, and the agent (and
its reviewers) write learnings back into it as work compounds.

Its shape is **prescribed** — identical in every repo driven by Sinfonia — so an
agent or a person dropped into any such repo finds the rules and the gates in
exactly the same place. See `docs/HARNESS-SPEC.md §11` (in the Sinfonia repo) for
the normative specification.

```
.harness/
├── standards/            ← HOW we build (durable rules; change rarely)
│   ├── coding.md
│   ├── architecture/        ← architecture standards + ADRs
│   ├── documentation.md
│   └── compounding.md
├── criteria/             ← the EXIT GATES each execution-loop step must meet
│   ├── plan.md
│   ├── build.md
│   └── review.md
└── knowledge/            ← compounded learnings, in an AI-indexable format
```

## How agents use it

**Read first.** Before planning or building, read the relevant `standards/`, the
`criteria/` file for the step you're on, and any `knowledge/` entries the issue
touches. Read just-in-time — only what the current issue needs.

**Write back, human-gated.** The compounding step writes learnings to
`knowledge/`. Every `.harness/` edit MUST ride the **same pull request** as the
code change that produced it and is approved at the human review gate — see
[`standards/compounding.md`](standards/compounding.md). **Autonomous, self-learning
writes outside a reviewed PR are prohibited.**

> Replace the bracketed placeholders throughout these files with your project's
> real standards, gates, and conventions. The layout is fixed; the prose is yours.
