from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

WITHIN = """
fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}
"""

# A correct block_on and a timer future for tests of other problems; defined in the test files (not exported by the solution).
BLOCK_ON_HELPER = """
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct W {
        t: std::thread::Thread,
        flag: AtomicBool,
    }
    impl Wake for W {
        fn wake(self: Arc<Self>) {
            self.flag.store(true, Ordering::Release);
            self.t.unpark();
        }
    }
    let w = Arc::new(W { t: std::thread::current(), flag: AtomicBool::new(false) });
    let waker = Waker::from(w.clone());
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        while !w.flag.swap(false, Ordering::Acquire) {
            std::thread::park();
        }
    }
}
"""

DELAY_HELPER = """
struct Delay {
    shared: std::sync::Arc<std::sync::Mutex<(bool, Option<std::task::Waker>)>>,
    started: bool,
    ms: u64,
}

fn delay(ms: u64) -> Delay {
    Delay { shared: std::sync::Arc::new(std::sync::Mutex::new((false, None))), started: false, ms }
}

impl std::future::Future for Delay {
    type Output = ();
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        let mut g = self.shared.lock().unwrap();
        if g.0 {
            return std::task::Poll::Ready(());
        }
        g.1 = Some(cx.waker().clone());
        drop(g);
        if !self.started {
            self.started = true;
            let sh = self.shared.clone();
            let ms = self.ms;
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                let mut g = sh.lock().unwrap();
                g.0 = true;
                if let Some(w) = g.1.take() {
                    drop(g);
                    w.wake();
                }
            });
        }
        std::task::Poll::Pending
    }
}
"""

# ---------------------------------------------------------------------------------------------------------------
BLOCKON = """
use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

/// What a `Waker` does here: remember that a wake happened, and unpark the thread that is blocked in `block_on`.
struct ThreadWaker {
    thread: Thread,
    woken: AtomicBool,
}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// Runs `fut` to completion on the current thread, sleeping (not spinning) while it is pending.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut); // pinned on this stack frame: the future never moves again
    let tw = Arc::new(ThreadWaker { thread: thread::current(), woken: AtomicBool::new(false) });
    let waker = Waker::from(tw.clone());
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        // `park` can return spuriously, and a wake can arrive during the poll above: the flag is the truth.
        while !tw.woken.swap(false, Ordering::Acquire) {
            thread::park();
        }
    }
}
"""

BLOCKON_STARTER = """
use std::future::Future;

/// Runs `fut` to completion on the current thread. While the future is pending the thread must sleep
/// (`thread::park`), and a `Waker` you build must wake it (`Thread::unpark`); it must not spin.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    todo!()
}
"""

