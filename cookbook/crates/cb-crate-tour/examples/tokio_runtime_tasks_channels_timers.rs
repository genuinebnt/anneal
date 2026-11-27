//! RECIPE 1114 (crates): tokio essentials on a paused clock: tasks, select, timers, channels, sync primitives, cancellation, shutdown
//! LEVEL: intermediate
//! Use in: network services, workers, anything with many concurrent waits.   Run: cargo run --release -p cb-crate-tour --example tokio_runtime_tasks_channels_timers
//! COMMON USES: connection handlers, bounded work queues, periodic jobs, graceful shutdown, calling blocking libraries from async code.
//! METHODS: tokio::runtime::Builder, Runtime::block_on, start_paused, tokio::spawn, JoinHandle::abort, JoinSet::spawn, JoinSet::join_next, JoinSet::shutdown, JoinError::is_panic, JoinError::is_cancelled, tokio::join!, tokio::try_join!, tokio::select!, tokio::pin!, tokio::time::{sleep, timeout, interval, Instant}, MissedTickBehavior, tokio::time::advance, tokio::sync::{mpsc, oneshot, broadcast, watch, Semaphore, Mutex, RwLock, Notify, Barrier}, mpsc::Sender::try_send, mpsc::Sender::reserve, tokio::task::spawn_blocking, tokio::task::yield_now, tokio::task::LocalSet, tokio::task::spawn_local, tokio_util::sync::CancellationToken, CancellationToken::child_token, CancellationToken::cancelled, std::sync::Mutex::try_lock, std::sync::TryLockError::WouldBlock
//! DOCS: https://docs.rs/tokio/1/tokio/ https://docs.rs/tokio-util/0.7/tokio_util/sync/struct.CancellationToken.html
//! CRATES: tokio = "1" (full, test-util), tokio-util = "0.7"
//!
//! DETERMINISTIC TIME. `start_paused(true)` (feature test-util, current-thread runtime) freezes the clock. When every task is
//! idle, tokio jumps to the next timer. Hours of simulated sleeping finish instantly and elapsed times are EXACT.
//!
//! CHEAT SHEET
//!   many tasks, collect as they finish -> JoinSet          fixed few, same task -> join! / try_join!
//!   first of several                   -> select!           give up after d      -> timeout(d, fut)
//!   work queue                         -> bounded mpsc      one reply            -> oneshot
//!   every subscriber sees every item   -> broadcast         latest value only    -> watch
//!   limit concurrency                  -> Semaphore         stop everything      -> CancellationToken + await the tasks
//!   blocking or CPU-heavy call         -> spawn_blocking
//!   block_in_place (described): multi-thread runtime only; it runs blocking code on the current worker after moving other
//!   tasks away. It panics on a current-thread runtime, and other futures in the SAME task (join!/select!) still stall.
//!
//! STD MUTEX ACROSS .await. Section `std_mutex_across_await` builds the broken program and proves it is stuck WITHOUT
//! running into the hang: the second future uses try_lock where the real bug calls lock(). clippy: `await_holding_lock`.
//!
//! CANCELLATION SAFETY. select! DROPS the losing futures. That is fine if a dropped future has lost nothing
//! (mpsc::Receiver::recv) and a bug if it holds partial progress. Section `cancellation_safety` shows both.
//!
//! Read: https://tokio.rs/tokio/tutorial ; https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety ; https://ryhl.io/blog/async-what-is-blocking/
//!
//! TRY NEXT:
//!   - [S] Change the interval's MissedTickBehavior and predict the tick times before running.
//!   - [M] Turn the worker pool into one with a per-job timeout that reports which job timed out.
//!   - [A] PROJECT: build a token-bucket rate limiter on `interval` + `Semaphore` and test it on the paused clock.
#![allow(dead_code)]
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::Duration;
use tokio::sync::{Barrier, Mutex, Notify, RwLock, Semaphore, broadcast, mpsc, oneshot, watch};
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, timeout};
use tokio_util::sync::CancellationToken;

const SEC: Duration = Duration::from_secs(1);
fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

