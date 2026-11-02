from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


P = []

# ---------------------------------------------------------------------------------------------------------------
STATS = """
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub count: u64,
    pub sum: u64,
    pub min: u64,
    pub max: u64,
}

/// Latency-style counters shared by many threads, with no lock.
pub struct Stats {
    count: AtomicU64,
    sum: AtomicU64,
    min: AtomicU64,
    max: AtomicU64,
}

impl Stats {
    pub fn new() -> Self {
        Stats { count: AtomicU64::new(0), sum: AtomicU64::new(0), min: AtomicU64::new(u64::MAX), max: AtomicU64::new(0) }
    }

    pub fn record(&self, v: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        // saturating add as a compare-exchange loop: `fetch_update` retries until nobody raced us
        let _ = self.sum.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| Some(s.saturating_add(v)));
        self.min.fetch_min(v, Ordering::Relaxed);
        self.max.fetch_max(v, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Option<Snapshot> {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return None;
        }
        Some(Snapshot {
            count,
            sum: self.sum.load(Ordering::Relaxed),
            min: self.min.load(Ordering::Relaxed),
            max: self.max.load(Ordering::Relaxed),
        })
    }

    /// Returns what was recorded and starts again from empty. Only called when no `record` is running.
    pub fn take(&self) -> Option<Snapshot> {
        let count = self.count.swap(0, Ordering::Relaxed);
        let sum = self.sum.swap(0, Ordering::Relaxed);
        let min = self.min.swap(u64::MAX, Ordering::Relaxed);
        let max = self.max.swap(0, Ordering::Relaxed);
        if count == 0 {
            return None;
        }
        Some(Snapshot { count, sum, min, max })
    }
}
"""