P.append(dict(
    slug="block-on", title="block_on: run a future with a hand-made Waker", level="medium", stage="wake-and-poll",
    tags=["Future", "Waker", "Context", "Pin", "pin!", "Wake trait", "park/unpark", "lost wakeup"],
    teaches=["What `async` really is: a `Future` is polled; `Pending` means 'I stored your `Waker` and will call it when progress is possible'.",
             "Building a `Waker` from an `Arc<impl Wake>`, and the flag-plus-`park` pattern that cannot lose a wake-up that happens between `poll` and `park`.",
             "Pinning a future on the stack with `std::pin::pin!` so a self-referential state machine never moves."],
    statement="""
        Write the smallest async runtime: `block_on(fut)` drives one future to completion on the calling thread.

        Poll the future with a `Context` built from a `Waker` you create. If it returns `Pending`, the thread must **sleep** until the
        waker is called (from any thread, at any time, including during the `poll` itself or before you park) and then poll
        again. It must not busy-loop: the tests count polls.

        Build the `Waker` from your own type with `std::task::Wake` (`Waker::from(Arc<W>)`), and pin the future with `pin!`.
    """,
    starter=BLOCKON_STARTER,
    solution=BLOCKON,
    visible=[
        T("ready_value", "an async block", "block_on(async { 1 + 2 })", "3"),
        T("awaits_chain", "await inside await", "block_on(async { let a = async { 20 }.await; let b = async { 22 }.await; a + b })", "42"),
        T("self_waking_future", "pending 5 times, waking itself each time", "{ let mut n = 5; let mut polls = 0; let out = block_on(std::future::poll_fn(|cx| { polls += 1; if n == 0 { std::task::Poll::Ready(7) } else { n -= 1; cx.waker().wake_by_ref(); std::task::Poll::Pending } })); (out, polls) }", "(7, 6)"),
        T("borrows_from_the_caller", "a non-'static future", "{ let data = vec![1, 2, 3]; block_on(async { data.iter().sum::<i32>() }) }", "6"),
        T("moves_the_output_out", "a String comes back", "block_on(async { format!(\"{}-{}\", \"a\", 1) })", "\"a-1\".to_string()"),
    ],
    hidden=[
        WITHIN + DELAY_HELPER + """
        use std::future::Future;
        use std::time::{Duration, Instant};

        #[test]
        fn a_thread_wakes_it_and_it_does_not_spin() {
            let r = within(5000, || {
                let polls = std::cell::Cell::new(0);
                let t = Instant::now();
                block_on(async {
                    delay(30).await;
                    polls.set(polls.get() + 1);
                    delay(30).await;
                    polls.set(polls.get() + 1);
                    delay(30).await;
                    polls.set(polls.get() + 1);
                });
                (polls.get(), t.elapsed())
            });
            let (n, took) = r.expect("block_on hung: a wake from another thread was lost");
            check!("three sequential delays resumed three times", n, 3);
            assert!(took >= Duration::from_millis(85), "finished too early: {took:?}");
        }

        #[test]
        fn poll_count_stays_small_while_waiting() {
            let polls = within(5000, || {
                let polls = std::cell::Cell::new(0u32);
                let mut d = delay(120);
                block_on(std::future::poll_fn(|cx| {
                    polls.set(polls.get() + 1);
                    std::pin::Pin::new(&mut d).poll(cx)
                }));
                polls.get()
            })
            .expect("hung");
            assert!(polls <= 4, "polled {polls} times while waiting 120ms: that is spinning");
        }

        #[test]
        fn later_waits_do_not_spin_either() {
            let polls = within(5000, || {
                let polls = std::cell::Cell::new(0u32);
                let mut fut = Box::pin(async {
                    delay(40).await;
                    delay(40).await;
                    delay(40).await;
                });
                block_on(std::future::poll_fn(|cx| {
                    polls.set(polls.get() + 1);
                    fut.as_mut().poll(cx)
                }));
                polls.get()
            })
            .expect("hung");
            assert!(polls <= 8, "polled {polls} times across three 40ms waits: it stopped sleeping after the first wake");
        }

        #[test]
        fn many_independent_threads_wake_it_and_none_is_lost() {
            let ok = within(15_000, || {
                for _ in 0..300 {
                    // the wake can happen before block_on has parked; it must not be lost
                    let out = block_on(async {
                        let (tx, rx) = std::sync::mpsc::channel::<std::task::Waker>();
                        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let d2 = done.clone();
                        let h = std::thread::spawn(move || {
                            let w = rx.recv().unwrap();
                            d2.store(true, std::sync::atomic::Ordering::SeqCst);
                            w.wake();
                        });
                        let mut sent = false;
                        std::future::poll_fn(|cx| {
                            if done.load(std::sync::atomic::Ordering::SeqCst) {
                                return std::task::Poll::Ready(());
                            }
                            if !sent {
                                sent = true;
                                tx.send(cx.waker().clone()).unwrap();
                            }
                            std::task::Poll::Pending
                        })
                        .await;
                        h.join().unwrap();
                        1
                    });
                    assert_eq!(out, 1);
                }
                true
            });
            assert_eq!(ok, Some(true), "a wake-up was lost and block_on slept forever");
        }

        #[test]
        fn waker_clone_can_outlive_the_poll() {
            let r = within(5000, || {
                let stash: std::sync::Arc<std::sync::Mutex<Option<std::task::Waker>>> = Default::default();
                let s2 = stash.clone();
                let h = std::thread::spawn(move || loop {
                    if let Some(w) = s2.lock().unwrap().take() {
                        std::thread::sleep(Duration::from_millis(40));
                        w.wake();
                        return;
                    }
                    std::thread::yield_now();
                });
                let mut first = true;
                block_on(std::future::poll_fn(move |cx| {
                    if first {
                        first = false;
                        *stash.lock().unwrap() = Some(cx.waker().clone());
                        std::task::Poll::Pending
                    } else {
                        std::task::Poll::Ready("woken")
                    }
                }))
                .to_string()
                    + &{ h.join().unwrap(); String::new() }
            });
            assert_eq!(r.as_deref(), Some("woken"));
        }

        #[test]
        fn nested_block_on_is_fine() {
            let r = within(5000, || block_on(async { block_on(async { 20 }) + block_on(async { delay(10).await; 22 }) }));
            assert_eq!(r, Some(42));
        }

        #[test]
        fn a_deep_chain_of_awaits() {
            fn count(n: u32) -> std::pin::Pin<Box<dyn std::future::Future<Output = u32>>> {
                Box::pin(async move { if n == 0 { 0 } else { 1 + count(n - 1).await } })
            }
            check!("recursion through boxed futures", block_on(count(500)), 500);
        }

        #[test]
        fn any_output_type_comes_back() {
            check!("a tuple of owned values", block_on(async { (String::from("a"), vec![1, 2], Some(3.5f64)) }), (String::from("a"), vec![1, 2], Some(3.5f64)));
            check!("unit", block_on(async {}), ());
        }

        #[test]
        fn a_future_that_returns_ready_immediately_is_polled_once() {
            let mut polls = 0;
            let v = block_on(std::future::poll_fn(|_| {
                polls += 1;
                std::task::Poll::Ready(9)
            }));
            check!("ready on the first poll", (v, polls), (9, 1));
        }
        """,
    ],
    wrong=dict(
        gives_up_on_pending=sub(BLOCKON, "        // `park` can return spuriously, and a wake can arrive during the poll above: the flag is the truth.\n        while !tw.woken.swap(false, Ordering::Acquire) {\n            thread::park();\n        }", "        panic!(\"pending\");"),
        spins_instead_of_parking=sub(BLOCKON, "while !tw.woken.swap(false, Ordering::Acquire) {\n            thread::park();\n        }", "tw.woken.swap(false, Ordering::Acquire);"),
        flag_never_cleared=sub(BLOCKON, "while !tw.woken.swap(false, Ordering::Acquire) {", "while !tw.woken.load(Ordering::Acquire) {"),
        unparks_the_wrong_thread=sub(BLOCKON, "        self.thread.unpark();", "        thread::current().unpark();"),
    ),
    hints=[("approach", "A `Waker` is a handle that says 'poll this task again'. Here the task is the thread blocked in `block_on`, so `wake` = set a flag and `unpark` that thread. The loop is: poll; if `Ready`, return; else wait until the flag is set, clear it, repeat."),
           ("rust", "Implement `std::task::Wake` for a struct holding `Thread` (from `thread::current()`) and an `AtomicBool`, wrap it in an `Arc`, and turn it into a `Waker` with `Waker::from(arc)`. `Context::from_waker(&waker)` is what `poll` takes. `pin!(fut)` gives a `Pin<&mut F>`; call `.as_mut().poll(&mut cx)` each time."),
           ("edge", "Use the flag, not `park` alone: `park` may return without anyone waking you (spurious), and a wake may land between your `poll` and your `park`. `swap(false)` consumes the wake exactly once, and `unpark` before `park` leaves a token so the next `park` returns at once.")],
    notes=("This is the core of `futures::executor::block_on` and of `pollster`: about 30 lines. Everything bigger (tokio, async-std, smol) is this loop plus a scheduler, an I/O reactor and a timer wheel. Knowing that `await` is only 'poll, and if pending, return to whoever polled me' removes most of the mystery from async Rust, including why a future that never stores its waker hangs forever.", "O(polls)", "O(1) extra"),
    follow_up="1) Remove the `woken` flag and rely on `park`/`unpark` alone: what breaks, and why is the flag still needed for spurious wake-ups? 2) Make a version that takes a deadline (`block_on_timeout`) with `park_timeout`. 3) Why must a pending future store the *latest* waker on every poll (see the oneshot problem)? 4) Replace `pin!` with `Box::pin`: what is the cost, and when is it required? 5) How does `Waker`'s `RawWaker` vtable work, and what is `Wake` saving you from writing by hand (read `std::task::Wake` and `RawWakerVTable`)?",
    related=["C4", "C3", "C1"],
))

