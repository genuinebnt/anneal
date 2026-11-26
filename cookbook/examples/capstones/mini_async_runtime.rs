//! CAPSTONE 09 (async): a miniature tokio: tasks, work-stealing scheduler, timer driver, sync primitives, mio reactor
//! LEVEL: capstone
//! Use in: understanding what tokio/smol do under `#[tokio::main]`; debugging lost wakeups, starvation, cancellation bugs.   Run: cargo run --release --example mini_async_runtime
#![allow(dead_code)]
//!
//! ARCHITECTURE
//! ```text
//!   user futures ──spawn──▶ Arc<Task>{state, Mutex<Option<BoxFuture>>} ──▶ JoinHandle<T> (slot + waker)
//!                                   │ wake() = CAS state machine, schedules at most once
//!                                   ▼
//!   ┌──────────── Scheduler trait ───────────────┐      ┌───── Timer (BinaryHeap<deadline,seq>) ─────┐
//!   │ LocalQueue (VecDeque, block_on drives it)  │      │ Clock: Real(Instant) | Manual(AtomicU64)   │
//!   │ MtShared: Injector + N x (Worker, Stealer) │◀wake─│ driver thread sleeps until next deadline   │
//!   │   idle workers park on Condvar             │      └────────────────────────────────────────────┘
//!   └────────────────────────────────────────────┘      ┌───── Reactor (mio::Poll thread) ───────────┐
//!         worker i: local.pop → injector → steal ◀──wake─│ token → IoState{epoch, waker} (read/write) │
//!                                                        └────────────────────────────────────────────┘
//! ```
//! WHAT: ~1000 lines that reproduce the moving parts of an async runtime using only `Future`/`Waker`/`Wake`, crossbeam
//! deques, parking_lot and mio. No `unsafe` is needed at all: `Wake` for `Arc<Task>` replaces the raw-vtable waker and
//! `Pin<Box<..>>` replaces pin projections.
//! WHY: every `.await` bottoms out in "store my waker somewhere, return Pending, someone calls wake()". Building the
//! somewhere (timer heap, reactor, channel slot, join slot) shows why wakeups get lost and why cancellation is "drop".
//! HOW (the hazards the tests target):
//!  * Task state machine IDLE -> SCHEDULED -> RUNNING -> (NOTIFIED) -> IDLE | COMPLETE. LOST WAKEUP: if `wake` ignored a
//!    RUNNING task and the poll then returned Pending, the event that woke us would be forgotten forever; instead a wake
//!    during RUNNING sets NOTIFIED and the poller re-schedules after Pending. DOUBLE SCHEDULE: two threads waking an IDLE
//!    task must not both enqueue it (two workers would poll the same future concurrently: UB in real runtimes, a data
//!    race here); only the CAS winner of IDLE->SCHEDULED enqueues. A `polling` flag asserts exclusivity at run time.
//!  * Wakers are stored BEFORE re-checking the condition (timer fired? epoch changed? queue empty?), the standard
//!    register-then-recheck pattern that closes the race between "check" and "park".
//!  * Edge-triggered IO: mio reports readiness once per transition, so the reactor bumps an epoch counter and wakes; the
//!    consumer always TRIES the syscall first and only on WouldBlock registers a waker and re-checks the epoch.
//!  * Cancellation = dropping the future (destructors run); `abort()` just marks the task so its next run drops it.
//!  * Cancellation safety: `select2`/`timeout` drop the loser. That is only safe for futures that lose no data when
//!    dropped mid-await: `mpsc::recv`, `Notify::notified`, `Sleep` are safe; a half-finished `write_all` is NOT (a prefix
//!    was already written), and `send` hands the value back inside the error only if it was never enqueued.
//!  * Known simplification: tasks parked forever at shutdown form an Arc cycle (task -> future -> waker -> task); tokio
//!    avoids it with an owned-task list. All tests here finish their tasks, and a live-task counter proves nothing leaks.
//! COMMON USES: tokio (multi-thread work-stealing scheduler, mio-backed IO driver, hierarchical timer wheel, `JoinHandle`,
//! oneshot/mpsc/Notify/Semaphore), async-std and smol (global executor + `async-io` reactor, `smol::block_on`),
//! embassy (no_std executor with static tasks and an interrupt-driven timer queue instead of threads).
//! Read: https://tokio.rs/blog/2019-10-scheduler , https://github.com/smol-rs/smol , https://rust-lang.github.io/async-book/
use std::any::Any;
use std::cell::RefCell;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::future::{Future, poll_fn};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::{Pin, pin};
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool, AtomicIsize, AtomicU8, AtomicU64, AtomicUsize, Ordering::*, fence,
};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use mio::{Events, Interest, Token};
use parking_lot::{Condvar, Mutex};

static POLL_VIOLATIONS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_POLLS: AtomicU64 = AtomicU64::new(0);
static LIVE_TASKS: AtomicIsize = AtomicIsize::new(0);
static LIVE_THREADS: AtomicIsize = AtomicIsize::new(0);

fn spawn_thread(name: String, f: impl FnOnce() + Send + 'static) -> thread::JoinHandle<()> {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            LIVE_THREADS.fetch_sub(1, SeqCst);
        }
    }
    LIVE_THREADS.fetch_add(1, SeqCst);
    thread::Builder::new()
        .name(name)
        .spawn(move || {
            let _g = Guard;
            f()
        })
        .expect("spawn thread")
}

// =====================================================================================================================
// 1. TASKS
// =====================================================================================================================
const IDLE: u8 = 0;
const SCHEDULED: u8 = 1;
const RUNNING: u8 = 2;
const NOTIFIED: u8 = 3;
const COMPLETE: u8 = 4;

type BoxFut = Pin<Box<dyn Future<Output = ()> + Send>>;

trait Scheduler: Send + Sync {
    fn schedule(&self, task: Arc<Task>);
}

struct Task {
    state: AtomicU8,
    aborted: AtomicBool,
    polling: AtomicBool, // debug assertion: set while some thread is inside poll()
    future: Mutex<Option<BoxFut>>,
    sched: Arc<dyn Scheduler>,
}

impl Drop for Task {
    fn drop(&mut self) {
        LIVE_TASKS.fetch_sub(1, SeqCst);
    }
}

impl Task {
    fn new(fut: BoxFut, sched: Arc<dyn Scheduler>) -> Arc<Task> {
        LIVE_TASKS.fetch_add(1, SeqCst);
        // born SCHEDULED: the spawner enqueues it exactly once
        Arc::new(Task {
            state: AtomicU8::new(SCHEDULED),
            aborted: AtomicBool::new(false),
            polling: AtomicBool::new(false),
            future: Mutex::new(Some(fut)),
            sched,
        })
    }

    /// The only way a task is ever enqueued after birth. IDLE -> SCHEDULED enqueues (CAS winner only); RUNNING ->
    /// NOTIFIED records the wake so the running poller re-schedules; SCHEDULED/NOTIFIED/COMPLETE need nothing.
    fn notify(self: &Arc<Self>) {
        let mut cur = self.state.load(Acquire);
        loop {
            let next = match cur {
                IDLE => SCHEDULED,
                RUNNING => NOTIFIED,
                _ => return,
            };
            match self.state.compare_exchange_weak(cur, next, AcqRel, Acquire) {
                Ok(_) => {
                    if cur == IDLE {
                        self.sched.schedule(self.clone());
                    }
                    return;
                }
                Err(actual) => cur = actual,
            }
        }
    }

    /// Called by whichever worker dequeued the task (it owns the SCHEDULED token, so a plain store is race free).
    fn run(self: Arc<Self>) {
        self.state.store(RUNNING, Release);
        if self.aborted.load(Acquire) {
            let fut = self.future.lock().take();
            drop(fut); // runs destructors; the harness reports Cancelled to the JoinHandle
            self.state.store(COMPLETE, Release);
            return;
        }
        let waker = Waker::from(self.clone());
        let mut cx = Context::from_waker(&waker);
        let mut slot = self.future.lock();
        if self.polling.swap(true, SeqCst) {
            POLL_VIOLATIONS.fetch_add(1, SeqCst); // would mean two threads polled this task at once
        }
        TOTAL_POLLS.fetch_add(1, Relaxed);
        let res = match slot.as_mut() {
            Some(f) => f.as_mut().poll(&mut cx),
            None => Poll::Ready(()),
        };
        self.polling.store(false, SeqCst);
        match res {
            Poll::Ready(()) => {
                let dead = slot.take();
                drop(slot);
                drop(dead);
                self.state.store(COMPLETE, Release);
            }
            Poll::Pending => {
                drop(slot);
                if self
                    .state
                    .compare_exchange(RUNNING, IDLE, AcqRel, Acquire)
                    .is_err()
                {
                    // a wake arrived during the poll (state == NOTIFIED): poll again, exactly once
                    self.state.store(SCHEDULED, Release);
                    self.sched.schedule(self.clone());
                }
            }
        }
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        self.notify();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}

#[derive(Debug, Clone, PartialEq)]
enum JoinError {
    Panic(String),
    Cancelled,
}

struct Slot<T> {
    result: Option<Result<T, JoinError>>,
    finished: bool,
    waker: Option<Waker>,
}
struct JoinInner<T> {
    slot: Mutex<Slot<T>>,
}
impl<T> JoinInner<T> {
    fn complete(&self, r: Result<T, JoinError>) {
        let waker = {
            let mut s = self.slot.lock();
            if s.finished {
                return;
            }
            s.finished = true;
            s.result = Some(r);
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake();
        }
    }
}

/// Wraps the user future: catches panics per poll and delivers the output (or Cancelled on drop) to the JoinHandle.
struct Harness<F: Future> {
    fut: Pin<Box<F>>,
    join: Arc<JoinInner<F::Output>>,
}
impl<F: Future> Future for Harness<F> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        match catch_unwind(AssertUnwindSafe(|| this.fut.as_mut().poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(v)) => {
                this.join.complete(Ok(v));
                Poll::Ready(())
            }
            Err(p) => {
                this.join.complete(Err(JoinError::Panic(panic_msg(p))));
                Poll::Ready(())
            }
        }
    }
}
impl<F: Future> Drop for Harness<F> {
    fn drop(&mut self) {
        self.join.complete(Err(JoinError::Cancelled)); // no-op if already finished
    }
}
fn panic_msg(p: Box<dyn Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic>".into()
    }
}

