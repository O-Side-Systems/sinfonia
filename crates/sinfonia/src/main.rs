//! Sinfonia CLI entry point.

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use sinfonia::config::{ServiceConfig, WorkflowWatcher};
use sinfonia::orchestrator::Orchestrator;
use sinfonia::tracker;
use sinfonia::workspace::WorkspaceManager;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "sinfonia", about = "Coding-agent orchestrator (Symphony-spec implementation)")]
struct Args {
    /// Path to WORKFLOW.md. Defaults to ./WORKFLOW.md.
    workflow: Option<PathBuf>,
    /// Bind the HTTP dashboard on this port (overrides WORKFLOW.md `server.port`).
    #[arg(long)]
    port: Option<u16>,
    /// Log format: `pretty` (default) or `json`.
    #[arg(long, default_value = "pretty")]
    log_format: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    init_logging(&args.log_format);

    if let Err(e) = run(args).await {
        error!(target: "main", error=%e, "fatal");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let workflow_path = args
        .workflow
        .clone()
        .unwrap_or_else(|| PathBuf::from("./WORKFLOW.md"));
    if !workflow_path.exists() {
        return Err(format!(
            "WORKFLOW.md not found at '{}'",
            workflow_path.display()
        )
        .into());
    }

    let workflow = sinfonia::config::read_workflow_file(&workflow_path)?;
    let cfg = ServiceConfig::from_workflow(&workflow)?;
    cfg.validate_for_dispatch()?;

    let tracker = tracker::build_from_config(&cfg)?;
    let workspace = Arc::new(WorkspaceManager::new(&cfg)?);

    let orch = Orchestrator::new(workflow.clone(), cfg.clone(), tracker, workspace).await?;

    // HTTP server.
    let port = args.port.or(cfg.server.port);
    if let Some(p) = port {
        let bind: SocketAddr = format!("{}:{}", cfg.server.bind, p)
            .parse()
            .map_err(|e| format!("invalid bind/port: {e}"))?;
        let orch_clone = orch.clone();
        tokio::spawn(async move {
            if let Err(e) = sinfonia::http::serve(orch_clone, bind).await {
                error!(target: "http", error=%e, "http server stopped");
            }
        });
    }

    // Workflow watcher for dynamic reload.
    spawn_watcher(orch.clone(), workflow_path.clone());

    info!(target: "main", path = %workflow_path.display(), "sinfonia started");
    orch.run().await?;
    Ok(())
}

fn spawn_watcher(orch: Orchestrator, path: PathBuf) {
    tokio::spawn(async move {
        let mut watcher = match WorkflowWatcher::new(&path) {
            Ok(w) => w,
            Err(e) => {
                warn!(target: "main", error=%e, "file watcher unavailable; reload-on-change disabled");
                return;
            }
        };
        while let Some(()) = watcher.rx.recv().await {
            match reload(&orch, &path).await {
                Ok(()) => {}
                Err(e) => warn!(target: "main", error=%e, "workflow reload failed; keeping last good config"),
            }
        }
    });
}

async fn reload(
    orch: &Orchestrator,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let workflow = sinfonia::config::read_workflow_file(path)?;
    let cfg = ServiceConfig::from_workflow(&workflow)?;
    cfg.validate_for_dispatch()?;
    let tracker = tracker::build_from_config(&cfg)?;
    let workspace = Arc::new(WorkspaceManager::new(&cfg)?);
    orch.apply_reload(workflow, cfg, tracker, workspace).await;
    Ok(())
}

fn init_logging(format: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if format == "json" {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}