async fn tasks_and_joinset() {
    let t0 = Instant::now();
    // Futures are LAZY: creating one does nothing until it is polled.
    let ran = Arc::new(AtomicUsize::new(0));
    let r = ran.clone();
    let lazy = async move { r.fetch_add(1, SEQ) };
    sleep(SEC).await;
    assert_eq!(ran.load(SEQ), 0, "never polled, never ran");
    drop(lazy);
    // spawn starts it running without being awaited.
    let r = ran.clone();
    let handle = tokio::spawn(async move { r.fetch_add(1, SEQ) });
    sleep(SEC).await;
    assert_eq!(ran.load(SEQ), 1);
    handle.await.unwrap();

    // Sequential awaits add up; join! overlaps.
    let t = Instant::now();
    sleep(secs(3)).await;
    sleep(secs(2)).await;
    assert_eq!(t.elapsed(), secs(5));
    let t = Instant::now();
    tokio::join!(sleep(secs(3)), sleep(secs(2)));
    assert_eq!(t.elapsed(), secs(3));
    // try_join! returns at the first error and drops the rest.
    let t = Instant::now();
    let r: Result<((), ()), &str> = tokio::try_join!(
        async {
            sleep(secs(100)).await;
            Ok(())
        },
        async {
            sleep(secs(1)).await;
            Err("boom")
        }
    );
    assert_eq!((r, t.elapsed()), (Err("boom"), secs(1)));

    // JoinSet yields results in COMPLETION order.
    let mut set = JoinSet::new();
    for d in [30u64, 10, 20] {
        set.spawn(async move {
            sleep(secs(d)).await;
            d
        });
    }
    let mut order = vec![];
    while let Some(r) = set.join_next().await {
        order.push(r.unwrap());
    }
    assert_eq!(order, [10, 20, 30]);
    // A panic is contained in its task and reported through JoinError.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let e = tokio::spawn(async { panic!("bug in task") })
        .await
        .map(|()| ())
        .unwrap_err();
    std::panic::set_hook(prev);
    assert!(e.is_panic() && !e.is_cancelled());
    // abort() cancels at the next .await. Dropping a JoinHandle DETACHES; dropping a JoinSet ABORTS.
    let h = tokio::spawn(sleep(secs(1000)));
    h.abort();
    assert!(h.await.unwrap_err().is_cancelled());
    let finished = Arc::new(AtomicUsize::new(0));
    let (a, b) = (finished.clone(), finished.clone());
    drop(tokio::spawn(async move {
        sleep(SEC).await;
        a.fetch_add(1, SEQ);
    }));
    let mut doomed = JoinSet::new();
    doomed.spawn(async move {
        sleep(SEC).await;
        b.fetch_add(100, SEQ);
    });
    drop(doomed);
    sleep(secs(2)).await;
    assert_eq!(
        finished.load(SEQ),
        1,
        "detached ran to completion; the JoinSet task did not"
    );
    assert!(t0.elapsed() > secs(10), "all of this was simulated time");
}
const SEQ: std::sync::atomic::Ordering = SeqCst;

async fn timers() {
    let t0 = Instant::now();
    assert!(timeout(secs(5), sleep(secs(4))).await.is_ok());
    assert!(timeout(secs(5), sleep(secs(6))).await.is_err());
    assert_eq!(t0.elapsed(), secs(9));
    // interval: the first tick is immediate; the period does not drift with the time spent between ticks.
    let start = Instant::now();
    let mut every = interval(secs(10));
    let mut at = vec![];
    for _ in 0..4 {
        every.tick().await;
        at.push(start.elapsed().as_secs());
        sleep(secs(3)).await; // work
    }
    assert_eq!(at, [0, 10, 20, 30]);
    // A loop with sleep() instead would tick at 0, 13, 26, 39.
    for (behaviour, expected) in [
        (MissedTickBehavior::Burst, [25u64, 25, 30]), // catch up immediately, then back on the original grid
        (MissedTickBehavior::Delay, [25, 35, 45]),    // restart the period from now
        (MissedTickBehavior::Skip, [25, 30, 40]),     // drop missed ticks, stay on the grid
    ] {
        let start = Instant::now();
        let mut i = interval(secs(10));
        i.set_missed_tick_behavior(behaviour);
        i.tick().await; // t=0
        sleep(secs(25)).await; // ticks due at 10 and 20 are missed
        let mut got = vec![];
        for _ in 0..3 {
            i.tick().await;
            got.push(start.elapsed().as_secs());
        }
        assert_eq!(got, expected, "{behaviour:?}");
    }
    let t = Instant::now();
    tokio::time::advance(secs(3600)).await;
    assert_eq!(t.elapsed(), secs(3600));
}

