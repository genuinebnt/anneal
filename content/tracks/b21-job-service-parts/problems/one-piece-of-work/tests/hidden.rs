use solution::*;

#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use tokio::sync::watch;
#[allow(unused_imports)]
use tokio::time::{Instant, sleep, timeout};

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[allow(dead_code)]
pub async fn settle() {
    sleep(ms(1)).await;
}

#[allow(dead_code)]
pub const HOUR: Duration = Duration::from_secs(3600);

#[allow(dead_code)]
/// Lives inside a piece of work: counts it as alive until its future is dropped.
pub struct Alive(Arc<AtomicUsize>);

#[allow(dead_code)]
impl Alive {
    pub fn new(n: &Arc<AtomicUsize>) -> Self {
        n.fetch_add(1, Ordering::SeqCst);
        Alive(n.clone())
    }
}

#[allow(dead_code)]
impl Drop for Alive {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
/// Work that takes `millis` and returns 7. `alive` counts its future; `finished` is set if it gets to the end.
pub fn work(millis: u64, alive: &Arc<AtomicUsize>, finished: &Arc<AtomicBool>) -> impl std::future::Future<Output = u32> + Send + 'static + use<> {
    let (guard, finished) = (Alive::new(alive), finished.clone());
    async move {
        let _guard = guard;
        sleep(ms(millis)).await;
        finished.store(true, Ordering::SeqCst);
        7
    }
}

#[allow(dead_code)]
pub fn send_true_after(sender: watch::Sender<bool>, millis: u64) {
    tokio::spawn(async move {
        sleep(ms(millis)).await;
        let _ = sender.send(true);
        sleep(HOUR).await;
    });
}

#[allow(dead_code)]
pub async fn bounded<T: Send + 'static>(cancel: watch::Receiver<bool>, limit: Duration, work: impl std::future::Future<Output = T> + Send + 'static) -> Ending<T> {
    timeout(HOUR * 24, supervise(cancel, limit, work)).await.expect("supervise did not return within a day of virtual time")
}

#[test]
fn panic_payloads() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let got: Ending<()> = bounded(rx.clone(), HOUR, async { panic!("plain text") }).await;
        check!("a &str payload", got, Ending::Panicked("plain text".to_string()));
        let got: Ending<()> = bounded(rx.clone(), HOUR, async { std::panic::panic_any(String::from("owned")) }).await;
        check!("a String payload", got, Ending::Panicked("owned".to_string()));
        let got: Ending<()> = bounded(rx, HOUR, async { std::panic::panic_any(42u32) }).await;
        check!("a u32 payload", got, Ending::Panicked("unknown panic".to_string()));
    });
}

#[test]
fn a_panic_after_waiting_drops_the_future() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let alive = Arc::new(AtomicUsize::new(0));
        let guard = Alive::new(&alive);
        let got: Ending<()> = bounded(rx, HOUR, async move {
            let _guard = guard;
            sleep(ms(5)).await;
            panic!("late")
        })
        .await;
        check!("(ending, alive)", (got, alive.load(Ordering::SeqCst)), (Ending::Panicked("late".to_string()), 0));
    });
}

#[test]
fn a_dropped_sender_does_not_cancel() {
    run(async {
        let (tx, rx) = watch::channel(false);
        drop(tx);
        let got = bounded(rx, HOUR, async {
            sleep(ms(10)).await;
            1
        })
        .await;
        check!("sender dropped before the call", got, Ending::Done(1));
        let (tx, rx) = watch::channel(false);
        tokio::spawn(async move {
            sleep(ms(5)).await;
            drop(tx);
        });
        let got = bounded(rx, HOUR, async {
            sleep(ms(10)).await;
            2
        })
        .await;
        check!("sender dropped during the work", got, Ending::Done(2));
        let (tx, rx) = watch::channel(false);
        drop(tx);
        let got = bounded(rx, ms(10), sleep(ms(50))).await;
        check!("sender dropped, work too slow", got, Ending::TimedOut);
    });
}

