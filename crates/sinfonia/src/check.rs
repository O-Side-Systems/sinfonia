//! `sinfonia --check WORKFLOW.md` implementation (Phase 5 §3.1).
//!
//! Loads a workflow file, runs the same schema validation `dispatch_run`
//! would run, then renders every prompt template (workflow body + per-state
//! overrides) against a stub Issue to catch template-compile / strict-mode
//! errors before the operator hits "go."
//!
//! Returns a typed exit code so the calling skill can branch on the failure
//! class without grepping stderr.
//!
//! Mapping rationale (each `Error::*` lands in exactly one bucket):
//! - `Yaml`, `WorkflowParseError`, `WorkflowFrontMatterNotMap` → 2 (YAML parse)
//! - `ConfigInvalid`, `Tracker(MissingTrackerProjectSlug)`,
//!   `Tracker(UnsupportedTrackerKind)` → 3 (schema)
//! - `TemplateParseError`, `TemplateRenderError` → 4 (template)
//! - `Tracker(MissingTrackerApiKey)` → 5 (tracker auth)
//! - Anything else (file missing, IO) falls out to the caller as a generic
//!   `Err(_)` which `main.rs` treats as exit 1.

use sinfonia::config::{ServiceConfig, WorkflowDefinition};
use sinfonia::errors::Error;
use std::path::PathBuf;

use crate::RunArgs;

/// Exit codes for `--check`. Plan §3.1.
#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CheckExit {
    Ok = 0,
    YamlParse = 2,
    Schema = 3,
    Template = 4,
    TrackerAuth = 5,
}

/// Public entry for `main.rs` — runs the full check pipeline.
pub(crate) fn check_workflow(args: &RunArgs) -> Result<(), (CheckExit, String)> {
    let workflow_path = args
        .workflow
        .clone()
        .unwrap_or_else(|| PathBuf::from("./WORKFLOW.md"));

    if !workflow_path.exists() {
        // File-not-found is a precondition failure, not a check class. Map to
        // schema (3) so skills can present "fix your path or YAML" remediation.
        return Err((
            CheckExit::Schema,
            format!("WORKFLOW.md not found at '{}'", workflow_path.display()),
        ));
    }

    let workflow = sinfonia::config::read_workflow_file(&workflow_path)
        .map_err(|e| (classify(&e), e.to_string()))?;
    let cfg = ServiceConfig::from_workflow(&workflow).map_err(|e| (classify(&e), e.to_string()))?;
    cfg.validate_for_dispatch()
        .map_err(|e| (classify(&e), e.to_string()))?;

    render_all_prompts(&workflow, &cfg).map_err(|e| (CheckExit::Template, e))?;

    Ok(())
}

/// Render every prompt template the orchestrator would render at runtime,
/// against a fixed stub `Issue`. Returns the first template error encountered
/// (annotated with which state it came from).
fn render_all_prompts(workflow: &WorkflowDefinition, cfg: &ServiceConfig) -> Result<(), String> {
    let stub = stub_issue();

    // Workflow-body template (the global prompt) always renders.
    sinfonia::template::render_prompt(&workflow.prompt_template, &stub, Some(1))
        .map_err(|e| format!("workflow prompt: {e}"))?;

    // Per-state overrides only render when they override the prompt.
    for (state_name, ov) in cfg.states.iter() {
        if let Some(body) = ov.prompt_template.as_deref() {
            sinfonia::template::render_prompt(body, &stub, Some(1))
                .map_err(|e| format!("state '{state_name}' prompt: {e}"))?;
        }
    }

    Ok(())
}