# ---------------------------------------------------------------------------------------------------------------
ONESHOT = """
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

struct State<T> {
    value: Option<T>,
    waker: Option<Waker>, // the receiver's, refreshed on every poll
    tx_gone: bool,
    rx_gone: bool,
}

pub struct Sender<T> {
    state: Arc<Mutex<State<T>>>,
}

pub struct Receiver<T> {
    state: Arc<Mutex<State<T>>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecvError;

#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    Empty,
    Closed,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let state = Arc::new(Mutex::new(State { value: None, waker: None, tx_gone: false, rx_gone: false }));
    (Sender { state: state.clone() }, Receiver { state })
}

impl<T> Sender<T> {
    /// Hands the value over and wakes the receiver. `Err(value)` if the receiver is already gone.
    pub fn send(self, value: T) -> Result<(), T> {
        let waker = {
            let mut s = self.state.lock().unwrap();
            if s.rx_gone {
                return Err(value);
            }
            s.value = Some(value);
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake(); // after the lock is released: a woken task may run at once and want the lock
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.state.lock().unwrap().rx_gone
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let waker = {
            let mut s = self.state.lock().unwrap();
            s.tx_gone = true;
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake(); // a receiver waiting on a sender that vanished must learn about it
        }
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let mut s = self.state.lock().unwrap();
        match s.value.take() {
            Some(v) => Ok(v),
            None if s.tx_gone => Err(TryRecvError::Closed),
            None => Err(TryRecvError::Empty),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.state.lock().unwrap().rx_gone = true;
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut s = self.state.lock().unwrap();
        if let Some(v) = s.value.take() {
            return Poll::Ready(Ok(v));
        }
        if s.tx_gone {
            return Poll::Ready(Err(RecvError));
        }
        // The task may have moved to a different waker since the last poll: always keep the newest.
        match &s.waker {
            Some(w) if w.will_wake(cx.waker()) => {}
            _ => s.waker = Some(cx.waker().clone()),
        }
        Poll::Pending
    }
}
"""

ONESHOT_STARTER = """
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Sender<T> {
    // TODO: shared state: the value, the receiver's Waker, and whether each side is gone
    _marker: std::marker::PhantomData<T>,
}

pub struct Receiver<T> {
    _marker: std::marker::PhantomData<T>,
}

/// The sender was dropped without sending.
#[derive(Debug, PartialEq, Eq)]
pub struct RecvError;

#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    /// Nothing yet, the sender may still send.
    Empty,
    /// Nothing, and the sender is gone.
    Closed,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    todo!()
}

impl<T> Sender<T> {
    /// Hands the value over and wakes the receiver. `Err(value)` if the receiver is already gone.
    pub fn send(self, value: T) -> Result<(), T> {
        todo!()
    }

    pub fn is_closed(&self) -> bool {
        todo!()
    }
}

// TODO: `Drop` for `Sender` (wake the receiver: it must learn the sender is gone) and for `Receiver`.

impl<T> Receiver<T> {
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        todo!()
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}
"""