P.append(dict(
    slug="atomic-stats", title="Lock-free statistics counters", level="medium", stage="understand-it",
    tags=["AtomicU64", "fetch_update", "fetch_min", "Relaxed"],
    teaches=["Independent counters need `Relaxed`: the ordering only matters when one atomic publishes another's data.",
             "`fetch_add`, `fetch_min`, `fetch_max` are single hardware read-modify-writes; `load` then `store` is a race.",
             "A compare-exchange loop (`fetch_update`) builds any other update, such as a saturating add."],
    statement="""
        A server records the latency of each request from many threads. Write `Stats`: `record(&self, v)` adds one
        observation, `snapshot(&self)` returns `Some(Snapshot { count, sum, min, max })` or `None` when nothing was
        recorded, and `take(&self)` returns the same and resets to empty (called only when no `record` is running).

        `record` takes `&self`, so `Stats` is shared by reference (or `Arc`) with no lock. The `sum` must **saturate**
        at `u64::MAX` instead of wrapping, and no update may be lost however many threads record at once.
    """,
    starter="""
        use std::sync::atomic::{AtomicU64, Ordering};

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Snapshot {
            pub count: u64,
            pub sum: u64,
            pub min: u64,
            pub max: u64,
        }

        pub struct Stats {
            // TODO: the fields
        }

        impl Stats {
            pub fn new() -> Self {
                todo!()
            }

            pub fn record(&self, v: u64) {
                todo!()
            }

            pub fn snapshot(&self) -> Option<Snapshot> {
                todo!()
            }

            pub fn take(&self) -> Option<Snapshot> {
                todo!()
            }
        }
    """,
    solution=STATS,
    visible=[
        T("empty", "nothing recorded", "Stats::new().snapshot()", "None"),
        T("one_value", "record 7", "{ let s = Stats::new(); s.record(7); s.snapshot() }", "Some(Snapshot { count: 1, sum: 7, min: 7, max: 7 })"),
        T("several", "record 5, 2, 9", "{ let s = Stats::new(); for v in [5, 2, 9] { s.record(v); } s.snapshot() }", "Some(Snapshot { count: 3, sum: 16, min: 2, max: 9 })"),
        T("zero_is_a_value", "record 0, 4", "{ let s = Stats::new(); s.record(0); s.record(4); s.snapshot() }", "Some(Snapshot { count: 2, sum: 4, min: 0, max: 4 })"),
        T("take_resets", "record 3, take, take", "{ let s = Stats::new(); s.record(3); (s.take(), s.take(), s.snapshot()) }", "(Some(Snapshot { count: 1, sum: 3, min: 3, max: 3 }), None, None)"),
    ],
    hidden=[
        T("snapshot_does_not_reset", "record 3, snapshot twice", "{ let s = Stats::new(); s.record(3); (s.snapshot(), s.snapshot()) }", "(Some(Snapshot { count: 1, sum: 3, min: 3, max: 3 }), Some(Snapshot { count: 1, sum: 3, min: 3, max: 3 }))"),
        T("sum_saturates", "record u64::MAX, 5", "{ let s = Stats::new(); s.record(u64::MAX); s.record(5); s.snapshot() }", "Some(Snapshot { count: 2, sum: u64::MAX, min: 5, max: u64::MAX })"),
        T("sum_saturates_in_steps", "record MAX-1, 1, 1", "{ let s = Stats::new(); s.record(u64::MAX - 1); s.record(1); s.record(1); s.snapshot().map(|x| x.sum) }", "Some(u64::MAX)"),
        T("min_can_be_max_value", "record u64::MAX only", "{ let s = Stats::new(); s.record(u64::MAX); s.snapshot() }", "Some(Snapshot { count: 1, sum: u64::MAX, min: u64::MAX, max: u64::MAX })"),
        T("reusable_after_take", "record 9, take, record 2", "{ let s = Stats::new(); s.record(9); s.take(); s.record(2); s.snapshot() }", "Some(Snapshot { count: 1, sum: 2, min: 2, max: 2 })"),
        """
        #[test]
        fn eight_threads_lose_nothing() {
            let s = std::sync::Arc::new(Stats::new());
            let mut hs = Vec::new();
            for t in 0..8u64 {
                let s = s.clone();
                hs.push(std::thread::spawn(move || {
                    for i in 0..50_000u64 {
                        s.record(t * 1_000_000 + i + 1);
                    }
                }));
            }
            for h in hs {
                h.join().unwrap();
            }
            let want_sum: u64 = (0..8u64).map(|t| (1..=50_000u64).map(|i| t * 1_000_000 + i).sum::<u64>()).sum();
            check!("8 threads x 50000 records", s.snapshot(), Some(Snapshot { count: 400_000, sum: want_sum, min: 1, max: 7_050_000 }));
        }

        #[test]
        fn racing_saturation_never_wraps() {
            let s = std::sync::Arc::new(Stats::new());
            let mut hs = Vec::new();
            for _ in 0..4 {
                let s = s.clone();
                hs.push(std::thread::spawn(move || {
                    for _ in 0..20_000 {
                        s.record(u64::MAX / 3);
                    }
                }));
            }
            for h in hs {
                h.join().unwrap();
            }
            let snap = s.snapshot().unwrap();
            check!("4 threads x 20000 records of u64::MAX/3", (snap.count, snap.sum), (80_000, u64::MAX));
        }

        #[test]
        fn extremes_found_under_contention() {
            let s = std::sync::Arc::new(Stats::new());
            let hs: Vec<_> = (0..6u64)
                .map(|t| {
                    let s = s.clone();
                    std::thread::spawn(move || {
                        for i in 0..20_000u64 {
                            // thread 3 alone records the smallest value, thread 5 alone the largest, late in its run
                            let v = if t == 3 && i == 10_000 { 1 } else if t == 5 && i == 19_999 { 999_999 } else { 1_000 + (i % 100) };
                            s.record(v);
                        }
                    })
                })
                .collect();
            for h in hs {
                h.join().unwrap();
            }
            let snap = s.snapshot().unwrap();
            check!("6 threads, one tiny and one huge value hidden among 120000", (snap.count, snap.min, snap.max), (120_000, 1, 999_999));
        }

        #[test]
        fn min_and_max_survive_racing_updates() {
            // every thread lowers the minimum and raises the maximum again and again: a check-then-store update
            // lets a stale write overwrite a better value
            for round in 0..5u64 {
                let s = std::sync::Arc::new(Stats::new());
                let hs: Vec<_> = (0..8u64)
                    .map(|t| {
                        let s = s.clone();
                        std::thread::spawn(move || {
                            for i in 0..20_000u64 {
                                s.record(100_000 - i * 5 + t);
                                s.record(i * 5 + t + 10);
                            }
                        })
                    })
                    .collect();
                for h in hs {
                    h.join().unwrap();
                }
                let snap = s.snapshot().unwrap();
                check!(format!("round {round}: 8 threads recording descending and ascending values"), (snap.min, snap.max), (5, 100_012));
            }
        }

        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(4101);
            for _ in 0..300 {
                let s = Stats::new();
                let n = rng.below(12);
                let mut vals = Vec::new();
                for _ in 0..n {
                    let v = if rng.below(6) == 0 { u64::MAX - rng.below(3) as u64 } else { rng.below(1000) as u64 };
                    vals.push(v);
                    s.record(v);
                }
                let want = if vals.is_empty() {
                    None
                } else {
                    Some(Snapshot {
                        count: vals.len() as u64,
                        sum: vals.iter().fold(0u64, |a, &b| a.saturating_add(b)),
                        min: *vals.iter().min().unwrap(),
                        max: *vals.iter().max().unwrap(),
                    })
                };
                check!(format!("record {vals:?}"), s.snapshot(), want);
            }
        }
        """,
    ],
    wrong=dict(
        load_store_races=sub(STATS, "self.count.fetch_add(1, Ordering::Relaxed);",
                             "let c = self.count.load(Ordering::Relaxed);\n        std::thread::yield_now();\n        self.count.store(c + 1, Ordering::Relaxed);"),
        wrapping_sum=sub(STATS, "let _ = self.sum.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| Some(s.saturating_add(v)));",
                         "self.sum.fetch_add(v, Ordering::Relaxed);"),
        min_starts_at_zero=sub(STATS, "min: AtomicU64::new(u64::MAX)", "min: AtomicU64::new(0)"),
    ),
    hints=[("approach", "Every field is updated by one atomic operation, so `Ordering::Relaxed` is enough: no field publishes data for another."),
           ("rust", "`AtomicU64::fetch_update(set, fetch, |old| Some(new))` is the compare-exchange loop: use it where no single `fetch_*` method does the job."),
           ("edge", "An empty `min` must not look like a real value: start it at `u64::MAX`, and decide emptiness from `count`.")],
    notes=("Four independent counters, each updated by one atomic read-modify-write. `Relaxed` still guarantees each counter's own total order and no lost updates; it only promises nothing about how different counters line up with each other, which is why `snapshot` can see a count that is one ahead of the sum. The saturating add is the pattern behind every custom atomic update: load, compute, compare-exchange, retry on conflict.", "O(1) per record", "O(1)"),
    follow_up="1) Why would a snapshot taken during writes be inconsistent, and what would you change to make `count`, `sum`, `min` and `max` agree (seqlock, or a mutex)? 2) With 64 threads hammering one `Stats`, what limits throughput (cache-line bouncing), and how does sharding per thread plus merging fix it? 3) Why is `fetch_update` allowed to fail spuriously, and what does that mean for the closure? 4) How would you add a mean without division by zero?",
    related=["C1", "F6"],
))

