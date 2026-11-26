//! RECIPE 55 (observability): structured logging with `tracing`: events, spans, fields, levels.
//! LEVEL: intermediate
//! Use in: services and workers where you must answer "what happened to job X?" (and later OpenTelemetry).   Run: cargo run --release --example tracing_logs
//! COMMON USES: production services: structured logs with request/job ids, spans across await points, later OpenTelemetry.
#![allow(dead_code)]
//!
//!  event  = something happened (`info!`, `warn!`, `error!`)     span = a period of time with context
//!  Fields are structured key=value pairs, not interpolated strings: machines can filter/aggregate them.
//!  A span's fields are attached to every event inside it, so you log `job_id` ONCE and see it everywhere,
//!  including across `.await` points when you use `#[instrument]` or `.instrument(span)`.
//!  A *subscriber* decides where output goes (stdout, JSON, OpenTelemetry). Libraries emit; the app installs one.
//! Docs: https://docs.rs/tracing and https://tokio.rs/tokio/topics/tracing
use tracing::{Level, debug, error, info, info_span, instrument, warn};

#[instrument(skip(payload), fields(bytes = payload.len()))]
fn process(job_id: u32, payload: &[u8]) -> Result<u32, String> {
    debug!("starting");
    if payload.is_empty() {
        error!(reason = "empty payload", "job failed");
        return Err("empty".into());
    }
    info!(attempt = 1, "processed");
    Ok(payload.len() as u32)
}

#[instrument]
async fn async_step(name: &str) -> usize {
    info!("inside async fn");
    tokio::task::yield_now().await; // the span stays attached across await points
    name.len()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .without_time() // deterministic output for the demo; keep timestamps in production
        .with_target(false)
        .with_ansi(false)
        .with_writer(std::io::stdout)
        .init();

    info!(version = "0.1", "service starting");
    let _ = process(1, b"hello");
    let _ = process(2, b"");

    let span = info_span!("batch", batch_id = 99);
    let _enter = span.enter(); // everything below carries batch_id=99
    warn!(queue_depth = 17, "queue is getting long");
    drop(_enter);

    let n = async_step("transcode").await;
    info!(n, "done"); // `n` shorthand = n = n
}
