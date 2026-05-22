//! `sinfonia init` REPL (Phase 5 §3.2).
//!
//! The AI-tool-free equivalent of the `setup-workflow` skill (`skills/setup-workflow/`).
//! When a user is bootstrapping Sinfonia itself — before they have a coding
//! agent configured to drive the skill — this subcommand walks the same
//! questions as the skill's `SKILL.md` procedure and emits a WORKFLOW.md.
//!
//! The flow is linear with abort-on-error (plan §7 open question 2 default).
//! No back-stepping; the user can re-run `sinfonia init` to start over.
//!
//! Live tracker API validation is intentionally NOT performed here — the
//! skill drives it when an AI tool runs the equivalent flow. `sinfonia init`
//! generates a config; `sinfonia --check` validates the result statically.

use inquire::{Confirm, Select, Text};
use std::path::{Path, PathBuf};

use crate::{check, InitArgs, RunArgs};

const TRACKER_LINEAR: &str = "linear";
const TRACKER_JIRA_CLOUD: &str = "jira-cloud";
const TRACKER_JIRA_SERVER: &str = "jira-server";

/// Top-level REPL entry. Returns `Ok(())` on success or a wrapped error that
/// `main.rs` will report and exit 1 on.
pub(crate) fn run(args: InitArgs) -> Result<(), Box<dyn std::error::Error>> {
    let out_path = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("./WORKFLOW.md"));

    if out_path.exists() && !confirm_overwrite(&out_path)? {
        eprintln!("aborted: {} already exists", out_path.display());
        return Ok(());
    }

    println!("sinfonia init — scaffold a new WORKFLOW.md");
    println!("Press Ctrl-C at any prompt to abort.\n");

    let answers = collect_answers()?;
    let rendered = render_workflow(&answers);

    std::fs::write(&out_path, &rendered)?;
    println!("\nwrote {}", out_path.display());

    if !args.no_validate {
        let check_args = RunArgs {
            workflow: Some(out_path.clone()),
            port: None,
            log_format: "pretty".into(),
            check: true,
        };
        match check::check_workflow(&check_args) {
            Ok(()) => println!("validation: ok"),
            Err((code, msg)) => {
                println!("validation: failed ({:?}): {msg}", code);
                println!("(this is usually because a required env-var isn't set yet — see the comments in the generated file)");
            }
        }
    }

    println!("\nnext steps:");
    println!("  1. review {}", out_path.display());
    println!("  2. export the env vars referenced in the file (LINEAR_API_KEY / ANTHROPIC_API_KEY / etc.)");
    println!("  3. run `sinfonia` to start the orchestrator");

    Ok(())
}

fn confirm_overwrite(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(
        Confirm::new(&format!("{} already exists — overwrite?", path.display()))
            .with_default(false)
            .prompt()?,
    )
}

/// Final answer-bundle the renderer consumes. One struct so the rendering
/// code doesn't have to thread a dozen arguments.
#[derive(Debug, Clone)]
pub(crate) struct Answers {
    pub tracker_kind: String, // "linear" / "jira-cloud" / "jira-server"
    pub project_slug: String,
    pub tracker_endpoint: Option<String>,
    pub tracker_email: Option<String>,
    pub active_states: Vec<String>,
    pub terminal_states: Vec<String>,
    pub agent_backend: String, // §2.5 list
    pub workspace_root: String,
}

