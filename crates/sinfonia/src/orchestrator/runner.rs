//! Worker: workspace + prompt + agent session per attempt (spec §16.5).

use crate::agent;
use crate::agent::events::{AgentEvent, EventSender};
use crate::agent::TurnOutcome;
use crate::config::{ServiceConfig, WorkflowDefinition};
use crate::domain::Issue;
use crate::errors::{Error, Result};
use crate::template;
use crate::tracker::IssueTracker;
use crate::workspace::{run_hook, HookKind, WorkspaceManager};
use chrono::Utc;
use std::sync::Arc;
use tracing::{debug, info};

#[allow(clippy::too_many_arguments)]
pub async fn run_agent_attempt(
    mut issue: Issue,
    initial_attempt: Option<u32>,
    cfg: &ServiceConfig,
    workflow: &WorkflowDefinition,
    tracker: Arc<dyn IssueTracker>,
    workspace_mgr: Arc<WorkspaceManager>,
    events: EventSender,
) -> Result<()> {
    let ws = workspace_mgr.ensure_for_issue(&issue.identifier)?;

    if ws.created_now {
        if let Some(script) = cfg.hooks.after_create.as_deref() {
            run_hook(HookKind::AfterCreate, script, &ws.path, cfg.hooks.timeout_ms).await?;
        }
    }
    if let Some(script) = cfg.hooks.before_run.as_deref() {
        run_hook(HookKind::BeforeRun, script, &ws.path, cfg.hooks.timeout_ms).await?;
    }

    // Build the agent for the issue's current tracker state. The state machine
    // (config `states:` block) routes each state to a different provider/model.
    let llm = cfg.effective_llm_for_state(&issue.state);
    info!(target: "runner",
        issue_identifier = %issue.identifier,
        tracker_state = %issue.state,
        provider = ?llm.provider,
        model = %llm.model,
        "agent selected for state"
    );
    let agent = agent::build_for(cfg, &llm)?;

    // Spec §10.2: start session with workspace as cwd.
    let mut session = match agent.start_session(&issue, ws.path.clone()).await {
        Ok(s) => {
            events.send(AgentEvent::SessionStarted {
                timestamp: Utc::now(),
                thread_id: s.thread_id.clone(),
                codex_app_server_pid: None,
            });
            s
        }
        Err(e) => {
            events.send(AgentEvent::StartupFailed {
                timestamp: Utc::now(),
                message: e.to_string(),
            });
            run_after_run_best_effort(&cfg.hooks, &ws.path).await;
            return Err(e);
        }
    };

    let max_turns = cfg.agent.max_turns.max(1);
    let mut turn_number: u32 = 1;
    let mut current_attempt = initial_attempt;
    let active_states_lc: Vec<String> = cfg
        .tracker
        .active_states
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    let outcome: Result<()> = async {
        loop {
            let prompt_input_attempt = if turn_number == 1 {
                current_attempt
            } else {
                Some(turn_number)
            };
            // Prompt template can be per-state; falls back to the workflow body.
            let template_body = cfg.effective_prompt_template(&issue.state, &workflow.prompt_template);
            let prompt = if turn_number == 1 {
                template::render_prompt(template_body, &issue, prompt_input_attempt)?
            } else {
                format!(
                    "Continue working on issue {} (turn {}/{}). The full task description was sent at the start of this thread; do not repeat it. Make additional progress if needed, then call `finish`.",
                    issue.identifier, turn_number, max_turns
                )
            };

            let res = agent
                .run_turn(&mut session, &prompt, turn_number == 1, &events)
                .await?;
            match res {
                TurnOutcome::Completed { .. } => {}
                TurnOutcome::Failed(msg) => {
                    return Err(Error::TurnFailed(msg));
                }
                TurnOutcome::Timeout => return Err(Error::TurnTimeout),
                TurnOutcome::InputRequired => return Err(Error::TurnInputRequired),
            }

            // §16.5: refresh issue state and continue inside the same worker if still active.
            let prior_state = issue.state.clone();
            match tracker.fetch_issue_states_by_ids(&[issue.id.clone()]).await {
                Ok(states) => {
                    if let Some(s) = states.into_iter().next() {
                        issue.state = s.state.clone();
                    }
                }
                Err(e) => {
                    debug!(target: "runner", error=%e, "post-turn state refresh failed");
                    return Err(Error::Other(format!("post-turn state refresh: {e}")));
                }
            }
            let state_l = issue.normalized_state();
            if !active_states_lc.iter().any(|a| a == &state_l) {
                info!(target: "runner", issue_identifier=%issue.identifier, state=%issue.state, "issue no longer active; ending worker");
                break;
            }
            // If the state machine routes this state to a different provider, end this
            // worker so the next dispatch picks up with the right agent. The orchestrator
            // will re-dispatch via the continuation retry.
            if prior_state.to_lowercase() != issue.state.to_lowercase() {
                let next_llm = cfg.effective_llm_for_state(&issue.state);
                if next_llm.provider != llm.provider || next_llm.model != llm.model {
                    info!(
                        target: "runner",
                        issue_identifier = %issue.identifier,
                        from = %prior_state,
                        to = %issue.state,
                        "state changed; handing off to a different runner"
                    );
                    break;
                }
            }
            if turn_number >= max_turns {
                info!(target: "runner", issue_identifier=%issue.identifier, "max_turns reached");
                break;
            }
            turn_number += 1;
            current_attempt = Some(turn_number);
        }
        Ok(())
    }
    .await;

    let _ = agent.stop_session(session).await;
    run_after_run_best_effort(&cfg.hooks, &ws.path).await;
    outcome
}

async fn run_after_run_best_effort(hooks: &crate::config::HooksConfig, ws: &std::path::Path) {
    if let Some(script) = hooks.after_run.as_deref() {
        if let Err(e) = run_hook(HookKind::AfterRun, script, ws, hooks.timeout_ms).await {
            debug!(target: "runner", error=%e, "after_run hook failed (ignored)");
        }
    }
}
