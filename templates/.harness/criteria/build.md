# Build gate

> The exit gate for the **Build** step of the execution loop. The worker builds to
> the approved plan; the validator checks the build against it. Worker and
> validator iterate until the build matches the plan with no gaps.

A build passes when:

- [ ] **Code, tests, and docs land together** — the change implements the approved
      plan, nothing planned is missing.
- [ ] **Tests written and passing** — every test the plan named exists and is
      green; gating scenarios pass (see `docs/HARNESS-SPEC.md §5.6`).
- [ ] **Standards honored** — coding
      ([`../standards/coding.md`](../standards/coding.md)) and architecture
      ([`../standards/architecture/`](../standards/architecture/)) standards are
      respected; no banned constructs.
- [ ] **Documentation updated** — exactly the docs the plan named.
- [ ] **Learnings compounded** — any knowledge worth carrying forward is written to
      [`../knowledge/`](../knowledge/) in this same PR.

<!-- Add project-specific build checks below (e.g. required CI gates). -->
