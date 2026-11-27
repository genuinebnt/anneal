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
//!   ┌──────────── Scheduler trait ───────────────┐      ┌───── Timer (BTreeMap<(deadline,seq)>) ─────┐
//!   │ LocalQueue (VecDeque, block_on drives it)  │      │ Clock: Real(Instant) | Manual(AtomicU64)   │
//!   │ MtShared: Injector + N x (Worker, Stealer) │◀wake─│ driver thread sleeps until next deadline   │
//!   │   idle workers park on Condvar             │      └────────────────────────────────────────────┘
//!   └────────────────────────────────────────────┘      ┌───── Reactor (mio::Poll thread) ───────────┐
//!         worker i: local.pop → injector → steal ◀──wake─│ token → IoState{epoch, wakers} x2 (r/w)    │
//!                                                        └────────────────────────────────────────────┘
//! ```
//! WHAT: one file (runtime first, its checks after it) that reproduces the moving parts of an async runtime using only `Future`/`Waker`/`Wake`, crossbeam
//! deques, parking_lot and mio. No `unsafe` is needed at all: `Wake` for `Arc<Task>` replaces the raw-vtable waker and
//! `Pin<Box<..>>` replaces pin projections.
//! WHY: every `.await` bottoms out in "store my waker somewhere, return Pending, someone calls wake()". Building the
//! somewhere (timer queue, reactor, channel slot, join slot) shows why wakeups get lost and why cancellation is "drop".
//! HOW (the hazards the tests target):
//!  * Task state machine IDLE -> SCHEDULED -> RUNNING -> (NOTIFIED) -> IDLE | COMPLETE. LOST WAKEUP: if `wake` ignored a
//!    RUNNING task and the poll then returned Pending, the event that woke us would be forgotten forever; instead a wake
//!    during RUNNING sets NOTIFIED and the poller re-schedules after Pending. DOUBLE SCHEDULE: two threads waking an IDLE
//!    task must not both enqueue it (two workers would poll the same future concurrently: UB in real runtimes, a data
//!    race here); only the CAS winner of IDLE->SCHEDULED enqueues. A `polling` flag asserts exclusivity at run time.
//!  * MEMORY ORDER: every wake writes the state, also when the task is already queued. A wake that only read it would
//!    publish nothing, and the run could miss data the waker had stored without a lock. Every later write of the state is
//!    a read-modify-write, so no plain store cuts that chain. The root wakers of `block_on` do the same with a swap.
//!  * Wakers are stored BEFORE re-checking the condition (timer fired? epoch changed? queue empty?), the standard
//!    register-then-recheck pattern that closes the race between "check" and "park".
//!  * SLEEPING WORKERS have no timeout. While a worker moves a batch from one queue to another, the tasks are in neither,
//!    so another worker can look, see nothing and sleep after their wake-ups are used up. So a worker that takes a task
//!    and still sees queued work wakes one more.
//!  * ORDER OF AN ENDING: the future is destroyed first, then the `JoinHandle` hears of it. A panic in its destructor
//!    (or in dropping an output nobody wants) is caught and does not reach the worker.
//!  * SHUTDOWN: each executor keeps a list of its live tasks (weak references). Dropping it refuses new work, stops the
//!    threads after the polls in progress, then destroys every remaining future. A waiting task is in no queue, and
//!    task -> future -> waker -> task would keep it alive for ever. Spawning later destroys the future unpolled.
//!    Limits: do not drop a runtime from one of its own tasks, and sockets used outside it afterwards get no more events.
//!  * Edge-triggered IO: mio reports readiness once per transition, so the reactor bumps an epoch counter and wakes; the
//!    consumer always TRIES the syscall first and only on WouldBlock registers a waker and re-checks the epoch. Several
//!    tasks may wait for one direction (two `accept` calls): each has its own entry, removed when its future is dropped.
//!  * Cancellation = dropping the future (destructors run); `abort()` just marks the task so its next run drops it.
//!  * Cancellation safety: `select2`/`timeout` drop the loser. That is only safe for futures that lose no data when
//!    dropped mid-await: `mpsc::recv`, `Notify::notified`, `Sleep` are safe; a half-finished `write_all` is NOT (a prefix
//!    was already written), and `send` hands the value back inside the error only if it was never enqueued.
//!  * Simpler than tokio: `Notify` keeps one flag, so two `notify_one` calls that arrive before either waiter runs release
//!    only one waiter (tokio picks a waiter per call). `Semaphore` and `mpsc` wake every waiter and are not fair.
//! TESTING: `cargo run --release --example mini_async_runtime` runs the checks. The memory-order checks can only fail on a
//! model of weak memory: `MIRIFLAGS=-Zmiri-seed=0 cargo +nightly miri run --example mini_async_runtime` runs just those.
//!    No check fails without the swap that re-queues a task after a wake during its poll.
//! COMMON USES: tokio (multi-thread work-stealing scheduler, mio-backed IO driver, hierarchical timer wheel, `JoinHandle`,
//! oneshot/mpsc/Notify/Semaphore), async-std and smol (global executor + `async-io` reactor, `smol::block_on`),
//! embassy (no_std executor with static tasks and an interrupt-driven timer queue instead of threads).
//! Read: https://tokio.rs/blog/2019-10-scheduler , https://github.com/smol-rs/smol , https://rust-lang.github.io/async-book/
use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::{Future, poll_fn};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::{Pin, pin};
use std::sync::atomic::{
    AtomicBool, AtomicIsize, AtomicU8, AtomicU64, AtomicUsize, Ordering::*, fence,
};
use std::sync::{Arc, Weak};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use mio::{Events, Interest, Token};
use parking_lot::{Condvar, Mutex, RwLock};

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
    fn owned(&self) -> &OwnedTasks;
}

