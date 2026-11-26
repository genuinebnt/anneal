//! RECIPE 604 (interview): Async Rust: futures, Pin, Send across await, blocking the runtime, cancellation and select!.
//! LEVEL: advanced
//! Use in: interview preparation, reviewing tokio services, diagnosing "future is not Send" and stalled-runtime bugs.   Run: cargo run --release --example rust_async_qa
//! COMMON USES: network services, task fan-out/fan-in, timeouts and cancellation, bridging blocking/CPU work into async code.
#![allow(dead_code)]
//! Read: https://tokio.rs/tokio/tutorial/async and https://fasterthanli.me/articles/pin-and-suffering
//! Read: https://www.techinterview.org/post/3233474457/rust-interview-questions-2025-ownership-borrowing-lifetimes-traits-async-await-error-handling-smart-pointers-concurrency/
//! Read: https://github.com/devinterview-io/rust-interview-questions and https://interviewing.io/rust-interview-questions
//!
//! QUESTION BANK
//! Q1. What does an `async fn` return? Does it run when called? [Easy] [asked: very common]
//! Q2. What are `Future::poll` and `Waker`? Write a Future by hand and a tiny `block_on`. [Hard] [asked: common]
//! Q3. Why must futures be pinned? What are `Pin`, `Unpin`, `Box::pin` and `pin!`? [Hard] [asked: very common]
//! Q4. `tokio::spawn` vs `join!` vs `select!`: when each? [Medium] [asked: very common]
//! Q5. What happens when a spawned task panics? What does `JoinHandle` give you? [Medium] [asked: common]
//! Q6. Why is a future holding an `Rc` or a `std::sync::MutexGuard` across `.await` not `Send`? [Hard] [asked: very common]
//! Q7. What happens if you block (`thread::sleep`) inside async code? How do you fix it? [Medium] [asked: very common]
//! Q8. `tokio::sync::Mutex` vs `std::sync::Mutex` in async code. [Medium] [asked: common]
//! Q9. What does cancellation mean in async Rust? [Medium] [asked: very common]
//! Q10. What is cancel safety in `select!`? Give a safe and an unsafe example. [Hard] [asked: common]
//! Q11. Why does async recursion need `Box::pin`? [Medium] [asked: occasional]
//! Q12. Streams vs iterators. [Medium] [asked: occasional]
//! Q13. Why did `async fn` in traits need special support? What about `dyn`? [Hard] [asked: occasional]
//! Q14. How do you run CPU-bound work from async code? [Medium] [asked: common]
//! Q15. When should you NOT use async? [Medium] [asked: common]

use std::cell::RefCell;
use std::future::Future;
use std::mem::size_of_val;
use std::pin::{Pin, pin};
use std::rc::Rc;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
use tokio::time::{sleep, timeout};

fn current_thread_rt() -> Runtime {
    Builder::new_current_thread().enable_time().build().unwrap()
}
fn multi_thread_rt() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(2)
        .enable_time()
        .build()
        .unwrap()
}
fn quiet<R>(f: impl FnOnce() -> R) -> R {
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = f();
    std::panic::set_hook(old);
    r
}

/// Minimal executor: poll in a loop with a no-op waker (fine for futures that wake themselves immediately).
fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut); // pin on the stack: the future will not move again
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

/// Probe whether a *value's type* is `Send` (autoref trick, works on opaque future types).
struct Probe<'a, T>(&'a T);
trait NotSend {
    fn is_send(&self) -> bool {
        false
    }
}
impl<T> NotSend for Probe<'_, T> {}
impl<T: Send> Probe<'_, T> {
    fn is_send(&self) -> bool {
        true
    }
}
macro_rules! is_send {
    ($e:expr) => {
        Probe(&$e).is_send()
    };
}

// ---------------------------------------------------------------------------
// Q1. what async fn returns
// ---------------------------------------------------------------------------
/// ANSWER: Calling an `async fn` runs NOTHING: it returns an anonymous type that implements `Future`, a compiler
/// generated STATE MACHINE (an enum of "suspended at await #k" states holding the locals that live across
/// awaits). Code runs only when an executor `poll`s the future, up to the next `.await` that returns
/// `Pending`. Dropping an unpolled future simply drops the state. The future's size is the size of that enum
/// (largest suspend point), which you can measure with `size_of_val`. `async { }` blocks are the same thing.
///
/// REALLY TESTING: lazy futures (unlike JavaScript promises, which start eagerly).
/// WRONG: "async fn starts a thread / a task" or "calling it begins the work".
/// FOLLOW-UP: what happens if you call an async fn and ignore the result? Nothing (and a `#[must_use]` warning).
async fn traced(log: Rc<RefCell<Vec<&'static str>>>) -> u32 {
    log.borrow_mut().push("body started");
    std::future::ready(()).await;
    log.borrow_mut().push("body finished");
    7
}
async fn big_state() -> u8 {
    let buf = [0u8; 1024];
    sleep(Duration::from_millis(1)).await; // `buf` must survive the suspension => stored in the future
    buf[0]
}
async fn small_state() -> u8 {
    1
}
fn q1_async_fn_is_lazy() {
    let log = Rc::new(RefCell::new(Vec::new()));
    log.borrow_mut().push("before call");
    let fut = traced(log.clone());
    log.borrow_mut().push("after call (nothing ran yet)");
    assert_eq!(
        *log.borrow(),
        ["before call", "after call (nothing ran yet)"]
    );
    let out = block_on(fut); // first poll runs the body
    assert_eq!(out, 7);
    assert_eq!(log.borrow()[2..], ["body started", "body finished"]);
    // The future is a value with a size:
    assert!(
        size_of_val(&big_state()) >= 1024,
        "the array lives inside the future"
    );
    assert!(size_of_val(&small_state()) <= 8);
    // `let _ = traced(..)` or dropping it never runs the body:
    let log2 = Rc::new(RefCell::new(Vec::new()));
    drop(traced(log2.clone()));
    assert!(log2.borrow().is_empty());
}

