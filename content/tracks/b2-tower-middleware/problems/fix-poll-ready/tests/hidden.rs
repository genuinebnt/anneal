use solution::*;

use std::future::Ready;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tower::limit::ConcurrencyLimit;
use tower::{Service, ServiceExt, service_fn};

#[allow(dead_code)]
fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
fn counts(s: &Stats) -> (usize, usize, usize, usize) {
    (s.in_flight.load(Ordering::SeqCst), s.ok.load(Ordering::SeqCst), s.err.load(Ordering::SeqCst), s.cancelled.load(Ordering::SeqCst))
}

#[allow(dead_code)]
async fn backend((ms, ok): (u64, bool)) -> Result<u64, String> {
    tokio::time::sleep(Duration::from_millis(ms)).await;
    if ok { Ok(ms) } else { Err(format!("failed after {ms}")) }
}

/// Insists that `call` happens only on a value that was polled ready. A clone is never ready.
struct Strict {
    ready: bool,
    broken: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}

impl Clone for Strict {
    fn clone(&self) -> Self {
        Strict { ready: false, broken: self.broken.clone(), calls: self.calls.clone() }
    }
}

impl Service<u32> for Strict {
    type Response = u32;
    type Error = &'static str;
    type Future = Ready<Result<u32, &'static str>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), &'static str>> {
        if self.broken.load(Ordering::SeqCst) {
            return Poll::Ready(Err("backend gone"));
        }
        self.ready = true;
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, x: u32) -> Self::Future {
        assert!(std::mem::take(&mut self.ready), "call on a service that was not polled ready");
        self.calls.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Ok(x * 2))
    }
}

#[allow(dead_code)]
fn strict() -> (Strict, Arc<AtomicBool>, Arc<AtomicUsize>) {
    let (broken, calls) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
    (Strict { ready: false, broken: broken.clone(), calls: calls.clone() }, broken, calls)
}

#[test]
fn respects_the_ready_rule() {
    run(async {
        let (inner, _, calls) = strict();
        let mut s = Observe::new(inner, Arc::new(Stats::default()));
        let mut got = Vec::new();
        for i in 0..5 {
            got.push(s.ready().await.unwrap().call(i).await);
        }
        check!("five calls through a strict service", (got, calls.load(Ordering::SeqCst)), ((0..5).map(|i| Ok(i * 2)).collect::<Vec<_>>(), 5));
    });
}

#[test]
fn cloned_middleware_follows_the_rule_too() {
    run(async {
        let (inner, _, _) = strict();
        let mut a = Observe::new(inner, Arc::new(Stats::default()));
        a.ready().await.unwrap();
        let mut b = a.clone();
        let x = a.call(1).await;
        let y = b.ready().await.unwrap().call(2).await;
        check!("use the original and a clone", (x, y), (Ok(2), Ok(4)));
    });
}

#[test]
fn readiness_errors_come_through() {
    run(async {
        let (inner, broken, _) = strict();
        broken.store(true, Ordering::SeqCst);
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(inner, stats.clone());
        let got = s.ready().await.map(|_| ());
        check!("inner poll_ready fails", got, Err("backend gone"));
        check!("nothing was counted", counts(&stats), (0, 0, 0, 0));
    });
}

#[test]
fn permits_come_back_after_cancellation() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(ConcurrencyLimit::new(service_fn(backend), 1), stats.clone());
        for _ in 0..3 {
            let call = s.ready().await.unwrap().call((1000, true));
            let _ = tokio::time::timeout(Duration::from_millis(1), call).await;
        }
        let got = tokio::time::timeout(Duration::from_secs(5), async { s.ready().await.unwrap().call((1, true)).await }).await;
        check!("after three cancellations on a limit of 1", (got.ok(), counts(&stats)), (Some(Ok(1)), (0, 1, 0, 3)));
    });
}

#[test]
fn clones_share_the_limit() {
    run(async {
        let mut a = Observe::new(ConcurrencyLimit::new(service_fn(backend), 1), Arc::new(Stats::default()));
        let mut b = a.clone();
        let call = a.ready().await.unwrap().call((100, true));
        let b_ready = tokio::time::timeout(Duration::from_millis(5), b.ready()).await.is_ok();
        check!("a clone is ready while the only permit is in use", b_ready, false);
        let _ = call.await;
    });
}

#[test]
fn an_abandoned_task_is_counted() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(service_fn(backend), stats.clone());
        let call = s.ready().await.unwrap().call((1000, true));
        let task = tokio::spawn(call);
        tokio::time::sleep(Duration::from_millis(5)).await;
        check!("while running", counts(&stats), (1, 0, 0, 0));
        task.abort();
        let _ = task.await;
        check!("after abort", counts(&stats), (0, 0, 0, 1));
    });
}

#[test]
fn finished_future_is_counted_once_even_if_kept() {
    run(async {
        let stats = Arc::new(Stats::default());
        let mut s = Observe::new(service_fn(backend), stats.clone());
        let mut call = s.ready().await.unwrap().call((1, true));
        let got = (&mut call).await;
        drop(call);
        check!("await by reference, then drop", (got, counts(&stats)), (Ok(1), (0, 1, 0, 0)));
    });
}

#[test]
fn two_stacked_layers() {
    run(async {
        let (outer, inner) = (Arc::new(Stats::default()), Arc::new(Stats::default()));
        let mut s = Observe::new(Observe::new(ConcurrencyLimit::new(service_fn(backend), 1), inner.clone()), outer.clone());
        let got = s.ready().await.unwrap().call((1, false)).await;
        check!("one failing call", (got.is_err(), counts(&outer), counts(&inner)), (true, (0, 0, 1, 0), (0, 0, 1, 0)));
    });
}

#[test]
fn random_mix_adds_up() {
    let mut rng = anneal_prelude::Rng::new(8119);
    for _ in 0..200 {
        let limit = 1 + rng.below(3);
        let len = rng.below(10);
        // (duration, succeeds, how long the caller waits)
        let jobs: Vec<(u64, bool, u64)> = (0..len).map(|_| (rng.int(1, 10) as u64 * 2, rng.bool(), rng.int(0, 10) as u64 * 2 + 1)).collect();
        let want_ok = jobs.iter().filter(|j| j.1 && j.2 > j.0).count();
        let want_err = jobs.iter().filter(|j| !j.1 && j.2 > j.0).count();
        let shown = format!("limit {limit}, (ms, ok, patience) {jobs:?}");
        let got = run(async {
            let stats = Arc::new(Stats::default());
            let mut s = Observe::new(ConcurrencyLimit::new(service_fn(backend), limit), stats.clone());
            let mut tasks = tokio::task::JoinSet::new();
            for (ms, ok, patience) in jobs.clone() {
                let ready = tokio::time::timeout(Duration::from_secs(600), s.ready()).await.expect("never ready: a permit was lost");
                let call = ready.unwrap().call((ms, ok));
                tasks.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_millis(patience), call).await;
                });
            }
            while let Some(joined) = tasks.join_next().await {
                joined.unwrap();
            }
            counts(&stats)
        });
        check!(format!("{shown}: (in_flight, ok, err, cancelled)"), got, (0, want_ok, want_err, len - want_ok - want_err));
    }
}
