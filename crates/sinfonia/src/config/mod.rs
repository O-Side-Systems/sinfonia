//! WORKFLOW.md loader and typed configuration layer (spec §5, §6).

mod loader;
mod typed;
mod watcher;

pub use loader::{parse_workflow_str, read_workflow_file, WorkflowDefinition};
pub use typed::{
    AgentConfig, AgentProvider, DispatchAllowlist, HooksConfig, LlmConfig, PollingConfig,
    ServerConfig, ServiceConfig, TelemetryConfig, TrackerConfig, TrackerKind, WorkspaceConfig,
};
pub use watcher::WorkflowWatcher;
