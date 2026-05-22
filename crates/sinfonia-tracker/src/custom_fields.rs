//! Tracker-agnostic custom-field values.
//!
//! A "custom field" in Sinfonia parlance is a named, typed, ticket-scoped
//! piece of metadata that a bridge service writes during the CI feedback
//! loop and the agent prompt reads via Liquid (`{{ issue.fields.<name> }}`).
//! See `docs/SPEC.md` §11.6 (bridge services) and §11.7 (custom-field
//! discovery) for the contract.
//!
//! Trackers diverge on how this concept maps to their data model:
//!
//! - **Jira** has real custom fields (`/rest/api/3/field`). The bridge
//!   creates them with [`CustomFieldSchema`] at startup and reads/writes
//!   them through `issue.fields.customfield_NNNNN` (Phase 4).
//! - **Linear** does not. Sinfonia models Linear "custom fields" as a
//!   single bot-owned comment per ticket whose body is a JSON object
//!   keyed by the sentinel `MARKER`. The whole field set is rewritten
//!   on every update; the bridge owns this comment and never touches
//!   user comments.
//!
//! ## Serialization contract
//!
//! [`CustomFieldValue`] has a **hand-written** `Serialize` impl that
//! flattens to the underlying primitive — `Number` to a JSON number,
//! `LongText` to a JSON string, and so on. The derived implementation
//! would have emitted tagged JSON like `{"Number": 3}`, which would
//! render in a Liquid prompt as the literal text `{"Number": 3}` rather
//! than the value `3`. Do **not** add `#[derive(Serialize)]` to the enum
//! — the round-trip test in the `sinfonia` crate's `template.rs::tests`
//! will catch the regression, but it's easier to spot when the contract
//! is documented at the source.

use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;

/// Sentinel key used to identify the bot-owned comment on a Linear ticket
/// (see module docs). Versioned so we can migrate the schema later without
/// ambiguity.
pub const MARKER: &str = "sinfonia_bridge_state_v1";

/// Well-known custom-field keys the v0.3 bridge writes (spec §11.6).
///
/// Templates rendered by Sinfonia pre-populate these to
/// [`CustomFieldValue::Null`] when absent, so that a Liquid prompt like
/// `{{ issue.fields.sinfonia_last_ci_failure | default: "…" }}` never trips
/// over strict-mode "Unknown index" on a ticket the bridge hasn't touched
/// yet (e.g. a human moving a ticket straight into a "Needs Fixes" state).
/// Adapter-side writes still ignore this list — the bridge can populate
/// any key it likes; this list is only the *known absent-default set*.
pub const WELL_KNOWN_FIELDS: &[&str] = &[
    "sinfonia_attempt_count",
    "sinfonia_last_ci_failure",
    "sinfonia_failure_category",
    "sinfonia_max_attempts",
    "sinfonia_tokens_consumed",
    "sinfonia_cost_consumed_usd",
    "sinfonia_max_cost_usd",
    // Phase 3 — written by the budget enforcer when a token/cost cap
    // fires. Templates can reference it to show the operator-facing
    // ticket-blocked timestamp (RFC-3339 string).
    "sinfonia_budget_exhausted_at",
];

/// Possible primitive shapes a custom-field value can take.
///
/// Deliberately collapsed to three JSON-distinguishable shapes:
///
/// - `Null` → JSON `null` (used to mean "field is unset").
/// - `Number` → JSON number (used for integer counters like
///   `sinfonia_attempt_count`).
/// - `String` → JSON string (used for everything else: failure-log text,
///   monetary values stringified to preserve precision, URLs, category
///   names, etc.).
///
/// Why not split `Decimal` / `Url` / `LongText` into their own variants?
/// Because `#[serde(untagged)]` deserialization can't distinguish two
/// variants that share a JSON shape — a JSON string always deserializes
/// as the first matching variant, which is brittle. Callers that need
/// semantic typing (e.g. "this string is a cost in USD") track that
/// out-of-band via [`CustomFieldKind`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum CustomFieldValue {
    Null,
    Number(f64),
    String(String),
}

impl CustomFieldValue {
    pub fn is_null(&self) -> bool {
        matches!(self, CustomFieldValue::Null)
    }

    /// Convenience: build a `String` variant from anything that yields a
    /// `String` — e.g. `&str`, `String`, `rust_decimal::Decimal::to_string()`.
    pub fn text(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }
}

// IMPORTANT: hand-written so the serialized representation is the underlying
// primitive, not the tagged enum. `#[derive(Serialize)]` would have produced
// `{"Number": 3}`, breaking the Liquid prompt rendering path. See the module
// docs for the contract and the regression test in `sinfonia::template`.
impl Serialize for CustomFieldValue {
    fn serialize<S: Serializer>(&self, ser: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Null => ser.serialize_none(),
            Self::Number(n) => n.serialize(ser),
            Self::String(s) => s.serialize(ser),
        }
    }
}

