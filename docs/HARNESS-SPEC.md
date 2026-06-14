# Harness Authoring Specification

- **Status:** *Recommended pattern.* This document specifies how a **target
  repository** produces the test-feedback contract that Sinfonia's bridge
  consumes (`docs/SPEC.md` §11.6, Proposal 0001). It is the **producer** half of
  a two-sided contract; Sinfonia is the **consumer**.
- **Audience:** authors bootstrapping a new project to be driven by Sinfonia's
  agentic SDLC loop.
- **Companion specs:** `docs/SPEC.md` (the orchestrator + bridge contract),
  `docs/proposals/0001-harness-feedback-ingestion.md` (the consumer of the
  `bridge.json` manifest defined here).

## Normative Language

The key words MUST, MUST NOT, REQUIRED, SHOULD, SHOULD NOT, RECOMMENDED, MAY, and
OPTIONAL are to be interpreted as in RFC 2119.

A repository is **Sinfonia-ready** when it satisfies every MUST in §3 and §7.

---

## 1. Purpose

Sinfonia drives an inner agent loop that turns a tracker issue into a green pull
request. To do that it needs a **deterministic sensor**: an executable
specification the agent's changes are graded against, emitting machine-readable,
agent-diagnosable results. This document specifies that sensor — the *harness* —
as a set of **contracts and invariants**, not an implementation.

The harness is **meta to the product**: it builds the product, it is not part of
it. It is the thing that tells the loop "you are not done yet, and here is
precisely why."

### 1.1 What this specification fixes, and what it leaves open

The opinionated surface is deliberately narrow — three pillars (§4–§6) and one
interface (§7). Everything else is the target repo's choice.

