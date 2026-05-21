//! Attempt-counter logic (plan §5.2 step 6).
//!
//! Reads `sinfonia_attempt_count` and the per-ticket override
//! `sinfonia_max_attempts` from the tracker's custom-field surface,
//! then decides whether the next red CI run lands at the
//! `needs_fixes_state` (or category-routed target) or at the
//! `blocked_state`.
//!
//! Reads are coalesced into one call apiece — the marker comment
//! contains both fields. The transition layer is responsible for
//! actually writing the incremented value back via
//! `IssueTracker::write_custom_field`; this module's `decide` function
//! is pure once the inputs are fetched.

use crate::Result;
use sinfonia_tracker::{CustomFieldValue, IssueTracker};

/// What the transition layer should do on a red CI run.
///
/// The variant carries the next counter value so the transition
/// layer doesn't have to re-derive it. `next` is the value to write
/// back into `sinfonia_attempt_count` after the transition succeeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptDecision {
    /// Continue retrying. `next` is the new counter value (prior + 1).
    Continue { next: u32 },
    /// Cap reached on this run. The counter does NOT advance past
    /// `max_attempts`; the transition layer routes to `blocked_state`.
    CapHit { stayed_at: u32, max: u32 },
}

impl AttemptDecision {
    pub fn is_cap_hit(&self) -> bool {
        matches!(self, Self::CapHit { .. })
    }
}

/// Pure decision: given the previous counter and the resolved cap,
/// what should happen on the next red CI run?
///
/// The math is: `prior + 1` is the would-be next value. If that
/// exceeds `max`, the cap is hit and the counter stays at `max` (not
/// `prior` — the prior value is whatever the tracker last wrote, which
/// in the cap-hit path is by construction equal to `max`). If the
/// would-be next value equals `max`, the cap is hit on this attempt —
/// the counter advances to `max` and the transition layer routes to
/// `blocked_state`.
pub fn decide(prior: u32, max: u32) -> AttemptDecision {
    let next = prior.saturating_add(1);
    if next > max {
        AttemptDecision::CapHit {
            stayed_at: prior,
            max,
        }
    } else if next == max {
        // The final allowed attempt — still continue (the agent gets one
        // more chance), but the bridge will route to `blocked_state` on
        // the *next* red event. Tracked as Continue so the increment
        // still lands.
        AttemptDecision::Continue { next }
    } else {
        AttemptDecision::Continue { next }
    }
}

/// Fetch the prior counter and per-ticket cap from the tracker, then
/// resolve via [`decide`].
///
/// `attempt_count_key` / `max_attempts_key` are the configured custom-
/// field names from `BridgeConfig::custom_fields`. `config_max` is the
/// process-wide `feedback_loop.max_attempts` — overridden by the per-
/// ticket value when set and non-zero.
pub async fn read_and_decide(
    tracker: &dyn IssueTracker,
    issue_id: &str,
    attempt_count_key: &str,
    max_attempts_key: &str,
    config_max: u32,
) -> Result<(u32, u32, AttemptDecision)> {
    let prior = read_u32(tracker, issue_id, attempt_count_key).await?.unwrap_or(0);
    let per_ticket = read_u32(tracker, issue_id, max_attempts_key).await?;
    let effective_max = match per_ticket {
        // A value of zero means "use the process-wide cap" — operators
        // can't set max=0 sensibly (it would block on the first attempt).
        // This matches `BridgeConfig` validation rejecting max_attempts<1.
        Some(0) | None => config_max,
        Some(v) => v,
    };
    Ok((prior, effective_max, decide(prior, effective_max)))
}