async fn select_and_cancellation_safety() {
    // biased: poll in the order written (otherwise the start branch is random).
    let (tx, mut rx) = mpsc::channel::<u32>(8);
    tx.send(1).await.unwrap();
    let first = tokio::select! {
        biased;
        v = rx.recv() => v,
        _ = sleep(SEC) => None,
    };
    assert_eq!(first, Some(1));
    // recv() is cancel safe: losing the race never loses a message.
    let producer = tokio::spawn(async move {
        for i in 0..20u32 {
            sleep(Duration::from_millis(7)).await;
            tx.send(i).await.unwrap();
        }
    });
    let mut got = vec![];
    let mut tick = interval(Duration::from_millis(3));
    loop {
        tokio::select! {
            m = rx.recv() => match m { Some(m) => got.push(m), None => break },
            _ = tick.tick() => {}
        }
    }
    producer.await.unwrap();
    assert_eq!(got, (0..20).collect::<Vec<_>>());

    // NOT cancel safe: the progress lives inside the dropped future.
    async fn read_batch(rx: &mut mpsc::Receiver<u32>) -> Vec<u32> {
        let mut batch = vec![];
        while batch.len() < 3 {
            batch.push(rx.recv().await.unwrap());
        }
        batch
    }
    let (tx, mut rx) = mpsc::channel(8);
    tx.send(1).await.unwrap();
    tx.send(2).await.unwrap();
    assert!(timeout(SEC, read_batch(&mut rx)).await.is_err());
    for i in [3, 4, 5] {
        tx.send(i).await.unwrap();
    }
    assert_eq!(
        read_batch(&mut rx).await,
        [3, 4, 5],
        "1 and 2 vanished with the cancelled future"
    );
    // FIX: keep the SAME future alive across loop iterations with pin! and poll it by reference.
    for i in [1, 2] {
        tx.send(i).await.unwrap();
    }
    let batch = {
        let fut = read_batch(&mut rx);
        tokio::pin!(fut);
        let mut heartbeats = 0;
        loop {
            tokio::select! {
                b = &mut fut => break b,
                _ = sleep(SEC) => {
                    heartbeats += 1;
                    if heartbeats == 5 { tx.send(3).await.unwrap(); }
                }
            }
        }
    };
    assert_eq!(batch, [1, 2, 3]);
}

