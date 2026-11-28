//! RECIPE 442 (concurrency): structured concurrency on tokio: CancellationToken tree, TaskTracker, fail-fast Scope, shutdown ordering.
//! LEVEL: advanced
//! Use in: servers with graceful shutdown, fan-out/fan-in request handlers, anything that spawns tasks that must not leak.   Run: cargo run --release --example structured_concurrency_cancellation
#![allow(dead_code)]
//!
//! WHAT: three small building blocks written from scratch (the production versions are `tokio_util::sync::CancellationToken`,
//! `tokio_util::task::TaskTracker` and `tokio::task::JoinSet`), then composed:
//! 1. `CancellationToken`: a TREE. `cancel()` marks the token and every descendant; `cancelled().await` completes
//!    once cancelled (immediately if already); `child()` makes a token cancelled by its parent but not vice versa;
//!    `drop_guard()` cancels when dropped (so an early `return`/`?`/panic still signals the others).
//! 2. `TaskTracker`: counts spawned tasks through a guard owned by the task future, so the count drops even when a task
//!    panics or is aborted before its first poll. `close()` then `wait()` returns when closed AND count == 0.
//! 3. `Scope`: tasks spawned in a scope cannot outlive it. `join()` waits for all of them; the FIRST error or panic
//!    cancels the scope's token (fail-fast) so siblings stop at their next cancellation point; after cancellation
//!    there is a GRACE period, then stragglers are aborted (counted as `forced`). Dropping a scope without `join`
//!    aborts everything (JoinSet semantics), but an abort only asks: the tasks end a little later. Only a finished `join()` means
//!    that they are gone. Real borrowing scopes (like `std::thread::scope`) are impossible for
//!    async without unsafe, hence `'static` futures plus a runtime guarantee of joining.
//! SHUTDOWN ORDER (servers): (1) stop accepting new work, (2) cancel in-flight work cooperatively, (3) drain with a
//! DEADLINE, (4) abort whatever is still running. Skipping (1) lets new work arrive during the drain; skipping (4)
//! lets one stuck task hang the process forever. Abort only takes effect at an `.await`: a task spinning in a
//! synchronous loop, or blocked in a sync call, cannot be aborted: put cancellation points in long loops
//! (`token.is_cancelled()` + `yield_now().await`) and use `spawn_blocking` plus a flag for blocking work.
//! CLEANUP: `Drop` cannot be async. Do async cleanup in the cancelled branch (before returning) and keep a plain drop
//! guard as the backstop for abort/panic. Both are demonstrated and counted.
//!
//! CANCEL SAFETY of every await used here (cancel-safe = dropping the future loses nothing):
//! - `Notify::notified()`: safe, it only waits. Created BEFORE re-checking the flag so a `notify_waiters` between the
//!   check and the await is not missed (the future is registered at creation, not first poll).
//! - `JoinSet::join_next*`: safe, a completed task is returned by the next call. `mpsc::Receiver::recv`: safe.
//! - `tokio::time::sleep` / `timeout_at`: safe (timeout_at drops the inner future on expiry; that is our intent).
//! - `JoinHandle` await: dropping a `JoinHandle` DETACHES the task, it does not cancel it.
//! - A hand written `async fn` with several awaits is not cancel-safe if what it has taken so far lives only in its locals
//!   (see async_cancel_safety).
//! LIMITS of the token: a second `cancel()` racing the first may return before every descendant is marked; propagation (and
//! dropping a very long chain of child tokens) recurses; `child()` scans the parent's live children, so very many are slow.
//! A grace period near `Duration::MAX` overflows the deadline and panics.
//! COMMON USES: HTTP servers (stop the listener, let in-flight requests finish, then force close), Kubernetes style
//! SIGTERM handling, request fan-out with deadlines (first failure cancels the rest), test harnesses that must not leak
//! tasks between tests, supervisors in actor systems.
//! Read: tokio-util CancellationToken https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html and https://docs.rs/tokio-util
//! and Tokio tutorial "Graceful Shutdown" https://tokio.rs/tokio/topics/shutdown and Nathaniel J. Smith, "Notes on structured concurrency, or: Go statement considered harmful" (vorpus.org, 2018)
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::*};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::{Notify, mpsc};
use tokio::task::{AbortHandle, Id, JoinHandle, JoinSet, yield_now};
use tokio::time::{Instant, sleep, timeout, timeout_at};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// ================================================================ CancellationToken

