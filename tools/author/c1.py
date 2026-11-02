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
BQ = """
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PopError {
    Timeout,
    Closed,
}

struct State<T> {
    items: VecDeque<T>,
    cap: usize,
    closed: bool,
}

/// A bounded FIFO shared by threads: producers wait while it is full, consumers wait while it is empty.
pub struct BlockingQueue<T> {
    state: Mutex<State<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

impl<T> BlockingQueue<T> {
    /// Panics if `cap` is 0.
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "capacity must be at least 1");
        BlockingQueue { state: Mutex::new(State { items: VecDeque::new(), cap, closed: false }), not_empty: Condvar::new(), not_full: Condvar::new() }
    }

    /// Waits for room. Gives the value back if the queue is (or becomes) closed.
    pub fn push(&self, value: T) -> Result<(), T> {
        let mut s = self.state.lock().unwrap();
        while s.items.len() == s.cap && !s.closed {
            s = self.not_full.wait(s).unwrap();
        }
        if s.closed {
            return Err(value);
        }
        s.items.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Never waits: `Err` when full or closed.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        let mut s = self.state.lock().unwrap();
        if s.closed || s.items.len() == s.cap {
            return Err(value);
        }
        s.items.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Waits for an item. `None` once the queue is closed AND drained.
    pub fn pop(&self) -> Option<T> {
        let mut s = self.state.lock().unwrap();
        while s.items.is_empty() && !s.closed {
            s = self.not_empty.wait(s).unwrap();
        }
        let item = s.items.pop_front();
        if item.is_some() {
            self.not_full.notify_one();
        }
        item
    }

    pub fn try_pop(&self) -> Option<T> {
        let mut s = self.state.lock().unwrap();
        let item = s.items.pop_front();
        if item.is_some() {
            self.not_full.notify_one();
        }
        item
    }

    /// Like `pop`, but gives up after `timeout` in total (spurious wake-ups must not shorten or extend it).
    pub fn pop_timeout(&self, timeout: Duration) -> Result<T, PopError> {
        let deadline = Instant::now() + timeout;
        let mut s = self.state.lock().unwrap();
        loop {
            if let Some(item) = s.items.pop_front() {
                self.not_full.notify_one();
                return Ok(item);
            }
            if s.closed {
                return Err(PopError::Closed);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(PopError::Timeout);
            }
            s = self.not_empty.wait_timeout(s, deadline - now).unwrap().0;
        }
    }

    /// No more pushes; items already queued can still be popped. Wakes every waiter.
    pub fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }

    pub fn len(&self) -> usize {
        self.state.lock().unwrap().items.len()
    }
}
"""

BQ_STARTER = """
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PopError {
    Timeout,
    Closed,
}

struct State<T> {
    items: VecDeque<T>,
    cap: usize,
    closed: bool,
}

pub struct BlockingQueue<T> {
    state: Mutex<State<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

impl<T> BlockingQueue<T> {
    /// Panics if `cap` is 0.
    pub fn new(cap: usize) -> Self {
        todo!()
    }

    /// Waits while full. Gives the value back if the queue is closed, even when it closes while we wait.
    pub fn push(&self, value: T) -> Result<(), T> {
        todo!()
    }

    /// Never waits: `Err(value)` when full or closed.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        todo!()
    }

    /// Waits while empty. `None` once the queue is closed and drained.
    pub fn pop(&self) -> Option<T> {
        todo!()
    }

    pub fn try_pop(&self) -> Option<T> {
        todo!()
    }

    /// Waits at most `timeout` in total. `Err(Closed)` when closed and drained, `Err(Timeout)` otherwise.
    pub fn pop_timeout(&self, timeout: Duration) -> Result<T, PopError> {
        todo!()
    }

    /// No more pushes; queued items can still be popped. Wakes every waiting thread.
    pub fn close(&self) {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }
}
"""