// ---------------------------------------------------------------------------
// Q2. poll / Waker / hand-written future
// ---------------------------------------------------------------------------
/// ANSWER: `trait Future { type Output; fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>; }`.
/// The executor calls `poll`; the future either returns `Ready(v)` or `Pending` AFTER arranging to be woken:
/// it clones `cx.waker()` and stores it where the event source (timer, socket readiness, channel) can call
/// `wake()`, which re-queues the task. An executor must NOT busy-poll a Pending future that never registered a
/// waker, and a future that returns Pending without a wake-up will hang forever (lost wakeup bug).
/// `.await` is a loop over `poll` that propagates `Pending` to the caller.
///
/// REALLY TESTING: the real mechanics under async/await: executor + reactor + waker.
/// WRONG: "poll is called repeatedly in a spin loop" (only on wake).
/// FOLLOW-UP: what does `Waker::noop()` mean? An executor that never needs wake-ups (we re-poll ourselves).
struct Countdown {
    remaining: u32,
    polls: u32,
}
impl Future for Countdown {
    type Output = u32;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u32> {
        self.polls += 1;
        if self.remaining == 0 {
            Poll::Ready(self.polls)
        } else {
            self.remaining -= 1;
            cx.waker().wake_by_ref(); // "poll me again": we are ready to progress immediately
            Poll::Pending
        }
    }
}
fn q2_poll_and_waker() {
    assert_eq!(
        block_on(Countdown {
            remaining: 3,
            polls: 0
        }),
        4,
        "3 Pending polls + 1 Ready poll"
    );
    // async/await is sugar over this:
    let v = block_on(async {
        let polls = Countdown {
            remaining: 2,
            polls: 0,
        }
        .await;
        polls * 10
    });
    assert_eq!(v, 30);
    // A real waker counts wake-ups (std::task::Wake):
    struct Counter(std::sync::atomic::AtomicUsize);
    impl std::task::Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let counter = Arc::new(Counter(Default::default()));
    let waker = Waker::from(counter.clone());
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(Countdown {
        remaining: 2,
        polls: 0
    });
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    assert!(fut.as_mut().poll(&mut cx).is_pending());
    assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(3));
    assert_eq!(
        counter.0.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "one wake per Pending"
    );
}

// ---------------------------------------------------------------------------
// Q3. Pin
// ---------------------------------------------------------------------------
/// ANSWER: An `async` body may hold a reference to one of its own locals across an `.await`
/// (`let x = ..; let r = &x; other().await; use(r)`). In the state machine that is a SELF-REFERENTIAL
/// struct (field `r` points to field `x`). Moving such a value would leave `r` dangling, so `poll` takes
/// `self: Pin<&mut Self>`: a promise that the pointee will not be moved or invalidated until dropped. `Pin<P>`
/// only hands out `&mut T` when `T: Unpin` (`Pin::new`, `get_mut`); most hand-written types are `Unpin`
/// automatically, compiler generated futures are `!Unpin`. You create a pinned future with `Box::pin(fut)` (heap),
/// `pin!(fut)` (stack, stays in scope) or by awaiting it (the compiler pins it inside the parent's state machine).
/// `Pin<Box<dyn Future>>` is the type-erased future you pass to executors.
///
/// REALLY TESTING: why `Pin` exists, not just its syntax.
/// WRONG: "Pin prevents moving a value" (only if it is !Unpin; for Unpin types Pin is a no-op) or "Pin pins the future in memory
/// forever" (until dropped).
/// FOLLOW-UP: why is `Pin<Box<T>>: Unpin`? The pointer can move; the heap target does not.
async fn borrows_own_local() -> u32 {
    let x = 41; // lives across the await => stored in the future
    let r = &x; // self-reference inside the state machine
    sleep(Duration::from_millis(1)).await;
    *r + 1
}
fn q3_pinning() {
    let rt = current_thread_rt();
    let _enter = rt.enter(); // timers need a runtime context even when we poll by hand
    // 1. Box::pin: heap-pinned, can be stored and moved around as a pointer.
    let boxed: Pin<Box<dyn Future<Output = u32>>> = Box::pin(borrows_own_local());
    assert_eq!(rt.block_on(boxed), 42);
    // 2. pin! on the stack; poll manually.
    let mut f = pin!(borrows_own_local());
    let mut cx = Context::from_waker(Waker::noop());
    assert!(
        f.as_mut().poll(&mut cx).is_pending(),
        "suspended at the sleep"
    );
    // 3. Unpin vs !Unpin: Pin::new only for Unpin targets.
    let mut ready = std::future::ready(5); // Unpin
    assert_eq!(Pin::new(&mut ready).poll(&mut cx), Poll::Ready(5));
    // let mut fut = borrows_own_local();
    // let p = Pin::new(&mut fut);   // error[E0277]: `{async fn body}` cannot be unpinned
    //                               // note: consider using the `pin!` macro / `Box::pin`
    fn needs_unpin<T: Unpin>(_: &T) {}
    needs_unpin(&Countdown {
        remaining: 0,
        polls: 0,
    }); // hand-written struct: auto Unpin
    // needs_unpin(&borrows_own_local());   // error[E0277]: async fn futures are !Unpin
}

