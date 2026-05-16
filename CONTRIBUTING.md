# Contributing to Sinfonia

Thanks for your interest in helping out. This document covers the basics; the [README](README.md) covers what the project is and how to run it.

## Ground rules

- Be kind. Assume good faith. Code review is about the code, not the author.
- Discussions and PRs are public. If you can't share something publicly (vulnerabilities, security incidents), see [SECURITY.md](SECURITY.md) instead.
- By contributing, you agree that your contributions are licensed under [Apache-2.0](LICENSE) (the project license). No CLA is required.

## Before you open a PR

1. **Open an issue first** for anything larger than a bug fix or a one-line change. We'd rather agree on the shape before you spend hours writing code.
2. **Check the spec.** Sinfonia conforms to [`docs/SPEC.md`](docs/SPEC.md) (the upstream Symphony Service Specification). Changes that drift from the spec need to be either spec-conformant extensions (call them out as such) or explicit deviations with a justification in the PR description.
3. **Keep the diff scoped.** One concern per PR. If your branch touches the orchestrator state machine *and* the Jira adapter *and* the README, split it.

## Development loop

```bash
# build
cargo build

# fast test loop
cargo test

# release build (the binary the README documents)
cargo build --release

# clippy + format before submitting
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

CI runs the same commands. PRs that don't pass `cargo fmt` or `cargo clippy` will be sent back.

### Local end-to-end test

If you have a Linear or Jira account and an LLM key, run against a sandbox project:

```bash
export LINEAR_API_KEY=lin_api_...
export ANTHROPIC_API_KEY=sk-ant-...
./target/release/sinfonia ./WORKFLOW.md --port 8080
```

Move a test ticket into `Todo` and watch the dashboard at <http://127.0.0.1:8080/>. Don't aim a real model at a real project unless you mean to.

## Style

- **Rust:** `cargo fmt` (default profile). 4-space indent, no manual formatting flourishes. Prefer `?` over explicit match-and-bail. `tracing` for logs (no `println!` in library code).
- **Comments:** explain *why*, not *what*. Don't restate the code. Spec references (e.g. `§8.2`) are welcome and helpful.
- **Tests:** colocate unit tests with the module they cover (`#[cfg(test)] mod tests`). Use `tests/spec_conformance.rs` for cross-module behavior.
- **Public API:** anything in `lib.rs` or re-exported is API. Mark experimental items `#[doc(hidden)]` until they're stable.

## Adding a new tracker

Drop a new file under `src/tracker/` and `impl IssueTracker for YourTracker`. Wire it into `tracker::build_from_config` and add a `TrackerKind` variant. The trait surface is small on purpose:

```rust
async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>>;
async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>>;
async fn fetch_issue_states_by_ids(&self, ids: &[String]) -> Result<Vec<IssueState>>;
```

Match the normalization in `src/tracker/linear.rs` — lowercase labels, blockers from "is blocked by" relations, ISO-8601 timestamps.

## Adding a new agent backend

Two patterns:

1. **Raw LLM** — implement `LlmCaller` in `src/agent/turn.rs` and a thin `CodingAgent` wrapper that calls `run_provider_turn`. See `openai.rs` / `anthropic.rs` for the shape.
2. **CLI subprocess** — extend `src/agent/cli.rs` with a new `CliFlavor` variant and a parser. See the Claude Code / Codex CLI implementations.

Either way, add an `AgentProvider` variant in `src/config/typed.rs` and wire it into `agent::build_for`.

## Commit messages

We use plain English, present tense, imperative mood:

```
add Jira self-hosted Bearer auth path
fix retry backoff cap off-by-one when attempt == 0
docs: link the team-workflow section from Getting Started
```

Reference the issue you're fixing in the body (`Fixes #123`) when there is one.

## Releases

Maintainers cut releases. The flow:

1. Update `CHANGELOG.md` under a new version heading.
2. Bump the version in `Cargo.toml`.
3. Tag `vX.Y.Z` and push the tag.
4. CI publishes to crates.io and attaches a build to the GitHub release.

## Questions

Open a [discussion](https://github.com/O-Side-Systems/sinfonia/discussions) if it doesn't fit an issue.
