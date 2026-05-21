//! File-watcher around WORKFLOW.md for dynamic reload (spec §6.2).

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::warn;

/// Lightweight wrapper that pushes a debounced `()` notification every time the
/// watched workflow file changes.
pub struct WorkflowWatcher {
    _watcher: RecommendedWatcher,
    pub rx: mpsc::Receiver<()>,
}

impl WorkflowWatcher {
    pub fn new(path: &Path) -> std::io::Result<Self> {
        let (raw_tx, raw_rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(ev) = res {
                if matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = raw_tx.send(());
                }
            }
        })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Watch the parent dir so atomic-rename writes (vim, editors) are caught.
        let parent = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        watcher
            .watch(&parent, RecursiveMode::NonRecursive)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let (tx, rx) = mpsc::channel(8);
        let target = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());

        // Spawn a debouncer task that coalesces bursts of raw events.
        tokio::spawn(async move {
            debounce_loop(raw_rx, tx, target).await;
        });

        Ok(WorkflowWatcher {
            _watcher: watcher,
            rx,
        })
    }
}

async fn debounce_loop(
    raw_rx: Receiver<()>,
    tx: mpsc::Sender<()>,
    _target: PathBuf,
) {
    let mut last_emit: Option<Instant> = None;
    loop {
        match raw_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(()) => {
                // Drain anything else immediately available.
                while raw_rx.try_recv().is_ok() {}
                let should_emit = match last_emit {
                    None => true,
                    Some(t) => t.elapsed() > Duration::from_millis(200),
                };
                if should_emit {
                    if tx.send(()).await.is_err() {
                        return;
                    }
                    last_emit = Some(Instant::now());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if tx.is_closed() {
                    return;
                }
            }
            Err(_) => {
                warn!("workflow watcher channel closed");
                return;
            }
        }
    }
}