// ---------------------------------------------------------------------------
// Q4. spawn / join / select
// ---------------------------------------------------------------------------
/// ANSWER: `tokio::spawn(fut)` hands a future to the runtime as an independent TASK: it runs concurrently (in
/// parallel on a multi-thread runtime), you get a `JoinHandle`, and it requires `Send + 'static` (the task may
/// move between worker threads and outlive the caller, so own your data: `move`, `Arc`). `join!(a, b)` polls several
/// futures CONCURRENTLY inside the current task (no spawn, no `'static`/`Send` requirement, no parallelism, waits for
/// ALL). `select!` races futures, completes when the FIRST branch finishes and DROPS (cancels) the others.
/// `try_join!` fails fast. Use spawn for independent background work, join for fan-in inside a request,
/// select for timeouts/shutdown/racing.
///
/// REALLY TESTING: concurrency vs parallelism, the `'static + Send` rule and cancellation by select.
/// WRONG: "join! runs them in parallel on different threads" (same task, same thread).
/// FOLLOW-UP: how to bound many spawned tasks? `JoinSet`, semaphore, or `buffer_unordered`.
fn q4_spawn_join_select() {
    let rt = multi_thread_rt();
    rt.block_on(async {
        // spawn: owns its data
        let data = vec![1, 2, 3];
        let h = tokio::spawn(async move { data.iter().sum::<i32>() });
        assert_eq!(h.await.unwrap(), 6);
        // FAILING (E0373 / E0597): a spawned task cannot borrow a local
        //   let local = vec![1]; tokio::spawn(async { local.len() });
        //   error[E0373]: async block may outlive the current function, but it borrows `local`

        // join!: both progress concurrently in ONE task; total time ~= the longer, not the sum.
        let t = Instant::now();
        let (a, b) = tokio::join!(
            async {
                sleep(Duration::from_millis(100)).await;
                "a"
            },
            async {
                sleep(Duration::from_millis(100)).await;
                "b"
            }
        );
        assert_eq!((a, b), ("a", "b"));
        assert!(
            t.elapsed() < Duration::from_millis(190),
            "concurrent, not sequential (2 x 100ms)"
        );

        // select!: first to finish wins; the loser is dropped.
        let winner = tokio::select! {
            _ = sleep(Duration::from_secs(30)) => "slow",
            v = async { "fast" } => v,
        };
        assert_eq!(winner, "fast");
    });
}

// ---------------------------------------------------------------------------
// Q5. JoinHandle and panics
// ---------------------------------------------------------------------------
/// ANSWER: A task's panic is CAUGHT by the runtime and does not take down the process or other tasks. Awaiting its
/// `JoinHandle` returns `Err(JoinError)` where `is_panic()` is true and `into_panic()` returns the payload. If
/// you never await the handle the panic is silently lost (except the panic message on stderr). Dropping a
/// `JoinHandle` DETACHES the task (it keeps running); `handle.abort()` cancels it (`is_cancelled()`). A panic inside
/// `join!`/`select!` branches (same task) propagates to the task instead.
///
/// REALLY TESTING: error handling for background work, detach vs abort.
/// WRONG: "dropping the JoinHandle cancels the task" (that is `abort`).
/// FOLLOW-UP: how do you return errors from tasks? Return `Result` from the task and `??` after `await`.
fn q5_joinhandle_panics() {
    let rt = multi_thread_rt();
    quiet(|| {
        rt.block_on(async {
            let h = tokio::spawn(async { panic!("task blew up") });
            let err: tokio::task::JoinError = h.await.unwrap_err();
            assert!(err.is_panic());
            let payload = err.into_panic();
            assert_eq!(payload.downcast_ref::<&str>(), Some(&"task blew up"));

            // Other tasks are unaffected.
            assert_eq!(tokio::spawn(async { 1 }).await.unwrap(), 1);

            // abort -> cancelled
            let h = tokio::spawn(async {
                sleep(Duration::from_secs(60)).await;
            });
            h.abort();
            assert!(h.await.unwrap_err().is_cancelled());

            // Dropping the handle detaches: the task still completes.
            let (tx, rx) = oneshot::channel();
            drop(tokio::spawn(async move {
                tx.send("still ran").unwrap();
            }));
            assert_eq!(rx.await.unwrap(), "still ran");

            // Result from a task: `?` twice (JoinError, then the task's own error).
            let r: Result<Result<u8, String>, _> =
                tokio::spawn(async { Err::<u8, String>("bad".into()) }).await;
            assert_eq!(r.unwrap(), Err("bad".to_string()));
        })
    });
}

