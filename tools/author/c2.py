from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# Runs `f` on its own thread and gives up after `ms`, so a deadlocked solution fails fast instead of hitting the 15 s cap.
WITHIN = """
fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}
"""

# ---------------------------------------------------------------------------------------------------------------
POOL = """
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    panicked: AtomicUsize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskPanicked;

/// The result of one `submit`.
pub struct TaskHandle<R> {
    rx: mpsc::Receiver<R>,
}

impl<R> TaskHandle<R> {
    /// Waits for the task. `Err` if it panicked.
    pub fn join(self) -> Result<R, TaskPanicked> {
        self.rx.recv().map_err(|_| TaskPanicked)
    }
}

pub struct ThreadPool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

fn worker(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut q = shared.queue.lock().unwrap();
            loop {
                if let Some(job) = q.jobs.pop_front() {
                    break Some(job);
                }
                if q.closed {
                    break None; // closed AND drained: every queued job has been taken
                }
                q = shared.wake.wait(q).unwrap();
            }
        };
        match job {
            Some(job) => {
                // run outside the lock; a panicking job must not take the worker down with it
                if catch_unwind(AssertUnwindSafe(job)).is_err() {
                    shared.panicked.fetch_add(1, Ordering::SeqCst);
                }
            }
            None => return,
        }
    }
}

impl ThreadPool {
    /// Panics if `workers` is 0.
    pub fn new(workers: usize) -> Self {
        assert!(workers > 0, "a pool needs at least one worker");
        let shared = Arc::new(Shared { queue: Mutex::new(Queue { jobs: VecDeque::new(), closed: false }), wake: Condvar::new(), panicked: AtomicUsize::new(0) });
        let workers = (0..workers)
            .map(|i| {
                let shared = shared.clone();
                std::thread::Builder::new().name(format!("pool-worker-{i}")).spawn(move || worker(shared)).unwrap()
            })
            .collect();
        ThreadPool { shared, workers }
    }

    pub fn execute<F: FnOnce() + Send + 'static>(&self, f: F) {
        self.shared.queue.lock().unwrap().jobs.push_back(Box::new(f));
        self.shared.wake.notify_one();
    }

    pub fn submit<F, R>(&self, f: F) -> TaskHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.execute(move || {
            let _ = tx.send(f()); // if `f` panics, `tx` is dropped unsent and `join` reports it
        });
        TaskHandle { rx }
    }

    /// Stops accepting work, lets the workers finish EVERY job already queued, joins them, and returns how many jobs panicked.
    pub fn shutdown(mut self) -> usize {
        self.finish()
    }

    fn finish(&mut self) -> usize {
        self.shared.queue.lock().unwrap().closed = true;
        self.shared.wake.notify_all();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
        self.shared.panicked.load(Ordering::SeqCst)
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.finish();
    }
}
"""

POOL_STARTER = """
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    panicked: AtomicUsize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskPanicked;

pub struct TaskHandle<R> {
    rx: mpsc::Receiver<R>,
}

impl<R> TaskHandle<R> {
    /// Waits for the task's result. `Err(TaskPanicked)` if the task panicked.
    pub fn join(self) -> Result<R, TaskPanicked> {
        todo!()
    }
}

pub struct ThreadPool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    /// Starts `workers` threads that take jobs in FIFO order. Panics if `workers` is 0.
    pub fn new(workers: usize) -> Self {
        todo!()
    }

    /// Queues a job. Never runs it on the calling thread.
    pub fn execute<F: FnOnce() + Send + 'static>(&self, f: F) {
        todo!()
    }

    /// Like `execute`, but the closure's result comes back through the handle.
    pub fn submit<F, R>(&self, f: F) -> TaskHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        todo!()
    }

    /// Stops accepting work, lets the workers finish every job already queued, joins them, and returns how many
    /// jobs panicked. A panicking job must not kill its worker.
    pub fn shutdown(self) -> usize {
        todo!()
    }
}

// TODO: dropping a pool must do the same as `shutdown` (no detached threads, no lost jobs).
"""

