# Review gate

> The exit gate for the **Review** step of the execution loop. A reviewer agent
> inspects the work and leaves comments; the developer agent addresses each one.
> They loop until clean, then the work is handed to the human gate.

A review passes when:

- [ ] **Every reviewer comment resolved** — addressed in code or answered, none
      left open.
- [ ] **Plan satisfied** — the build delivers what the plan promised; no scope
      silently dropped or added.
- [ ] **Gates green** — all required checks pass, including the harness sensor
      (`docs/HARNESS-SPEC.md §4–§7`).
- [ ] **Mergeable** — no conflict against the base branch (`docs/HARNESS-SPEC.md
      §7.4`).

When clean, hand off to the **human gate**:

- **Approve** → ship.
- **Request changes** → back to the execution loop.
- **Don't ship** → drop.

Human approval is required to merge; the agent cannot self-merge.

<!-- Add project-specific review requirements below. -->
