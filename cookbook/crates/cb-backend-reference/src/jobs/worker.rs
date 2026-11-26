//! The worker: claim one job, run its handler under a timeout, record the outcome.
//!
//! `run_one` is the whole state machine and is what tests call directly (no sleeping, no races).
//! `run_pool` just loops it. Shutdown drains: the stop signal is only observed BETWEEN jobs, so an
//! in-flight job finishes and is acked instead of being abandoned until its lease expires.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::queue::{self, Failure, Job};
use super::{relay, webhook};
use crate::config::JobsConfig;
use crate::db::{Db, accounts};
use crate::ports::{Clock, Email, Mailer};

pub const EMAIL_JOB: &str = "email";
pub const PURGE_IDEMPOTENCY_JOB: &str = "purge_idempotency";
pub const PURGE_JOBS_JOB: &str = "purge_jobs";

const IDEMPOTENCY_TTL_MS: i64 = 24 * 3_600_000;
const DONE_JOB_TTL_MS: i64 = 7 * 24 * 3_600_000;

/// Everything a job handler may touch. Smaller than `AppState`, so jobs are testable without HTTP.
#[derive(Clone)]
pub struct JobContext {
    pub db: Db,
    pub clock: Arc<dyn Clock>,
    pub mailer: Arc<dyn Mailer>,
    pub http: reqwest::Client,
    pub cfg: JobsConfig,
}

#[derive(Debug)]
struct JobError {
    kind: Failure,
    message: String,
}

impl JobError {
    fn retry(e: impl std::fmt::Display) -> Self {
        Self { kind: Failure::Retry, message: e.to_string() }
    }
    fn permanent(e: impl std::fmt::Display) -> Self {
        Self { kind: Failure::Permanent, message: e.to_string() }
    }
}

fn parse<T: serde::de::DeserializeOwned>(job: &Job) -> Result<T, JobError> {
    serde_json::from_str(&job.payload).map_err(JobError::permanent)
}

#[derive(Deserialize)]
struct WebhookPayload {
    webhook_id: String,
    org_id: String,
    event: Value,
}

async fn send_webhook(ctx: &JobContext, job: &Job) -> Result<(), JobError> {
    let p: WebhookPayload = parse(job)?;
    let hook = {
        // Release the connection before the network call: never hold a pooled connection across slow I/O.
        let mut c = ctx.db.pool().acquire().await.map_err(JobError::retry)?;
        accounts::webhook_by_id(&mut c, &p.webhook_id).await.map_err(JobError::retry)?
    };
    let hook = hook.filter(|h| h.org_id == p.org_id).ok_or_else(|| JobError::permanent("webhook is gone"))?;
    let delivery = webhook::Delivery {
        url: &hook.url,
        secret: &hook.secret,
        delivery_id: &job.id,
        body: p.event.to_string(),
        now_secs: ctx.clock.now_ms() / 1000,
    };
    webhook::deliver(&ctx.http, delivery).await.map_err(JobError::retry)
}

async fn purge(ctx: &JobContext, sql: &'static str, ttl_ms: i64) -> Result<(), JobError> {
    sqlx::query(sql)
        .bind(ctx.clock.now_ms() - ttl_ms)
        .execute(ctx.db.pool())
        .await
        .map(|_| ())
        .map_err(JobError::retry)
}

async fn handle(ctx: &JobContext, job: &Job) -> Result<(), JobError> {
    match job.kind.as_str() {
        EMAIL_JOB => {
            let mail: Email = parse(job)?;
            ctx.mailer.send(&mail).await.map_err(JobError::retry)
        }
        relay::WEBHOOK_JOB => send_webhook(ctx, job).await,
        PURGE_IDEMPOTENCY_JOB => purge(ctx, "DELETE FROM idempotency WHERE created_at < ?", IDEMPOTENCY_TTL_MS).await,
        PURGE_JOBS_JOB => {
            purge(ctx, "DELETE FROM jobs WHERE state = 'done' AND created_at < ?", DONE_JOB_TTL_MS).await
        }
        other => Err(JobError::permanent(format!("unknown job kind {other:?}"))),
    }
}

/// Process at most one due job. `Ok(false)` = nothing to do.
pub async fn run_one(ctx: &JobContext) -> Result<bool, sqlx::Error> {
    let lease_ms = ctx.cfg.lease_secs.saturating_mul(1000);
    let job = {
        let mut c = ctx.db.pool().acquire().await?;
        queue::claim(&mut c, ctx.clock.now_ms(), lease_ms).await?
    };
    let Some(job) = job else { return Ok(false) };
    // Give up before the lease does, so that two workers are not running the job at the same time.
    let budget = Duration::from_millis((lease_ms as u64).saturating_mul(4) / 5);
    let outcome = tokio::time::timeout(budget, handle(ctx, &job))
        .await
        .unwrap_or_else(|_| Err(JobError::retry("handler timed out")));
    let mut c = ctx.db.pool().acquire().await?;
    let owned = match outcome {
        Ok(()) => queue::ack(&mut c, &job).await?,
        Err(e) => {
            tracing::warn!(job = %job.id, kind = %job.kind, attempt = job.attempts, error = %e.message, "job failed");
            queue::fail(&mut c, &job, &e.message, e.kind, ctx.clock.now_ms(), ctx.cfg.backoff_base_secs).await?
        }
    };
    if !owned {
        tracing::warn!(job = %job.id, "lease was lost; outcome discarded");
    }
    Ok(true)
}