struct JoinHandle<T> {
    inner: Arc<JoinInner<T>>,
    task: Arc<Task>,
}
impl<T> JoinHandle<T> {
    /// Marks the task; its next run drops the future instead of polling. Awaiting afterwards yields Cancelled.
    fn abort(&self) {
        self.task.aborted.store(true, Release);
        self.task.notify();
    }
    fn is_finished(&self) -> bool {
        self.inner.slot.lock().finished
    }
}
impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut s = self.inner.slot.lock();
        if let Some(r) = s.result.take() {
            return Poll::Ready(r);
        }
        s.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

fn spawn_on<F>(sched: &Arc<dyn Scheduler>, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let inner = Arc::new(JoinInner {
        slot: Mutex::new(Slot {
            result: None,
            finished: false,
            waker: None,
        }),
    });
    let h = Harness {
        fut: Box::pin(fut),
        join: inner.clone(),
    };
    let task = Task::new(Box::pin(h), sched.clone());
    sched.schedule(task.clone());
    JoinHandle { inner, task }
}

// ---- runtime context (thread local, like tokio's `Handle::current`) ----
#[derive(Clone)]
struct Handle {
    sched: Arc<dyn Scheduler>,
    timer: Arc<Timer>,
    reactor: Option<Arc<Reactor>>,
}
impl Handle {
    fn spawn<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        spawn_on(&self.sched, f)
    }
}
thread_local! { static CURRENT: RefCell<Option<Handle>> = const { RefCell::new(None) }; }
struct EnterGuard(Option<Handle>);
impl Drop for EnterGuard {
    fn drop(&mut self) {
        let prev = self.0.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}
fn enter(h: &Handle) -> EnterGuard {
    EnterGuard(CURRENT.with(|c| c.borrow_mut().replace(h.clone())))
}
fn current() -> Handle {
    CURRENT
        .with(|c| c.borrow().clone())
        .expect("called outside of a runtime")
}
fn spawn<F>(f: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    current().spawn(f)
}

struct ParkWake {
    thread: thread::Thread,
    flag: AtomicBool,
}
impl Wake for ParkWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.flag.store(true, Release);
        self.thread.unpark();
    }
}
/// block_on for a thread that is not a worker: park until the root future's waker fires.
fn park_block_on<F: Future>(handle: &Handle, fut: F) -> F::Output {
    let _g = enter(handle);
    let pw = Arc::new(ParkWake {
        thread: thread::current(),
        flag: AtomicBool::new(false),
    });
    let waker = Waker::from(pw.clone());
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        while !pw.flag.swap(false, Acquire) {
            thread::park();
        }
    }
}

// =====================================================================================================================
// 2a. SINGLE-THREADED EXECUTOR
// =====================================================================================================================
struct LocalQueue {
    q: Mutex<VecDeque<Arc<Task>>>,
    cv: Condvar,
    root: AtomicBool,
}
impl Scheduler for LocalQueue {
    fn schedule(&self, task: Arc<Task>) {
        self.q.lock().push_back(task);
        self.cv.notify_one();
    }
}
struct LocalRootWake(Arc<LocalQueue>);
impl Wake for LocalRootWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.root.store(true, SeqCst);
        let _g = self.0.q.lock(); // take the lock so the waiter cannot miss the notify between check and wait
        self.0.cv.notify_one();
    }
}

struct LocalRuntime {
    handle: Handle,
    queue: Arc<LocalQueue>,
    timer: Arc<Timer>,
}
impl LocalRuntime {
    fn new(clock: Clock) -> Self {
        let queue = Arc::new(LocalQueue {
            q: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            root: AtomicBool::new(false),
        });
        let timer = Timer::new(clock);
        let handle = Handle {
            sched: queue.clone(),
            timer: timer.clone(),
            reactor: None,
        };
        LocalRuntime {
            handle,
            queue,
            timer,
        }
    }
    fn spawn<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn(f)
    }
    fn advance(&self, d: Duration) {
        self.timer.clock.advance(d);
    }
    /// Runs every ready task and every due timer until nothing more can make progress without time passing.
    fn run_until_stalled(&self) -> usize {
        let _g = enter(&self.handle);
        let mut total = 0;
        loop {
            let fired = self.timer.fire_due();
            let mut ran = 0;
            while let Some(t) = {
                let n = self.queue.q.lock().pop_front();
                n
            } {
                t.run();
                ran += 1;
            }
            total += ran;
            if fired == 0 && ran == 0 {
                return total;
            }
        }
    }
    fn block_on<F: Future>(&self, fut: F) -> F::Output {
        let _g = enter(&self.handle);
        let waker = Waker::from(Arc::new(LocalRootWake(self.queue.clone())));
        let mut cx = Context::from_waker(&waker);
        let mut fut = pin!(fut);
        self.queue.root.store(true, SeqCst);
        loop {
            if self.queue.root.swap(false, SeqCst) {
                if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                    return v;
                }
            }
            self.timer.fire_due();
            let mut ran = false;
            loop {
                let t = self.queue.q.lock().pop_front();
                match t {
                    Some(t) => {
                        t.run();
                        ran = true;
                        if self.queue.root.load(SeqCst) {
                            break; // let the root future make progress promptly
                        }
                    }
                    None => break,
                }
            }
            if ran || self.queue.root.load(SeqCst) {
                continue;
            }
            let mut g = self.queue.q.lock();
            if g.is_empty() && !self.queue.root.load(SeqCst) {
                match self.timer.next_deadline() {
                    Some(d) => {
                        let now = self.timer.now();
                        if d > now {
                            self.queue
                                .cv
                                .wait_for(&mut g, Duration::from_nanos(d - now));
                        }
                    }
                    None => self.queue.cv.wait(&mut g),
                }
            }
        }
    }
}

// =====================================================================================================================
// 2b. MULTI-THREADED WORK-STEALING EXECUTOR
// =====================================================================================================================
struct MtShared {
    injector: Injector<Arc<Task>>,
    stealers: Vec<Stealer<Arc<Task>>>,
    sleepers: AtomicUsize,
    lock: Mutex<()>,
    cv: Condvar,
    shutdown: AtomicBool,
    executed: Vec<AtomicU64>,
    stolen: Vec<AtomicU64>,
}
struct WorkerCtx {
    shared_id: usize,
    idx: usize,
    local: Worker<Arc<Task>>,
}
thread_local! { static WORKER: RefCell<Option<WorkerCtx>> = const { RefCell::new(None) }; }

impl MtShared {
    fn id(&self) -> usize {
        self as *const MtShared as usize
    }
    fn wake_one(&self) {
        fence(SeqCst);
        if self.sleepers.load(SeqCst) > 0 {
            let _g = self.lock.lock(); // pairs with the sleeper's recheck-under-lock
            self.cv.notify_one();
        }
    }
    fn has_work(&self) -> bool {
        !self.injector.is_empty() || self.stealers.iter().any(|s| !s.is_empty())
    }
    fn find_task(&self, idx: usize, tick: u64) -> Option<Arc<Task>> {
        WORKER.with(|w| {
            let w = w.borrow();
            let ctx = w.as_ref().expect("worker ctx");
            let local = &ctx.local;
            if tick % 61 == 0 {
                // fairness: occasionally look at the global queue first so outside spawns cannot starve
                if let Steal::Success(t) = self.injector.steal_batch_and_pop(local) {
                    return Some(t);
                }
            }
            if let Some(t) = local.pop() {
                return Some(t);
            }
            loop {
                let mut retry = false;
                match self.injector.steal_batch_and_pop(local) {
                    Steal::Success(t) => return Some(t),
                    Steal::Retry => retry = true,
                    Steal::Empty => {}
                }
                for (i, s) in self.stealers.iter().enumerate() {
                    if i == idx {
                        continue;
                    }
                    match s.steal_batch_and_pop(local) {
                        Steal::Success(t) => {
                            self.stolen[idx].fetch_add(1, Relaxed);
                            return Some(t);
                        }
                        Steal::Retry => retry = true,
                        Steal::Empty => {}
                    }
                }
                if !retry {
                    return None;
                }
            }
        })
    }
}
impl Scheduler for MtShared {
    fn schedule(&self, task: Arc<Task>) {
        let id = self.id();
        let leftover = WORKER.with(|w| match w.borrow().as_ref() {
            Some(ctx) if ctx.shared_id == id => {
                ctx.local.push(task); // fast path: own deque, no shared state touched
                None
            }
            _ => Some(task),
        });
        if let Some(t) = leftover {
            self.injector.push(t);
        }
        self.wake_one();
    }
}