async fn channels() {
    // Bounded mpsc = BACKPRESSURE: a full queue makes the producer wait.
    let (tx, mut rx) = mpsc::channel::<u32>(2);
    tx.try_send(1).unwrap();
    tx.try_send(2).unwrap();
    assert!(matches!(
        tx.try_send(3),
        Err(mpsc::error::TrySendError::Full(3))
    ));
    let t = Instant::now();
    let consumer = tokio::spawn(async move {
        let mut seen = vec![];
        while let Some(v) = rx.recv().await {
            sleep(SEC).await;
            seen.push(v);
        }
        seen
    });
    for v in 3..=6 {
        tx.send(v).await.unwrap();
    }
    assert!(
        t.elapsed() >= secs(2),
        "the producer was slowed to the consumer's pace"
    );
    // reserve(): wait for capacity first, then send without .await (cancel safe: the value is not consumed early).
    tx.reserve().await.unwrap().send(7);
    drop(tx); // recv() returns None once ALL senders are gone and the queue is drained
    assert_eq!(consumer.await.unwrap(), [1, 2, 3, 4, 5, 6, 7]);

    // Actor: one task owns the state; requests carry a oneshot for the reply.
    let (tx, mut rx) = mpsc::channel::<(u64, oneshot::Sender<u64>)>(8);
    tokio::spawn(async move {
        let mut total = 0;
        while let Some((n, reply)) = rx.recv().await {
            total += n;
            let _ = reply.send(total); // the caller may have given up: that is not an error
        }
    });
    let mut replies = vec![];
    for n in 1..=4 {
        let (otx, orx) = oneshot::channel();
        tx.send((n, otx)).await.unwrap();
        replies.push(orx.await.unwrap());
    }
    assert_eq!(replies, [1, 3, 6, 10]);
    let (otx, orx) = oneshot::channel::<()>();
    drop(otx);
    assert!(orx.await.is_err(), "sender dropped without replying");

    // broadcast: every receiver gets every message; a slow one is told how many it LOST.
    let (btx, mut fast) = broadcast::channel(2);
    let mut slow = btx.subscribe();
    for i in 0..4 {
        btx.send(i).unwrap();
        assert_eq!(fast.recv().await.unwrap(), i);
    }
    assert_eq!(
        slow.recv().await,
        Err(broadcast::error::RecvError::Lagged(2))
    );
    assert_eq!((slow.recv().await, slow.recv().await), (Ok(2), Ok(3)));

    // watch: only the LATEST value matters (configuration, shutdown flags).
    let (wtx, mut wrx) = watch::channel("v0");
    wtx.send("v1").unwrap();
    wtx.send("v2").unwrap();
    wrx.changed().await.unwrap();
    assert_eq!(*wrx.borrow_and_update(), "v2");
    assert!(!wrx.has_changed().unwrap());
    drop(wtx);
    assert!(wrx.changed().await.is_err());
}

async fn sync_primitives() {
    // Semaphore: at most 3 jobs at once. 12 one-second jobs take exactly 4 s.
    let (sem, active, peak) = (
        Arc::new(Semaphore::new(3)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    );
    let t = Instant::now();
    let mut set = JoinSet::new();
    for _ in 0..12 {
        let (sem, active, peak) = (sem.clone(), active.clone(), peak.clone());
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap(); // released on drop, even if the task panics
            peak.fetch_max(active.fetch_add(1, SEQ) + 1, SEQ);
            sleep(SEC).await;
            active.fetch_sub(1, SEQ);
        });
    }
    set.join_all().await;
    assert_eq!((peak.load(SEQ), t.elapsed()), (3, secs(4)));

    // tokio::Mutex may be held across .await. For short, await-free critical sections prefer std::sync::Mutex.
    let counter = Arc::new(Mutex::new(0));
    let mut set = JoinSet::new();
    for _ in 0..10 {
        let c = counter.clone();
        set.spawn(async move {
            let mut g = c.lock().await;
            let read = *g;
            sleep(SEC).await; // the lock is held across this await, so no update is lost
            *g = read + 1;
        });
    }
    set.join_all().await;
    assert_eq!(*counter.lock().await, 10);
    // std Mutex done right in async code: lock, update, drop BEFORE awaiting. (Holding a std guard across .await makes
    // the future !Send, and on one thread it can deadlock the runtime.)
    let std_counter = Arc::new(std::sync::Mutex::new(0));
    let c = std_counter.clone();
    tokio::spawn(async move {
        {
            *c.lock().unwrap() += 1;
        }
        sleep(SEC).await;
    })
    .await
    .unwrap();
    assert_eq!(*std_counter.lock().unwrap(), 1);

    let rw = RwLock::new(1);
    let (a, b) = (rw.read().await, rw.read().await);
    assert!(rw.try_write().is_err());
    drop((a, b));
    *rw.write().await += 1;
    assert_eq!(*rw.read().await, 2);

    // Notify: notify_one stores ONE permit if nobody is waiting, so the wake-up is not lost.
    let n = Notify::new();
    n.notify_one();
    n.notify_one();
    timeout(SEC, n.notified()).await.unwrap();
    assert!(
        timeout(SEC, n.notified()).await.is_err(),
        "permits do not accumulate"
    );
    n.notify_waiters(); // wakes only CURRENT waiters and stores nothing
    assert!(timeout(SEC, n.notified()).await.is_err());

    let barrier = Arc::new(Barrier::new(4));
    let mut set = JoinSet::new();
    for i in 0..4u64 {
        let b = barrier.clone();
        set.spawn(async move {
            sleep(secs(i)).await;
            let leader = b.wait().await.is_leader();
            (Instant::now(), leader)
        });
    }
    let out = set.join_all().await;
    assert!(
        out.iter().all(|o| o.0 == out[0].0),
        "all released at the same instant"
    );
    assert_eq!(out.iter().filter(|o| o.1).count(), 1);
}

