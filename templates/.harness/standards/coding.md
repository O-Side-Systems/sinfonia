# Coding standards

> HOW we write code in this repo. Durable rules the agent must honor and the plan
> must reference explicitly. Keep this short and enforceable — link to tooling
> config (formatter, linter) rather than restating it.

## Language & tooling

- **Language / version:** <!-- replace -->
- **Formatter:** <!-- replace, e.g. `cargo fmt` / `prettier` — config is authoritative -->
- **Linter:** <!-- replace, e.g. `clippy -D warnings` / `eslint` -->

## Rules

- **DRY / reuse.** Before writing shared or utility code, check for an existing
  implementation; don't roll your own. <!-- point at the relevant module-ownership table -->
- **Error handling:** <!-- replace -->
- **Naming & structure:** <!-- replace -->
- **Banned constructs:** <!-- replace, or "none" -->

## Tests

- New behavior ships with tests in the same change (unit / integration / e2e as
  appropriate). See the Build gate ([`../criteria/build.md`](../criteria/build.md)).