/// Kind tag used when calling [`super::IssueTracker::ensure_custom_field`].
///
/// Trackers that don't have a corresponding native type (e.g. Linear) ignore
/// this; Jira maps it onto a `customFieldType` when creating the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomFieldKind {
    Number,
    Decimal,
    LongText,
    Url,
}

/// Schema for a tracker custom field. Passed to `ensure_custom_field` at
/// bridge startup to make creation idempotent.
#[derive(Debug, Clone)]
pub struct CustomFieldSchema {
    /// Stable key the bridge uses to refer to the field — e.g.
    /// `"sinfonia_attempt_count"`. Persisted across releases.
    pub key: String,
    /// Human-readable display name shown in the tracker UI.
    pub display_name: String,
    pub kind: CustomFieldKind,
    pub description: Option<String>,
}

/// The whole field-set the bridge owns for one ticket, as it appears in the
/// Linear marker comment. Jira reads/writes individual fields; Linear
/// load-modify-stores this whole struct each update.
pub type FieldsMap = HashMap<String, CustomFieldValue>;

/// JSON body shape of the Linear bot-owned marker comment. The outer object
/// has a single key (`MARKER`) so we can also use the same marker as a
/// regex sentinel when listing comments.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarkerEnvelope {
    /// The actual field map. Keyed by stable field name
    /// (e.g. `"sinfonia_attempt_count"`).
    #[serde(default)]
    pub fields: FieldsMap,
}

/// Wrap a field map into the marker-comment JSON body.
pub fn encode_marker(envelope: &MarkerEnvelope) -> String {
    let mut outer = serde_json::Map::new();
    outer.insert(
        MARKER.to_string(),
        serde_json::to_value(envelope).unwrap_or(serde_json::Value::Null),
    );
    serde_json::to_string(&outer).unwrap_or_else(|_| "{}".to_string())
}

/// Parse a comment body looking for the marker envelope. Returns `None` if
/// this comment isn't the bot's state comment. Tolerant of leading /
/// trailing whitespace and of Markdown code-fence wrappers (some tracker
/// UIs add them when the body is pasted in by a human).
pub fn decode_marker(body: &str) -> Option<MarkerEnvelope> {
    let trimmed = strip_code_fence(body.trim());
    let outer: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let envelope = outer.get(MARKER)?;
    serde_json::from_value(envelope.clone()).ok()
}

/// Strip a leading ``` ```json ``` and trailing ``` ``` if present.
fn strip_code_fence(s: &str) -> &str {
    let mut out = s;
    if let Some(rest) = out.strip_prefix("```json") {
        out = rest.trim_start();
    } else if let Some(rest) = out.strip_prefix("```") {
        out = rest.trim_start();
    }
    if let Some(rest) = out.strip_suffix("```") {
        out = rest.trim_end();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_number_as_bare_primitive() {
        let v = CustomFieldValue::Number(3.0);
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "3.0");
    }

    #[test]
    fn serializes_long_text_as_bare_string() {
        let v = CustomFieldValue::text("hello");
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"hello\"");
    }

    #[test]
    fn serializes_stringified_decimal_for_precision() {
        // Cost values are written as the result of `Decimal::to_string()`
        // so JSON round-tripping doesn't lose precision via f64 cast.
        let v = CustomFieldValue::text("8.23");
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"8.23\"");
    }

    #[test]
    fn round_trip_through_marker_envelope() {
        let mut fields = FieldsMap::new();
        fields.insert(
            "sinfonia_attempt_count".to_string(),
            CustomFieldValue::Number(3.0),
        );
        fields.insert(
            "sinfonia_last_ci_failure".to_string(),
            CustomFieldValue::text("CI failed on attempt 3 of 5."),
        );

        let env = MarkerEnvelope { fields };
        let encoded = encode_marker(&env);
        assert!(encoded.contains(MARKER));

        let decoded = decode_marker(&encoded).expect("decode");
        assert_eq!(decoded.fields.len(), 2);
        let count = decoded.fields.get("sinfonia_attempt_count").unwrap();
        assert_eq!(*count, CustomFieldValue::Number(3.0));
    }

    #[test]
    fn decode_tolerates_code_fence() {
        let mut fields = FieldsMap::new();
        fields.insert("k".into(), CustomFieldValue::text("v"));
        let body = format!("```json\n{}\n```", encode_marker(&MarkerEnvelope { fields }));
        let decoded = decode_marker(&body).expect("decode");
        let v = decoded.fields.get("k").unwrap();
        assert!(matches!(v, CustomFieldValue::String(s) if s == "v"));
    }

    #[test]
    fn decode_returns_none_for_non_marker_comment() {
        assert!(decode_marker("just a human comment").is_none());
        assert!(decode_marker("{\"other_marker\": {}}").is_none());
        assert!(decode_marker("").is_none());
    }
}