// ---------------------------------------------------------------------------
// Q6. Send across await
// ---------------------------------------------------------------------------
/// ANSWER: `tokio::spawn` needs the future to be `Send` because a multi-thread runtime may resume it on a
/// different worker. A future is `Send` only if EVERY value held ACROSS an `.await` is `Send`. An `Rc` (non-atomic
/// counter) or a `std::sync::MutexGuard` (must be unlocked on the thread that locked it) living across an await
/// makes the whole future `!Send`: error E0277 "future cannot be sent between threads safely ... `Rc<..>` is not
/// `Send` ... await occurs here, with `rc` maybe used later". Fixes: drop/scope the value BEFORE awaiting (a block
/// or `drop(guard)`), use `Arc` / `tokio::sync::Mutex`, or run on a `LocalSet`/`current_thread` with `spawn_local`.
/// Holding a std guard across await is also a deadlock risk even when it compiles on a single thread.
///
/// REALLY TESTING: that you can read this error and know the fix.
/// WRONG: "just wrap it in unsafe impl Send".
/// FOLLOW-UP: what is the scope trick? Values dropped before the await point are not part of the state machine.
async fn bad_rc() -> usize {
    let rc = Rc::new(5);
    sleep(Duration::from_millis(1)).await; // rc is alive across this await => !Send
    Rc::strong_count(&rc)
}
async fn good_scoped_rc() -> usize {
    let n = {
        let rc = Rc::new(5);
        Rc::strong_count(&rc) // rc dropped at the end of this block, before the await
    };
    sleep(Duration::from_millis(1)).await;
    n
}
async fn bad_std_guard(m: Arc<StdMutex<u32>>) -> u32 {
    let guard = m.lock().unwrap();
    sleep(Duration::from_millis(1)).await; // guard alive across await => !Send
    *guard
}
async fn good_std_guard(m: Arc<StdMutex<u32>>) -> u32 {
    let v = *m.lock().unwrap(); // temporary guard dropped at the end of the statement
    sleep(Duration::from_millis(1)).await;
    v
}
async fn good_async_guard(m: Arc<AsyncMutex<u32>>) -> u32 {
    let g = m.lock().await; // tokio guard IS Send and may be held across awaits
    sleep(Duration::from_millis(1)).await;
    *g
}
fn q6_send_across_await() {
    let std_m = Arc::new(StdMutex::new(3));
    let async_m = Arc::new(AsyncMutex::new(4));
    assert!(!is_send!(bad_rc()));
    assert!(is_send!(good_scoped_rc()));
    assert!(!is_send!(bad_std_guard(std_m.clone())));
    assert!(is_send!(good_std_guard(std_m.clone())));
    assert!(is_send!(good_async_guard(async_m.clone())));
    // FAILING (E0277):
    //   tokio::spawn(bad_rc());
    //   error[E0277]: future cannot be sent between threads safely
    //   within `impl Future<Output = usize>`, the trait `Send` is not implemented for `Rc<i32>`
    //   note: future is not `Send` as this value is used across an await
    let rt = multi_thread_rt();
    rt.block_on(async {
        assert_eq!(tokio::spawn(good_scoped_rc()).await.unwrap(), 1);
        assert_eq!(tokio::spawn(good_std_guard(std_m)).await.unwrap(), 3);
        assert_eq!(tokio::spawn(good_async_guard(async_m)).await.unwrap(), 4);
    });
    // `bad_rc` still works where Send is not needed: block_on / LocalSet
    assert_eq!(current_thread_rt().block_on(bad_rc()), 1);
}

// ---------------------------------------------------------------------------
// Q7. blocking the runtime
// ---------------------------------------------------------------------------
/// ANSWER: Async tasks are COOPERATIVE: a task yields only at `.await`. A blocking call
/// (`std::thread::sleep`, blocking I/O, a long CPU loop, `std::sync::Mutex::lock` under contention) never yields,
/// so it occupies the worker thread and every other task scheduled on it STARVES (on a `current_thread`
/// runtime the whole runtime freezes; on multi-thread you lose a worker). Fix: use the async version
/// (`tokio::time::sleep`, `tokio::fs`, async clients) or move blocking work to `spawn_blocking` (dedicated blocking
/// thread pool) / a rayon or std thread. Rule of thumb: no more than ~10-100 microseconds between awaits.
///
/// REALLY TESTING: the cooperative scheduling model; most common production async bug.
/// WRONG: "tokio::spawn makes it run in parallel so blocking is fine" (on current_thread it is not).
/// FOLLOW-UP: how do you detect it? tokio-console, `tokio::runtime::Handle::metrics`, long poll times.
fn order_with(kind: &'static str) -> Vec<&'static str> {
    let rt = current_thread_rt();
    rt.block_on(async {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let l2 = log.clone();
        // Task B is spawned FIRST and wants to run immediately.
        let b = tokio::spawn(async move {
            l2.lock().unwrap().push("B ran");
        });
        // Task A (the main future) now "waits 30ms" in different ways.
        match kind {
            "blocking" => std::thread::sleep(Duration::from_millis(30)), // never yields
            "async_sleep" => sleep(Duration::from_millis(30)).await,     // yields
            "spawn_blocking" => {
                tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(30)))
                    .await
                    .unwrap()
            }
            _ => unreachable!(),
        }
        log.lock().unwrap().push("A done waiting");
        b.await.unwrap();
        log.lock().unwrap().clone()
    })
}
fn q7_blocking_the_runtime() {
    assert_eq!(
        order_with("blocking"),
        ["A done waiting", "B ran"],
        "B was starved until A finished"
    );
    assert_eq!(order_with("async_sleep"), ["B ran", "A done waiting"]);
    assert_eq!(order_with("spawn_blocking"), ["B ran", "A done waiting"]);
}