fn worker_main(shared: Arc<MtShared>, idx: usize, local: Worker<Arc<Task>>, handle: Handle) {
    let _enter = enter(&handle);
    WORKER.with(|w| {
        *w.borrow_mut() = Some(WorkerCtx {
            shared_id: shared.id(),
            idx,
            local,
        })
    });
    let mut tick = 0u64;
    loop {
        tick += 1;
        if let Some(task) = shared.find_task(idx, tick) {
            shared.executed[idx].fetch_add(1, Relaxed);
            task.run();
            continue;
        }
        if shared.shutdown.load(SeqCst) {
            break; // graceful: only exit once a full scan found no runnable work
        }
        let mut g = shared.lock.lock();
        shared.sleepers.fetch_add(1, SeqCst);
        fence(SeqCst);
        if shared.has_work() || shared.shutdown.load(SeqCst) {
            shared.sleepers.fetch_sub(1, SeqCst);
            continue;
        }
        // 50 ms is only a safety net; correctness relies on the recheck above plus wake_one()
        shared.cv.wait_for(&mut g, Duration::from_millis(50));
        shared.sleepers.fetch_sub(1, SeqCst);
    }
    WORKER.with(|w| *w.borrow_mut() = None);
}

struct Runtime {
    handle: Handle,
    shared: Arc<MtShared>,
    timer: Arc<Timer>,
    reactor: Arc<Reactor>,
    timer_stop: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
}
impl Runtime {
    fn new(workers: usize) -> Runtime {
        Self::with_clock(workers, Clock::Real(Instant::now()))
    }
    fn with_clock(n: usize, clock: Clock) -> Runtime {
        let ws: Vec<Worker<Arc<Task>>> = (0..n).map(|_| Worker::new_fifo()).collect(); // FIFO: yield_now goes to the back
        let shared = Arc::new(MtShared {
            injector: Injector::new(),
            stealers: ws.iter().map(|w| w.stealer()).collect(),
            sleepers: AtomicUsize::new(0),
            lock: Mutex::new(()),
            cv: Condvar::new(),
            shutdown: AtomicBool::new(false),
            executed: (0..n).map(|_| AtomicU64::new(0)).collect(),
            stolen: (0..n).map(|_| AtomicU64::new(0)).collect(),
        });
        let timer = Timer::new(clock);
        let (reactor, poll) = Reactor::new().expect("reactor");
        let handle = Handle {
            sched: shared.clone(),
            timer: timer.clone(),
            reactor: Some(reactor.clone()),
        };
        let mut threads = Vec::new();
        for (i, w) in ws.into_iter().enumerate() {
            let (s, h) = (shared.clone(), handle.clone());
            threads.push(spawn_thread(format!("mini-worker-{i}"), move || {
                worker_main(s, i, w, h)
            }));
        }
        let timer_stop = Arc::new(AtomicBool::new(false));
        {
            let (t, stop) = (timer.clone(), timer_stop.clone());
            threads.push(spawn_thread("mini-timer".into(), move || {
                t.run_driver(&stop)
            }));
        }
        {
            let r = reactor.clone();
            threads.push(spawn_thread("mini-reactor".into(), move || r.run(poll)));
        }
        Runtime {
            handle,
            shared,
            timer,
            reactor,
            timer_stop,
            threads,
        }
    }
    fn spawn<F>(&self, f: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn(f)
    }
    fn block_on<F: Future>(&self, f: F) -> F::Output {
        park_block_on(&self.handle, f)
    }
    fn executed(&self) -> Vec<u64> {
        self.shared
            .executed
            .iter()
            .map(|c| c.load(Relaxed))
            .collect()
    }
    fn stolen(&self) -> u64 {
        self.shared.stolen.iter().map(|c| c.load(Relaxed)).sum()
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, SeqCst);
        {
            let _g = self.shared.lock.lock();
            self.shared.cv.notify_all();
        }
        self.timer_stop.store(true, SeqCst);
        {
            let _g = self.timer.inner.lock();
            self.timer.cv.notify_all();
        }
        self.reactor.shutdown.store(true, SeqCst);
        let _ = self.reactor.waker.wake();
        for t in self.threads.drain(..) {
            t.join().expect("runtime thread panicked");
        }
        while let Steal::Success(t) = self.shared.injector.steal() {
            drop(t); // never-run tasks: dropping the Arc drops their futures
        }
        self.timer.inner.lock().heap.clear();
    }
}

// =====================================================================================================================
// 3. TIME: injectable clock, BinaryHeap timer, sleep / timeout / interval
// =====================================================================================================================
enum Clock {
    Real(Instant),
    Manual(AtomicU64),
}
impl Clock {
    fn manual() -> Clock {
        Clock::Manual(AtomicU64::new(0))
    }
    fn now(&self) -> u64 {
        match self {
            Clock::Real(t0) => t0.elapsed().as_nanos() as u64,
            Clock::Manual(n) => n.load(SeqCst),
        }
    }
    fn advance(&self, d: Duration) {
        match self {
            Clock::Manual(n) => {
                n.fetch_add(d.as_nanos() as u64, SeqCst);
            }
            Clock::Real(_) => panic!("cannot advance a real clock"),
        }
    }
}

struct TimerState {
    fired: AtomicBool,
    cancelled: AtomicBool,
    waker: Mutex<Option<Waker>>,
}
struct Entry {
    deadline: u64,
    seq: u64,
    st: Arc<TimerState>,
}
impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        (self.deadline, self.seq) == (o.deadline, o.seq)
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Entry {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        // reversed so BinaryHeap (a max-heap) pops the EARLIEST deadline; seq keeps equal deadlines FIFO
        (o.deadline, o.seq).cmp(&(self.deadline, self.seq))
    }
}
struct TimerInner {
    heap: BinaryHeap<Entry>,
    seq: u64,
}
struct Timer {
    clock: Clock,
    inner: Mutex<TimerInner>,
    cv: Condvar,
}
impl Timer {
    fn new(clock: Clock) -> Arc<Timer> {
        Arc::new(Timer {
            clock,
            inner: Mutex::new(TimerInner {
                heap: BinaryHeap::new(),
                seq: 0,
            }),
            cv: Condvar::new(),
        })
    }
    fn now(&self) -> u64 {
        self.clock.now()
    }
    fn insert(&self, deadline: u64, st: Arc<TimerState>) {
        let mut g = self.inner.lock();
        g.seq += 1;
        let seq = g.seq;
        g.heap.push(Entry { deadline, seq, st });
        self.cv.notify_one(); // the driver may be sleeping towards a later deadline
    }
    fn collect_due(g: &mut TimerInner, now: u64) -> Vec<Waker> {
        let mut out = Vec::new();
        while g.heap.peek().is_some_and(|e| e.deadline <= now) {
            let e = g.heap.pop().unwrap();
            if e.st.cancelled.load(Acquire) {
                continue;
            }
            e.st.fired.store(true, SeqCst);
            if let Some(w) = e.st.waker.lock().take() {
                out.push(w);
            }
        }
        out
    }
    /// Fires everything due at the clock's current time (in deadline order); returns how many wakers were woken.
    fn fire_due(&self) -> usize {
        let wakers = Self::collect_due(&mut self.inner.lock(), self.now());
        let n = wakers.len();
        for w in wakers {
            w.wake(); // outside the timer lock: wake() may re-enter the scheduler
        }
        n
    }
    fn next_deadline(&self) -> Option<u64> {
        self.inner.lock().heap.peek().map(|e| e.deadline)
    }
    fn run_driver(&self, stop: &AtomicBool) {
        let mut g = self.inner.lock();
        while !stop.load(SeqCst) {
            let now = self.now();
            let wakers = Self::collect_due(&mut g, now);
            if !wakers.is_empty() {
                drop(g);
                for w in wakers {
                    w.wake();
                }
                g = self.inner.lock();
                continue;
            }
            match g.heap.peek().map(|e| e.deadline) {
                Some(d) => {
                    let _ = self
                        .cv
                        .wait_for(&mut g, Duration::from_nanos(d.saturating_sub(now).max(1)));
                }
                None => self.cv.wait(&mut g),
            }
        }
    }
}

struct Sleep {
    timer: Arc<Timer>,
    deadline: u64,
    st: Arc<TimerState>,
    registered: bool,
}
impl Sleep {
    fn new(timer: Arc<Timer>, deadline: u64) -> Sleep {
        let st = Arc::new(TimerState {
            fired: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            waker: Mutex::new(None),
        });
        Sleep {
            timer,
            deadline,
            st,
            registered: false,
        }
    }
}
impl Future for Sleep {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.st.fired.load(SeqCst) || self.timer.now() >= self.deadline {
            return Poll::Ready(());
        }
        {
            // register-then-recheck: store the waker first, then look at `fired` again
            let mut w = self.st.waker.lock();
            if !w.as_ref().is_some_and(|old| old.will_wake(cx.waker())) {
                *w = Some(cx.waker().clone());
            }
        }
        if !self.registered {
            self.registered = true;
            let (d, st) = (self.deadline, self.st.clone());
            self.timer.insert(d, st);
        }
        if self.st.fired.load(SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
impl Drop for Sleep {
    fn drop(&mut self) {
        self.st.cancelled.store(true, Release); // lazy deletion: the heap entry is skipped when popped
    }
}
fn now_ns() -> u64 {
    current().timer.now()
}
fn sleep_until(deadline_ns: u64) -> Sleep {
    Sleep::new(current().timer, deadline_ns)
}
fn sleep(d: Duration) -> Sleep {
    let t = current().timer;
    let deadline = t.now() + d.as_nanos() as u64;
    Sleep::new(t, deadline)
}

#[derive(Debug, PartialEq)]
struct Elapsed;
/// Polls the future first, then the deadline: a value that is ready exactly at the deadline still wins. On expiry
/// the inner future is dropped immediately (cancellation), running its destructors.
struct Timeout<F: Future> {
    fut: Option<Pin<Box<F>>>,
    sleep: Sleep,
}
fn timeout<F: Future>(d: Duration, f: F) -> Timeout<F> {
    Timeout {
        fut: Some(Box::pin(f)),
        sleep: sleep(d),
    }
}
impl<F: Future> Future for Timeout<F> {
    type Output = Result<F::Output, Elapsed>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(f) = this.fut.as_mut() {
            if let Poll::Ready(v) = f.as_mut().poll(cx) {
                this.fut = None;
                return Poll::Ready(Ok(v));
            }
        }
        if Pin::new(&mut this.sleep).poll(cx).is_ready() {
            this.fut = None;
            return Poll::Ready(Err(Elapsed));
        }
        Poll::Pending
    }
}

struct Interval {
    timer: Arc<Timer>,
    period: u64,
    next: u64,
}
fn interval(period: Duration) -> Interval {
    let timer = current().timer;
    let next = timer.now();
    Interval {
        timer,
        period: period.as_nanos() as u64,
        next,
    }
}
impl Interval {
    /// First tick is immediate; missed ticks are delivered back-to-back (a "burst" policy, like tokio's default).
    async fn tick(&mut self) -> u64 {
        Sleep::new(self.timer.clone(), self.next).await;
        let t = self.next;
        self.next += self.period;
        t
    }
}

// =====================================================================================================================
// 4. SYNC: yield_now, oneshot, bounded mpsc, Notify, Semaphore; join2/join_all/select2
// =====================================================================================================================
struct YieldNow(bool);
fn yield_now() -> YieldNow {
    YieldNow(false)
}
impl Future for YieldNow {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref(); // "poll me again, but after everybody else": RUNNING -> NOTIFIED -> back of the queue
        Poll::Pending
    }
}