/// Every task of one executor that may still hold a future. Shutdown needs it, because a waiting task is in no queue.
#[derive(Default)]
struct OwnedTasks {
    closed: RwLock<bool>,
    next_id: AtomicU64,
    live: Mutex<HashMap<u64, Weak<Task>>>,
}
impl OwnedTasks {
    /// Calls `push` unless the executor is closed, in which case the reference is just dropped. The read lock is held
    /// across the push, so nothing can be queued after `close` has returned.
    fn push_if_open(&self, task: Arc<Task>, push: impl FnOnce(Arc<Task>)) {
        let closed = self.closed.read();
        if *closed {
            drop(closed);
            drop(task); // outside the lock: this may run a destructor
        } else {
            push(task);
        }
    }
    fn close(&self) {
        *self.closed.write() = true;
    }
    /// Destroys every remaining future. Only call it after `close`, when no thread can run tasks any more.
    fn cancel_all(&self) {
        // collect first: no destructor may run while the list is locked
        let tasks: Vec<Arc<Task>> = std::mem::take(&mut *self.live.lock())
            .into_values()
            .filter_map(|t| t.upgrade())
            .collect();
        for task in &tasks {
            task.destroy();
        }
    }
}

struct Task {
    id: u64,
    state: AtomicU8,
    aborted: AtomicBool,
    polling: AtomicBool, // debug assertion: set while some thread is inside poll()
    future: Mutex<Option<BoxFut>>,
    sched: Arc<dyn Scheduler>,
}

impl Drop for Task {
    fn drop(&mut self) {
        self.sched.owned().live.lock().remove(&self.id);
        LIVE_TASKS.fetch_sub(1, SeqCst);
    }
}

impl Task {
    fn new(fut: BoxFut, sched: Arc<dyn Scheduler>) -> Arc<Task> {
        LIVE_TASKS.fetch_add(1, SeqCst);
        // born SCHEDULED: the spawner enqueues it exactly once
        Arc::new(Task {
            id: sched.owned().next_id.fetch_add(1, Relaxed),
            state: AtomicU8::new(SCHEDULED),
            aborted: AtomicBool::new(false),
            polling: AtomicBool::new(false),
            future: Mutex::new(Some(fut)),
            sched,
        })
    }

    /// The only way a task is ever enqueued after birth. IDLE -> SCHEDULED enqueues (CAS winner only); RUNNING ->
    /// NOTIFIED records the wake so the running poller re-schedules; SCHEDULED/NOTIFIED stay as they are but are still
    /// WRITTEN: the release makes what the waker did before visible to the run that follows. COMPLETE is final.
    fn notify(self: &Arc<Self>) {
        let mut cur = self.state.load(Acquire);
        while cur != COMPLETE {
            let next = match cur {
                IDLE => SCHEDULED,
                RUNNING => NOTIFIED,
                unchanged => unchanged,
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

    /// Drops the future instead of polling it. The caller must be the only one who could poll: the run that dequeued
    /// the task, or shutdown after the threads have stopped.
    fn destroy(&self) {
        let fut = self.future.lock().take();
        drop(fut); // the harness contains panics and reports to the JoinHandle once the user's future is gone
        self.state.swap(COMPLETE, AcqRel);
    }

    /// Called by whichever worker dequeued the task (it owns the SCHEDULED token). The swap reads the latest wake and
    /// acquires what that waker published. A plain store would acquire nothing.
    fn run(self: Arc<Self>) {
        self.state.swap(RUNNING, AcqRel);
        if self.aborted.load(Acquire) {
            self.destroy();
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
                self.state.swap(COMPLETE, AcqRel);
            }
            Poll::Pending => {
                drop(slot);
                if self
                    .state
                    .compare_exchange(RUNNING, IDLE, AcqRel, Acquire)
                    .is_err()
                {
                    // a wake arrived during the poll (state == NOTIFIED): poll again, exactly once
                    // a swap, not a store: it keeps the release of a wake that arrived since the failed exchange
                    self.state.swap(SCHEDULED, AcqRel);
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
            s.finished = true;
            s.result = Some(r);
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake();
        }
    }
}

/// Drops a value whose destructor may panic, and keeps that panic away from the worker.
fn drop_quietly<T>(value: T) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(move || drop(value))) {
        std::mem::forget(payload); // its destructor could panic again
    }
}

/// Wraps the user future: catches panics from each poll and from its destructor, and tells the JoinHandle how it ended
/// only after the future is destroyed.
struct Harness<F: Future> {
    fut: Option<Pin<Box<F>>>,
    join: Option<Arc<JoinInner<F::Output>>>,
}
impl<F: Future> Harness<F> {
    fn finish(&mut self, ending: Result<F::Output, JoinError>) {
        let fut = self.fut.take();
        let destroyed = catch_unwind(AssertUnwindSafe(move || drop(fut))).map_err(panic_msg);
        let result = match (ending, destroyed) {
            (ending, Ok(())) => ending,
            (Err(JoinError::Panic(first)), Err(_)) => Err(JoinError::Panic(first)),
            (ending, Err(from_drop)) => {
                drop_quietly(ending);
                Err(JoinError::Panic(from_drop))
            }
        };
        if let Some(join) = self.join.take() {
            join.complete(result);
            drop_quietly(join); // if the handle is gone, this drops the output nobody took
        }
    }
}
impl<F: Future> Future for Harness<F> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        let fut = this.fut.as_mut().expect("polled after it finished");
        let ending = match catch_unwind(AssertUnwindSafe(|| fut.as_mut().poll(cx))) {
            Ok(Poll::Pending) => return Poll::Pending,
            Ok(Poll::Ready(v)) => Ok(v),
            Err(p) => Err(JoinError::Panic(panic_msg(p))),
        };
        this.finish(ending);
        Poll::Ready(())
    }
}
impl<F: Future> Drop for Harness<F> {
    fn drop(&mut self) {
        if self.join.is_some() {
            self.finish(Err(JoinError::Cancelled));
        }
    }
}
fn panic_msg(p: Box<dyn Any + Send>) -> String {
    let msg = if let Some(s) = p.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic>".into()
    };
    drop_quietly(p);
    msg
}