// ---------------------------------------------------------------------------
// Q8. tokio Mutex vs std Mutex
// ---------------------------------------------------------------------------
/// ANSWER: Use `std::sync::Mutex` (or parking_lot) when the critical section is short and contains NO `.await`: it is
/// faster and its guard dropping before any await keeps the future `Send`. Use `tokio::sync::Mutex` when you must hold the
/// lock ACROSS an await (e.g. a connection used for request/response): `lock()` is async (waits without blocking the
/// thread), the guard is `Send`, but it is slower and fair/FIFO. Never hold either across a long await without a
/// reason: it serialises tasks. The best fix is often to avoid sharing: an actor task owning the state and a channel.
/// A std Mutex held across `.await` can DEADLOCK on a single-thread runtime: the second task blocks the only thread.
///
/// REALLY TESTING: practical correct choice, deadlock awareness.
/// WRONG: "always use tokio::sync::Mutex in async code".
/// FOLLOW-UP: why does tokio's Mutex not poison? No panics unwinding past `await` guards across tasks matter the same way.
fn q8_mutex_choice() {
    let rt = multi_thread_rt();
    rt.block_on(async {
        // tokio mutex across await: tasks serialise on the lock; the counter stays exact.
        let m = Arc::new(AsyncMutex::new(Vec::<u32>::new()));
        let mut hs = Vec::new();
        for i in 0..4 {
            let m = m.clone();
            hs.push(tokio::spawn(async move {
                let mut g = m.lock().await;
                g.push(i);
                sleep(Duration::from_millis(2)).await; // held across await
                g.push(i + 100);
            }));
        }
        for h in hs {
            h.await.unwrap();
        }
        let v = m.lock().await.clone();
        for pair in v.chunks(2) {
            assert_eq!(
                pair[1],
                pair[0] + 100,
                "each task's two writes were adjacent (no interleaving)"
            );
        }
        // std mutex for a short synchronous section:
        let c = Arc::new(StdMutex::new(0u32));
        let hs: Vec<_> = (0..8)
            .map(|_| {
                let c = c.clone();
                tokio::spawn(async move {
                    *c.lock().unwrap() += 1; // guard is a temporary: gone before any await
                    sleep(Duration::from_millis(1)).await;
                })
            })
            .collect();
        for h in hs {
            h.await.unwrap();
        }
        assert_eq!(*c.lock().unwrap(), 8);
    });
}

// ---------------------------------------------------------------------------
// Q9. cancellation
// ---------------------------------------------------------------------------
/// ANSWER: In Rust a future is just a value; "cancelling" it means DROPPING it, usually at an `.await` where it was suspended
/// (via `select!`, `timeout`, `JoinHandle::abort`, a dropped task set). Nothing after the suspension point runs;
/// locals alive at that point are dropped normally, so RAII guards are the cleanup mechanism (no `finally`). There is no
/// async drop: cleanup in `Drop` must be synchronous (send a message, set a flag). Cancellation can happen at EVERY
/// await point, so code must keep invariants across awaits (hence cancel safety, Q10).
///
/// REALLY TESTING: that cancellation = drop and what that means for cleanup and invariants.
/// WRONG: "cancelled futures keep running in the background" (that is detach, not drop).
/// FOLLOW-UP: how does `timeout` report it? `Err(Elapsed)`; the inner future has already been dropped.
struct DropFlag(Arc<StdMutex<Vec<&'static str>>>, &'static str);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.lock().unwrap().push(self.1);
    }
}
async fn long_job(log: Arc<StdMutex<Vec<&'static str>>>) -> u32 {
    let _guard = DropFlag(log.clone(), "cleanup ran");
    log.lock().unwrap().push("started");
    sleep(Duration::from_secs(3600)).await; // cancelled here
    log.lock().unwrap().push("never reached");
    1
}
fn q9_cancellation() {
    let rt = current_thread_rt();
    rt.block_on(async {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let r = timeout(Duration::from_millis(20), long_job(log.clone())).await;
        assert!(r.is_err(), "timed out: the inner future was dropped");
        assert_eq!(*log.lock().unwrap(), ["started", "cleanup ran"]);

        // select!: the losing branch is dropped too.
        let log2 = Arc::new(StdMutex::new(Vec::new()));
        tokio::select! {
            _ = long_job(log2.clone()) => unreachable!(),
            _ = sleep(Duration::from_millis(10)) => {}
        }
        assert_eq!(*log2.lock().unwrap(), ["started", "cleanup ran"]);

        // abort drops the task at its next await.
        let log3 = Arc::new(StdMutex::new(Vec::new()));
        let h = tokio::spawn(long_job(log3.clone()));
        tokio::task::yield_now().await; // let it start and suspend
        h.abort();
        let _ = h.await;
        assert_eq!(*log3.lock().unwrap(), ["started", "cleanup ran"]);
    });
}