struct TokenInner {
    cancelled: AtomicBool,
    notify: Notify,
    children: Mutex<Vec<Weak<TokenInner>>>,
    /// Keeps the chain up to the root alive: a parent only holds its children weakly, so without this, dropping a middle
    /// token's last handle would cut its descendants off from every later `cancel()` above it.
    _parent: Option<Arc<TokenInner>>,
}

#[derive(Clone)]
struct CancellationToken(Arc<TokenInner>);

impl CancellationToken {
    fn new() -> Self {
        Self::with_parent(None)
    }

    fn with_parent(parent: Option<Arc<TokenInner>>) -> Self {
        Self(Arc::new(TokenInner {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
            children: Mutex::new(Vec::new()),
            _parent: parent,
        }))
    }

    fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Acquire)
    }

    /// Cancels this token and all descendants. Idempotent.
    fn cancel(&self) {
        if self.0.cancelled.swap(true, AcqRel) {
            return; // someone else already did it (and is propagating)
        }
        self.0.notify.notify_waiters();
        let children = std::mem::take(&mut *self.0.children.lock().unwrap());
        for c in children.into_iter().filter_map(|w| w.upgrade()) {
            CancellationToken(c).cancel();
        }
    }

    /// A token cancelled when `self` is, but whose own cancellation does not touch `self`.
    fn child(&self) -> CancellationToken {
        let child = CancellationToken::with_parent(Some(self.0.clone()));
        {
            let mut kids = self.0.children.lock().unwrap();
            kids.retain(|w| w.strong_count() > 0); // prune dropped children so a long-lived root does not grow forever
            kids.push(Arc::downgrade(&child.0));
        }
        // Check AFTER registering: if the parent was cancelled concurrently it either sees our entry (and cancels us)
        // or we see its flag here. Either way no cancellation is lost.
        if self.is_cancelled() {
            child.cancel();
        }
        child
    }

    /// Completes when cancelled. CANCEL-SAFE: it holds no state a drop could lose.
    async fn cancelled(&self) {
        loop {
            let waiter = self.0.notify.notified(); // register before checking the flag
            if self.is_cancelled() {
                return;
            }
            waiter.await;
        }
    }

    fn drop_guard(self) -> DropGuard {
        DropGuard(Some(self))
    }
}

/// Cancels its token when dropped, unless disarmed.
struct DropGuard(Option<CancellationToken>);

impl DropGuard {
    fn disarm(mut self) -> CancellationToken {
        self.0.take().unwrap()
    }
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        if let Some(t) = &self.0 {
            t.cancel();
        }
    }
}

// ================================================================ TaskTracker

struct TrackerInner {
    count: AtomicUsize,
    closed: AtomicBool,
    notify: Notify,
}

#[derive(Clone)]
struct TaskTracker(Arc<TrackerInner>);

/// Lives inside the task's future: dropped on completion, panic, abort, or if the task never started.
struct TrackGuard(Arc<TrackerInner>);

impl Drop for TrackGuard {
    fn drop(&mut self) {
        if self.0.count.fetch_sub(1, AcqRel) == 1 {
            self.0.notify.notify_waiters();
        }
    }
}

impl TaskTracker {
    fn new() -> Self {
        Self(Arc::new(TrackerInner {
            count: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            notify: Notify::new(),
        }))
    }
    fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.count.fetch_add(1, AcqRel); // count BEFORE spawning so `wait` can never miss a task
        let guard = TrackGuard(self.0.clone());
        tokio::spawn(async move {
            let _guard = guard;
            fut.await
        })
    }
    fn close(&self) {
        self.0.closed.store(true, Release);
        self.0.notify.notify_waiters();
    }
    fn len(&self) -> usize {
        self.0.count.load(Acquire)
    }
    /// Returns once the tracker is closed and every task is gone. CANCEL-SAFE (only waits).
    async fn wait(&self) {
        loop {
            let waiter = self.0.notify.notified();
            if self.0.closed.load(Acquire) && self.len() == 0 {
                return;
            }
            waiter.await;
        }
    }
}