P.append(dict(
    slug="oneshot-channel", title="A oneshot channel whose receiver is a Future", level="medium", stage="wake-and-poll",
    tags=["Future", "Waker", "will_wake", "Mutex", "Drop", "oneshot", "async channels"],
    teaches=["The contract every leaf future follows: on `Pending`, store `cx.waker()`; whoever makes progress possible calls `wake`.",
             "Why you must replace the stored waker on every poll (the task can move between executors or be re-polled with a different waker).",
             "Closing: a receiver waiting on a dropped sender must be woken and told, or it hangs forever."],
    statement="""
        Implement `channel::<T>() -> (Sender<T>, Receiver<T>)`, a channel that carries **one** value.

        `Sender::send(self, v)` stores the value and wakes the receiver, or returns `Err(v)` if the receiver is already dropped.
        `Receiver<T>` is a `Future<Output = Result<T, RecvError>>`: `Ok(v)` when a value arrives, `Err(RecvError)` if the sender is
        dropped without sending. `try_recv` is the non-blocking version (`Empty` while the sender may still send, `Closed` once it cannot).

        On `Pending` the receiver stores the waker from **this** poll's context (replace the old one: the tests poll with
        two different wakers in turn) and the sender wakes it after releasing the lock.
    """,
    starter=ONESHOT_STARTER,
    solution=ONESHOT,
    visible=[
        BLOCK_ON_HELPER + DELAY_HELPER + """
        use std::task::{Context, Poll, Wake, Waker};
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

        struct Counter(AtomicUsize);
        impl Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        #[test]
        fn send_before_await() {
            let (tx, rx) = channel();
            tx.send(5).unwrap();
            check!("value waiting in the channel", block_on(rx), Ok(5));
        }

        #[test]
        fn send_after_pending_wakes_the_task() {
            let (tx, rx) = channel();
            let h = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(40));
                tx.send("late").unwrap();
            });
            check!("receiver slept, then got the value", block_on(rx), Ok("late"));
            h.join().unwrap();
        }

        #[test]
        fn dropped_sender_closes_the_channel() {
            let (tx, rx) = channel::<u8>();
            drop(tx);
            check!("no value will ever come", block_on(rx), Err(RecvError));
        }

        #[test]
        fn send_to_a_dropped_receiver_returns_the_value() {
            let (tx, rx) = channel();
            drop(rx);
            check!("the value is handed back", (tx.is_closed(), tx.send(String::from("x"))), (true, Err(String::from("x"))));
        }

        #[test]
        fn pending_stores_the_waker_and_send_calls_it_once() {
            let c = Arc::new(Counter(AtomicUsize::new(0)));
            let w = Waker::from(c.clone());
            let mut cx = Context::from_waker(&w);
            let (tx, mut rx) = channel::<u32>();
            check!("nothing sent yet", Pin::new(&mut rx).poll(&mut cx), Poll::Pending);
            check!("not woken before the send", c.0.load(Ordering::SeqCst), 0);
            tx.send(7).unwrap();
            check!("woken exactly once by the send", c.0.load(Ordering::SeqCst), 1);
            check!("and now ready", Pin::new(&mut rx).poll(&mut cx), Poll::Ready(Ok(7)));
        }
        """.replace("use std::task::{Context", "use std::future::Future;\n        use std::pin::Pin;\n        use std::task::{Context"),
    ],
    hidden=[
        WITHIN + BLOCK_ON_HELPER + DELAY_HELPER + """
        use std::future::Future;
        use std::pin::Pin;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct Counter(AtomicUsize);
        impl Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        #[test]
        fn the_newest_waker_is_the_one_woken() {
            let a = Arc::new(Counter(AtomicUsize::new(0)));
            let b = Arc::new(Counter(AtomicUsize::new(0)));
            let (wa, wb) = (Waker::from(a.clone()), Waker::from(b.clone()));
            let (tx, mut rx) = channel::<u32>();
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&wa));
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&wb));
            tx.send(1).unwrap();
            check!("old waker not called, new waker called once", (a.0.load(Ordering::SeqCst), b.0.load(Ordering::SeqCst)), (0, 1));
        }

        #[test]
        fn dropping_the_sender_wakes_a_waiting_receiver() {
            let c = Arc::new(Counter(AtomicUsize::new(0)));
            let w = Waker::from(c.clone());
            let (tx, mut rx) = channel::<u32>();
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&w));
            drop(tx);
            check!("woken by the drop", c.0.load(Ordering::SeqCst), 1);
            check!("and it reports closed", Pin::new(&mut rx).poll(&mut Context::from_waker(&w)), Poll::Ready(Err(RecvError)));
        }

        #[test]
        fn a_waiting_receiver_learns_the_sender_vanished() {
            let r = within(5000, || {
                let (tx, rx) = channel::<u32>();
                let h = std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    drop(tx);
                });
                let out = block_on(rx);
                h.join().unwrap();
                out
            });
            assert_eq!(r, Some(Err(RecvError)), "the receiver hung or got a wrong result");
        }

        #[test]
        fn try_recv_states() {
            let (tx, mut rx) = channel::<u32>();
            check!("empty while the sender lives", rx.try_recv(), Err(TryRecvError::Empty));
            tx.send(3).unwrap();
            check!("the value", rx.try_recv(), Ok(3));
            check!("then closed (the sender is consumed)", rx.try_recv(), Err(TryRecvError::Closed));
            let (tx2, mut rx2) = channel::<u32>();
            drop(tx2);
            check!("closed without a value", rx2.try_recv(), Err(TryRecvError::Closed));
        }

        #[test]
        fn value_sent_just_before_the_sender_drops_is_not_lost() {
            let (tx, rx) = channel();
            tx.send(String::from("kept")).unwrap(); // the sender is dropped at the end of send
            check!("the value wins over closed", block_on(rx), Ok(String::from("kept")));
        }

        #[test]
        fn many_channels_across_threads() {
            let r = within(20_000, || {
                let mut handles = Vec::new();
                for i in 0..200u32 {
                    let (tx, rx) = channel();
                    handles.push(std::thread::spawn(move || block_on(rx)));
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis((i % 5) as u64));
                        tx.send(i).unwrap();
                    });
                }
                handles.into_iter().map(|h| h.join().unwrap().unwrap()).sum::<u32>()
            });
            assert_eq!(r, Some((0..200u32).sum()), "a channel lost its value or hung");
        }

        #[test]
        fn sending_a_large_value_moves_it() {
            let (tx, rx) = channel();
            let big: Vec<u64> = (0..100_000).collect();
            let ptr = big.as_ptr();
            tx.send(big).unwrap();
            let got = block_on(rx).unwrap();
            check!("the same allocation arrived", (got.as_ptr() == ptr, got.len()), (true, 100_000));
        }

        #[test]
        fn dropping_the_receiver_after_pending_is_clean() {
            let c = Arc::new(Counter(AtomicUsize::new(0)));
            let w = Waker::from(c.clone());
            let (tx, mut rx) = channel::<u32>();
            let _ = Pin::new(&mut rx).poll(&mut Context::from_waker(&w));
            drop(rx);
            check!("the sender sees it", (tx.is_closed(), tx.send(1)), (true, Err(1)));
        }
        """,
    ],
    wrong=dict(
        send_forgets_to_wake=sub(ONESHOT, "        if let Some(w) = waker {\n            w.wake(); // after the lock is released: a woken task may run at once and want the lock\n        }\n        Ok(())", "        let _ = waker;\n        Ok(())"),
        drop_does_not_wake=sub(ONESHOT, "        if let Some(w) = waker {\n            w.wake(); // a receiver waiting on a sender that vanished must learn about it\n        }\n", "        let _ = waker;\n"),
        keeps_the_first_waker=sub(ONESHOT, "match &s.waker {\n            Some(w) if w.will_wake(cx.waker()) => {}\n            _ => s.waker = Some(cx.waker().clone()),\n        }", "if s.waker.is_none() {\n            s.waker = Some(cx.waker().clone());\n        }"),
        send_ignores_a_dropped_receiver=sub(ONESHOT, "            if s.rx_gone {\n                return Err(value);\n            }\n", ""),
        closed_reports_empty=sub(ONESHOT, "None if s.tx_gone => Err(TryRecvError::Closed),", "None if s.tx_gone => Err(TryRecvError::Empty),"),
    ),
    hints=[("approach", "Shared state behind `Arc<Mutex<_>>`: `value: Option<T>`, `waker: Option<Waker>`, `tx_gone`, `rx_gone`. `poll`: value present → `Ready(Ok)`; sender gone → `Ready(Err)`; else store the waker and return `Pending`. `send`: store the value, take the waker, wake it."),
           ("rust", "`cx.waker().clone()` is cheap (a refcount). `Waker::will_wake` tells you whether the stored waker is already equivalent, so you can skip the clone. Call `wake()` after the `MutexGuard` is dropped (put the lock in an inner block) so the woken task cannot immediately block on your lock."),
           ("edge", "Implement `Drop` for `Sender`: set `tx_gone` and wake the receiver. `send(self, ..)` consumes the sender, so its `Drop` runs at the end of `send`: a value sent just before must still win over 'closed'. Check `value` before `tx_gone` in `poll`.")],
    notes=("`tokio::sync::oneshot` is this plus an atomic-state fast path and a `close()` on the receiver; `futures::channel::oneshot` is the same shape. Almost every async primitive (mutex, semaphore, channel, timer) is 'a queue of wakers plus a condition': once you can write this one, `tokio::sync::Notify` and `Semaphore` are variations.", "O(1)", "O(1)"),
    follow_up="1) Replace the `Mutex` with an atomic state machine (EMPTY / WAITING / READY / CLOSED) and a `UnsafeCell<Option<T>>`: which transitions need Acquire/Release? 2) Add `Receiver::close()` and `Sender::closed()` (a future that completes when the receiver is dropped), which the tokio `select!` pattern uses. 3) Why must the waker be woken outside the lock, and what does `will_wake` buy you? 4) Build an unbounded mpsc channel on top by keeping a `VecDeque<T>` and one waker: what goes wrong with several receivers? 5) What happens if `poll` is called again after it returned `Ready`? Make that a panic like `std`'s futures, or return `Pending` forever?",
    related=["C2", "C4", "C3"],
))

