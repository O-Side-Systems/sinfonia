//! Sinfonia CLI entry point.

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use sinfonia::config::{ServiceConfig, WorkflowWatcher};
use sinfonia::orchestrator::Orchestrator;
use sinfonia::telemetry;
use sinfonia::tracker;
use sinfonia::workspace::WorkspaceManager;
use tracing::{error, info, warn};

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

    if let Err(e) = run(args).await {
        // `run` initializes the subscriber as soon as it has a parsed config,
        // so `error!` works for any failure past that point. Errors raised
        // earlier (e.g. WORKFLOW.md missing) fall back to stderr — the
        // structured subscriber doesn't exist yet to receive them.
        if !e.to_string().is_empty() {
            error!(target: "main", error=%e, "fatal");
            eprintln!("fatal: {e}");
        }
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

    // Install the tracing subscriber + optional OTel layer now that we have
    // the parsed `telemetry:` block. The returned guard flushes buffered
    // spans on drop at end of `main`.
    let _telemetry_guard = telemetry::init_observability(&args.log_format, &cfg.telemetry);
    info!(
        target: "main",
        tenant_id = %cfg.telemetry.tenant_id,
        otel_enabled = cfg.telemetry.otlp_endpoint.is_some()
            || std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok(),
        "telemetry initialized"
    );

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

