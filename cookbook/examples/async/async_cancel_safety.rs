//! RECIPE 81 (async): cancellation safety: why `select!` can silently lose data, and the fix.
//! LEVEL: advanced
//! Use in: any loop with `select!`/`timeout` around an operation that takes several steps (framing, handshakes, batching).   Run: cargo run --release --example async_cancel_safety
//! COMMON USES: every select!/timeout loop: do not lose half-read frames or half-built batches.
#![allow(dead_code)]
//!
//! When a `select!` branch loses, its future is dropped part way. If that future had already taken data out of a channel
//! or a socket, and not yet returned it, the data is gone. `mpsc::Receiver::recv` is cancel-safe: if it loses, it has
//! taken nothing. An `async fn` that calls `recv` twice is not: the first value lives only in the future's local variable.
//! The fix: keep progress in state the caller owns, outside the future, and race only cancel-safe calls in `select!`.
//! The demo runs on Tokio's paused clock (one thread, and time that jumps to the next timer when every task waits; feature
//! `test-util`). So which side wins each race is the same on every run, however loaded the machine is.
//! Tokio lists which of its methods are cancel-safe: https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Sends 1 at t=0, 2 at t=60 ms, 3 at t=120 ms and 4 at t=180 ms, then closes the channel at t=240 ms.
fn producer() -> mpsc::Receiver<u32> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        for i in 1..=4 {
            tx.send(i).await.unwrap();
            sleep(Duration::from_millis(60)).await;
        }
    });
    rx
}

/// Reads two values and returns them together. Not cancel-safe as a whole.
async fn read_pair(rx: &mut mpsc::Receiver<u32>) -> Option<(u32, u32)> {
    let a = rx.recv().await?; // if the future is dropped after this line, `a` is lost
    let b = rx.recv().await?;
    Some((a, b))
}

/// BUGGY: the 50 ms heartbeat is shorter than the 60 ms gap between values, so it wins every race against the second
/// `recv`. Each time, the dropped `read_pair` takes one value with it.
async fn pairs_buggy() -> Vec<(u32, u32)> {
    let mut rx = producer();
    let mut pairs = vec![];
    loop {
        tokio::select! {
            r = read_pair(&mut rx) => match r {
                Some(p) => pairs.push(p),
                None => break,
            },
            _ = sleep(Duration::from_millis(50)) => println!("  heartbeat: dropped read_pair and the value it held"),
        }
    }
    pairs
}

/// FIXED: the first value of a pair waits in `pending`, which outlives every `select!`. Only `recv` is raced.
async fn pairs_fixed() -> (Vec<(u32, u32)>, usize) {
    let mut rx = producer();
    let mut pending: Option<u32> = None;
    let mut pairs = vec![];
    let mut heartbeats = 0;
    loop {
        tokio::select! {
            item = rx.recv() => match item {
                Some(v) => match pending.take() {
                    Some(first) => pairs.push((first, v)),
                    None => pending = Some(v),
                },
                None => break,
            },
            _ = sleep(Duration::from_millis(50)) => heartbeats += 1, // loses nothing now
        }
    }
    (pairs, heartbeats)
}

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn main() {
    let pairs = pairs_buggy().await;
    println!("buggy loop produced {} pairs from 4 values", pairs.len());
    assert!(pairs.is_empty(), "every value was lost with a dropped read_pair");

    let (pairs, heartbeats) = pairs_fixed().await;
    println!("fixed loop produced pairs {pairs:?} ({heartbeats} heartbeats in between)");
    assert_eq!(pairs, [(1, 2), (3, 4)]);
    assert_eq!(heartbeats, 4, "one heartbeat in each 60 ms gap");
    println!("rule: in select!, race only cancel-safe futures, or keep progress outside the future.");
}