# ---------------------------------------------------------------------------------------------------------------
ONCE = """
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU8, Ordering};

const EMPTY: u8 = 0;
const RUNNING: u8 = 1;
const READY: u8 = 2;

/// A cell written at most once, from any thread, through `&self`.
pub struct Once<T> {
    state: AtomicU8,
    slot: UnsafeCell<MaybeUninit<T>>,
}

// SAFETY: the value is written by exactly one thread (the one that moved EMPTY -> RUNNING) and only read after
// READY was observed with Acquire, which pairs with the writer's Release store.
unsafe impl<T: Send + Sync> Sync for Once<T> {}
unsafe impl<T: Send> Send for Once<T> {}

/// Puts the state back to EMPTY if the initializer unwinds.
struct Reset<'a>(&'a AtomicU8);

impl Drop for Reset<'_> {
    fn drop(&mut self) {
        self.0.store(EMPTY, Ordering::Release);
    }
}

impl<T> Once<T> {
    pub const fn new() -> Self {
        Once { state: AtomicU8::new(EMPTY), slot: UnsafeCell::new(MaybeUninit::uninit()) }
    }

    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == READY {
            // SAFETY: READY means the slot is initialized and never written again.
            Some(unsafe { (*self.slot.get()).assume_init_ref() })
        } else {
            None
        }
    }

    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        loop {
            match self.state.compare_exchange(EMPTY, RUNNING, Ordering::Acquire, Ordering::Acquire) {
                Ok(_) => {
                    let reset = Reset(&self.state);
                    let value = f();
                    // SAFETY: we own the RUNNING state, so nobody else touches the slot.
                    unsafe { (*self.slot.get()).write(value) };
                    std::mem::forget(reset);
                    self.state.store(READY, Ordering::Release);
                    break;
                }
                Err(READY) => break,
                Err(_) => std::thread::yield_now(), // someone else is initializing: wait for READY or a reset
            }
        }
        self.get().unwrap()
    }

    pub fn into_inner(mut self) -> Option<T> {
        if *self.state.get_mut() == READY {
            *self.state.get_mut() = EMPTY; // so Drop doesn't drop it again
            Some(unsafe { self.slot.get_mut().assume_init_read() })
        } else {
            None
        }
    }
}

impl<T> Drop for Once<T> {
    fn drop(&mut self) {
        if *self.state.get_mut() == READY {
            unsafe { self.slot.get_mut().assume_init_drop() };
        }
    }
}
"""