/// Run due jobs until none is left. Returns how many ran. For tests and the demo.
pub async fn drain_due(ctx: &JobContext) -> Result<usize, sqlx::Error> {
    let mut n = 0;
    while run_one(ctx).await? {
        n += 1;
    }
    Ok(n)
}

async fn worker_loop(ctx: JobContext, stop: CancellationToken) {
    let idle = Duration::from_millis(ctx.cfg.poll_ms);
    while !stop.is_cancelled() {
        // `run_one` is deliberately NOT inside `select!`: cancelling it would drop a half-done job.
        let worked = run_one(&ctx).await.unwrap_or_else(|e| {
            tracing::error!(error = %e, "worker could not reach the queue");
            false
        });
        if !worked {
            tokio::select! {
                () = stop.cancelled() => {}
                () = tokio::time::sleep(idle) => {}
            }
        }
    }
}

/// Spawn `cfg.workers` loops. Await the returned set to know they have drained.
#[must_use]
pub fn run_pool(ctx: &JobContext, stop: &CancellationToken) -> tokio::task::JoinSet<()> {
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..ctx.cfg.workers {
        set.spawn(worker_loop(ctx.clone(), stop.clone()));
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::queue::NewJob;
    use crate::ports::{ManualClock, MemoryMailer};
    use async_trait::async_trait;
    use serde_json::json;
    use tokio::sync::Semaphore;

    async fn context(mailer: Arc<dyn Mailer>, clock: Arc<ManualClock>) -> JobContext {
        let db = Db::temp(Duration::from_secs(5), 4).await.unwrap();
        let cfg = JobsConfig { workers: 1, poll_ms: 1, lease_secs: 30, max_attempts: 3, backoff_base_secs: 2 };
        JobContext { db, clock, mailer, http: webhook::client(Duration::from_secs(5)).unwrap(), cfg }
    }

    async fn put(ctx: &JobContext, id: &str, kind: &str, payload: Value) {
        let mut c = ctx.db.pool().acquire().await.unwrap();
        let job = NewJob { id, kind, payload: &payload, run_at: 0, max_attempts: 3, dedupe_key: None };
        queue::enqueue(&mut c, job).await.unwrap();
    }

    async fn states(ctx: &JobContext) -> Vec<(String, i64)> {
        let mut c = ctx.db.pool().acquire().await.unwrap();
        queue::counts_by_state(&mut c).await.unwrap()
    }

    fn mail() -> Value {
        json!({"to": "a@x.io", "subject": "s", "body": "b"})
    }

    #[tokio::test]
    async fn flaky_mailer_is_retried_until_it_works() {
        let (mailer, clock) = (Arc::new(MemoryMailer::default()), Arc::new(ManualClock::new(1_000)));
        let ctx = context(mailer.clone(), clock.clone()).await;
        put(&ctx, "j", EMAIL_JOB, mail()).await;
        mailer.fail_next(2);
        assert_eq!(drain_due(&ctx).await.unwrap(), 1);
        assert!(mailer.sent().is_empty());
        clock.advance_secs(2);
        assert_eq!(drain_due(&ctx).await.unwrap(), 1);
        clock.advance_secs(4);
        assert_eq!(drain_due(&ctx).await.unwrap(), 1);
        assert_eq!(mailer.sent().len(), 1);
        assert_eq!(states(&ctx).await, [("done".to_string(), 1)]);
    }

    #[tokio::test]
    async fn bad_payload_and_unknown_kind_go_straight_to_the_dlq() {
        let ctx = context(Arc::new(MemoryMailer::default()), Arc::new(ManualClock::new(1_000))).await;
        put(&ctx, "j1", EMAIL_JOB, json!({"nope": 1})).await;
        put(&ctx, "j2", "mystery", json!({})).await;
        assert_eq!(drain_due(&ctx).await.unwrap(), 2);
        assert_eq!(states(&ctx).await, [("dead".to_string(), 2)]);
    }

    /// A mailer that blocks until the test lets it go.
    struct Gated {
        entered: Semaphore,
        release: Semaphore,
    }

    #[async_trait]
    impl Mailer for Gated {
        async fn send(&self, _: &Email) -> Result<(), crate::ports::MailError> {
            self.entered.add_permits(1);
            self.release.acquire().await.unwrap().forget();
            Ok(())
        }
    }

    #[tokio::test]
    async fn shutdown_waits_for_the_job_in_flight_and_starts_no_new_one() {
        let gate = Arc::new(Gated { entered: Semaphore::new(0), release: Semaphore::new(0) });
        let ctx = context(gate.clone(), Arc::new(ManualClock::new(1_000))).await;
        put(&ctx, "j1", EMAIL_JOB, mail()).await;
        put(&ctx, "j2", EMAIL_JOB, mail()).await;
        let stop = CancellationToken::new();
        let mut pool = run_pool(&ctx, &stop);
        gate.entered.acquire().await.unwrap().forget(); // j1 is now in flight
        stop.cancel();
        gate.release.add_permits(10);
        while let Some(r) = pool.join_next().await {
            r.unwrap();
        }
        assert_eq!(states(&ctx).await, [("done".to_string(), 1), ("queued".to_string(), 1)]);
    }
}
