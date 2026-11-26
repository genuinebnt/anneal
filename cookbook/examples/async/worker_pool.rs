//! RECIPE 94 (advanced): a generic async worker pool: bounded queue, bounded concurrency, panic isolation, graceful drain.
//! LEVEL: advanced
//! Use in: any "submit jobs, run at most N at once, collect results" component. Combines joinset_semaphore, bounded_channel, graceful_shutdown, async_patterns, closures_hof.   Run: cargo run --release --example worker_pool
//! COMMON USES: image/video processing farms, scraping pools, job runners: bounded queue + bounded workers + panic isolation.
#![allow(dead_code)]
//!
//! Design (domain: image resizing, NOT the DAG executor):
//!   submit() --bounded mpsc--> dispatcher task --Semaphore(N permits)--> spawned job --> results channel
//!  * BACKPRESSURE: `submit().await` waits when the queue is full (producers cannot outrun workers: bounded memory)
//!  * CONCURRENCY LIMIT: the dispatcher takes a permit BEFORE spawning, so at most N jobs run
//!  * PANIC ISOLATION: each job runs in its own `tokio::spawn`; a panic becomes `Outcome::Panicked`, the pool lives on
//!  * GRACEFUL SHUTDOWN: `finish()` closes the queue, waits for queued and running jobs, then returns every outcome
//! The generics: `F: Fn(J) -> Fut`, `Fut: Future<Output = R> + Send + 'static`. Study how real pools are built:
//! https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime and https://docs.rs/deadpool
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinHandle;

#[derive(Debug)]
enum Outcome<R> {
    Done(R),
    Panicked(String),
}

struct Pool<J, R> {
    jobs: mpsc::Sender<J>,
    results: mpsc::UnboundedReceiver<Outcome<R>>,
    dispatcher: JoinHandle<()>,
}

impl<J: Send + 'static, R: Send + 'static> Pool<J, R> {
    fn start<F, Fut>(workers: usize, queue: usize, f: F) -> Self
    where
        F: Fn(J) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
    {
        let (jobs, mut job_rx) = mpsc::channel::<J>(queue);
        let (res_tx, results) = mpsc::unbounded_channel();
        let permits = Arc::new(Semaphore::new(workers));
        let f = Arc::new(f);
        let dispatcher = tokio::spawn(async move {
            let mut running = tokio::task::JoinSet::new();
            while let Some(job) = job_rx.recv().await {
                let permit = permits
                    .clone()
                    .acquire_owned()
                    .await
                    .expect("semaphore never closed");
                let (f, res_tx) = (f.clone(), res_tx.clone());
                running.spawn(async move {
                    let _permit = permit; // released when the job (or its panic) is finished
                    let outcome = match tokio::spawn(f(job)).await {
                        Ok(r) => Outcome::Done(r),
                        Err(e) => Outcome::Panicked(e.to_string()),
                    };
                    let _ = res_tx.send(outcome);
                });
            }
            while running.join_next().await.is_some() {} // drain: wait for everything in flight
        });
        Pool {
            jobs,
            results,
            dispatcher,
        }
    }

    /// waits if the queue is full; Err gives the job back if the pool is shut down
    async fn submit(&self, job: J) -> Result<(), J> {
        self.jobs.send(job).await.map_err(|e| e.0)
    }

    async fn finish(self) -> Vec<Outcome<R>> {
        let Pool {
            jobs,
            mut results,
            dispatcher,
        } = self;
        drop(jobs); // closing the queue tells the dispatcher to stop after draining
        dispatcher.await.expect("dispatcher panicked");
        let mut out = vec![];
        while let Some(o) = results.recv().await {
            out.push(o);
        }
        out
    }
}

#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|_| {})); // keep the demo output clean
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let (inf, max) = (in_flight.clone(), max_seen.clone());

    let pool = Pool::start(3, 2, move |id: u32| {
        let (inf, max) = (inf.clone(), max.clone());
        async move {
            let now = inf.fetch_add(1, SeqCst) + 1;
            max.fetch_max(now, SeqCst);
            tokio::time::sleep(Duration::from_millis(25)).await;
            inf.fetch_sub(1, SeqCst);
            if id == 7 {
                panic!("corrupt image {id}");
            }
            format!("resized-{id}")
        }
    });

    let start = Instant::now();
    for id in 0..12 {
        pool.submit(id).await.expect("pool open"); // blocks when queue (2) and workers (3) are saturated
    }
    println!(
        "submitted 12 jobs in {:?} (backpressure slowed the producer)",
        start.elapsed()
    );
    let outcomes = pool.finish().await;
    let elapsed = start.elapsed();

    let done = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Done(_)))
        .count();
    let panicked: Vec<_> = outcomes
        .iter()
        .filter_map(|o| {
            if let Outcome::Panicked(m) = o {
                Some(m.as_str())
            } else {
                None
            }
        })
        .collect();
    println!(
        "{} outcomes: {done} done, {} panicked {panicked:?}",
        outcomes.len(),
        panicked.len()
    );
    println!(
        "max concurrent jobs = {} (limit 3), total {:?} (serial would be ~300ms)",
        max_seen.load(SeqCst),
        elapsed
    );
    assert_eq!(outcomes.len(), 12);
    assert_eq!((done, panicked.len()), (11, 1));
    assert!(max_seen.load(SeqCst) <= 3);
    assert!(elapsed < Duration::from_millis(280));
}