P.append(dict(
    slug="thread-pool", title="A thread pool with results and graceful shutdown", level="hard", stage="build-it",
    tags=["thread pool", "Condvar", "catch_unwind", "JoinHandle", "graceful shutdown", "Drop"],
    teaches=["Workers sleeping on a condvar over a shared queue; closed-and-drained is the only exit condition.",
             "Running jobs outside the lock under `catch_unwind` so one panic does not shrink the pool.",
             "Returning results through a channel: a dropped sender (panic) becomes `Err` at `join`. `Drop` that shuts down cleanly."],
    statement="""
        `ThreadPool::new(n)` starts `n` worker threads that take boxed jobs from one FIFO queue.
        `execute(f)` queues a job; `submit(f)` queues it and returns a `TaskHandle<R>` whose `join()` gives the
        closure's result, or `Err(TaskPanicked)` if the closure panicked.

        - A job that panics must **not** kill its worker; the pool keeps its full size.
        - `shutdown(self)` stops accepting work, lets the workers finish **every job already queued**, joins all
          threads and returns how many jobs panicked. Dropping the pool without calling it must do the same.
        - Jobs never run on the calling thread, and the pool never starts more than `n` threads.
    """,
    starter=POOL_STARTER,
    solution=POOL,
    visible=[
        T("submit_returns_the_result", "submit(|| 6 * 7)", "{ let p = ThreadPool::new(2); p.submit(|| 6 * 7).join() }", "Ok(42)"),
        T("all_jobs_run", "20 jobs add to a counter; shutdown", "{ let n = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)); let p = ThreadPool::new(3); for _ in 0..20 { let n = n.clone(); p.execute(move || { n.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }); } p.shutdown(); n.load(std::sync::atomic::Ordering::SeqCst) }", "20"),
        T("runs_off_the_calling_thread", "job thread id differs", "{ let me = std::thread::current().id(); let p = ThreadPool::new(1); p.submit(move || std::thread::current().id() != me).join() }", "Ok(true)"),
        T("single_worker_is_fifo", "one worker keeps submit order", "{ let p = ThreadPool::new(1); let hs: Vec<_> = (0..5).map(|i| { p.submit(move || i) }).collect(); let r: Vec<_> = hs.into_iter().map(|h| h.join().unwrap()).collect(); r }", "vec![0, 1, 2, 3, 4]"),
        T("panic_becomes_err", "a panicking task", "{ let p = ThreadPool::new(1); let h = p.submit(|| -> u8 { panic!(\"boom\") }); h.join() }", "Err(TaskPanicked)"),
    ],
    hidden=[
        WITHIN + """
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier, Mutex};
        use std::time::Duration;

        #[test]
        fn zero_workers_panics() {
            assert!(std::panic::catch_unwind(|| ThreadPool::new(0)).is_err());
        }

        #[test]
        fn worker_survives_a_panicking_job() {
            let p = ThreadPool::new(1);
            let bad = p.submit(|| -> u8 { panic!("job panic") });
            let good = p.submit(|| 5u8);
            check!("the panicking task", within(3000, move || bad.join()), Some(Err(TaskPanicked)));
            check!("the only worker is still alive for the next job", within(3000, move || good.join()), Some(Ok(5)));
        }

        #[test]
        fn pool_keeps_its_size_after_many_panics() {
            let p = ThreadPool::new(2);
            for _ in 0..6 {
                p.execute(|| panic!("again"));
            }
            // two workers must still be able to run two jobs at the same time
            let barrier = Arc::new(Barrier::new(2));
            let hs: Vec<_> = (0..2).map(|_| { let b = barrier.clone(); p.submit(move || { b.wait(); 1 }) }).collect();
            let r = within(3000, move || hs.into_iter().map(|h| h.join().unwrap()).sum::<i32>());
            check!("two rendezvousing jobs after six panics", r, Some(2));
            check!("shutdown reports the panics", p.shutdown(), 6);
        }

        #[test]
        fn jobs_really_run_in_parallel() {
            let p = ThreadPool::new(4);
            let barrier = Arc::new(Barrier::new(4));
            let hs: Vec<_> = (0..4).map(|_| { let b = barrier.clone(); p.submit(move || { b.wait(); 1 }) }).collect();
            let r = within(3000, move || hs.into_iter().map(|h| h.join().unwrap()).sum::<i32>());
            check!("4 jobs that wait for each other on 4 workers", r, Some(4));
        }

        #[test]
        fn never_more_threads_than_workers() {
            let ids = Arc::new(Mutex::new(std::collections::HashSet::new()));
            let p = ThreadPool::new(3);
            for _ in 0..200 {
                let ids = ids.clone();
                p.execute(move || {
                    ids.lock().unwrap().insert(std::thread::current().id());
                    std::thread::sleep(Duration::from_micros(200));
                });
            }
            p.shutdown();
            let n = ids.lock().unwrap().len();
            check!("distinct threads that ran 200 jobs (1 to 3)", (1..=3).contains(&n), true);
        }

        #[test]
        fn shutdown_runs_every_queued_job() {
            let done = Arc::new(AtomicUsize::new(0));
            let p = ThreadPool::new(2);
            for _ in 0..60 {
                let done = done.clone();
                p.execute(move || {
                    std::thread::sleep(Duration::from_millis(2));
                    done.fetch_add(1, Ordering::SeqCst);
                });
            }
            let panics = p.shutdown(); // most jobs are still queued right now
            check!("jobs finished and panics", (done.load(Ordering::SeqCst), panics), (60, 0));
        }

        #[test]
        fn drop_joins_the_workers_and_finishes_the_queue() {
            let done = Arc::new(AtomicUsize::new(0));
            {
                let p = ThreadPool::new(2);
                for _ in 0..10 {
                    let done = done.clone();
                    p.execute(move || {
                        std::thread::sleep(Duration::from_millis(20));
                        done.fetch_add(1, Ordering::SeqCst);
                    });
                }
            } // dropped here
            check!("all ten finished by the time the pool is gone", done.load(Ordering::SeqCst), 10);
        }

        #[test]
        fn results_come_back_to_the_right_handle() {
            let p = ThreadPool::new(4);
            let hs: Vec<_> = (0..200u64).map(|i| p.submit(move || { std::thread::sleep(Duration::from_micros(50 * (i % 5))); i * i })).collect();
            let got: Vec<u64> = hs.into_iter().map(|h| h.join().unwrap()).collect();
            check!("200 tasks with mixed durations: squares in handle order", got, (0..200u64).map(|i| i * i).collect::<Vec<_>>());
        }

        #[test]
        fn executing_from_several_threads_at_once() {
            let p = Arc::new(ThreadPool::new(3));
            let total = Arc::new(AtomicUsize::new(0));
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    let (p, total) = (p.clone(), total.clone());
                    std::thread::spawn(move || {
                        for _ in 0..2_500 {
                            let total = total.clone();
                            p.execute(move || { total.fetch_add(1, Ordering::SeqCst); });
                        }
                    })
                })
                .collect();
            for h in hs {
                h.join().unwrap();
            }
            let p = Arc::try_unwrap(p).ok().expect("pool still shared");
            p.shutdown();
            check!("4 submitting threads x 2500 jobs", total.load(Ordering::SeqCst), 10_000);
        }
        """,
    ],
    wrong=dict(
        panic_kills_the_worker=sub(POOL, "if catch_unwind(AssertUnwindSafe(job)).is_err() {\n                    shared.panicked.fetch_add(1, Ordering::SeqCst);\n                }", "job();"),
        shutdown_abandons_queue=sub(POOL, "if let Some(job) = q.jobs.pop_front() {\n                    break Some(job);\n                }\n                if q.closed {\n                    break None; // closed AND drained: every queued job has been taken\n                }", "if q.closed {\n                    break None;\n                }\n                if let Some(job) = q.jobs.pop_front() {\n                    break Some(job);\n                }"),
        thread_per_job=sub(POOL, "self.shared.queue.lock().unwrap().jobs.push_back(Box::new(f));\n        self.shared.wake.notify_one();", "std::thread::spawn(f);"),
        drop_does_not_join=sub(POOL, "impl Drop for ThreadPool {\n    fn drop(&mut self) {\n        self.finish();\n    }\n}", ""),
    ),
    hints=[("approach", "One `Mutex<{ jobs: VecDeque<Job>, closed }>` and a `Condvar`. A worker loops: lock, take a job (or wait), unlock, run it. It exits only when `closed` is set **and** the queue is empty, which is what makes shutdown graceful."),
           ("rust", "`type Job = Box<dyn FnOnce() + Send + 'static>`. Run with `catch_unwind(AssertUnwindSafe(job))` outside the lock. `submit` wraps the closure: create `mpsc::channel()`, send `f()` from inside the job, and let the handle `recv()`; a panic drops the sender, so `recv` fails."),
           ("edge", "`shutdown(self)` and `Drop` share one `finish(&mut self)`: set `closed`, `notify_all`, join every `JoinHandle` via `workers.drain(..)` so `Drop` after `shutdown` has nothing left to do.")],
    notes=("The pool is exactly the blocking queue from C1 with workers on the consumer side. The two decisions that matter are *where the closed flag is tested* (after the queue is empty, so nothing queued is lost) and *what a panic does* (caught per job, counted, the worker lives on). Results travel back over a one-shot channel, so the handle needs no knowledge of the pool, and a worker that dies with the sender simply drops it.", "O(1) per job", "O(workers + queued jobs)"),
    follow_up="1) Why does every worker contend on one mutex, and how does per-worker queues with work stealing (rayon, crossbeam-deque) remove that? 2) What happens if a job submits to the same pool and then blocks on the result (nested `join`)? Construct the deadlock with one worker, and the fix. 3) How would you add `scope`-style borrowing so jobs can use non-`'static` data (what `std::thread::scope` and `rayon::scope` guarantee)? 4) Add a bounded queue and decide what `execute` does when full: block, drop or error (backpressure). 5) What does `JoinHandle::join` return for a panicked thread, and why `AssertUnwindSafe` is a promise, not a fix?",
    related=["C6", "S7"],
))

