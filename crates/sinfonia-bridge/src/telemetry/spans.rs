//! Bridge-side span + metric name constants (plan §5, §6).
//!
//! Sibling of `sinfonia::telemetry::spans`. The two are intentionally not
//! shared: span names are part of the operator-facing contract of each
//! binary, and a single typo in a shared constant would touch both.

// Span names (§5).
pub const BRIDGE_WEBHOOK: &str = "bridge.webhook";
pub const BRIDGE_CI_RESULT: &str = "bridge.ci_result";
pub const BRIDGE_STATE_TRANSITION: &str = "bridge.state_transition";
pub const BRIDGE_CAP_HIT: &str = "bridge.cap_hit";
pub const BRIDGE_COST_UPDATE: &str = "bridge.cost_update";
pub const BRIDGE_PR_LABEL: &str = "bridge.pr_label";
pub const BRIDGE_EVENTS_RECEIVE: &str = "bridge.events_receive";

// Attribute keys (shared with Sinfonia where the data model overlaps, but
// each binary keeps its own constants so the contracts don't drift). Per
// plan §3.3 every span carries `tenant_id`.
pub const ATTR_TENANT_ID: &str = "tenant_id";
pub const ATTR_EVENT_TYPE: &str = "event_type";
pub const ATTR_DELIVERY_ID: &str = "delivery_id";
pub const ATTR_REPO: &str = "repo";
pub const ATTR_DURATION_MS: &str = "duration_ms";
pub const ATTR_PR_NUMBER: &str = "pr_number";
pub const ATTR_TICKET_ID: &str = "ticket_id";
pub const ATTR_OUTCOME: &str = "outcome";
pub const ATTR_ATTEMPT_COUNT: &str = "attempt_count";
pub const ATTR_FAILURE_CATEGORY: &str = "failure_category";
pub const ATTR_FROM_STATE: &str = "from_state";
pub const ATTR_TO_STATE: &str = "to_state";
pub const ATTR_REASON: &str = "reason";
pub const ATTR_FINAL_ATTEMPT_COUNT: &str = "final_attempt_count";
pub const ATTR_CAP_KIND: &str = "cap_kind";
pub const ATTR_TOKENS_DELTA: &str = "tokens_delta";
pub const ATTR_COST_DELTA_USD: &str = "cost_delta_usd";
pub const ATTR_TOKENS_TOTAL: &str = "tokens_total";
pub const ATTR_COST_TOTAL_USD: &str = "cost_total_usd";
pub const ATTR_LABEL: &str = "label";
pub const ATTR_ACTION: &str = "action";

// Cap-kind enumerated values for the `cap_kind` attribute (§7.4).
pub const CAP_KIND_ATTEMPTS: &str = "attempts";
pub const CAP_KIND_TOKENS: &str = "tokens";
pub const CAP_KIND_COST: &str = "cost";

// Transition-reason enumerated values for the `reason` attribute (§5).
pub const REASON_CI_FAILURE: &str = "ci_failure";
pub const REASON_CI_GREEN: &str = "ci_green";
pub const REASON_CAP_HIT: &str = "cap_hit";
pub const REASON_BUDGET_EXCEEDED: &str = "budget_exceeded";
pub const REASON_MANUAL: &str = "manual";
