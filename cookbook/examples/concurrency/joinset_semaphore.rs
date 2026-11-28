//! RECIPE 07 (concurrency): bounded concurrency with JoinSet + Semaphore, and CHECKING the bound.
//! LEVEL: intermediate
//! Use in: running many async jobs with at most N at a time.   Run: cargo run --example joinset_semaphore
//! COMMON USES: bounded concurrency in crawlers, batch processors and API clients: Semaphore + JoinSet with a limit that each run checks.
//!
//! Pattern: spawn everything into a `JoinSet`, but each task must hold a semaphore permit to run.
//! The test is an invariant: track in-flight count with atomics and assert max_seen <= limit. A run can show that the bound
//! was broken; passing does not prove it holds in every schedule. All 12 tasks are spawned at once (only 3 run past the
//! permit): the limit bounds the work in progress, not the number of waiting tasks.
//! Real code: tokio's sync primitives -> https://github.com/tokio-rs/tokio/tree/master/tokio/src/sync
#![allow(dead_code)]
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() {
    let limit = 3;
    let permits = Arc::new(Semaphore::new(limit));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let mut jobs = JoinSet::new();

    for id in 0..12 {
        let (permits, in_flight, max_seen) = (permits.clone(), in_flight.clone(), max_seen.clone());
        jobs.spawn(async move {
            let _permit = permits.acquire_owned().await.expect("semaphore closed");
            let now = in_flight.fetch_add(1, SeqCst) + 1;
            max_seen.fetch_max(now, SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            in_flight.fetch_sub(1, SeqCst);
            id
        });
    }
    while let Some(res) = jobs.join_next().await {
        println!("finished job {}", res.expect("job panicked"));
    }
    let seen = max_seen.load(SeqCst);
    println!("max in flight: {seen} (limit {limit})");
    assert!(seen <= limit, "limit violated!");
}