fn collect_answers() -> Result<Answers, Box<dyn std::error::Error>> {
    let tracker_kind = Select::new(
        "Which tracker?",
        vec![TRACKER_LINEAR, TRACKER_JIRA_CLOUD, TRACKER_JIRA_SERVER],
    )
    .prompt()?
    .to_string();

    let project_slug = Text::new("Project slug (Linear: project key; Jira: project key like 'ENG'):")
        .prompt()?;
    if project_slug.trim().is_empty() {
        return Err("project_slug is required".into());
    }

    let tracker_endpoint = match tracker_kind.as_str() {
        TRACKER_JIRA_CLOUD => Some(
            Text::new("Jira Cloud endpoint (e.g. https://yourorg.atlassian.net):")
                .prompt()?,
        ),
        TRACKER_JIRA_SERVER => Some(
            Text::new("Jira Server/DC endpoint (e.g. https://jira.example.com):")
                .prompt()?,
        ),
        _ => None,
    };
    let tracker_email = if tracker_kind == TRACKER_JIRA_CLOUD {
        Some(Text::new("Atlassian account email:").prompt()?)
    } else {
        None
    };

    let active_states = comma_list(
        Text::new("Active states (Sinfonia dispatches an agent on these):")
            .with_default("Todo, In Progress")
            .prompt()?,
    );
    let terminal_states = comma_list(
        Text::new("Terminal states (Sinfonia ignores these):")
            .with_default("Done, Cancelled, Closed")
            .prompt()?,
    );

    let agent_backend = Select::new(
        "Default agent backend?",
        vec![
            "claude_code",
            "codex",
            "opencode",
            "anthropic",
            "openai",
            "google",
            "ollama",
        ],
    )
    .with_help_message("CLI backends require the corresponding CLI installed; API backends require an env var.")
    .prompt()?
    .to_string();

    let workspace_root = Text::new("Workspace root (where Sinfonia clones per-issue copies):")
        .with_default("./workspaces")
        .prompt()?;

    Ok(Answers {
        tracker_kind,
        project_slug,
        tracker_endpoint,
        tracker_email,
        active_states,
        terminal_states,
        agent_backend,
        workspace_root,
    })
}

fn comma_list(s: String) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Render the WORKFLOW.md text. Hand-rolled rather than Liquid because the
/// template is small, fully static, and benefits from being readable next to
/// the answer struct. Skills use Liquid for the same purpose (plan §2.1) —
/// the duplication is deliberate: skills run inside AI tools, `init` doesn't.
pub(crate) fn render_workflow(a: &Answers) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("# Generated by `sinfonia init`. Edit freely.\n");
    out.push_str("tracker:\n");

    match a.tracker_kind.as_str() {
        TRACKER_LINEAR => {
            out.push_str("  kind: linear\n");
            out.push_str("  api_key: $LINEAR_API_KEY\n");
            out.push_str(&format!("  project_slug: {}\n", a.project_slug));
        }
        TRACKER_JIRA_CLOUD => {
            out.push_str("  kind: jira\n");
            out.push_str("  api_key: $JIRA_API_TOKEN\n");
            out.push_str(&format!("  project_slug: {}\n", a.project_slug));
            if let Some(ep) = &a.tracker_endpoint {
                out.push_str(&format!("  endpoint: {}\n", ep));
            }
            if let Some(em) = &a.tracker_email {
                out.push_str(&format!("  email: {}\n", em));
            }
        }
        TRACKER_JIRA_SERVER => {
            out.push_str("  kind: jira\n");
            out.push_str("  api_key: $JIRA_PAT\n");
            out.push_str(&format!("  project_slug: {}\n", a.project_slug));
            if let Some(ep) = &a.tracker_endpoint {
                out.push_str(&format!("  endpoint: {}\n", ep));
            }
            // Server/DC uses PAT-only Bearer auth; no email required.
        }
        _ => {}
    }

    out.push_str("  active_states:\n");
    for s in &a.active_states {
        out.push_str(&format!("    - \"{}\"\n", s));
    }
    out.push_str("  terminal_states:\n");
    for s in &a.terminal_states {
        out.push_str(&format!("    - \"{}\"\n", s));
    }

    out.push_str("\nagent:\n");
    out.push_str(&format!("  provider: {}\n", a.agent_backend));

    // LLM block — only the API-backed providers need an env-var key.
    let api_env = match a.agent_backend.as_str() {
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "google" => Some("GOOGLE_API_KEY"),
        _ => None,
    };
    if let Some(env) = api_env {
        out.push_str("\nllm:\n");
        out.push_str(&format!("  api_key: ${}\n", env));
        out.push_str(&format!("  model: {}\n", default_model_for(a.agent_backend.as_str())));
    }

    out.push_str("\nworkspace:\n");
    out.push_str(&format!("  root: {}\n", a.workspace_root));

    out.push_str("---\n");
    out.push_str("\nYou are working on issue {{ issue.identifier }}: {{ issue.title }}.\n");
    out.push_str("\n{% if issue.description %}{{ issue.description }}{% endif %}\n");
    out
}

