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

# ---------------------------------------------------------------------------------------------------------------
ARC = """
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

struct Inner<T> {
    strong: AtomicUsize,
    data: T,
}

/// A reference-counted pointer that can cross threads.
pub struct MiniArc<T> {
    ptr: NonNull<Inner<T>>,
    _owns: PhantomData<Inner<T>>,
}

unsafe impl<T: Send + Sync> Send for MiniArc<T> {}
unsafe impl<T: Send + Sync> Sync for MiniArc<T> {}

impl<T> MiniArc<T> {
    pub fn new(data: T) -> Self {
        let b = Box::new(Inner { strong: AtomicUsize::new(1), data });
        MiniArc { ptr: NonNull::from(Box::leak(b)), _owns: PhantomData }
    }

    fn inner(&self) -> &Inner<T> {
        unsafe { self.ptr.as_ref() }
    }

    pub fn strong_count(this: &Self) -> usize {
        this.inner().strong.load(Ordering::Acquire)
    }

    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.ptr == b.ptr
    }

    /// `Some` only while this is the sole owner.
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        if this.inner().strong.load(Ordering::Acquire) == 1 {
            Some(unsafe { &mut (*this.ptr.as_ptr()).data })
        } else {
            None
        }
    }

    /// Takes the value out if this is the only owner, else gives `this` back.
    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        if this.inner().strong.compare_exchange(1, 0, Ordering::Relaxed, Ordering::Relaxed).is_err() {
            return Err(this);
        }
        fence(Ordering::Acquire);
        let ptr = this.ptr.as_ptr();
        std::mem::forget(this);
        let boxed = unsafe { Box::from_raw(ptr) };
        let Inner { data, .. } = *boxed;
        Ok(data)
    }

    /// Clone-on-write: a unique owner gets `&mut T` directly, a shared one first copies the value.
    pub fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        if Self::strong_count(this) != 1 {
            *this = MiniArc::new((**this).clone());
        }
        Self::get_mut(this).unwrap()
    }
}

impl<T> Deref for MiniArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner().data
    }
}

impl<T> Clone for MiniArc<T> {
    fn clone(&self) -> Self {
        let old = self.inner().strong.fetch_add(1, Ordering::Relaxed);
        if old > isize::MAX as usize {
            std::process::abort(); // a leaked-clone loop would wrap the count and free live data
        }
        MiniArc { ptr: self.ptr, _owns: PhantomData }
    }
}

impl<T> Drop for MiniArc<T> {
    fn drop(&mut self) {
        if self.inner().strong.fetch_sub(1, Ordering::Release) == 1 {
            fence(Ordering::Acquire);
            drop(unsafe { Box::from_raw(self.ptr.as_ptr()) });
        }
    }
}
"""

ARC_STARTER = """
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

struct Inner<T> {
    strong: AtomicUsize,
    data: T,
}

pub struct MiniArc<T> {
    ptr: NonNull<Inner<T>>,
    _owns: PhantomData<Inner<T>>,
}

// Given loose so the tests compile: tighten them to what sharing a `T` across threads really requires.
unsafe impl<T> Send for MiniArc<T> {}
unsafe impl<T> Sync for MiniArc<T> {}

impl<T> MiniArc<T> {
    pub fn new(data: T) -> Self {
        todo!()
    }

    /// Number of live owners.
    pub fn strong_count(this: &Self) -> usize {
        todo!()
    }

    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        todo!()
    }

    /// `Some` only while `this` is the sole owner.
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        todo!()
    }

    /// Moves the value out when `this` is the only owner, otherwise gives `this` back untouched.
    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        todo!()
    }

    /// Clone-on-write: unique owners get `&mut T` directly; a shared value is cloned into a fresh allocation first.
    pub fn make_mut(this: &mut Self) -> &mut T
    where
        T: Clone,
    {
        todo!()
    }
}

impl<T> Deref for MiniArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        todo!()
    }
}

// TODO: `Clone` (what ordering does a count increment need?) and `Drop` (which ordering, and what fence, before freeing?).
impl<T> Clone for MiniArc<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl<T> Drop for MiniArc<T> {
    fn drop(&mut self) {}
}
"""

PROBE = """
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct Probe(Arc<AtomicUsize>);
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
"""