| Prescribed (this spec) | NOT prescribed (target repo's choice) |
|---|---|
| The loop *structure*: spec → test → verify → feed back (§4) | The spec language, the agent runtime, the prompt wording |
| The four-artifact result *contract* and its schema (§5) | The test framework, runner, language, browser/driver |
| Structured, agent-readable failure *fields* (§5.3) | How those strings are produced |
| The determinism *NFR* (§5.4) | How isolation/repeatability is achieved |
| The observability-feedback *contract* (§6) | Which telemetry backends or query dialects |
| The Sinfonia interface: `bridge.json`, bundle, conventions (§7) | Everything behind those interface points |

A conforming harness for a Python/FastAPI + React app, a Plotly/Dash analytics
app, and the reference TypeScript/Playwright implementation are all equally
valid — they differ only in the substrate, never in the contract. §8 works two of
these examples explicitly.

## 2. Producer / Consumer Split

```
  ┌─────────────────────────────┐         ┌──────────────────────────────┐
  │   TARGET REPO (producer)     │         │   SINFONIA (consumer)        │
  │   — this specification —     │         │   — docs/SPEC.md §11.6 —     │
  │                              │         │                              │
  │  spec-deriving step  (§4.1)† │         │  bridge: reads bridge.json   │
  │  outside-in tests    (§4.2)  │  CI     │  → failure digest → prompt   │
  │  four-artifact bundle (§5)   │ ───────►│  → category routing          │
  │  bridge.json manifest (§7.1) │ artifact│  → attempt/budget caps       │
  │  observability feed   (§6)   │  + PR   │  orchestrator: runs agent    │
  └─────────────────────────────┘         └──────────────────────────────┘
```

The producer emits artifacts and a manifest into CI. The consumer reads them and
decides whether to re-dispatch the agent. Neither reaches into the other: the
target repo never calls Sinfonia's API, and Sinfonia never assumes anything about
the harness beyond §7.

† The spec-deriving step is **OPTIONAL** (§4.1) — the agent takes its
instructions from the tracker. Every other producer box is required.

## 3. Conformance (overview)

A Sinfonia-ready harness MUST:

1. Author tests **outside-in** — against behavior that does not yet exist (§4.2).
2. Emit the **four-artifact contract** per scenario, unconditionally, pass or
   fail (§5.1–§5.2).
3. Express failures as **structured `step` / `assertion` strings** (§5.3).
4. Satisfy the **determinism NFR** (§5.4).
5. Assemble a **`bridge.json` manifest** at `schema_version 2` (§7.1).
6. Honor the **repository conventions** Sinfonia drives by (§7.3).

It SHOULD additionally provide architectural-invariant gating (§5.5) and the
observability feedback contract (§6).

It MAY provide a natural-language → executable-specification step (§4.1) for
bootstrapping a repo from prose. This is **OPTIONAL**: Sinfonia's agent takes
its per-issue instructions from the tracker, not from a generated spec, so a
conforming harness MAY hand-author its executable specs and omit the
spec-deriving step entirely.

---

## 4. Pillar 1 — The Agentic SDLC Loop Structure

### 4.1 Spec-deriving step (OPTIONAL)

A harness MAY include a step that turns a natural-language input (prose,
transcript, shorthand) into an **executable specification** plus a human-readable
decomposition and an explicit record of gaps. This is a convenience for
bootstrapping a repo from prose; **Sinfonia does not depend on it.** The agent
takes its per-issue instructions from the tracker, so a conforming harness MAY
instead hand-author its executable specs (§4.2) and omit this step. The
reference implementation (BCF) ships one because it bootstraps features from
transcripts and shorthand, not because the loop requires it.

When a harness *does* provide this step:

- It MUST write **files only** — no writes to the tracker, version control host,
  or a database. Idempotency and testability depend on this.
- It MUST emit, for each input:
  - an **executable spec** (one or more files the harness can run as tests),
  - a **decomposition** (epics/stories or equivalent, cross-referenced to the
    spec),
  - an **open-questions log** — every assumption or gap, each with a defensible
    default and rationale.
- On re-run against an existing input it MUST NOT silently overwrite a
  human-reviewed spec; it SHOULD write to a side location for diff inspection.

The spec *language* is unconstrained. Gherkin is one choice; a table of
input/expected pairs, a notebook of assertions, or a typed scenario DSL are
equally conformant as long as the harness can execute them.

### 4.2 Outside-in authoring (REQUIRED)

Scenarios MUST be authored against behavior that **does not yet exist** — the
spec is written first and is expected to fail until the agent implements the
feature. The harness is the executable definition of "done," not a regression net
bolted on afterward.

### 4.3 Loop runner (RECOMMENDED)

The harness SHOULD provide a re-run loop with two explicit termination
conditions:

- **terminate-on-clean** — stop when the targeted scenarios pass;
- **terminate-on-cap** — stop after a configured maximum number of iterations.

The cap is a cost-control boundary; reaching it is a normal, reportable outcome,
not an error.

### 4.4 Escalation path (RECOMMENDED)

When the loop cannot reach green within its cap, the harness SHOULD emit an
explicit escalation signal (a dedicated artifact and a reference in the result —
see `escalation_ref` in §5.2) so the consumer can route the ticket to a blocked /
human-review state rather than looping indefinitely. This composes with
Sinfonia's attempt and budget caps (SPEC §11.6).

## 5. Pillar 2 — Testing & Verification Contract

### 5.1 The four-artifact contract (REQUIRED)

Every scenario run MUST emit, **unconditionally on both pass and fail**, four
artifacts with these stable role names:

| Role | Content | Reference filename |
|---|---|---|
| `result` | The machine-readable outcome (§5.2) | `result.json` |
| `trace` | A replayable execution trace | `trace.zip` |
| `video` | A visual recording of the run | `video.webm` |
| `a11y` | An accessibility / semantic snapshot of the surface under test | `a11y.json` |

The *role names* are normative; the reference *filenames* are RECOMMENDED and are
what the reference consumer expects in `artifact_urls`. The artifact **formats**
are not prescribed — `trace.zip` may be any replayable trace, `a11y.json` any
structured semantic snapshot. For a non-UI target (e.g. a data pipeline),
`video`/`a11y` MAY be a rendered output snapshot and a schema/semantic dump
respectively; the contract is "four artifacts, always, including a visual and a
semantic one," not "a browser video."

Artifacts MUST be laid out one directory per scenario so the consumer can address
each scenario's bundle independently:

```
runs/<run-id>/<scenario-slug>/{result.json, trace.zip, video.webm, a11y.json}
```

### 5.2 `result.json` schema (REQUIRED)

`result.json` MUST be a JSON object carrying at least:

```jsonc
{
  "schema_version": <integer>,        // REQUIRED, present from day one
  "scenario": "<human label>",        // REQUIRED
  "passed": <bool>,                   // REQUIRED
  "duration_s": <number>,             // REQUIRED
  "feature_file": "<spec path>",      // RECOMMENDED — the executable spec source
  "failed_step": "<structured>",      // REQUIRED when passed=false (§5.3)
  "assertion": "<structured>",        // REQUIRED when passed=false (§5.3)
  "url_at_failure": "<surface ref>",  // OPTIONAL
  "obs_endpoints": { … },             // OPTIONAL (§6)
  "escalation_ref": "<artifact name>",// OPTIONAL (§4.4)
  "artifacts": { "trace": "trace.zip", "video": "video.webm", "a11y": "a11y.json" }
}
```

- `schema_version` MUST be an integer present from the first release.
  Version bumps MUST be **additive** (new optional fields) so consumers can
  forward-read; an incompatible change MUST increment the version and SHOULD keep
  the prior version readable for one cycle. The reference implementation is at
  `schema_version 5`; the Sinfonia interface (§7) is pinned at the `bridge.json`
  level, decoupling it from `result.json` minor evolution.
- The typed source of truth MAY be a language-native type rather than a JSON
  Schema, provided `schema_version` is emitted.

### 5.3 Agent-readable failures (REQUIRED)

`failed_step` and `assertion` MUST be **structured strings**, not free-form
exception text or stack traces. They MUST describe *what was expected and what was
observed* in terms stable enough for an agent to act on without re-running
locally. Example:

```
step:      Then the tenant list shows "Acme"
assertion: Expected element [data-testid='tenant-row-acme'] to be visible;
           was not present in DOM
```

This is the load-bearing NFR of the whole pattern: the artifacts MUST be
sufficient to **diagnose a failure without re-running it locally**. Raw framework
error dumps do not satisfy this.

### 5.4 Determinism (REQUIRED)

For a fixed code state, the harness MUST return an identical pass/fail verdict
across repeated runs — **N-of-N identical** (RECOMMENDED N ≥ 20). Any storage- or
environment-keyed value (run id, timestamps, ports) MUST NOT influence the
verdict. Non-determinism sends the agent to fix the wrong thing and is a
cost defect, not a flake to tolerate. A conforming harness SHOULD ship a
determinism checker that asserts this.

Flake-retry policy (e.g. a bounded retry on explicitly-tagged scenarios) is
permitted but MUST NOT mask non-determinism in load-bearing scenarios.

### 5.5 Architectural-invariant gating (RECOMMENDED)

The harness SHOULD enforce structural invariants the agent must not violate
(layer/dependency direction, banned constructs, cross-cutting access rules) via a
manifest-driven linter that **fails loud** both locally and in CI. It MAY
additionally compute a soft quality grade and open bounded, deduplicated,
no-auto-merge refactor PRs against below-threshold areas. These keep an
autonomous loop from trading structure for green.

## 6. Pillar 3 — Observability Feedback Contract

This pillar answers prompts like "ensure service startup completes under 800ms"
that cannot be verified from the UI surface alone.

- The harness SHOULD stand up an **isolated, per-workspace observability stack**
  for the system under test, so concurrent agent workspaces never collide and no
  production telemetry is required.
- The system under test SHOULD emit telemetry tagged with a **workspace
  identifier** resource attribute, so signals are filterable per workspace.
- The harness SHOULD expose **standard query endpoints** (logs / metrics /
  traces) and surface them in `result.json.obs_endpoints` (§5.2) using a
  vendor-neutral query surface — raw queries against standard endpoints, no
  proprietary dialect lock-in.
- **The consumer of these endpoints is the agent, not Sinfonia.** Scenarios query
  them as part of asserting behavior; the orchestrator never reads them. This
  keeps observability feedback inside the deterministic-sensor boundary.

Isolation invariants the stack MUST honor: bind to loopback only, allocate ports
deterministically from the workspace id (not randomly), and record live
endpoints/pids in a workspace-scoped lockfile so boot is idempotent and teardown
is crash-safe.

The specific backends and query languages are unconstrained. OpenTelemetry +
LogQL/PromQL/TraceQL is one conforming choice; any stack exposing standard
queryable log/metric/trace endpoints qualifies.

## 7. The Preserved Sinfonia Interface (REQUIRED)

These are the contract points Sinfonia consumes. They MUST be preserved exactly,
regardless of the substrate behind them. This is the non-negotiable core of
"Sinfonia-ready."

### 7.1 `bridge.json` manifest (`schema_version 2`)

On each CI run, the harness MUST assemble a single `bridge.json` summarizing
failures, and upload it as a CI artifact (default name `bridge-<run-id>`,
matching the consumer's `harness_manifest_artifact_glob`):

```jsonc
{
  "schema_version": 2,                      // REQUIRED — the pinned interface version
  "pr_number": <int|null>,
  "branch": "<head ref>",
  "commit_sha": "<sha>",
  "run_url": "<CI run url>",                // REQUIRED — where the bundle lives
  "artifact_bundle_name": "<bundle name>",  // REQUIRED — the four-artifact bundle artifact
  "failures": [                             // REQUIRED — empty array when green
    {
      "scenario": "<label>",                // REQUIRED
      "feature_file": "<spec path|null>",
      "step": "<structured|null>",          // §5.3
      "assertion": "<structured|null>",     // §5.3
      "artifact_urls": {                    // bundle-relative references
        "result": "<dir>/result.json",
        "trace":  "<dir>/trace.zip",
        "video":  "<dir>/video.webm",
        "a11y":   "<dir>/a11y.json"
      }
    }
  ]
}
```

`schema_version` MUST be `2` to match the consumer's
`SUPPORTED_BRIDGE_MANIFEST_VERSIONS`. Additive evolution is permitted; the
consumer forward-reads known fields (Proposal 0001 §4.3).

### 7.2 CI check naming (REQUIRED)

The CI check that runs the harness gate MUST have a name that matches the
consumer's `failure_categories` routing pattern, so failures route to the correct
tracker state. The reference consumer routes `(?i)(e2e|playwright|harness)` →
`Needs Fixes - E2E`; the reference producer names its gate
`e2e harness (@smoke gate)`. A target repo MAY define its own categories, but the
check name and the configured pattern MUST agree.

### 7.3 Repository conventions (REQUIRED)

- **Branch:** agent work lands on `sinfonia/<issue-id>` branches.
- **PR body:** MUST contain a tracker-identifier line the bridge's
  `pr_link_pattern` matches (default `Resolves <ID>` /
  `(?i)(?:closes|fixes|resolves)\s+([A-Z]+-\d+|[a-z]+-\d+)`).
- **Labels:** the `sinfonia:*` label namespace is **bridge-owned**; the repo MUST
  NOT manually manage those labels.
- **CODEOWNERS:** a human-review gate MUST cover Sinfonia-touched paths; the agent
  can satisfy checks and address comments but MUST NOT be able to self-merge.
- **CI gates:** the harness gate (and any invariant linters) MUST block merge on
  failure.
- **Context graph:** The repo MUST maintain a hierarchical `AGENTS.md` doc-graph
  conforming to `docs/CONTEXT-CONTRACT.md`. The root `AGENTS.md` is the agent
  entry point; all node edits ride in the code PR under CODEOWNERS. See
  [`docs/CONTEXT-CONTRACT.md`](docs/CONTEXT-CONTRACT.md) for the full contract.

### 7.4 Merge gating (REQUIRED)

Green CI is necessary but not sufficient to merge; the CODEOWNERS human gate
(§7.3) is the terminal authority. The harness gate's job is to make "green" *mean*
something — see the determinism NFR (§5.4) and load-bearing scenario tagging
(§5.5).

**Merge queue.** The target repo SHOULD enable a GitHub native merge
queue configured to rebase-and-test each PR against the latest `main` before
merging. This ensures that a PR that was green when submitted is still green once
integrated with concurrent work. Method: "Rebase and merge"; all required status
checks (including the harness gate) must pass after the rebase.

**Post-merge harness gate.** The harness gate MUST also run on `main` after every
merge (a CI workflow triggered on `push` to `main`). A green-at-PR-time change
that breaks once integrated with concurrent work is caught by this gate before the
next agent dispatch sees a broken base. Gate failure MUST alert operators.

**Mergeable-not-CLEAN gate refinement.** For agent workflows the
pre-`In Review` gate is "mergeable w.r.t. `main`" — specifically,
`mergeStateStatus` is anything *except* `DIRTY` or `BEHIND`. `BLOCKED` (awaiting
required-review approval) and `UNSTABLE` (non-required checks failing) count as
conflict-free and SHOULD trigger the `In Review` transition; required-review
approval is the human gate that happens *in* the `In Review` state. Only `DIRTY`
or `BEHIND` keep the mergeability loop running; `UNKNOWN` (GitHub still computing)
triggers a re-poll. **Explicit note:** this refines a literal "only when
`mergeStateStatus == CLEAN`" reading (including the MERGE-02 success-criterion
literal-CLEAN wording). A fresh agent PR awaiting required review is `BLOCKED`,
never `CLEAN` — gating literally on `CLEAN` deadlocks the agent against the very
branch-protection this section mandates. The gate is "no merge conflict against
`main`"; human approval is not the agent's gate.

**Serial-foundation / leaf-fan-out convention.** Foundational or
cross-cutting stories in a milestone run serially: one story must land on `main`
before the next begins. Only leaf stories (no shared-surface dependencies within
the milestone) may fan out in parallel. This prevents merge-conflict cascades on
shared code and is enforced at the dispatch layer by
`agent.max_concurrent_agents_by_state: "In Progress": 1` in `WORKFLOW.md`. Leaf
stories are identified during milestone decomposition and may raise this limit
when the milestone graph confirms non-overlapping surface.

---

## 8. Portability — Worked Examples

The same contract, two non-reference substrates. Only the implementation column
changes; the contract column is fixed.

### 8.1 Python / FastAPI + React

| Contract (§) | Reference (TS/Playwright) | This stack |
|---|---|---|
| Spec-deriving step (§4.1) | skill → Gherkin `.feature` | skill → `pytest`-readable scenario files + stories + OQ log |
| Executable spec (§4.2) | Cucumber.js | `pytest` + `pytest-bdd` (or plain parametrized tests) |
| Four artifacts (§5.1) | Playwright trace/video + aria snapshot | Playwright-Python trace/video + `axe`/DOM snapshot → same four role names |
| `result.json` (§5.2) | TS typed writer | a `pydantic` model `.model_dump_json()`, same fields |
| Determinism (§5.4) | `determinism.mjs`, 20× | a `pytest` rerun harness, 20× |
| Observability (§6) | OTel + Loki/Prom/Tempo | identical stack, or the team's existing OTel collector |
| `bridge.json` (§7.1) | github-script step | a Python step in CI writing the same JSON |

### 8.2 Plotly / Dash analytics app

| Contract (§) | This stack |
|---|---|
| Spec-deriving step (§4.1) | skill → scenarios over expected figures/tables + OQ log |
| Executable spec (§4.2) | `pytest` driving Dash via `dash.testing` + Selenium/Playwright |
| Four artifacts (§5.1) | `trace` = interaction log; `video` = recorded callback session; `a11y` = serialized figure/component tree + ARIA; `result.json` as §5.2 |
| Structured failures (§5.3) | `assertion: Expected figure 'revenue-by-region' to have 4 traces; got 3` |
| Determinism (§5.4) | seed data fixtures; 20× verdict check |
| Observability (§6) | per-workspace OTel around the Dash server; scenarios assert callback latency via PromQL |
| `bridge.json` (§7.1) | CI step emits the identical `schema_version 2` shape |

The interface points (§7) are byte-for-byte identical across all three. That is
the portability guarantee.

## 9. Conformance Checklist

A repo is **Sinfonia-ready** when:

- [ ] Scenarios are authored outside-in, against behavior that does not yet
      exist (the executable spec exists, however it was produced). (§4.2)
- [ ] Every scenario emits four artifacts, pass or fail, one dir per scenario.
      (§5.1)
- [ ] `result.json` carries `schema_version`, `scenario`, `passed`,
      `duration_s`, and structured `failed_step`/`assertion` on failure. (§5.2–§5.3)
- [ ] A determinism check passes N-of-N for a fixed code state. (§5.4)
- [ ] CI assembles `bridge.json` at `schema_version 2` and uploads it +
      the four-artifact bundle. (§7.1)
- [ ] The harness CI check name matches the bridge's `failure_categories`
      pattern. (§7.2)
- [ ] `sinfonia/<id>` branches, a `Resolves <ID>` PR-body line, bridge-owned
      `sinfonia:*` labels, and a CODEOWNERS human-merge gate are in place. (§7.3)
- [ ] A GitHub native merge queue is configured for rebase-and-test, and a
      post-merge harness gate runs on `main` (push trigger). (§7.4)
- [ ] For agent workflows, the agent prompt applies the mergeable-w.r.t.-`main`
      gate — looping only on `DIRTY`/`BEHIND` and treating `BLOCKED`/`UNSTABLE`
      as ready-for-human. (§7.4)
- [ ] A root `AGENTS.md` exists and conforms to `docs/CONTEXT-CONTRACT.md`;
      CODEOWNERS gates all `**/AGENTS.md` edits. (§7.3)
- [ ] *(RECOMMENDED)* architectural-invariant gating (§5.5) and the observability
      feedback contract (§6).
- [ ] *(OPTIONAL)* a natural-language → executable-specification step for
      bootstrapping from prose. (§4.1)

## 10. Reference Implementation (non-normative)

The BCF admin-UI harness is one concrete instantiation of this specification:
TypeScript + Cucumber.js driving Playwright (Chromium), a Next.js system under
test, Rust services, OpenTelemetry + Loki/Prometheus/Tempo for §6, and GitHub
Actions assembling `bridge.json`. It is cited throughout as *an* example, never
as the rule — every framework-specific choice there is substitutable per §8
without changing a single interface point in §7.

---

## Appendix A — Field reference

**`bridge.json` (consumed by Sinfonia, §7.1):** `schema_version` (=2),
`run_url`, `artifact_bundle_name`, `failures[]` of
`{scenario, feature_file?, step?, assertion?, artifact_urls?}`. See Proposal
0001 §10 for the exact consumed subset.

**`result.json` (per scenario, §5.2):** `schema_version`, `scenario`, `passed`,
`duration_s`, `feature_file?`, `failed_step?`, `assertion?`, `url_at_failure?`,
`obs_endpoints?`, `escalation_ref?`, `artifacts{trace,video,a11y}`.
