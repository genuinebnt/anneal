//! A durable job queue in the database: enqueue, claim with a lease, ack, retry with backoff, dead-letter.
//!
//! WHY in the database: a job can be enqueued in the SAME transaction as the data it concerns, so
//! "user created but no verification mail queued" cannot happen.
//! Delivery is AT LEAST ONCE: a worker may finish the side effect and die before `ack`, and the lease
//! expiry hands the job to another worker. Handlers must be idempotent, or the receiver must dedupe.
//! Fencing: `attempts` increments on every claim, and `ack`/`fail` require the claimed value, so a slow
//! worker whose lease was taken over cannot overwrite the new owner's result.
//! Production alternatives: Postgres `FOR UPDATE SKIP LOCKED`, `sqlxmq`, `apalis`, SQS, Kafka.

use serde_json::Value;
use sqlx::SqliteConnection;

const MAX_BACKOFF_MS: i64 = 3_600_000;

#[derive(Debug, Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub payload: String,
    pub attempts: i64,
    pub max_attempts: i64,
}

pub struct NewJob<'a> {
    pub id: &'a str,
    pub kind: &'a str,
    pub payload: &'a Value,
    pub run_at: i64,
    pub max_attempts: i64,
    /// Same key twice = one job. `None` = always insert.
    pub dedupe_key: Option<&'a str>,
}

/// Returns `false` when the dedupe key already exists.
pub async fn enqueue(c: &mut SqliteConnection, j: NewJob<'_>) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "INSERT INTO jobs (id, kind, payload, state, run_at, max_attempts, dedupe_key, created_at) \
         VALUES (?,?,?,'queued',?,?,?,?) ON CONFLICT(dedupe_key) DO NOTHING",
    )
    .bind(j.id)
    .bind(j.kind)
    .bind(j.payload.to_string())
    .bind(j.run_at)
    .bind(j.max_attempts)
    .bind(j.dedupe_key)
    .bind(j.run_at)
    .execute(c)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Claim the oldest runnable job. The claim is ONE statement, hence atomic: two workers cannot both get it.
pub async fn claim(c: &mut SqliteConnection, now: i64, lease_ms: i64) -> Result<Option<Job>, sqlx::Error> {
    // A job that kills its worker on every attempt never reaches `fail`, so reap it here.
    sqlx::query(
        "UPDATE jobs SET state = 'dead', lease_until = NULL, last_error = 'lease expired on final attempt' \
         WHERE state = 'running' AND lease_until <= ? AND attempts >= max_attempts",
    )
    .bind(now)
    .execute(&mut *c)
    .await?;
    sqlx::query_as(
        "UPDATE jobs SET state = 'running', attempts = attempts + 1, lease_until = ? WHERE id = ( \
           SELECT id FROM jobs WHERE (state = 'queued' AND run_at <= ?) \
              OR (state = 'running' AND lease_until <= ?) ORDER BY run_at, id LIMIT 1) \
         RETURNING id, kind, payload, attempts, max_attempts",
    )
    .bind(now + lease_ms)
    .bind(now)
    .bind(now)
    .fetch_optional(c)
    .await
}