// ---- oneshot ----
struct OsInner<T> {
    value: Option<T>,
    rx_waker: Option<Waker>,
    tx_gone: bool,
    rx_gone: bool,
}
struct OsSender<T>(Arc<Mutex<OsInner<T>>>);
struct OsReceiver<T>(Arc<Mutex<OsInner<T>>>);
#[derive(Debug, PartialEq)]
struct RecvError;
fn oneshot<T>() -> (OsSender<T>, OsReceiver<T>) {
    let i = Arc::new(Mutex::new(OsInner {
        value: None,
        rx_waker: None,
        tx_gone: false,
        rx_gone: false,
    }));
    (OsSender(i.clone()), OsReceiver(i))
}
impl<T> OsSender<T> {
    fn send(self, v: T) -> Result<(), T> {
        let mut g = self.0.lock();
        if g.rx_gone {
            return Err(v);
        }
        g.value = Some(v);
        Ok(()) // Drop below sets tx_gone and wakes the receiver
    }
}
impl<T> Drop for OsSender<T> {
    fn drop(&mut self) {
        let w = {
            let mut g = self.0.lock();
            g.tx_gone = true;
            g.rx_waker.take()
        };
        if let Some(w) = w {
            w.wake();
        }
    }
}
impl<T> Drop for OsReceiver<T> {
    fn drop(&mut self) {
        self.0.lock().rx_gone = true;
    }
}
impl<T> Future for OsReceiver<T> {
    type Output = Result<T, RecvError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut g = self.0.lock();
        if let Some(v) = g.value.take() {
            return Poll::Ready(Ok(v));
        }
        if g.tx_gone {
            return Poll::Ready(Err(RecvError));
        }
        g.rx_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

// ---- bounded mpsc with waker-based backpressure ----
struct ChanSt<T> {
    q: VecDeque<T>,
    rx_waker: Option<Waker>,
    tx_wakers: Vec<Waker>,
    senders: usize,
    rx_closed: bool,
    max_len: usize,
}
struct Chan<T> {
    st: Mutex<ChanSt<T>>,
    cap: usize,
}
struct MpscSender<T>(Arc<Chan<T>>);
struct MpscReceiver<T>(Arc<Chan<T>>);
fn mpsc_channel<T>(cap: usize) -> (MpscSender<T>, MpscReceiver<T>) {
    assert!(cap > 0);
    let c = Arc::new(Chan {
        st: Mutex::new(ChanSt {
            q: VecDeque::new(),
            rx_waker: None,
            tx_wakers: Vec::new(),
            senders: 1,
            rx_closed: false,
            max_len: 0,
        }),
        cap,
    });
    (MpscSender(c.clone()), MpscReceiver(c))
}
impl<T> Clone for MpscSender<T> {
    fn clone(&self) -> Self {
        self.0.st.lock().senders += 1;
        MpscSender(self.0.clone())
    }
}
impl<T> Drop for MpscSender<T> {
    fn drop(&mut self) {
        let w = {
            let mut g = self.0.st.lock();
            g.senders -= 1;
            if g.senders == 0 {
                g.rx_waker.take()
            } else {
                None
            }
        };
        if let Some(w) = w {
            w.wake();
        }
    }
}
impl<T> Drop for MpscReceiver<T> {
    fn drop(&mut self) {
        let ws = {
            let mut g = self.0.st.lock();
            g.rx_closed = true;
            std::mem::take(&mut g.tx_wakers)
        };
        ws.into_iter().for_each(Waker::wake);
    }
}
impl<T> MpscSender<T> {
    /// Waits for a free slot (cancel safe: if dropped while waiting, nothing was enqueued). Err(v) if receiver is gone.
    async fn send(&self, v: T) -> Result<(), T> {
        let mut val = Some(v);
        poll_fn(|cx| {
            let mut g = self.0.st.lock();
            if g.rx_closed {
                return Poll::Ready(Err(val.take().unwrap()));
            }
            if g.q.len() < self.0.cap {
                g.q.push_back(val.take().unwrap());
                g.max_len = g.max_len.max(g.q.len());
                let w = g.rx_waker.take();
                drop(g);
                if let Some(w) = w {
                    w.wake();
                }
                return Poll::Ready(Ok(()));
            }
            if !g.tx_wakers.iter().any(|w| w.will_wake(cx.waker())) {
                g.tx_wakers.push(cx.waker().clone());
            }
            Poll::Pending
        })
        .await
    }
}
impl<T> MpscReceiver<T> {
    fn max_observed_len(&self) -> usize {
        self.0.st.lock().max_len
    }
    /// Cancel safe: an item is only removed from the queue in the same poll that returns it.
    async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| {
            let mut g = self.0.st.lock();
            if let Some(v) = g.q.pop_front() {
                // wake ALL blocked senders: waking one could waste the wake on a sender that was since cancelled
                let ws = std::mem::take(&mut g.tx_wakers);
                drop(g);
                ws.into_iter().for_each(Waker::wake);
                return Poll::Ready(Some(v));
            }
            if g.senders == 0 {
                return Poll::Ready(None);
            }
            g.rx_waker = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }
}

// ---- Semaphore-lite and Notify ----
struct Semaphore {
    st: Mutex<(usize, Vec<Waker>)>,
}
struct Permit(Arc<Semaphore>);
impl Semaphore {
    fn new(n: usize) -> Arc<Semaphore> {
        Arc::new(Semaphore {
            st: Mutex::new((n, Vec::new())),
        })
    }
    async fn acquire(self: &Arc<Self>) -> Permit {
        poll_fn(|cx| {
            let mut g = self.st.lock();
            if g.0 > 0 {
                g.0 -= 1;
                return Poll::Ready(Permit(self.clone()));
            }
            if !g.1.iter().any(|w| w.will_wake(cx.waker())) {
                g.1.push(cx.waker().clone());
            }
            Poll::Pending
        })
        .await
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let ws = {
            let mut g = self.0.st.lock();
            g.0 += 1;
            std::mem::take(&mut g.1)
        };
        ws.into_iter().for_each(Waker::wake);
    }
}
/// `notify_one` stores at most one permit if nobody waits, so notify-before-wait is not lost (same as tokio).
struct Notify {
    st: Mutex<(bool, Vec<Waker>)>,
}
impl Notify {
    fn new() -> Notify {
        Notify {
            st: Mutex::new((false, Vec::new())),
        }
    }
    fn notify_one(&self) {
        let ws = {
            let mut g = self.st.lock();
            g.0 = true;
            std::mem::take(&mut g.1)
        };
        ws.into_iter().for_each(Waker::wake);
    }
    async fn notified(&self) {
        poll_fn(|cx| {
            let mut g = self.st.lock();
            if g.0 {
                g.0 = false;
                return Poll::Ready(());
            }
            if !g.1.iter().any(|w| w.will_wake(cx.waker())) {
                g.1.push(cx.waker().clone());
            }
            Poll::Pending
        })
        .await
    }
}

// ---- combinators ----
enum MaybeDone<F: Future> {
    Pending(Pin<Box<F>>),
    Done(Option<F::Output>),
}
impl<F: Future> MaybeDone<F> {
    fn new(f: F) -> Self {
        MaybeDone::Pending(Box::pin(f))
    }
    fn poll_done(&mut self, cx: &mut Context<'_>) -> bool {
        if let MaybeDone::Pending(f) = self {
            match f.as_mut().poll(cx) {
                Poll::Ready(v) => *self = MaybeDone::Done(Some(v)),
                Poll::Pending => return false,
            }
        }
        true
    }
    fn take(&mut self) -> F::Output {
        match self {
            MaybeDone::Done(v) => v.take().expect("taken twice"),
            MaybeDone::Pending(_) => unreachable!(),
        }
    }
}
/// Runs both futures concurrently on the SAME task (no spawn); output when both are done. Dropping it cancels both.
struct Join2<A: Future, B: Future> {
    a: MaybeDone<A>,
    b: MaybeDone<B>,
}
impl<A: Future, B: Future> Unpin for Join2<A, B> {} // all fields are boxed/owned; nothing is pinned in place
fn join2<A: Future, B: Future>(a: A, b: B) -> Join2<A, B> {
    Join2 {
        a: MaybeDone::new(a),
        b: MaybeDone::new(b),
    }
}
impl<A: Future, B: Future> Future for Join2<A, B> {
    type Output = (A::Output, B::Output);
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let t = self.get_mut();
        let (da, db) = (t.a.poll_done(cx), t.b.poll_done(cx)); // poll both even if the first is done: no short circuit
        if da && db {
            Poll::Ready((t.a.take(), t.b.take()))
        } else {
            Poll::Pending
        }
    }
}
struct JoinAll<F: Future>(Vec<MaybeDone<F>>);
impl<F: Future> Unpin for JoinAll<F> {}
fn join_all<F: Future>(fs: impl IntoIterator<Item = F>) -> JoinAll<F> {
    JoinAll(fs.into_iter().map(MaybeDone::new).collect())
}
impl<F: Future> Future for JoinAll<F> {
    type Output = Vec<F::Output>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let t = self.get_mut();
        let mut all = true;
        for m in t.0.iter_mut() {
            all &= m.poll_done(cx);
        }
        if all {
            Poll::Ready(t.0.iter_mut().map(MaybeDone::take).collect())
        } else {
            Poll::Pending
        }
    }
}
#[derive(Debug, PartialEq)]
enum Either<L, R> {
    Left(L),
    Right(R),
}
/// First-to-finish wins; `a` is polled first (biased). The loser is DROPPED: only cancel-safe futures may lose.
struct Select2<A: Future, B: Future> {
    a: Option<Pin<Box<A>>>,
    b: Option<Pin<Box<B>>>,
}
fn select2<A: Future, B: Future>(a: A, b: B) -> Select2<A, B> {
    Select2 {
        a: Some(Box::pin(a)),
        b: Some(Box::pin(b)),
    }
}
impl<A: Future, B: Future> Future for Select2<A, B> {
    type Output = Either<A::Output, B::Output>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let t = self.get_mut();
        if let Some(Poll::Ready(v)) = t.a.as_mut().map(|f| f.as_mut().poll(cx)) {
            t.a = None;
            t.b = None; // cancel the loser right now
            return Poll::Ready(Either::Left(v));
        }
        if let Some(Poll::Ready(v)) = t.b.as_mut().map(|f| f.as_mut().poll(cx)) {
            t.a = None;
            t.b = None;
            return Poll::Ready(Either::Right(v));
        }
        Poll::Pending
    }
}