# ---------------------------------------------------------------------------------------------------------------
CHAN = """
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub struct SendError<T>(pub T);

#[derive(Debug, PartialEq, Eq)]
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RecvError;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RecvTimeoutError {
    Timeout,
    Disconnected,
}

struct State<T> {
    queue: VecDeque<T>,
    cap: usize,
    senders: usize,
    receivers: usize,
}

struct Chan<T> {
    state: Mutex<State<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

pub struct Sender<T> {
    chan: Arc<Chan<T>>,
}

pub struct Receiver<T> {
    chan: Arc<Chan<T>>,
}

/// A bounded multi-producer multi-consumer channel. Panics if `cap` is 0.
pub fn channel<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    assert!(cap > 0, "capacity must be at least 1");
    let chan = Arc::new(Chan {
        state: Mutex::new(State { queue: VecDeque::new(), cap, senders: 1, receivers: 1 }),
        not_empty: Condvar::new(),
        not_full: Condvar::new(),
    });
    (Sender { chan: chan.clone() }, Receiver { chan })
}

impl<T> Sender<T> {
    /// Waits for room. `Err` (with the value) once every receiver is gone, also when that happens while waiting.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut s = self.chan.state.lock().unwrap();
        loop {
            if s.receivers == 0 {
                return Err(SendError(value));
            }
            if s.queue.len() < s.cap {
                s.queue.push_back(value);
                self.chan.not_empty.notify_one();
                return Ok(());
            }
            s = self.chan.not_full.wait(s).unwrap();
        }
    }

    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        let mut s = self.chan.state.lock().unwrap();
        if s.receivers == 0 {
            return Err(TrySendError::Disconnected(value));
        }
        if s.queue.len() == s.cap {
            return Err(TrySendError::Full(value));
        }
        s.queue.push_back(value);
        self.chan.not_empty.notify_one();
        Ok(())
    }
}

impl<T> Receiver<T> {
    /// Waits for a value. `Err` once the queue is empty and every sender is gone.
    pub fn recv(&self) -> Result<T, RecvError> {
        let mut s = self.chan.state.lock().unwrap();
        loop {
            if let Some(v) = s.queue.pop_front() {
                self.chan.not_full.notify_one();
                return Ok(v);
            }
            if s.senders == 0 {
                return Err(RecvError);
            }
            s = self.chan.not_empty.wait(s).unwrap();
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut s = self.chan.state.lock().unwrap();
        if let Some(v) = s.queue.pop_front() {
            self.chan.not_full.notify_one();
            return Ok(v);
        }
        if s.senders == 0 { Err(TryRecvError::Disconnected) } else { Err(TryRecvError::Empty) }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let deadline = Instant::now() + timeout;
        let mut s = self.chan.state.lock().unwrap();
        loop {
            if let Some(v) = s.queue.pop_front() {
                self.chan.not_full.notify_one();
                return Ok(v);
            }
            if s.senders == 0 {
                return Err(RecvTimeoutError::Disconnected);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(RecvTimeoutError::Timeout);
            }
            s = self.chan.not_empty.wait_timeout(s, deadline - now).unwrap().0;
        }
    }
}

impl<T> Iterator for Receiver<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.recv().ok()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.chan.state.lock().unwrap().senders += 1;
        Sender { chan: self.chan.clone() }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.chan.state.lock().unwrap().receivers += 1;
        Receiver { chan: self.chan.clone() }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut s = self.chan.state.lock().unwrap();
        s.senders -= 1;
        if s.senders == 0 {
            self.chan.not_empty.notify_all(); // blocked receivers must see the disconnect
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut s = self.chan.state.lock().unwrap();
        s.receivers -= 1;
        if s.receivers == 0 {
            self.chan.not_full.notify_all(); // blocked senders must see the disconnect
        }
    }
}
"""