P.append(dict(
    slug="blocking-queue", title="A bounded blocking queue with Condvar", level="medium", stage="understand-it",
    tags=["Condvar", "Mutex", "wait loop", "wait_timeout", "close semantics"],
    teaches=["Wait in a `while` loop on the condition, never an `if`: wake-ups can be spurious or stolen.",
             "Two condition variables (not empty, not full) over one mutex, and `notify_all` when the state change is for everyone (close).",
             "A total timeout across wake-ups: compute a deadline once, wait for the time left."],
    statement="""
        `BlockingQueue<T>` is the work queue between producer and consumer threads. It holds at most `cap` items.
        `push` waits while the queue is full, `pop` waits while it is empty. `close()` ends the stream: pushes (also
        ones already waiting) give their value back, pops drain what is queued and then return `None` instead of waiting
        forever. `pop_timeout` waits at most the given time **in total**, whatever the wake-ups do.

        Share it with `Arc` between any number of producers and consumers; every item pushed must be popped exactly once.
    """,
    starter=BQ_STARTER,
    solution=BQ,
    visible=[
        T("fifo", "push 1, 2, 3 then pop three", "{ let q = BlockingQueue::new(3); q.push(1).unwrap(); q.push(2).unwrap(); q.push(3).unwrap(); (q.pop(), q.pop(), q.pop()) }", "(Some(1), Some(2), Some(3))"),
        T("try_push_full", "capacity 1, second try_push", "{ let q = BlockingQueue::new(1); q.try_push('a').unwrap(); q.try_push('b') }", "Err('b')"),
        T("try_pop_empty", "nothing queued", "BlockingQueue::<u8>::new(2).try_pop()", "None"),
        T("closed_drains_then_none", "push 1, 2, close, pop thrice", "{ let q = BlockingQueue::new(4); q.push(1).unwrap(); q.push(2).unwrap(); q.close(); (q.pop(), q.pop(), q.pop()) }", "(Some(1), Some(2), None)"),
        T("push_after_close", "close, then push", "{ let q = BlockingQueue::new(4); q.close(); (q.push(7), q.try_push(8)) }", "(Err(7), Err(8))"),
    ],
    hidden=[
        WITHIN + """
        use std::sync::Arc;
        use std::time::Duration;

        #[test]
        fn pop_timeout_times_out_after_about_that_long() {
            let q = BlockingQueue::<u8>::new(1);
            let t = std::time::Instant::now();
            let r = q.pop_timeout(Duration::from_millis(80));
            let took = t.elapsed();
            check!("empty queue, 80ms timeout: result", r, Err(PopError::Timeout));
            assert!(took >= Duration::from_millis(80), "returned early after {took:?}");
            assert!(took < Duration::from_millis(1500), "took far too long: {took:?}");
        }

        #[test]
        fn pop_timeout_reports_closed_and_returns_items() {
            let q = BlockingQueue::new(2);
            q.push(5).unwrap();
            q.close();
            check!("closed with one item: item first, then Closed", (q.pop_timeout(Duration::from_millis(10)), q.pop_timeout(Duration::from_millis(10))), (Ok(5), Err(PopError::Closed)));
        }

        #[test]
        fn pop_timeout_gets_an_item_that_arrives_in_time() {
            let q = Arc::new(BlockingQueue::new(2));
            let q2 = q.clone();
            let h = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                q2.push(42).unwrap();
            });
            check!("item pushed after 60ms, timeout 3s", q.pop_timeout(Duration::from_secs(3)), Ok(42));
            h.join().unwrap();
        }

        #[test]
        fn blocked_push_continues_after_a_pop() {
            let q = Arc::new(BlockingQueue::new(1));
            q.push(1).unwrap();
            let q2 = q.clone();
            let pushed = std::thread::spawn(move || q2.push(2));
            std::thread::sleep(Duration::from_millis(60));
            check!("second push is still waiting: len", q.len(), 1);
            check!("pop frees the slot", q.pop(), Some(1));
            check!("the blocked push finishes", within(2000, move || pushed.join().unwrap()), Some(Ok(())));
            check!("and its value is next", q.pop(), Some(2));
        }

        #[test]
        fn blocked_pop_continues_after_a_push() {
            let q = Arc::new(BlockingQueue::new(1));
            let q2 = q.clone();
            let popper = std::thread::spawn(move || q2.pop());
            std::thread::sleep(Duration::from_millis(60));
            q.push(9).unwrap();
            check!("the blocked pop gets the value", within(2000, move || popper.join().unwrap()), Some(Some(9)));
        }

        #[test]
        fn close_wakes_blocked_poppers_and_pushers() {
            let q = Arc::new(BlockingQueue::new(1));
            let poppers: Vec<_> = (0..3).map(|_| { let q = q.clone(); std::thread::spawn(move || q.pop()) }).collect();
            std::thread::sleep(Duration::from_millis(60));
            q.close();
            let r = within(2000, move || poppers.into_iter().map(|h| h.join().unwrap()).collect::<Vec<Option<u8>>>());
            check!("3 blocked pops after close", r, Some(vec![None, None, None]));

            let q = Arc::new(BlockingQueue::new(1));
            q.push(1u8).unwrap();
            let pushers: Vec<_> = (0..3u8).map(|i| { let q = q.clone(); std::thread::spawn(move || q.push(10 + i)) }).collect();
            std::thread::sleep(Duration::from_millis(60));
            q.close();
            let mut back = within(2000, move || pushers.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()).expect("pushers never woke up");
            back.sort();
            check!("3 blocked pushes after close give their values back", back, vec![Err(10), Err(11), Err(12)]);
            check!("the item queued before close is still there", (q.pop(), q.pop()), (Some(1), None));
        }

        #[test]
        fn one_item_wakes_only_one_of_two_blocked_poppers() {
            // a pop that wakes up to an empty queue must go back to waiting, not return None
            let q = Arc::new(BlockingQueue::new(4));
            let (tx, rx) = std::sync::mpsc::channel();
            let hs: Vec<_> = (0..2)
                .map(|i| {
                    let (q, tx) = (q.clone(), tx.clone());
                    std::thread::spawn(move || {
                        let r = q.pop();
                        tx.send((i, r)).unwrap();
                    })
                })
                .collect();
            std::thread::sleep(Duration::from_millis(80));
            q.push(11).unwrap();
            let first = rx.recv_timeout(Duration::from_secs(2)).expect("nobody got the item");
            check!("one popper got the item", first.1, Some(11));
            std::thread::sleep(Duration::from_millis(120));
            check!("the other popper must still be waiting", rx.try_recv().is_err(), true);
            q.close();
            let second = rx.recv_timeout(Duration::from_secs(2)).expect("close did not wake the second popper");
            check!("close releases it with None", second.1, None);
            for h in hs {
                h.join().unwrap();
            }
        }

        #[test]
        fn timeout_is_a_total_not_per_wakeup() {
            // Items are pushed and stolen back again: the popper wakes up with nothing to take, and must not restart its timeout
            let mut conclusive = 0;
            for _ in 0..12 {
                let q = Arc::new(BlockingQueue::new(4));
                let q2 = q.clone();
                let popper = std::thread::spawn(move || {
                    let t = std::time::Instant::now();
                    let r = q2.pop_timeout(Duration::from_millis(300));
                    (r, t.elapsed())
                });
                for _ in 0..3 {
                    std::thread::sleep(Duration::from_millis(90));
                    if q.try_push(1).is_ok() {
                        let _ = q.try_pop();
                    }
                }
                let (r, took) = popper.join().unwrap();
                if r == Err(PopError::Timeout) {
                    conclusive += 1;
                    assert!(took < Duration::from_millis(400), "timeout restarted on wake-ups: took {took:?} instead of about 300ms");
                    break;
                }
            }
            assert!(conclusive > 0, "the popper kept winning the race for the item; no conclusive round");
        }

        #[test]
        fn zero_capacity_panics() {
            assert!(std::panic::catch_unwind(|| BlockingQueue::<u8>::new(0)).is_err());
        }

        #[test]
        fn four_producers_four_consumers_exactly_once() {
            let q = Arc::new(BlockingQueue::new(8));
            let producers: Vec<_> = (0..4u64)
                .map(|p| {
                    let q = q.clone();
                    std::thread::spawn(move || {
                        for i in 0..20_000u64 {
                            q.push(p * 100_000 + i).unwrap();
                        }
                    })
                })
                .collect();
            let consumers: Vec<_> = (0..4)
                .map(|_| {
                    let q = q.clone();
                    std::thread::spawn(move || {
                        let mut got = Vec::new();
                        while let Some(v) = q.pop() {
                            got.push(v);
                        }
                        got
                    })
                })
                .collect();
            for p in producers {
                p.join().unwrap();
            }
            q.close();
            let mut all: Vec<u64> = within(10_000, move || consumers.into_iter().flat_map(|c| c.join().unwrap()).collect::<Vec<_>>()).expect("consumers never finished");
            all.sort_unstable();
            let mut want: Vec<u64> = (0..4u64).flat_map(|p| (0..20_000u64).map(move |i| p * 100_000 + i)).collect();
            want.sort_unstable();
            check!("4 producers x 20000 into 4 consumers through capacity 8: items match", all == want, true);
        }

        #[test]
        fn capacity_one_ping_pong() {
            let q = Arc::new(BlockingQueue::new(1));
            let q2 = q.clone();
            let prod = std::thread::spawn(move || {
                for i in 0..30_000u32 {
                    q2.push(i).unwrap();
                }
                q2.close();
            });
            let mut expect = 0u32;
            let mut ok = true;
            while let Some(v) = q.pop() {
                ok &= v == expect;
                expect += 1;
            }
            prod.join().unwrap();
            check!("30000 items through a one-slot queue, in order", (ok, expect), (true, 30_000));
        }

        #[test]
        fn random_single_thread_vs_model() {
            let mut rng = anneal_prelude::Rng::new(5101);
            for _ in 0..300 {
                let cap = rng.below(5) + 1;
                let q = BlockingQueue::new(cap);
                let mut model = std::collections::VecDeque::new();
                let mut closed = false;
                let mut log = Vec::new();
                for _ in 0..rng.below(30) {
                    match rng.below(5) {
                        0 | 1 => {
                            let v = rng.below(100) as u32;
                            let want = if closed || model.len() == cap { Err(v) } else { model.push_back(v); Ok(()) };
                            log.push(format!("try_push {v}"));
                            check!(format!("cap {cap}: {}", log.join(", ")), q.try_push(v), want);
                        }
                        2 | 3 => {
                            log.push("try_pop".to_string());
                            check!(format!("cap {cap}: {}", log.join(", ")), q.try_pop(), model.pop_front());
                        }
                        _ => {
                            q.close();
                            closed = true;
                            log.push("close".to_string());
                        }
                    }
                    check!(format!("cap {cap}: {} then len", log.join(", ")), q.len(), model.len());
                }
            }
        }
        """,
    ],
    wrong=dict(
        if_instead_of_while=sub(BQ, "while s.items.is_empty() && !s.closed {\n            s = self.not_empty.wait(s).unwrap();\n        }\n        let item = s.items.pop_front();\n        if item.is_some() {\n            self.not_full.notify_one();\n        }\n        item\n    }\n\n    pub fn try_pop",
                                "if s.items.is_empty() && !s.closed {\n            s = self.not_empty.wait(s).unwrap();\n        }\n        let item = s.items.pop_front();\n        if item.is_some() {\n            self.not_full.notify_one();\n        }\n        item\n    }\n\n    pub fn try_pop")
        .replace("self.not_empty.notify_one();\n        Ok(())\n    }\n\n    /// Never", "self.not_empty.notify_all();\n        Ok(())\n    }\n\n    /// Never"),
        close_does_not_wake=sub(BQ, "        self.not_empty.notify_all();\n        self.not_full.notify_all();\n    }\n\n    pub fn len", "    }\n\n    pub fn len"),
        close_drops_queued_items=sub(BQ, "        while s.items.is_empty() && !s.closed {\n            s = self.not_empty.wait(s).unwrap();\n        }\n        let item = s.items.pop_front();",
                                     "        while s.items.is_empty() && !s.closed {\n            s = self.not_empty.wait(s).unwrap();\n        }\n        let item = if s.closed { None } else { s.items.pop_front() };"),
        timeout_restarts_on_wakeup=sub(BQ, "s = self.not_empty.wait_timeout(s, deadline - now).unwrap().0;", "s = self.not_empty.wait_timeout(s, timeout).unwrap().0;"),
    ),
    hints=[("approach", "One mutex around `{ items, cap, closed }`, two condvars: `not_empty` (consumers wait on it) and `not_full` (producers wait on it). Every wait is `while <cannot proceed> && !closed { guard = cv.wait(guard)? }`."),
           ("rust", "`Condvar::wait` takes the guard and returns it: `s = cv.wait(s).unwrap()`. For the timeout compute `deadline = Instant::now() + timeout` once, loop, and wait `deadline - now` with `wait_timeout` (or use `wait_timeout_while`)."),
           ("edge", "`close` must `notify_all` on both condvars: waiters of both kinds have to re-check `closed`. After taking an item, notify the *other* side that its condition may now hold.")],
    notes=("A condition variable carries no state: the state is the data under the mutex, and `wait` only means 'something changed, look again'. That is why the loop re-checks the condition after every wake-up (spurious wake-ups happen, and with several consumers another thread may take the item first). The deadline in `pop_timeout` is computed once, otherwise every wake-up would restart the full timeout.", "O(1) per operation", "O(capacity)"),
    follow_up="1) Why can `notify_one` be wrong here if producers and consumers shared a single condvar? 2) What would `Mutex` poisoning do to every waiter if a consumer panics while holding the lock, and how would you handle it? 3) Why must `drop` of the last `Arc` happen after `close` for a clean shutdown, and how does `std::sync::mpsc` signal the same thing? 4) Replace the mutex with `parking_lot`: what changes (no poisoning, fair unlock)? 5) How would you add priorities (a second queue, or a `BinaryHeap`) without starving low-priority items?",
    related=["C2", "S5"],
))