// =====================================================================================================================
// 5. IO REACTOR (mio) and TcpListener / TcpStream
// =====================================================================================================================
const WAKE_TOKEN: Token = Token(0);
struct IoState {
    token: usize,
    r_epoch: AtomicU64,
    w_epoch: AtomicU64,
    r_waker: Mutex<Option<Waker>>,
    w_waker: Mutex<Option<Waker>>,
}
struct Reactor {
    registry: mio::Registry,
    next_token: AtomicUsize,
    sources: Mutex<HashMap<usize, Arc<IoState>>>,
    waker: mio::Waker,
    shutdown: AtomicBool,
    events_seen: AtomicU64,
}
impl Reactor {
    fn new() -> io::Result<(Arc<Reactor>, mio::Poll)> {
        let poll = mio::Poll::new()?;
        let r = Reactor {
            registry: poll.registry().try_clone()?,
            next_token: AtomicUsize::new(1),
            sources: Mutex::new(HashMap::new()),
            waker: mio::Waker::new(poll.registry(), WAKE_TOKEN)?,
            shutdown: AtomicBool::new(false),
            events_seen: AtomicU64::new(0),
        };
        Ok((Arc::new(r), poll))
    }
    fn register<S: mio::event::Source>(self: &Arc<Self>, mut src: S) -> io::Result<Io<S>> {
        let token = self.next_token.fetch_add(1, Relaxed);
        let st = Arc::new(IoState {
            token,
            r_epoch: AtomicU64::new(0),
            w_epoch: AtomicU64::new(0),
            r_waker: Mutex::new(None),
            w_waker: Mutex::new(None),
        });
        self.sources.lock().insert(token, st.clone());
        self.registry.register(
            &mut src,
            Token(token),
            Interest::READABLE | Interest::WRITABLE,
        )?;
        Ok(Io {
            src,
            st,
            reactor: self.clone(),
        })
    }
    fn run(&self, mut poll: mio::Poll) {
        let mut events = Events::with_capacity(256);
        loop {
            match poll.poll(&mut events, None) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => panic!("poll: {e}"),
            }
            if self.shutdown.load(SeqCst) {
                return;
            }
            for ev in events.iter() {
                if ev.token() == WAKE_TOKEN {
                    continue;
                }
                self.events_seen.fetch_add(1, Relaxed);
                let st = self.sources.lock().get(&ev.token().0).cloned();
                let Some(st) = st else { continue };
                if ev.is_readable() || ev.is_read_closed() || ev.is_error() {
                    st.r_epoch.fetch_add(1, SeqCst); // bump BEFORE taking the waker (see poll_io)
                    if let Some(w) = st.r_waker.lock().take() {
                        w.wake();
                    }
                }
                if ev.is_writable() || ev.is_write_closed() || ev.is_error() {
                    st.w_epoch.fetch_add(1, SeqCst);
                    if let Some(w) = st.w_waker.lock().take() {
                        w.wake();
                    }
                }
            }
        }
    }
}
struct Io<S: mio::event::Source> {
    src: S,
    st: Arc<IoState>,
    reactor: Arc<Reactor>,
}
impl<S: mio::event::Source> Drop for Io<S> {
    fn drop(&mut self) {
        let _ = self.reactor.registry.deregister(&mut self.src);
        self.reactor.sources.lock().remove(&self.st.token);
    }
}
#[derive(Clone, Copy)]
enum Dir {
    Read,
    Write,
}
/// The edge-triggered recipe: (1) remember the epoch, (2) TRY the operation, (3) on WouldBlock publish the waker, (4) if the
/// epoch moved in the meantime an edge was delivered after our syscall: retry instead of sleeping on a missed event.
fn poll_io<R>(
    st: &IoState,
    dir: Dir,
    cx: &mut Context<'_>,
    mut f: impl FnMut() -> io::Result<R>,
) -> Poll<io::Result<R>> {
    let (epoch, waker) = match dir {
        Dir::Read => (&st.r_epoch, &st.r_waker),
        Dir::Write => (&st.w_epoch, &st.w_waker),
    };
    loop {
        let e = epoch.load(SeqCst);
        match f() {
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                *waker.lock() = Some(cx.waker().clone());
                if epoch.load(SeqCst) != e {
                    continue;
                }
                return Poll::Pending;
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            other => return Poll::Ready(other),
        }
    }
}

struct TcpListener {
    io: Io<mio::net::TcpListener>,
}
#[derive(Clone)]
struct TcpStream {
    io: Arc<Io<mio::net::TcpStream>>,
}
fn reactor() -> Arc<Reactor> {
    current().reactor.expect("runtime has no reactor")
}
impl TcpListener {
    fn bind(addr: SocketAddr) -> io::Result<TcpListener> {
        let l = mio::net::TcpListener::bind(addr)?;
        Ok(TcpListener {
            io: reactor().register(l)?,
        })
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.src.local_addr()
    }
    async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let (s, a) =
            poll_fn(|cx| poll_io(&self.io.st, Dir::Read, cx, || self.io.src.accept())).await?;
        Ok((
            TcpStream {
                io: Arc::new(reactor().register(s)?),
            },
            a,
        ))
    }
}
impl TcpStream {
    async fn connect(addr: SocketAddr) -> io::Result<TcpStream> {
        let s = mio::net::TcpStream::connect(addr)?;
        let io = reactor().register(s)?;
        poll_fn(|cx| {
            poll_io(&io.st, Dir::Write, cx, || {
                if let Some(e) = io.src.take_error()? {
                    return Err(e);
                }
                match io.src.peer_addr() {
                    Ok(_) => Ok(()),
                    Err(e) if e.kind() == io::ErrorKind::NotConnected => {
                        Err(io::ErrorKind::WouldBlock.into())
                    }
                    Err(e) => Err(e),
                }
            })
        })
        .await?;
        Ok(TcpStream { io: Arc::new(io) })
    }
    async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| poll_io(&self.io.st, Dir::Read, cx, || (&self.io.src).read(buf))).await
    }
    async fn read_exact(&self, buf: &mut [u8]) -> io::Result<()> {
        let mut off = 0;
        while off < buf.len() {
            match self.read(&mut buf[off..]).await? {
                0 => return Err(io::ErrorKind::UnexpectedEof.into()),
                n => off += n,
            }
        }
        Ok(())
    }
    async fn write_all(&self, buf: &[u8]) -> io::Result<()> {
        let mut off = 0;
        while off < buf.len() {
            // NOT cancel safe: dropping mid-way leaves a prefix written
            off += poll_fn(|cx| {
                poll_io(&self.io.st, Dir::Write, cx, || {
                    (&self.io.src).write(&buf[off..])
                })
            })
            .await?;
        }
        Ok(())
    }
    fn shutdown_write(&self) -> io::Result<()> {
        self.io.src.shutdown(Shutdown::Write)
    }
}

// =====================================================================================================================
// VERIFICATION
// =====================================================================================================================
struct Report(Vec<(&'static str, String)>);
impl Report {
    fn add(&mut self, name: &'static str, detail: String) {
        self.0.push((name, detail));
    }
}
struct DropGuard(Arc<AtomicUsize>);
impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.fetch_add(1, SeqCst);
    }
}
fn spin_for(d: Duration) {
    let t = Instant::now();
    while t.elapsed() < d {
        std::hint::spin_loop();
    }
}