# ---------------------------------------------------------------------------------------------------------------
EXEC = """
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};

type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Shared {
    queue: Mutex<VecDeque<Arc<Task>>>, // tasks that have been woken and need a poll, oldest first
    cv: Condvar,
    pending: AtomicUsize, // spawned and not yet finished
}

struct Task {
    future: Mutex<Option<BoxFuture>>, // None once finished
    shared: Arc<Shared>,
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        let shared = self.shared.clone();
        shared.queue.lock().unwrap().push_back(self);
        shared.cv.notify_one();
    }
}

struct JoinState<T> {
    result: Option<T>,
    waker: Option<Waker>,
}

/// Awaiting a `JoinHandle` gives the task's output. Dropping it does not cancel the task.
pub struct JoinHandle<T> {
    state: Arc<Mutex<JoinState<T>>>,
}

impl<T> JoinHandle<T> {
    /// The output, if the task has finished and nobody took it yet.
    pub fn take(&self) -> Option<T> {
        self.state.lock().unwrap().result.take()
    }

    pub fn is_finished(&self) -> bool {
        self.state.lock().unwrap().result.is_some()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut s = self.state.lock().unwrap();
        match s.result.take() {
            Some(v) => Poll::Ready(v),
            None => {
                s.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

/// A future that is pending exactly once and asks to be polled again at once: it sends the task to the back of the queue.
pub struct YieldNow(bool);

pub fn yield_now() -> YieldNow {
    YieldNow(false)
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[derive(Clone)]
pub struct Executor {
    shared: Arc<Shared>,
}

impl Executor {
    pub fn new() -> Self {
        Executor { shared: Arc::new(Shared { queue: Mutex::new(VecDeque::new()), cv: Condvar::new(), pending: AtomicUsize::new(0) }) }
    }

    pub fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let state = Arc::new(Mutex::new(JoinState { result: None, waker: None }));
        let st = state.clone();
        let wrapped = async move {
            let out = fut.await;
            let joiner = {
                let mut s = st.lock().unwrap();
                s.result = Some(out);
                s.waker.take()
            };
            if let Some(w) = joiner {
                w.wake();
            }
        };
        self.shared.pending.fetch_add(1, Ordering::SeqCst);
        let task = Arc::new(Task { future: Mutex::new(Some(Box::pin(wrapped))), shared: self.shared.clone() });
        self.shared.queue.lock().unwrap().push_back(task);
        self.shared.cv.notify_one();
        JoinHandle { state }
    }

    /// Runs tasks until every spawned task has finished (including ones spawned while running). Sleeps when nothing is
    /// ready but tasks are still waiting for a wake-up from elsewhere.
    pub fn run(&self) {
        loop {
            let task = {
                let mut q = self.shared.queue.lock().unwrap();
                loop {
                    if let Some(t) = q.pop_front() {
                        break t;
                    }
                    if self.shared.pending.load(Ordering::SeqCst) == 0 {
                        return;
                    }
                    q = self.shared.cv.wait(q).unwrap();
                }
            };
            let waker = Waker::from(task.clone());
            let mut cx = Context::from_waker(&waker);
            let mut slot = task.future.lock().unwrap();
            if let Some(mut fut) = slot.take() {
                match fut.as_mut().poll(&mut cx) {
                    Poll::Pending => *slot = Some(fut),
                    Poll::Ready(()) => {
                        self.shared.pending.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            }
        }
    }
}
"""