P.append(dict(
    slug="mini-arc", title="MiniArc: your own Arc", level="hard", stage="build-it",
    tags=["Arc", "Release/Acquire fence", "NonNull", "Send/Sync", "clone-on-write"],
    teaches=["A relaxed increment on clone, a Release decrement on drop, and an Acquire fence before freeing.",
             "`Send`/`Sync` for a shared owner require `T: Send + Sync`: why `Arc<Cell<_>>` is not `Send`.",
             "`try_unwrap` with compare-exchange(1, 0) and `make_mut`: how `Arc` gives mutation back safely."],
    statement="""
        Write `MiniArc<T>`: `new`, `Clone`, `Deref`, `Drop`, plus the associated functions `strong_count`, `ptr_eq`,
        `get_mut` (only while unique), `try_unwrap` (move the value out if unique, else give the `MiniArc` back) and
        `make_mut` (clone-on-write). Allocate one block holding the count and the value, and free it when the last
        owner is dropped: the value's destructor must run **exactly once**, never early, never twice.

        The count is shared by threads, so `clone` and `drop` are atomic. Think about which ordering each one needs:
        the thread that frees must see every write made by every other owner before it dropped its handle.
    """,
    starter=ARC_STARTER,
    solution=ARC,
    visible=[
        T("deref", "new(5)", "*MiniArc::new(5)", "5"),
        T("clone_shares", "two handles, one allocation", "{ let a = MiniArc::new(String::from(\"x\")); let b = a.clone(); (MiniArc::ptr_eq(&a, &b), MiniArc::strong_count(&a), b.len()) }", "(true, 2, 1)"),
        T("count_goes_down", "clone, clone, drop one", "{ let a = MiniArc::new(1); let b = a.clone(); let c = a.clone(); drop(b); let n = MiniArc::strong_count(&a); drop(c); (n, MiniArc::strong_count(&a)) }", "(2, 1)"),
        T("get_mut_only_when_unique", "get_mut with and without a clone", "{ let mut a = MiniArc::new(1); let b = a.clone(); let shared = MiniArc::get_mut(&mut a).is_none(); drop(b); let unique = MiniArc::get_mut(&mut a).is_some(); (shared, unique) }", "(true, true)"),
        T("try_unwrap_unique_and_shared", "unique, then shared", "{ let a = MiniArc::new(7); let ok = MiniArc::try_unwrap(a).ok(); let b = MiniArc::new(8); let b2 = b.clone(); let err = MiniArc::try_unwrap(b).err().map(|x| *x); drop(b2); (ok, err) }", "(Some(7), Some(8))"),
    ],
    hidden=[
        PROBE + """
        #[test]
        fn value_dropped_once_when_last_handle_goes() {
            let n = Arc::new(AtomicUsize::new(0));
            let a = MiniArc::new(Probe(n.clone()));
            let b = a.clone();
            let c = b.clone();
            drop(a);
            drop(c);
            check!("two of three handles dropped", n.load(Ordering::SeqCst), 0);
            drop(b);
            check!("last handle dropped", n.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn try_unwrap_moves_value_without_dropping_it() {
            let n = Arc::new(AtomicUsize::new(0));
            let a = MiniArc::new(Probe(n.clone()));
            let p = MiniArc::try_unwrap(a).ok().unwrap();
            check!("after try_unwrap, value alive", n.load(Ordering::SeqCst), 0);
            drop(p);
            check!("after dropping the moved value", n.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn failed_try_unwrap_keeps_everything_alive() {
            let n = Arc::new(AtomicUsize::new(0));
            let a = MiniArc::new(Probe(n.clone()));
            let b = a.clone();
            let back = MiniArc::try_unwrap(a).err().unwrap();
            check!("shared try_unwrap fails: count, drops", (MiniArc::strong_count(&back), n.load(Ordering::SeqCst)), (2, 0));
            drop(back);
            drop(b);
            check!("both handles dropped", n.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn sixteen_threads_clone_and_drop() {
            let n = Arc::new(AtomicUsize::new(0));
            let a = MiniArc::new(Probe(n.clone()));
            let hs: Vec<_> = (0..16)
                .map(|_| {
                    let a = a.clone();
                    std::thread::spawn(move || {
                        for _ in 0..5_000 {
                            let b = a.clone();
                            let c = b.clone();
                            drop(b);
                            drop(c);
                        }
                    })
                })
                .collect();
            for h in hs {
                h.join().unwrap();
            }
            check!("after the threads finish: count, drops", (MiniArc::strong_count(&a), n.load(Ordering::SeqCst)), (1, 0));
            drop(a);
            check!("after the last drop", n.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn last_dropper_frees_exactly_once_under_a_race() {
            for _ in 0..300 {
                let n = Arc::new(AtomicUsize::new(0));
                let a = MiniArc::new(Probe(n.clone()));
                let hs: Vec<_> = (0..4)
                    .map(|_| {
                        let a = a.clone();
                        std::thread::spawn(move || drop(a))
                    })
                    .collect();
                drop(a);
                for h in hs {
                    h.join().unwrap();
                }
                check!("five handles dropped on five threads", n.load(Ordering::SeqCst), 1);
            }
        }
        """,
        T("data_written_before_clone_is_visible", "a thread reads a Vec built before the spawn", "{ let a = MiniArc::new((0..1000u32).collect::<Vec<_>>()); let b = a.clone(); std::thread::spawn(move || b.iter().sum::<u32>()).join().unwrap() }", "499_500"),
        T("make_mut_unique_does_not_copy", "unique owner keeps its allocation", "{ let mut a = MiniArc::new(vec![1]); let before = &*a as *const Vec<i32>; MiniArc::make_mut(&mut a).push(2); let same = before == &*a as *const Vec<i32>; (same, (*a).clone()) }", "(true, vec![1, 2])"),
        T("make_mut_shared_copies", "shared owner detaches; the other copy is unchanged", "{ let mut a = MiniArc::new(vec![1]); let b = a.clone(); MiniArc::make_mut(&mut a).push(2); (MiniArc::ptr_eq(&a, &b), (*a).clone(), (*b).clone(), MiniArc::strong_count(&b)) }", "(false, vec![1, 2], vec![1], 1)"),
        T("zero_sized_values", "MiniArc<()> counts correctly", "{ let a = MiniArc::new(()); let b = a.clone(); let n = MiniArc::strong_count(&a); drop(b); (n, MiniArc::strong_count(&a)) }", "(2, 1)"),
        T("send_and_sync_for_sendable_data", "compile-time bounds", "{ fn s<T: Send + Sync>() {} s::<MiniArc<String>>(); s::<MiniArc<Vec<u8>>>(); true }", "true"),
        """
        #[test]
        fn random_clone_drop_matches_a_model() {
            let mut rng = anneal_prelude::Rng::new(4202);
            for _ in 0..200 {
                let first = MiniArc::new(0u8);
                let mut handles = vec![first];
                for _ in 0..rng.below(40) {
                    if handles.len() == 1 || rng.below(3) != 0 {
                        let i = rng.below(handles.len());
                        let c = handles[i].clone();
                        handles.push(c);
                    } else {
                        let i = rng.below(handles.len());
                        handles.swap_remove(i);
                    }
                    check!(format!("{} live handles", handles.len()), MiniArc::strong_count(&handles[0]), handles.len());
                }
            }
        }
        """,
    ],
    wrong=dict(
        racy_decrement=sub(ARC, "if self.inner().strong.fetch_sub(1, Ordering::Release) == 1 {",
                           "let c = self.inner().strong.load(Ordering::Acquire);\n        std::thread::yield_now();\n        self.inner().strong.store(c - 1, Ordering::Release);\n        if c == 1 {"),
        get_mut_when_two=sub(ARC, "if this.inner().strong.load(Ordering::Acquire) == 1 {\n            Some(", "if this.inner().strong.load(Ordering::Acquire) <= 2 {\n            Some("),
        make_mut_always_copies=sub(ARC, "if Self::strong_count(this) != 1 {", "if true {"),
    ),
    hints=[("approach", "One heap block: `Inner { strong: AtomicUsize, data: T }`. `new` leaks a `Box` into a `NonNull`; the last owner rebuilds the box with `Box::from_raw` and drops it."),
           ("rust", "Clone: `fetch_add(1, Relaxed)`. Drop: `fetch_sub(1, Release) == 1`, and only then `fence(Acquire)` and free, so the freeing thread sees every other owner's writes. `try_unwrap`: `compare_exchange(1, 0)`, then move `data` out of the rebuilt box and `mem::forget` the handle."),
           ("edge", "`make_mut` on a shared handle replaces `*this` with a brand new `MiniArc` of a clone; assigning drops the old handle (and decrements the old count) for you.")],
    notes=("Clone needs no ordering because holding a handle already proves the allocation is alive. Drop is the subtle part: Release on every decrement publishes each owner's last writes, and the thread that sees the count reach zero issues an Acquire fence before it destroys the data. `Send` and `Sync` both need `T: Send + Sync` because a `MiniArc` lets any thread both read the value and (as last owner) drop it. `Weak` adds a second count and is the next step.", "O(1) per clone or drop", "O(1) extra per allocation"),
    follow_up="1) Why is `Arc<RefCell<T>>` not `Send`, and `Arc<Mutex<T>>` is? 2) The `abort` when the count passes `isize::MAX`: what attack or bug does it prevent? 3) Add `Weak<T>`: how many counts, and why does `upgrade` need a compare-exchange loop and not `fetch_add`? 4) How would you support `MiniArc<dyn Trait>` and `MiniArc<[T]>` (unsized `NonNull<Inner<T>>`, `CoerceUnsized`)? 5) Why does `std::sync::Arc` use `Release` on drop and `Acquire` as a fence rather than `AcqRel` on every decrement?",
    related=["S7", "Y2"],
))