#[test]
fn sending_false_changes_nothing() {
    run(async {
        let (tx, rx) = watch::channel(false);
        tokio::spawn(async move {
            for _ in 0..5 {
                sleep(ms(2)).await;
                let _ = tx.send(false);
            }
            sleep(HOUR).await;
        });
        let got = bounded(rx, HOUR, async {
            sleep(ms(30)).await;
            3
        })
        .await;
        check!("false sent five times", got, Ending::Done(3));
    });
}

#[test]
fn true_then_dropped_still_cancels() {
    run(async {
        let (tx, rx) = watch::channel(false);
        tokio::spawn(async move {
            sleep(ms(5)).await;
            let _ = tx.send(true);
        });
        let start = Instant::now();
        let got = bounded(rx, HOUR, sleep(ms(500))).await;
        check!("send(true), then the sender is dropped", (got, start.elapsed()), (Ending::Cancelled, ms(5)));
    });
}

#[test]
fn giving_up_on_supervise_stops_the_work() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let (alive, finished) = (Arc::default(), Arc::<AtomicBool>::default());
        let outer = timeout(ms(10), supervise(rx, HOUR, work(100, &alive, &finished))).await;
        check!("the outer timeout fired", outer.is_err(), true);
        settle().await;
        check!("work futures alive afterwards", alive.load(Ordering::SeqCst), 0);
        sleep(ms(500)).await;
        check!("work ran to its end", finished.load(Ordering::SeqCst), false);
    });
}

#[test]
fn the_longest_limit() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let got = bounded(rx, Duration::MAX, async {
            sleep(ms(10)).await;
            4
        })
        .await;
        check!("limit Duration::MAX", got, Ending::Done(4));
    });
}

#[test]
fn the_work_is_gone_even_if_it_was_busy() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_time().build().unwrap();
    runtime.block_on(async {
        for by_timeout in [true, false] {
            let (tx, rx) = watch::channel(false);
            let alive = Arc::new(AtomicUsize::new(0));
            let guard = Alive::new(&alive);
            let (started_tx, started) = tokio::sync::oneshot::channel();
            let busy = async move {
                let _guard = guard;
                let _ = started_tx.send(());
                // Holds its thread: an abort cannot take effect until this poll returns.
                std::thread::sleep(Duration::from_millis(150));
                sleep(HOUR).await;
            };
            let canceller = tokio::spawn(async move {
                let _ = started.await;
                if !by_timeout {
                    let _ = tx.send(true);
                }
                tx
            });
            let limit = if by_timeout { Duration::from_millis(30) } else { HOUR };
            let got = supervise(rx, limit, busy).await;
            let left = alive.load(Ordering::SeqCst);
            let want = if by_timeout { Ending::TimedOut } else { Ending::Cancelled };
            check!(format!("busy work, by_timeout = {by_timeout}: (ending, futures alive on return)"), (got, left), (want, 0));
            drop(canceller);
        }
    });
}

#[test]
fn works_from_a_spawned_task() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let handle = tokio::spawn(supervise(rx, HOUR, async { 5 }));
        check!("spawned supervise", handle.await.unwrap(), Ending::Done(5));
    });
}

#[test]
fn random_races_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(21304);
    for _ in 0..300 {
        // Three distinct times: no two events at the same instant.
        let mut times: Vec<u64> = (1..=40).collect();
        rng.shuffle(&mut times);
        let (took, limit, at) = (times[0], times[1], times[2]);
        let cancels = rng.bool();
        let want = if cancels && at < took && at < limit {
            (Ending::Cancelled, at)
        } else if took < limit {
            (Ending::Done(7), took)
        } else {
            (Ending::TimedOut, limit)
        };
        let got = run(async {
            let (tx, rx) = watch::channel(false);
            let (alive, finished) = (Arc::default(), Arc::default());
            let keep = if cancels {
                send_true_after(tx, at);
                None
            } else {
                Some(tx)
            };
            let start = Instant::now();
            let ending = bounded(rx, ms(limit), work(took, &alive, &finished)).await;
            drop(keep);
            assert_eq!(alive.load(Ordering::SeqCst), 0, "work future still alive on return");
            (ending, start.elapsed().as_millis() as u64)
        });
        check!(format!("work {took} ms, limit {limit} ms, cancel {}: (ending, ms)", if cancels { format!("at {at} ms") } else { "never".to_string() }), got, want);
    }
}