fn stub_issue() -> sinfonia::domain::Issue {
    sinfonia::domain::Issue {
        id: "stub-id".into(),
        identifier: "STUB-1".into(),
        title: "Stub issue (for --check)".into(),
        description: Some("Synthetic Issue used only by `sinfonia --check`.".into()),
        priority: Some(2),
        state: "Todo".into(),
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

fn classify(e: &Error) -> CheckExit {
    match e {
        Error::Yaml(_) | Error::WorkflowParseError(_) | Error::WorkflowFrontMatterNotMap => {
            CheckExit::YamlParse
        }
        Error::ConfigInvalid(_) => CheckExit::Schema,
        Error::TemplateParseError(_) | Error::TemplateRenderError(_) => CheckExit::Template,
        Error::Tracker(t) => classify_tracker(t),
        // Anything else (IO, JSON, missing-file via the loader's
        // MissingWorkflowFile, agent errors) is a precondition failure rather
        // than a class the plan-doc names. Schema is the safest bucket — it
        // matches "the structure isn't usable as-is" without forging a new
        // exit code.
        _ => CheckExit::Schema,
    }
}

fn classify_tracker(e: &sinfonia_tracker::Error) -> CheckExit {
    use sinfonia_tracker::Error as T;
    match e {
        T::MissingTrackerApiKey => CheckExit::TrackerAuth,
        T::MissingTrackerProjectSlug | T::UnsupportedTrackerKind(_) => CheckExit::Schema,
        _ => CheckExit::Schema,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn check_text(text: &str) -> Result<(), (CheckExit, String)> {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(text.as_bytes()).unwrap();
        let args = RunArgs {
            workflow: Some(f.path().to_path_buf()),
            port: None,
            log_format: "pretty".into(),
            check: true,
        };
        check_workflow(&args)
    }

    fn valid_workflow_text() -> String {
        // A minimal WORKFLOW.md that should pass --check when its tracker
        // api_key env var is set. The validators look at `tracker.kind`,
        // `tracker.api_key`, `tracker.project_slug`, and the prompt body.
        //
        // We deliberately reference a test-unique env var (rather than the
        // canonical `LINEAR_API_KEY`) so this test can run in parallel with
        // siblings that exercise the auth-missing path. `std::env::set_var`
        // is process-wide; using distinct names keeps the suite race-free.
        std::env::set_var("SINFONIA_CHECK_TEST_LINEAR_KEY", "test-key-not-used");
        r#"---
tracker:
  kind: linear
  api_key: $SINFONIA_CHECK_TEST_LINEAR_KEY
  project_slug: my-project
agent:
  provider: anthropic
llm:
  api_key: $ANTHROPIC_API_KEY
  model: claude-opus-4-7
---
You are working on {{ issue.identifier }}: {{ issue.title }}.
"#
        .to_string()
    }

    #[test]
    fn yaml_parse_error_returns_exit_2() {
        let bad = "---\ntracker: {kind: linear, [bogus\n---\nbody\n";
        let err = check_text(bad).unwrap_err();
        assert_eq!(err.0, CheckExit::YamlParse, "got: {:?}", err);
    }

    #[test]
    fn missing_project_slug_returns_schema_exit_3() {
        std::env::set_var("SINFONIA_CHECK_TEST_PROJSLUG_KEY", "test-key");
        let text = r#"---
tracker:
  kind: linear
  api_key: $SINFONIA_CHECK_TEST_PROJSLUG_KEY
agent:
  provider: anthropic
---
hello
"#;
        let err = check_text(text).unwrap_err();
        assert_eq!(err.0, CheckExit::Schema, "got: {:?}", err);
    }

    #[test]
    fn missing_tracker_api_key_returns_exit_5() {
        // Reference a test-unique env-var name that is NOT set so the
        // resolver yields None. Avoids racing with siblings that do
        // `set_var("LINEAR_API_KEY", ...)` — see `valid_workflow_text`.
        std::env::remove_var("SINFONIA_CHECK_TEST_MISSING_KEY");
        let text = r#"---
tracker:
  kind: linear
  api_key: $SINFONIA_CHECK_TEST_MISSING_KEY
  project_slug: my-project
agent:
  provider: anthropic
---
hello
"#;
        let err = check_text(text).unwrap_err();
        assert_eq!(err.0, CheckExit::TrackerAuth, "got: {:?}", err);
    }

    #[test]
    fn template_parse_error_returns_exit_4() {
        let text = valid_workflow_text().replace(
            "{{ issue.identifier }}: {{ issue.title }}.",
            // strict Liquid rejects an unknown filter at parse time
            "{{ issue.title | nosuchfilter }}",
        );
        let err = check_text(&text).unwrap_err();
        assert_eq!(err.0, CheckExit::Template, "got: {:?}", err);
    }

    #[test]
    fn template_unknown_variable_returns_exit_4() {
        // `c` is out of scope after `endfor`; strict Liquid rejects this at
        // render time. Belongs to the same Template exit bucket.
        let text = valid_workflow_text().replace(
            "{{ issue.identifier }}: {{ issue.title }}.",
            "{% for c in issue.children %}{{ c.identifier }}{% endfor %}{{ c.identifier }}",
        );
        let err = check_text(&text).unwrap_err();
        assert_eq!(err.0, CheckExit::Template, "got: {:?}", err);
    }

    #[test]
    fn valid_workflow_returns_ok() {
        let text = valid_workflow_text();
        check_text(&text).expect("valid workflow should pass --check");
    }
}