/// `false` = this worker no longer owns the job.
pub async fn ack(c: &mut SqliteConnection, job: &Job) -> Result<bool, sqlx::Error> {
    // Delivered mail contains live one-time tokens: scrub it so the queue is not a token store.
    let r = sqlx::query(
        "UPDATE jobs SET state = 'done', lease_until = NULL, \
         payload = CASE WHEN kind = 'email' THEN '{}' ELSE payload END \
         WHERE id = ? AND state = 'running' AND attempts = ?",
    )
    .bind(&job.id)
    .bind(job.attempts)
    .execute(c)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Delay before the attempt after attempt number `attempts`: base, 2*base, 4*base, ... capped.
/// Real systems add random jitter so that jobs failed by one outage do not all retry at once.
#[must_use]
pub fn backoff_ms(base_secs: i64, attempts: i64) -> i64 {
    let doublings = (attempts - 1).clamp(0, 30) as u32;
    base_secs.saturating_mul(1000).saturating_mul(1 << doublings).min(MAX_BACKOFF_MS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Try again later.
    Retry,
    /// Retrying cannot help (for example a payload that does not parse).
    Permanent,
}

pub async fn fail(
    c: &mut SqliteConnection,
    job: &Job,
    error: &str,
    kind: Failure,
    now: i64,
    base_secs: i64,
) -> Result<bool, sqlx::Error> {
    let dead = kind == Failure::Permanent || job.attempts >= job.max_attempts;
    let r = sqlx::query(
        "UPDATE jobs SET state = ?, run_at = ?, lease_until = NULL, last_error = ? \
         WHERE id = ? AND state = 'running' AND attempts = ?",
    )
    .bind(if dead { "dead" } else { "queued" })
    .bind(now + backoff_ms(base_secs, job.attempts))
    .bind(error.chars().take(500).collect::<String>())
    .bind(&job.id)
    .bind(job.attempts)
    .execute(c)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Move a dead-lettered job back to the queue with a fresh attempt budget (an operator action).
pub async fn requeue_dead(c: &mut SqliteConnection, id: &str, now: i64) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE jobs SET state = 'queued', attempts = 0, run_at = ? WHERE id = ? AND state = 'dead'")
        .bind(now)
        .bind(id)
        .execute(c)
        .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn counts_by_state(c: &mut SqliteConnection) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as("SELECT state, count(*) FROM jobs GROUP BY state ORDER BY state").fetch_all(c).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    async fn put(c: &mut SqliteConnection, id: &str, kind: &str, max: i64, key: Option<&str>) -> bool {
        let payload = json!({"token": "live-secret"});
        enqueue(c, NewJob { id, kind, payload: &payload, run_at: 100, max_attempts: max, dedupe_key: key })
            .await
            .unwrap()
    }

    async fn state(c: &mut SqliteConnection, id: &str) -> (String, String) {
        sqlx::query_as("SELECT state, payload FROM jobs WHERE id = ?").bind(id).fetch_one(c).await.unwrap()
    }

    #[tokio::test]
    async fn claim_respects_run_at_and_is_exclusive() {
        let db = Db::memory().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        assert!(put(&mut c, "j1", "x", 3, None).await);
        assert!(claim(&mut c, 99, 50).await.unwrap().is_none(), "not due yet");
        assert_eq!(claim(&mut c, 100, 50).await.unwrap().unwrap().attempts, 1);
        assert!(claim(&mut c, 149, 50).await.unwrap().is_none(), "leased");
    }

    #[tokio::test]
    async fn expired_lease_is_taken_over_and_old_worker_is_fenced() {
        let db = Db::memory().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        put(&mut c, "j1", "x", 3, None).await;
        let slow = claim(&mut c, 100, 50).await.unwrap().unwrap();
        let new = claim(&mut c, 150, 50).await.unwrap().unwrap();
        assert_eq!(new.attempts, 2);
        assert!(!ack(&mut c, &slow).await.unwrap(), "stale ack");
        assert!(!fail(&mut c, &slow, "late", Failure::Retry, 151, 1).await.unwrap(), "stale fail");
        assert!(ack(&mut c, &new).await.unwrap());
    }

    #[tokio::test]
    async fn retries_back_off_then_dead_letter_then_requeue() {
        let db = Db::memory().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        put(&mut c, "j1", "x", 3, None).await;
        let mut now = 100;
        for expected_delay in [2_000, 4_000] {
            let j = claim(&mut c, now, 50).await.unwrap().unwrap();
            fail(&mut c, &j, "boom", Failure::Retry, now, 2).await.unwrap();
            assert!(claim(&mut c, now + expected_delay - 1, 50).await.unwrap().is_none(), "too early");
            now += expected_delay;
        }
        let j = claim(&mut c, now, 50).await.unwrap().unwrap();
        fail(&mut c, &j, "boom", Failure::Retry, now, 2).await.unwrap();
        assert_eq!(state(&mut c, "j1").await.0, "dead");
        assert!(claim(&mut c, now + 10_000_000, 50).await.unwrap().is_none());
        assert!(requeue_dead(&mut c, "j1", now).await.unwrap());
        assert_eq!(claim(&mut c, now, 50).await.unwrap().unwrap().attempts, 1);
    }

    #[tokio::test]
    async fn crash_loop_ends_in_the_dead_letter_queue() {
        let db = Db::memory().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        put(&mut c, "j1", "x", 2, None).await;
        assert!(claim(&mut c, 100, 10).await.unwrap().is_some());
        assert!(claim(&mut c, 110, 10).await.unwrap().is_some());
        assert!(claim(&mut c, 120, 10).await.unwrap().is_none(), "no third attempt");
        assert_eq!(state(&mut c, "j1").await.0, "dead");
    }

    #[tokio::test]
    async fn dedupe_and_email_scrubbing() {
        let db = Db::memory().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        assert!(put(&mut c, "j1", "email", 3, Some("k")).await);
        assert!(!put(&mut c, "j2", "email", 3, Some("k")).await);
        let j = claim(&mut c, 100, 10).await.unwrap().unwrap();
        ack(&mut c, &j).await.unwrap();
        assert_eq!(state(&mut c, "j1").await, ("done".into(), "{}".into()));
    }

    #[test]
    fn backoff_doubles_and_saturates() {
        let seq: Vec<i64> = (1..=4).map(|a| backoff_ms(2, a)).collect();
        assert_eq!(seq, [2_000, 4_000, 8_000, 16_000]);
        assert_eq!(backoff_ms(2, 1_000), MAX_BACKOFF_MS);
        assert_eq!(backoff_ms(i64::MAX, 5), MAX_BACKOFF_MS);
    }
}
