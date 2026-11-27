use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::{poll_fn, Future};
#[allow(dead_code, unused_imports)]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Waker};

#[allow(dead_code)]
type Log = Arc<Mutex<Vec<String>>>;

#[allow(dead_code)]
fn note(log: &Log, line: impl Into<String>) {
    log.lock().unwrap().push(line.into());
}

#[allow(dead_code)]
fn lines(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// Turns a hang into a failure.
#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(seconds))
        .expect("the test hung or panicked")
}

/// A one-shot signal that any thread can set.
#[allow(dead_code)]
#[derive(Default)]
struct Latch {
    set: AtomicBool,
    waker: Mutex<Option<Waker>>,
}
#[allow(dead_code)]
impl Latch {
    fn open(&self) {
        self.set.store(true, Ordering::SeqCst);
        let waker = self.waker.lock().unwrap().take();
        if let Some(w) = waker {
            w.wake();
        }
    }
    fn poll(&self, cx: &mut Context<'_>) -> Poll<()> {
        *self.waker.lock().unwrap() = Some(cx.waker().clone());
        if self.set.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[allow(dead_code)]
async fn opened(latch: Arc<Latch>) {
    poll_fn(|cx| latch.poll(cx)).await
}

#[allow(dead_code)]
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[allow(dead_code, unused_imports)]
use std::time::Duration;

#[allow(dead_code)]
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Polls once with a waker that does nothing.
#[allow(dead_code)]
fn peek<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
}

/// Spawns a sleeper per delay; each writes down its id when it wakes.
#[allow(dead_code)]
fn sleepers(rt: &LocalRuntime, delays: &[u64]) -> Log {
    let log = Log::default();
    let _inside = rt.handle().enter();
    for (id, &delay) in delays.iter().enumerate() {
        let log = log.clone();
        let wait = sleep(ms(delay));
        spawn(async move {
            wait.await;
            note(&log, id.to_string());
        });
    }
    log
}

#[test]
fn nothing_fires_before_the_clock_moves() {
    let rt = LocalRuntime::new();
    let log = sleepers(&rt, &[10, 20]);
    rt.run_until_stalled();
    check!("waiting", (lines(&log), rt.pending_timers()), (Vec::<String>::new(), 2));
}

#[test]
fn timers_fire_in_deadline_order() {
    let rt = LocalRuntime::new();
    let log = sleepers(&rt, &[30, 10, 20]);
    rt.run_until_stalled();
    rt.clock().advance(ms(100));
    rt.run_until_stalled();
    check!("by deadline", lines(&log), vec!["1", "2", "0"].into_iter().map(String::from).collect::<Vec<_>>());
}

#[test]
fn a_sleep_is_never_early() {
    let rt = LocalRuntime::new();
    let log = sleepers(&rt, &[50]);
    rt.run_until_stalled();
    rt.clock().advance(ms(49));
    rt.run_until_stalled();
    check!("not at 49", lines(&log).len(), 0);
    rt.clock().advance(ms(1));
    rt.run_until_stalled();
    check!("at 50", lines(&log).len(), 1);
}

#[test]
fn timeout_expires_at_its_deadline() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let mut h = spawn(timeout(ms(50), std::future::pending::<u8>()));
    rt.run_until_stalled();
    rt.clock().advance(ms(49));
    rt.run_until_stalled();
    check!("at 49", peek(&mut h), Poll::Pending);
    rt.clock().advance(ms(1));
    rt.run_until_stalled();
    check!("at 50", peek(&mut h), Poll::Ready(Ok(Err(Elapsed))));
}

#[test]
fn interval_ticks_in_bursts_when_late() {
    let rt = LocalRuntime::new();
    let log = Log::default();
    let l = log.clone();
    rt.spawn(async move {
        let mut every = interval(ms(10));
        loop {
            let due = every.tick().await;
            note(&l, due.as_millis().to_string());
        }
    });
    rt.run_until_stalled();
    check!("the first tick at once", lines(&log), vec!["0".to_string()]);
    rt.clock().advance(ms(35));
    rt.run_until_stalled();
    check!("three late ticks", lines(&log), vec!["0", "10", "20", "30"].into_iter().map(String::from).collect::<Vec<_>>());
}

#[test]
fn another_thread_can_move_the_clock() {
    within(5, || {
        let rt = LocalRuntime::new();
        let clock = rt.clock();
        let t = std::thread::spawn(move || {
            std::thread::sleep(ms(30));
            clock.advance(ms(1000));
        });
        rt.block_on(async { sleep(ms(1000)).await });
        t.join().unwrap();
    });
}