EXEC_STARTER = """
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A single-threaded executor: `spawn` queues tasks, `run` polls them until all have finished.
/// Tasks are woken by their `Waker` (from any thread) and polled in the order they were woken.
#[derive(Clone)]
pub struct Executor {
    // TODO: shared state: a queue of woken tasks, a Condvar for sleeping, and a count of unfinished tasks
    _marker: (),
}

/// Awaiting a `JoinHandle` gives the task's output. Dropping it does not cancel the task.
pub struct JoinHandle<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> JoinHandle<T> {
    /// The output, if the task has finished and nobody took it yet.
    pub fn take(&self) -> Option<T> {
        todo!()
    }

    pub fn is_finished(&self) -> bool {
        todo!()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        todo!()
    }
}

/// A future that is pending exactly once and asks to be polled again at once: it sends the task to the back of the queue.
pub struct YieldNow(bool);

pub fn yield_now() -> YieldNow {
    todo!()
}

impl Future for YieldNow {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        todo!()
    }
}

impl Executor {
    pub fn new() -> Self {
        todo!()
    }

    /// Queues `fut` as a new task. `Send` because a waker (which holds the task) may be called from any thread.
    pub fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        todo!()
    }

    /// Runs tasks until every spawned task has finished (including ones spawned while running). Sleeps when nothing is
    /// ready but tasks are still waiting for a wake-up from elsewhere.
    pub fn run(&self) {
        todo!()
    }
}
"""

