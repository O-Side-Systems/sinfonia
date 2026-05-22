//! Span name + attribute key constants (plan §4).
//!
//! Centralizing these here so refactors in emission sites can grep for one
//! symbol, and so a future query layer in Phase 3's Postgres reference
//! deployment can join on the same strings without typo drift.

// Span names.
pub const ORCHESTRATOR_TICK: &str = "orchestrator.tick";
pub const ORCHESTRATOR_DISPATCH: &str = "orchestrator.dispatch";
pub const RUNNER_SESSION: &str = "runner.session";
pub const RUNNER_TURN: &str = "runner.turn";
pub const WORKSPACE_HOOK: &str = "workspace.hook";
pub const TRACKER_FETCH: &str = "tracker.fetch";

// Attribute keys shared across spans + metrics. Per plan §3.3 every span
// carries `tenant_id`; the resource-level `service.namespace` mirrors it so
// Collector routing-processor configs work without per-span lookups.
pub const ATTR_TENANT_ID: &str = "tenant_id";
pub const ATTR_ISSUE_ID: &str = "issue_id";
pub const ATTR_ISSUE_IDENTIFIER: &str = "issue_identifier";
pub const ATTR_STATE: &str = "state";
pub const ATTR_PROVIDER: &str = "provider";
pub const ATTR_MODEL: &str = "model";
pub const ATTR_TURN_INDEX: &str = "turn_index";
pub const ATTR_TURN_COUNT: &str = "turn_count";
pub const ATTR_TOTAL_TOKENS: &str = "total_tokens";
pub const ATTR_PROMPT_TOKENS: &str = "prompt_tokens";
pub const ATTR_COMPLETION_TOKENS: &str = "completion_tokens";
pub const ATTR_EXIT_REASON: &str = "exit_reason";
pub const ATTR_DURATION_MS: &str = "duration_ms";
pub const ATTR_CANDIDATES_COUNT: &str = "candidates_count";
pub const ATTR_DISPATCHED_COUNT: &str = "dispatched_count";
pub const ATTR_TICK_DURATION_MS: &str = "tick_duration_ms";
pub const ATTR_HOOK_NAME: &str = "hook_name";
pub const ATTR_EXIT_CODE: &str = "exit_code";
pub const ATTR_TRACKER_KIND: &str = "tracker_kind";
pub const ATTR_REQUEST_KIND: &str = "request_kind";
pub const ATTR_RESULT_COUNT: &str = "result_count";
pub const ATTR_TOOL_CALLS_COUNT: &str = "tool_calls_count";
pub const ATTR_TOKENS: &str = "tokens";
