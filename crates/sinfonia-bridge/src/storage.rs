//! SQLite-backed bridge state.
//!
//! Two tables, both owned by P1-E:
//!
//! - `processed_deliveries` — GitHub delivery-ID idempotency. The bridge
//!   inserts the delivery ID once per accepted webhook. A duplicate
//!   delivery (GitHub retries on a 5xx, or a misconfigured load balancer
//!   replays the request) fails the insert with [`Error::Storage`] and
//!   the handler returns 200 without doing any further work.
//! - `pr_ticket_map` — PR ↔ tracker-ticket mapping, populated by
//!   `pull_request opened` / `synchronize` events when the configured
//!   `pr_link_pattern` matches the PR body or title. SQLite is the source
//!   of truth during the bridge's lifetime; if the row is missing on
//!   restart the bridge re-derives it from the next PR event or from a
//!   startup replay (Phase 1 only does the event path).
//!
//! Concurrency: a single `rusqlite::Connection` wrapped in
//! [`tokio::sync::Mutex`]. Webhook traffic at v0.3 scale is tens of
//! events per minute at most (see `01-bridge-mvp.md` §11 question 4), so
//! serializing DB access behind one mutex is cheaper than running a pool
//! and easier to reason about. If the bridge ever sits behind enough
//! traffic to make this a bottleneck, the swap to `r2d2_sqlite` is
//! mechanical.

