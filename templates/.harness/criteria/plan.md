# Plan gate

> The exit gate for the **Plan** step of the execution loop. Before any code is
> written, the validator confirms the plan earns the build. The plan does not pass
> until every item below is present.

A plan passes when it includes:

- [ ] **Tests to create** — unit, integration, or e2e as appropriate, named
      against the behavior they prove.
- [ ] **Documentation to update** — what gets written or revised, and where (per
      [`../standards/documentation.md`](../standards/documentation.md)).
- [ ] **Coding & project standards** — referenced explicitly
      ([`../standards/coding.md`](../standards/coding.md); e.g. the DRY/reuse rule).
- [ ] **Architecture standards & ADRs** — the decisions this work must respect
      ([`../standards/architecture/`](../standards/architecture/)); a new ADR if
      this work makes one.
- [ ] **Acceptance-criteria mapping** — each acceptance criterion of the story
      maps to something the plan delivers and a test that proves it.
- [ ] **A compounding step** — how learnings get captured back to
      [`../knowledge/`](../knowledge/) (see
      [`../standards/compounding.md`](../standards/compounding.md)).

<!-- Add project-specific plan requirements below. -->