# ---------------------------------------------------------------------------------------------------------------
SEM = """
use std::sync::{Condvar, Mutex};

struct State {
    available: usize,
    next_ticket: u64,
    serving: u64,
}

/// A counting semaphore that serves waiters strictly in arrival order, so a request for many permits is never
/// starved by a stream of small ones.
pub struct Semaphore {
    total: usize,
    state: Mutex<State>,
    turn: Condvar,
}

/// Returns its permits when dropped.
pub struct Permit<'a> {
    sem: &'a Semaphore,
    n: usize,
}

impl Semaphore {
    pub fn new(total: usize) -> Self {
        Semaphore { total, state: Mutex::new(State { available: total, next_ticket: 0, serving: 0 }), turn: Condvar::new() }
    }

    /// Waits for its turn and for `n` permits. Panics unless `1 <= n <= total`.
    pub fn acquire(&self, n: usize) -> Permit<'_> {
        assert!(n >= 1 && n <= self.total, "request must be between 1 and the total");
        let mut s = self.state.lock().unwrap();
        let ticket = s.next_ticket;
        s.next_ticket += 1;
        while s.serving != ticket || s.available < n {
            s = self.turn.wait(s).unwrap();
        }
        s.available -= n;
        s.serving += 1;
        drop(s);
        self.turn.notify_all(); // the next ticket may be satisfiable now
        Permit { sem: self, n }
    }

    /// Succeeds only if nobody is queued and `n` permits are free right now.
    pub fn try_acquire(&self, n: usize) -> Option<Permit<'_>> {
        assert!(n >= 1 && n <= self.total, "request must be between 1 and the total");
        let mut s = self.state.lock().unwrap();
        if s.next_ticket != s.serving || s.available < n {
            return None;
        }
        s.available -= n;
        Some(Permit { sem: self, n })
    }

    pub fn available(&self) -> usize {
        self.state.lock().unwrap().available
    }

    /// Threads currently queued in `acquire`.
    pub fn waiting(&self) -> usize {
        let s = self.state.lock().unwrap();
        (s.next_ticket - s.serving) as usize
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.sem.state.lock().unwrap().available += self.n;
        self.sem.turn.notify_all();
    }
}
"""

