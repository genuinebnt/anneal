use solution::*;

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};
use tokio_util::sync::CancellationToken;

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

/// A job is its own duration in milliseconds.
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
fn finishes_when_senders_are_dropped() {
    run(async {
        let (tx, rx) = queued(&[10, 20, 30]);
        drop(tx);
        let got = run_worker(rx, CancellationToken::new(), Duration::from_millis(1), work).await;
        check!("three jobs, no shutdown, grace 1 ms", got, report(&[10, 20, 30], None, &[]));
    });
}

#[test]
fn drains_what_was_accepted() {
    run(async {
        let (_tx, rx) = queued(&[10, 10, 10]);
        let token = CancellationToken::new();
        cancel_after(&token, 5);
        let got = tokio::time::timeout(Duration::from_secs(60), run_worker(rx, token, Duration::from_millis(100), work)).await.expect("never returned");
        check!("cancel at 5 ms, grace 100 ms", got, report(&[10, 10, 10], None, &[]));
    });
}

#[test]
fn gives_up_at_the_deadline() {
    run(async {
        let (_tx, rx) = queued(&[30, 30, 30, 30]);
        let token = CancellationToken::new();
        cancel_after(&token, 10);
        let start = Instant::now();
        let got = run_worker(rx, token, Duration::from_millis(40), work).await;
        check!("30 ms jobs, cancel at 10 ms, grace 40 ms: returns at 50 ms", (got, start.elapsed()), (report(&[30], Some(30), &[30, 30]), Duration::from_millis(50)));
    });
}

#[test]
fn senders_fail_as_soon_as_shutdown_starts() {
    run(async {
        let (tx, rx) = queued(&[1000]);
        let token = CancellationToken::new();
        cancel_after(&token, 10);
        let probe = tokio::spawn(async move {
            sleep(Duration::from_millis(20)).await;
            tx.send(1).await.is_err()
        });
        let got = run_worker(rx, token, Duration::from_millis(100), work).await;
        check!("send at 20 ms, while the first job still runs: refused", probe.await.unwrap(), true);
        check!("report", got, report(&[], Some(1000), &[]));
    });
}

#[test]
fn already_cancelled_still_drains() {
    run(async {
        let (_tx, rx) = queued(&[5, 5]);
        let token = CancellationToken::new();
        token.cancel();
        let got = run_worker(rx, token, Duration::from_millis(50), work).await;
        check!("cancelled before start", got, report(&[5, 5], None, &[]));
    });
}