# ---------------------------------------------------------------------------------------------------------------
SPSC = """
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize, // next slot to pop: written only by the consumer
    tail: AtomicUsize, // next slot to push: written only by the producer
}

// SAFETY: each slot is touched by only one side at a time; the head/tail handshake hands it over.
unsafe impl<T: Send> Sync for Shared<T> {}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
}

pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    assert!(capacity > 0, "capacity must be at least 1");
    let buf: Box<[UnsafeCell<MaybeUninit<T>>]> = (0..capacity).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
    let shared = Arc::new(Shared { buf, head: AtomicUsize::new(0), tail: AtomicUsize::new(0) });
    (Producer { shared: shared.clone() }, Consumer { shared })
}

impl<T> Producer<T> {
    /// Gives `value` back when the buffer is full.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let s = &*self.shared;
        let tail = s.tail.load(Ordering::Relaxed); // only we write it
        let head = s.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) == s.buf.len() {
            return Err(value);
        }
        unsafe { (*s.buf[tail % s.buf.len()].get()).write(value) };
        s.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// True once the consumer has been dropped.
    pub fn is_abandoned(&self) -> bool {
        Arc::strong_count(&self.shared) == 1
    }
}

impl<T> Consumer<T> {
    pub fn pop(&mut self) -> Option<T> {
        let s = &*self.shared;
        let head = s.head.load(Ordering::Relaxed); // only we write it
        let tail = s.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let value = unsafe { (*s.buf[head % s.buf.len()].get()).assume_init_read() };
        s.head.store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    /// True once the producer has been dropped. Items may still be waiting: pop until `None` after seeing this.
    pub fn is_abandoned(&self) -> bool {
        Arc::strong_count(&self.shared) == 1
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        let (mut i, tail) = (*self.head.get_mut(), *self.tail.get_mut());
        while i != tail {
            unsafe { self.buf[i % self.buf.len()].get_mut().assume_init_drop() };
            i = i.wrapping_add(1);
        }
    }
}
"""

SPSC_STARTER = """
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Shared<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize, // next slot to pop
    tail: AtomicUsize, // next slot to push
}

// Given loose so the tests compile: what does `Shared<T>` need from `T` to be shared by two threads?
unsafe impl<T> Sync for Shared<T> {}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
}

/// Panics if `capacity` is 0. Exactly `capacity` items fit.
pub fn channel<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    todo!()
}

impl<T> Producer<T> {
    /// Gives `value` back when the buffer is full. Never blocks.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        todo!()
    }

    /// True once the consumer has been dropped.
    pub fn is_abandoned(&self) -> bool {
        todo!()
    }
}

impl<T> Consumer<T> {
    /// `None` when empty. Never blocks.
    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    /// True once the producer has been dropped (items may still be waiting).
    pub fn is_abandoned(&self) -> bool {
        todo!()
    }
}

// TODO: items still in the buffer when everything is dropped must be dropped exactly once.
"""

SPSC_PUMP = """
        fn pump(cap: usize, n: u64) -> (u64, bool) {
            let (mut p, mut c) = channel::<u64>(cap);
            let prod = std::thread::spawn(move || {
                for i in 0..n {
                    let mut v = i;
                    loop {
                        match p.push(v) {
                            Ok(()) => break,
                            Err(back) => {
                                v = back;
                                std::hint::spin_loop();
                            }
                        }
                    }
                }
            });
            let mut expect = 0u64;
            let mut in_order = true;
            let mut sum = 0u64;
            while expect < n {
                match c.pop() {
                    Some(v) => {
                        in_order &= v == expect;
                        sum += v;
                        expect += 1;
                    }
                    None => std::hint::spin_loop(),
                }
            }
            prod.join().unwrap();
            (sum, in_order)
        }
"""

