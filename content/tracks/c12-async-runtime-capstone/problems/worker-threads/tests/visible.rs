use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::{poll_fn, Future};
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::channel;
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Barrier, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Waker};
#[allow(dead_code, unused_imports)]
use std::time::{Duration, Instant};

#[allow(dead_code)]
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Turns a hang into a failure.
#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(seconds))
        .expect("the test hung or panicked: a lost wake-up, or a thread that never stops")
}

/// `n` tasks that each block their thread until all `n` are running.
#[allow(dead_code)]
fn meet_up(n: usize) -> Vec<impl Future<Output = ()> + Send + 'static> {
    let barrier = Arc::new(Barrier::new(n));
    (0..n)
        .map(|_| {
            let barrier = barrier.clone();
            async move {
                barrier.wait();
            }
        })
        .collect()
}

#[allow(dead_code)]
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
fn flag() -> (Arc<AtomicBool>, DropFlag) {
    let seen = Arc::new(AtomicBool::new(false));
    (seen.clone(), DropFlag(seen))
}

/// Never finishes; leaves its waker in `keep`.
#[allow(dead_code)]
async fn parked(guard: DropFlag, keep: Arc<Mutex<Vec<Waker>>>) {
    let _guard = guard;
    poll_fn(|cx| {
        keep.lock().unwrap().push(cx.waker().clone());
        Poll::Pending
    })
    .await
}

#[allow(dead_code)]
fn poll_once<T>(handle: &mut JoinHandle<T>) -> Poll<Result<T, JoinError>> {
    Pin::new(handle).poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn spawned_work_runs_on_other_threads() {
    within(8, || {
        let rt = Runtime::new(2);
        let me = std::thread::current().id();
        let h = rt.spawn(async { std::thread::current().id() });
        let ran_on = rt.block_on(h).unwrap();
        assert!(ran_on != me, "the task ran on the calling thread");
    });
}

#[test]
fn tasks_can_spawn_and_await_tasks() {
    let sum = within(8, || {
        let rt = Runtime::new(2);
        rt.block_on(async {
            let outer = spawn(async {
                let parts: Vec<_> = (0..10).map(|i| spawn(async move { i })).collect();
                let mut sum = 0;
                for p in parts {
                    sum += p.await.unwrap();
                }
                sum
            });
            outer.await.unwrap()
        })
    });
    check!("0 + ... + 9", sum, 45);
}

#[test]
fn all_workers_run_at_once() {
    within(8, || {
        let rt = Runtime::new(4);
        let handles: Vec<_> = meet_up(4).into_iter().map(|f| rt.spawn(f)).collect();
        rt.block_on(async {
            for h in handles {
                h.await.unwrap();
            }
        });
    });
}

#[test]
fn sleeping_is_never_early() {
    within(8, || {
        let rt = Runtime::new(2);
        let started = Instant::now();
        rt.block_on(async { sleep(ms(30)).await });
        assert!(started.elapsed() >= ms(30), "woke after {:?}", started.elapsed());
    });
}

#[test]
fn dropping_the_runtime_cancels_waiting_tasks() {
    within(8, || {
        let rt = Runtime::new(2);
        let (gone, guard) = flag();
        let keep = Arc::new(Mutex::new(Vec::new()));
        let mut h = rt.spawn(parked(guard, keep.clone()));
        while keep.lock().unwrap().is_empty() {
            std::thread::yield_now();
        }
        drop(rt);
        assert!(gone.load(Ordering::SeqCst), "the waiting task's future still exists");
        assert_eq!(poll_once(&mut h), Poll::Ready(Err(JoinError::Cancelled)));
    });
}

#[test]
#[should_panic]
fn zero_workers_panics() {
    Runtime::new(0);
}
