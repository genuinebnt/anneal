//! BLOCK 5: database access. Pool + migrations + unit-of-work transactions + audit + outbox helpers.
//!
//! Style: repositories are free functions taking `&mut SqliteConnection`. A `Tx` derefs to a connection, so
//! the same function works standalone or inside a unit of work; the CALLER decides the transaction boundary
//! (the service/handler), which is what makes "domain write + audit row + outbox row" atomic.
//! Production alternatives: `diesel`, `sea-orm`, plain `tokio-postgres`; Postgres in real deployments.
//! Pitfalls: SQLite upgrades a deferred read transaction to a write lazily and can fail with SQLITE_BUSY
//! mid-transaction, so every `Tx` starts `BEGIN IMMEDIATE` (take the write lock up front, wait via busy_timeout).

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, SqliteConnection, SqlitePool};

use crate::config::DbConfig;

pub mod accounts;
pub mod projects;
pub mod tasks;

pub type Tx = sqlx::Transaction<'static, Sqlite>;

/// Result of an optimistic-locking write. Distinguishing `Stale` (412) from `NotFound` (404)
/// matters: a client that gets 412 should re-fetch and retry, one that gets 404 should stop.
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub enum Write<T> {
    Done(T),
    NotFound,
    Stale,
}

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
    /// Keeps a temporary database directory alive for as long as any clone of `Db` exists.
    _tmp: Option<Arc<tempfile::TempDir>>,
}

impl std::fmt::Debug for Db {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db").field("size", &self.pool.size()).finish()
    }
}

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

fn file_options(path: &std::path::Path, busy: Duration) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        // WAL: readers do not block the single writer and vice versa.
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(busy)
        .foreign_keys(true)
}

impl Db {
    /// Open from config (empty path = temporary database), then run migrations.
    pub async fn open(cfg: &DbConfig) -> Result<Self, sqlx::Error> {
        let busy = Duration::from_millis(cfg.busy_timeout_ms);
        if cfg.path.is_empty() {
            return Self::temp(busy, cfg.max_connections).await;
        }
        let pool = SqlitePoolOptions::new()
            .max_connections(cfg.max_connections)
            .connect_with(file_options(std::path::Path::new(&cfg.path), busy))
            .await?;
        let db = Self { pool, _tmp: None };
        db.migrate().await?;
        Ok(db)
    }

    /// File-backed DB in a temp dir: the closest-to-production fake (real WAL, real locking).
    pub async fn temp(busy: Duration, max_connections: u32) -> Result<Self, sqlx::Error> {
        let dir = tempfile::tempdir().map_err(sqlx::Error::Io)?;
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(file_options(&dir.path().join("app.db"), busy))
            .await?;
        let db = Self { pool, _tmp: Some(Arc::new(dir)) };
        db.migrate().await?;
        Ok(db)
    }

    /// Single-connection in-memory DB for fast repository tests. One connection because every
    /// `:memory:` connection is its own empty database; never hold a `Tx` while also using the pool.
    pub async fn memory() -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(opts)
            .await?;
        let db = Self { pool, _tmp: None };
        db.migrate().await?;
        Ok(db)
    }

    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        MIGRATOR.run(&self.pool).await.map_err(sqlx::Error::from)
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Start a unit of work. Drop without `commit()` rolls back.
    pub async fn begin(&self) -> Result<Tx, sqlx::Error> {
        self.pool.begin_with("BEGIN IMMEDIATE").await
    }

    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").execute(&self.pool).await.map(|_| ())
    }

    /// Path of the temp database file (used by fault-injection tests to open a competing connection).
    #[must_use]
    pub fn temp_path(&self) -> Option<std::path::PathBuf> {
        self._tmp.as_ref().map(|d| d.path().join("app.db"))
    }
}

/// Who is acting, and in which tenant. A struct so that two adjacent string ids cannot be swapped.
#[derive(Debug, Clone, Copy)]
pub struct Actor<'a> {
    pub org_id: &'a str,
    pub user_id: &'a str,
}