fn default_model_for(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-opus-4-7",
        "openai" => "gpt-4",
        "google" => "gemini-2.0-pro",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline_answers() -> Answers {
        Answers {
            tracker_kind: TRACKER_LINEAR.into(),
            project_slug: "my-proj".into(),
            tracker_endpoint: None,
            tracker_email: None,
            active_states: vec!["Todo".into(), "In Progress".into()],
            terminal_states: vec!["Done".into(), "Cancelled".into()],
            agent_backend: "anthropic".into(),
            workspace_root: "./workspaces".into(),
        }
    }

    #[test]
    fn render_linear_anthropic_workflow_parses_under_check() {
        // Roundtrip: render → parse → validate. This is the contract `init`
        // promises every successful REPL session: the file it writes is
        // accepted by `sinfonia --check` (modulo the env-var dance, which we
        // satisfy here). The canonical `LINEAR_API_KEY` / `ANTHROPIC_API_KEY`
        // env-var names are what real users set; we don't have a way to avoid
        // them here since `render_workflow` writes them by design. The
        // sibling `check::tests` use unique names to avoid racing this test.
        std::env::set_var("LINEAR_API_KEY", "test-key");
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let rendered = render_workflow(&baseline_answers());
        let def = sinfonia::config::parse_workflow_str(&rendered).expect("yaml parses");
        let cfg = sinfonia::config::ServiceConfig::from_workflow(&def).expect("schema valid");
        cfg.validate_for_dispatch().expect("validate_for_dispatch");
    }

    #[test]
    fn render_jira_cloud_includes_email_and_endpoint() {
        let mut a = baseline_answers();
        a.tracker_kind = TRACKER_JIRA_CLOUD.into();
        a.tracker_endpoint = Some("https://example.atlassian.net".into());
        a.tracker_email = Some("user@example.com".into());
        a.agent_backend = "claude_code".into();
        let r = render_workflow(&a);
        assert!(r.contains("kind: jira"));
        assert!(r.contains("endpoint: https://example.atlassian.net"));
        assert!(r.contains("email: user@example.com"));
        assert!(r.contains("api_key: $JIRA_API_TOKEN"));
    }

    #[test]
    fn render_jira_server_uses_pat_env_no_email() {
        let mut a = baseline_answers();
        a.tracker_kind = TRACKER_JIRA_SERVER.into();
        a.tracker_endpoint = Some("https://jira.example.com".into());
        let r = render_workflow(&a);
        assert!(r.contains("api_key: $JIRA_PAT"));
        assert!(r.contains("endpoint: https://jira.example.com"));
        assert!(!r.contains("email:"), "Server/DC PAT mode has no email field");
    }

    #[test]
    fn render_cli_backend_omits_llm_block_api_env() {
        let mut a = baseline_answers();
        a.agent_backend = "claude_code".into();
        let r = render_workflow(&a);
        // CLI backends drive auth via the tool itself, not an env-var key.
        assert!(!r.contains("ANTHROPIC_API_KEY"));
        assert!(!r.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn comma_list_strips_and_drops_empty() {
        assert_eq!(
            comma_list("Todo, In Progress ,,Ready".into()),
            vec!["Todo", "In Progress", "Ready"]
        );
    }
}
