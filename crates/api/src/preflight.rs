//! Keeping progress safe across deploys.
//!
//! - [`apply_renames`] runs at startup: rows stored under a problem's old id (`renamed_from` in problem.toml) move
//!   to its current id, so renaming or moving a problem keeps its attempts, reviews, drafts and focus time.
//! - [`check`] is `anneal-api preflight`, which deploy/deploy.sh runs with the new image against the live database
//!   before switching over. It fails when the new version would strand progress (an id with stored rows that the
//!   new content doesn't know) or when an applied migration was edited (sqlx would refuse to start). The old
//!   version keeps serving until both are fixed.

use std::collections::HashSet;

use anneal_content::Catalog;
use sqlx::PgPool;

/// Tables whose rows belong to a problem.
const PROBLEM_TABLES: [&str; 6] = ["attempts", "runs", "drafts", "reviews", "focus_time", "scratch"];

/// Moves progress from old problem ids to current ones. Idempotent: once moved, nothing matches the old id.
pub async fn apply_renames(db: &PgPool, catalog: &Catalog) -> sqlx::Result<u64> {
    let mut moved = 0;
    let mut tx = db.begin().await?;
    for (old, new) in catalog.renames() {
        // History tables: every row moves.
        for table in ["attempts", "runs"] {
            let q = format!("UPDATE {table} SET problem_id = $2 WHERE problem_id = $1");
            moved += sqlx::query(&q).bind(old).bind(new).execute(&mut *tx).await?.rows_affected();
        }
        // One row per problem: keep the current problem's row if it already has one.
        for table in ["drafts", "reviews", "scratch"] {
            let q = format!(
                "UPDATE {table} SET problem_id = $2 WHERE problem_id = $1
                 AND NOT EXISTS (SELECT 1 FROM {table} WHERE problem_id = $2)"
            );
            moved += sqlx::query(&q).bind(old).bind(new).execute(&mut *tx).await?.rows_affected();
            // What did not move because the current problem already has a row is kept in the archive.
            crate::store::archive_rows(&mut tx, "rename_conflict", table, "problem_id", &[old.to_string()]).await?;
            sqlx::query(&format!("DELETE FROM {table} WHERE problem_id = $1")).bind(old).execute(&mut *tx).await?;
        }
        // Focus time is per day: add the old seconds to the new problem's day.
        moved += sqlx::query(
            "INSERT INTO focus_time (day, problem_id, seconds)
             SELECT day, $2, seconds FROM focus_time WHERE problem_id = $1
             ON CONFLICT (day, problem_id) DO UPDATE SET seconds = focus_time.seconds + EXCLUDED.seconds",
        )
        .bind(old)
        .bind(new)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query("DELETE FROM focus_time WHERE problem_id = $1").bind(old).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(moved)
}

/// Deletes the progress of retired problems (`content/retired.txt`): removed on purpose, so it doesn't strand
/// anything. Idempotent. Returns how many rows went.
pub async fn purge_retired(db: &PgPool, catalog: &Catalog) -> sqlx::Result<u64> {
    if catalog.retired.is_empty() {
        return Ok(0);
    }
    let ids: Vec<&str> = catalog.retired.iter().map(String::as_str).collect();
    let mut tx = db.begin().await?;
    let mut gone = 0;
    let keys: Vec<String> = catalog.retired.iter().cloned().collect();
    for table in PROBLEM_TABLES {
        crate::store::archive_rows(&mut tx, "purge_retired", table, "problem_id", &keys).await?;
        let q = format!("DELETE FROM {table} WHERE problem_id = ANY($1)");
        gone += sqlx::query(&q).bind(&ids).execute(&mut *tx).await?.rows_affected();
    }
    tx.commit().await?;
    Ok(gone)
}

/// What `preflight` found. Empty lists mean the deploy is safe.
#[derive(Debug, Default)]
pub struct Report {
    /// Problem ids with stored progress that the new content neither has nor lists in any `renamed_from`.
    pub stranded: Vec<(String, i64)>,
    /// Applied migrations whose file changed or disappeared.
    pub migrations: Vec<String>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        self.stranded.is_empty() && self.migrations.is_empty()
    }
}

/// Checks the live database against this build's content and migrations, without changing anything.
pub async fn check(db: &PgPool, catalog: &Catalog, migrator: &sqlx::migrate::Migrator) -> sqlx::Result<Report> {
    let mut report = Report::default();
    let existing: HashSet<String> =
        sqlx::query_scalar("SELECT table_name::text FROM information_schema.tables WHERE table_schema = 'public'")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect();

    if existing.contains("_sqlx_migrations") {
        let applied: Vec<(i64, Vec<u8>)> =
            sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations WHERE success ORDER BY version")
                .fetch_all(db)
                .await?;
        for (version, checksum) in applied {
            match migrator.iter().find(|m| m.version == version) {
                None => report.migrations.push(format!("{version}: applied, but its file is gone")),
                Some(m) if *m.checksum != *checksum => {
                    report.migrations.push(format!("{version} ({}): edited after it was applied", m.description))
                }
                Some(_) => {}
            }
        }
    }

    let tables: Vec<&str> = PROBLEM_TABLES.into_iter().filter(|t| existing.contains(*t)).collect();
    if !tables.is_empty() {
        let union = tables.iter().map(|t| format!("SELECT problem_id FROM {t}")).collect::<Vec<_>>().join(" UNION ALL ");
        let ids: Vec<(String, i64)> =
            sqlx::query_as(&format!("SELECT problem_id, count(*) FROM ({union}) p GROUP BY problem_id ORDER BY problem_id"))
                .fetch_all(db)
                .await?;
        report.stranded = ids.into_iter().filter(|(id, _)| !catalog.knows(id)).collect();
    }
    Ok(report)
}
