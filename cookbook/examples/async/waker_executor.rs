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
//! count polls. Real runtimes add multi-threading, timer wheels, IO reactors (mio/epoll), work stealing.
//! One change from the book matters: the ready queue is UNBOUNDED. With a bounded one, a `wake` or a `spawn` made by the executor
//! thread while the queue is full waits for the executor itself, for ever.
//! LIMITS: waking a task twice queues it twice (harmless, one wasted visit); every `Timer` starts a thread; a panic in one task
//! stops `run`; `run` returns once no task is queued and no waker is left, so a stored waker that is never used keeps it waiting.
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::Duration;

static POLLS: AtomicUsize = AtomicUsize::new(0);

struct Task {
    future: Mutex<Option<Pin<Box<dyn Future<Output = ()> + Send>>>>,
    queue: Sender<Arc<Task>>,
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        let clone = self.clone();
        // "poll me again". A waker may outlive the executor (a timer thread, say): then there is nobody to tell.
        let _ = self.queue.send(clone);
    }
}

struct Executor {
    ready: Receiver<Arc<Task>>,
}
#[derive(Clone)]
struct Spawner {
    queue: Sender<Arc<Task>>,
}

fn new_executor() -> (Executor, Spawner) {
    let (tx, rx) = channel();
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
    // the order they finish in is printed, not asserted: a loaded machine may run the timer threads late
    let mut names = order.clone();
    names.sort_unstable();
    assert_eq!(names, ["fast", "mid", "slow", "two-step"]);
    // each timer parks its task at most once: 3 tasks x 2 polls, and 3 polls for the task with two timers
    assert!(POLLS.load(Ordering::Relaxed) <= 9);

    queue_never_blocks();
    // waking after the executor is gone does nothing
    let (executor, spawner) = new_executor();
    let late = Waker::from(Arc::new(Task {
        future: Mutex::new(None),
        queue: spawner.queue.clone(),
    }));
    drop(executor);
    late.wake();
}

/// Many more tasks than any small queue holds, all spawned before `run`, each waking itself once while being polled.
fn queue_never_blocks() {
    const TASKS: usize = 5000;
    let (done_tx, done_rx) = channel();
    thread::spawn(move || {
        let (executor, spawner) = new_executor();
        let finished = Arc::new(AtomicUsize::new(0));
        for _ in 0..TASKS {
            let finished = finished.clone();
            let mut yielded = false;
            spawner.spawn(async move {
                std::future::poll_fn(move |cx| {
                    if yielded {
                        return Poll::Ready(());
                    }
                    yielded = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                })
                .await;
                finished.fetch_add(1, Ordering::Relaxed);
            });
        }
        drop(spawner);
        executor.run();
        done_tx.send(finished.load(Ordering::Relaxed)).unwrap();
    });
    // the time limit only turns a deadlock into a failure
    assert_eq!(done_rx.recv_timeout(Duration::from_secs(30)), Ok(TASKS));
}
