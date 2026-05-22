//! Spec-driven integration tests (§17).

use std::collections::HashSet;
use sinfonia::config::{AgentProvider, ServiceConfig, TrackerKind};
use sinfonia::domain::{sanitize_workspace_key, BlockerRef, Issue};

// §17.1 Workflow + config parsing -------------------------------------------------

#[test]
fn workflow_cwd_default_required_fields_validate() {
    let def = sinfonia::config::parse_workflow_str(
        "---\ntracker:\n  kind: linear\n  api_key: testkey\n  project_slug: proj\n---\nhi",
    )
    .unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    assert!(matches!(cfg.tracker.kind, TrackerKind::Linear));
    assert!(cfg.validate_for_dispatch().is_ok());
}

#[test]
fn workflow_missing_api_key_blocks_dispatch() {
    let def = sinfonia::config::parse_workflow_str(
        "---\ntracker:\n  kind: linear\n  project_slug: x\n---\n",
    )
    .unwrap();
    // Ensure no inherited env var pollutes the test.
    std::env::remove_var("LINEAR_API_KEY");
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    let err = cfg.validate_for_dispatch().unwrap_err();
    assert!(format!("{err}").contains("missing_tracker_api_key"));
}

#[test]
fn template_renders_issue_and_attempt() {
    let issue = sample_issue("ABC-1", "Todo", Some(2));
    let s = sinfonia::template::render_prompt(
        "{{ issue.identifier }} :: attempt={{ attempt }}",
        &issue,
        Some(2),
    )
    .unwrap();
    assert!(s.contains("ABC-1"));
    assert!(s.contains("attempt=2"));
}

#[test]
fn template_strict_unknown_variable_errors() {
    let issue = sample_issue("ABC-1", "Todo", Some(2));
    let err = sinfonia::template::render_prompt("{{ noSuchVar }}", &issue, None).unwrap_err();
    let s = format!("{err}");
    assert!(s.contains("template_render_error") || s.contains("template_parse_error"));
}

// §17.2 Workspace + safety -------------------------------------------------------

#[test]
fn workspace_sanitization_replaces_unsafe_chars() {
    assert_eq!(sanitize_workspace_key("MT-649"), "MT-649");
    assert_eq!(sanitize_workspace_key("../../etc/passwd"), ".._.._etc_passwd");
    assert_eq!(sanitize_workspace_key("a b/c\\d"), "a_b_c_d");
}

// §17.4 Dispatch sort + blocker rule --------------------------------------------

#[test]
fn dispatch_sort_priority_then_creation() {
    use chrono::TimeZone;
    let make = |id: &str, prio: Option<i64>, created: i64| Issue {
        id: id.to_string(),
        identifier: id.to_string(),
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
    };
    let v = vec![
        make("z", Some(3), 1),
        make("a", Some(1), 100),
        make("b", Some(1), 50),
        make("n", None, 0),
    ];
    let sorted = sinfonia::orchestrator::dispatch_for_test(v);
    let ids: Vec<_> = sorted.iter().map(|i| i.id.clone()).collect();
    assert_eq!(ids, vec!["b", "a", "z", "n"]);
}

#[test]
fn todo_with_non_terminal_blocker_is_not_eligible() {
    let def = sinfonia::config::parse_workflow_str(
        "---\ntracker:\n  kind: linear\n  api_key: x\n  project_slug: p\n---\n",
    )
    .unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    let mut issue = sample_issue("ABC-1", "Todo", Some(2));
    issue.blocked_by = vec![BlockerRef {
        id: Some("b1".into()),
        identifier: Some("ABC-9".into()),
        state: Some("In Progress".into()),
    }];
    assert!(!sinfonia::orchestrator::is_eligible_for_test(&issue, &cfg));
}

#[test]
fn todo_with_terminal_blocker_is_eligible() {
    let def = sinfonia::config::parse_workflow_str(
        "---\ntracker:\n  kind: linear\n  api_key: x\n  project_slug: p\n---\n",
    )
    .unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    let mut issue = sample_issue("ABC-1", "Todo", Some(2));
    issue.blocked_by = vec![BlockerRef {
        id: Some("b1".into()),
        identifier: Some("ABC-9".into()),
        state: Some("Done".into()),
    }];
    assert!(sinfonia::orchestrator::is_eligible_for_test(&issue, &cfg));
}

