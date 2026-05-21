//! Sinfonia — a Rust scheduler/runner that orchestrates LLM coding agents against
//! an issue tracker.
//!
//! Sinfonia implements the upstream **Symphony Service Specification** (Draft v1),
//! reproduced at `docs/SPEC.md`. The name is intentionally distinct so that "Symphony"
//! continues to refer to the spec while "Sinfonia" refers to this independent
//! implementation. See `README.md` for the user-facing overview.

pub mod agent;
pub mod config;
pub mod domain;
pub mod errors;
pub mod http;
pub mod orchestrator;
pub mod template;
pub mod tracker;
pub mod workspace;

pub use config::{ServiceConfig, WorkflowDefinition};
pub use domain::{Issue, IssueState, LiveSession, RetryEntry, RunningEntry};
pub use errors::Error;
pub use orchestrator::Orchestrator;