// ================================================================ Scope

#[derive(Debug, PartialEq)]
enum ScopeError {
    Failed { task: &'static str, msg: String },
    Panicked { task: &'static str },
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ScopeError::Failed { task, msg } => write!(f, "task `{task}` failed: {msg}"),
            ScopeError::Panicked { task } => write!(f, "task `{task}` panicked"),
        }
    }
}

#[derive(Debug)]
struct ScopeStats {
    /// Tasks that ignored cancellation past the grace period and were aborted.
    forced: usize,
}

struct Scope {
    token: CancellationToken,
    set: JoinSet<Result<(), String>>,
    names: HashMap<Id, &'static str>,
    grace: Duration,
}

impl Scope {
    fn new(grace: Duration) -> Self {
        Self::with_token(CancellationToken::new(), grace)
    }
    /// A nested scope: cancelling the parent token cancels this scope too.
    fn child_of(parent: &CancellationToken, grace: Duration) -> Self {
        Self::with_token(parent.child(), grace)
    }
    fn with_token(token: CancellationToken, grace: Duration) -> Self {
        Self {
            token,
            set: JoinSet::new(),
            names: HashMap::new(),
            grace,
        }
    }
    fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// The closure receives the scope's token so the task can observe cancellation.
    fn spawn<F, Fut>(&mut self, name: &'static str, f: F)
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let fut = f(self.token.clone());
        let handle = self.set.spawn(fut);
        self.names.insert(handle.id(), name);
    }

    fn record(
        &self,
        r: Result<(Id, Result<(), String>), tokio::task::JoinError>,
    ) -> Option<ScopeError> {
        match r {
            Ok((_, Ok(()))) => None,
            Ok((id, Err(msg))) => Some(ScopeError::Failed {
                task: self.names[&id],
                msg,
            }),
            Err(e) if e.is_panic() => Some(ScopeError::Panicked {
                task: self.names[&e.id()],
            }),
            Err(_) => None, // aborted by us: not an error of the task itself
        }
    }

    /// Waits for every task. First error/panic cancels the scope (fail-fast) and is returned after the rest are done.
    async fn join(mut self) -> Result<ScopeStats, ScopeError> {
        let (mut first_err, mut forced) = (None, 0);
        let mut deadline: Option<Instant> = None;
        while !self.set.is_empty() {
            let next = match deadline {
                // CANCEL-SAFE: join_next_with_id; timeout_at only drops that cancel-safe future.
                Some(d) => match timeout_at(d, self.set.join_next_with_id()).await {
                    Ok(n) => n,
                    Err(_) => {
                        self.set.abort_all();
                        while let Some(r) = self.set.join_next_with_id().await {
                            // Abort only asks. A task busy in blocking code finishes normally and is not counted.
                            forced += matches!(&r, Err(e) if e.is_cancelled()) as usize;
                            first_err = first_err.or(self.record(r)); // aborted tasks end quickly at their next await
                        }
                        break;
                    }
                },
                None => tokio::select! {
                    n = self.set.join_next_with_id() => n,
                    // External cancel (someone called token.cancel()): start the grace clock.
                    _ = self.token.cancelled() => {
                        deadline = Some(Instant::now() + self.grace);
                        continue;
                    }
                },
            };
            if let Some(r) = next {
                if let Some(e) = self.record(r) {
                    if first_err.is_none() {
                        first_err = Some(e);
                        self.token.cancel(); // fail fast: siblings see it at their next cancellation point
                        deadline.get_or_insert(Instant::now() + self.grace);
                    }
                }
            }
        }
        self.token.cancel(); // the scope is over: release anything that was waiting on its token
        match first_err {
            Some(e) => Err(e),
            None => Ok(ScopeStats { forced }),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        self.token.cancel(); // JoinSet's own Drop then aborts whatever is left
    }
}

// ================================================================ test helpers

/// Counts tasks currently alive (a guard lives inside each task future).
#[derive(Clone, Default)]
struct Live(Arc<AtomicUsize>);
struct LiveGuard(Arc<AtomicUsize>);
impl Live {
    fn enter(&self) -> LiveGuard {
        self.0.fetch_add(1, SeqCst);
        LiveGuard(self.0.clone())
    }
    fn now(&self) -> usize {
        self.0.load(SeqCst)
    }
    /// Abort/drop of a scope releases futures asynchronously: poll (bounded) until the count reaches zero.
    async fn wait_zero(&self) {
        let t = Instant::now();
        while self.now() != 0 {
            assert!(
                t.elapsed() < Duration::from_secs(5),
                "{} tasks leaked",
                self.now()
            );
            sleep(ms(1)).await;
        }
    }
}
impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, SeqCst);
    }
}

