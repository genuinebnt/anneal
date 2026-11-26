use solution::*;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tower::limit::ConcurrencyLimit;
use tower::{Service, ServiceExt, service_fn};

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

/// `(in_flight, ok, err, cancelled)`
#[allow(dead_code)]
fn counts(s: &Stats) -> (usize, usize, usize, usize) {
    (s.in_flight.load(Ordering::SeqCst), s.ok.load(Ordering::SeqCst), s.err.load(Ordering::SeqCst), s.cancelled.load(Ordering::SeqCst))
}

/// A request is `(milliseconds, succeeds)`.
#[allow(dead_code)]
async fn backend((ms, ok): (u64, bool)) -> Result<u64, String> {
    tokio::time::sleep(Duration::from_millis(ms)).await;
    if ok { Ok(ms) } else { Err(format!("failed after {ms}")) }
}

#[test]
fn passes_results_through() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(service_fn(backend), stats.clone());
        let a = s.ready().await.unwrap().call((1, true)).await;
        let b = s.ready().await.unwrap().call((2, false)).await;
        check!("one success, one failure", (a, b), (Ok(1), Err("failed after 2".to_string())));
        check!("(in_flight, ok, err, cancelled)", counts(&stats), (0, 1, 1, 0));
    });
}

#[test]
fn works_over_a_concurrency_limit() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(ConcurrencyLimit::new(service_fn(backend), 1), stats.clone());
        let got = s.ready().await.unwrap().call((1, true)).await;
        check!("a limit of 1: first call", got, Ok(1));
        let got = tokio::time::timeout(Duration::from_secs(5), async { s.ready().await.unwrap().call((2, true)).await }).await;
        check!("second call", got.ok(), Some(Ok(2)));
    });
}

#[test]
fn the_limit_still_limits() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(ConcurrencyLimit::new(service_fn(backend), 2), stats.clone());
        let first = s.ready().await.unwrap().call((100, true));
        let second = s.ready().await.unwrap().call((100, true));
        let third = tokio::time::timeout(Duration::from_millis(10), s.ready()).await.is_ok();
        check!("third call is ready while two are in flight", third, false);
        let _ = tokio::join!(first, second);
    });
}

#[test]
fn dropped_future_is_cancelled() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(service_fn(backend), stats.clone());
        let call = s.ready().await.unwrap().call((1000, true));
        let _ = tokio::time::timeout(Duration::from_millis(5), call).await;
        check!("timed out and dropped", counts(&stats), (0, 0, 0, 1));
    });
}

#[test]
fn in_flight_counts_from_call() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(service_fn(backend), stats.clone());
        let a = s.ready().await.unwrap().call((1, true));
        let b = s.ready().await.unwrap().call((1, true));
        check!("two futures not yet polled", counts(&stats), (2, 0, 0, 0));
        drop((a, b));
        check!("dropped unpolled", counts(&stats), (0, 0, 0, 2));
    });
}