struct JoinHandle<T> {
    inner: Arc<JoinInner<T>>,
    /// Weak, so that a task nobody can wake any more is freed, and its handle learns of it.
    task: Weak<Task>,
}
impl<T> JoinHandle<T> {
    /// Marks the task; its next run drops the future instead of polling. Awaiting afterwards yields Cancelled.
    fn abort(&self) {
        if let Some(task) = self.task.upgrade() {
            task.aborted.store(true, Release);
            task.notify();
        }
    }
    fn is_finished(&self) -> bool {
        self.inner.slot.lock().finished
    }
}
impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut s = self.inner.slot.lock();
        if s.finished {
            let r = s.result.take();
            drop(s);
            return Poll::Ready(r.expect("JoinHandle polled after it returned the result"));
        }
        let old = s.waker.replace(cx.waker().clone());
        drop(s);
        drop(old); // dropping a waker can drop a whole task, so never under a lock
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
        fut: Some(Box::pin(fut)),
        join: Some(inner.clone()),
    };
    let task = Task::new(Box::pin(h), sched.clone());
    let handle = JoinHandle {
        inner,
        task: Arc::downgrade(&task),
    };
    // On a closed executor `schedule` drops the only strong reference: never polled, and the handle says Cancelled.
    sched
        .owned()
        .live
        .lock()
        .insert(task.id, handle.task.clone());
    sched.schedule(task);
    handle
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
        // a swap: with two wakers, a second plain store would hide what the first one published
        self.flag.swap(true, AcqRel);
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
    owned: OwnedTasks,
}
impl Scheduler for LocalQueue {
    fn schedule(&self, task: Arc<Task>) {
        self.owned
            .push_if_open(task, |t| self.q.lock().push_back(t));
        self.cv.notify_one();
    }
    fn owned(&self) -> &OwnedTasks {
        &self.owned
    }
}
struct LocalRootWake(Arc<LocalQueue>);
impl Wake for LocalRootWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.root.swap(true, SeqCst); // a swap for the same reason as in ParkWake
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
            owned: OwnedTasks::default(),
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
        self.timer.advance(d);
    }
    fn pop(&self) -> Option<Arc<Task>> {
        self.queue.q.lock().pop_front()
    }
    /// Runs the tasks that are queued now, once each.
    fn run_until_stalled_once(&self) {
        let _g = enter(&self.handle);
        let queued = self.queue.q.lock().len();
        for t in (0..queued).map_while(|_| self.pop()) {
            t.run();
        }
    }
    /// Runs every ready task and every due timer until nothing more can make progress without time passing.
    fn run_until_stalled(&self) -> usize {
        let _g = enter(&self.handle);
        let mut total = 0;
        loop {
            let fired = self.timer.fire_due();
            // only what is queued now: a task that keeps yielding must not keep the timers waiting
            let mut ran = 0;
            let queued = self.queue.q.lock().len();
            for t in (0..queued).map_while(|_| self.pop()) {
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
            let queued = self.queue.q.lock().len();
            for t in (0..queued).map_while(|_| self.pop()) {
                t.run();
                ran = true;
                if self.queue.root.load(SeqCst) {
                    break; // let the root future make progress promptly
                }
            }
            if ran || self.queue.root.load(SeqCst) {
                continue;
            }
            let mut g = self.queue.q.lock();
            if g.is_empty() && !self.queue.root.load(SeqCst) {
                let now = self.timer.now();
                match self.timer.next_deadline() {
                    Some(d) if d <= now => {} // already due: go round and fire it
                    Some(d) if matches!(self.timer.clock, Clock::Real(_)) => {
                        self.queue
                            .cv
                            .wait_for(&mut g, Duration::from_nanos(d - now));
                    }
                    // a manual clock only moves through `Timer::advance`, which wakes whoever is due
                    _ => self.queue.cv.wait(&mut g),
                }
            }
        }
    }
}
impl Drop for LocalRuntime {
    fn drop(&mut self) {
        let _g = enter(&self.handle); // destructors may spawn
        self.queue.owned.close();
        self.queue.owned.cancel_all();
        let queued = std::mem::take(&mut *self.queue.q.lock());
        drop(queued);
        self.timer.clear();
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
    owned: OwnedTasks,
    /// Test hook, `None` outside one check: called while tasks taken from the injector are in neither queue.
    in_transit: Option<Hook>,
}
type Hook = Box<dyn Fn(&MtShared) + Send + Sync>;
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
    fn take_injected(&self, local: &Worker<Arc<Task>>) -> Steal<Arc<Task>> {
        let Some(hook) = &self.in_transit else {
            return self.injector.steal_batch_and_pop(local);
        };
        // a batch steal split in two, so a check can act in the gap that every batch steal has
        let held = Worker::new_fifo();
        let first = self.injector.steal_batch_and_pop(&held);
        if !held.is_empty() {
            hook(self);
            while let Some(t) = held.pop() {
                local.push(t);
            }
        }
        first
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
                match self.take_injected(local) {
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
        self.owned.push_if_open(task, |task| {
            let leftover = WORKER.with(|w| match w.borrow().as_ref() {
                Some(ctx) if ctx.shared_id == id => {
                    ctx.local.push(task); // own deque: no queue lock
                    None
                }
                _ => Some(task),
            });
            if let Some(t) = leftover {
                self.injector.push(t);
            }
        });
        self.wake_one();
    }
    fn owned(&self) -> &OwnedTasks {
        &self.owned
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
    // stop after the poll in progress; `Runtime::drop` destroys whatever is left, queued or waiting
    while !shared.shutdown.load(SeqCst) {
        tick += 1;
        if let Some(task) = shared.find_task(idx, tick) {
            // This worker is about to be busy, perhaps for long. Others may have looked while tasks were being moved
            // (in neither queue) and gone to sleep, so if any work is still queued, wake one of them.
            if shared.has_work() {
                shared.wake_one();
            }
            shared.executed[idx].fetch_add(1, Relaxed);
            task.run();
            continue;
        }
        let mut g = shared.lock.lock();
        shared.sleepers.fetch_add(1, SeqCst);
        fence(SeqCst);
        if shared.has_work() || shared.shutdown.load(SeqCst) {
            shared.sleepers.fetch_sub(1, SeqCst);
            continue;
        }
        // no timeout: a lost wake-up must be fixed, not covered up
        shared.cv.wait(&mut g);
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
        Self::build(n, clock, None)
    }
    fn build(n: usize, clock: Clock, in_transit: Option<Hook>) -> Runtime {
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
            owned: OwnedTasks::default(),
            in_transit,
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
        self.shared.owned.close(); // from here on nothing is queued, and a spawn is cancelled at once
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
        // no thread runs tasks now, so every remaining future can be destroyed here, with the runtime current
        let _current = enter(&self.handle);
        self.shared.owned.cancel_all();
        // queued references would keep each task and the scheduler pointing at each other
        let queues = std::iter::once(None).chain(self.shared.stealers.iter().map(Some));
        for queue in queues {
            loop {
                match queue.map_or_else(|| self.shared.injector.steal(), |s| s.steal()) {
                    Steal::Success(t) => drop(t),
                    Steal::Retry => {}
                    Steal::Empty => break,
                }
            }
        }
        self.timer.clear();
    }
}

// =====================================================================================================================
// 3. TIME: injectable clock, timer ordered by deadline, sleep / timeout / interval
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
    waker: Mutex<Option<Waker>>,
}
/// (deadline, sequence number): the map's order is firing order, equal deadlines first come first served.
type TimerKey = (u64, u64);
struct TimerInner {
    pending: BTreeMap<TimerKey, Arc<TimerState>>,
    seq: u64,
}
/// A map rather than a heap, so that dropping a `Sleep` can remove its entry at once.
/// No `TimerState` is dropped while `inner` is locked: its waker may own a task whose destructor cancels a sleep.
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
                pending: BTreeMap::new(),
                seq: 0,
            }),
            cv: Condvar::new(),
        })
    }
    fn now(&self) -> u64 {
        self.clock.now()
    }
    /// Moves a manual clock and wakes what became due, so that a `block_on` asleep on another thread hears of it.
    fn advance(&self, d: Duration) {
        self.clock.advance(d);
        self.fire_due();
    }
    fn insert(&self, deadline: u64, st: Arc<TimerState>) -> TimerKey {
        let mut g = self.inner.lock();
        g.seq += 1;
        let key = (deadline, g.seq);
        g.pending.insert(key, st);
        self.cv.notify_one(); // the driver may be sleeping towards a later deadline
        key
    }
    fn cancel(&self, key: TimerKey) {
        let removed = self.inner.lock().pending.remove(&key);
        drop(removed); // after the lock is released
    }
    fn pending(&self) -> usize {
        self.inner.lock().pending.len()
    }
    fn clear(&self) {
        let all = std::mem::take(&mut self.inner.lock().pending);
        drop(all);
    }
    fn take_due(g: &mut TimerInner, now: u64) -> Vec<Arc<TimerState>> {
        let mut due = Vec::new();
        while let Some(entry) = g.pending.first_entry() {
            if entry.key().0 > now {
                break;
            }
            let st = entry.remove();
            st.fired.store(true, SeqCst);
            due.push(st);
        }
        due
    }
    /// Call without any lock held: wake() may re-enter the scheduler and the timer.
    fn wake(due: Vec<Arc<TimerState>>) -> usize {
        let mut woken = 0;
        for st in due {
            let waker = st.waker.lock().take();
            if let Some(w) = waker {
                w.wake();
                woken += 1;
            }
        }
        woken
    }
    /// Fires everything due at the clock's current time (in deadline order); returns how many wakers were woken.
    fn fire_due(&self) -> usize {
        let due = Self::take_due(&mut self.inner.lock(), self.now());
        Self::wake(due)
    }
    fn next_deadline(&self) -> Option<u64> {
        self.inner.lock().pending.keys().next().map(|k| k.0)
    }
    fn run_driver(&self, stop: &AtomicBool) {
        let mut g = self.inner.lock();
        while !stop.load(SeqCst) {
            let now = self.now();
            let due = Self::take_due(&mut g, now);
            if !due.is_empty() {
                drop(g);
                Self::wake(due);
                g = self.inner.lock();
                continue;
            }
            match g.pending.keys().next().map(|k| k.0) {
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
    /// Where the timer keeps this sleep, once it has been polled.
    key: Option<TimerKey>,
}
impl Sleep {
    fn new(timer: Arc<Timer>, deadline: u64) -> Sleep {
        let st = Arc::new(TimerState {
            fired: AtomicBool::new(false),
            waker: Mutex::new(None),
        });
        Sleep {
            timer,
            deadline,
            st,
            key: None,
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
            let old = if w.as_ref().is_some_and(|old| old.will_wake(cx.waker())) {
                None
            } else {
                w.replace(cx.waker().clone())
            };
            drop(w);
            drop(old);
        }
        if self.key.is_none() {
            let key = self.timer.insert(self.deadline, self.st.clone());
            self.key = Some(key);
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
        if let Some(key) = self.key.take() {
            self.timer.cancel(key); // a no-op if it has fired
        }
    }
}
fn nanos(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}
fn now_ns() -> u64 {
    current().timer.now()
}
fn sleep_until(deadline_ns: u64) -> Sleep {
    Sleep::new(current().timer, deadline_ns)
}
fn sleep(d: Duration) -> Sleep {
    let t = current().timer;
    let deadline = t.now().saturating_add(nanos(d)); // sleep(Duration::MAX) means "for ever"
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
        period: nanos(period),
        next,
    }
}
impl Interval {
    /// First tick is immediate; missed ticks are delivered back-to-back (a "burst" policy, like tokio's default).
    async fn tick(&mut self) -> u64 {
        Sleep::new(self.timer.clone(), self.next).await;
        let t = self.next;
        self.next = self.next.saturating_add(self.period);
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
        let unread = {
            let mut g = self.0.lock();
            g.rx_gone = true;
            (g.value.take(), g.rx_waker.take())
        };
        drop(unread); // a value nobody will read is dropped now, outside the lock
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
        let old = g.rx_waker.replace(cx.waker().clone());
        drop(g);
        drop(old);
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
        let (ws, queued, own) = {
            let mut g = self.0.st.lock();
            g.rx_closed = true;
            (
                std::mem::take(&mut g.tx_wakers),
                std::mem::take(&mut g.q),
                g.rx_waker.take(),
            )
        };
        // Senders keep the channel alive, so queued values must be dropped here: one may hold a reply channel whose
        // other end is waiting. Dropped outside the lock, because their destructors may use this channel.
        drop((queued, own));
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
            let old = g.rx_waker.replace(cx.waker().clone());
            drop(g);
            drop(old);
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
/// `notify_one` stores at most one permit, so notify-before-wait is not lost. Unlike tokio, which picks a waiter for each
/// call, this keeps one flag: two calls made before a waiter runs release one waiter only.
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
        assert!(t.a.is_some(), "Select2 polled after it returned");
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
#[derive(Default)]
struct Waiters {
    next_id: u64,
    list: Vec<(u64, Waker)>,
}
/// Reading or writing: how many readiness edges there have been, and who waits for the next one.
#[derive(Default)]
struct Direction {
    epoch: AtomicU64,
    waiters: Mutex<Waiters>,
}
impl Direction {
    /// An edge arrived. Wake everyone: each tries its operation, and whoever gets nothing registers again.
    fn ready(&self) {
        self.epoch.fetch_add(1, SeqCst); // bump BEFORE taking the wakers (see poll_io)
        let woken = std::mem::take(&mut self.waiters.lock().list);
        for (_, waker) in woken {
            waker.wake();
        }
    }
}
/// One operation's place among the waiters of a direction. Dropping the operation's future takes it out again.
struct Waiter<'a> {
    dir: &'a Direction,
    id: Option<u64>,
}
impl<'a> Waiter<'a> {
    fn new(dir: &'a Direction) -> Self {
        Waiter { dir, id: None }
    }
    fn register(&mut self, waker: &Waker) {
        let mut g = self.dir.waiters.lock();
        let id = match self.id {
            Some(id) => id,
            None => {
                g.next_id += 1;
                *self.id.insert(g.next_id)
            }
        };
        let old = match g.list.iter_mut().find(|entry| entry.0 == id) {
            Some(entry) => Some(std::mem::replace(&mut entry.1, waker.clone())),
            None => {
                g.list.push((id, waker.clone()));
                None
            }
        };
        drop(g);
        drop(old);
    }
}
impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            let removed = {
                let mut g = self.dir.waiters.lock();
                let at = g.list.iter().position(|entry| entry.0 == id);
                at.map(|at| g.list.swap_remove(at))
            };
            drop(removed);
        }
    }
}
#[derive(Default)]
struct IoState {
    token: usize,
    read: Direction,
    write: Direction,
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
            ..IoState::default()
        });
        // listed before it is registered, so that the first event cannot miss it
        self.sources.lock().insert(token, st.clone());
        let interest = Interest::READABLE | Interest::WRITABLE;
        if let Err(e) = self.registry.register(&mut src, Token(token), interest) {
            self.sources.lock().remove(&token);
            return Err(e);
        }
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
                    st.read.ready();
                }
                if ev.is_writable() || ev.is_write_closed() || ev.is_error() {
                    st.write.ready();
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
/// The edge-triggered recipe: (1) remember the epoch, (2) TRY the operation, (3) on WouldBlock publish the waker, (4) if the
/// epoch moved in the meantime an edge was delivered after our syscall: retry instead of sleeping on a missed event.
fn poll_io<R>(
    waiter: &mut Waiter<'_>,
    cx: &mut Context<'_>,
    mut f: impl FnMut() -> io::Result<R>,
) -> Poll<io::Result<R>> {
    loop {
        let e = waiter.dir.epoch.load(SeqCst);
        match f() {
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                waiter.register(cx.waker());
                if waiter.dir.epoch.load(SeqCst) != e {
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
        let mut waiter = Waiter::new(&self.io.st.read);
        let (s, a) = poll_fn(|cx| poll_io(&mut waiter, cx, || self.io.src.accept())).await?;
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
        let mut waiter = Waiter::new(&io.st.write);
        poll_fn(|cx| {
            poll_io(&mut waiter, cx, || {
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
        drop(waiter);
        Ok(TcpStream { io: Arc::new(io) })
    }
    async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut waiter = Waiter::new(&self.io.st.read);
        poll_fn(|cx| poll_io(&mut waiter, cx, || (&self.io.src).read(buf))).await
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
            let mut waiter = Waiter::new(&self.io.st.write);
            off += poll_fn(|cx| poll_io(&mut waiter, cx, || (&self.io.src).write(&buf[off..])))
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
    // sleep(0) and past deadlines complete immediately; a cancelled sleep leaves the timer at once
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
        // some ran; `Runtime::drop` destroyed the rest (the live-task count at the end of main proves it)
        assert!(counter.load(SeqCst) <= 1000);
    }
    assert_eq!(LIVE_THREADS.load(SeqCst), before, "leaked runtime threads");
    rep.add(
        "clean shutdown",
        "all worker/timer/reactor threads joined after drop".into(),
    );
}

// ---- one check per ordering, cleanup and wake-up bug. Each drives the schedule itself or through a hook: no sleeps. ----

/// Turns a hang into a failure. The limit only matters when a check is failing.
fn within(what: &str, f: impl FnOnce() + Send + 'static) {
    let (tx, rx) = std::sync::mpsc::channel();
    let t = thread::spawn(move || {
        f();
        let _ = tx.send(());
    });
    if rx.recv_timeout(Duration::from_secs(20)) == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
        panic!("{what}: hung");
    }
    t.join().unwrap_or_else(|p| std::panic::resume_unwind(p));
}

/// A waker that runs a closure, so a check can look at the world at the exact moment of a wake.
struct OnWake<F: Fn() + Send + Sync>(F);
impl<F: Fn() + Send + Sync + 'static> Wake for OnWake<F> {
    fn wake(self: Arc<Self>) {
        (self.0)()
    }
}
fn on_wake(f: impl Fn() + Send + Sync + 'static) -> Waker {
    Waker::from(Arc::new(OnWake(f)))
}
fn counting_waker() -> (Arc<AtomicUsize>, Waker) {
    let n = Arc::new(AtomicUsize::new(0));
    let m = n.clone();
    (
        n,
        on_wake(move || {
            m.fetch_add(1, SeqCst);
        }),
    )
}
fn poll_with<F: Future + Unpin>(f: &mut F, waker: &Waker) -> Poll<F::Output> {
    Pin::new(f).poll(&mut Context::from_waker(waker))
}

/// A future that is ready (or not) at once and whose destructor does what the check says.
struct Probe<D: FnMut()> {
    ready: bool,
    on_drop: D,
}
impl<D: FnMut() + Unpin> Future for Probe<D> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.ready {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
impl<D: FnMut()> Drop for Probe<D> {
    fn drop(&mut self) {
        (self.on_drop)()
    }
}

fn t_join_after_cleanup(rep: &mut Report) {
    for finishes in [true, false] {
        let rt = LocalRuntime::new(Clock::manual());
        let gone = Arc::new(AtomicBool::new(false));
        let g = gone.clone();
        let mut h = rt.spawn(Probe {
            ready: finishes,
            on_drop: move || g.store(true, SeqCst),
        });
        // 0 = not woken, 1 = woken while the future still existed, 2 = woken after it was destroyed
        let seen = Arc::new(AtomicU8::new(0));
        let (s, g) = (seen.clone(), gone.clone());
        let waker = on_wake(move || s.store(1 + g.load(SeqCst) as u8, SeqCst));
        assert!(poll_with(&mut h, &waker).is_pending());
        rt.run_until_stalled();
        if !finishes {
            h.abort();
            rt.run_until_stalled();
        }
        assert_eq!(
            seen.load(SeqCst),
            2,
            "the joiner was told before the future was destroyed"
        );
    }
    rep.add(
        "join after cleanup",
        "finished and aborted: destroyed, then the joiner is woken".into(),
    );
}

/// An output whose destructor panics.
struct BoomOnDrop;
impl Drop for BoomOnDrop {
    fn drop(&mut self) {
        panic!("boom in a destructor");
    }
}

fn t_panicking_destructors(rep: &mut Report) {
    let rt = LocalRuntime::new(Clock::manual());
    let boom = || panic!("boom in a destructor");
    let finished = rt.spawn(Probe {
        ready: true,
        on_drop: boom,
    });
    let aborted = rt.spawn(Probe {
        ready: false,
        on_drop: boom,
    });
    let unwakeable = rt.spawn(Probe {
        ready: false,
        on_drop: boom,
    });
    drop(rt.spawn(async { BoomOnDrop })); // nobody takes this output
    aborted.abort();
    let payload = rt.spawn(async { std::panic::panic_any(BoomOnDrop) });
    let escaped = catch_unwind(AssertUnwindSafe(|| rt.run_until_stalled()));
    assert!(escaped.is_ok(), "a destructor's panic reached the executor");
    let want = Err(JoinError::Panic("boom in a destructor".into()));
    for h in [finished, aborted, unwakeable] {
        assert!(h.is_finished());
        assert_eq!(rt.block_on(h), want);
    }
    assert!(matches!(rt.block_on(payload), Err(JoinError::Panic(_))));
    rep.add(
        "panicking destructors",
        "caught on finish, abort, free, unread output and panic payload".into(),
    );
}

/// Spawns another task from its destructor.
struct SpawnOnDrop(Arc<AtomicUsize>);
impl Drop for SpawnOnDrop {
    fn drop(&mut self) {
        let inner = DropGuard(self.0.clone());
        drop(spawn(async move {
            let _g = inner;
            std::future::pending::<()>().await;
        }));
    }
}

fn check_shutdown_of(spawner: Handle, shut_down: impl FnOnce()) {
    let drops = Arc::new(AtomicUsize::new(0));
    let (started_tx, started) = std::sync::mpsc::channel();
    let (tx, rx) = oneshot::<()>(); // the check keeps the sender: someone else still holds this task's waker
    let (g, child) = (DropGuard(drops.clone()), SpawnOnDrop(drops.clone()));
    let mut waiting = spawner.spawn(async move {
        let _held = (g, child);
        started_tx.send(()).unwrap();
        let _ = rx.await;
    });
    let g = DropGuard(drops.clone());
    let mut sleeping = spawner.spawn(async move {
        let _g = g;
        sleep(Duration::from_secs(3600)).await;
    });
    let g = DropGuard(drops.clone());
    let mut spinning = spawner.spawn(async move {
        let _g = g;
        loop {
            yield_now().await;
        }
    });
    // on a local executor nothing has run yet, and `shut_down` first lets it run
    shut_down();
    let _ = started.try_recv();
    // three guards, and the child that a destructor spawned during the shutdown
    assert_eq!(drops.load(SeqCst), 4, "a future survived the shutdown");
    let (_, w) = counting_waker();
    for h in [&mut waiting, &mut sleeping, &mut spinning] {
        assert_eq!(poll_with(h, &w), Poll::Ready(Err(JoinError::Cancelled)));
    }
    let g = DropGuard(drops.clone());
    let mut late = spawner.spawn(async move {
        let _g = g;
        unreachable!("polled after shutdown");
    });
    assert_eq!(
        drops.load(SeqCst),
        5,
        "a task spawned after shutdown was kept"
    );
    assert_eq!(
        poll_with(&mut late, &w),
        Poll::Ready(Err::<(), _>(JoinError::Cancelled))
    );
    assert!(tx.send(()).is_err());
}

fn t_shutdown_strands_nothing(rep: &mut Report) {
    within("shutdown of a runtime with threads", || {
        let rt = Runtime::new(2);
        check_shutdown_of(rt.handle.clone(), move || drop(rt));
    });
    within("shutdown of a local runtime", || {
        let rt = LocalRuntime::new(Clock::manual());
        check_shutdown_of(rt.handle.clone(), move || {
            rt.run_until_stalled_once();
            drop(rt);
        });
    });
    // a handle alone does not keep a task alive that nobody can wake
    let rt = LocalRuntime::new(Clock::manual());
    let drops = Arc::new(AtomicUsize::new(0));
    let g = DropGuard(drops.clone());
    let h = rt.spawn(async move {
        let _g = g;
        std::future::pending::<()>().await;
    });
    rt.run_until_stalled();
    assert_eq!(
        drops.load(SeqCst),
        1,
        "an unwakeable task was kept by its handle"
    );
    assert_eq!(rt.block_on(h), Err(JoinError::Cancelled));
    rep.add(
        "shutdown",
        "waiting, sleeping, yielding and late tasks destroyed; handles say Cancelled".into(),
    );
}

fn t_two_waiters(rep: &mut Report) {
    within("two acceptors", || {
        let rt = Runtime::new(1);
        let _g = enter(&rt.handle);
        let listener = TcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let wakers: Vec<Waker> = (0..2)
            .map(|i| {
                let tx = tx.clone();
                on_wake(move || {
                    let _ = tx.send(i);
                })
            })
            .collect();
        let mut first = Box::pin(listener.accept());
        let mut second = Box::pin(listener.accept());
        assert!(poll_with(&mut first, &wakers[0]).is_pending());
        assert!(poll_with(&mut second, &wakers[1]).is_pending());
        let _client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let mut woken = [false; 2];
        while woken != [true; 2] {
            let i = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("an acceptor was forgotten: not every waiter was woken");
            woken[i] = true;
        }
    });
    // each waiter has one entry, however often it is polled, and cancelling removes it
    let st = IoState::default();
    let (a_woken, a) = counting_waker();
    let (b_woken, b) = counting_waker();
    let would_block = || Err::<(), _>(io::Error::from(io::ErrorKind::WouldBlock));
    let mut first = Waiter::new(&st.read);
    let mut second = Waiter::new(&st.read);
    for _ in 0..3 {
        assert!(poll_io(&mut first, &mut Context::from_waker(&a), would_block).is_pending());
        assert!(poll_io(&mut second, &mut Context::from_waker(&b), would_block).is_pending());
    }
    assert_eq!(st.read.waiters.lock().list.len(), 2);
    drop(second);
    assert_eq!(
        st.read.waiters.lock().list.len(),
        1,
        "a cancelled waiter stayed"
    );
    st.read.ready();
    assert_eq!((a_woken.load(SeqCst), b_woken.load(SeqCst)), (1, 0));
    rep.add(
        "two waiters, one socket",
        "both acceptors woken; a cancelled wait leaves no entry".into(),
    );
}

fn t_cancelled_timers(rep: &mut Report) {
    let rt = LocalRuntime::new(Clock::manual());
    let _g = enter(&rt.handle);
    let (_, w) = counting_waker();
    for i in 1..=1000 {
        let mut s = sleep(Duration::from_secs(i));
        assert!(poll_with(&mut s, &w).is_pending());
    }
    assert_eq!(
        rt.timer.pending(),
        0,
        "cancelled sleeps stayed in the timer"
    );
    let mut forever = sleep(Duration::MAX);
    assert!(poll_with(&mut forever, &w).is_pending());
    assert_eq!(rt.timer.pending(), 1);
    drop(forever);
    assert_eq!(rt.timer.pending(), 0);
    rep.add(
        "cancelled timers",
        "1000 dropped sleeps leave the timer empty".into(),
    );
}

fn t_manual_advance_wakes(rep: &mut Report) {
    let rt = LocalRuntime::new(Clock::manual());
    {
        let _g = enter(&rt.handle);
        let (woken, w) = counting_waker();
        let mut s = sleep(Duration::from_secs(3600));
        assert!(poll_with(&mut s, &w).is_pending());
        rt.advance(Duration::from_secs(3600));
        assert_eq!(woken.load(SeqCst), 1, "advancing the clock woke nobody");
    }
    within("advance from another thread", || {
        let rt = LocalRuntime::new(Clock::manual());
        let timer = rt.timer.clone();
        let (polled_tx, polled) = std::sync::mpsc::channel();
        let advancer = thread::spawn(move || {
            polled.recv().unwrap();
            timer.advance(Duration::from_secs(3600));
        });
        rt.block_on(async {
            let mut s = sleep(Duration::from_secs(3600));
            poll_fn(|cx| {
                let r = Pin::new(&mut s).poll(cx);
                let _ = polled_tx.send(());
                r
            })
            .await;
        });
        advancer.join().unwrap();
    });
    rep.add(
        "manual clock",
        "advance wakes sleepers, also a block_on on another thread".into(),
    );
}

fn t_yielding_cannot_starve_timers(rep: &mut Report) {
    let rt = LocalRuntime::new(Clock::Real(Instant::now()));
    let done = Arc::new(AtomicBool::new(false));
    let d = done.clone();
    // gives up after a long while only so that a failure is not a hang
    let spinner = rt.spawn(async move {
        let started = Instant::now();
        while !d.load(SeqCst) {
            if started.elapsed() > Duration::from_secs(10) {
                return false;
            }
            yield_now().await;
        }
        true
    });
    let seen = rt.block_on(async move {
        spawn(async move {
            sleep(Duration::from_millis(1)).await;
            done.store(true, SeqCst);
        });
        spinner.await
    });
    assert_eq!(
        seen,
        Ok(true),
        "a task that keeps yielding kept the timer from firing"
    );
    rep.add(
        "yield loop vs timer",
        "the sleep fired while another task kept yielding".into(),
    );
}

/// While worker X moves a batch from the injector, the other tasks are in no queue. The hook makes worker Y look exactly
/// then and go to sleep. X then runs a task that blocks until another of the batch has run.
fn t_batch_wake(rep: &mut Report) {
    within("batch wake-up", || {
        let (release_y, y_released) = std::sync::mpsc::channel::<()>();
        let release_y = Mutex::new(release_y);
        let armed = Arc::new(AtomicBool::new(false));
        let a = armed.clone();
        let hook: Hook = Box::new(move |shared: &MtShared| {
            if !a.swap(false, SeqCst) {
                return;
            }
            release_y.lock().send(()).unwrap();
            // Y counts itself under the lock and releases it only by waiting, so seeing 1 while holding it means asleep
            loop {
                let _g = shared.lock.lock();
                if shared.sleepers.load(SeqCst) == 1 {
                    break;
                }
                drop(_g);
                thread::yield_now();
            }
        });
        let rt = Runtime::build(2, Clock::Real(Instant::now()), Some(hook));
        // keep both workers busy while the batch is put into the injector
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release_x, x_released) = std::sync::mpsc::channel::<()>();
        let blockers: Vec<_> = [x_released, y_released]
            .into_iter()
            .map(|wait| {
                let started = started_tx.clone();
                rt.spawn(async move {
                    started.send(()).unwrap();
                    wait.recv().unwrap();
                })
            })
            .collect();
        started.recv().unwrap();
        started.recv().unwrap();
        const OTHERS: usize = 7;
        let (ran_tx, ran) = std::sync::mpsc::channel();
        // its thread stays blocked until the rest of the batch has run, which needs the other worker
        let first = rt.spawn(async move {
            (0..OTHERS).all(|_| ran.recv_timeout(Duration::from_secs(5)).is_ok())
        });
        for _ in 0..OTHERS {
            let ran = ran_tx.clone();
            rt.spawn(async move {
                let _ = ran.send(());
            });
        }
        armed.store(true, SeqCst);
        release_x.send(()).unwrap();
        let all_ran = rt.block_on(first);
        assert!(
            !armed.load(SeqCst),
            "no batch was moved, so the check tested nothing"
        );
        assert_eq!(
            all_ran,
            Ok(true),
            "a worker slept while tasks of a moved batch were waiting"
        );
        for b in blockers {
            assert_eq!(rt.block_on(b), Ok(()));
        }
    });
    rep.add(
        "batch wake-up",
        "a worker that slept during a batch move is woken again".into(),
    );
}

fn t_dropped_receiver_drops_the_queue(rep: &mut Report) {
    let rt = LocalRuntime::new(Clock::manual());
    let (tx, rx) = mpsc_channel::<OsSender<u8>>(1);
    let (reply_tx, mut reply) = oneshot::<u8>();
    assert!(rt.block_on(tx.send(reply_tx)).is_ok());
    drop(rx);
    let (_, w) = counting_waker();
    assert_eq!(
        poll_with(&mut reply, &w),
        Poll::Ready(Err(RecvError)),
        "the request is still queued"
    );
    rep.add(
        "dropped receiver",
        "queued requests are destroyed, so their reply channels close".into(),
    );
}

/// A replaced waker may own the last reference to something that locks the same channel when dropped.
fn t_wakers_dropped_outside_locks(rep: &mut Report) {
    within("a replaced waker is dropped", || {
        let (tx, mut rx) = mpsc_channel::<u8>(1);
        let rt = LocalRuntime::new(Clock::manual());
        let mut recv = Box::pin(rx.recv());
        {
            let owned = tx.clone();
            let waker = on_wake(move || {
                let _ = &owned;
            });
            assert!(poll_with(&mut recv, &waker).is_pending());
        }
        // replacing it drops a sender of this channel
        let (_, w) = counting_waker();
        assert!(poll_with(&mut recv, &w).is_pending());
        drop(rt);
    });
    rep.add(
        "wakers and locks",
        "replacing a channel's waker cannot deadlock".into(),
    );
}

/// Needs a model of weak memory to be able to fail: run with Miri. A waker publishes data with a relaxed store and then
/// wakes; the poll that follows the wake must see it. Natively this only checks that nothing is stuck.
fn t_wake_publishes(rep: &mut Report) {
    const N: usize = if cfg!(miri) { 2000 } else { 200 };
    let queue = Arc::new(LocalQueue {
        q: Mutex::new(VecDeque::new()),
        cv: Condvar::new(),
        root: AtomicBool::new(false),
        owned: OwnedTasks::default(),
    });
    let sched: Arc<dyn Scheduler> = queue.clone();
    let run_all = || {
        loop {
            let t = queue.q.lock().pop_front();
            match t {
                Some(t) => t.run(),
                None => break,
            }
        }
    };
    let mut lost = 0;
    for _ in 0..N {
        let data = Arc::new(AtomicUsize::new(0));
        let d = data.clone();
        let handle = spawn_on(
            &sched,
            poll_fn(move |_| match d.load(Relaxed) {
                1 => Poll::Ready(()),
                _ => Poll::Pending,
            }),
        );
        // hold a waker so that the pending task stays alive, then race its wake against the first run
        let task = handle.task.upgrade().expect("queued");
        let waker = Waker::from(task);
        let publisher = thread::spawn(move || {
            data.store(1, Relaxed);
            waker.wake_by_ref();
            waker
        });
        run_all();
        let waker = publisher.join().unwrap();
        run_all();
        if !handle.is_finished() {
            lost += 1;
        }
        drop(waker);
    }
    assert_eq!(
        lost, 0,
        "wakes whose data the following poll did not see, out of {N}"
    );
    rep.add(
        "wake publishes",
        format!("{N} racing wakes of a queued or running task, none lost"),
    );
}

/// The same for `block_on` with two wakers: it must see what BOTH published. A loss shows as a deadlock under Miri.
fn t_root_wake_publishes(rep: &mut Report) {
    const N: usize = if cfg!(miri) { 500 } else { 100 };
    let local = LocalRuntime::new(Clock::manual());
    for round in 0..N {
        let flags = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
        let mut threads = Vec::new();
        let f = flags.clone();
        let root = poll_fn(|cx| {
            if threads.is_empty() {
                for i in 0..2 {
                    let (f, w) = (f.clone(), cx.waker().clone());
                    threads.push(thread::spawn(move || {
                        f[i].store(1, Relaxed);
                        w.wake();
                    }));
                }
            }
            if f.iter().all(|x| x.load(Relaxed) == 1) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        });
        if round % 2 == 0 {
            park_block_on(&local.handle, root);
        } else {
            local.block_on(root);
        }
        for t in threads {
            t.join().unwrap();
        }
    }
    rep.add(
        "root wake publishes",
        format!("{N} block_on calls saw both wakers' data"),
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
        let expected = msg.as_deref().is_some_and(|m| m.starts_with("boom"));
        if !expected && !info.payload().is::<BoomOnDrop>() {
            default_hook(info); // only the deliberate "boom" panics are silenced
        }
    }));
    let mut rep = Report(Vec::new());
    println!("c09 mini async runtime");
    if cfg!(miri) {
        // Miri has no mio on every host, and these are the checks that need its weak memory model
        t_wake_publishes(&mut rep);
        t_root_wake_publishes(&mut rep);
        for (name, detail) in &rep.0 {
            println!("{name:<24} | {detail}");
        }
        return;
    }
    t_join_after_cleanup(&mut rep);
    t_panicking_destructors(&mut rep);
    t_shutdown_strands_nothing(&mut rep);
    t_two_waiters(&mut rep);
    t_cancelled_timers(&mut rep);
    t_manual_advance_wakes(&mut rep);
    t_yielding_cannot_starve_timers(&mut rep);
    t_batch_wake(&mut rep);
    t_dropped_receiver_drops_the_queue(&mut rep);
    t_wakers_dropped_outside_locks(&mut rep);
    t_wake_publishes(&mut rep);
    t_root_wake_publishes(&mut rep);
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