/// Evaluates to whether the TYPE of an expression is Send, without making a !Send one a compile error. It relies on
/// method lookup preferring an inherent method whenever its bounds hold. It only works on concrete types (hence a macro).
macro_rules! is_send {
    ($value:expr) => {{
        struct Probe<T>(std::marker::PhantomData<T>);
        #[allow(dead_code)]
        trait NotSend {
            fn is_send(&self) -> bool {
                false
            }
        }
        impl<T> NotSend for Probe<T> {}
        #[allow(dead_code)]
        impl<T: Send> Probe<T> {
            fn is_send(&self) -> bool {
                true
            }
        }
        fn probe<T>(_: &T) -> Probe<T> {
            Probe(std::marker::PhantomData)
        }
        probe(&$value).is_send()
    }};
}

// Separate functions: auto traits of an `async` block are not yet known inside the function body that defines it.
#[allow(clippy::await_holding_lock)]
async fn guard_across_await(m: &std::sync::Mutex<u32>) {
    let mut g = m.lock().unwrap();
    tokio::task::yield_now().await;
    *g += 1;
}
async fn guard_dropped_by_hand(m: &std::sync::Mutex<u32>) {
    let g = m.lock().unwrap();
    drop(g);
    tokio::task::yield_now().await;
}
async fn guard_scoped(m: &std::sync::Mutex<u32>) {
    {
        *m.lock().unwrap() += 1;
    }
    tokio::task::yield_now().await;
}