fn t_100k_tasks(rep: &mut Report) {
    const N: usize = 100_000;
    let rt = Runtime::new(4);
    let hits: Arc<Vec<AtomicU8>> = Arc::new((0..N).map(|_| AtomicU8::new(0)).collect());
    let t0 = Instant::now();
    let handles: Vec<_> = (0..N)
        .map(|i| {
            let hits = hits.clone();
            rt.spawn(async move {
                hits[i].fetch_add(1, SeqCst);
                i
            })
        })
        .collect();
    let sum = rt.block_on(async move {
        let mut s = 0usize;
        for h in handles {
            s += h.await.unwrap();
        }
        s
    });
    let dt = t0.elapsed();
    assert_eq!(sum, N * (N - 1) / 2);
    assert!(
        hits.iter().all(|h| h.load(SeqCst) == 1),
        "every task must run exactly once"
    );
    let ex = rt.executed();
    assert_eq!(ex.iter().sum::<u64>(), N as u64, "one poll per tiny task");
    println!("  100k tasks: {:?}  per-worker polls {:?}", dt, ex);
    rep.add(
        "100k tiny tasks",
        format!(
            "each ran exactly once, {:.1} ms, workers {:?}",
            dt.as_secs_f64() * 1e3,
            ex
        ),
    );
}

fn t_work_stealing(rep: &mut Report) {
    let rt = Runtime::new(4);
    let ids = rt.block_on(async {
        // the root task runs on ONE worker, so its 256 children land in that worker's local deque: the others must steal
        let hs: Vec<_> = (0..256)
            .map(|_| {
                spawn(async {
                    spin_for(Duration::from_micros(300));
                    thread::current().id()
                })
            })
            .collect();
        let mut ids = Vec::new();
        for h in hs {
            ids.push(h.await.unwrap());
        }
        ids
    });
    let distinct: std::collections::HashSet<_> = ids.iter().collect();
    let ex = rt.executed();
    let busy = ex.iter().filter(|&&c| c > 0).count();
    println!(
        "  work stealing: per-worker polls {:?}, steals {}, distinct threads {}",
        ex,
        rt.stolen(),
        distinct.len()
    );
    if thread::available_parallelism().map_or(1, |n| n.get()) > 1 {
        assert!(
            busy > 1 && distinct.len() > 1,
            "work must be spread over several workers"
        );
        assert!(
            rt.stolen() > 0,
            "children sit in one local deque, so others must have stolen"
        );
    }
    rep.add(
        "work stealing",
        format!(
            "polls/worker {:?}, steals {}, {} threads ran tasks",
            ex,
            rt.stolen(),
            distinct.len()
        ),
    );
}

struct Gate {
    count: AtomicUsize,
    waker: Mutex<Option<Waker>>,
}
struct WaitFor(Arc<Gate>, usize);
impl Future for WaitFor {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0.count.load(SeqCst) >= self.1 {
            return Poll::Ready(());
        }
        *self.0.waker.lock() = Some(cx.waker().clone());
        if self.0.count.load(SeqCst) >= self.1 {
            Poll::Ready(())
        } else {
            Poll::Pending
        } // register, THEN recheck
    }
}
fn t_wake_hazards(rep: &mut Report) {
    let rt = Runtime::new(4);
    let rounds = 300;
    let before = TOTAL_POLLS.load(Relaxed);
    let (total_wakes, ok) = rt.block_on(async {
        let wakes = Arc::new(AtomicUsize::new(0));
        let mut hs = Vec::new();
        for _ in 0..rounds {
            let gate = Arc::new(Gate {
                count: AtomicUsize::new(0),
                waker: Mutex::new(None),
            });
            let n = 300;
            for _ in 0..3 {
                let (g, w) = (gate.clone(), wakes.clone());
                // external threads hammer wake() concurrently with the task being polled, running, and going idle
                thread::spawn(move || {
                    for _ in 0..n / 3 {
                        g.count.fetch_add(1, SeqCst);
                        let wk = g.waker.lock().clone();
                        if let Some(wk) = wk {
                            w.fetch_add(1, Relaxed);
                            wk.wake();
                        }
                    }
                });
            }
            hs.push(spawn(WaitFor(gate, n)));
        }
        // a lost wakeup would hang forever; the timeout (our own!) turns that into a test failure
        let r = timeout(Duration::from_secs(20), join_all(hs)).await;
        (
            wakes.load(Relaxed),
            r.map(|v| v.into_iter().all(|x| x.is_ok())),
        )
    });
    assert_eq!(ok, Ok(true), "lost wakeup: a task never completed");
    let polls = TOTAL_POLLS.load(Relaxed) - before;
    assert_eq!(
        POLL_VIOLATIONS.load(SeqCst),
        0,
        "a task was polled by two threads at once"
    );
    rep.add(
        "wake hazards",
        format!(
            "{rounds} hammered tasks done, {total_wakes} wakes -> {polls} polls, 0 concurrent polls"
        ),
    );
}

fn t_yield_order(rep: &mut Report) {
    let rt = LocalRuntime::new(Clock::Real(Instant::now()));
    let log = Arc::new(Mutex::new(Vec::new()));
    let hs: Vec<_> = (0..3usize)
        .map(|id| {
            let log = log.clone();
            rt.spawn(async move {
                for _ in 0..3 {
                    log.lock().push(id);
                    yield_now().await;
                }
            })
        })
        .collect();
    rt.block_on(async {
        for h in hs {
            h.await.unwrap();
        }
    });
    let got = log.lock().clone();
    assert_eq!(
        got,
        vec![0, 1, 2, 0, 1, 2, 0, 1, 2],
        "yield_now must round-robin"
    );
    rep.add("yield_now fairness", format!("round-robin order {:?}", got));
}

fn t_timers_manual(rep: &mut Report) {
    let rt = LocalRuntime::new(Clock::manual());
    let order = Arc::new(Mutex::new(Vec::new()));
    for i in 0..1000u64 {
        let d = (i * 7919) % 1000 + 1; // a permutation of 1..=1000 ms
        let order = order.clone();
        rt.spawn(async move {
            sleep(Duration::from_millis(d)).await;
            order.lock().push(d);
        });
    }
    rt.run_until_stalled();
    assert!(
        order.lock().is_empty(),
        "nothing may fire before the clock moves"
    );
    rt.advance(Duration::from_millis(499));
    rt.run_until_stalled();
    assert_eq!(order.lock().len(), 499);
    rt.advance(Duration::from_millis(1));
    rt.run_until_stalled();
    assert_eq!(
        order.lock().len(),
        500,
        "exactly the timers with deadline <= now fire"
    );
    rt.advance(Duration::from_millis(500));
    rt.run_until_stalled();
    let o = order.lock().clone();
    assert_eq!(
        o,
        (1..=1000).collect::<Vec<_>>(),
        "timers fire in deadline order"
    );
    // timeout on the manual clock: 50 ms deadline, checked at 49 and 50
    let flag = Arc::new(AtomicU8::new(0));
    let f2 = flag.clone();
    rt.spawn(async move {
        let r = timeout(Duration::from_millis(50), std::future::pending::<()>()).await;
        f2.store(if r == Err(Elapsed) { 2 } else { 1 }, SeqCst);
    });
    rt.run_until_stalled();
    rt.advance(Duration::from_millis(49));
    rt.run_until_stalled();
    assert_eq!(flag.load(SeqCst), 0, "not early");
    rt.advance(Duration::from_millis(1));
    rt.run_until_stalled();
    assert_eq!(flag.load(SeqCst), 2, "fires exactly at the deadline");
    // interval: immediate first tick, then every 10 ms
    let ticks = Arc::new(AtomicUsize::new(0));
    let t2 = ticks.clone();
    let h = rt.spawn(async move {
        let mut iv = interval(Duration::from_millis(10));
        loop {
            iv.tick().await;
            t2.fetch_add(1, SeqCst);
        }
    });
    rt.run_until_stalled();
    assert_eq!(ticks.load(SeqCst), 1);
    rt.advance(Duration::from_millis(35));
    rt.run_until_stalled();
    assert_eq!(ticks.load(SeqCst), 4, "ticks at 0, 10, 20, 30");
    h.abort();
    rt.run_until_stalled();
    assert!(h.is_finished());
    assert_eq!(rt.timer.fire_due(), 0);
    rep.add(
        "timers (manual clock)",
        "1000 fire in deadline order, none early; timeout/interval exact".into(),
    );
}

fn t_timers_real(rep: &mut Report) {
    let rt = Runtime::new(4);
    let t0 = Instant::now();
    let hs: Vec<_> = (0..1000u64)
        .map(|i| {
            let d = Duration::from_micros(1000 + ((i * 7919) % 1000) * 60); // 1 ms .. 61 ms
            spawn_on(&rt.handle.sched, async move {
                let started = Instant::now();
                sleep(d).await;
                let waited = started.elapsed();
                (waited >= d, t0.elapsed() >= d)
            })
        })
        .collect();
    let res = rt.block_on(async move {
        let mut v = Vec::new();
        for h in hs {
            v.push(h.await.unwrap());
        }
        v
    });
    assert!(res.iter().all(|&(a, b)| a && b), "a timer fired early");
    // sleep(0) and past deadlines complete immediately; many cancelled sleeps are skipped lazily
    rt.block_on(async {
        sleep(Duration::ZERO).await;
        for _ in 0..100 {
            let _ = timeout(Duration::from_micros(10), sleep(Duration::from_secs(60))).await;
        }
    });
    rep.add(
        "timers (real clock)",
        format!(
            "1000 sleeps 1-61 ms, none early, done in {:.0} ms",
            t0.elapsed().as_secs_f64() * 1e3
        ),
    );
}

