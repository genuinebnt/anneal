//! RECIPE 78 (async): build a real executor: tasks, wakers, a ready-queue, and a timer future that wakes from another thread.
//! LEVEL: advanced
//! Use in: truly understanding tokio: what `spawn`, `Waker`, and "task is pending" mean.   Run: cargo run --release --example waker_executor
//! COMMON USES: understanding tokio internals; custom executors for embedded/real-time/test harnesses.
#![allow(dead_code)]
//!
//! Pieces: (1) `Task` = a boxed future + a handle to the queue; (2) `impl Wake for Task` so `waker.wake()` re-queues the task;
//! (3) the executor loop pulls READY tasks and polls them; a task that returns Pending is parked until its waker fires;
//! (4) a `Timer` future stores the latest waker and a helper thread calls it when time is up.
//! Nothing busy-waits: the executor blocks on `recv()` when nothing is ready (compare futures_by_hand, which polls in a loop).
//! This is the executor from the Async Book https://rust-lang.github.io/async-book/02_execution/04_executor.html extended to
//! count polls. Real runtimes add multi-threading, timers wheels, IO reactors (mio/epoll), work stealing.
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::Duration;

static POLLS: AtomicUsize = AtomicUsize::new(0);

struct Task {
    future: Mutex<Option<Pin<Box<dyn Future<Output = ()> + Send>>>>,
    queue: SyncSender<Arc<Task>>,
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        let clone = self.clone();
        self.queue.send(clone).expect("executor queue closed"); // "poll me again"
    }
}

struct Executor {
    ready: Receiver<Arc<Task>>,
}
#[derive(Clone)]
struct Spawner {
    queue: SyncSender<Arc<Task>>,
}

fn new_executor() -> (Executor, Spawner) {
    let (tx, rx) = sync_channel(1024);
    (Executor { ready: rx }, Spawner { queue: tx })
}

impl Spawner {
    fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
        let task = Arc::new(Task {
            future: Mutex::new(Some(Box::pin(fut))),
            queue: self.queue.clone(),
        });
        self.queue.send(task).unwrap();
    }
}

impl Executor {
    fn run(&self) {
        // recv() blocks while every task is parked; it returns Err once all senders (spawner + tasks) are gone
        while let Ok(task) = self.ready.recv() {
            let mut slot = task.future.lock().unwrap();
            if let Some(mut fut) = slot.take() {
                let waker = Waker::from(task.clone());
                let mut cx = Context::from_waker(&waker);
                POLLS.fetch_add(1, Ordering::Relaxed);
                if fut.as_mut().poll(&mut cx).is_pending() {
                    *slot = Some(fut); // park it; the waker will re-queue the task
                }
            }
        }
    }
}

struct TimerState {
    done: bool,
    waker: Option<Waker>,
}
struct Timer {
    state: Arc<Mutex<TimerState>>,
}
impl Timer {
    fn new(d: Duration) -> Timer {
        let state = Arc::new(Mutex::new(TimerState {
            done: false,
            waker: None,
        }));
        let shared = state.clone();
        thread::spawn(move || {
            thread::sleep(d);
            let mut s = shared.lock().unwrap();
            s.done = true;
            if let Some(w) = s.waker.take() {
                w.wake(); // tell the executor this task can make progress
            }
        });
        Timer { state }
    }
}
impl Future for Timer {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut s = self.state.lock().unwrap();
        if s.done {
            Poll::Ready(())
        } else {
            s.waker = Some(cx.waker().clone()); // ALWAYS store the latest waker: the task may have moved
            Poll::Pending
        }
    }
}

fn main() {
    let (executor, spawner) = new_executor();
    let finished: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(vec![]));

    for (name, ms) in [("slow", 90), ("fast", 30), ("mid", 60)] {
        let finished = finished.clone();
        spawner.spawn(async move {
            Timer::new(Duration::from_millis(ms)).await;
            finished.lock().unwrap().push(name);
        });
    }
    let f2 = finished.clone();
    spawner.spawn(async move {
        // two sequential awaits in one task: suspends twice
        Timer::new(Duration::from_millis(10)).await;
        Timer::new(Duration::from_millis(10)).await;
        f2.lock().unwrap().push("two-step");
    });
    drop(spawner); // no more spawning: run() ends when the last task completes
    executor.run();

    let order = finished.lock().unwrap().clone();
    println!("completion order: {order:?}");
    println!(
        "total polls: {} (4 tasks; each parked while waiting instead of spinning)",
        POLLS.load(Ordering::Relaxed)
    );
    assert_eq!(order, ["two-step", "fast", "mid", "slow"]);
    assert!(POLLS.load(Ordering::Relaxed) <= 12);
}