/// THE BROKEN CASE: a std (blocking) mutex guard that lives across an .await.
#[allow(clippy::await_holding_lock)]
async fn std_mutex_across_await() {
    use std::sync::{Mutex as StdMutex, TryLockError};

    // 1. The compiler catches it only where Send is required (tokio::spawn). These futures are created, never polled.
    let m = StdMutex::new(0);
    assert!(
        !is_send!(guard_across_await(&m)),
        "MutexGuard is !Send, so the whole future is: tokio::spawn would reject it"
    );
    // On the toolchain this was written with (rustc 1.98), moving the guard away with drop() before the await is enough,
    // as is ending its scope. Older advice says only a block works; the block is also the clearest to a reader.
    assert!(is_send!(guard_dropped_by_hand(&m)));
    assert!(is_send!(guard_scoped(&m)));

    // 2. block_on, join!, select! and spawn_local do NOT require Send, so there the bug compiles. Why it deadlocks:
    //    two futures in ONE task share one thread. A takes the lock and suspends until B tells it to continue.
    let lock = StdMutex::new(Vec::new());
    let (locked_tx, locked_rx) = oneshot::channel::<()>();
    let (go_tx, go_rx) = oneshot::channel::<()>();
    let (released_tx, released_rx) = oneshot::channel::<()>();
    let a = async {
        {
            let mut g = lock.lock().unwrap();
            locked_tx.send(()).unwrap();
            go_rx.await.unwrap(); // suspended WHILE HOLDING the std guard
            g.push("a");
        }
        released_tx.send(()).unwrap();
    };
    let b = async {
        locked_rx.await.unwrap();
        // The buggy program calls lock() here. That blocks the THREAD. Only this thread can poll `a`, and `a` is the
        // only owner of the guard, so the call could never return; a timeout could not fire either, because the same
        // thread would have to poll it. try_lock asks the same question without blocking:
        assert!(
            matches!(lock.try_lock(), Err(TryLockError::WouldBlock)),
            "lock() would block here forever"
        );
        go_tx.send(()).unwrap(); // reachable only because we did not block
        // Wait to be TOLD that the guard is gone. A yield_now() here would not be enough: it does not promise that `a`
        // runs first (join! varies which branch it polls first), and a blocking lock() that comes too early hangs.
        released_rx.await.unwrap();
        lock.lock().unwrap().push("b");
    };
    tokio::join!(a, b);
    assert_eq!(*lock.lock().unwrap(), ["a", "b"]);

    // 3. FIX with tokio::sync::Mutex: lock().await suspends only the waiting FUTURE, so the holder keeps running.
    let lock = Mutex::new(Vec::new());
    let (locked_tx, locked_rx) = oneshot::channel::<()>();
    let t = Instant::now();
    let a = async {
        let mut g = lock.lock().await;
        locked_tx.send(()).unwrap();
        sleep(SEC).await;
        g.push("a");
    };
    let b = async {
        locked_rx.await.unwrap();
        assert!(lock.try_lock().is_err(), "contended, exactly as before");
        lock.lock().await.push("b"); // waits one simulated second; the thread stays free to finish `a`
    };
    tokio::join!(a, b);
    assert_eq!((t.elapsed(), lock.into_inner()), (SEC, vec!["a", "b"]));

    // 4. An async mutex does not remove LOGICAL deadlocks (A waits for B, B waits for A's lock). It only keeps the
    //    thread alive, so a timeout can see the problem.
    let lock = Mutex::new(());
    let (go_tx, go_rx) = oneshot::channel::<()>();
    let (locked_tx, locked_rx) = oneshot::channel::<()>();
    let cycle = async {
        tokio::join!(
            async {
                let _g = lock.lock().await;
                locked_tx.send(()).unwrap();
                let _ = go_rx.await; // never arrives: the sender is stuck behind our lock
            },
            async {
                locked_rx.await.unwrap();
                let _g = lock.lock().await;
                let _ = go_tx.send(());
            }
        )
    };
    assert!(timeout(secs(60), cycle).await.is_err());
    assert!(
        lock.try_lock().is_ok(),
        "dropping the stuck futures released the guard"
    );
}

/// The blocking mistake and its fix, measured with a heartbeat that SHOULD tick while the work runs.
async fn blocking_work() {
    let beats = Arc::new(AtomicUsize::new(0));
    let b = beats.clone();
    let heart = tokio::spawn(async move {
        loop {
            tokio::task::yield_now().await;
            b.fetch_add(1, SEQ);
        }
    });
    tokio::task::yield_now().await;
    let before = beats.load(SEQ);
    std::thread::sleep(Duration::from_millis(20)); // WRONG: blocks the only runtime thread
    assert_eq!(beats.load(SEQ), before, "nothing else could run");
    // RIGHT: run it on the blocking pool and await the handle. The blocking closure waits until the heartbeat has
    // advanced, which can only happen if the runtime thread stayed free.
    let watch_beats = beats.clone();
    let result = tokio::task::spawn_blocking(move || {
        let start = watch_beats.load(SEQ);
        let deadline = std::time::Instant::now() + secs(10);
        while watch_beats.load(SEQ) < start + 100 {
            assert!(
                std::time::Instant::now() < deadline,
                "runtime appears blocked"
            );
            std::thread::yield_now();
        }
        42
    })
    .await
    .unwrap();
    assert_eq!(result, 42);
    heart.abort();
}