P.append(dict(
    slug="spsc-ring", title="A wait-free SPSC ring buffer", level="hard", stage="build-it",
    tags=["SPSC", "Acquire/Release", "head/tail", "MaybeUninit", "Drop"],
    teaches=["Single-producer single-consumer needs no CAS: each index has exactly one writer.",
             "Each side reads its own index Relaxed and the other side's Acquire, and publishes its own with Release.",
             "Initialized-region bookkeeping with `MaybeUninit`, and dropping what is left in the buffer."],
    statement="""
        `channel(capacity)` returns a `Producer` and a `Consumer` over a fixed ring. `Producer::push` hands the value
        back (`Err(value)`) when the ring is full; `Consumer::pop` returns `None` when it is empty. Neither ever blocks
        or takes a lock, and each end is used by one thread (hence `&mut self`). Exactly `capacity` items fit, and items
        come out in the order they went in.

        Items still inside when both ends are gone must be dropped exactly once. `is_abandoned` tells one side the other
        was dropped, so a consumer can tell "empty for now" from "empty forever".
    """,
    starter=SPSC_STARTER,
    solution=SPSC,
    visible=[
        T("push_then_pop", "capacity 4, push 1, 2, 3, pop three", "{ let (mut p, mut c) = channel(4); p.push(1).unwrap(); p.push(2).unwrap(); p.push(3).unwrap(); (c.pop(), c.pop(), c.pop(), c.pop()) }", "(Some(1), Some(2), Some(3), None)"),
        T("full_gives_the_value_back", "capacity 2, third push", "{ let (mut p, _c) = channel(2); p.push(\"a\".to_string()).unwrap(); p.push(\"b\".to_string()).unwrap(); p.push(\"c\".to_string()) }", "Err(\"c\".to_string())"),
        T("exactly_capacity_fit", "capacity 3, three pushes succeed", "{ let (mut p, _c) = channel(3); (p.push(1).is_ok(), p.push(2).is_ok(), p.push(3).is_ok(), p.push(4).is_ok()) }", "(true, true, true, false)"),
        T("space_comes_back_after_pop", "capacity 1, push pop push", "{ let (mut p, mut c) = channel(1); p.push(1).unwrap(); let full = p.push(2).is_err(); let a = c.pop(); let ok = p.push(3).is_ok(); (full, a, ok, c.pop()) }", "(true, Some(1), true, Some(3))"),
        T("empty_pop", "nothing pushed", "{ let (_p, mut c) = channel::<u8>(2); c.pop() }", "None"),
    ],
    hidden=[
        T("zero_capacity_panics", "capacity 0", "std::panic::catch_unwind(|| channel::<u8>(0)).is_err()", "true"),
        T("wraps_around_many_times", "capacity 3, 1000 push/pop pairs", "{ let (mut p, mut c) = channel(3); let mut ok = true; for i in 0..1000u32 { p.push(i).unwrap(); p.push(i + 1).unwrap(); ok &= c.pop() == Some(i); ok &= c.pop() == Some(i + 1); } ok }", "true"),
        T("abandon_flags", "dropping one end shows on the other", "{ let (p, c) = channel::<u8>(2); let before = (p.is_abandoned(), c.is_abandoned()); drop(p); let after = c.is_abandoned(); (before, after) }", "((false, false), true)"),
        T("abandoned_consumer_seen_by_producer", "drop consumer", "{ let (p, c) = channel::<u8>(2); drop(c); p.is_abandoned() }", "true"),
        T("items_left_after_producer_dropped", "pop still works", "{ let (mut p, mut c) = channel(4); p.push(1).unwrap(); p.push(2).unwrap(); drop(p); (c.pop(), c.pop(), c.pop(), c.is_abandoned()) }", "(Some(1), Some(2), None, true)"),
        T("string_values_are_moved_not_copied", "heap values round-trip", "{ let (mut p, mut c) = channel(2); p.push(vec![1u8; 100]).unwrap(); c.pop().map(|v| v.len()) }", "Some(100)"),
        """
        #[test]
        fn leftover_items_dropped_once() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::Arc;
            struct D(Arc<AtomicUsize>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let n = Arc::new(AtomicUsize::new(0));
            let (mut p, mut c) = channel(5);
            for _ in 0..5 {
                p.push(D(n.clone())).ok().unwrap();
            }
            drop(c.pop()); // one consumed (dropped by us)
            drop(c.pop());
            check!("two popped and dropped", n.load(Ordering::SeqCst), 2);
            p.push(D(n.clone())).ok().unwrap(); // wraps into slot 0
            drop(p);
            drop(c);
            check!("six pushed in total, all dropped exactly once", n.load(Ordering::SeqCst), 6);
        }

        #[test]
        fn rejected_value_is_not_dropped_twice() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::Arc;
            struct D(Arc<AtomicUsize>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let n = Arc::new(AtomicUsize::new(0));
            let (mut p, c) = channel(1);
            p.push(D(n.clone())).ok().unwrap();
            let back = p.push(D(n.clone())).err().unwrap();
            check!("full: nothing dropped yet", n.load(Ordering::SeqCst), 0);
            drop(back);
            drop(p);
            drop(c);
            check!("rejected value and stored value", n.load(Ordering::SeqCst), 2);
        }

        #[test]
        fn ends_are_send() {
            fn needs_send<T: Send>() {}
            needs_send::<Producer<String>>();
            needs_send::<Consumer<String>>();
        }

        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(4303);
            for _ in 0..300 {
                let cap = rng.below(6) + 1;
                let (mut p, mut c) = channel(cap);
                let mut model = std::collections::VecDeque::new();
                let mut log = Vec::new();
                for _ in 0..rng.below(40) {
                    if rng.bool() {
                        let v = rng.below(1000) as u32;
                        let want = if model.len() == cap { Err(v) } else { model.push_back(v); Ok(()) };
                        log.push(format!("push {v}"));
                        check!(format!("cap {cap}: {}", log.join(", ")), p.push(v), want);
                    } else {
                        log.push("pop".to_string());
                        check!(format!("cap {cap}: {}", log.join(", ")), c.pop(), model.pop_front());
                    }
                }
            }
        }
        """,
        SPSC_PUMP + """
        #[test]
        fn million_items_in_order_through_a_small_ring() {
            let (sum, in_order) = pump(64, 1_000_000);
            check!("1,000,000 items through capacity 64: sum and order", (sum, in_order), (499_999_500_000, true));
        }

        #[test]
        fn capacity_one_ping_pong_between_threads() {
            let (sum, in_order) = pump(1, 50_000);
            check!("50,000 items through capacity 1: sum and order", (sum, in_order), (1_249_975_000, true));
        }

        #[test]
        fn consumer_drains_until_the_producer_is_gone() {
            let (mut p, mut c) = channel::<String>(8);
            let prod = std::thread::spawn(move || {
                for i in 0..20_000 {
                    let mut s = format!("msg{i}");
                    while let Err(back) = p.push(s) {
                        s = back;
                        std::thread::yield_now();
                    }
                }
            });
            let mut got = 0usize;
            let mut last_ok = true;
            loop {
                match c.pop() {
                    Some(s) => {
                        last_ok &= s == format!("msg{got}");
                        got += 1;
                    }
                    None if c.is_abandoned() => match c.pop() {
                        Some(s) => {
                            last_ok &= s == format!("msg{got}");
                            got += 1;
                        }
                        None => break,
                    },
                    None => std::thread::yield_now(),
                }
            }
            prod.join().unwrap();
            check!("consumer loops until abandoned and empty", (got, last_ok), (20_000, true));
        }
        """,
    ],
    wrong=dict(
        one_slot_wasted=sub(SPSC, "if tail.wrapping_sub(head) == s.buf.len() {", "if tail.wrapping_sub(head) + 1 >= s.buf.len() {"),
        leaks_leftovers=sub(SPSC, "while i != tail {\n            unsafe { self.buf[i % self.buf.len()].get_mut().assume_init_drop() };\n            i = i.wrapping_add(1);\n        }", "let _ = (&mut i, tail);"),
        publishes_before_writing=sub(SPSC, """unsafe { (*s.buf[tail % s.buf.len()].get()).write(value) };
        s.tail.store(tail.wrapping_add(1), Ordering::Release);""", """s.tail.store(tail.wrapping_add(1), Ordering::Release);
        std::thread::yield_now();
        unsafe { (*s.buf[tail % s.buf.len()].get()).write(value) };"""),
    ),
    hints=[("approach", "Two counters that only ever grow: `tail` (pushed so far, written by the producer) and `head` (popped so far, written by the consumer). Used = `tail - head`; the slot for count `i` is `i % capacity`. Full means `tail - head == capacity`."),
           ("rust", "Producer: load `tail` Relaxed (it is the only writer), load `head` Acquire, write the slot, then `tail.store(tail+1, Release)`. The consumer mirrors it. The `Release` store is what publishes the slot's contents."),
           ("edge", "Drop lives on the shared block (it runs when the second end goes away): drop the initialized range `head..tail` with `assume_init_drop`. `is_abandoned` is `Arc::strong_count == 1`.")],
    notes=("There is no compare-exchange: each counter has one writer, so a plain store is enough. The Release store of `tail` makes the freshly written slot visible to a consumer that Acquire-loads `tail`; symmetrically the consumer's Release store of `head` tells the producer that a slot has been read and may be overwritten. Counters grow without wrapping to a modulo so full and empty are never ambiguous. Producer and consumer are `&mut self` so the type system enforces 'one thread per end'.", "O(1) push and pop, wait-free", "O(capacity)"),
    follow_up="1) Why does `std::sync::mpsc` not work like this? What do you pay for multiple producers? 2) Power-of-two capacity: replace `% capacity` with `& mask`. What breaks at `usize` overflow when the capacity is not a power of two? 3) Cache lines: where do `head` and `tail` live, and what does padding each to 64 bytes (and caching the other side's index locally) buy? (rtrb and crossbeam do both.) 4) Add a blocking `recv` using `thread::park`/`unpark`: where is the lost-wakeup race? 5) What does `loom` check that a stress test cannot?",
    related=["C2", "F6"],
))

# ---------------------------------------------------------------------------------------------------------------
ISTACK = """
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const NIL: u32 = u32::MAX;

pub fn pack(tag: u32, index: u32) -> u64 {
    ((tag as u64) << 32) | index as u64
}

pub fn unpack(word: u64) -> (u32, u32) {
    ((word >> 32) as u32, word as u32)
}

/// A lock-free LIFO of `u64` values over a fixed pool of slots. Two Treiber lists share the pool: the stack itself
/// and a free list. Each head packs a 32-bit version tag next to the slot index, so a compare-exchange fails if the
/// head was popped and pushed back in between (the ABA problem).
pub struct Stack {
    values: Box<[AtomicU64]>,
    next: Box<[AtomicU32]>,
    top: AtomicU64,
    free: AtomicU64,
}

fn pop_slot(list: &AtomicU64, next: &[AtomicU32]) -> Option<u32> {
    let mut cur = list.load(Ordering::Acquire);
    loop {
        let (tag, index) = unpack(cur);
        if index == NIL {
            return None;
        }
        let after = next[index as usize].load(Ordering::Relaxed);
        match list.compare_exchange_weak(cur, pack(tag.wrapping_add(1), after), Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => return Some(index),
            Err(seen) => cur = seen,
        }
    }
}

fn push_slot(list: &AtomicU64, next: &[AtomicU32], index: u32) {
    let mut cur = list.load(Ordering::Relaxed);
    loop {
        let (tag, head) = unpack(cur);
        next[index as usize].store(head, Ordering::Relaxed);
        match list.compare_exchange_weak(cur, pack(tag.wrapping_add(1), index), Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return,
            Err(seen) => cur = seen,
        }
    }
}

impl Stack {
    /// Room for `capacity` values (at most `u32::MAX - 1`).
    pub fn new(capacity: usize) -> Self {
        assert!(capacity < NIL as usize);
        let values = (0..capacity).map(|_| AtomicU64::new(0)).collect();
        let next = (0..capacity).map(|i| AtomicU32::new(if i + 1 < capacity { i as u32 + 1 } else { NIL })).collect();
        let first_free = if capacity == 0 { NIL } else { 0 };
        Stack { values, next, top: AtomicU64::new(pack(0, NIL)), free: AtomicU64::new(pack(0, first_free)) }
    }

    pub fn capacity(&self) -> usize {
        self.values.len()
    }

    /// Gives the value back when every slot is in use.
    pub fn push(&self, value: u64) -> Result<(), u64> {
        let Some(slot) = pop_slot(&self.free, &self.next) else {
            return Err(value);
        };
        self.values[slot as usize].store(value, Ordering::Relaxed);
        push_slot(&self.top, &self.next, slot);
        Ok(())
    }

    pub fn pop(&self) -> Option<u64> {
        let slot = pop_slot(&self.top, &self.next)?;
        let value = self.values[slot as usize].load(Ordering::Relaxed);
        push_slot(&self.free, &self.next, slot);
        Some(value)
    }
}
"""

