use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    // --- Workflow / config (§5.5, §6) ---
    #[error("missing_workflow_file: {0}")]
    MissingWorkflowFile(String),
    #[error("workflow_parse_error: {0}")]
    WorkflowParseError(String),
    #[error("workflow_front_matter_not_a_map")]
    WorkflowFrontMatterNotMap,
    #[error("template_parse_error: {0}")]
    TemplateParseError(String),
    #[error("template_render_error: {0}")]
    TemplateRenderError(String),
    #[error("config_invalid: {0}")]
    ConfigInvalid(String),

    // --- Tracker (§11.4) ---
    #[error("unsupported_tracker_kind: {0}")]
    UnsupportedTrackerKind(String),
    #[error("missing_tracker_api_key")]
    MissingTrackerApiKey,
    #[error("missing_tracker_project_slug")]
    MissingTrackerProjectSlug,
    #[error("linear_api_request: {0}")]
    LinearApiRequest(String),
    #[error("linear_api_status: {0}")]
    LinearApiStatus(String),
    #[error("linear_graphql_errors: {0}")]
    LinearGraphqlErrors(String),
    #[error("linear_unknown_payload: {0}")]
    LinearUnknownPayload(String),
    #[error("linear_missing_end_cursor")]
    LinearMissingEndCursor,
    #[error("jira_api_request: {0}")]
    JiraApiRequest(String),
    #[error("jira_api_status: {0}")]
    JiraApiStatus(String),
    #[error("jira_unknown_payload: {0}")]
    JiraUnknownPayload(String),

    // --- Workspace (§9) ---
    #[error("workspace_create: {0}")]
    WorkspaceCreate(String),
    #[error("workspace_out_of_root: workspace={workspace}, root={root}")]
    WorkspaceOutOfRoot { workspace: String, root: String },
    #[error("hook_failed: name={name}, reason={reason}")]
    HookFailed { name: String, reason: String },
    #[error("hook_timeout: name={name}")]
    HookTimeout { name: String },

    // --- Agent runner (§10.6) ---
    #[error("codex_not_found: {0}")]
    CodexNotFound(String),
    #[error("invalid_workspace_cwd: {0}")]
    InvalidWorkspaceCwd(String),
    #[error("response_timeout")]
    ResponseTimeout,
    #[error("turn_timeout")]
    TurnTimeout,
    #[error("port_exit: {0}")]
    PortExit(String),
    #[error("response_error: {0}")]
    ResponseError(String),
    #[error("turn_failed: {0}")]
    TurnFailed(String),
    #[error("turn_cancelled")]
    TurnCancelled,
    #[error("turn_input_required")]
    TurnInputRequired,
    #[error("llm_api: {0}")]
    LlmApi(String),

    // --- IO / generic ---
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
