//! RECIPE 94 (advanced): a generic async worker pool: bounded queue, bounded concurrency, panic isolation, graceful drain.
//! LEVEL: advanced
//! Use in: any "submit jobs, run at most N at once, collect results" component. Combines joinset_semaphore, bounded_channel, graceful_shutdown, async_patterns, closures_hof.   Run: cargo run --release --example worker_pool
//! COMMON USES: image/video processing farms, scraping pools, job runners: bounded queue + bounded workers + panic isolation.
#![allow(dead_code)]
//!
//! Design (domain: image resizing, NOT the DAG executor):
//!   submit() --bounded mpsc--> dispatcher task --Semaphore(N permits)--> spawned job --> results channel
//!  * BACKPRESSURE (jobs only): `submit().await` waits when the job queue is full. While nothing finishes, exactly
//!    `workers + queue + 1` submits return: `workers` jobs run, the dispatcher holds one more while it waits for a
//!    permit, and `queue` jobs wait in the channel
//!  * NOT BOUNDED: results go to an unbounded channel that only `finish()` reads, so they grow with the number of jobs,
//!    and every caller waiting in `submit()` holds its own job. Use this pool for a finite batch
//!  * CONCURRENCY LIMIT: the dispatcher takes a permit BEFORE spawning, so at most N jobs run
//!  * BOOKKEEPING: finished entries leave the `JoinSet` before the next spawn, so it never holds more than N
//!  * PANIC ISOLATION: `f` is called, and its future runs, inside the job's own `tokio::spawn`; a panic in either
//!    becomes `Outcome::Panicked` and the pool lives on (this needs unwinding panics, not `panic = "abort"`)
//!  * GRACEFUL SHUTDOWN: `finish()` closes the queue, waits for queued and running jobs, then returns every outcome.
//!    It has no deadline: a job that never ends makes it wait forever
//!
//! The generics: `F: Fn(J) -> Fut`, `Fut: Future<Output = R> + Send + 'static`. Study how a real runtime is built:
//! https://github.com/tokio-rs/tokio/tree/master/tokio/src/runtime
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::{Duration, Instant};
use tokio::sync::{Barrier, Semaphore, mpsc};
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
                // With a permit in hand, at most `workers - 1` jobs still hold one. So when the set has `workers`
                // entries, one task is finished or finishing, and `join_next` never waits for a long job.
                while running.len() >= workers {
                    running.join_next().await;
                }
                let (f, res_tx) = (f.clone(), res_tx.clone());
                running.spawn(async move {
                    let _permit = permit; // released when the job (or its panic) is finished
                    // `f(job)` is called inside the new task, so a panic before it returns a future is caught too
                    let outcome = match tokio::spawn(async move { f(job).await }).await {
                        Ok(r) => Outcome::Done(r),
                        Err(e) => Outcome::Panicked(e.to_string()),
                    };
                    let _ = res_tx.send(outcome);
                });
                assert!(
                    running.len() <= workers,
                    "finished tasks are piling up in the JoinSet"
                );
            }
            while running.join_next().await.is_some() {} // drain: wait for everything in flight
        });
        Pool {
            jobs,
            results,
            dispatcher,
        }
    }

    /// waits if the queue is full; Err gives the job back if the dispatcher is gone
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

fn panics<R>(outcomes: &[Outcome<R>]) -> Vec<&str> {
    outcomes
        .iter()
        .filter_map(|o| match o {
            Outcome::Panicked(m) => Some(m.as_str()),
            Outcome::Done(_) => None,
        })
        .collect()
}

/// 12 jobs on 3 workers. Each job waits at a barrier for two others, so the batch can only finish if 3 jobs really
/// run at the same time: the check does not depend on how fast the machine is.
async fn batch() {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_seen = Arc::new(AtomicUsize::new(0));
    let together = Arc::new(Barrier::new(3));
    let (inf, max) = (in_flight.clone(), max_seen.clone());

    let pool = Pool::start(3, 2, move |id: u32| {
        let (inf, max, together) = (inf.clone(), max.clone(), together.clone());
        async move {
            let now = inf.fetch_add(1, SeqCst) + 1;
            max.fetch_max(now, SeqCst);
            together.wait().await;
            tokio::time::sleep(Duration::from_millis(25)).await; // stands in for the work
            inf.fetch_sub(1, SeqCst);
            if id == 7 {
                panic!("corrupt image {id}"); // expected: this message is printed
            }
            format!("resized-{id}")
        }
    });

    let start = Instant::now();
    for id in 0..12 {
        pool.submit(id).await.expect("pool open"); // waits once the queue and the workers are full
    }
    println!(
        "submitted 12 jobs in {:?} (backpressure slowed the producer)",
        start.elapsed()
    );
    let outcomes = pool.finish().await;

    let panicked = panics(&outcomes);
    let done = outcomes.len() - panicked.len();
    println!(
        "{} outcomes: {done} done, {} panicked {panicked:?}",
        outcomes.len(),
        panicked.len()
    );
    println!(
        "max concurrent jobs = {} (limit 3), total {:?}",
        max_seen.load(SeqCst),
        start.elapsed()
    );
    assert_eq!(outcomes.len(), 12);
    assert_eq!((done, panicked.len()), (11, 1));
    assert_eq!(max_seen.load(SeqCst), 3);
}

/// A panic raised by `f` itself, before it returns a future, must still produce an outcome.
async fn panic_before_the_future() {
    let pool = Pool::start(2, 2, |id: u32| {
        assert!(id != 1, "bad job {id}"); // expected: this message is printed
        async move { id }
    });
    for id in 0..4 {
        pool.submit(id).await.expect("pool open");
    }
    let outcomes = pool.finish().await;
    assert_eq!(outcomes.len(), 4, "every accepted job needs an outcome");
    assert_eq!(panics(&outcomes).len(), 1);
    println!("panic before the future: 4 jobs, 4 outcomes");
}

/// While no job can finish, `workers + queue + 1` submits return and the next one cannot.
async fn accepted_while_stuck() {
    let (workers, queue) = (3, 2);
    let gate = Arc::new(Semaphore::new(0));
    let job_gate = gate.clone();
    let pool = Pool::start(workers, queue, move |id: usize| {
        let gate = job_gate.clone();
        async move {
            gate.acquire().await.expect("gate open").forget();
            id
        }
    });
    let accepted = workers + queue + 1;
    for id in 0..accepted {
        pool.submit(id).await.expect("pool open");
    }
    // nothing can make room while the gate is shut, so this can never return, however long we wait
    let extra = tokio::time::timeout(Duration::from_millis(100), pool.submit(accepted)).await;
    assert!(extra.is_err(), "one submit too many returned");
    gate.add_permits(accepted);
    assert_eq!(pool.finish().await.len(), accepted);
    println!("while stuck: {accepted} submits returned (workers + queue + 1)");
}

#[tokio::main]
async fn main() {
    // the limit only turns a deadlock into a failure; it is not a speed check
    tokio::time::timeout(Duration::from_secs(60), async {
        batch().await;
        panic_before_the_future().await;
        accepted_while_stuck().await;
    })
    .await
    .expect("the pool stopped making progress");
}