ISTACK_STARTER = """
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub const NIL: u32 = u32::MAX;

/// Version tag in the high 32 bits, slot index in the low 32.
pub fn pack(tag: u32, index: u32) -> u64 {
    todo!()
}

pub fn unpack(word: u64) -> (u32, u32) {
    todo!()
}

/// A lock-free LIFO of `u64` values over a fixed pool of slots. Two lists share the pool: the stack (`top`) and the
/// free slots (`free`). `next[i]` links slot `i` to the slot below it in whichever list it is on.
pub struct Stack {
    values: Box<[AtomicU64]>,
    next: Box<[AtomicU32]>,
    top: AtomicU64,
    free: AtomicU64,
}

impl Stack {
    /// A stack with room for `capacity` values; every slot starts on the free list.
    pub fn new(capacity: usize) -> Self {
        todo!()
    }

    pub fn capacity(&self) -> usize {
        todo!()
    }

    /// Gives the value back when every slot is in use. Lock-free: never waits on another thread.
    pub fn push(&self, value: u64) -> Result<(), u64> {
        todo!()
    }

    pub fn pop(&self) -> Option<u64> {
        todo!()
    }
}
"""

P.append(dict(
    slug="index-stack", title="An ABA-safe lock-free stack", level="hard", stage="build-it",
    tags=["Treiber stack", "ABA", "bit packing", "compare_exchange", "free list"],
    teaches=["A Treiber stack: push and pop are a CAS loop on the head.",
             "The ABA problem: the head looks unchanged but the list under it is not. A version tag packed next to the index fixes it.",
             "Bit packing two 32-bit fields into one `AtomicU64`, and a slot pool instead of per-node `Box`es (no reclamation problem)."],
    statement="""
        Write `Stack`, a lock-free LIFO of `u64`s with a fixed capacity, shared by reference between any number of
        threads. Values live in a pool of slots; slots are linked by index. There are two lists in the pool: the stack
        itself and a free list of unused slots, and both are classic Treiber lists (push: set `next`, then CAS the head;
        pop: CAS the head to the slot's `next`).

        Pop reads `next[index]` *before* its CAS. If another thread pops that slot, pops its successor and pushes the slot
        back, the head looks unchanged but `next` is stale, and a plain CAS corrupts the list. Pack a **32-bit version
        tag** with the 32-bit index in each head and bump it on every successful CAS, so that CAS fails instead.

        `push` returns `Err(value)` when the pool is empty; `pop` returns `None` when the stack is empty.
    """,
    starter=ISTACK_STARTER,
    solution=ISTACK,
    visible=[
        T("lifo_order", "push 1, 2, 3; pop three", "{ let s = Stack::new(4); s.push(1).unwrap(); s.push(2).unwrap(); s.push(3).unwrap(); (s.pop(), s.pop(), s.pop(), s.pop()) }", "(Some(3), Some(2), Some(1), None)"),
        T("full_gives_value_back", "capacity 2, third push", "{ let s = Stack::new(2); s.push(10).unwrap(); s.push(20).unwrap(); s.push(30) }", "Err(30)"),
        T("pop_frees_a_slot", "fill, pop, push again", "{ let s = Stack::new(2); s.push(1).unwrap(); s.push(2).unwrap(); let a = s.pop(); let ok = s.push(3).is_ok(); (a, ok, s.pop(), s.pop(), s.pop()) }", "(Some(2), true, Some(3), Some(1), None)"),
        T("empty_pop", "new stack", "Stack::new(3).pop()", "None"),
        T("pack_roundtrip", "tag and index survive packing", "unpack(pack(7, 9))", "(7, 9)"),
    ],
    hidden=[
        T("zero_capacity", "capacity 0", "{ let s = Stack::new(0); (s.push(1), s.pop(), s.capacity()) }", "(Err(1), None, 0)"),
        T("pack_extremes", "max tag, NIL index", "(unpack(pack(u32::MAX, NIL)), unpack(pack(0, 0)))", "((u32::MAX, NIL), (0, 0))"),
        T("values_are_full_u64", "u64::MAX and 0 round-trip", "{ let s = Stack::new(2); s.push(u64::MAX).unwrap(); s.push(0).unwrap(); (s.pop(), s.pop()) }", "(Some(0), Some(u64::MAX))"),
        T("capacity_reported", "capacity 7", "Stack::new(7).capacity()", "7"),
        T("slots_recycle_many_times", "capacity 3, 10000 push/pop", "{ let s = Stack::new(3); let mut ok = true; for i in 0..10_000u64 { ok &= s.push(i).is_ok(); ok &= s.pop() == Some(i); } (ok, s.pop()) }", "(true, None)"),
        T("interleaved_depths", "push 3, pop 2, push 2, drain", "{ let s = Stack::new(4); for v in 1..=3 { s.push(v).unwrap(); } let a = (s.pop(), s.pop()); s.push(8).unwrap(); s.push(9).unwrap(); let b: Vec<u64> = std::iter::from_fn(|| s.pop()).collect(); (a, b) }", "((Some(3), Some(2)), vec![9, 8, 1])"),
        """
        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(4404);
            for _ in 0..300 {
                let cap = rng.below(6);
                let s = Stack::new(cap);
                let mut model: Vec<u64> = Vec::new();
                let mut log = Vec::new();
                for _ in 0..rng.below(40) {
                    if rng.bool() {
                        let v = rng.below(100) as u64;
                        let want = if model.len() == cap { Err(v) } else { model.push(v); Ok(()) };
                        log.push(format!("push {v}"));
                        check!(format!("cap {cap}: {}", log.join(", ")), s.push(v), want);
                    } else {
                        log.push("pop".to_string());
                        check!(format!("cap {cap}: {}", log.join(", ")), s.pop(), model.pop());
                    }
                }
            }
        }

        // Every thread pushes its own distinct values and pops as many; whoever pops what, no value may be lost,
        // duplicated or invented. A tiny pool makes slots recycle constantly, which is what provokes ABA.
        #[test]
        fn small_pool_four_threads_conserve_every_value() {
            for round in 0..20u64 {
                let s = std::sync::Arc::new(Stack::new(5));
                let per = 5_000u64;
                let hs: Vec<_> = (0..4u64)
                    .map(|t| {
                        let s = s.clone();
                        std::thread::spawn(move || {
                            let mut got = Vec::new();
                            for i in 0..per {
                                let mut v = t * per + i + 1;
                                while let Err(back) = s.push(v) {
                                    v = back;
                                    std::thread::yield_now();
                                }
                                loop {
                                    if let Some(x) = s.pop() {
                                        got.push(x);
                                        break;
                                    }
                                    std::thread::yield_now();
                                }
                            }
                            got
                        })
                    })
                    .collect();
                let mut all: Vec<u64> = hs.into_iter().flat_map(|h| h.join().unwrap()).collect();
                while let Some(x) = s.pop() {
                    all.push(x);
                }
                all.sort_unstable();
                let want: Vec<u64> = (1..=4 * per).collect();
                check!(format!("round {round}: 4 threads x {per} values through a pool of 5"), all == want, true);
            }
        }

        #[test]
        fn producers_and_consumers_split() {
            let s = std::sync::Arc::new(Stack::new(64));
            let done = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let producers: Vec<_> = (0..3u64)
                .map(|t| {
                    let (s, done) = (s.clone(), done.clone());
                    std::thread::spawn(move || {
                        for i in 1..=30_000u64 {
                            let mut v = t * 1_000_000 + i;
                            while let Err(back) = s.push(v) {
                                v = back;
                                std::thread::yield_now();
                            }
                        }
                        done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    })
                })
                .collect();
            let consumers: Vec<_> = (0..3)
                .map(|_| {
                    let (s, done) = (s.clone(), done.clone());
                    std::thread::spawn(move || {
                        let mut sum = 0u64;
                        let mut n = 0u64;
                        loop {
                            match s.pop() {
                                Some(v) => {
                                    sum += v;
                                    n += 1;
                                }
                                None if done.load(std::sync::atomic::Ordering::SeqCst) == 3 => match s.pop() {
                                    Some(v) => {
                                        sum += v;
                                        n += 1;
                                    }
                                    None => break,
                                },
                                None => std::thread::yield_now(),
                            }
                        }
                        (n, sum)
                    })
                })
                .collect();
            for p in producers {
                p.join().unwrap();
            }
            let (n, sum) = consumers.into_iter().map(|c| c.join().unwrap()).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            let want: u64 = (0..3u64).map(|t| (1..=30_000u64).map(|i| t * 1_000_000 + i).sum::<u64>()).sum();
            check!("3 producers x 30000 into 3 consumers: count and sum", (n, sum), (90_000, want));
        }
        """,
    ],
    wrong=dict(
        no_version_tag=sub(ISTACK, "pack(tag.wrapping_add(1), after)", "pack(tag, after)")
        .replace("pack(tag.wrapping_add(1), index)", "pack(tag, index)")
        .replace("let after = next[index as usize].load(Ordering::Relaxed);", "let after = next[index as usize].load(Ordering::Relaxed);\n        std::thread::yield_now();"),
        stack_ignores_capacity=sub(ISTACK, "let Some(slot) = pop_slot(&self.free, &self.next) else {\n            return Err(value);\n        };", "let Some(slot) = pop_slot(&self.free, &self.next) else {\n            return Ok(());\n        };"),
    ),
    hints=[("approach", "Slot `i` is a node: `values[i]` and `next[i]`. A list head is one `AtomicU64` = `(tag << 32) | index`, with index `NIL` for empty. `push` = take a slot off the free list, store the value, push the slot on `top`; `pop` is the reverse."),
           ("rust", "pop loop: `load` head, split it, read `next[index]`, `compare_exchange_weak(cur, pack(tag + 1, next), Acquire, Acquire)`; on `Err(seen)` retry with `seen`. push loop: write `next[index] = head_index` first, then CAS to `pack(tag + 1, index)` with `Release`."),
           ("edge", "Use `wrapping_add` on the tag. A reader can see a stale `next[index]`; that is harmless exactly because a stale read can only pair with a CAS that will then fail on the tag.")],
    notes=("Without the tag, pop reads `A.next = B`, is preempted, others pop A, pop B, push A: the head is `A` again, so the CAS succeeds and installs `B`, which is no longer on the list. The tag makes the head word different every time even if the index repeats (it can only repeat after 2^32 operations in between, which is the accepted residual risk of tagged pointers). Using a fixed pool of slots instead of heap nodes avoids the other half of lock-free stacks, memory reclamation: a slot is never freed, only recycled, so a stale reader never touches freed memory. Values are `AtomicU64`s so a racing stale read is defined behaviour.", "O(1) amortised, lock-free", "O(capacity)"),
    follow_up="1) Is this stack wait-free? Why can a thread starve? 2) Replace the pool with `Box`ed nodes: what must you do about freeing a popped node while another thread may still read its `next`? Compare hazard pointers, epoch-based reclamation (crossbeam-epoch) and the tag-on-pointer approach. 3) Why does the tag live in the same word as the index, and what would a double-width CAS (`cmpxchg16b`) allow? 4) Could `pop` use `Relaxed` for the `next` load and why do we still need `Acquire` on the CAS? 5) Why LIFO stacks make poor work queues, and what do you use instead?",
    related=["C3", "F6", "D14"],
))