// ---------------------------------------------------------------------------
// Q10. select! and cancel safety
// ---------------------------------------------------------------------------
/// ANSWER: A future is CANCEL SAFE if dropping it at any await loses no data and leaves no half-done state, so it can be
/// retried inside a `select!` loop. `mpsc::Receiver::recv` is cancel safe: if the branch loses, no message is consumed.
/// `sleep`, `oneshot::Receiver` (same value kept) are fine. A multi-step async function that reads part of a message,
/// awaits again, then assembles it (e.g. `read_u32` built from two `recv`s, `read_exact` into a local buffer, `write_all`
/// partially done) is NOT: dropping it between steps loses the partial progress. Fix: keep the state outside the
/// future (a struct field/buffer owned by the caller), use a cancel-safe primitive (`read_buf`, `recv`), or spawn
/// the multi-step operation as its own task and `select!` on its JoinHandle (abort does not lose it silently).
///
/// REALLY TESTING: the sharpest async pitfall; the docs of each tokio method state "Cancel safety".
/// WRONG: "select! just waits for the first one".
/// FOLLOW-UP: what is `biased;`? Poll branches in order (default is random for fairness).
async fn read_pair(rx: &mut mpsc::Receiver<u32>) -> (u32, u32) {
    let a = rx.recv().await.unwrap(); // progress held in a local...
    let b = rx.recv().await.unwrap(); // ...if cancelled here, `a` is lost
    (a, b)
}
fn q10_select_cancel_safety() {
    let rt = current_thread_rt();
    rt.block_on(async {
        // SAFE: recv inside a select loop loses nothing.
        let (tx, mut rx) = mpsc::channel::<u32>(8);
        tx.send(1).await.unwrap();
        let mut got = Vec::new();
        for _ in 0..3 {
            tokio::select! {
                Some(v) = rx.recv() => got.push(v),
                _ = sleep(Duration::from_millis(15)) => got.push(0), // timer wins; the recv was dropped safely
            }
            if got.len() == 1 {
                tx.send(2).await.unwrap(); // after the first loop turn a new item arrives
            }
        }
        assert_eq!(
            got,
            [1, 2, 0],
            "no message lost although recv was cancelled in the last iteration"
        );

        // UNSAFE: read_pair dropped after reading `a`, so `a` is gone.
        let (tx, mut rx) = mpsc::channel::<u32>(8);
        tx.send(10).await.unwrap(); // only the first half is available
        let r = timeout(Duration::from_millis(15), read_pair(&mut rx)).await;
        assert!(r.is_err(), "cancelled while waiting for the second half");
        tx.send(20).await.unwrap();
        tx.send(30).await.unwrap();
        assert_eq!(read_pair(&mut rx).await, (20, 30));
        // The 10 that was consumed by the cancelled call is gone for good (no error, no log).

        // FIX: keep partial progress outside the future so a cancelled call can resume.
        let (tx, mut rx) = mpsc::channel::<u32>(8);
        let mut partial: Option<u32> = None;
        tx.send(1).await.unwrap();
        let first = timeout(Duration::from_millis(15), async {
            if partial.is_none() {
                partial = rx.recv().await;
            }
            rx.recv().await
        })
        .await;
        assert!(first.is_err());
        assert_eq!(partial, Some(1), "state survived the cancellation");
        tx.send(2).await.unwrap();
        assert_eq!((partial, rx.recv().await), (Some(1), Some(2)));
    });
}

// ---------------------------------------------------------------------------
// Q11. async recursion
// ---------------------------------------------------------------------------
/// ANSWER: An `async fn` that awaits itself would produce a state machine that CONTAINS itself, so its size would be
/// infinite (same problem as a recursive struct, E0733: "recursion in an async fn requires boxing"). Break the
/// cycle with indirection: return `Pin<Box<dyn Future<Output = T> + Send>>` (via `Box::pin(async move { .. })`). Since Rust
/// 1.77 it is enough to box the RECURSIVE CALL inside the async fn (`Box::pin(f(n - 1)).await`). The `async-recursion`
/// crate automates the older form.
///
/// REALLY TESTING: relation between futures as sized state machines and recursion.
/// WRONG: "async functions cannot be recursive".
/// FOLLOW-UP: is there heap allocation per level? Yes, one box per recursive call.
async fn fib(n: u32) -> u64 {
    if n < 2 {
        return n as u64;
    }
    // FAILING (E0733 before boxing):   fib(n - 1).await + fib(n - 2).await
    let a = Box::pin(fib(n - 1)).await;
    let b = Box::pin(fib(n - 2)).await;
    a + b
}
fn sum_tree<'a>(depth: u32) -> Pin<Box<dyn Future<Output = u32> + Send + 'a>> {
    Box::pin(async move {
        if depth == 0 {
            1
        } else {
            sum_tree(depth - 1).await + sum_tree(depth - 1).await // classic explicit form
        }
    })
}
fn q11_async_recursion() {
    assert_eq!(block_on(fib(15)), 610);
    assert_eq!(block_on(sum_tree(4)), 16);
}