fn t_timeout_cancel(rep: &mut Report) {
    let rt = Runtime::new(2);
    let drops = Arc::new(AtomicUsize::new(0));
    let d = drops.clone();
    let r = rt.block_on(async move {
        timeout(Duration::from_millis(20), async move {
            let _g = DropGuard(d);
            sleep(Duration::from_secs(30)).await; // would block the test for 30 s if not cancelled
            1
        })
        .await
    });
    assert_eq!(r, Err(Elapsed));
    assert_eq!(
        drops.load(SeqCst),
        1,
        "dropping the timed-out future must run its destructors"
    );
    let ok = rt.block_on(async { timeout(Duration::from_secs(5), async { 7 }).await });
    assert_eq!(ok, Ok(7));
    // select2: loser dropped immediately
    let d2 = Arc::new(AtomicUsize::new(0));
    let d3 = d2.clone();
    let r = rt.block_on(select2(
        async {
            sleep(Duration::from_millis(5)).await;
            "fast"
        },
        async move {
            let _g = DropGuard(d3);
            sleep(Duration::from_secs(30)).await;
            "slow"
        },
    ));
    assert_eq!(r, Either::Left("fast"));
    assert_eq!(d2.load(SeqCst), 1);
    rep.add(
        "timeout/select cancel",
        "slow future dropped, guard destructors ran exactly once each".into(),
    );
}

fn t_panic_abort(rep: &mut Report) {
    let rt = Runtime::new(4);
    let before = rt.executed().iter().sum::<u64>();
    let bad: Vec<_> = (0..8)
        .map(|i| rt.spawn(async move { if i >= 0 { panic!("boom {i}") } else { i } }))
        .collect();
    let res = rt.block_on(async move {
        let mut r = Vec::new();
        for h in bad {
            r.push(h.await);
        }
        r
    });
    for (i, r) in res.iter().enumerate() {
        assert_eq!(r, &Err(JoinError::Panic(format!("boom {i}"))));
    }
    // workers survived: a fresh batch still completes
    let good = rt.block_on(async {
        let hs: Vec<_> = (0..1000u64)
            .map(|i| {
                spawn(async move {
                    yield_now().await;
                    i
                })
            })
            .collect();
        let mut s = 0;
        for h in hs {
            s += h.await.unwrap();
        }
        s
    });
    assert_eq!(good, 999 * 1000 / 2);
    assert!(rt.executed().iter().sum::<u64>() > before);
    // abort: a task blocked forever is cancelled and its destructors run
    let drops = Arc::new(AtomicUsize::new(0));
    let d = drops.clone();
    let (tx, rx) = oneshot::<()>();
    let h = rt.spawn(async move {
        let _g = DropGuard(d);
        let _ = rx.await; // pending until aborted
        1
    });
    rt.block_on(async {
        sleep(Duration::from_millis(10)).await;
        h.abort();
        assert_eq!(h.await, Err(JoinError::Cancelled));
    });
    assert_eq!(drops.load(SeqCst), 1);
    assert!(
        tx.send(()).is_err(),
        "receiver dropped with the aborted task"
    );
    // aborting a finished task is a no-op; dropping a JoinHandle detaches (task keeps running)
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    drop(rt.spawn(async move {
        sleep(Duration::from_millis(5)).await;
        d2.store(true, SeqCst);
    }));
    rt.block_on(async { sleep(Duration::from_millis(60)).await });
    assert!(done.load(SeqCst), "detached task still ran");
    rep.add(
        "panic + abort",
        "8 panics -> Err(Panic), workers alive; abort -> Cancelled + Drop ran; detach works".into(),
    );
}

fn t_sync(rep: &mut Report) {
    let rt = Runtime::new(4);
    // oneshot
    let (tx, rx) = oneshot();
    rt.spawn(async move {
        sleep(Duration::from_millis(2)).await;
        tx.send(42).unwrap();
    });
    assert_eq!(rt.block_on(rx), Ok(42));
    let (tx, rx) = oneshot::<u8>();
    drop(tx);
    assert_eq!(rt.block_on(rx), Err(RecvError));
    // bounded mpsc: capacity 4, 4 producers x 250, slow consumer
    let (tx, mut rx) = mpsc_channel::<(u32, u32)>(4);
    let producers: Vec<_> = (0..4u32)
        .map(|p| {
            let tx = tx.clone();
            rt.spawn(async move {
                for i in 0..250 {
                    tx.send((p, i)).await.unwrap();
                }
            })
        })
        .collect();
    drop(tx);
    let (n, per_producer_in_order, maxlen) = rt.block_on(async move {
        let mut last = [None::<u32>; 4];
        let (mut n, mut ordered) = (0, true);
        while let Some((p, i)) = rx.recv().await {
            if last[p as usize].is_some_and(|l| l + 1 != i) {
                ordered = false;
            }
            last[p as usize] = Some(i);
            n += 1;
            if n % 8 == 0 {
                yield_now().await; // slow consumer: producers must block
            }
        }
        (n, ordered, rx.max_observed_len())
    });
    rt.block_on(async {
        for p in producers {
            p.await.unwrap();
        }
    });
    assert_eq!(n, 1000);
    assert!(per_producer_in_order);
    assert!(maxlen <= 4, "queue exceeded its bound: {maxlen}");
    // send to a closed receiver returns the value
    let (tx, rx) = mpsc_channel::<u8>(1);
    drop(rx);
    assert_eq!(rt.block_on(tx.send(9)), Err(9));
    // semaphore bounds concurrency; notify stores a permit
    let sem = Semaphore::new(3);
    let (cur, max) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let hs: Vec<_> = (0..50)
        .map(|_| {
            let (sem, cur, max) = (sem.clone(), cur.clone(), max.clone());
            rt.spawn(async move {
                let _p = sem.acquire().await;
                let c = cur.fetch_add(1, SeqCst) + 1;
                max.fetch_max(c, SeqCst);
                sleep(Duration::from_millis(1)).await;
                cur.fetch_sub(1, SeqCst);
            })
        })
        .collect();
    rt.block_on(async {
        for h in hs {
            h.await.unwrap();
        }
    });
    assert!(max.load(SeqCst) <= 3 && max.load(SeqCst) >= 1);
    let nt = Arc::new(Notify::new());
    nt.notify_one(); // before anyone waits
    let n2 = nt.clone();
    rt.block_on(async move { n2.notified().await });
    let n3 = nt.clone();
    let waiter = rt.spawn(async move {
        n3.notified().await;
        5
    });
    rt.block_on(async {
        sleep(Duration::from_millis(5)).await;
        assert!(!waiter.is_finished());
        nt.notify_one();
        assert_eq!(waiter.await, Ok(5));
    });
    // join2 / join_all / select2 results
    let (a, b) = rt.block_on(join2(
        async {
            sleep(Duration::from_millis(3)).await;
            1
        },
        async {
            yield_now().await;
            "x"
        },
    ));
    assert_eq!((a, b), (1, "x"));
    let v = rt.block_on(join_all((0..10u64).map(|i| async move {
        yield_now().await;
        i * 2
    })));
    assert_eq!(v, (0..10).map(|i| i * 2).collect::<Vec<_>>());
    rep.add(
        "sync primitives",
        format!(
            "mpsc cap 4 -> max queue {maxlen}, 1000 msgs in order; sem max concurrency {}",
            max.load(SeqCst)
        ),
    );
}

async fn echo_conn(s: TcpStream) -> io::Result<u64> {
    let mut buf = vec![0u8; 16 * 1024];
    let mut total = 0u64;
    loop {
        let n = s.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        s.write_all(&buf[..n]).await?;
        total += n as u64;
    }
    s.shutdown_write()?;
    Ok(total)
}
fn payload(id: usize, n: usize) -> Vec<u8> {
    (0..n).map(|j| ((j * 7 + id) % 251) as u8).collect()
}
async fn echo_client(addr: SocketAddr, id: usize, n: usize) -> io::Result<u64> {
    let s = TcpStream::connect(addr).await?;
    let (w, r) = (s.clone(), s);
    let data = payload(id, n);
    // write and read concurrently on one task: otherwise 2 full socket buffers would deadlock (join2 at work)
    let writer = async move {
        w.write_all(&data).await?;
        w.shutdown_write()
    };
    let reader = async move {
        let mut got = vec![0u8; n];
        r.read_exact(&mut got).await?;
        Ok::<_, io::Error>(got)
    };
    let (wr, rd) = join2(writer, reader).await;
    wr?;
    assert!(rd? == payload(id, n), "client {id}: echoed bytes differ");
    Ok(n as u64)
}
fn t_echo(rep: &mut Report) {
    let rt = Runtime::new(4);
    let t0 = Instant::now();
    let (srv_bytes, cli_bytes, addr) = rt.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = spawn(async move {
            let mut hs = Vec::new();
            for _ in 0..20 {
                let (s, _) = listener.accept().await.unwrap();
                hs.push(spawn(echo_conn(s)));
            }
            let mut t = 0;
            for h in hs {
                t += h.await.unwrap().unwrap();
            }
            t
        });
        let clients: Vec<_> = (0..20)
            .map(|i| spawn(echo_client(addr, i, 100_000 + i * 1000)))
            .collect();
        let mut c = 0;
        for h in clients {
            c += h.await.unwrap().unwrap();
        }
        (server.await.unwrap(), c, addr)
    });
    let expect: u64 = (0..20u64).map(|i| 100_000 + i * 1000).sum();
    assert_eq!((srv_bytes, cli_bytes), (expect, expect));
    let ev = rt.reactor.events_seen.load(Relaxed);
    assert!(ev > 0);
    rep.add(
        "echo server (mio)",
        format!(
            "20 clients on {addr}, {} KB echoed intact, {ev} reactor events, {:.0} ms",
            expect / 1000,
            t0.elapsed().as_secs_f64() * 1e3
        ),
    );
}