/// Three workers on one queue.
fn spawn_workers(
    root: &CancellationToken,
    rx: mpsc::Receiver<u32>,
    done: &Arc<AtomicUsize>,
) -> JoinSet<()> {
    let rx = Arc::new(Mutex::new(rx));
    let mut workers = JoinSet::new();
    for _ in 0..3 {
        let (token, rx, done) = (root.child_token(), rx.clone(), done.clone());
        workers.spawn(async move {
            loop {
                let job = {
                    let mut rx = rx.lock().await;
                    tokio::select! {
                        biased;
                        // Cancellation is looked at FIRST: if queued work came first, a producer that keeps the queue
                        // full would keep this branch from ever being reached.
                        _ = token.cancelled() => {
                            // Stop accepting, THEN drain. `try_recv` alone would leave the queue open: a send that
                            // succeeds after the last worker looked would be lost without anyone being told.
                            rx.close(); // harmless when already closed
                            rx.recv().await // what was accepted, including reserved slots; then None
                        }
                        job = rx.recv() => job,
                    }
                };
                let Some(_job) = job else { break };
                sleep(SEC).await; // the job itself is NOT inside select!, so it is never cut in half
                done.fetch_add(1, SEQ);
            }
        });
    }
    workers
}

/// A producer that never stops by itself: shutdown must still end, and run every job the queue accepted.
async fn shutdown_under_load() {
    let root = CancellationToken::new();
    let (tx, rx) = mpsc::channel::<u32>(10);
    let done = Arc::new(AtomicUsize::new(0));
    let workers = spawn_workers(&root, rx, &done);
    let producer = tokio::spawn(async move {
        let mut accepted = 0;
        while tx.send(accepted as u32).await.is_ok() {
            accepted += 1;
        }
        accepted // the queue was closed under it
    });
    sleep(Duration::from_millis(2500)).await;
    root.cancel();
    timeout(secs(60), workers.join_all())
        .await
        .expect("shutdown deadline: the queue was never closed");
    assert_eq!(done.load(SEQ), producer.await.unwrap());
}

/// Graceful shutdown: signal, close the queue, let each worker finish its current job and drain, then WAIT for all of them.
/// The deadline matters: closing waits for every reserved slot to be used or dropped.
async fn graceful_shutdown() {
    let root = CancellationToken::new();
    let (tx, rx) = mpsc::channel::<u32>(100);
    let done = Arc::new(AtomicUsize::new(0));
    let workers = spawn_workers(&root, rx, &done);
    for j in 0..10 {
        tx.send(j).await.unwrap();
    }
    sleep(Duration::from_millis(1500)).await;
    // a producer that already holds a slot in the queue, and fills it only after the workers have run out of work
    let slot = tx.clone().reserve_owned().await.unwrap();
    let late = tokio::spawn(async move {
        sleep(secs(30)).await;
        slot.send(10);
    });
    root.cancel();
    timeout(secs(60), workers.join_all())
        .await
        .expect("shutdown deadline");
    late.await.unwrap();
    assert_eq!(done.load(SEQ), 11, "a job the queue had accepted was lost");
    assert!(tx.try_send(11).is_err(), "new work is refused, not dropped");
    // A child can be cancelled without touching the parent; not the other way round.
    let parent = CancellationToken::new();
    let child = parent.child_token();
    child.cancel();
    assert!(!parent.is_cancelled());
    let child2 = parent.child_token();
    parent.cancel();
    assert!(child2.is_cancelled());
}

/// LocalSet runs !Send futures (here an Rc held across an .await) on the current thread.
async fn local_tasks() {
    let local = tokio::task::LocalSet::new();
    let out = local
        .run_until(async {
            let shared = Rc::new(5);
            let s = shared.clone();
            tokio::task::spawn_local(async move {
                tokio::task::yield_now().await;
                *s * 2
            })
            .await
            .unwrap()
        })
        .await;
    assert_eq!(out, 10);
}

pub fn main() {
    let paused = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
    };
    paused().block_on(async {
        tasks_and_joinset().await;
        timers().await;
        select_and_cancellation_safety().await;
        channels().await;
        sync_primitives().await;
        std_mutex_across_await().await;
        graceful_shutdown().await;
        shutdown_under_load().await;
        local_tasks().await;
    });
    // Real clock for the section that involves real blocking.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(blocking_work());
    println!("tokio: all sections passed");
}