SEM_STARTER = """
use std::sync::{Condvar, Mutex};

struct State {
    available: usize,
    next_ticket: u64,
    serving: u64,
}

pub struct Semaphore {
    total: usize,
    state: Mutex<State>,
    turn: Condvar,
}

pub struct Permit<'a> {
    sem: &'a Semaphore,
    n: usize,
}

impl Semaphore {
    pub fn new(total: usize) -> Self {
        todo!()
    }

    /// Waits for its turn (strict arrival order) and for `n` permits. Panics unless `1 <= n <= total`.
    pub fn acquire(&self, n: usize) -> Permit<'_> {
        todo!()
    }

    /// Succeeds only if nobody is queued and `n` permits are free right now. Panics unless `1 <= n <= total`.
    pub fn try_acquire(&self, n: usize) -> Option<Permit<'_>> {
        todo!()
    }

    pub fn available(&self) -> usize {
        todo!()
    }

    /// Threads currently queued inside `acquire`.
    pub fn waiting(&self) -> usize {
        todo!()
    }
}

// TODO: `Drop` for `Permit`: give the permits back and wake the waiters.
"""

P.append(dict(
    slug="fair-semaphore", title="A fair counting semaphore", level="hard", stage="build-it",
    tags=["Condvar", "ticket queue", "fairness", "RAII permit", "starvation"],
    teaches=["A ticket counter (`next_ticket`, `serving`) turns a stampede into a strict FIFO line.",
             "`notify_all` plus per-waiter predicates when only one specific waiter may proceed.",
             "RAII permits with a lifetime tied to the semaphore."],
    statement="""
        `Semaphore::new(total)` hands out up to `total` permits. `acquire(n)` waits until `n` permits are free **and every
        earlier caller has been served**: a request for 3 permits that arrived first must not be overtaken by later requests
        for 1, even when one permit is free. It returns a `Permit` that gives its `n` permits back when dropped.
        `try_acquire(n)` never waits and also respects the queue (it fails if anyone is waiting). `acquire` and `try_acquire`
        panic unless `1 <= n <= total`. `waiting()` reports how many threads are queued inside `acquire`; `available()` how many
        permits are free.
    """,
    starter=SEM_STARTER,
    solution=SEM,
    visible=[
        T("acquire_and_release", "total 2: take 1, take 1, drop", "{ let s = Semaphore::new(2); let a = s.acquire(1); let b = s.acquire(1); let none = s.available(); drop(a); drop(b); (none, s.available()) }", "(0, 2)"),
        T("try_acquire_fails_when_short", "total 2: hold 2, try 1", "{ let s = Semaphore::new(2); let _a = s.acquire(2); let r = s.try_acquire(1).is_none(); r }", "true"),
        T("try_acquire_ok", "total 3: try 2 then 1 then 1", "{ let s = Semaphore::new(3); let a = s.try_acquire(2); let b = s.try_acquire(1); let c = s.try_acquire(1); (a.is_some(), b.is_some(), c.is_some()) }", "(true, true, false)"),
        T("permit_returns_on_drop", "scope ends", "{ let s = Semaphore::new(1); { let _p = s.acquire(1); } s.available() }", "1"),
        T("bad_request_panics", "0 and total+1", "{ let s = Semaphore::new(2); (std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { s.acquire(0); })).is_err(), std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { s.acquire(3); })).is_err()) }", "(true, true)"),
    ],
    hidden=[
        WITHIN + """
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        fn wait_until_waiting(s: &Semaphore, n: usize) {
            for _ in 0..400 {
                if s.waiting() == n {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("never saw {n} waiting threads");
        }

        #[test]
        fn a_waiter_is_woken_by_a_release() {
            let s = Arc::new(Semaphore::new(1));
            let p = s.acquire(1);
            let s2 = s.clone();
            let h = std::thread::spawn(move || {
                let _q = s2.acquire(1);
                7
            });
            wait_until_waiting(&s, 1);
            drop(p);
            check!("blocked acquire completes after the drop", within(2000, move || h.join().unwrap()), Some(7));
        }

        #[test]
        fn big_request_is_not_overtaken_by_small_ones() {
            // total 2; main holds 1. B asks for 2 and must wait. C asks for 1 later: one permit IS free, but B is first in line.
            let s = Arc::new(Semaphore::new(2));
            let order = Arc::new(Mutex::new(Vec::new()));
            let held = s.acquire(1);
            let (sb, ob) = (s.clone(), order.clone());
            let b = std::thread::spawn(move || {
                let _p = sb.acquire(2);
                ob.lock().unwrap().push("B");
            });
            wait_until_waiting(&s, 1);
            let (sc, oc) = (s.clone(), order.clone());
            let c = std::thread::spawn(move || {
                let _p = sc.acquire(1);
                oc.lock().unwrap().push("C");
            });
            wait_until_waiting(&s, 2);
            std::thread::sleep(Duration::from_millis(60));
            check!("C must still be waiting behind B", order.lock().unwrap().len(), 0);
            check!("try_acquire must not jump the queue either", s.try_acquire(1).is_none(), true);
            drop(held);
            within(3000, move || {
                b.join().unwrap();
                c.join().unwrap();
            })
            .expect("waiters never finished");
            check!("B before C", order.lock().unwrap().clone(), vec!["B", "C"]);
            check!("all permits back", s.available(), 2);
        }

        #[test]
        fn waiters_are_served_in_arrival_order() {
            let s = Arc::new(Semaphore::new(1));
            let order = Arc::new(Mutex::new(Vec::new()));
            let held = s.acquire(1);
            let mut hs = Vec::new();
            for i in 0..5usize {
                let (s2, o2) = (s.clone(), order.clone());
                hs.push(std::thread::spawn(move || {
                    let _p = s2.acquire(1);
                    o2.lock().unwrap().push(i);
                }));
                wait_until_waiting(&s, i + 1);
            }
            drop(held);
            within(3000, move || {
                for h in hs {
                    h.join().unwrap();
                }
            })
            .expect("waiters never finished");
            check!("served in the order they arrived", order.lock().unwrap().clone(), vec![0, 1, 2, 3, 4]);
        }

        #[test]
        fn never_more_holders_than_permits() {
            let s = Arc::new(Semaphore::new(3));
            let inside = Arc::new(AtomicUsize::new(0));
            let peak = Arc::new(AtomicUsize::new(0));
            let hs: Vec<_> = (0..8usize)
                .map(|t| {
                    let (s, inside, peak) = (s.clone(), inside.clone(), peak.clone());
                    std::thread::spawn(move || {
                        for i in 0..300 {
                            let n = 1 + (t + i) % 3;
                            let _p = s.acquire(n);
                            let now = inside.fetch_add(n, Ordering::SeqCst) + n;
                            peak.fetch_max(now, Ordering::SeqCst);
                            std::thread::yield_now();
                            inside.fetch_sub(n, Ordering::SeqCst);
                        }
                    })
                })
                .collect();
            let done = within(15_000, move || {
                for h in hs {
                    h.join().unwrap();
                }
            });
            assert!(done.is_some(), "threads deadlocked");
            check!("peak permits held at once, available at the end", (peak.load(Ordering::SeqCst) <= 3, s.available()), (true, 3));
        }

        #[test]
        fn try_acquire_after_queue_drains() {
            let s = Arc::new(Semaphore::new(2));
            let p = s.acquire(2);
            let s2 = s.clone();
            let h = std::thread::spawn(move || drop(s2.acquire(1)));
            wait_until_waiting(&s, 1);
            drop(p);
            within(2000, move || h.join().unwrap()).unwrap();
            check!("queue empty again: try_acquire works", s.try_acquire(2).is_some(), true);
        }

        #[test]
        fn waiting_counts_only_queued_threads() {
            let s = Arc::new(Semaphore::new(1));
            check!("fresh", s.waiting(), 0);
            let p = s.acquire(1);
            check!("holder is not waiting", s.waiting(), 0);
            let s2 = s.clone();
            let h = std::thread::spawn(move || drop(s2.acquire(1)));
            wait_until_waiting(&s, 1);
            drop(p);
            within(2000, move || h.join().unwrap()).unwrap();
            check!("after everyone is served", s.waiting(), 0);
        }
        """,
        T("single_permit_is_a_mutex", "total 1: second try fails until drop", "{ let s = Semaphore::new(1); let a = s.try_acquire(1); let b = s.try_acquire(1).is_none(); drop(a); let c = s.try_acquire(1).is_some(); (b, c) }", "(true, true)"),
        T("full_total_in_one_request", "total 4, take all", "{ let s = Semaphore::new(4); let p = s.acquire(4); let a = s.available(); drop(p); (a, s.available()) }", "(0, 4)"),
    ],
    wrong=dict(
        unfair_barging=sub(SEM, "while s.serving != ticket || s.available < n {", "while s.available < n {")
        .replace("s.serving += 1;\n        drop(s);", "s.serving += 1;\n        drop(s);"),
        notify_one_loses_the_turn=sub(sub(SEM, "drop(s);\n        self.turn.notify_all(); // the next ticket may be satisfiable now", "drop(s);\n        self.turn.notify_one();"),
                                      "self.sem.turn.notify_all();", "self.sem.turn.notify_one();"),
        try_acquire_ignores_queue=sub(SEM, "if s.next_ticket != s.serving || s.available < n {", "if s.available < n {"),
    ),
    hints=[("approach", "State: `available`, `next_ticket`, `serving`. `acquire` takes `ticket = next_ticket++`, then waits until `serving == ticket && available >= n`; when it proceeds it subtracts `n`, bumps `serving` and wakes everyone so the next ticket can re-check."),
           ("rust", "Use `notify_all`: with per-waiter predicates `notify_one` may wake a thread whose ticket is not up, which goes back to sleep while the right thread never wakes. The `Permit` drop adds `n` back under the lock, then notifies."),
           ("edge", "`try_acquire` must check `next_ticket == serving` (nobody queued) as well as `available >= n`, or it overtakes waiters. `waiting() = next_ticket - serving`.")],
    notes=("Strict FIFO is bought with a ticket counter: only the thread whose ticket equals `serving` may take permits, so a big request cannot be starved by later small ones (the price: a free permit stays idle while the head of the queue still needs more, which is why `std`'s futex-based primitives are not fair). Because each waiter has its own predicate, wake-ups use `notify_all`; the cost is a thundering herd of cheap re-checks. A per-waiter condvar (a queue of `Thread` handles to `unpark`) avoids the herd.", "O(waiters) wake-ups per release", "O(1) state"),
    follow_up="1) What does Tokio's `Semaphore` do differently (it is async and fair, and why does it store waiters in an intrusive list)? 2) How would you avoid the thundering herd (per-waiter `Condvar` or `thread::park`)? 3) Add `acquire_timeout`: what happens to the ticket of a waiter that gives up, and why must `serving` skip it? 4) Where do you use a semaphore in real systems (connection limits, bounded parallelism, rate limiting)? 5) Why does fairness reduce throughput, and when is that the right trade?",
    related=["C2", "B5"],
))

