//! RECIPE 13 (async): cancellation with `select!`, and timeouts.
//! LEVEL: intermediate
//! Use in: cancellation and timeout stages.   Run: cargo run --example select_cancel
//! COMMON USES: request timeouts, shutdown signals, racing a cache against a database: select!/timeout drop the loser.
//!
//! Pattern: race the real work against a cancel signal. When one branch wins, `select!` drops the other branches' futures: a
//! dropped future is never polled again, so its work stops at the `.await` where it was waiting. What it had done up to there
//! stays done. A branch built from a reference (`&mut cancel_rx`) drops only the reference, so the same future can be waited
//! on again in the next turn of a loop. A future is "cancel-safe" if dropping it before it finishes loses nothing:
//! `mpsc::Receiver::recv` is; `read_exact` and `write_all` are not. `timeout` is the same idea with a clock.
//! LIMITS: cancellation only happens when a future yields; a future that blocks or loops without awaiting cannot be timed
//! out. Dropping a `JoinHandle` does not cancel a spawned task (`abort` does).
//! Tokio docs: https://tokio.rs/tokio/tutorial/select
#![allow(dead_code)]
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, timeout};

/// Says when it is dropped: proof that a future holding it was dropped.
struct OnDrop(Option<oneshot::Sender<()>>);
impl Drop for OnDrop {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::main]
async fn main() {
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    let (step_tx, mut step_rx) = mpsc::unbounded_channel();
    let worker = tokio::spawn(async move {
        let mut steps = 0u32;
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    println!("worker: cancelled after {steps} steps, cleaning up");
                    return steps;
                }
                _ = sleep(Duration::from_millis(10)) => {
                    steps += 1;
                    let _ = step_tx.send(steps);
                }
            }
        }
    });
    // cancel after the third step has been reported, however slow the machine is
    while step_rx.recv().await.expect("the worker is running") < 3 {}
    cancel_tx.send(()).unwrap();
    let steps = worker.await.unwrap();
    assert!(steps >= 3);

    let (dropped_tx, mut dropped_rx) = oneshot::channel();
    let guard = OnDrop(Some(dropped_tx));
    let never_ends = async move {
        let _guard = guard;
        std::future::pending::<()>().await
    };
    let result = timeout(Duration::from_millis(20), never_ends).await;
    assert!(result.is_err(), "it never ends, so the clock wins");
    assert_eq!(dropped_rx.try_recv(), Ok(()), "the loser was dropped");
    println!("timed out after 20ms (the unfinished future was dropped)");
}