# ---------------------------------------------------------------------------------------------------------------
MPMC = """
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Keeps `head` and `tail` on different cache lines so producers and consumers do not invalidate each other.
#[repr(align(64))]
struct Padded<T>(T);

struct Slot<T> {
    seq: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

/// Bounded multi-producer multi-consumer FIFO (Dmitry Vyukov's design): every slot carries a sequence number that
/// says whose turn it is. For slot `i` at lap `k`: `seq == i + k*cap` means ready for a push, `seq == i + k*cap + 1`
/// means ready for a pop.
pub struct Queue<T> {
    slots: Box<[Slot<T>]>,
    mask: usize,
    head: Padded<AtomicUsize>,
    tail: Padded<AtomicUsize>,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Send> Sync for Queue<T> {}

impl<T> Queue<T> {
    /// Capacity is rounded up to a power of two, at least 2.
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(2).next_power_of_two();
        let slots = (0..cap).map(|i| Slot { seq: AtomicUsize::new(i), value: UnsafeCell::new(MaybeUninit::uninit()) }).collect();
        Queue { slots, mask: cap - 1, head: Padded(AtomicUsize::new(0)), tail: Padded(AtomicUsize::new(0)) }
    }

    pub fn capacity(&self) -> usize {
        self.mask + 1
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let mut pos = self.tail.0.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[pos & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            let diff = seq as isize - pos as isize;
            if diff == 0 {
                match self.tail.0.compare_exchange_weak(pos, pos.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => {
                        unsafe { (*slot.value.get()).write(value) };
                        slot.seq.store(pos.wrapping_add(1), Ordering::Release);
                        return Ok(());
                    }
                    Err(now) => pos = now,
                }
            } else if diff < 0 {
                return Err(value); // the slot still holds last lap's value: full
            } else {
                pos = self.tail.0.load(Ordering::Relaxed); // someone else claimed it: reload
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        let mut pos = self.head.0.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[pos & self.mask];
            let seq = slot.seq.load(Ordering::Acquire);
            let diff = seq as isize - pos.wrapping_add(1) as isize;
            if diff == 0 {
                match self.head.0.compare_exchange_weak(pos, pos.wrapping_add(1), Ordering::Relaxed, Ordering::Relaxed) {
                    Ok(_) => {
                        let value = unsafe { (*slot.value.get()).assume_init_read() };
                        slot.seq.store(pos.wrapping_add(self.mask + 1), Ordering::Release);
                        return Some(value);
                    }
                    Err(now) => pos = now,
                }
            } else if diff < 0 {
                return None; // not written yet: empty
            } else {
                pos = self.head.0.load(Ordering::Relaxed);
            }
        }
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}
"""