use crate::{Error, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OpenFlags};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Handle to the bridge's SQLite database.
///
/// Cloning yields another reference to the same underlying connection
/// (via `Arc<Mutex<…>>`) — both clones can be handed to different axum
/// handlers without coordination.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    /// Open (or create) the database at `path` and run the one-shot
    /// schema migration. The parent directory is created if missing,
    /// matching the developer-friendly behaviour of
    /// `crates/sinfonia/src/config/loader.rs` for its own state files.
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(|e| Error::Storage(format!("open '{}': {e}", path.display())))?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database. Used by unit tests so each test starts
    /// from a clean slate without hitting the filesystem.
    #[cfg(test)]
    pub async fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| Error::Storage(format!("open in-memory: {e}")))?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Record a webhook delivery as processed.
    ///
    /// Returns `Ok(())` on the first call for a given `delivery_id` and
    /// `Err(Error::Storage("duplicate"))` on a redelivery. The handler
    /// translates `duplicate` into a 200 no-op response so GitHub stops
    /// retrying.
    pub async fn record_delivery(&self, delivery_id: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now().timestamp();
        match conn.execute(
            "INSERT INTO processed_deliveries (delivery_id, processed_at) VALUES (?1, ?2)",
            params![delivery_id, now],
        ) {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(Error::Storage("duplicate".into()))
            }
            Err(e) => Err(Error::Storage(format!("record_delivery: {e}"))),
        }
    }

    /// Upsert a PR ↔ ticket mapping.
    ///
    /// Called on `pull_request opened` / `synchronize` after
    /// `pr_link_pattern` matches the PR body or title. On
    /// `synchronize` the mapping may already exist; we overwrite so the
    /// most recent extraction wins (a contributor can edit a PR body to
    /// change the tracker link).
    pub async fn upsert_pr_ticket(
        &self,
        repo: &str,
        pr_number: u64,
        ticket_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().await;
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO pr_ticket_map (repo, pr_number, ticket_id, discovered_at) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(repo, pr_number) DO UPDATE SET \
                ticket_id = excluded.ticket_id, \
                discovered_at = excluded.discovered_at",
            params![repo, pr_number as i64, ticket_id, now],
        )
        .map_err(|e| Error::Storage(format!("upsert_pr_ticket: {e}")))?;
        Ok(())
    }

    /// Look up a previously-recorded ticket ID for a (repo, PR) pair.
    /// Returns `Ok(None)` if no mapping exists.
    pub async fn lookup_pr_ticket(&self, repo: &str, pr_number: u64) -> Result<Option<String>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare(
                "SELECT ticket_id FROM pr_ticket_map WHERE repo = ?1 AND pr_number = ?2",
            )
            .map_err(|e| Error::Storage(format!("lookup_pr_ticket prepare: {e}")))?;
        let mut rows = stmt
            .query(params![repo, pr_number as i64])
            .map_err(|e| Error::Storage(format!("lookup_pr_ticket query: {e}")))?;
        match rows
            .next()
            .map_err(|e| Error::Storage(format!("lookup_pr_ticket next: {e}")))?
        {
            Some(row) => Ok(Some(
                row.get::<_, String>(0)
                    .map_err(|e| Error::Storage(format!("lookup_pr_ticket col: {e}")))?,
            )),
            None => Ok(None),
        }
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS processed_deliveries (\
            delivery_id TEXT PRIMARY KEY, \
            processed_at INTEGER NOT NULL\
         );\
         CREATE TABLE IF NOT EXISTS pr_ticket_map (\
            repo TEXT NOT NULL, \
            pr_number INTEGER NOT NULL, \
            ticket_id TEXT NOT NULL, \
            discovered_at INTEGER NOT NULL, \
            PRIMARY KEY(repo, pr_number)\
         );",
    )
    .map_err(|e| Error::Storage(format!("migrate: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_delivery_new_id_succeeds() {
        let store = Store::open_in_memory().await.expect("open");
        store
            .record_delivery("abc-123")
            .await
            .expect("first insert should succeed");
    }

    #[tokio::test]
    async fn record_delivery_duplicate_errors() {
        let store = Store::open_in_memory().await.expect("open");
        store.record_delivery("dup-1").await.expect("first insert");
        let err = store
            .record_delivery("dup-1")
            .await
            .expect_err("duplicate insert should error");
        match err {
            Error::Storage(s) => assert_eq!(s, "duplicate"),
            other => panic!("expected Storage(\"duplicate\"), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn upsert_pr_ticket_insert_then_update() {
        let store = Store::open_in_memory().await.expect("open");
        store
            .upsert_pr_ticket("acme/repo", 42, "ENG-1")
            .await
            .expect("initial upsert");
        assert_eq!(
            store
                .lookup_pr_ticket("acme/repo", 42)
                .await
                .expect("lookup"),
            Some("ENG-1".into())
        );

        // PR body edited — new tracker link replaces the old one.
        store
            .upsert_pr_ticket("acme/repo", 42, "ENG-2")
            .await
            .expect("second upsert");
        assert_eq!(
            store
                .lookup_pr_ticket("acme/repo", 42)
                .await
                .expect("lookup-after-update"),
            Some("ENG-2".into())
        );
    }

    #[tokio::test]
    async fn lookup_pr_ticket_missing_returns_none() {
        let store = Store::open_in_memory().await.expect("open");
        assert_eq!(
            store
                .lookup_pr_ticket("acme/repo", 99)
                .await
                .expect("lookup"),
            None
        );
    }

    #[tokio::test]
    async fn restart_replay_reads_same_row() {
        // Persistent path so a second `Store::open` sees what the first wrote.
        let dir = tempfile::tempdir().expect("tmpdir");
        let path = dir.path().join("bridge.db");

        // First "process": insert a row, then drop the store.
        {
            let store = Store::open(&path).await.expect("open-1");
            store
                .upsert_pr_ticket("acme/repo", 7, "ENG-7")
                .await
                .expect("upsert");
            store
                .record_delivery("delivery-7")
                .await
                .expect("record");
        }

        // Second "process": fresh handle on the same file should see the row
        // and reject a redelivery of the same ID.
        let store = Store::open(&path).await.expect("open-2");
        assert_eq!(
            store
                .lookup_pr_ticket("acme/repo", 7)
                .await
                .expect("lookup"),
            Some("ENG-7".into()),
            "restart should observe the prior upsert"
        );
        let err = store
            .record_delivery("delivery-7")
            .await
            .expect_err("redelivery after restart should still be duplicate");
        assert!(matches!(err, Error::Storage(ref s) if s == "duplicate"));
    }

    #[tokio::test]
    async fn open_creates_missing_parent_directory() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let nested = dir.path().join("a/b/c/bridge.db");
        let _store = Store::open(&nested).await.expect("open with nested parent");
        assert!(nested.exists(), "DB file should exist at nested path");
    }
}