ONCE_STARTER = """
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU8, Ordering};

const EMPTY: u8 = 0;
const RUNNING: u8 = 1;
const READY: u8 = 2;

pub struct Once<T> {
    state: AtomicU8,
    slot: UnsafeCell<MaybeUninit<T>>,
}

// These compile as given so the tests can share the cell across threads. Decide what bound `T` really needs for
// `Sync` and `Send` to be sound, and tighten them.
unsafe impl<T> Sync for Once<T> {}
unsafe impl<T> Send for Once<T> {}

impl<T> Once<T> {
    pub const fn new() -> Self {
        todo!()
    }

    /// `Some` once a value has been stored.
    pub fn get(&self) -> Option<&T> {
        todo!()
    }

    /// Runs `f` in exactly one caller, even when many threads race here; the others wait and then all of them get
    /// the same reference. If `f` panics, the cell stays empty so a later caller can try again.
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        todo!()
    }

    pub fn into_inner(self) -> Option<T> {
        todo!()
    }
}

// TODO: `Drop`, so a stored value is dropped exactly once.
"""

DROP_COUNTER = """
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct Probe(Arc<AtomicUsize>, u32);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
"""

P.append(dict(
    slug="once-cell", title="A thread-safe write-once cell", level="hard", stage="understand-it",
    tags=["AtomicU8", "UnsafeCell", "MaybeUninit", "Acquire/Release", "Sync"],
    teaches=["A state machine in one atomic (EMPTY → RUNNING → READY): one thread wins the compare-exchange and writes.",
             "Release on the READY store and Acquire on the READY load publish the written value to readers.",
             "`unsafe impl Sync` with the right bounds, and a drop guard that makes initialization panic-safe."],
    statement="""
        Write `Once<T>`, the core of `std::sync::OnceLock`. `get_or_init(&self, f)` runs `f` in exactly one of the callers
        that race on it; the others wait and every caller gets the same `&T`. `get` returns `None` until a value is stored.

        - **Panics:** if `f` panics, the cell returns to empty and the panic keeps going; another caller may initialize it.
        - **Drop:** a stored value is dropped exactly once, when the `Once` is dropped, or moved out by `into_inner`.
        - **Threads:** `Once<T>` is `Sync` when `T: Send + Sync`, and `Send` when `T: Send`. Nothing else is allowed.

        Use one `AtomicU8` for the state and `UnsafeCell<MaybeUninit<T>>` for the value. Waiting callers may
        `yield_now()` in a loop.
    """,
    starter=ONCE_STARTER,
    solution=ONCE,
    visible=[
        T("empty_get", "fresh cell", "Once::<u32>::new().get().copied()", "None"),
        T("init_then_get", "get_or_init(|| 5), then get", "{ let c = Once::new(); let a = *c.get_or_init(|| 5); (a, c.get().copied()) }", "(5, Some(5))"),
        T("second_init_ignored", "get_or_init(|| 1), get_or_init(|| 2)", "{ let c = Once::new(); c.get_or_init(|| 1); *c.get_or_init(|| 2) }", "1"),
        T("same_reference", "two calls return the same address", "{ let c = Once::new(); let a = c.get_or_init(|| String::from(\"x\")) as *const String; let b = c.get_or_init(|| String::from(\"y\")) as *const String; a == b }", "true"),
        T("into_inner", "init, into_inner; and an empty one", "{ let c = Once::new(); c.get_or_init(|| \"v\".to_string()); (c.into_inner(), Once::<u8>::new().into_inner()) }", "(Some(\"v\".to_string()), None)"),
    ],
    hidden=[
        DROP_COUNTER + """
        #[test]
        fn value_dropped_exactly_once() {
            let drops = Arc::new(AtomicUsize::new(0));
            {
                let c = Once::new();
                c.get_or_init(|| Probe(drops.clone(), 1));
                c.get_or_init(|| Probe(drops.clone(), 2));
                check!("two get_or_init calls, drop the cell", drops.load(Ordering::SeqCst), 0);
            }
            check!("after the cell is dropped", drops.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn empty_cell_drops_nothing() {
            let drops = Arc::new(AtomicUsize::new(0));
            drop(Once::<Probe>::new());
            check!("an empty cell is dropped", drops.load(Ordering::SeqCst), 0);
        }

        #[test]
        fn into_inner_moves_the_value_out_without_dropping_it() {
            let drops = Arc::new(AtomicUsize::new(0));
            let c = Once::new();
            c.get_or_init(|| Probe(drops.clone(), 7));
            let p = c.into_inner().unwrap();
            check!("into_inner, value still alive", (drops.load(Ordering::SeqCst), p.1), (0, 7));
            drop(p);
            check!("after dropping the moved value", drops.load(Ordering::SeqCst), 1);
        }
        """,
        T("panic_leaves_it_empty", "initializer panics, then another succeeds",
          "{ let c = Once::new(); let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { c.get_or_init(|| -> u32 { panic!(\"boom\") }); })); (r.is_err(), c.get().copied(), *c.get_or_init(|| 9)) }",
          "(true, None, 9)"),
        T("panic_twice_then_ok", "two failed initializers, then success",
          "{ let c = Once::new(); for _ in 0..2 { let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { c.get_or_init(|| -> u8 { panic!(\"no\") }); })); } *c.get_or_init(|| 4) }",
          "4"),
        T("initializer_not_run_when_full", "closure counts its calls", "{ let c = Once::new(); let mut calls = 0; c.get_or_init(|| { calls += 1; 1 }); c.get_or_init(|| { calls += 1; 2 }); calls }", "1"),
        T("borrowing_initializer", "FnOnce moves a captured String", "{ let s = String::from(\"moved\"); let c = Once::new(); c.get_or_init(move || s).len() }", "5"),
        """
        #[test]
        fn sixteen_threads_one_initializer() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::{Arc, Barrier};
            let cell = Arc::new(Once::new());
            let calls = Arc::new(AtomicUsize::new(0));
            let barrier = Arc::new(Barrier::new(16));
            let hs: Vec<_> = (0..16usize)
                .map(|t| {
                    let (cell, calls, barrier) = (cell.clone(), calls.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        barrier.wait();
                        let r = cell.get_or_init(|| {
                            calls.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(20));
                            vec![t; 1000]
                        });
                        // every thread must see the finished vector, never a half-written one
                        (r.len(), r.iter().all(|&x| x == r[0]), r as *const Vec<usize> as usize)
                    })
                })
                .collect();
            let results: Vec<_> = hs.into_iter().map(|h| h.join().unwrap()).collect();
            let same_addr = results.iter().all(|r| r.2 == results[0].2);
            check!("16 racing callers: initializer calls", calls.load(Ordering::SeqCst), 1);
            check!("all see len 1000 and one address", (results.iter().all(|r| r.0 == 1000 && r.1), same_addr), (true, true));
        }

        #[test]
        fn many_cells_many_rounds() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::Arc;
            for _ in 0..200 {
                let cell = Arc::new(Once::new());
                let calls = Arc::new(AtomicUsize::new(0));
                let hs: Vec<_> = (0..4)
                    .map(|_| {
                        let (cell, calls) = (cell.clone(), calls.clone());
                        std::thread::spawn(move || *cell.get_or_init(|| { calls.fetch_add(1, Ordering::SeqCst); 42u64 }))
                    })
                    .collect();
                let vals: Vec<u64> = hs.into_iter().map(|h| h.join().unwrap()).collect();
                check!("4 threads racing on a fresh cell", (vals, calls.load(Ordering::SeqCst)), (vec![42; 4], 1));
            }
        }

        #[test]
        fn is_sync_and_send_for_the_right_types() {
            fn needs_sync<T: Sync>() {}
            fn needs_send<T: Send>() {}
            needs_sync::<Once<String>>();
            needs_send::<Once<String>>();
            needs_sync::<Once<Vec<u8>>>();
        }
        """,
    ],
    wrong=dict(
        every_racer_runs_f=sub(ONCE, "match self.state.compare_exchange(EMPTY, RUNNING, Ordering::Acquire, Ordering::Acquire) {",
                               "match (if self.state.load(Ordering::Acquire) == EMPTY { Ok(0) } else { Err(self.state.load(Ordering::Acquire)) }) {"),
        leaks_the_value=sub(ONCE, "if *self.state.get_mut() == READY {\n            unsafe { self.slot.get_mut().assume_init_drop() };\n        }", ""),
        double_drop_after_into_inner=sub(ONCE, "*self.state.get_mut() = EMPTY; // so Drop doesn't drop it again", ""),
    ),
    hints=[("approach", "Three states in one `AtomicU8`. `compare_exchange(EMPTY, RUNNING)` picks the single winner; losers see `RUNNING` and wait, or `READY` and return."),
           ("rust", "Store `READY` with `Release` after writing the slot, and load it with `Acquire` before reading: that pair is what makes the value visible. A small guard struct with `Drop` resets the state on unwind; `mem::forget` it on success."),
           ("edge", "`into_inner(self)` still runs `Drop` for the `Once`: set the state back to `EMPTY` (use `get_mut`, no atomics needed with `&mut self`) so the value isn't dropped twice.")],
    notes=("The compare-exchange gives mutual exclusion for the write; Release/Acquire on READY gives visibility of what was written. Readers that see anything but READY never touch the slot. The reset guard is the panic story: without it a panicking initializer leaves RUNNING forever and every other caller spins. This is the shape of `std::sync::Once` and `OnceLock`, minus the futex parking.", "O(1) after init", "O(1)"),
    follow_up="1) Why `Send + Sync` (not just `Sync`) for `T`? Give a type that breaks if you only require `Sync`. 2) Replace the `yield_now` spin with parking: what does `std::sync::Once` do (futex on Linux), and why is spinning a problem when the initializer takes seconds? 3) What goes wrong if `f` calls `get_or_init` on the same cell (re-entrancy), and how does std react? 4) Which orderings could be weakened here, and which cannot? 5) How would you build `Lazy<T>` and a global `static` on top of it?",
    related=["S7", "Y2"],
))

