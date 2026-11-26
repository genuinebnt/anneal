//! RECIPE 81 (async): cancellation safety: why `select!` can silently lose data, and the fix.
//! LEVEL: advanced
//! Use in: any loop with `select!`/`timeout` around a multi-step async operation (framing, handshakes, batching).   Run: cargo run --release --example async_cancel_safety
//! COMMON USES: every select!/timeout loop: do not lose half-read frames or half-built batches.
#![allow(dead_code)]
//!
//! When a `select!` branch loses, its future is DROPPED mid-way. If that future had already taken data out of a channel/socket
//! but not yet returned it, the data is gone. `mpsc::Receiver::recv` is cancel-safe (nothing is consumed unless it returns).
//! A user-written `async fn` that does recv + recv is NOT: the first value lives only in the future's local variable.
//! Fix: keep progress in state OWNED BY THE CALLER (outside the future) and only await cancel-safe primitives inside `select!`.
//! Tokio documents the cancel-safety of every method: https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

fn producer() -> mpsc::Receiver<u32> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        for i in 1..=4 {
            tx.send(i).await.unwrap(); // 1 at t=0, 2 at t=60, 3 at t=120, 4 at t=180
            sleep(Duration::from_millis(60)).await;
        }
    });
    rx
}

/// reads two values and returns them together: NOT cancel-safe as a unit
async fn read_pair(rx: &mut mpsc::Receiver<u32>) -> Option<(u32, u32)> {
    let a = rx.recv().await?; // if we are cancelled after this line, `a` is lost forever
    let b = rx.recv().await?;
    Some((a, b))
}

#[tokio::main]
async fn main() {
    // BUGGY: timeout (50ms) is shorter than the gap between items (60ms), so every pair attempt is cancelled
    let mut rx = producer();
    let mut pairs = vec![];
    loop {
        tokio::select! {
            r = read_pair(&mut rx) => match r {
                Some(p) => pairs.push(p),
                None => break,
            },
            _ = sleep(Duration::from_millis(50)) => println!("  timeout: dropped the read_pair future (and any value it held)"),
        }
    }
    println!("buggy loop produced {} pairs from 4 items", pairs.len());
    assert!(pairs.is_empty());

    // FIXED: partial progress lives in `pending`, outside any future; only `recv()` (cancel-safe) is raced
    let mut rx = producer();
    let mut pending: Option<u32> = None;
    let mut pairs = vec![];
    let mut idle_ticks = 0;
    loop {
        tokio::select! {
            item = rx.recv() => match item {
                Some(v) => match pending.take() {
                    Some(first) => pairs.push((first, v)),
                    None => pending = Some(v),
                },
                None => break,
            },
            _ = sleep(Duration::from_millis(50)) => idle_ticks += 1, // timeouts no longer lose anything
        }
    }
    println!("fixed loop produced pairs {pairs:?} (saw {idle_ticks} idle timeouts)");
    assert_eq!(pairs, [(1, 2), (3, 4)]);
    println!(
        "rule: inside select!, only await futures documented as cancel-safe, or make progress state external."
    );
}
