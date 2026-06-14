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

    let terminal_lc: Vec<String> = cfg
        .tracker
        .terminal_states
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    // [D-05: parent-child hierarchy gate removed — dependency gating keys solely on
    // Linear `blocks` relations. See Phase 3 implementation.]

    // Blocker rule for Todo state (§8.2).
    let todo = cfg
        .tracker
        .active_states
        .iter()
        .any(|s| s.eq_ignore_ascii_case("Todo"))
        && state_l == "todo";
    if todo {
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
        Some(0) => (1, 0), // 0 ("no priority" in Linear) sorts last like None
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
            children: vec![],
            created_at: Some(chrono::Utc.timestamp_opt(created, 0).unwrap()),
            updated_at: None,
            fields: Default::default(),
        }
    }

    fn cfg_with_states(active: &[&str], terminal: &[&str]) -> crate::config::ServiceConfig {
        let active_yaml = active
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let terminal_yaml = terminal
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let wf = format!(
            "---\ntracker:\n  kind: linear\n  api_key: testkey\n  project_slug: p\n  active_states: [{active_yaml}]\n  terminal_states: [{terminal_yaml}]\n---\nbody"
        );
        let def = crate::config::parse_workflow_str(&wf).unwrap();
        crate::config::ServiceConfig::from_workflow(&def).unwrap()
    }

    fn blocked_by(id: &str, blocker_states: &[&str]) -> Issue {
        let mut i = iss(id, Some(1), 100);
        i.blocked_by = blocker_states
            .iter()
            .enumerate()
            .map(|(idx, st)| crate::domain::BlockerRef {
                id: Some(format!("{id}-b{idx}")),
                identifier: Some(format!("BLK-{}", idx + 1)),
                state: Some((*st).into()),
            })
            .collect();
        i
    }

    #[test]
    fn todo_with_open_blocker_is_not_eligible() {
        let cfg = cfg_with_states(&["Todo", "In Progress"], &["Done", "Cancelled"]);
        // Issue is Todo (default from iss); blocker is In Progress (non-terminal).
        let issue = blocked_by("T1", &["In Progress"]);
        assert!(!is_dispatch_eligible(&issue, &cfg));
    }

    #[test]
    fn in_progress_ignores_blockers() {
        let cfg = cfg_with_states(&["Todo", "In Progress"], &["Done", "Cancelled"]);
        // Same blocker, but the issue is In Progress — blocker gate is Todo-only.
        let mut issue = blocked_by("T2", &["In Progress"]);
        issue.state = "In Progress".into();
        assert!(is_dispatch_eligible(&issue, &cfg));
    }

    #[test]
    fn todo_with_terminal_blocker_is_eligible() {
        let cfg = cfg_with_states(&["Todo", "In Progress"], &["Done", "Cancelled"]);
        // Todo + blocker in terminal state → gate opens.
        let issue = blocked_by("T3", &["Done"]);
        assert!(is_dispatch_eligible(&issue, &cfg));
    }

    #[test]
    fn parent_with_open_child_is_now_eligible() {
        // After D-05 removal: parent-child hierarchy gate is gone, so a parent
        // with a non-terminal child IS dispatch-eligible. This is the explicit
        // inverse of the deleted `parent_with_open_child_is_not_eligible` and
        // pins the removal as intentional to guard against re-introduction.
        let cfg = cfg_with_states(&["Todo", "In Progress"], &["Done", "Cancelled"]);
        let mut parent = iss("P1", Some(1), 100);
        parent.children = vec![crate::domain::ChildRef {
            id: Some("c1".into()),
            identifier: Some("P1-sub".into()),
            state: "In Progress".into(),
        }];
        // Hierarchy gate removed (D-05) — parent is now eligible.
        assert!(is_dispatch_eligible(&parent, &cfg));
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