#[test]
fn terminal_states_default_set_is_inclusive() {
    let def = sinfonia::config::parse_workflow_str(
        "---\ntracker:\n  kind: linear\n  api_key: x\n  project_slug: p\n---\n",
    )
    .unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    let set: HashSet<String> = cfg.tracker.terminal_states.iter().cloned().collect();
    for s in ["Closed", "Cancelled", "Canceled", "Duplicate", "Done"] {
        assert!(set.contains(s), "missing default terminal state: {s}");
    }
}

// §17.4 Retry backoff -----------------------------------------------------------

#[test]
fn retry_backoff_doubles_until_cap() {
    assert_eq!(sinfonia::orchestrator::backoff_for_test(1, 300_000), 10_000);
    assert_eq!(sinfonia::orchestrator::backoff_for_test(2, 300_000), 20_000);
    assert_eq!(sinfonia::orchestrator::backoff_for_test(3, 300_000), 40_000);
    assert_eq!(sinfonia::orchestrator::backoff_for_test(4, 300_000), 80_000);
    assert_eq!(sinfonia::orchestrator::backoff_for_test(5, 300_000), 160_000);
    assert_eq!(sinfonia::orchestrator::backoff_for_test(6, 300_000), 300_000);
    assert_eq!(sinfonia::orchestrator::backoff_for_test(20, 300_000), 300_000);
}

// State-machine extension -------------------------------------------------------

#[test]
fn state_machine_routes_to_distinct_runners() {
    let yaml = r#"---
tracker:
  kind: linear
  api_key: x
  project_slug: p
agent:
  provider: anthropic
  model: claude-sonnet-4-6
states:
  Todo:
    provider: claude_code
    model: claude-sonnet-4-6
    prompt: |
      Investigate {{ issue.identifier }}.
  "In Progress":
    provider: claude_code
    model: claude-opus-4-7
    turn_timeout_ms: 5400000
  "In Review":
    provider: anthropic
    model: claude-haiku-4-5-20251001
---
Default body for {{ issue.identifier }}.
"#;
    let def = sinfonia::config::parse_workflow_str(yaml).unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();

    // Todo → claude_code with default command auto-applied.
    let todo = cfg.effective_llm_for_state("Todo");
    assert_eq!(todo.provider, AgentProvider::ClaudeCode);
    assert!(todo.command.starts_with("claude -p"));
    let p = cfg.effective_prompt_template("Todo", "fallback");
    assert!(p.contains("Investigate"));

    // In Progress → claude_code on a stronger model + longer timeout.
    let ip = cfg.effective_llm_for_state("In Progress");
    assert_eq!(ip.provider, AgentProvider::ClaudeCode);
    assert_eq!(ip.model, "claude-opus-4-7");
    assert_eq!(ip.turn_timeout_ms, 5_400_000);

    // In Review → raw Anthropic.
    let rev = cfg.effective_llm_for_state("In Review");
    assert_eq!(rev.provider, AgentProvider::Anthropic);
    assert_eq!(rev.model, "claude-haiku-4-5-20251001");

    // Unlisted state → global defaults, falls back to workflow body.
    let other = cfg.effective_llm_for_state("Blocked");
    assert_eq!(other.provider, AgentProvider::Anthropic);
    assert_eq!(other.model, "claude-sonnet-4-6");
    assert_eq!(
        cfg.effective_prompt_template("Blocked", "fallback"),
        "fallback"
    );
}