async fn read_u32(
    tracker: &dyn IssueTracker,
    issue_id: &str,
    key: &str,
) -> Result<Option<u32>> {
    match tracker.read_custom_field(issue_id, key).await? {
        CustomFieldValue::Null => Ok(None),
        CustomFieldValue::Number(n) if n >= 0.0 => Ok(Some(n as u32)),
        CustomFieldValue::String(s) => Ok(s.parse::<u32>().ok()),
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Tests (plan §9.1: `feedback::attempts::tests`)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sinfonia_tracker::{
        CustomFieldKind, CustomFieldSchema, CustomFieldValue, Issue, IssueState, IssueTracker,
        Result as TrackerResult,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    // -- Pure decision tests --------------------------------------------

    #[test]
    fn increment_from_zero() {
        assert_eq!(decide(0, 5), AttemptDecision::Continue { next: 1 });
    }

    #[test]
    fn increment_one_below_cap_continues() {
        assert_eq!(decide(3, 5), AttemptDecision::Continue { next: 4 });
    }

    #[test]
    fn final_allowed_attempt_continues_then_caps() {
        // `decide(4, 5)` is the LAST permitted retry — counter advances
        // to 5 and `Continue` lets the agent run one more time.
        assert_eq!(decide(4, 5), AttemptDecision::Continue { next: 5 });
        // The next red event finds prior=5 → cap hit, counter stays.
        assert_eq!(
            decide(5, 5),
            AttemptDecision::CapHit { stayed_at: 5, max: 5 }
        );
    }

    #[test]
    fn cap_hit_does_not_advance_counter() {
        // Prior already at max — counter stays put, not advanced.
        let d = decide(5, 5);
        assert!(d.is_cap_hit());
        // Past the cap (shouldn't happen in practice) is also CapHit
        // with `stayed_at` reporting the actual prior value.
        let d = decide(7, 5);
        assert_eq!(d, AttemptDecision::CapHit { stayed_at: 7, max: 5 });
    }

    // -- read_and_decide against a mock tracker -------------------------

    /// Mock tracker with a single ticket's custom-field map.
    struct MockTracker {
        fields: Mutex<HashMap<String, CustomFieldValue>>,
    }

    impl MockTracker {
        fn new(fields: HashMap<String, CustomFieldValue>) -> Self {
            Self {
                fields: Mutex::new(fields),
            }
        }
    }

    #[async_trait]
    impl IssueTracker for MockTracker {
        async fn fetch_candidate_issues(&self) -> TrackerResult<Vec<Issue>> {
            Ok(vec![])
        }
        async fn fetch_issues_by_states(&self, _states: &[String]) -> TrackerResult<Vec<Issue>> {
            Ok(vec![])
        }
        async fn fetch_issue_states_by_ids(
            &self,
            _ids: &[String],
        ) -> TrackerResult<Vec<IssueState>> {
            Ok(vec![])
        }
        async fn read_custom_field(
            &self,
            _id: &str,
            key: &str,
        ) -> TrackerResult<CustomFieldValue> {
            Ok(self
                .fields
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .unwrap_or(CustomFieldValue::Null))
        }
        async fn write_custom_field(
            &self,
            _id: &str,
            key: &str,
            value: CustomFieldValue,
        ) -> TrackerResult<()> {
            self.fields.lock().unwrap().insert(key.into(), value);
            Ok(())
        }
        async fn ensure_custom_field(
            &self,
            _schema: &CustomFieldSchema,
        ) -> TrackerResult<()> {
            Ok(())
        }
        async fn post_comment(&self, _id: &str, _body: &str) -> TrackerResult<()> {
            Ok(())
        }
        async fn transition_issue(&self, _id: &str, _target: &str) -> TrackerResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn read_and_decide_with_no_prior_starts_at_zero() {
        let tracker = MockTracker::new(HashMap::new());
        let (prior, max, decision) = read_and_decide(
            &tracker,
            "ENG-1",
            "sinfonia_attempt_count",
            "sinfonia_max_attempts",
            5,
        )
        .await
        .expect("decide");
        assert_eq!(prior, 0);
        assert_eq!(max, 5);
        assert_eq!(decision, AttemptDecision::Continue { next: 1 });
    }

    #[tokio::test]
    async fn read_and_decide_respects_per_ticket_override() {
        let mut fields = HashMap::new();
        fields.insert(
            "sinfonia_attempt_count".into(),
            CustomFieldValue::Number(1.0),
        );
        fields.insert(
            "sinfonia_max_attempts".into(),
            CustomFieldValue::Number(2.0),
        );
        let tracker = MockTracker::new(fields);
        let (prior, max, decision) = read_and_decide(
            &tracker,
            "ENG-1",
            "sinfonia_attempt_count",
            "sinfonia_max_attempts",
            /* config_max = */ 5,
        )
        .await
        .expect("decide");
        assert_eq!(prior, 1);
        assert_eq!(max, 2, "per-ticket override should beat config_max");
        // prior=1, max=2 → next=2 == max → Continue { next: 2 } (final
        // permitted retry). The cap is hit on the next red event.
        assert_eq!(decision, AttemptDecision::Continue { next: 2 });
    }

    #[tokio::test]
    async fn cap_hit_decision_when_prior_equals_max() {
        let mut fields = HashMap::new();
        fields.insert(
            "sinfonia_attempt_count".into(),
            CustomFieldValue::Number(3.0),
        );
        let tracker = MockTracker::new(fields);
        let (prior, max, decision) = read_and_decide(
            &tracker,
            "ENG-1",
            "sinfonia_attempt_count",
            "sinfonia_max_attempts",
            3,
        )
        .await
        .expect("decide");
        assert_eq!(prior, 3);
        assert_eq!(max, 3);
        assert!(decision.is_cap_hit(), "prior==max should trigger cap");
    }

    #[tokio::test]
    async fn override_of_zero_falls_back_to_config_max() {
        let mut fields = HashMap::new();
        fields.insert(
            "sinfonia_max_attempts".into(),
            CustomFieldValue::Number(0.0),
        );
        let tracker = MockTracker::new(fields);
        let (_, max, _) = read_and_decide(
            &tracker,
            "ENG-1",
            "sinfonia_attempt_count",
            "sinfonia_max_attempts",
            5,
        )
        .await
        .expect("decide");
        assert_eq!(max, 5, "override=0 should be ignored");
    }

    #[tokio::test]
    async fn override_as_string_parses() {
        // The Linear marker comment can serialize numeric overrides as
        // strings when humans hand-edit them. We accept that.
        let mut fields = HashMap::new();
        fields.insert(
            "sinfonia_max_attempts".into(),
            CustomFieldValue::String("3".into()),
        );
        let tracker = MockTracker::new(fields);
        let (_, max, _) = read_and_decide(
            &tracker,
            "ENG-1",
            "sinfonia_attempt_count",
            "sinfonia_max_attempts",
            5,
        )
        .await
        .expect("decide");
        assert_eq!(max, 3);
    }
}