// The helpers below are counted from the moment their future is CREATED, not from its first poll: otherwise a task that
// leaked before it ever ran would not be seen by the leak checks.

/// Puts a guard into a future now.
fn counted<F: Future>(live: &Live, fut: F) -> impl Future<Output = F::Output> + use<F> {
    let alive = live.enter();
    async move {
        let _alive = alive;
        fut.await
    }
}

/// Runs until cancelled; wakes immediately on cancel (no polling delay).
fn polite_worker(token: CancellationToken, live: Live) -> impl Future<Output = Result<(), String>> {
    counted(&live, async move {
        // CANCEL-SAFE select: `cancelled()` and `sleep` are both safe to drop.
        tokio::select! {
            _ = token.cancelled() => Ok(()),
            _ = sleep(Duration::from_secs(10)) => Err("timed out waiting for cancel".into()),
        }
    })
}

/// Never looks at the token: only an abort can stop it.
fn stuck_worker(live: Live) -> impl Future<Output = Result<(), String>> {
    counted(&live, async {
        loop {
            sleep(Duration::from_secs(3600)).await; // an await point, so abort() works
        }
    })
}

/// Tiny seeded xorshift so the "random" cancel timings are reproducible.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

// ================================================================ tests

async fn test_token_tree() {
    let root = CancellationToken::new();
    let (a, b) = (root.child(), root.child());
    let grandchild = a.child();
    let waiter = tokio::spawn({
        let g = grandchild.clone();
        async move { g.cancelled().await }
    });
    a.cancel();
    assert!(a.is_cancelled() && grandchild.is_cancelled());
    assert!(
        !root.is_cancelled() && !b.is_cancelled(),
        "cancelling a child never touches parent or sibling"
    );
    timeout(Duration::from_secs(5), waiter)
        .await
        .expect("grandchild waiter woke")
        .unwrap();
    root.cancel();
    assert!(b.is_cancelled());
    // nobody keeps the middle token: its child must still hear the root
    let top = CancellationToken::new();
    let leaf = top.child().child();
    top.cancel();
    assert!(
        leaf.is_cancelled(),
        "dropping a middle token cut off its child"
    );
    assert!(
        root.child().is_cancelled(),
        "a child of a cancelled parent starts cancelled"
    );
    // Drop guard: leaving the scope early still signals.
    let t = CancellationToken::new();
    {
        let _g = t.clone().drop_guard();
    }
    assert!(t.is_cancelled());
    let t = CancellationToken::new();
    let g = t.clone().drop_guard();
    let _ = g.disarm();
    assert!(!t.is_cancelled(), "disarmed guard does nothing");
    println!(
        "1. token tree: parent->child->grandchild propagation, upward isolation, late child, drop guard OK"
    );
}

async fn test_tracker() {
    let (tracker, live) = (TaskTracker::new(), Live::default());
    for i in 0..100u64 {
        let live = live.clone();
        tracker.spawn(async move {
            let _a = live.enter();
            sleep(ms(i % 7)).await;
            if i == 13 {
                panic!("expected panic in tracked task 13");
            }
        });
    }
    // Not closed yet: wait() must NOT complete even though tasks will all finish (more could still be spawned).
    assert!(timeout(ms(100), tracker.wait()).await.is_err());
    tracker.close();
    timeout(Duration::from_secs(5), tracker.wait())
        .await
        .expect("all tasks done");
    assert_eq!(
        (tracker.len(), live.now()),
        (0, 0),
        "a panicking task is still counted out"
    );
    println!("2. tracker: 100 tasks (one panics), wait() blocks until close+drain, then returns");
}

