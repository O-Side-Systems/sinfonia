//! Dispatch eligibility, sort order, and slot accounting (spec §8.2, §8.3).

use crate::config::ServiceConfig;
use crate::domain::{Issue, OrchestratorState};

pub fn is_dispatch_eligible(issue: &Issue, cfg: &ServiceConfig) -> bool {
    if issue.id.is_empty()
        || issue.identifier.is_empty()
        || issue.title.is_empty()
        || issue.state.is_empty()
    {
        return false;
    }
    let state_l = issue.normalized_state();
    let active = cfg
        .tracker
        .active_states
        .iter()
        .any(|s| s.to_lowercase() == state_l);
    let terminal = cfg
        .tracker
        .terminal_states
        .iter()
        .any(|s| s.to_lowercase() == state_l);
    if !active || terminal {
        return false;
    }

    // Blocker rule for Todo state (§8.2).
    let todo = cfg
        .tracker
        .active_states
        .iter()
        .any(|s| s.eq_ignore_ascii_case("Todo"))
        && state_l == "todo";
    if todo {
        let terminal_lc: Vec<String> = cfg
            .tracker
            .terminal_states
            .iter()
            .map(|s| s.to_lowercase())
            .collect();
        for b in &issue.blocked_by {
            let bs = b.state.as_deref().unwrap_or("").to_lowercase();
            if bs.is_empty() {
                return false;
            }
            if !terminal_lc.iter().any(|t| t == &bs) {
                return false;
            }
        }
    }
    true
}

pub fn has_slot(state: &OrchestratorState, issue: &Issue, cfg: &ServiceConfig) -> bool {
    let global = cfg.agent.max_concurrent_agents as usize;
    let used = state.running.len();
    if used >= global {
        return false;
    }
    let state_key = issue.normalized_state();
    if let Some(per_state) = cfg.agent.max_concurrent_agents_by_state.get(&state_key) {
        let in_same_state = state
            .running
            .values()
            .filter(|r| r.issue.normalized_state() == state_key)
            .count();
        if in_same_state >= *per_state as usize {
            return false;
        }
    }
    true
}

/// §8.2 sort order: priority asc (with `None`/0 last per spec note that null sorts last),
/// then created_at oldest first, then identifier lex.
pub fn sort_for_dispatch(mut issues: Vec<Issue>) -> Vec<Issue> {
    issues.sort_by(|a, b| {
        let pa = priority_key(a.priority);
        let pb = priority_key(b.priority);
        pa.cmp(&pb)
            .then(a.created_at.cmp(&b.created_at))
            .then(a.identifier.cmp(&b.identifier))
    });
    issues
}

fn priority_key(p: Option<i64>) -> (u8, i64) {
    match p {
        None => (1, i64::MAX),
        Some(0) => (1, 0),     // 0 ("no priority" in Linear) sorts last like None
        Some(n) => (0, n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn iss(id: &str, prio: Option<i64>, created: i64) -> Issue {
        Issue {
            id: id.into(),
            identifier: id.into(),
            title: "t".into(),
            description: None,
            priority: prio,
            state: "Todo".into(),
            branch_name: None,
            url: None,
            labels: vec![],
            blocked_by: vec![],
            created_at: Some(chrono::Utc.timestamp_opt(created, 0).unwrap()),
            updated_at: None,
        }
    }

    #[test]
    fn sorts_priority_then_created_then_identifier() {
        let v = vec![
            iss("c", Some(2), 100),
            iss("a", Some(1), 200),
            iss("b", Some(1), 100),
            iss("d", None, 50),
            iss("e", Some(0), 10),
        ];
        let sorted = sort_for_dispatch(v);
        let ids: Vec<_> = sorted.iter().map(|i| i.id.clone()).collect();
        assert_eq!(ids, vec!["b", "a", "c", "e", "d"]);
    }
}