/// Append an audit row. Called with the SAME connection/transaction as the change it records,
/// so "changed but not audited" is impossible, and a rolled-back change leaves no audit row.
pub async fn audit(
    conn: &mut SqliteConnection,
    who: Actor<'_>,
    action: &str,
    entity: &str,
    entity_id: &str,
    at: i64,
    detail: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO audit_log (org_id, actor, action, entity, entity_id, at, detail) VALUES (?,?,?,?,?,?,?)")
        .bind(who.org_id)
        .bind(who.user_id)
        .bind(action)
        .bind(entity)
        .bind(entity_id)
        .bind(at)
        .bind(detail.to_string())
        .execute(conn)
        .await
        .map(|_| ())
}

/// Transactional outbox: record "something happened" atomically with the change; a relay publishes it later.
/// This avoids the dual-write problem (DB commit succeeded but the publish crashed, or the reverse).
pub async fn outbox_push(
    conn: &mut SqliteConnection,
    org_id: &str,
    topic: &str,
    payload: &Value,
    at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO outbox (org_id, topic, payload, created_at) VALUES (?,?,?,?)")
        .bind(org_id)
        .bind(topic)
        .bind(payload.to_string())
        .bind(at)
        .execute(conn)
        .await
        .map(|_| ())
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct AuditRow {
    pub id: i64,
    pub org_id: String,
    pub actor: String,
    pub action: String,
    pub entity: String,
    pub entity_id: String,
    pub at: i64,
}

pub async fn audit_list(conn: &mut SqliteConnection, org_id: &str) -> Result<Vec<AuditRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, org_id, actor, action, entity, entity_id, at FROM audit_log WHERE org_id = ? ORDER BY id",
    )
    .bind(org_id)
    .fetch_all(conn)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `begin` must take the write lock immediately. With a deferred transaction the second `begin`
    /// would succeed, and the loser would only fail later, in the middle of its work.
    #[tokio::test]
    async fn a_unit_of_work_takes_the_write_lock_up_front() {
        let db = Db::temp(Duration::from_millis(20), 2).await.unwrap();
        let first = db.begin().await.unwrap();
        assert!(db.begin().await.is_err(), "two units of work held the write lock at once");
        drop(first);
        db.begin().await.unwrap();
    }

    #[tokio::test]
    async fn migrations_apply_and_fts5_exists() {
        let db = Db::memory().await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE name = 'tasks_fts'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(n, 1, "FTS5 virtual table must have been created");
    }

    #[tokio::test]
    async fn audit_and_change_roll_back_together() {
        let db = Db::memory().await.unwrap();
        let mut tx = db.begin().await.unwrap();
        sqlx::query("INSERT INTO orgs (id, name, created_at) VALUES ('o1','Acme',1)")
            .execute(&mut *tx)
            .await
            .unwrap();
        audit(&mut tx, Actor { org_id: "o1", user_id: "u1" }, "org.create", "org", "o1", 1, &serde_json::json!({}))
            .await
            .unwrap();
        drop(tx); // no commit = rollback
        let orgs: i64 = sqlx::query_scalar("SELECT count(*) FROM orgs").fetch_one(db.pool()).await.unwrap();
        let logs: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log").fetch_one(db.pool()).await.unwrap();
        assert_eq!((orgs, logs), (0, 0));
    }

    #[tokio::test]
    async fn committed_unit_of_work_persists_both() {
        let db = Db::memory().await.unwrap();
        let mut tx = db.begin().await.unwrap();
        sqlx::query("INSERT INTO orgs (id, name, created_at) VALUES ('o1','Acme',1)")
            .execute(&mut *tx)
            .await
            .unwrap();
        audit(&mut tx, Actor { org_id: "o1", user_id: "u1" }, "org.create", "org", "o1", 1, &serde_json::json!({}))
            .await
            .unwrap();
        outbox_push(&mut tx, "o1", "org.created", &serde_json::json!({"id":"o1"}), 1).await.unwrap();
        tx.commit().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        assert_eq!(audit_list(&mut c, "o1").await.unwrap().len(), 1);
    }
}