// ---------------------------------------------------------------------------
// Q12. streams vs iterators
// ---------------------------------------------------------------------------
/// ANSWER: An `Iterator` yields items synchronously via `next() -> Option<T>`. A `Stream` (`futures::Stream`, `tokio_stream`)
/// is its async twin: `poll_next(Pin<&mut Self>, &mut Context) -> Poll<Option<T>>`; the next item may not be ready yet, so
/// the consumer awaits it (`while let Some(x) = stream.next().await`). Typical sources: channel receivers, sockets, intervals,
/// paginated APIs. Combinators mirror iterators (`map`, `filter`, `buffer_unordered`, `throttle`). std has no stable
/// `Stream` yet, so crates provide it; a channel receiver loop (below) is the dependency-free equivalent. A hand-rolled
/// stream is a state machine exactly like a hand-written Future.
///
/// REALLY TESTING: sync vs async sequences, and `async` iterators not being in std.
/// WRONG: "a Stream is an Iterator<Item = Future>" (that is a different thing: it completes items concurrently).
/// FOLLOW-UP: backpressure? Bounded channels, `buffer(n)`.
trait MiniStream {
    type Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
}
struct Ticks {
    n: u32,
    max: u32,
    pending_toggle: bool,
}
impl MiniStream for Ticks {
    type Item = u32;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<u32>> {
        self.pending_toggle = !self.pending_toggle;
        if self.pending_toggle {
            cx.waker().wake_by_ref(); // simulate "not ready yet" every other poll
            return Poll::Pending;
        }
        if self.n >= self.max {
            return Poll::Ready(None);
        }
        self.n += 1;
        Poll::Ready(Some(self.n))
    }
}
async fn next_of<S: MiniStream + Unpin>(s: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| Pin::new(&mut *s).poll_next(cx)).await
}
fn q12_streams() {
    let items = block_on(async {
        let mut t = Ticks {
            n: 0,
            max: 3,
            pending_toggle: false,
        };
        let mut v = Vec::new();
        while let Some(x) = next_of(&mut t).await {
            v.push(x);
        }
        v
    });
    assert_eq!(items, [1, 2, 3]);
    // The dependency-free everyday stream: a channel receiver.
    let rt = current_thread_rt();
    let got = rt.block_on(async {
        let (tx, mut rx) = mpsc::channel(4);
        tokio::spawn(async move {
            for i in 0..3u32 {
                tx.send(i * i).await.unwrap();
            }
        });
        let mut v = Vec::new();
        while let Some(x) = rx.recv().await {
            v.push(x);
        }
        v
    });
    assert_eq!(got, [0, 1, 4]);
}

// ---------------------------------------------------------------------------
// Q13. async fn in traits
// ---------------------------------------------------------------------------
/// ANSWER: `async fn` in a trait desugars to a method returning `impl Future<Output = T>` (return-position impl Trait in
/// traits, RPITIT), i.e. an anonymous associated type different per impl and borrowing `&self`. That needed generic
/// associated types and RPITIT support in the compiler; both are stable since Rust 1.75, so `trait T { async fn f(&self); }`
/// works. Limitations: (1) the trait is NOT dyn-compatible (the future type is unknown/unsized; use the `async-trait` crate
/// which boxes, or an enum, or generics); (2) the returned future's `Send`-ness is not promised to callers, so for
/// `tokio::spawn` write `fn f(&self) -> impl Future<Output = T> + Send` in the trait (or add `trait_variant`).
///
/// REALLY TESTING: why it took years, and the Send bound and dyn trade-offs.
/// WRONG: "async fn in traits is just sugar for Box<dyn Future>" (static, no allocation, unlike async-trait).
/// FOLLOW-UP: what did people use before 1.75? `#[async_trait]` (boxes every call).
trait Repo {
    async fn get(&self, id: u32) -> String; // unspecified Send-ness
    fn get_send(&self, id: u32) -> impl Future<Output = String> + Send; // explicit Send promise
}
struct Mem;
impl Repo for Mem {
    async fn get(&self, id: u32) -> String {
        sleep(Duration::from_millis(1)).await;
        format!("row{id}")
    }
    fn get_send(&self, id: u32) -> impl Future<Output = String> + Send {
        async move { format!("sendrow{id}") }
    }
}
async fn use_repo<R: Repo>(r: &R) -> String {
    r.get(1).await
}
fn q13_async_in_traits() {
    let rt = multi_thread_rt();
    rt.block_on(async {
        assert_eq!(use_repo(&Mem).await, "row1");
        // spawn requires Send: only the explicitly Send method (and concrete impl) works generically
        let h = tokio::spawn(async { Mem.get_send(2).await });
        assert_eq!(h.await.unwrap(), "sendrow2");
    });
    // FAILING (E0038): let r: Box<dyn Repo> = Box::new(Mem);   // `Repo` is not dyn compatible
    // FAILING (generic spawn): fn spawn_it<R: Repo + Send + Sync + 'static>(r: R) { tokio::spawn(async move { r.get(1).await }); }
    //   error: future cannot be sent between threads safely (callers of `get` get no Send guarantee)
}

