//! The outbox relay: turns committed outbox rows into (a) published events for SSE and (b) webhook jobs.
//!
//! Everything happens in one transaction: publish, fan out and mark-as-published commit together, so a
//! crash leaves the outbox row unpublished and it is simply relayed again. With an EXTERNAL broker that
//! atomicity is impossible, publishing becomes at-least-once, and consumers must dedupe.
//! Recipe: `examples/microservices/transactional_outbox_inbox_sqlite.rs`.

use serde_json::{Value, json};

use super::queue::{self, NewJob};
use crate::db::{Db, accounts};
use crate::ports::IdGen;

pub const WEBHOOK_JOB: &str = "webhook";
const BATCH: i64 = 100;

#[derive(sqlx::FromRow)]
struct OutboxRow {
    id: i64,
    org_id: String,
    topic: String,
    payload: String,
    created_at: i64,
}

/// Relay up to one batch. Returns how many rows were published.
pub async fn relay_once(db: &Db, ids: &dyn IdGen, now: i64, max_attempts: i64) -> Result<usize, sqlx::Error> {
    let mut tx = db.begin().await?;
    let rows: Vec<OutboxRow> = sqlx::query_as(
        "SELECT id, org_id, topic, payload, created_at FROM outbox WHERE published_at IS NULL ORDER BY id LIMIT ?",
    )
    .bind(BATCH)
    .fetch_all(&mut *tx)
    .await?;
    for row in &rows {
        let seq = sqlx::query("INSERT INTO events (org_id, topic, payload, at) VALUES (?,?,?,?)")
            .bind(&row.org_id)
            .bind(&row.topic)
            .bind(&row.payload)
            .bind(row.created_at)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid();
        let data: Value = serde_json::from_str(&row.payload).unwrap_or(Value::Null);
        for hook in accounts::webhooks_for_org(&mut tx, &row.org_id).await? {
            let payload = json!({
                "webhook_id": hook.id,
                "org_id": row.org_id,
                "event": { "id": seq, "topic": row.topic, "data": data },
            });
            let key = format!("wh:{}:{}", row.id, hook.id);
            let job = NewJob {
                id: &ids.new_id(),
                kind: WEBHOOK_JOB,
                payload: &payload,
                run_at: now,
                max_attempts,
                dedupe_key: Some(&key),
            };
            queue::enqueue(&mut tx, job).await?;
        }
        sqlx::query("UPDATE outbox SET published_at = ? WHERE id = ?")
            .bind(now)
            .bind(row.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(rows.len())
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct EventRow {
    pub seq: i64,
    pub topic: String,
    pub payload: String,
}

/// Events of ONE tenant after `after_seq`, oldest first.
pub async fn events_after(db: &Db, org_id: &str, after_seq: i64, limit: i64) -> Result<Vec<EventRow>, sqlx::Error> {
    sqlx::query_as("SELECT seq, topic, payload FROM events WHERE org_id = ? AND seq > ? ORDER BY seq LIMIT ?")
        .bind(org_id)
        .bind(after_seq)
        .bind(limit)
        .fetch_all(db.pool())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::outbox_push;
    use crate::ports::SeqIds;

    #[tokio::test]
    async fn publishes_once_and_fans_out_only_to_the_same_tenant() {
        let db = Db::memory().await.unwrap();
        {
            let mut c = db.pool().acquire().await.unwrap();
            for (hook, org) in [("h1", "o1"), ("h2", "o1"), ("h3", "o2")] {
                accounts::insert_webhook(&mut c, hook, org, "http://127.0.0.1:1/", "s", 0).await.unwrap();
            }
            outbox_push(&mut c, "o1", "task.created", &json!({"id": "t1"}), 1).await.unwrap();
        }
        let ids = SeqIds::default();
        assert_eq!(relay_once(&db, &ids, 2, 5).await.unwrap(), 1);
        assert_eq!(relay_once(&db, &ids, 3, 5).await.unwrap(), 0, "already published");
        assert_eq!(events_after(&db, "o1", 0, 10).await.unwrap().len(), 1);
        assert!(events_after(&db, "o2", 0, 10).await.unwrap().is_empty());
        let targets: Vec<String> =
            sqlx::query_scalar("SELECT json_extract(payload, '$.webhook_id') FROM jobs ORDER BY 1")
                .fetch_all(db.pool())
                .await
                .unwrap();
        assert_eq!(targets, ["h1", "h2"]);
    }

    #[tokio::test]
    async fn failure_mid_batch_publishes_nothing() {
        let db = Db::memory().await.unwrap();
        {
            let mut c = db.pool().acquire().await.unwrap();
            accounts::insert_webhook(&mut c, "h1", "o1", "http://127.0.0.1:1/", "s", 0).await.unwrap();
            outbox_push(&mut c, "o1", "a", &json!({}), 1).await.unwrap();
        }
        // Fault injection: enqueueing the webhook job fails after the event row has been inserted.
        sqlx::query("CREATE TRIGGER boom BEFORE INSERT ON jobs BEGIN SELECT RAISE(ABORT, 'injected'); END")
            .execute(db.pool())
            .await
            .unwrap();
        assert!(relay_once(&db, &SeqIds::default(), 2, 5).await.is_err());
        assert!(events_after(&db, "o1", 0, 10).await.unwrap().is_empty(), "event rolled back");
        sqlx::query("DROP TRIGGER boom").execute(db.pool()).await.unwrap();
        assert_eq!(relay_once(&db, &SeqIds::default(), 3, 5).await.unwrap(), 1, "retried in full");
        assert_eq!(events_after(&db, "o1", 0, 10).await.unwrap().len(), 1);
    }
}
