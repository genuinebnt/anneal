//! RECIPE 17 (async): bounded channels and backpressure.
//! LEVEL: intermediate
//! Use in: worker pool / queue stages.   Run: cargo run --example bounded_channel
//! COMMON USES: producer/consumer queues where memory must stay bounded: backpressure is the whole point.
//!
//! Pattern: a bounded `mpsc` makes producers wait (`send().await`) or fail fast (`try_send`) when the
//! consumer is slow, so memory stays bounded. An unbounded channel just grows until the process dies.
//! Docs: https://docs.rs/tokio/latest/tokio/sync/mpsc/
#![allow(dead_code)]
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<u32>(2);
    let results: Vec<&str> = (0..4)
        .map(|i| match tx.try_send(i) {
            Ok(()) => "queued",
            Err(TrySendError::Full(_)) => "FULL (rejected)",
            Err(TrySendError::Closed(_)) => "closed",
        })
        .collect();
    println!("try_send x4 into capacity 2: {results:?}");
    assert_eq!(
        results,
        ["queued", "queued", "FULL (rejected)", "FULL (rejected)"]
    );

    let consumer = tokio::spawn(async move {
        let mut got = vec![];
        while let Some(v) = rx.recv().await {
            tokio::time::sleep(Duration::from_millis(20)).await; // slow consumer
            got.push(v);
        }
        got
    });
    let start = Instant::now();
    for i in 10..14 {
        tx.send(i).await.unwrap(); // waits when the queue is full: that is backpressure
    }
    println!(
        "producer was slowed to {:?} by the consumer",
        start.elapsed()
    );
    drop(tx);
    println!("consumer got {:?}", consumer.await.unwrap());
}
