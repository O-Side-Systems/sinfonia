//! Workspace management and hook execution (spec §9).

mod hooks;
mod manager;

pub use hooks::{run_hook, HookKind};
pub use manager::{Workspace, WorkspaceManager};