# ---------------------------------------------------------------------------------------------------------------
RW = """
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    readers: usize,
    writer: bool,
    writers_waiting: usize,
}

/// A readers-writer lock that prefers writers: once a writer is waiting, no new reader gets in.
pub struct RwLock<T> {
    state: Mutex<State>,
    cv: Condvar,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for RwLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

pub struct ReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

pub struct WriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        RwLock { state: Mutex::new(State::default()), cv: Condvar::new(), value: UnsafeCell::new(value) }
    }

    pub fn read(&self) -> ReadGuard<'_, T> {
        let mut s = self.state.lock().unwrap();
        while s.writer || s.writers_waiting > 0 {
            s = self.cv.wait(s).unwrap();
        }
        s.readers += 1;
        ReadGuard { lock: self }
    }

    pub fn try_read(&self) -> Option<ReadGuard<'_, T>> {
        let mut s = self.state.lock().unwrap();
        if s.writer || s.writers_waiting > 0 {
            return None;
        }
        s.readers += 1;
        Some(ReadGuard { lock: self })
    }

    pub fn write(&self) -> WriteGuard<'_, T> {
        let mut s = self.state.lock().unwrap();
        s.writers_waiting += 1;
        while s.writer || s.readers > 0 {
            s = self.cv.wait(s).unwrap();
        }
        s.writers_waiting -= 1;
        s.writer = true;
        WriteGuard { lock: self }
    }

    pub fn try_write(&self) -> Option<WriteGuard<'_, T>> {
        let mut s = self.state.lock().unwrap();
        if s.writer || s.readers > 0 {
            return None;
        }
        s.writer = true;
        Some(WriteGuard { lock: self })
    }

    /// Writers queued in `write` (not counting one that holds the lock).
    pub fn pending_writers(&self) -> usize {
        self.state.lock().unwrap().writers_waiting
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        let mut s = self.lock.state.lock().unwrap();
        s.readers -= 1;
        if s.readers == 0 {
            self.lock.cv.notify_all();
        }
    }
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.lock().unwrap().writer = false;
        self.lock.cv.notify_all();
    }
}
"""

