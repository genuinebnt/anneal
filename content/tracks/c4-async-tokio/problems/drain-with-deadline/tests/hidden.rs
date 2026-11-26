use solution::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};
use tokio_util::sync::CancellationToken;

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
async fn work(ms: u64) {
    sleep(Duration::from_millis(ms)).await;
}

#[allow(dead_code)]
fn queued(jobs: &[u64]) -> (mpsc::Sender<u64>, mpsc::Receiver<u64>) {
    let (tx, rx) = mpsc::channel(64);
    for &j in jobs {
        tx.try_send(j).unwrap();
    }
    (tx, rx)
}

#[allow(dead_code)]
fn report(completed: &[u64], interrupted: Option<u64>, unstarted: &[u64]) -> Report<u64> {
    Report { completed: completed.to_vec(), interrupted, unstarted: unstarted.to_vec() }
}

#[allow(dead_code)]
fn cancel_after(token: &CancellationToken, ms: u64) {
    let token = token.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(ms)).await;
        token.cancel();
    });
}

#[test]
fn idle_worker_stops_at_once() {
    run(async {
        let (_tx, rx) = queued(&[]);
        let token = CancellationToken::new();
        cancel_after(&token, 15);
        let start = Instant::now();
        let got = tokio::time::timeout(Duration::from_secs(60), run_worker(rx, token, Duration::from_secs(10), work)).await.expect("never returned");
        check!("nothing queued, cancel at 15 ms, grace 10 s", (got, start.elapsed()), (report(&[], None, &[]), Duration::from_millis(15)));
    });
}

#[test]
fn drain_finishing_early_does_not_wait_for_the_deadline() {
    run(async {
        let (_tx, rx) = queued(&[5, 5]);
        let token = CancellationToken::new();
        token.cancel();
        let start = Instant::now();
        let _ = run_worker(rx, token, Duration::from_secs(30), work).await;
        check!("two 5 ms jobs, grace 30 s: elapsed", start.elapsed(), Duration::from_millis(10));
    });
}

#[test]
fn deadline_counts_from_cancellation_not_from_the_job_end() {
    run(async {
        let (_tx, rx) = queued(&[100, 15]);
        let token = CancellationToken::new();
        cancel_after(&token, 10);
        let start = Instant::now();
        let got = run_worker(rx, token, Duration::from_millis(100), work).await;
        check!("cancel at 10 ms inside a 100 ms job, grace 100 ms", (got, start.elapsed()), (report(&[100], Some(15), &[]), Duration::from_millis(110)));
    });
}

#[test]
fn one_deadline_for_the_whole_drain() {
    run(async {
        let (_tx, rx) = queued(&[20; 10]);
        let token = CancellationToken::new();
        token.cancel();
        let start = Instant::now();
        let got = run_worker(rx, token, Duration::from_millis(50), work).await;
        check!("ten 20 ms jobs, grace 50 ms", (got, start.elapsed()), (report(&[20, 20], Some(20), &[20; 7]), Duration::from_millis(50)));
    });
}

#[test]
fn interrupted_future_is_dropped() {
    run(async {
        struct Flag(Arc<AtomicUsize>);
        impl Drop for Flag {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let ran_on = Arc::new(AtomicUsize::new(0));
        let (_tx, rx) = queued(&[1]);
        let token = CancellationToken::new();
        token.cancel();
        let (d, r) = (drops.clone(), ran_on.clone());
        let _ = run_worker(rx, token, Duration::from_millis(10), move |_| {
            let (flag, r) = (Flag(d.clone()), r.clone());
            async move {
                let _flag = flag;
                sleep(Duration::from_millis(100)).await;
                r.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;
        sleep(Duration::from_secs(1)).await;
        check!("(futures dropped, handlers that ran on afterwards)", (drops.load(Ordering::SeqCst), ran_on.load(Ordering::SeqCst)), (1, 0));
    });
}

#[test]
fn handler_is_never_called_for_unstarted_jobs() {
    run(async {
        let (tx, rx) = mpsc::channel(8);
        for j in [10u64, 11, 12] {
            tx.try_send(j).unwrap();
        }
        let token = CancellationToken::new();
        token.cancel();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let _ = run_worker(rx, token, Duration::from_millis(5), move |j: u64| {
            s.lock().unwrap().push(j);
            work(j)
        })
        .await;
        let got = seen.lock().unwrap().clone();
        check!("grace 5 ms: jobs passed to handle", got, vec![10]);
    });
}

#[test]
fn zero_grace() {
    run(async {
        let (_tx, rx) = queued(&[5, 6]);
        let token = CancellationToken::new();
        token.cancel();
        let got = run_worker(rx, token, Duration::ZERO, work).await;
        check!("grace 0: nothing may complete", (got.completed.len(), got.interrupted.into_iter().chain(got.unstarted).collect::<Vec<_>>()), (0, vec![5, 6]));
    });
}

#[test]
fn jobs_arriving_during_work_are_processed() {
    run(async {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            for j in 1..=5u64 {
                tx.send(j).await.unwrap();
            }
        });
        let got = tokio::time::timeout(Duration::from_secs(60), run_worker(rx, CancellationToken::new(), Duration::ZERO, work)).await.expect("never returned");
        check!("capacity 1, five jobs from a producer", got, report(&[1, 2, 3, 4, 5], None, &[]));
    });
}

#[test]
fn every_accepted_job_is_accounted_for() {
    let mut rng = anneal_prelude::Rng::new(8105);
    for _ in 0..250 {
        let cap = 1 + rng.below(4);
        let n = rng.below(10);
        let cancel = rng.int(0, 60) as u64 * 2 + 1;
        let grace = rng.int(0, 30) as u64 * 2;
        let schedule: Vec<(u64, u64)> = (0..n).map(|_| (rng.int(0, 10) as u64 * 2, rng.int(1, 10) as u64 * 2)).collect();
        let shown = format!("capacity {cap}, cancel at {cancel} ms, grace {grace} ms, (gap, duration) {schedule:?}");
        let (accepted, got, took) = run(async {
            let (tx, rx) = mpsc::channel::<(usize, u64)>(cap);
            let accepted = Arc::new(Mutex::new(Vec::new()));
            let a = accepted.clone();
            tokio::spawn(async move {
                for (id, (gap, dur)) in schedule.into_iter().enumerate() {
                    sleep(Duration::from_millis(gap)).await;
                    if tx.send((id, dur)).await.is_err() {
                        break;
                    }
                    a.lock().unwrap().push(id);
                }
                // Keep the sender alive so that only the token can stop the worker.
                sleep(Duration::from_secs(3600)).await;
                drop(tx);
            });
            let token = CancellationToken::new();
            cancel_after(&token, cancel);
            let start = Instant::now();
            let r = tokio::time::timeout(Duration::from_secs(600), run_worker(rx, token, Duration::from_millis(grace), |(_, d): (usize, u64)| work(d))).await.expect("never returned");
            let took = start.elapsed().as_millis() as u64;
            sleep(Duration::from_millis(5)).await;
            let accepted = accepted.lock().unwrap().clone();
            (accepted, r, took)
        });
        let all: Vec<usize> = got.completed.iter().chain(got.interrupted.iter()).chain(got.unstarted.iter()).map(|j| j.0).collect();
        check!(format!("{shown}: accepted jobs in the report"), all, accepted);
        check!(format!("{shown}: returned within the deadline"), took <= cancel + grace, true);
        if got.interrupted.is_some() {
            check!(format!("{shown}: an interrupted job means the full grace was used"), took, cancel + grace);
        }
    }
}
