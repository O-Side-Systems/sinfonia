---
title: "Stale-Node Linter Test Fixture"
version: "1.0.0"
status: active
owners: ["@osidesys", "@leebrett"]
last_verified_sha: "e51aff0"
derived_from: "tests/lint/run-stale-test.sh"
---

# Stale-Node Linter Test Fixture

This is a minimal AGENTS.md node used exclusively by the stale-node linter
behavior tests in `tests/lint/run-stale-test.sh`. It owns only the
`tests/lint/stale-node-fixture/` directory — a path that never churns in
normal development, so this node will never appear stale under normal
conditions.

The `last_verified_sha` is set to the commit that authored this file. The
test runner sets `STALE_COMMIT_THRESHOLD=9999` to ensure the threshold never
triggers even if the fixture directory accumulates a few commits over time.

## Module Ownership

| Capability | Path-glob | Owned By | Don't Roll Your Own |
|------------|-----------|----------|---------------------|
| Stale-linter test fixture | `tests/lint/stale-node-fixture/` | @osidesys @leebrett | — |

## See also

- [`tests/lint/run-stale-test.sh`](../run-stale-test.sh) — the test that uses this fixture
- [`scripts/lint-stale-nodes.sh`](../../../scripts/lint-stale-nodes.sh) — the linter under test
