//! RECIPE 53 (async): join!, spawn, JoinSet, spawn_blocking, timeouts, and panic isolation in tokio.
//! LEVEL: intermediate
//! Use in: concurrent executor stages, API handlers that fan out, mixing CPU and IO work.   Run: cargo run --release --example async_patterns
//! COMMON USES: fan-out API calls, mixing CPU and IO work, per-task timeouts and panic isolation in services.
#![allow(dead_code)]
//!
//!  join!(a, b)        run futures CONCURRENTLY on the same task; wait for all. (try_join! stops on first Err)
//!  tokio::spawn(f)    run on the runtime as an independent task; JoinHandle gives the result (or panic as Err)
//!  JoinSet            a dynamic bag of spawned tasks; `join_next()` yields results as they finish
//!  spawn_blocking(f)  run blocking or CPU-heavy code on a separate thread pool so it cannot stall async tasks
//! Never `std::thread::sleep` or heavy loops inside async code: they block the executor thread.
//! Docs: https://tokio.rs/tokio/tutorial/spawning and https://docs.rs/tokio/latest/tokio/task/index.html
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout};

async fn fetch(name: &str, ms: u64) -> String {
    sleep(Duration::from_millis(ms)).await;
    format!("{name} done")
}

fn fib(n: u32) -> u64 {
    if n < 2 {
        n as u64
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

#[tokio::main]
async fn main() {
    // concurrent vs sequential
    let t = Instant::now();
    let (a, b, c) = tokio::join!(fetch("a", 100), fetch("b", 100), fetch("c", 100));
    let concurrent = t.elapsed();
    println!(
        "{a}, {b}, {c}: ~{}ms (sequential would be ~300)",
        concurrent.as_millis() / 10 * 10
    );
    assert!(concurrent < Duration::from_millis(280));

    // try_join!: fail fast
    let r: Result<(u32, u32), String> = tokio::try_join!(async { Ok(1) }, async {
        Err::<u32, _>("second failed".to_string())
    });
    println!("{r:?}");

    // JoinSet: results arrive in COMPLETION order
    let mut set = JoinSet::new();
    for (name, ms) in [("slow", 60), ("fast", 10), ("mid", 30)] {
        set.spawn(async move { fetch(name, ms).await });
    }
    let mut order = vec![];
    while let Some(res) = set.join_next().await {
        order.push(res.unwrap());
    }
    println!("{order:?}");

    // CPU work on the blocking pool, awaited like any future
    let big = tokio::task::spawn_blocking(|| fib(32)).await.unwrap();
    println!("fib(32) = {big}");

    // a panic in one task is contained and reported
    std::panic::set_hook(Box::new(|_| {}));
    let res = tokio::spawn(async { if true { panic!("task exploded") } else { 1 } }).await;
    println!(
        "panicked task isolated: is_panic = {}",
        res.as_ref().err().map(|e| e.is_panic()).unwrap_or(false)
    );

    // timeout wraps any future; the inner future is dropped (cancelled) on expiry
    let slow = timeout(Duration::from_millis(30), fetch("stuck", 5000)).await;
    println!("timeout fired: {}", slow.is_err());

    // abort a running task
    let h = tokio::spawn(fetch("long", 5000));
    h.abort();
    println!(
        "aborted: is_cancelled = {}",
        h.await.unwrap_err().is_cancelled()
    );
}
