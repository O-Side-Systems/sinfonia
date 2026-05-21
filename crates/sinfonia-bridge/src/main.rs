//! `sinfonia-bridge` CLI entry point.
//!
//! Two operating modes in P1-D:
//!
//! ```text
//! sinfonia-bridge BRIDGE.md                # serve (default)
//! sinfonia-bridge BRIDGE.md --check        # parse + validate; exit 0/non-zero
//! ```
//!
//! `--self-test` will be added in P1-G; webhook handler bodies in P1-E.

use clap::Parser;
use sinfonia_bridge::config::read_bridge_file;
use sinfonia_bridge::github::{GhOps, OctocrabGhOps};
use sinfonia_bridge::labels::LabelManager;
use sinfonia_bridge::storage::Store;
use sinfonia_bridge::webhook::{router, AppState};
use sinfonia_tracker::{IssueTracker, LinearTracker, TrackerKind};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "sinfonia-bridge",
    about = "CI feedback bridge for Sinfonia — translates GitHub webhooks into tracker state transitions."
)]
struct Args {
    /// Path to `BRIDGE.md`. Defaults to `./BRIDGE.md`.
    bridge: Option<PathBuf>,

    /// Validate `BRIDGE.md` without running. Returns 0 on a clean config,
    /// non-zero on any schema or template error.
    #[arg(long)]
    check: bool,

    /// Override the bind port from `BRIDGE.md` (`server.port`).
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
    let bridge_path = args
        .bridge
        .clone()
        .unwrap_or_else(|| PathBuf::from("./BRIDGE.md"));
    if !bridge_path.exists() {
        return Err(format!("BRIDGE.md not found at '{}'", bridge_path.display()).into());
    }

    let cfg = read_bridge_file(&bridge_path)?;

    if args.check {
        // Match `sinfonia --check`'s convention: print one line and exit 0.
        println!("ok");
        return Ok(());
    }

    let port = args.port.unwrap_or(cfg.server.port);
    let bind: SocketAddr = format!("{}:{}", cfg.server.bind, port)
        .parse()
        .map_err(|e| format!("invalid bind/port: {e}"))?;

    // Open (or create) the bridge state DB before binding the listener.
    // A failure here is fatal — no point listening if we can't record
    // delivery IDs.
    let store = Store::open(&cfg.storage.state_db_path).await?;
    info!(
        target: "main",
        db = %cfg.storage.state_db_path.display(),
        "bridge state DB opened"
    );

    // Construct the tracker adapter once and share it as a trait object
    // so handlers can do tracker writes without per-request setup. Phase
    // 1 only supports Linear (Jira deferred to Phase 4 — BridgeConfig
    // validation already rejects `kind: jira`).
    let tracker_cfg = cfg.tracker.to_tracker_config();
    let tracker: Arc<dyn IssueTracker> = match tracker_cfg.kind {
        TrackerKind::Linear => Arc::new(LinearTracker::new(&tracker_cfg)?),
        TrackerKind::Jira => {
            return Err("BRIDGE.md tracker.kind 'jira' not supported until Phase 4".into());
        }
    };

    // GitHub client. P1-F: PAT mode only. P1-G adds App mode.
    let gh: Arc<dyn GhOps> = match (&cfg.github.pat, &cfg.github.app_id) {
        (Some(token), _) => Arc::new(OctocrabGhOps::from_pat(token.clone())?),
        (None, Some(_)) => {
            return Err(
                "BRIDGE.md github.app_id auth is deferred to P1-G; set github.pat in Phase 1"
                    .into(),
            );
        }
        (None, None) => {
            // Already rejected by BridgeConfig validation; defensive.
            return Err("BRIDGE.md github: neither pat nor app_id is set".into());
        }
    };

    let labels = LabelManager::new(
        gh.clone(),
        cfg.github.manage_labels,
        cfg.github.label_prefix.clone(),
        cfg.github.label_aliases.clone(),
    );
    if !cfg.github.manage_labels {
        info!(target: "main", "label management disabled (manage_labels: false)");
    }

    let state = AppState::new(cfg, store, tracker, gh, labels);
    let app = router(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    let actual = listener.local_addr()?;
    info!(target: "main", %actual, "sinfonia-bridge listening");

    axum::serve(listener, app).await?;
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
