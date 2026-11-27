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
fn work_that_finishes() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let got = bounded(rx, ms(100), async {
            sleep(ms(10)).await;
            "result"
        })
        .await;
        check!("10 ms of work, limit 100 ms", got, Ending::Done("result"));
    });
}

#[test]
fn work_that_panics() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let got: Ending<()> = bounded(rx, HOUR, async {
            let name = "cat.png";
            panic!("cannot decode {name}")
        })
        .await;
        check!("a panic with a formatted message", got, Ending::Panicked("cannot decode cat.png".to_string()));
    });
}

#[test]
fn work_that_is_too_slow() {
    run(async {
        let (_tx, rx) = watch::channel(false);
        let (alive, finished) = (Arc::default(), Arc::default());
        let start = Instant::now();
        let got = bounded(rx, ms(30), work(500, &alive, &finished)).await;
        check!("500 ms of work, limit 30 ms: (ending, elapsed, futures alive)", (got, start.elapsed(), alive.load(Ordering::SeqCst)), (Ending::TimedOut, ms(30), 0));
    });
}

#[test]
fn work_that_is_cancelled() {
    run(async {
        let (tx, rx) = watch::channel(false);
        let (alive, finished) = (Arc::default(), Arc::default());
        send_true_after(tx, 20);
        let start = Instant::now();
        let got = bounded(rx, HOUR, work(500, &alive, &finished)).await;
        check!("cancel at 20 ms: (ending, elapsed, futures alive)", (got, start.elapsed(), alive.load(Ordering::SeqCst)), (Ending::Cancelled, ms(20), 0));
    });
}

#[test]
fn already_cancelled() {
    run(async {
        let (_tx, rx) = watch::channel(true);
        let polled = Arc::new(AtomicBool::new(false));
        let p = polled.clone();
        let got = bounded(rx, HOUR, async move {
            p.store(true, Ordering::SeqCst);
        })
        .await;
        settle().await;
        check!("cancelled before the call: (ending, work polled)", (got, polled.load(Ordering::SeqCst)), (Ending::Cancelled, false));
    });
}