CHAN_STARTER = """
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub struct SendError<T>(pub T);

#[derive(Debug, PartialEq, Eq)]
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RecvError;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RecvTimeoutError {
    Timeout,
    Disconnected,
}

struct State<T> {
    queue: VecDeque<T>,
    cap: usize,
    senders: usize,
    receivers: usize,
}

struct Chan<T> {
    state: Mutex<State<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

pub struct Sender<T> {
    chan: Arc<Chan<T>>,
}

pub struct Receiver<T> {
    chan: Arc<Chan<T>>,
}

/// A bounded multi-producer multi-consumer channel (both ends are `Clone`). Panics if `cap` is 0.
pub fn channel<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    todo!()
}

impl<T> Sender<T> {
    /// Waits while the channel is full. `Err(SendError(value))` once every receiver is gone (also if that happens while waiting).
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        todo!()
    }
}

impl<T> Receiver<T> {
    /// Waits while empty. `Err` once it is empty and every sender is gone; values sent before that are still delivered.
    pub fn recv(&self) -> Result<T, RecvError> {
        todo!()
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        todo!()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        todo!()
    }
}

/// Receiving until disconnected: `for v in rx { .. }`.
impl<T> Iterator for Receiver<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        todo!()
    }
}

// TODO: `Clone` for both ends and `Drop` for both. The counts decide when the other side sees "disconnected",
// and a blocked thread has to be woken when the count reaches zero.
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}
"""

