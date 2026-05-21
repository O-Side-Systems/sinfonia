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
use sinfonia_bridge::webhook::{router, AppState};
use std::net::SocketAddr;
use std::path::PathBuf;
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

    let state = AppState::new(cfg);
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