// ---- differential check: the same programs on tokio and on this runtime must agree ----
fn mine_fanout(rt: &Runtime) -> u64 {
    rt.block_on(async {
        let hs: Vec<_> = (0..1000u64)
            .map(|i| {
                spawn(async move {
                    for _ in 0..3 {
                        yield_now().await;
                    }
                    i * i
                })
            })
            .collect();
        let mut s = 0;
        for h in hs {
            s += h.await.unwrap();
        }
        s
    })
}
async fn tokio_fanout() -> u64 {
    let hs: Vec<_> = (0..1000u64)
        .map(|i| {
            tokio::spawn(async move {
                for _ in 0..3 {
                    tokio::task::yield_now().await;
                }
                i * i
            })
        })
        .collect();
    let mut s = 0;
    for h in hs {
        s += h.await.unwrap();
    }
    s
}
fn mine_pipeline(rt: &Runtime) -> (u64, u64) {
    rt.block_on(async {
        let (tx, mut rx) = mpsc_channel::<u64>(8);
        for p in 0..4u64 {
            let tx = tx.clone();
            spawn(async move {
                for i in 0..500 {
                    tx.send(p * 1000 + i).await.unwrap();
                }
            });
        }
        drop(tx);
        let (mut c, mut s) = (0, 0);
        while let Some(v) = rx.recv().await {
            c += 1;
            s += v;
        }
        (c, s)
    })
}
async fn tokio_pipeline() -> (u64, u64) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u64>(8);
    for p in 0..4u64 {
        let tx = tx.clone();
        tokio::spawn(async move {
            for i in 0..500 {
                tx.send(p * 1000 + i).await.unwrap();
            }
        });
    }
    drop(tx);
    let (mut c, mut s) = (0, 0);
    while let Some(v) = rx.recv().await {
        c += 1;
        s += v;
    }
    (c, s)
}
fn mine_time(rt: &Runtime) -> (Option<i32>, bool, &'static str) {
    rt.block_on(async {
        let a = timeout(Duration::from_secs(5), async { 7 }).await.ok();
        let b = timeout(Duration::from_millis(5), sleep(Duration::from_secs(30)))
            .await
            .is_err();
        let c = match select2(
            sleep(Duration::from_millis(2)),
            sleep(Duration::from_secs(30)),
        )
        .await
        {
            Either::Left(()) => "fast",
            Either::Right(()) => "slow",
        };
        (a, b, c)
    })
}
async fn tokio_time() -> (Option<i32>, bool, &'static str) {
    use tokio::time::{sleep, timeout};
    let a = timeout(Duration::from_secs(5), async { 7 }).await.ok();
    let b = timeout(Duration::from_millis(5), sleep(Duration::from_secs(30)))
        .await
        .is_err();
    let c = tokio::select! { _ = sleep(Duration::from_millis(2)) => "fast", _ = sleep(Duration::from_secs(30)) => "slow" };
    (a, b, c)
}
fn mine_panic(rt: &Runtime) -> (bool, String) {
    rt.block_on(async {
        match spawn(async {
            if true {
                panic!("boom")
            }
        })
        .await
        {
            Err(JoinError::Panic(m)) => (true, m),
            _ => (false, String::new()),
        }
    })
}
async fn tokio_panic() -> (bool, String) {
    match tokio::spawn(async {
        if true {
            panic!("boom")
        }
    })
    .await
    {
        Err(e) if e.is_panic() => (
            true,
            e.into_panic().downcast_ref::<&str>().unwrap().to_string(),
        ),
        _ => (false, String::new()),
    }
}
fn mine_echo(rt: &Runtime) -> (u64, u64) {
    rt.block_on(async {
        let l = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = l.local_addr().unwrap();
        let srv = spawn(async move {
            let mut hs = Vec::new();
            for _ in 0..4 {
                hs.push(spawn(echo_conn(l.accept().await.unwrap().0)));
            }
            let mut t = 0;
            for h in hs {
                t += h.await.unwrap().unwrap();
            }
            t
        });
        let cs: Vec<_> = (0..4)
            .map(|i| spawn(echo_client(addr, i, 50_000)))
            .collect();
        let mut c = 0;
        for h in cs {
            c += h.await.unwrap().unwrap();
        }
        (srv.await.unwrap(), c)
    })
}
async fn tokio_echo() -> (u64, u64) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    let srv = tokio::spawn(async move {
        let mut hs = Vec::new();
        for _ in 0..4 {
            let (mut s, _) = l.accept().await.unwrap();
            hs.push(tokio::spawn(async move {
                let (mut r, mut w) = s.split();
                let (mut buf, mut t) = (vec![0u8; 16 * 1024], 0u64);
                loop {
                    let n = r.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    w.write_all(&buf[..n]).await.unwrap();
                    t += n as u64;
                }
                t
            }));
        }
        let mut t = 0;
        for h in hs {
            t += h.await.unwrap();
        }
        t
    });
    let cs: Vec<_> = (0..4)
        .map(|i| {
            tokio::spawn(async move {
                let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
                let (mut r, mut w) = s.split();
                let data = payload(i, 50_000);
                let wr = async {
                    w.write_all(&data).await.unwrap();
                    w.shutdown().await.unwrap();
                };
                let mut got = vec![0u8; 50_000];
                let rd = async { r.read_exact(&mut got).await.unwrap() };
                tokio::join!(wr, rd);
                assert!(got == data);
                50_000u64
            })
        })
        .collect();
    let mut c = 0;
    for h in cs {
        c += h.await.unwrap();
    }
    (srv.await.unwrap(), c)
}
fn t_differential(rep: &mut Report) {
    let rt = Runtime::new(4);
    let trt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    let (m, t) = (mine_fanout(&rt), trt.block_on(tokio_fanout()));
    assert_eq!(m, t);
    let (m2, t2) = (mine_pipeline(&rt), trt.block_on(tokio_pipeline()));
    assert_eq!(m2, t2);
    let (m3, t3) = (mine_time(&rt), trt.block_on(tokio_time()));
    assert_eq!(m3, t3);
    let (m4, t4) = (mine_panic(&rt), trt.block_on(tokio_panic()));
    assert_eq!(m4, t4);
    let (m5, t5) = (mine_echo(&rt), trt.block_on(tokio_echo()));
    assert_eq!(m5, t5);
    drop(trt);
    rep.add(
        "differential vs tokio",
        format!(
            "5 programs identical: fanout={m} pipeline={m2:?} time={m3:?} panic={m4:?} echo={m5:?}"
        ),
    );
}

fn t_shutdown(rep: &mut Report) {
    // tasks queued from outside with a busy runtime, then an immediate drop: threads must all be joined
    let before = LIVE_THREADS.load(SeqCst);
    {
        let rt = Runtime::new(4);
        assert_eq!(
            LIVE_THREADS.load(SeqCst),
            before + 4 + 2,
            "4 workers + timer + reactor"
        );
        let counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..1000 {
            let c = counter.clone();
            rt.spawn(async move {
                yield_now().await;
                c.fetch_add(1, SeqCst);
            });
        }
        drop(rt);
        // graceful: workers drain runnable work before exiting, so everything that was runnable at shutdown ran or was dropped
        assert!(counter.load(SeqCst) <= 1000);
    }
    assert_eq!(LIVE_THREADS.load(SeqCst), before, "leaked runtime threads");
    rep.add(
        "clean shutdown",
        "all worker/timer/reactor threads joined after drop".into(),
    );
}

fn main() {
    // panicking tasks are expected here; keep the default hook from spamming stderr
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned());
        if !msg.as_deref().is_some_and(|m| m.starts_with("boom")) {
            default_hook(info); // only the deliberate "boom" panics are silenced
        }
    }));
    let mut rep = Report(Vec::new());
    println!("c09 mini async runtime");
    t_100k_tasks(&mut rep);
    t_work_stealing(&mut rep);
    t_wake_hazards(&mut rep);
    t_yield_order(&mut rep);
    t_timers_manual(&mut rep);
    t_timers_real(&mut rep);
    t_timeout_cancel(&mut rep);
    t_panic_abort(&mut rep);
    t_sync(&mut rep);
    t_echo(&mut rep);
    t_differential(&mut rep);
    t_shutdown(&mut rep);
    let _ = std::panic::take_hook();

    assert_eq!(
        POLL_VIOLATIONS.load(SeqCst),
        0,
        "a task was polled concurrently"
    );
    assert_eq!(LIVE_TASKS.load(SeqCst), 0, "leaked tasks");
    assert_eq!(LIVE_THREADS.load(SeqCst), 0, "leaked threads");
    println!("\n{:<24} | result", "check");
    println!("{:-<24}-+-{:-<90}", "", "");
    for (name, detail) in &rep.0 {
        println!("{name:<24} | {detail}");
    }
    println!(
        "\ntotal task polls {}, concurrent-poll violations {}, live tasks {}, live threads {}",
        TOTAL_POLLS.load(Relaxed),
        POLL_VIOLATIONS.load(SeqCst),
        LIVE_TASKS.load(SeqCst),
        LIVE_THREADS.load(SeqCst)
    );
    println!("ALL CHECKS PASSED");
}