// ---------------------------------------------------------------------------
// Q14. CPU-bound work
// ---------------------------------------------------------------------------
/// ANSWER: Async is for waiting (I/O), not computing. A CPU-heavy loop inside a task holds its worker thread and starves
/// other tasks (Q7). Offload it: `tokio::task::spawn_blocking(|| heavy())` runs it on the blocking pool and gives an awaitable
/// `JoinHandle` (cannot be cancelled once started; the pool defaults to 512 threads); for data parallelism use rayon
/// and return the result through a `oneshot` channel (`rayon::spawn` + `tx.send`), or a dedicated thread. For code that must
/// stay in async but is merely long, insert `tokio::task::yield_now().await` periodically.
///
/// REALLY TESTING: whether you understand the executor does not make CPU work parallel by itself.
/// WRONG: "just tokio::spawn it" (it will block a worker).
/// FOLLOW-UP: how many worker threads does the multi-thread runtime have by default? Number of CPU cores.
fn heavy(n: u64) -> u64 {
    (1..=n).fold(0u64, |a, x| a.wrapping_add(x * x))
}
fn q14_cpu_bound() {
    let rt = multi_thread_rt();
    rt.block_on(async {
        let h = tokio::task::spawn_blocking(|| heavy(2_000_000));
        // Meanwhile async tasks stay responsive:
        let ticker = tokio::spawn(async {
            let mut ticks = 0;
            for _ in 0..3 {
                sleep(Duration::from_millis(1)).await;
                ticks += 1;
            }
            ticks
        });
        assert_eq!(ticker.await.unwrap(), 3);
        assert_eq!(h.await.unwrap(), heavy(2_000_000));
        // std thread + oneshot as the rayon-style bridge:
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(heavy(1000));
        });
        assert_eq!(rx.await.unwrap(), heavy(1000));
        // Cooperative yielding inside a long async loop:
        let mut acc = 0u64;
        for chunk in 0..4u64 {
            acc += heavy(1000 + chunk);
            tokio::task::yield_now().await;
        }
        assert!(acc > 0);
    });
}

// ---------------------------------------------------------------------------
// Q15. when NOT to use async
// ---------------------------------------------------------------------------
/// ANSWER: Async pays off for MANY concurrent, mostly-waiting operations (thousands of sockets, timers, fan-out RPC), where
/// OS threads would cost ~MBs of stack and context switches each, while a task is a small heap state machine. Do NOT use it
/// for: CPU-bound number crunching (use threads/rayon); a handful of concurrent operations (threads are simpler and
/// debuggable); blocking-only libraries; simple scripts and CLIs; code where "function colouring" cost (async infects
/// callers, runtime choice, `Send`/`'static` bounds, cancellation hazards, poorer stack traces) outweighs scalability.
/// Mixing is normal: async at the edge, blocking pool or threads for compute.
///
/// REALLY TESTING: pragmatism over hype.
/// WRONG: "async is always faster".
/// FOLLOW-UP: what is the memory cost of a task vs a thread? Hundreds of bytes vs tens of KB-MB of stack.
fn q15_when_not_async() {
    let rt = multi_thread_rt();
    // Where async shines: 10_000 concurrent waiters on 2 worker threads.
    let t = Instant::now();
    let done = rt.block_on(async {
        let hs: Vec<_> = (0..10_000u32)
            .map(|i| {
                tokio::spawn(async move {
                    sleep(Duration::from_millis(20)).await;
                    i
                })
            })
            .collect();
        let mut n = 0u64;
        for h in hs {
            n += h.await.unwrap() as u64;
        }
        n
    });
    assert_eq!(done, 49_995_000);
    let elapsed = t.elapsed();
    assert!(elapsed < Duration::from_secs(10));
    println!(
        "  [info] 10_000 concurrent sleeping tasks finished in {elapsed:?} on 2 worker threads (10_000 OS threads would cost GBs of stack)"
    );
    // A task's state machine is tiny compared with a thread stack (default 2 MiB reserved for spawned std threads):
    let fut = async {
        sleep(Duration::from_millis(1)).await;
        1u8
    };
    assert!(size_of_val(&fut) < 256);
    // Where threads are just as good: a few blocking workers.
    let handles: Vec<_> = (0..4)
        .map(|i| std::thread::spawn(move || heavy(1000 + i)))
        .collect();
    assert_eq!(handles.into_iter().map(|h| h.join().unwrap()).count(), 4);
}

fn main() {
    q1_async_fn_is_lazy();
    println!("Q1: ok");
    q2_poll_and_waker();
    println!("Q2: ok");
    q3_pinning();
    println!("Q3: ok");
    q4_spawn_join_select();
    println!("Q4: ok");
    q5_joinhandle_panics();
    println!("Q5: ok");
    q6_send_across_await();
    println!("Q6: ok");
    q7_blocking_the_runtime();
    println!("Q7: ok");
    q8_mutex_choice();
    println!("Q8: ok");
    q9_cancellation();
    println!("Q9: ok");
    q10_select_cancel_safety();
    println!("Q10: ok");
    q11_async_recursion();
    println!("Q11: ok");
    q12_streams();
    println!("Q12: ok");
    q13_async_in_traits();
    println!("Q13: ok");
    q14_cpu_bound();
    println!("Q14: ok");
    q15_when_not_async();
    println!("Q15: ok");
}