MPMC_STARTER = """
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Slot<T> {
    /// Whose turn this slot is. For slot `i`: `i + lap * capacity` = free for the push of that lap,
    /// `i + lap * capacity + 1` = full, ready for the pop of that lap.
    seq: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

pub struct Queue<T> {
    slots: Box<[Slot<T>]>,
    mask: usize,
    head: AtomicUsize, // next position to pop
    tail: AtomicUsize, // next position to push
}

// Given loose so the tests compile: what must `T` be for a queue shared by many threads?
unsafe impl<T> Send for Queue<T> {}
unsafe impl<T> Sync for Queue<T> {}

impl<T> Queue<T> {
    /// Capacity is rounded up to a power of two, and is at least 2.
    pub fn new(capacity: usize) -> Self {
        todo!()
    }

    pub fn capacity(&self) -> usize {
        todo!()
    }

    /// Gives the value back when the queue is full. Never blocks.
    pub fn push(&self, value: T) -> Result<(), T> {
        todo!()
    }

    /// `None` when empty. Never blocks.
    pub fn pop(&self) -> Option<T> {
        todo!()
    }
}

// TODO: values still queued when the queue is dropped must be dropped exactly once.
"""

MPMC_WRONG = """
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Slot<T> {
    value: UnsafeCell<Option<T>>,
}

/// Claims a position first, fills it afterwards: a consumer can pop a claimed-but-empty slot.
pub struct Queue<T> {
    slots: Box<[Slot<T>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Send> Sync for Queue<T> {}

impl<T> Queue<T> {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(2).next_power_of_two();
        Queue { slots: (0..cap).map(|_| Slot { value: UnsafeCell::new(None) }).collect(), head: AtomicUsize::new(0), tail: AtomicUsize::new(0) }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        loop {
            let tail = self.tail.load(Ordering::Acquire);
            let head = self.head.load(Ordering::Acquire);
            if tail - head >= self.slots.len() {
                return Err(value);
            }
            if self.tail.compare_exchange(tail, tail + 1, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                std::thread::yield_now();
                unsafe { *self.slots[tail % self.slots.len()].value.get() = Some(value) };
                return Ok(());
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            let head = self.head.load(Ordering::Acquire);
            let tail = self.tail.load(Ordering::Acquire);
            if head >= tail {
                return None;
            }
            if self.head.compare_exchange(head, head + 1, Ordering::AcqRel, Ordering::Relaxed).is_ok() {
                return unsafe { (*self.slots[head % self.slots.len()].value.get()).take() };
            }
        }
    }
}
"""