async fn test_scope_no_outlive() {
    let live = Live::default();
    let mut scope = Scope::new(ms(500));
    for _ in 0..50 {
        let live = live.clone();
        scope.spawn("short", move |_| async move {
            let _a = live.enter();
            sleep(ms(5)).await;
            Ok(())
        });
    }
    let stats = scope.join().await.unwrap();
    assert_eq!(
        (live.now(), stats.forced),
        (0, 0),
        "join returns only after all 50 tasks are gone"
    );

    // Dropping the scope without join aborts the tasks.
    let mut scope = Scope::new(ms(500));
    for _ in 0..20 {
        let live = live.clone();
        scope.spawn("stuck", move |_| stuck_worker(live));
    }
    assert_eq!(live.now(), 20, "all are counted, started or not");
    yield_now().await;
    drop(scope);
    live.wait_zero().await;
    println!("3. scope: 50 tasks never outlive join(); a dropped scope aborts 20 stuck tasks");
}

async fn test_fail_fast() {
    let (live, cleaned) = (Live::default(), Arc::new(AtomicUsize::new(0)));
    let mut scope = Scope::new(Duration::from_secs(2));
    for _ in 0..7 {
        let (live, cleaned) = (live.clone(), cleaned.clone());
        scope.spawn("sibling", move |tok| async move {
            struct Cleanup(Arc<AtomicUsize>);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    self.0.fetch_add(1, SeqCst);
                }
            }
            let _c = Cleanup(cleaned);
            polite_worker(tok, live).await
        });
    }
    scope.spawn("bad", |_| async {
        sleep(ms(20)).await;
        Err("boom".to_string())
    });
    let t = Instant::now();
    let err = scope.join().await.unwrap_err();
    let took = t.elapsed();
    assert_eq!(
        err,
        ScopeError::Failed {
            task: "bad",
            msg: "boom".into()
        }
    );
    assert!(
        took < Duration::from_secs(2),
        "siblings were cancelled, not left to run their 10 s timers"
    );
    assert_eq!(
        (live.now(), cleaned.load(SeqCst)),
        (0, 7),
        "every sibling ran its cleanup"
    );
    println!(
        "4. fail-fast: `{err}`; 7 siblings cancelled and cleaned up in {took:?} (their timers were 10 s)"
    );
}

async fn test_deadline_force_abort() {
    let live = Live::default();
    let mut scope = Scope::new(ms(60));
    scope.spawn("stuck", {
        let live = live.clone();
        move |_| stuck_worker(live)
    });
    scope.spawn("polite", {
        let live = live.clone();
        move |tok| polite_worker(tok, live)
    });
    let token = scope.token();
    tokio::spawn(async move {
        sleep(ms(10)).await;
        token.cancel(); // external cancel starts the grace clock
    });
    let t = Instant::now();
    let stats = scope.join().await.unwrap();
    let took = t.elapsed();
    assert_eq!(stats.forced, 1, "only the stuck task needed the abort");
    assert!(
        took >= ms(60),
        "join waited out the grace period before aborting: {took:?}"
    );
    live.wait_zero().await;

    // A task blocked in synchronous code cannot be aborted: it runs to its end, and is not counted as forced.
    let mut scope = Scope::new(ms(20));
    let t = Instant::now();
    let (started, has_started) = tokio::sync::oneshot::channel();
    scope.spawn("blocking", |_| async {
        started.send(()).unwrap();
        std::thread::sleep(ms(150)); // no .await: abort has nowhere to take effect
        Ok(())
    });
    has_started.await.unwrap(); // so the abort below cannot land before the task's first poll
    scope.token().cancel();
    let stats = scope.join().await.unwrap();
    assert!(t.elapsed() >= ms(150), "join waited for the blocking task");
    assert_eq!(stats.forced, 0, "the blocking task finished by itself");
    println!(
        "5. deadline: stuck task ignored cancel, aborted after the 60 ms grace (join took {took:?}); a blocking task could not be aborted"
    );
}

