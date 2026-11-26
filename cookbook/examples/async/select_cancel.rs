//! RECIPE 13 (async): cancellation with `select!`, and timeouts.
//! LEVEL: intermediate
//! Use in: cancellation and timeout stages.   Run: cargo run --example select_cancel
//! COMMON USES: request timeouts, shutdown signals, racing a cache against a database: select!/timeout drop the loser.
//!
//! Pattern: race the real work against a cancel signal. When one branch wins, the other future is DROPPED,
//! which cancels it at its next `.await`. A future must therefore be "cancel-safe": dropping it midway
//! must not lose data. `timeout` is the same idea with a clock.
//! Tokio docs: https://tokio.rs/tokio/tutorial/select
#![allow(dead_code)]
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

#[tokio::main]
async fn main() {
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    let worker = tokio::spawn(async move {
        let mut steps = 0u32;
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    println!("worker: cancelled after {steps} steps, cleaning up");
                    return steps;
                }
                _ = sleep(Duration::from_millis(10)) => steps += 1,
            }
        }
    });
    sleep(Duration::from_millis(55)).await;
    cancel_tx.send(()).unwrap();
    let steps = worker.await.unwrap();
    assert!(steps >= 1);

    let slow = sleep(Duration::from_secs(5));
    match timeout(Duration::from_millis(20), slow).await {
        Ok(()) => println!("finished"),
        Err(_) => println!("timed out after 20ms (the slow future was dropped)"),
    }
}
