//! RECIPE 18 (concurrency): shared state: `Arc<Mutex<_>>` vs an actor that owns the state.
//! LEVEL: intermediate
//! Use in: schedulers, caches, execution state.   Run: cargo run --example shared_state
//! COMMON USES: caches, registries and schedulers: mutex for tiny sections, actor task when invariants span several fields.
//!
//! Pattern A: `Arc<std::sync::Mutex<T>>`: fine for tiny critical sections. NEVER hold the guard across an
//! `.await` (it is not Send, and it blocks the runtime thread).
//! Pattern B: one task owns the data; others send it commands over a channel and get replies on a `oneshot`.
//! No locks, one place to reason about invariants.
#![allow(dead_code)]
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

enum Cmd {
    Inc,
    Get(oneshot::Sender<u32>),
}

#[tokio::main]
async fn main() {
    // A: mutex
    let shared = Arc::new(Mutex::new(0u32));
    let mut handles = vec![];
    for _ in 0..10 {
        let shared = shared.clone();
        handles.push(tokio::spawn(async move {
            *shared.lock().unwrap() += 1; // guard dropped at end of statement, before any await
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    println!("mutex total: {}", shared.lock().unwrap());

    // B: actor
    let (tx, mut rx) = mpsc::channel::<Cmd>(16);
    tokio::spawn(async move {
        let mut count = 0u32; // owned by this task only
        while let Some(cmd) = rx.recv().await {
            match cmd {
                Cmd::Inc => count += 1,
                Cmd::Get(reply) => {
                    let _ = reply.send(count);
                }
            }
        }
    });
    for _ in 0..10 {
        tx.send(Cmd::Inc).await.unwrap();
    }
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Cmd::Get(reply_tx)).await.unwrap();
    println!("actor total: {}", reply_rx.await.unwrap());
}