#[test]
fn opencode_provider_parses() {
    // §4 of the Phase 2 plan documents three example `WORKFLOW.md` shapes
    // for `provider: opencode`: a default-lane usage, a state-machine
    // routing block, and a local Ollama-with-LSP setup. All three must
    // round-trip cleanly through `ServiceConfig::from_workflow()` so the
    // typed config layer recognizes the new provider variant.

    // §4.1 — default-lane usage. OpenCode owns auth, so no api_key is
    // wired here.
    let default_lane = r#"---
tracker:
  kind: linear
  api_key: x
  project_slug: p
agent:
  provider: opencode
  model: anthropic/claude-sonnet-4-6
  command: opencode run --format json
  turn_timeout_ms: 1800000
---
"#;
    let def = sinfonia::config::parse_workflow_str(default_lane).unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    assert_eq!(cfg.llm.provider, AgentProvider::OpenCode);
    assert_eq!(cfg.llm.model, "anthropic/claude-sonnet-4-6");
    assert_eq!(cfg.llm.command, "opencode run --format json");
    assert_eq!(cfg.llm.turn_timeout_ms, 1_800_000);
    // OpenCode is in the CLI family, not the raw-LLM family.
    assert!(cfg.llm.provider.is_cli());
    assert!(!cfg.llm.provider.is_raw_llm());

    // §4.2 — state-machine routing. Triage → cheap raw Haiku, Ready and
    // "Needs Fixes" → OpenCode + Sonnet. The override in "Needs Fixes - E2E"
    // exercises both the `model:` and `turn_timeout_ms:` per-state knobs.
    let state_machine = r#"---
tracker:
  kind: linear
  api_key: x
  project_slug: p
  active_states: ["Triage", "Ready", "Needs Fixes", "Needs Fixes - E2E"]
agent:
  provider: anthropic
  model: claude-sonnet-4-6
states:
  Triage:
    provider: anthropic
    model: claude-haiku-4-5-20251001
  Ready:
    provider: opencode
    model: anthropic/claude-sonnet-4-6
    turn_timeout_ms: 3600000
  "Needs Fixes":
    provider: opencode
    model: anthropic/claude-sonnet-4-6
    prompt: |
      Address the CI failures on {{ issue.identifier }}.
  "Needs Fixes - E2E":
    provider: opencode
    model: anthropic/claude-opus-4-7
    turn_timeout_ms: 5400000
---
body
"#;
    let def = sinfonia::config::parse_workflow_str(state_machine).unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    // Triage stays on the raw Anthropic backend.
    let triage = cfg.effective_llm_for_state("Triage");
    assert_eq!(triage.provider, AgentProvider::Anthropic);
    // Ready → OpenCode with an extended turn timeout.
    let ready = cfg.effective_llm_for_state("Ready");
    assert_eq!(ready.provider, AgentProvider::OpenCode);
    assert_eq!(ready.model, "anthropic/claude-sonnet-4-6");
    assert_eq!(ready.turn_timeout_ms, 3_600_000);
    // Switching provider re-applies the provider-shaped default command
    // — same behaviour the claude_code / codex states get.
    assert!(
        ready.command.starts_with("opencode run"),
        "expected opencode default command, got: {}",
        ready.command
    );
    // "Needs Fixes" carries the prompt override verbatim.
    let needs_fixes_prompt = cfg.effective_prompt_template("Needs Fixes", "fallback");
    assert!(needs_fixes_prompt.contains("Address the CI failures"));
    // E2E lane: stronger model + longer turn.
    let e2e = cfg.effective_llm_for_state("Needs Fixes - E2E");
    assert_eq!(e2e.provider, AgentProvider::OpenCode);
    assert_eq!(e2e.model, "anthropic/claude-opus-4-7");
    assert_eq!(e2e.turn_timeout_ms, 5_400_000);

    // §4.3 — local Ollama-with-LSP. Sinfonia's model field is passed
    // through verbatim; OpenCode's own config picks the underlying
    // provider. No api_key needed.
    let ollama_local = r#"---
tracker:
  kind: linear
  api_key: x
  project_slug: p
agent:
  provider: opencode
  model: ollama/qwen2.5-coder:32b
---
body
"#;
    let def = sinfonia::config::parse_workflow_str(ollama_local).unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    assert_eq!(cfg.llm.provider, AgentProvider::OpenCode);
    assert_eq!(cfg.llm.model, "ollama/qwen2.5-coder:32b");
    // No api_key resolution attempted (CLI providers own their own auth).
    assert!(cfg.llm.api_key.is_none());
}

#[test]
fn cli_provider_without_command_fails_validation() {
    let yaml = r#"---
tracker:
  kind: linear
  api_key: x
  project_slug: p
agent:
  provider: claude_code
  command: ""
---
"#;
    let def = sinfonia::config::parse_workflow_str(yaml).unwrap();
    let cfg = ServiceConfig::from_workflow(&def).unwrap();
    // command parsed as ""; preflight validation must reject it.
    let err = cfg.validate_for_dispatch().unwrap_err();
    assert!(
        format!("{err}").contains("command"),
        "expected command-required validation error, got: {err}"
    );
}

// Helpers -----------------------------------------------------------------------

fn sample_issue(id: &str, state: &str, priority: Option<i64>) -> Issue {
    Issue {
        id: id.into(),
        identifier: id.into(),
        title: "T".into(),
        description: None,
        priority,
        state: state.into(),
        branch_name: None,
        url: None,
        labels: vec![],
        blocked_by: vec![],
        children: vec![],
        created_at: None,
        updated_at: None,
        fields: Default::default(),
    }
}
