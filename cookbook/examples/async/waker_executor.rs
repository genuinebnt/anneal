//! RECIPE 78 (async): build a real executor: tasks, wakers, a ready-queue, and a timer future that wakes from another thread.
//! LEVEL: advanced
//! Use in: truly understanding tokio: what `spawn`, `Waker`, and "task is pending" mean.   Run: cargo run --release --example waker_executor
//! COMMON USES: understanding tokio internals; custom executors for embedded/real-time/test harnesses.
#![allow(dead_code)]
//!
//! A `Task` holds a boxed future and a sender for the ready queue. `impl Wake for Task` makes `waker.wake()` put the task
//! back in the queue. The executor takes ready tasks from the queue and polls them. A task that returns `Pending` waits
//! outside the queue until its waker is called. A `Timer` future stores the latest waker, and a helper thread calls it
//! when the time is up.
//! Nothing busy-waits: the executor blocks on `recv()` when nothing is ready (compare `futures_by_hand`, which polls in a
//! loop). This is the executor from the Async Book, extended to count polls:
//! https://rust-lang.github.io/async-book/02_execution/04_executor.html
//! One change from the book matters: the ready queue is unbounded. With a bounded one, a `wake` or a `spawn` made on the
//! executor's own thread while the queue is full would wait for the executor, which is that same thread, for ever.
//! Limits: waking a task twice queues it twice (one wasted visit). Every `Timer` starts a thread. A panic in a task stops
//! `run`. `run` returns only when every sender is gone, so a waker that is stored and never used keeps it waiting.
//! Real runtimes add more threads, timer wheels, an I/O reactor (mio over epoll or kqueue) and work stealing.
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
            let waker = {
                let mut s = shared.lock().unwrap();
                s.done = true;
                s.waker.take()
            };
            // Wake after unlocking. A waker may run code that polls this timer, and that poll takes the same lock.
            if let Some(w) = waker {
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
            s.waker = Some(cx.waker().clone()); // always store the latest waker: the task may be polled with a new one
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
    timer_wakes_after_unlocking();
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
    const TASKS: usize = if cfg!(miri) { 300 } else { 5000 }; // Miri interprets, and is far slower
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

/// The timer thread must call the waker with the timer's lock released. This waker looks at the lock when it is called.
fn timer_wakes_after_unlocking() {
    struct LockCheck {
        state: Arc<Mutex<TimerState>>,
        free: Sender<bool>,
    }
    impl Wake for LockCheck {
        fn wake(self: Arc<Self>) {
            let _ = self.free.send(self.state.try_lock().is_ok());
        }
    }
    // The check needs a poll that returns Pending: retry with a longer timer if the thread was quicker than the poll.
    for ms in [20, 200, 2000] {
        let mut timer = Timer::new(Duration::from_millis(ms));
        let (tx, rx) = channel();
        let waker = Waker::from(Arc::new(LockCheck {
            state: timer.state.clone(),
            free: tx,
        }));
        if Pin::new(&mut timer).poll(&mut Context::from_waker(&waker)).is_pending() {
            assert_eq!(rx.recv(), Ok(true), "the waker was called while the timer's lock was held");
            return;
        }
    }
    panic!("every timer fired before its first poll");
}
