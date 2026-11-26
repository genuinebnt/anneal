//! RECIPE 79 (async): bounded concurrency with Streams: `buffer_unordered`, `buffered`, `FuturesUnordered`, `for_each_concurrent`.
//! LEVEL: intermediate
//! Use in: "run these N async jobs, at most K at a time" without hand-rolling Semaphore + JoinSet (compare joinset_semaphore).   Run: cargo run --release --example async_streams_concurrency
//! COMMON USES: crawlers, batch API calls, ETL: at most K concurrent futures with streaming results.
#![allow(dead_code)]
//!
//! A `Stream` is an async iterator. Turn a list of inputs into a stream, `map` each to a future, then:
//!   .buffer_unordered(k)  run up to k futures at once; yield results as they COMPLETE (fastest first)
//!   .buffered(k)          same concurrency but yield results in INPUT order
//!   FuturesUnordered      a dynamic set you can keep pushing futures into
//! These run on the SAME task (concurrent, not parallel). To use many threads, `tokio::spawn` each future first.
//! Docs: https://docs.rs/futures-util/latest/futures_util/stream/trait.StreamExt.html and https://tokio.rs/tokio/tutorial/streams
use futures_util::StreamExt;
use futures_util::stream::{self, FuturesUnordered};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone, Default)]
struct Gauge {
    now: Arc<AtomicUsize>,
    max: Arc<AtomicUsize>,
}
impl Gauge {
    async fn job(&self, id: u32, ms: u64) -> u32 {
        let n = self.now.fetch_add(1, SeqCst) + 1;
        self.max.fetch_max(n, SeqCst);
        sleep(Duration::from_millis(ms)).await;
        self.now.fetch_sub(1, SeqCst);
        id * 10
    }
}

#[tokio::main]
async fn main() {
    // durations chosen so completion order differs from input order
    let jobs: Vec<(u32, u64)> = vec![(1, 60), (2, 10), (3, 40), (4, 10), (5, 30), (6, 10)];

    let g = Gauge::default();
    let unordered: Vec<u32> = stream::iter(jobs.clone())
        .map(|(id, ms)| {
            let g = g.clone();
            async move { g.job(id, ms).await }
        })
        .buffer_unordered(3)
        .collect()
        .await;
    println!(
        "buffer_unordered(3): {unordered:?}  max in flight = {}",
        g.max.load(SeqCst)
    );
    assert!(g.max.load(SeqCst) <= 3);

    let g = Gauge::default();
    let ordered: Vec<u32> = stream::iter(jobs.clone())
        .map(|(id, ms)| {
            let g = g.clone();
            async move { g.job(id, ms).await }
        })
        .buffered(3)
        .collect()
        .await;
    println!("buffered(3):         {ordered:?}  (input order kept)");
    assert_eq!(ordered, [10, 20, 30, 40, 50, 60]);

    // FuturesUnordered: add work dynamically while consuming results
    let g = Gauge::default();
    let mut set = FuturesUnordered::new();
    for (id, ms) in jobs.iter().take(3) {
        set.push(g.job(*id, *ms));
    }
    let mut seen = vec![];
    while let Some(v) = set.next().await {
        seen.push(v);
        if seen.len() == 1 {
            set.push(g.job(99, 5)); // a follow-up job discovered while running
        }
    }
    println!(
        "FuturesUnordered with a dynamically added job: {} results, 990 included: {}",
        seen.len(),
        seen.contains(&990)
    );
    assert_eq!(seen.len(), 4);

    // for_each_concurrent: side effects, no collected results
    let total = Arc::new(AtomicUsize::new(0));
    let t2 = total.clone();
    stream::iter(1..=10usize)
        .for_each_concurrent(4, move |n| {
            let t = t2.clone();
            async move {
                sleep(Duration::from_millis(5)).await;
                t.fetch_add(n, SeqCst);
            }
        })
        .await;
    println!("for_each_concurrent total = {}", total.load(SeqCst));
    assert_eq!(total.load(SeqCst), 55);
}
