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

#[test]
fn block_on_returns_the_value() {
    let rt = LocalRuntime::new();
    let text = String::from("borrowed");
    check!("a root that borrows", rt.block_on(async { text.len() }), 8);
}

#[test]
fn spawned_tasks_take_turns() {
    let rt = LocalRuntime::new();
    let log = Log::default();
    let handles: Vec<_> = (0..3)
        .map(|id| {
            let log = log.clone();
            rt.spawn(async move {
                for _ in 0..3 {
                    note(&log, id.to_string());
                    yield_now().await;
                }
            })
        })
        .collect();
    check!("spawn only queues", lines(&log), Vec::<String>::new());
    rt.block_on(async {
        for h in handles {
            h.await.unwrap();
        }
    });
    check!("round robin", lines(&log).concat(), "012012012".to_string());
}

#[test]
fn run_until_stalled_counts_the_runs() {
    let rt = LocalRuntime::new();
    rt.spawn(async {
        yield_now().await;
        yield_now().await;
    });
    rt.spawn(async {});
    check!("3 runs and 1 run", rt.run_until_stalled(), 4);
    check!("nothing left", rt.run_until_stalled(), 0);
}

#[test]
fn spawn_from_inside_the_runtime() {
    let rt = LocalRuntime::new();
    let sum = rt.block_on(async {
        let a = spawn(async { spawn(async { 1 }).await.unwrap() + 2 });
        a.await.unwrap()
    });
    check!("nested spawns", sum, 3);
}

#[test]
#[should_panic]
fn spawn_outside_a_runtime_panics() {
    drop(spawn(async {}));
}

#[test]
fn yield_now_is_pending_once() {
    let mut f = Box::pin(yield_now());
    let mut cx = Context::from_waker(Waker::noop());
    check!("first", f.as_mut().poll(&mut cx), Poll::Pending);
    check!("second", f.as_mut().poll(&mut cx), Poll::Ready(()));
}

#[test]
fn another_thread_wakes_the_root() {
    let got = within(5, || {
        let rt = LocalRuntime::new();
        let latch = Arc::new(Latch::default());
        let l = latch.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            l.open();
        });
        rt.block_on(opened(latch));
        t.join().unwrap();
        1
    });
    check!("woken", got, 1);
}