P.append(dict(
    slug="bounded-channel", title="A bounded MPMC channel with disconnect", level="hard", stage="build-it",
    tags=["channel", "Condvar", "disconnect", "backpressure", "Clone/Drop counting"],
    teaches=["Disconnect is a count reaching zero: senders gone means recv drains then fails; receivers gone means send fails.",
             "Waking blocked threads when the *last* handle drops (the part everyone forgets).",
             "Backpressure: a full bounded channel blocks senders until receivers catch up."],
    statement="""
        Build `channel(cap)`: a bounded queue split into a cloneable `Sender` and a cloneable `Receiver`, like a
        `std::sync::mpsc::sync_channel` that also allows several consumers.

        - `send` waits while full. `recv` waits while empty.
        - When **every** `Sender` is dropped, `recv` still delivers what was queued, then returns `Err(RecvError)`; a
          `recv` that is already blocked must wake up and fail. `try_recv` reports `Empty` vs `Disconnected`.
        - When **every** `Receiver` is dropped, `send` returns `Err(SendError(value))` (even one that was blocked on a
          full channel), and `try_send` returns `Disconnected(value)`; `Full(value)` if merely full.
        - `recv_timeout` waits at most the given time in total. `Receiver` is an `Iterator` that ends on disconnect.
    """,
    starter=CHAN_STARTER,
    solution=CHAN,
    visible=[
        T("send_recv_in_order", "send 1, 2, 3", "{ let (tx, rx) = channel(3); tx.send(1).unwrap(); tx.send(2).unwrap(); tx.send(3).unwrap(); (rx.recv(), rx.recv(), rx.recv()) }", "(Ok(1), Ok(2), Ok(3))"),
        T("try_recv_empty", "nothing sent", "{ let (_tx, rx) = channel::<u8>(2); rx.try_recv() }", "Err(TryRecvError::Empty)"),
        T("try_send_full", "capacity 1, second try_send", "{ let (tx, _rx) = channel(1); tx.try_send(1).unwrap(); tx.try_send(2) }", "Err(TrySendError::Full(2))"),
        T("recv_drains_then_disconnects", "send 1, 2, drop sender", "{ let (tx, rx) = channel(4); tx.send(1).unwrap(); tx.send(2).unwrap(); drop(tx); (rx.recv(), rx.recv(), rx.recv(), rx.try_recv()) }", "(Ok(1), Ok(2), Err(RecvError), Err(TryRecvError::Disconnected))"),
        T("send_after_receiver_gone", "drop receiver, then send", "{ let (tx, rx) = channel(2); drop(rx); (tx.send(5), tx.try_send(6)) }", "(Err(SendError(5)), Err(TrySendError::Disconnected(6)))"),
    ],
    hidden=[
        WITHIN + """
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        #[test]
        fn cloned_sender_keeps_the_channel_connected() {
            let (tx, rx) = channel(2);
            let tx2 = tx.clone();
            drop(tx);
            tx2.send(1).unwrap();
            check!("one of two senders dropped: still connected", rx.try_recv(), Ok(1));
            check!("and empty, not disconnected", rx.try_recv(), Err(TryRecvError::Empty));
            drop(tx2);
            check!("both dropped", rx.try_recv(), Err(TryRecvError::Disconnected));
        }

        #[test]
        fn cloned_receiver_keeps_senders_working() {
            let (tx, rx) = channel(2);
            let rx2 = rx.clone();
            drop(rx);
            check!("one of two receivers dropped", tx.send(1), Ok(()));
            check!("the other one gets it", rx2.recv(), Ok(1));
            drop(rx2);
            check!("both dropped", tx.send(2), Err(SendError(2)));
        }

        #[test]
        fn blocked_recv_wakes_when_the_last_sender_drops() {
            let (tx, rx) = channel::<u8>(2);
            let tx2 = tx.clone();
            let h = std::thread::spawn(move || rx.recv());
            std::thread::sleep(Duration::from_millis(60));
            drop(tx);
            std::thread::sleep(Duration::from_millis(60));
            drop(tx2);
            check!("recv blocked across the first drop, woken by the second", within(2000, move || h.join().unwrap()), Some(Err(RecvError)));
        }

        #[test]
        fn blocked_send_wakes_when_the_last_receiver_drops() {
            let (tx, rx) = channel(1);
            tx.send(1u8).unwrap();
            let h = std::thread::spawn(move || tx.send(2));
            std::thread::sleep(Duration::from_millis(60));
            drop(rx);
            check!("send blocked on a full channel gets its value back", within(2000, move || h.join().unwrap()), Some(Err(SendError(2))));
        }

        #[test]
        fn backpressure_is_exactly_the_capacity() {
            let (tx, rx) = channel(3);
            let sent = Arc::new(AtomicUsize::new(0));
            let s2 = sent.clone();
            let h = std::thread::spawn(move || {
                for i in 0..100u32 {
                    tx.send(i).unwrap();
                    s2.fetch_add(1, Ordering::SeqCst);
                }
            });
            std::thread::sleep(Duration::from_millis(100));
            check!("nobody receives: the sender is stuck after capacity sends", sent.load(Ordering::SeqCst), 3);
            check!("one recv frees one slot", rx.recv(), Ok(0));
            std::thread::sleep(Duration::from_millis(100));
            check!("exactly one more send went through", sent.load(Ordering::SeqCst), 4);
            let rest: Vec<u32> = rx.collect();
            h.join().unwrap();
            check!("the rest arrives in order", rest, (1..100u32).collect::<Vec<_>>());
        }

        #[test]
        fn recv_timeout_variants() {
            let (tx, rx) = channel::<u8>(1);
            let t = std::time::Instant::now();
            check!("nothing arrives", rx.recv_timeout(Duration::from_millis(80)), Err(RecvTimeoutError::Timeout));
            assert!(t.elapsed() >= Duration::from_millis(80));
            let tx2 = tx.clone();
            let h = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                tx2.send(9).unwrap();
            });
            check!("a value arrives in time", rx.recv_timeout(Duration::from_secs(3)), Ok(9));
            h.join().unwrap();
            drop(tx);
            check!("all senders gone", rx.recv_timeout(Duration::from_secs(3)), Err(RecvTimeoutError::Disconnected));
        }

        #[test]
        fn iterator_ends_on_disconnect() {
            let (tx, rx) = channel(2);
            let h = std::thread::spawn(move || {
                for i in 0..500u32 {
                    tx.send(i).unwrap();
                }
            });
            let got: Vec<u32> = rx.collect();
            h.join().unwrap();
            check!("500 values through capacity 2 then the loop ends", got, (0..500u32).collect::<Vec<_>>());
        }

        #[test]
        fn four_senders_three_receivers_exactly_once() {
            let (tx, rx) = channel(4);
            let senders: Vec<_> = (0..4u64)
                .map(|p| {
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        for i in 0..20_000u64 {
                            tx.send(p * 1_000_000 + i).unwrap();
                        }
                    })
                })
                .collect();
            drop(tx);
            let receivers: Vec<_> = (0..3).map(|_| { let rx = rx.clone(); std::thread::spawn(move || rx.collect::<Vec<u64>>()) }).collect();
            drop(rx);
            for s in senders {
                s.join().unwrap();
            }
            let mut all: Vec<u64> = within(10_000, move || receivers.into_iter().flat_map(|r| r.join().unwrap()).collect::<Vec<_>>()).expect("receivers never saw the disconnect");
            all.sort_unstable();
            let mut want: Vec<u64> = (0..4u64).flat_map(|p| (0..20_000u64).map(move |i| p * 1_000_000 + i)).collect();
            want.sort_unstable();
            check!("every value delivered exactly once", all == want, true);
        }

        #[test]
        fn queued_values_are_dropped_with_the_channel() {
            struct D(Arc<AtomicUsize>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let n = Arc::new(AtomicUsize::new(0));
            let (tx, rx) = channel(4);
            for _ in 0..3 {
                tx.send(D(n.clone())).ok().unwrap();
            }
            drop(rx.recv());
            drop(tx);
            drop(rx);
            check!("three sent, one received and dropped, two left in the channel", n.load(Ordering::SeqCst), 3);
        }

        #[test]
        fn zero_capacity_panics() {
            assert!(std::panic::catch_unwind(|| channel::<u8>(0)).is_err());
        }
        """,
    ],
    wrong=dict(
        last_sender_drop_does_not_wake=sub(CHAN, "if s.senders == 0 {\n            self.chan.not_empty.notify_all(); // blocked receivers must see the disconnect\n        }", ""),
        last_receiver_drop_does_not_wake=sub(CHAN, "if s.receivers == 0 {\n            self.chan.not_full.notify_all(); // blocked senders must see the disconnect\n        }", ""),
        clone_forgets_to_count=sub(CHAN, "self.chan.state.lock().unwrap().senders += 1;\n        Sender", "Sender"),
        capacity_ignored=sub(CHAN, "if s.queue.len() < s.cap {\n                s.queue.push_back(value);", "if true {\n                s.queue.push_back(value);"),
        disconnect_discards_queue=sub(CHAN, "            if let Some(v) = s.queue.pop_front() {\n                self.chan.not_full.notify_one();\n                return Ok(v);\n            }\n            if s.senders == 0 {\n                return Err(RecvError);\n            }\n            s = self.chan.not_empty.wait(s).unwrap();",
                                      "            if s.senders == 0 {\n                return Err(RecvError);\n            }\n            if let Some(v) = s.queue.pop_front() {\n                self.chan.not_full.notify_one();\n                return Ok(v);\n            }\n            s = self.chan.not_empty.wait(s).unwrap();"),
    ),
    hints=[("approach", "State under one mutex: the queue, `cap`, and two counts, `senders` and `receivers` (both start at 1). `Clone` increments, `Drop` decrements. `send` fails when `receivers == 0`; `recv` fails when the queue is empty **and** `senders == 0`."),
           ("rust", "Two condvars as in the blocking queue. In `Drop`, when a count reaches zero call `notify_all` on the condvar the *other* side waits on, otherwise blocked threads sleep forever. Check the queue before the sender count in `recv` so queued values are still delivered after disconnect."),
           ("edge", "`send` must re-check `receivers == 0` on every loop iteration: the last receiver can drop while the sender is asleep on `not_full`.")],
    notes=("This is what `std::sync::mpsc::sync_channel` does, plus multiple consumers. 'Disconnected' is not a message but a count; both `Clone` and `Drop` have to maintain it, and the last drop must wake the other side because nobody else will. Delivering queued values after the senders are gone is the rule that makes `for v in rx` a complete drain.", "O(1) per operation", "O(capacity)"),
    follow_up="1) Add `capacity 0` (a rendezvous channel): who must wait for whom, and what does `try_send` mean there? 2) Add `select` over two receivers (`crossbeam::select!`): why is it hard with a condvar per channel, and what do real implementations register instead (waker lists)? 3) Add `send_timeout` and a `Sender::is_disconnected()`: which is racy by nature and why? 4) Compare backpressure policies: block, drop newest, drop oldest, error. Which does a metrics pipeline want, and which a payment pipeline? 5) How do you shut down a pipeline of three stages connected by these channels without losing items?",
    related=["C1", "S5"],
))

STAGES = [("use-it", "Use it", "easy"), ("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    stages = [("build-it", "Build it", "hard")]
    n = write_track("c2-message-passing", "C2", "Message passing", "C", "core", 2,
                    "Thread pools and channels by building them: workers, results, graceful shutdown, bounded queues and disconnection.",
                    stages, P)
    print("C2", n)
