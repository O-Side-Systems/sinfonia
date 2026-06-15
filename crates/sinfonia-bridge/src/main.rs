//! `sinfonia-bridge` CLI entry point.
//!
//! Three operating modes:
//!
//! ```text
//! sinfonia-bridge BRIDGE.md                # serve (default)
//! sinfonia-bridge BRIDGE.md --check        # parse + validate; exit 0/non-zero
//! sinfonia-bridge BRIDGE.md --self-test    # run install gate; exit = failure count
//! ```
//!
//! `--self-test` is the install-gate runner — it builds the tracker
//! client, the GitHub client, and probes the public webhook URL (if
//! configured), then prints one `PASS` / `FAIL` / `SKIP` line per check.
//! Exit code = number of `FAIL` lines so a `setup-bridge` skill can
//! gate on `[[ $? -eq 0 ]]`.

use clap::Parser;
use sinfonia_bridge::config::read_bridge_file;
use sinfonia_bridge::feedback::budget::{spawn_debounce_reconciler, BudgetManager};
use sinfonia_bridge::feedback::cost::CostTable;
use sinfonia_bridge::github::{build_gh_ops, GhOps};
use sinfonia_bridge::labels::LabelManager;
use sinfonia_bridge::selftest::run_selftest;
use sinfonia_bridge::storage::Store;
use sinfonia_bridge::telemetry;
use sinfonia_bridge::webhook::{router, AppState};
use sinfonia_tracker::{IssueTracker, LinearTracker, TrackerKind};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

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

    /// Run the install-gate self-test and exit. Returns the number of
    /// failed checks; SKIPs do not count. Mutually exclusive with
    /// `--check` (which only parses the config).
    #[arg(long = "self-test")]
    self_test: bool,

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

    match run(args).await {
        Ok(code) if code == 0 => {}
        Ok(code) => std::process::exit(code),
        Err(e) => {
            // If the failure happened after `init_observability`, the
            // subscriber is live and `error!` lands in stdout/OTel. Earlier
            // failures (BRIDGE.md missing, parse errors) fall back to stderr
            // because the subscriber doesn't exist yet.
            error!(target: "main", error=%e, "fatal");
            eprintln!("fatal: {e}");
            std::process::exit(1);
        }
    }
}

async fn run(args: Args) -> Result<i32, Box<dyn std::error::Error>> {
    let bridge_path = args
        .bridge
        .clone()
        .unwrap_or_else(|| PathBuf::from("./BRIDGE.md"));
    if !bridge_path.exists() {
        return Err(format!("BRIDGE.md not found at '{}'", bridge_path.display()).into());
    }

    let cfg = read_bridge_file(&bridge_path)?;

    if args.check && args.self_test {
        return Err("--check and --self-test are mutually exclusive".into());
    }

    if args.check {
        // Match `sinfonia --check`'s convention: print one line and exit 0.
        // No subscriber is installed for --check — the call is a one-shot
        // schema validator and doesn't need the OTel layer.
        println!("ok");
        return Ok(0);
    }

    if args.self_test {
        // Exit code carries the failure count so install scripts can
        // gate on it. SKIPs do not count.
        let failures = run_selftest(&cfg).await;
        return Ok(failures);
    }

    // Install the tracing subscriber + optional OTel layer now that we have
    // the parsed `telemetry:` block. Held across the listener's lifetime;
    // the guard's Drop flushes buffered spans on graceful shutdown.
    let telemetry_guard = telemetry::init_observability(&args.log_format, &cfg.telemetry);
    info!(
        target: "main",
        tenant_id = %telemetry_guard.tenant_id,
        otel_enabled = cfg.telemetry.otlp_endpoint.is_some()
            || std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok(),
        "telemetry initialized"
    );
    // Bind the guard's lifetime to the rest of `run`. Dropped at function
    // return — graceful shutdown of the OTel batch processor.
    let _telemetry_guard = telemetry_guard;

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
    // so handlers can do tracker writes without per-request setup. Phase 4
    // adds Jira alongside Linear; both implement the full §11.6 write
    // surface. Config validation already rejects unsupported combinations
    // (missing endpoint, missing email for Cloud, etc.).
    let tracker_cfg = cfg.tracker.to_tracker_config();
    let tracker: Arc<dyn IssueTracker> = match tracker_cfg.kind {
        TrackerKind::Linear => Arc::new(LinearTracker::new(&tracker_cfg)?),
        TrackerKind::Jira => Arc::new(sinfonia_tracker::JiraTracker::new(&tracker_cfg)?),
    };

    // GitHub client. PAT vs App is selected inside `build_gh_ops` from
    // the parsed config; both modes implement the same `GhOps` trait.
    let gh: Arc<dyn GhOps> = build_gh_ops(&cfg.github)?;

    let labels = LabelManager::new(
        gh.clone(),
        cfg.github.manage_labels,
        cfg.github.label_prefix.clone(),
        cfg.github.label_aliases.clone(),
    );
    if !cfg.github.manage_labels {
        info!(target: "main", "label management disabled (manage_labels: false)");
    }

    // Phase 3 §7.1 — load the cost table (embedded default unless
    // BRIDGE.md provides an override path). The cost-cap freshness gate
    // (M-2) fires inside `BudgetManager::new` if the table is past the
    // 180-day block window.
    let cost_table = CostTable::embedded_default();
    let today = chrono::Utc::now().date_naive();
    if cost_table.is_stale_warn(today) {
        tracing::warn!(
            target: "main",
            verified_at = %cost_table.verified_at,
            "cost table is older than {FRESHNESS_WARN_DAYS} days; consider refreshing",
            FRESHNESS_WARN_DAYS = sinfonia_bridge::feedback::cost::FRESHNESS_WARN_DAYS,
        );
    }

    let budget = BudgetManager::new(cost_table, &cfg, tracker.clone());
    let _debounce_handle = spawn_debounce_reconciler(budget.clone());

    let state = AppState::new(cfg, store, tracker, gh, labels, budget);

    // Merge coordinator (Proposal 0005 §8.4): reconcile every in-flight landing
    // against GitHub's actual state BEFORE the webhook server binds, so an
    // out-of-band human merge that happened while the bridge was down cannot
    // cause a double-merge. No-op unless `merge_coordinator.enabled`.
    if state.config.feedback_loop.merge_coordinator.enabled {
        let ctx = sinfonia_bridge::merge::Ctx {
            config: state.config.as_ref(),
            store: state.store.as_ref(),
            tracker: state.tracker.as_ref(),
            gh: &state.gh,
            labels: &state.labels,
        };
        if let Err(e) = sinfonia_bridge::merge::reconcile_on_boot(&ctx).await {
            tracing::warn!(target: "main", error = %e, "merge coordinator boot reconcile failed (continuing)");
        }
    }

    let app = router(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    let actual = listener.local_addr()?;
    info!(target: "main", %actual, "sinfonia-bridge listening");

    axum::serve(listener, app).await?;
    Ok(0)
}