# ---------------------------------------------------------------------------------------------------------------
SPIN = """
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

pub struct Guard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        SpinLock { locked: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }

    pub fn lock(&self) -> Guard<'_, T> {
        loop {
            if let Some(g) = self.try_lock() {
                return g;
            }
            // test-and-test-and-set: wait on a plain load so the cache line stays shared while it is held
            while self.locked.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
        }
    }

    pub fn try_lock(&self) -> Option<Guard<'_, T>> {
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(Guard { lock: self })
        } else {
            None
        }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}
"""

SPIN_STARTER = """
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// Given loose so the tests compile: tighten the bounds to what is actually sound.
unsafe impl<T> Sync for SpinLock<T> {}
unsafe impl<T> Send for SpinLock<T> {}

pub struct Guard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        todo!()
    }

    /// Spins until the lock is free.
    pub fn lock(&self) -> Guard<'_, T> {
        todo!()
    }

    /// `None` immediately if somebody holds it.
    pub fn try_lock(&self) -> Option<Guard<'_, T>> {
        todo!()
    }

    pub fn into_inner(self) -> T {
        todo!()
    }

    /// No locking needed: `&mut self` proves nobody else has it.
    pub fn get_mut(&mut self) -> &mut T {
        todo!()
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        todo!()
    }
}

// TODO: `Drop` for `Guard`.
"""