RW_STARTER = """
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    readers: usize,
    writer: bool,
    writers_waiting: usize,
}

pub struct RwLock<T> {
    state: Mutex<State>,
    cv: Condvar,
    value: UnsafeCell<T>,
}

// Given loose so the tests compile: tighten them (a `&RwLock<T>` hands `&T` to many threads at once).
unsafe impl<T> Send for RwLock<T> {}
unsafe impl<T> Sync for RwLock<T> {}

pub struct ReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

pub struct WriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> RwLock<T> {
    pub fn new(value: T) -> Self {
        todo!()
    }

    /// Many readers at once; waits while a writer holds the lock OR is waiting for it.
    pub fn read(&self) -> ReadGuard<'_, T> {
        todo!()
    }

    pub fn try_read(&self) -> Option<ReadGuard<'_, T>> {
        todo!()
    }

    /// Exclusive. Announces itself as waiting so new readers stay out, then waits for the readers to leave.
    pub fn write(&self) -> WriteGuard<'_, T> {
        todo!()
    }

    pub fn try_write(&self) -> Option<WriteGuard<'_, T>> {
        todo!()
    }

    /// Writers currently queued inside `write`.
    pub fn pending_writers(&self) -> usize {
        todo!()
    }

    pub fn into_inner(self) -> T {
        todo!()
    }
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        todo!()
    }
}

// TODO: `Drop` for both guards: update the state and wake the waiters.
"""