P.append(dict(
    slug="bounded-mpmc", title="A bounded MPMC queue with sequence numbers", level="hard", stage="build-it",
    tags=["MPMC", "Vyukov queue", "sequence numbers", "false sharing", "cache padding"],
    teaches=["Per-slot sequence numbers turn 'claim a position' and 'publish the value' into two separate, safe steps.",
             "CAS on the position claims a slot; the slot's own Release store hands it to the other side.",
             "`#[repr(align(64))]` padding so head and tail do not share a cache line."],
    statement="""
        Write `Queue<T>`: a bounded FIFO that any number of threads may `push` to and `pop` from at the same time, through
        `&self`, with no lock. `push` gives the value back (`Err`) when full and `pop` returns `None` when empty; neither blocks.
        Capacity is rounded up to a power of two (at least 2) and reported by `capacity()`.

        The trap is the gap between *claiming* a position and *writing* the value: a consumer must never read a slot
        whose producer has claimed it but not finished writing. Give every slot a sequence number saying whose turn it is:
        for slot `i` and lap `k` it holds `i + k * cap` when a push may fill it and `i + k * cap + 1` when a pop may take it.
        Values left in the queue when it is dropped must be dropped exactly once.
    """,
    starter=MPMC_STARTER,
    solution=MPMC,
    visible=[
        T("fifo_order", "push 1, 2, 3, pop three", "{ let q = Queue::new(4); q.push(1).unwrap(); q.push(2).unwrap(); q.push(3).unwrap(); (q.pop(), q.pop(), q.pop(), q.pop()) }", "(Some(1), Some(2), Some(3), None)"),
        T("capacity_rounds_up", "5 -> 8, 1 -> 2, 8 -> 8", "(Queue::<u8>::new(5).capacity(), Queue::<u8>::new(1).capacity(), Queue::<u8>::new(8).capacity())", "(8, 2, 8)"),
        T("full_gives_value_back", "capacity 2, third push", "{ let q = Queue::new(2); q.push('a').unwrap(); q.push('b').unwrap(); q.push('c') }", "Err('c')"),
        T("pop_makes_room", "fill, pop, push", "{ let q = Queue::new(2); q.push(1).unwrap(); q.push(2).unwrap(); let a = q.pop(); let ok = q.push(3).is_ok(); (a, ok, q.pop(), q.pop()) }", "(Some(1), true, Some(2), Some(3))"),
        T("empty_pop", "new queue", "Queue::<i32>::new(4).pop()", "None"),
    ],
    hidden=[
        T("many_laps_single_thread", "capacity 4, 10000 push/pop pairs", "{ let q = Queue::new(4); let mut ok = true; for i in 0..10_000u32 { ok &= q.push(i).is_ok(); ok &= q.push(i + 1).is_ok(); ok &= q.pop() == Some(i); ok &= q.pop() == Some(i + 1); } (ok, q.pop()) }", "(true, None)"),
        T("zero_and_one_capacity", "capacity 0 and 1 both give 2 usable slots", "{ let q = Queue::<u8>::new(0); let r = Queue::<u8>::new(1); (q.capacity(), r.capacity(), q.push(1).is_ok(), q.push(2).is_ok(), q.push(3).is_err()) }", "(2, 2, true, true, true)"),
        T("full_then_empty_cycles", "fill to the brim each lap", "{ let q = Queue::new(4); let mut ok = true; for lap in 0..50u32 { for i in 0..4 { ok &= q.push(lap * 4 + i).is_ok(); } ok &= q.push(0).is_err(); for i in 0..4 { ok &= q.pop() == Some(lap * 4 + i); } ok &= q.pop().is_none(); } ok }", "true"),
        T("string_values", "heap values move through", "{ let q = Queue::new(2); q.push(\"a\".to_string()).unwrap(); q.push(\"b\".to_string()).unwrap(); (q.pop(), q.pop()) }", "(Some(\"a\".to_string()), Some(\"b\".to_string()))"),
        """
        #[test]
        fn leftovers_dropped_exactly_once() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::Arc;
            struct D(Arc<AtomicUsize>);
            impl Drop for D {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::SeqCst);
                }
            }
            let n = Arc::new(AtomicUsize::new(0));
            let q = Queue::new(8);
            for _ in 0..6 {
                q.push(D(n.clone())).ok().unwrap();
            }
            drop(q.pop());
            check!("one popped and dropped", n.load(Ordering::SeqCst), 1);
            let rejected = {
                let q2 = Queue::new(2);
                q2.push(D(n.clone())).ok().unwrap();
                q2.push(D(n.clone())).ok().unwrap();
                q2.push(D(n.clone())).err().unwrap()
            }; // q2 dropped with its two values; `rejected` is still alive
            check!("q2 dropped: its two values are gone, the rejected one is not", n.load(Ordering::SeqCst), 3);
            drop(rejected);
            drop(q);
            check!("everything dropped", n.load(Ordering::SeqCst), 9);
        }

        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(4505);
            for _ in 0..300 {
                let cap = rng.below(9) + 1;
                let q = Queue::new(cap);
                let real = cap.max(2).next_power_of_two();
                let mut model = std::collections::VecDeque::new();
                let mut log = Vec::new();
                for _ in 0..rng.below(60) {
                    if rng.bool() {
                        let v = rng.below(1000) as u32;
                        let want = if model.len() == real { Err(v) } else { model.push_back(v); Ok(()) };
                        log.push(format!("push {v}"));
                        check!(format!("cap {cap}: {}", log.join(", ")), q.push(v), want);
                    } else {
                        log.push("pop".to_string());
                        check!(format!("cap {cap}: {}", log.join(", ")), q.pop(), model.pop_front());
                    }
                }
            }
        }

        // 4 producers and 4 consumers. Every value must come out exactly once, and the values of one producer must
        // reach any single consumer in the order that producer pushed them.
        #[test]
        fn four_by_four_exactly_once_and_per_producer_order() {
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::Arc;
            let per = 50_000u64;
            let q = Arc::new(Queue::new(64));
            let finished = Arc::new(AtomicUsize::new(0));
            let producers: Vec<_> = (0..4u64)
                .map(|p| {
                    let (q, finished) = (q.clone(), finished.clone());
                    std::thread::spawn(move || {
                        for i in 0..per {
                            let mut v = (p << 32) | i;
                            while let Err(back) = q.push(v) {
                                v = back;
                                std::hint::spin_loop();
                            }
                        }
                        finished.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .collect();
            let consumers: Vec<_> = (0..4)
                .map(|_| {
                    let (q, finished) = (q.clone(), finished.clone());
                    std::thread::spawn(move || {
                        let mut got: Vec<u64> = Vec::new();
                        loop {
                            match q.pop() {
                                Some(v) => got.push(v),
                                None if finished.load(Ordering::SeqCst) == 4 => match q.pop() {
                                    Some(v) => got.push(v),
                                    None => break,
                                },
                                None => std::hint::spin_loop(),
                            }
                        }
                        got
                    })
                })
                .collect();
            for p in producers {
                p.join().unwrap();
            }
            let per_consumer: Vec<Vec<u64>> = consumers.into_iter().map(|c| c.join().unwrap()).collect();
            let mut ordered = true;
            for got in &per_consumer {
                let mut last = [None::<u64>; 4];
                for &v in got {
                    let (p, i) = ((v >> 32) as usize, v & 0xffff_ffff);
                    if let Some(prev) = last[p] {
                        ordered &= i > prev;
                    }
                    last[p] = Some(i);
                }
            }
            let mut seen = vec![false; 4 * per as usize];
            let mut dup_or_bad = 0;
            let mut total = 0usize;
            for &v in per_consumer.iter().flatten() {
                let idx = ((v >> 32) * per + (v & 0xffff_ffff)) as usize;
                if idx >= seen.len() || seen[idx] {
                    dup_or_bad += 1;
                } else {
                    seen[idx] = true;
                }
                total += 1;
            }
            check!("4 producers x 50000, 4 consumers: total, duplicates/invented, per-producer order", (total, dup_or_bad, ordered), (200_000, 0, true));
        }

        #[test]
        fn tiny_queue_heavy_contention() {
            use std::sync::Arc;
            let q = Arc::new(Queue::new(2));
            let hs: Vec<_> = (0..6u64)
                .map(|t| {
                    let q = q.clone();
                    std::thread::spawn(move || {
                        let mut sum = 0u64;
                        for i in 1..=10_000u64 {
                            let mut v = t * 100_000 + i;
                            while let Err(back) = q.push(v) {
                                v = back;
                                // make room by consuming: every thread is both producer and consumer
                                if let Some(x) = q.pop() {
                                    sum += x;
                                }
                            }
                        }
                        sum
                    })
                })
                .collect();
            let mut total: u64 = hs.into_iter().map(|h| h.join().unwrap()).sum();
            while let Some(x) = q.pop() {
                total += x;
            }
            let want: u64 = (0..6u64).map(|t| (1..=10_000u64).map(|i| t * 100_000 + i).sum::<u64>()).sum();
            check!("6 threads push into a queue of 2, draining it themselves", total, want);
        }
        """,
    ],
    wrong=dict(
        claim_then_fill=MPMC_WRONG,
        no_drop=sub(MPMC, "while self.pop().is_some() {}", ""),
        capacity_not_rounded=sub(MPMC, "let cap = capacity.max(2).next_power_of_two();", "let cap = capacity.max(2);"),
    ),
    hints=[("approach", "Slot `i` starts with `seq = i`. A push at position `pos` may use slot `pos & mask` when its `seq == pos`; after writing, store `seq = pos + 1`. A pop at `pos` needs `seq == pos + 1`; after reading, store `seq = pos + cap` so the slot is ready for the next lap's push."),
           ("rust", "Loop: load the slot's `seq` (Acquire), compute `diff = seq as isize - expected as isize`. `diff == 0`: try to CAS the position (`compare_exchange_weak`, Relaxed is enough: the slot's `seq` carries the ordering); success means the slot is yours. `diff < 0`: full (push) or empty (pop). `diff > 0`: another thread moved on, reload the position."),
           ("edge", "Use wrapping arithmetic on positions. Put `head` and `tail` in `#[repr(align(64))]` wrappers. `Drop` can simply pop until empty.")],
    notes=("The position CAS decides *who* owns a slot; the slot's sequence number (written with Release, read with Acquire) decides *when* the other side may touch it, which closes the claimed-but-not-written window that a naive head/tail design has. A full queue is detected without reading the other index: the slot still carries last lap's sequence, so `seq < pos`. The structure is lock-free (a stalled thread between claim and publish blocks only that slot's consumer, not the whole queue, but strict lock-freedom is debatable there) and is the basis of crossbeam's `ArrayQueue`.", "O(1) per operation", "O(capacity)"),
    follow_up="1) Why is this not strictly lock-free (a thread preempted between its CAS and its `seq` store)? What does that do to the consumer at that position? 2) Add `push_blocking` and `pop_blocking` with a parking strategy: spin, yield, then park. What is the lost-wakeup race? 3) Compare with `crossbeam::queue::ArrayQueue` and `std::sync::mpsc::sync_channel`. 4) Why do `head` and `tail` need separate cache lines, and how would you measure it (perf c2c)? 5) Make it unbounded (a linked list of blocks, like crossbeam's `SegQueue`): what is the reclamation story? 6) How would you add a `len()` that is right under concurrency, and why is it only ever an estimate?",
    related=["C2", "F6", "S5"],
))

STAGES = [("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("c3-atomics-lock-free", "C3", "Atomics & lock-free", "C", "sde3", 3,
                    "Memory ordering and lock-free structures by building them: counters, a once-cell, a spinlock, queues, a stack and an Arc.",
                    STAGES, P)
    print("C3", n)