async fn test_panic() {
    let live = Live::default();
    let mut scope = Scope::new(Duration::from_secs(2));
    scope.spawn("sibling", {
        let live = live.clone();
        move |tok| polite_worker(tok, live)
    });
    scope.spawn("crasher", |_| async {
        sleep(ms(5)).await;
        panic!("expected panic in scope task `crasher`");
    });
    let err = scope.join().await.unwrap_err();
    assert_eq!(err, ScopeError::Panicked { task: "crasher" });
    assert_eq!(live.now(), 0, "the sibling was cancelled by the panic");
    println!("6. panic: surfaced as `{err}` and cancelled the sibling");
}

async fn test_cancellation_points() {
    // Hot loop with a cancellation point every 1000 iterations: `is_cancelled` is a cheap atomic load, and
    // `yield_now` hands the thread back so abort/other tasks can run. Without it a CPU loop never reaches an await.
    let token = CancellationToken::new();
    let iterations = Arc::new(AtomicUsize::new(0));
    let worker = tokio::spawn({
        let (token, iterations) = (token.clone(), iterations.clone());
        async move {
            let mut x = 0u64;
            loop {
                for i in 0..1000u64 {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(i);
                }
                iterations.fetch_add(1, Relaxed);
                if token.is_cancelled() {
                    return x;
                }
                yield_now().await; // CANCEL-SAFE: nothing is held across it
            }
        }
    });
    sleep(ms(20)).await;
    token.cancel();
    timeout(Duration::from_secs(5), worker)
        .await
        .expect("loop stopped at a cancellation point")
        .unwrap();
    let n = iterations.load(Relaxed);
    assert!(n > 0);
    println!(
        "7. cancellation points: hot loop ran {n} chunks of 1000 then stopped within one chunk of cancel"
    );
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Conn {
    Quick(u64),
    Stuck,
}

/// Models a server: acceptor -> tracked connection handlers; returns the ordered shutdown events and the counters.
async fn run_server(conns: Vec<Conn>) -> (Vec<&'static str>, usize, usize) {
    let (root, accept_stop) = (CancellationToken::new(), CancellationToken::new());
    let (tracker, live) = (TaskTracker::new(), Live::default());
    let (done_ok, aborts) = (
        Arc::new(AtomicUsize::new(0)),
        Arc::new(Mutex::new(Vec::<AbortHandle>::new())),
    );
    let (tx, mut rx) = mpsc::channel::<Conn>(8);

    let acceptor = tokio::spawn({
        let (accept_stop, root, tracker, live, done_ok, aborts) = (
            accept_stop.clone(),
            root.clone(),
            tracker.clone(),
            live.clone(),
            done_ok.clone(),
            aborts.clone(),
        );
        async move {
            loop {
                // CANCEL-SAFE: both arms are (mpsc recv loses nothing when dropped). If both are ready, either may win.
                let conn = tokio::select! {
                    _ = accept_stop.cancelled() => break,
                    c = rx.recv() => match c { Some(c) => c, None => break },
                };
                let (tok, live, done_ok) = (root.child(), live.clone(), done_ok.clone());
                let h = tracker.spawn(async move {
                    let _alive = live.enter();
                    match conn {
                        Conn::Quick(work) => {
                            tokio::select! {
                                _ = sleep(ms(work)) => {}
                                // Cancelled: do the async flush now, before returning; Drop could not await.
                                _ = tok.cancelled() => sleep(ms(1)).await,
                            }
                            done_ok.fetch_add(1, SeqCst);
                        }
                        Conn::Stuck => stuck_worker(live.clone()).await.unwrap(),
                    }
                });
                aborts.lock().unwrap().push(h.abort_handle());
            }
        }
    });

    let accepted = conns.len();
    for c in conns {
        tx.send(c).await.unwrap();
    }
    while tx.capacity() != tx.max_capacity() {
        sleep(ms(1)).await; // the acceptor has taken every queued connection (so none is silently dropped)
    }
    sleep(ms(10)).await;

    // ---- the ordered shutdown ----
    let mut events = Vec::new();
    accept_stop.cancel(); // (1) stop accepting
    acceptor.await.unwrap();
    events.push("stop_accepting");
    drop(tx);
    tracker.close();
    root.cancel(); // (2) ask in-flight work to stop
    events.push("cancel");
    // (3) drain with a deadline
    match timeout(ms(100), tracker.wait()).await {
        Ok(()) => events.push("drained"),
        Err(_) => {
            // (4) force abort
            for a in aborts.lock().unwrap().iter() {
                a.abort();
            }
            tracker.wait().await; // aborted tasks drop their guards, so this now completes
            events.push("forced_abort");
        }
    }
    live.wait_zero().await;
    assert_eq!(tracker.len(), 0);
    (events, done_ok.load(SeqCst), accepted)
}

async fn test_shutdown_order() {
    let (ev, ok, accepted) = run_server((0..20).map(|i| Conn::Quick(i % 5 * 10)).collect()).await;
    assert_eq!(ev, ["stop_accepting", "cancel", "drained"]);
    assert_eq!(
        ok, accepted,
        "every well behaved handler finished or flushed"
    );

    let mut conns: Vec<Conn> = (0..9).map(|_| Conn::Quick(5)).collect();
    conns.push(Conn::Stuck);
    let (ev, ok, accepted) = run_server(conns).await;
    assert_eq!(ev, ["stop_accepting", "cancel", "forced_abort"]);
    assert_eq!(
        (ok, accepted),
        (9, 10),
        "the stuck handler never completed; it was aborted"
    );
    println!(
        "8. shutdown order: clean run -> drained; with one stuck handler -> forced_abort after the 100 ms deadline"
    );
}

async fn random_trial(rng: &mut Rng, live: &Live) {
    let mut scope = Scope::new(Duration::from_secs(2));
    for _ in 0..1 + rng.below(5) {
        let live = live.clone();
        match rng.below(4) {
            0 => scope.spawn("polite", move |t| polite_worker(t, live)),
            1 => scope.spawn("nested", move |t| {
                counted(&live.clone(), async move {
                    // A scope inside a task: its token is a child of ours, so the outer cancel reaches it.
                    let mut inner = Scope::child_of(&t, Duration::from_secs(2));
                    for _ in 0..2 {
                        let live = live.clone();
                        inner.spawn("inner", move |t| polite_worker(t, live));
                    }
                    inner.join().await.map(|_| ()).map_err(|e| e.to_string())
                })
            }),
            2 => scope.spawn("spinner", move |t| {
                counted(&live, async move {
                    while !t.is_cancelled() {
                        yield_now().await;
                    }
                    Ok(())
                })
            }),
            _ => scope.spawn("sleeper", move |_| {
                counted(&live, async {
                    sleep(ms(2)).await; // ignores cancel but finishes by itself
                    Ok(())
                })
            }),
        }
    }
    for _ in 0..rng.below(40) {
        yield_now().await; // random point in the tasks' lives at which the cancel lands
    }
    match rng.below(3) {
        0 => {
            scope.token().cancel();
            let stats = scope.join().await.expect("external cancel is not an error");
            assert_eq!(stats.forced, 0);
        }
        1 => {
            scope.spawn("bad", |_| async { Err("x".to_string()) });
            assert!(scope.join().await.is_err());
        }
        _ => drop(scope), // abandon the scope: tasks must still end
    }
    live.wait_zero().await; // asserts "no leaked task" with a 5 s bound, so a deadlock fails instead of hanging
}

async fn test_random_cancels() {
    let (live, mut rng) = (Live::default(), Rng(0x9E37_79B9_7F4A_7C15));
    let t = Instant::now();
    for _ in 0..1000 {
        timeout(Duration::from_secs(10), random_trial(&mut rng, &live))
            .await
            .expect("trial deadlocked");
    }
    assert_eq!(live.now(), 0);
    println!(
        "9. 1000 random cancel timings: no leaked task, no deadlock ({:?})",
        t.elapsed()
    );
}

#[tokio::main]
async fn main() {
    test_token_tree().await;
    test_tracker().await;
    test_scope_no_outlive().await;
    test_fail_fast().await;
    test_deadline_force_abort().await;
    test_panic().await;
    test_cancellation_points().await;
    test_shutdown_order().await;
    test_random_cancels().await;
}