P.append(dict(
    slug="rwlock-writer-preferring", title="A writer-preferring RwLock", level="hard", stage="build-it",
    tags=["RwLock", "Condvar", "writer starvation", "RAII guards", "Sync bounds"],
    teaches=["The state machine of a readers-writer lock: readers count, writer flag, writers waiting.",
             "Writer preference: a waiting writer blocks new readers, so a steady stream of readers cannot starve it.",
             "`Sync` for a lock needs `T: Send + Sync` (readers share `&T`), unlike a mutex (`T: Send`)."],
    statement="""
        Write `RwLock<T>` on a `Mutex` + `Condvar`. `read()` gives a shared guard (`Deref`), `write()` an exclusive one
        (`Deref` + `DerefMut`); any number of readers may hold the lock together, a writer holds it alone.

        **Writer preference:** as soon as a writer is *waiting*, new readers must wait too (`try_read` fails), so readers
        that keep arriving cannot starve it. Readers already inside finish normally. `pending_writers()` counts writers
        queued in `write`. `try_read`/`try_write` never wait. Choose the bounds for `Send`/`Sync` carefully.
    """,
    starter=RW_STARTER,
    solution=RW,
    visible=[
        T("many_readers_at_once", "three read guards together", "{ let l = RwLock::new(5); let a = l.read(); let b = l.read(); let c = l.read(); *a + *b + *c }", "15"),
        T("writer_excludes_readers", "write guard alive: try_read and try_write fail", "{ let l = RwLock::new(0); let w = l.write(); let r = (l.try_read().is_none(), l.try_write().is_none()); drop(w); r }", "(true, true)"),
        T("readers_exclude_writer", "read guard alive: try_write fails, try_read works", "{ let l = RwLock::new(0); let r = l.read(); let out = (l.try_write().is_none(), l.try_read().is_some()); drop(r); out }", "(true, true)"),
        T("write_then_read", "mutation is visible", "{ let l = RwLock::new(vec![1]); l.write().push(2); let v = l.read().clone(); v }", "vec![1, 2]"),
        T("released_on_drop", "all guards dropped", "{ let l = RwLock::new(1); { let _a = l.read(); let _b = l.read(); } { let mut w = l.write(); *w += 1; } let v = *l.read(); v }", "2"),
    ],
    hidden=[
        WITHIN + """
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};
        use std::time::Duration;

        fn wait_for_pending(l: &RwLock<u32>, n: usize) {
            for _ in 0..400 {
                if l.pending_writers() == n {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("never saw {n} pending writers");
        }

        #[test]
        fn a_waiting_writer_blocks_new_readers() {
            let l = Arc::new(RwLock::new(0u32));
            let r = l.read();
            let l2 = l.clone();
            let w = std::thread::spawn(move || {
                *l2.write() += 1;
            });
            wait_for_pending(&l, 1);
            check!("writer waits behind a reader: a new try_read must fail", l.try_read().is_none(), true);
            drop(r);
            within(2000, move || w.join().unwrap()).expect("writer never got in");
            check!("after the writer: reads work and see its write", l.try_read().map(|g| *g), Some(1));
        }

        #[test]
        fn blocked_reader_is_released_after_the_writer() {
            let l = Arc::new(RwLock::new(0u32));
            let w = l.write();
            let l2 = l.clone();
            let reader = std::thread::spawn(move || *l2.read());
            std::thread::sleep(Duration::from_millis(60));
            drop(w);
            check!("blocked read returns after the write guard drops", within(2000, move || reader.join().unwrap()), Some(0));
        }

        #[test]
        fn readers_really_share() {
            // four readers wait for each other at a barrier while all inside: impossible if reads were exclusive
            let l = Arc::new(RwLock::new(7u32));
            let b = Arc::new(Barrier::new(4));
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    let (l, b) = (l.clone(), b.clone());
                    std::thread::spawn(move || {
                        let g = l.read();
                        b.wait();
                        *g
                    })
                })
                .collect();
            let vals = within(3000, move || hs.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>());
            check!("4 readers inside together", vals, Some(vec![7, 7, 7, 7]));
        }

        #[test]
        fn writers_are_exclusive_and_see_consistent_pairs() {
            // invariant: the two halves are equal whenever the lock is free; a writer breaks it while it holds the lock
            let l = Arc::new(RwLock::new((0u64, 0u64)));
            let torn = Arc::new(AtomicUsize::new(0));
            let mut hs = Vec::new();
            for _ in 0..2 {
                let l = l.clone();
                hs.push(std::thread::spawn(move || {
                    for _ in 0..20_000 {
                        let mut g = l.write();
                        g.0 += 1;
                        std::hint::spin_loop();
                        g.1 += 1;
                    }
                }));
            }
            for _ in 0..4 {
                let (l, torn) = (l.clone(), torn.clone());
                hs.push(std::thread::spawn(move || {
                    for _ in 0..20_000 {
                        let g = l.read();
                        if g.0 != g.1 {
                            torn.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }));
            }
            let done = within(15_000, move || {
                for h in hs {
                    h.join().unwrap();
                }
            });
            assert!(done.is_some(), "deadlock");
            let (a, b) = *l.read();
            check!("2 writers x 20000 and 4 readers: torn reads, final pair", (torn.load(Ordering::SeqCst), a, b), (0, 40_000, 40_000));
        }

        #[test]
        fn a_stream_of_readers_cannot_starve_a_writer() {
            let l = Arc::new(RwLock::new(0u32));
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let readers: Vec<_> = (0..4)
                .map(|_| {
                    let (l, stop) = (l.clone(), stop.clone());
                    std::thread::spawn(move || {
                        while !stop.load(Ordering::Relaxed) {
                            let _g = l.read();
                            std::thread::sleep(Duration::from_micros(300)); // readers overlap almost always
                        }
                    })
                })
                .collect();
            std::thread::sleep(Duration::from_millis(30));
            let l2 = l.clone();
            let got = within(3000, move || {
                *l2.write() += 1;
            });
            stop.store(true, Ordering::Relaxed);
            for h in readers {
                h.join().unwrap();
            }
            assert!(got.is_some(), "the writer starved behind the readers");
            check!("writer got through", *l.read(), 1);
        }

        #[test]
        fn pending_writers_counts_waiters_only() {
            let l = Arc::new(RwLock::new(0u32));
            check!("idle", l.pending_writers(), 0);
            let w = l.write();
            check!("a holder is not pending", l.pending_writers(), 0);
            let hs: Vec<_> = (0..3).map(|_| { let l = l.clone(); std::thread::spawn(move || { *l.write() += 1; }) }).collect();
            wait_for_pending(&l, 3);
            drop(w);
            within(3000, move || { for h in hs { h.join().unwrap(); } }).expect("writers never finished");
            check!("all done: pending, value", (l.pending_writers(), *l.read()), (0, 3));
        }

        #[test]
        fn try_write_fails_while_readers_exist() {
            let l = RwLock::new(1);
            let r1 = l.read();
            let r2 = l.read();
            let a = l.try_write().is_none();
            drop(r1);
            let b = l.try_write().is_none();
            drop(r2);
            check!("two readers, then one, then none", (a, b, l.try_write().is_some()), (true, true, true));
        }

        #[test]
        fn into_inner_and_drop() {
            let l = RwLock::new(String::from("v"));
            check!("into_inner", l.into_inner(), "v".to_string());
        }

        #[test]
        fn bounds_are_the_right_ones() {
            fn sync<T: Sync>() {}
            fn send<T: Send>() {}
            sync::<RwLock<Vec<u8>>>();
            send::<RwLock<Vec<u8>>>();
            send::<RwLock<std::cell::Cell<u8>>>(); // Cell is Send, so the lock may move between threads
        }
        """,
    ],
    wrong=dict(
        reader_preferring=sub(sub(RW, "while s.writer || s.writers_waiting > 0 {\n            s = self.cv.wait(s).unwrap();", "while s.writer {\n            s = self.cv.wait(s).unwrap();"),
                              "if s.writer || s.writers_waiting > 0 {\n            return None;", "if s.writer {\n            return None;"),
        writer_drop_forgets_to_wake=sub(RW, "self.lock.state.lock().unwrap().writer = false;\n        self.lock.cv.notify_all();", "self.lock.state.lock().unwrap().writer = false;"),
        try_write_ignores_readers=sub(RW, "if s.writer || s.readers > 0 {\n            return None;", "if s.writer {\n            return None;"),
        waiting_never_decremented=sub(RW, "s.writers_waiting -= 1;\n        s.writer = true;", "s.writer = true;"),
    ),
    hints=[("approach", "State under one mutex: `readers: usize`, `writer: bool`, `writers_waiting: usize`. Readers enter when `!writer && writers_waiting == 0`; a writer first increments `writers_waiting`, then waits for `!writer && readers == 0`, then decrements and sets `writer = true`."),
           ("rust", "One `Condvar` and `notify_all` on every state change that could unblock someone: the last reader leaving, a writer leaving. Guards hold `&RwLock<T>` and reach the data through `UnsafeCell::get`."),
           ("edge", "`unsafe impl<T: Send + Sync> Sync`: a read guard shares `&T` between threads, so `T` must be `Sync` (a `Mutex<T>` only needs `Send`). `Send` needs just `T: Send`.")],
    notes=("Readers-preferring locks starve writers under steady read traffic; writer preference fixes that at the price of readers waiting behind a queued writer (and, if readers re-enter the lock recursively while holding a guard, a deadlock: a classic reason `std`'s `RwLock` documents that recursive reads may deadlock). The `writers_waiting` count is what turns 'a writer is blocked' into a state that new readers can see.", "O(1) per operation", "O(1) extra"),
    follow_up="1) Why can `read` called twice on the same thread deadlock with writer preference? 2) What is `RwLock` poisoning and should you reproduce it? 3) Make it fair for both sides with a ticket (see the fair semaphore). 4) When is a `Mutex` plus short critical sections faster than an `RwLock` (reader count bookkeeping is a shared write: cache line bouncing)? Compare `parking_lot::RwLock` and `arc-swap` for read-mostly data. 5) Upgradable reads: why do they exist and what deadlock do they avoid?",
    related=["S7", "C3"],
))

STAGES = [("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("c1-threads-shared-state", "C1", "Threads & shared state", "C", "core", 1,
                    "Threads, mutexes and condition variables, by building the blocking primitives yourself: a queue, a fair semaphore and a readers-writer lock.",
                    STAGES, P)
    print("C1", n)
