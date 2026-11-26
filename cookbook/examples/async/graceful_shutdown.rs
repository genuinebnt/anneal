//! RECIPE 22 (async): graceful shutdown: stop accepting work, drain what is queued, then exit.
//! LEVEL: intermediate
//! Use in: worker pool / service stages.   Run: cargo run --example graceful_shutdown
//! COMMON USES: servers and workers: finish in-flight work on SIGTERM instead of dropping jobs (Kubernetes rolling deploys).
//!
//! Pattern: a `watch` channel broadcasts "shutting down". The worker `select!`s between a job and that
//! signal; when it fires, it drains jobs already queued (`try_recv`) before returning. No job is lost.
//! Docs: https://tokio.rs/tokio/topics/shutdown
#![allow(dead_code)]
use std::time::Duration;
use tokio::sync::{mpsc, watch};

async fn process(job: u32) {
    tokio::time::sleep(Duration::from_millis(5)).await;
    println!("processed job {job}");
}

#[tokio::main]
async fn main() {
    let (job_tx, mut job_rx) = mpsc::channel::<u32>(16);
    let (stop_tx, mut stop_rx) = watch::channel(false);

    let worker = tokio::spawn(async move {
        let mut done = 0;
        loop {
            tokio::select! {
                _ = stop_rx.changed() => break,
                job = job_rx.recv() => match job {
                    Some(j) => { process(j).await; done += 1; }
                    None => break,
                },
            }
        }
        while let Ok(j) = job_rx.try_recv() {
            // drain what was already accepted
            process(j).await;
            done += 1;
        }
        done
    });

    for j in 0..5 {
        job_tx.send(j).await.unwrap();
    }
    stop_tx.send(true).unwrap(); // shutdown requested immediately after queuing
    let done = worker.await.unwrap();
    println!("worker exited after processing {done} jobs");
    assert_eq!(done, 5);
}
