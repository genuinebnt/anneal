//! BLOCK 13: running the server and shutting it down without dropping work.
//!
//! One `CancellationToken` drives everything. On cancel: `/readyz` turns 503, the listener stops
//! accepting, in-flight requests finish, SSE streams end, workers finish their current job, and the
//! relay and scheduler stop. All of it is bounded by `shutdown_grace_ms`: past that we give up, and the
//! job leases and database transactions make the interrupted work safe to redo.
//! In Kubernetes also add a short pre-stop delay: load balancers keep sending traffic for a moment
//! after SIGTERM, and a closed listener turns that traffic into errors.
//! Recipe: `examples/async/graceful_shutdown.rs`.

use std::future::IntoFuture;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tracing_subscriber::EnvFilter;

use crate::http;
use crate::jobs::schedule::{self, CronLite, Schedule, When};
use crate::jobs::worker::{PURGE_IDEMPOTENCY_JOB, PURGE_JOBS_JOB, run_pool};
use crate::state::AppState;

const SCHEDULER_PERIOD: Duration = Duration::from_secs(1);

/// BLOCK 12 (part): JSON logs, one object per line. `RUST_LOG` filters. Safe to call more than once.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().json().with_env_filter(filter).with_writer(std::io::stderr).try_init();
}

pub fn default_schedules() -> Result<Vec<Schedule>, String> {
    Ok(vec![
        Schedule { name: "purge-idempotency", job_kind: PURGE_IDEMPOTENCY_JOB, when: When::EverySecs(3600) },
        Schedule {
            name: "purge-done-jobs",
            job_kind: PURGE_JOBS_JOB,
            when: When::Cron(CronLite::parse("30 3 * * *")?),
        },
    ])
}

fn spawn_relay(app: AppState, set: &mut JoinSet<()>) {
    set.spawn(async move {
        let period = Duration::from_millis(app.cfg.jobs.poll_ms);
        while !app.shutdown.is_cancelled() {
            if let Err(e) = app.relay_events().await {
                tracing::error!(error = %e, "outbox relay failed");
            }
            tokio::select! {
                () = app.shutdown.cancelled() => {}
                () = tokio::time::sleep(period) => {}
            }
        }
    });
}

fn spawn_scheduler(app: AppState, schedules: Vec<Schedule>, set: &mut JoinSet<()>) {
    set.spawn(async move {
        while !app.shutdown.is_cancelled() {
            if let Err(e) = schedule::tick(&app.db, app.deps.ids.as_ref(), &schedules, app.now_ms()).await {
                tracing::error!(error = %e, "scheduler tick failed");
            }
            tokio::select! {
                () = app.shutdown.cancelled() => {}
                () = tokio::time::sleep(SCHEDULER_PERIOD) => {}
            }
        }
    });
}

/// Serve until `app.shutdown` is cancelled, then drain. Returns when everything has stopped or the grace
/// period ran out. (Connections still open at that point die when the process exits.)
pub async fn serve(app: AppState, listener: TcpListener) -> anyhow::Result<()> {
    let schedules = default_schedules().map_err(anyhow::Error::msg)?;
    let mut background = run_pool(&app.job_context(), &app.shutdown);
    spawn_relay(app.clone(), &mut background);
    spawn_scheduler(app.clone(), schedules, &mut background);

    let grace = Duration::from_millis(app.cfg.server.shutdown_grace_ms);
    let server = axum::serve(listener, http::router(app.clone()))
        .with_graceful_shutdown(app.shutdown.clone().cancelled_owned())
        .into_future();
    let drained = async {
        server.await?;
        while background.join_next().await.is_some() {}
        Ok::<(), std::io::Error>(())
    };
    let deadline = async {
        app.shutdown.cancelled().await;
        tokio::time::sleep(grace).await;
    };
    tokio::select! {
        result = drained => result?,
        () = deadline => tracing::warn!("shutdown grace period expired; abandoning remaining work"),
    }
    Ok(())
}

/// Resolves on Ctrl-C or SIGTERM (what orchestrators send).
pub async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_schedules_parse() {
        assert_eq!(default_schedules().unwrap().len(), 2);
    }
}