P.append(dict(
    slug="mini-executor", title="A task executor with spawn, join handles and wakers", level="hard", stage="build-it",
    tags=["executor", "Waker", "Wake trait", "Pin<Box<dyn Future>>", "JoinHandle", "yield_now", "run queue", "Condvar"],
    teaches=["The shape of every async runtime: a run queue of tasks, a `Waker` that re-queues its task, and a loop that polls the front.",
             "`Pin<Box<dyn Future + Send>>` as the type-erased task, and `Arc<Task>` + `Wake` as the way a waker owns its task.",
             "Cooperative scheduling: `yield_now` is pending-once-and-wake-self, and FIFO order is what makes tasks take turns."],
    statement="""
        Build a single-threaded executor. `spawn(fut)` wraps the future in a task, queues it, and returns a `JoinHandle<T>` that is
        itself a `Future` (awaiting it yields the task's output; `take()` reads the output after `run`). `run()` polls queued
        tasks **first-woken, first-polled** until every spawned task has finished, including tasks spawned from inside tasks.

        A task's `Waker` must re-queue **that task** and notify the executor, from any thread: a task waiting for a timer thread
        or another thread's channel is woken while `run` sleeps on a `Condvar`. `run` returns only when no task is unfinished (not
        merely when the queue is momentarily empty). `yield_now().await` lets other tasks run once.

        Also provide `Executor: Clone` (clones share the queue, so a task can spawn more tasks).
    """,
    starter=EXEC_STARTER,
    solution=EXEC,
    visible=[
        """
        #[test]
        fn runs_a_task() {
            let ex = Executor::new();
            let h = ex.spawn(async { 21 * 2 });
            check!("not finished before run", h.is_finished(), false);
            ex.run();
            check!("output after run", h.take(), Some(42));
        }

        #[test]
        fn run_with_nothing_to_do_returns() {
            let ex = Executor::new();
            ex.run();
            ex.run();
        }

        #[test]
        fn yielding_tasks_take_turns() {
            use std::sync::{Arc, Mutex};
            let ex = Executor::new();
            let log = Arc::new(Mutex::new(Vec::new()));
            for id in 0..2u32 {
                let log = log.clone();
                ex.spawn(async move {
                    for step in 0..3u32 {
                        log.lock().unwrap().push((id, step));
                        yield_now().await;
                    }
                });
            }
            ex.run();
            check!("interleaved in FIFO order", log.lock().unwrap().clone(), vec![(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)]);
        }

        #[test]
        fn a_task_awaits_another_through_its_join_handle() {
            let ex = Executor::new();
            let child = ex.spawn(async { yield_now().await; 41 });
            let parent = ex.spawn(async move { child.await + 1 });
            ex.run();
            check!("the parent saw the child's output", parent.take(), Some(42));
        }

        #[test]
        fn tasks_can_spawn_tasks() {
            let ex = Executor::new();
            let ex2 = ex.clone();
            let top = ex.spawn(async move {
                let a = ex2.spawn(async { 1 });
                let b = ex2.spawn(async { yield_now().await; 2 });
                a.await + b.await
            });
            ex.run();
            check!("sum of the spawned children", top.take(), Some(3));
        }
        """,
    ],
    hidden=[
        WITHIN + DELAY_HELPER + """
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        #[test]
        fn run_sleeps_until_another_thread_wakes_a_task() {
            let r = within(5000, || {
                let ex = Executor::new();
                let h = ex.spawn(async { delay(60).await; "done" });
                let t = Instant::now();
                ex.run();
                (h.take(), t.elapsed())
            });
            let (out, took) = r.expect("run() hung or returned early");
            check!("the task finished with its output", out, Some("done"));
            assert!(took >= Duration::from_millis(55), "run returned after {took:?}, before the timer fired");
        }

        #[test]
        fn run_does_not_return_while_a_task_is_waiting() {
            let r = within(5000, || {
                let ex = Executor::new();
                let a = ex.spawn(async { delay(80).await; 1 });
                let b = ex.spawn(async { 2 });
                ex.run();
                (a.take(), b.take())
            });
            assert_eq!(r, Some((Some(1), Some(2))), "run() returned while task a was still waiting for its timer");
        }

        #[test]
        fn timers_run_concurrently_not_one_after_another() {
            let r = within(10_000, || {
                let ex = Executor::new();
                let done = Arc::new(AtomicUsize::new(0));
                for _ in 0..200 {
                    let done = done.clone();
                    ex.spawn(async move {
                        delay(100).await;
                        done.fetch_add(1, Ordering::SeqCst);
                    });
                }
                let t = Instant::now();
                ex.run();
                (done.load(Ordering::SeqCst), t.elapsed())
            });
            let (n, took) = r.expect("hung");
            check!("every task finished", n, 200);
            assert!(took < Duration::from_millis(2000), "200 x 100ms timers took {took:?}: tasks ran one at a time");
        }

        #[test]
        fn join_handle_wakes_a_waiting_parent() {
            let r = within(5000, || {
                let ex = Executor::new();
                let child = ex.spawn(async { delay(40).await; 6 });
                let parent = ex.spawn(async move { child.await * 7 });
                ex.run();
                parent.take()
            });
            assert_eq!(r, Some(Some(42)), "the parent was never woken when the child finished");
        }

        #[test]
        fn dropping_a_join_handle_does_not_cancel_the_task() {
            let ex = Executor::new();
            let flag = Arc::new(AtomicUsize::new(0));
            let f2 = flag.clone();
            drop(ex.spawn(async move {
                yield_now().await;
                f2.store(7, Ordering::SeqCst);
            }));
            ex.run();
            check!("the detached task still ran", flag.load(Ordering::SeqCst), 7);
        }

        #[test]
        fn fifo_order_for_plain_spawns() {
            let ex = Executor::new();
            let log = Arc::new(Mutex::new(Vec::new()));
            for i in 0..20 {
                let log = log.clone();
                ex.spawn(async move { log.lock().unwrap().push(i) });
            }
            ex.run();
            check!("spawn order is poll order", log.lock().unwrap().clone(), (0..20).collect::<Vec<_>>());
        }

        #[test]
        fn a_task_that_wakes_itself_while_being_polled() {
            let r = within(5000, || {
                let ex = Executor::new();
                let n = Arc::new(AtomicUsize::new(0));
                let n2 = n.clone();
                ex.spawn(async move {
                    for _ in 0..1000 {
                        n2.fetch_add(1, Ordering::SeqCst);
                        yield_now().await;
                    }
                });
                ex.run();
                n.load(Ordering::SeqCst)
            });
            assert_eq!(r, Some(1000));
        }

        #[test]
        fn many_tasks_with_random_yields() {
            let r = within(10_000, || {
                let mut rng = anneal_prelude::Rng::new(6701);
                let ex = Executor::new();
                let total = Arc::new(AtomicUsize::new(0));
                let mut want = 0;
                for _ in 0..2000 {
                    let yields = rng.below(6);
                    want += yields + 1;
                    let total = total.clone();
                    ex.spawn(async move {
                        for _ in 0..yields {
                            total.fetch_add(1, Ordering::SeqCst);
                            yield_now().await;
                        }
                        total.fetch_add(1, Ordering::SeqCst);
                    });
                }
                ex.run();
                (total.load(Ordering::SeqCst), want)
            });
            let (got, want) = r.expect("hung");
            check!("every step of every task ran exactly once", got, want);
        }

        #[test]
        fn a_deep_chain_of_joins() {
            let r = within(10_000, || {
                let ex = Executor::new();
                let mut prev = ex.spawn(async { 0u32 });
                for _ in 0..200 {
                    prev = ex.spawn(async move { yield_now().await; prev.await + 1 });
                }
                ex.run();
                prev.take()
            });
            assert_eq!(r, Some(Some(200)));
        }
        """,
    ],
    wrong=dict(
        lifo_queue=sub(EXEC, "if let Some(t) = q.pop_front() {", "if let Some(t) = q.pop_back() {"),
        returns_when_the_queue_is_empty=sub(EXEC, "if self.shared.pending.load(Ordering::SeqCst) == 0 {\n                        return;\n                    }\n                    q = self.shared.cv.wait(q).unwrap();", "return;"),
        wake_does_not_notify=sub(EXEC, "shared.queue.lock().unwrap().push_back(self);\n        shared.cv.notify_one();", "shared.queue.lock().unwrap().push_back(self);"),
        spawn_forgets_to_count=sub(EXEC, "self.shared.pending.fetch_add(1, Ordering::SeqCst);\n        let task", "let task"),
        finished_task_never_wakes_its_joiner=sub(EXEC, "            if let Some(w) = joiner {\n                w.wake();\n            }\n        };", "            let _ = joiner;\n        };"),
    ),
    hints=[("approach", "A `Task` holds `Mutex<Option<Pin<Box<dyn Future<Output = ()> + Send>>>>` and the shared queue. Its `Wake::wake` pushes `Arc<Task>` onto the queue and notifies a `Condvar`. `run` pops the front task, makes a waker from the task itself, polls it, puts the future back if `Pending`, and decrements a pending-count if `Ready`."),
           ("rust", "`spawn` wraps your future in `async move { let out = fut.await; store out in the join state; wake the joiner }`, so every task has `Output = ()` and can live in one queue. `Waker::from(arc_task)` needs `impl Wake for Task` (`self: Arc<Self>`). The `JoinHandle` shares an `Arc<Mutex<{ result, waker }>>` with that wrapper."),
           ("edge", "`run` must not return just because the queue is empty: wait on the `Condvar` while `pending > 0`. Increment `pending` in `spawn` before queueing. Wake the joiner after releasing the join-state lock. A task may be queued twice (woken twice before it ran): the second poll must simply find nothing to do or poll a still-live future harmlessly.")],
    notes=("This is the skeleton of `async-task` + a single-threaded `smol`/`tokio::task::LocalSet`: tokio adds work stealing across threads, an I/O reactor (`epoll`/`mio`) that calls wakers when sockets are ready, and a timer wheel. What the toy shows is where the pieces meet: wakers re-queue tasks, the queue gives fairness, and a runtime that blocks inside a task (a long `thread::sleep`) stalls every other task on that thread.", "O(1) per wake", "O(tasks)"),
    follow_up="1) Make `yield_now` fairer or unfair on purpose: what does a LIFO queue do to latency, and why does tokio use a LIFO slot for the task that was just woken? 2) Make `run` multi-threaded with N worker threads sharing the queue: what has to change (`pending` already atomic; one task must not be polled by two threads at once: the `Mutex<Option<future>>` slot)? 3) Add a timer: a `sleep(ms)` future backed by one timer thread with a `BinaryHeap` of deadlines instead of a thread per delay. 4) Add task cancellation (`JoinHandle::abort`) and say what must happen to the future's destructors. 5) Where is the `Pin` guarantee used here, and what would break if you moved a future out of its `Box` between polls?",
    related=["C4", "C2", "C1"],
))

STAGES = [("wake-and-poll", "Wake and poll", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("c5-async-internals", "C5", "Async internals", "C", "sde3", 5,
                    "How async Rust works underneath: a hand-made block_on and waker, a oneshot channel as a future, and a task executor.",
                    STAGES, P)
    print("C5", n)