P.append(dict(
    slug="spinlock", title="A spinlock with a guard", level="hard", stage="build-it",
    tags=["AtomicBool", "Acquire/Release", "RAII guard", "test-and-test-and-set"],
    teaches=["`compare_exchange(Acquire)` to take the lock and `store(Release)` to give it back: the pair that makes a critical section.",
             "An RAII guard (`Deref`, `DerefMut`, `Drop`) so the lock cannot be forgotten, with lifetimes tying the data to the guard.",
             "Test-and-test-and-set: spin on a read so waiting cores do not fight over the cache line."],
    statement="""
        Write `SpinLock<T>`, a mutex that busy-waits. `lock()` returns a `Guard` that derefs to `T` and unlocks when
        dropped. `try_lock()` never waits. `into_inner(self)` and `get_mut(&mut self)` reach the value without locking.

        Many threads will increment a plain (non-atomic) `u64` through the lock and the total must be exact, so the
        lock's ordering has to be right: everything done under one hold must be visible to the next. Mark the type `Sync`
        and `Send` with the weakest bound that is still sound.
    """,
    starter=SPIN_STARTER,
    solution=SPIN,
    visible=[
        T("lock_and_read", "lock a 5", "{ let l = SpinLock::new(5); let g = l.lock(); let v = *g; v }", "5"),
        T("mutate_through_guard", "add 1 twice", "{ let l = SpinLock::new(1); { *l.lock() += 1; } { *l.lock() += 1; } let v = *l.lock(); v }", "3"),
        T("try_lock_fails_while_held", "try_lock while a guard lives", "{ let l = SpinLock::new(0); let g = l.lock(); let busy = l.try_lock().is_none(); drop(g); let again = l.try_lock().is_some(); (busy, again) }", "(true, true)"),
        T("drop_releases", "lock, drop, lock again", "{ let l = SpinLock::new(String::new()); l.lock().push('a'); l.lock().push('b'); let v = l.lock().clone(); v }", "\"ab\".to_string()"),
        T("into_inner", "value comes back", "SpinLock::new(vec![1, 2]).into_inner()", "vec![1, 2]"),
    ],
    hidden=[
        T("get_mut_without_locking", "&mut access", "{ let mut l = SpinLock::new(1); *l.get_mut() = 8; l.into_inner() }", "8"),
        T("try_lock_guard_gives_access", "try_lock succeeds and mutates", "{ let l = SpinLock::new(1); if let Some(mut g) = l.try_lock() { *g = 6; } let v = *l.lock(); v }", "6"),
        T("two_locks_independent", "holding one does not block the other", "{ let a = SpinLock::new(1); let b = SpinLock::new(2); let _ga = a.lock(); let v = *b.lock(); v }", "2"),
        T("guard_derefs_to_methods", "vec methods through the guard", "{ let l = SpinLock::new(Vec::new()); l.lock().push(3); l.lock().push(4); let n = l.lock().len(); let second = l.lock()[1]; (n, second) }", "(2, 4)"),
        """
        #[test]
        fn value_dropped_when_lock_dropped() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::Arc;
            struct D(Arc<AtomicUsize>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let n = Arc::new(AtomicUsize::new(0));
            drop(SpinLock::new(D(n.clone())));
            check!("drop a lock holding a value", n.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn plain_counter_is_exact_across_threads() {
            let l = std::sync::Arc::new(SpinLock::new(0u64));
            let hs: Vec<_> = (0..8)
                .map(|_| {
                    let l = l.clone();
                    std::thread::spawn(move || {
                        for _ in 0..50_000 {
                            *l.lock() += 1;
                        }
                    })
                })
                .collect();
            for h in hs {
                h.join().unwrap();
            }
            let total = *l.lock();
            check!("8 threads x 50000 increments of a plain u64", total, 400_000);
        }

        #[test]
        fn long_critical_sections_stay_exclusive() {
            // each holder writes two fields in two steps; an observer that sees them differ broke mutual exclusion
            let l = std::sync::Arc::new(SpinLock::new((0u64, 0u64)));
            let hs: Vec<_> = (0..4)
                .map(|_| {
                    let l = l.clone();
                    std::thread::spawn(move || {
                        let mut torn = 0;
                        for _ in 0..20_000 {
                            let mut g = l.lock();
                            if g.0 != g.1 {
                                torn += 1;
                            }
                            g.0 += 1;
                            std::hint::spin_loop();
                            g.1 += 1;
                        }
                        torn
                    })
                })
                .collect();
            let torn: u32 = hs.into_iter().map(|h| h.join().unwrap()).sum();
            let g = l.lock();
            check!("4 threads, two-step updates", (torn, g.0, g.1), (0, 80_000, 80_000));
        }

        #[test]
        fn try_lock_contention_never_double_grants() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::Arc;
            let l = Arc::new(SpinLock::new(()));
            let inside = Arc::new(AtomicUsize::new(0));
            let overlap = Arc::new(AtomicUsize::new(0));
            let hs: Vec<_> = (0..6)
                .map(|_| {
                    let (l, inside, overlap) = (l.clone(), inside.clone(), overlap.clone());
                    std::thread::spawn(move || {
                        let mut got = 0u32;
                        for _ in 0..30_000 {
                            if let Some(_g) = l.try_lock() {
                                if inside.fetch_add(1, Ordering::SeqCst) != 0 {
                                    overlap.fetch_add(1, Ordering::SeqCst);
                                }
                                got += 1;
                                inside.fetch_sub(1, Ordering::SeqCst);
                            }
                        }
                        got
                    })
                })
                .collect();
            let got: u32 = hs.into_iter().map(|h| h.join().unwrap()).sum();
            check!("six threads try_lock in a loop: overlapping holders", overlap.load(Ordering::SeqCst), 0);
            assert!(got > 0, "nobody ever got the lock");
        }

        #[test]
        fn works_with_non_sync_payload() {
            // Cell<u32> is Send but not Sync: SpinLock<Cell<u32>> must still be Sync (T: Send is the bound)
            fn needs_sync<T: Sync>() {}
            needs_sync::<SpinLock<std::cell::Cell<u32>>>();
            needs_sync::<SpinLock<Vec<String>>>();
        }
        """,
    ],
    wrong=dict(
        check_then_set=sub(SPIN, """if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(Guard { lock: self })
        } else {
            None
        }""", """if !self.locked.load(Ordering::Acquire) {
            std::thread::yield_now();
            self.locked.store(true, Ordering::Release);
            Some(Guard { lock: self })
        } else {
            None
        }"""),
        try_lock_always_succeeds=sub(SPIN, "if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {",
                                     "self.locked.store(true, Ordering::Release);\n        if true {"),
    ),
    hints=[("approach", "`locked: AtomicBool`. `try_lock` is one `compare_exchange(false, true, Acquire, Relaxed)`. `Guard::drop` is one `store(false, Release)`."),
           ("rust", "`lock` loops on `try_lock`; between attempts spin on `load(Relaxed)` while it is still `true` (`std::hint::spin_loop()` inside), so waiting cores only read the cache line."),
           ("edge", "The `Sync` bound is `T: Send`, not `T: Sync`: the lock hands out `&mut T` to one thread at a time, so even a `Cell<u32>` is safe inside.")],
    notes=("Acquire on the successful swap and Release on unlock form the happens-before edge: all writes made under one hold are visible to the next holder, so the plain `u64` is safe. `Relaxed` is enough for the failure case and for the spin-wait load because neither reads protected data. The guard borrows the lock, so the borrow checker prevents using the data after unlock.", "O(1) uncontended", "O(1)"),
    follow_up="1) Why is a spinlock a bad idea in user space when the holder can be descheduled (priority inversion, wasted quantum)? What does `std::sync::Mutex` do instead? 2) Make it fair with a ticket lock (two counters): what does fairness cost? 3) Why is test-and-test-and-set faster than a plain CAS loop on a many-core machine? 4) What would change if a panic happens while the guard is held (poisoning)? 5) Which of the two orderings could you weaken to Relaxed, and what breaks?",
    related=["C1", "F6"],
))

STAGES = [("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("c3-atomics-lock-free", "C3", "Atomics & lock-free", "C", "sde3", 3,
                    "Memory ordering and lock-free structures by building them: counters, a once-cell, a spinlock, queues, a stack and an Arc.",
                    STAGES, P)
    print("C3", n)
