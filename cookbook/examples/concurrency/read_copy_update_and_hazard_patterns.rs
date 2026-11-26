//! RECIPE 903 (concurrency): read-mostly sharing patterns: RCU with grace periods, left-right, and cached ArcSwap-style pointers.
//! LEVEL: advanced
//! Use in: config/routing tables read on every request and updated rarely, in-memory indexes, feature-flag snapshots, DNS/route caches.   Run: cargo run --release --example read_copy_update_and_hazard_patterns
//! COMMON USES: the Linux kernel's RCU (routing tables, dentry cache), Gjengset's `left-right`/`evmap` crates, the `arc-swap` crate for hot config, Envoy/Nginx-style config reload.
//! Course: Paul McKenney, "Is Parallel Programming Hard, And, If So, What Can You Do About It?" (RCU chapter); Jon Gjengset, "Left-Right" talks and the `left-right` crate docs.
#![allow(dead_code)]
//!
//! THE PROBLEM: many readers want a consistent multi-word snapshot with NO locks and no cache-line ping-pong; a few writers can be slow.
//! All three designs below give readers a wait-free-ish fast path; they differ in what the WRITER has to do to know it is safe to reuse memory.
//!   A. `Rcu`       : two slots + a `cur` index. A writer fills the idle slot, publishes by flipping `cur`, then waits out a GRACE PERIOD: every
//!                    reader keeps a private counter that is odd while inside a read-side critical section; the writer snapshots the counters
//!                    after publishing and waits until each odd one has changed. Readers that enter later necessarily see the new `cur`.
//!                    Only after the grace period may the old slot be overwritten. Skip the wait and a slow reader sees a TORN snapshot: shown
//!                    deterministically below with a reader paused mid-read.
//!                    ORDERING: reader "enter" (RMW on its counter) then load of `cur` need a store-load barrier => SeqCst on both, matching the
//!                    writer's SeqCst store of `cur` then SeqCst loads of the counters. Acquire/Release alone would let both sides miss each other
//!                    (the store-buffering litmus test, see `memory_ordering_litmus`).
//!   B. `LeftRight` : two FULL copies of the data. Writes are an operation (`bump(d)`): apply to the inactive copy, flip `read`, wait for readers of the old
//!                    copy to drain, apply the SAME op to the old copy. Both copies are identical between writes, so writes can be incremental
//!                    instead of whole-value replacement (Gjengset's `left-right`/`evmap`). Writers pay 2x work and one grace wait; readers pay one counter bump.
//!   C. `Cached Arc`: `Mutex<Arc<T>>` is the baseline (every read locks and clones the Arc: two atomic RMWs on a shared line). The optimisation used by
//!                    `arc-swap::Cache`: readers keep their own `Arc` clone and one `AtomicU64` version; the fast path is a single load and compare, the
//!                    lock is taken only when the version moved. The real `ArcSwap` goes further (debt slots + `AtomicPtr`, unsafe) to avoid even that.
//!   D. `PtrRcu`     : the textbook form: `AtomicPtr::swap` a new boxed object in, wait a grace period, `Box::from_raw` the old one. The only
//!                    unsafe code in the file; drop counting shows every object is freed exactly once. Passing tests are not a
//!                    memory-safety proof; a scaled-down copy (25 bumps per writer) ran clean under Miri, seeds 0-3. ThreadSanitizer was not run.
//! WRITER PROGRESS: `synchronize` waits only for readers that were inside when it looked, so overlapping readers cannot starve the writer.
//! WHAT ABOUT HAZARD POINTERS / EPOCHS? Same job (when may I free?) with different bookkeeping: RCU/epoch = "wait for everyone who started
//! before the swap"; hazard pointers = "check whether anybody announced this exact pointer" (see `epoch_hazard_pointer_reclamation`).
//! VERIFICATION: 6 readers + 2 writers on each design. The data is 8 words that must all be equal (`bump(d)` adds d to every word), so a torn
//! read is a mismatch; each reader also checks that values never go backwards; writers do exactly 3000 + 3000 bumps of 1 and 2, so the final value is exactly 9000 on every design.
//! Read: McKenney, "Is Parallel Programming Hard" (title only, not re-read). https://docs.rs/left-right/latest/left_right/ and https://docs.rs/arc-swap/latest/arc_swap/
//! Read: https://marabos.nl/atomics/ (Mara Bos, "Rust Atomics and Locks")
//!
//! TRY NEXT:
//!   - [S] In `Rcu::read`, change the counter bump from `SeqCst` to `AcqRel` and the `cur` load to `Acquire`. Predict whether the torn-read test or the stress test fails first (or neither on your CPU), then explain why a pass proves nothing.
//!   - [S] Make `synchronize` wait for ALL readers to be even at once instead of only the ones odd in the snapshot. What new liveness problem appears with a reader that loops continuously?
//!   - [M] Give `LeftRight` a second op `Reset` and make writes take `&[Op]` batches (one flip per batch). Measure writer throughput vs batch size.
//!   - [M] Replace the per-reader counter array with one global epoch (`AtomicU64`) plus per-reader "last seen epoch" (QSBR style). What must a reader promise not to do between quiescent states?
//!   - [A] PROJECT: serve the routing table of `api_gateway_service_discovery` through the best pattern here, with a writer applying registry updates and 8 request threads reading; add the checker from `concurrent_hashmap_designs` to prove reads are linearizable.

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering::*};
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const W: usize = 8;
const READERS: usize = 6;

#[repr(align(64))]
struct Pad<T>(T);
type Words = [AtomicU64; W];
fn words(v: u64) -> Words {
    std::array::from_fn(|_| AtomicU64::new(v))
}
/// (consistent?, value). A torn snapshot has words that disagree.
fn check(w: &Words) -> (bool, u64) {
    let first = w[0].load(Relaxed);
    (w.iter().all(|x| x.load(Relaxed) == first), first)
}

/// Spin, then yield: grace periods must not burn a core when readers are descheduled.
fn wait_until(mut done: impl FnMut() -> bool) {
    let mut n = 0u32;
    while !done() {
        n += 1;
        if n > 100 {
            thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }
}

// ---------------------------------------------------------------- shared reader bookkeeping
struct Epochs(Vec<Pad<AtomicU64>>);
impl Epochs {
    fn new(n: usize) -> Self {
        Epochs((0..n).map(|_| Pad(AtomicU64::new(0))).collect())
    }
    fn enter(&self, r: usize) {
        self.0[r].0.fetch_add(1, SeqCst); // becomes odd
    }
    fn exit(&self, r: usize) {
        self.0[r].0.fetch_add(1, Release); // becomes even: our reads happen-before the writer's next overwrite
    }
    /// Grace period: wait for every reader that was inside a critical section when we looked.
    fn synchronize(&self) {
        let snap = self.snapshot();
        self.wait_for(&snap);
    }
    fn snapshot(&self) -> Vec<u64> {
        self.0.iter().map(|c| c.0.load(SeqCst)).collect()
    }
    fn wait_for(&self, snap: &[u64]) {
        for (i, &v) in snap.iter().enumerate() {
            if v & 1 == 1 {
                wait_until(|| self.0[i].0.load(Acquire) != v);
            }
        }
    }
}

// ---------------------------------------------------------------- A. RCU with two slots
struct Rcu {
    slots: [Words; 2],
    cur: AtomicUsize,
    epochs: Epochs,
    writer: Mutex<()>,
}
struct RcuGuard<'a> {
    rcu: &'a Rcu,
    r: usize,
    idx: usize,
}
impl Rcu {
    fn new(readers: usize) -> Self {
        Rcu {
            slots: [words(0), words(0)],
            cur: AtomicUsize::new(0),
            epochs: Epochs::new(readers),
            writer: Mutex::new(()),
        }
    }
    fn read_lock(&self, r: usize) -> RcuGuard<'_> {
        self.epochs.enter(r);
        let idx = self.cur.load(SeqCst);
        RcuGuard { rcu: self, r, idx }
    }
    /// `wait = false` is the BROKEN variant used by the torn-read demonstration.
    fn bump(&self, d: u64, wait: bool) {
        let _g = self.writer.lock().unwrap();
        let old = self.cur.load(Relaxed);
        let new = 1 - old;
        let v = self.slots[old][0].load(Relaxed) + d;
        for w in &self.slots[new] {
            w.store(v, Relaxed); // nobody can be reading `new`: the previous grace period drained it
        }
        self.cur.store(new, SeqCst); // publish
        if wait {
            self.epochs.synchronize(); // old slot is reusable only after this returns
        }
    }
}
impl RcuGuard<'_> {
    fn slot(&self) -> &Words {
        &self.rcu.slots[self.idx]
    }
}
impl Drop for RcuGuard<'_> {
    fn drop(&mut self) {
        self.rcu.epochs.exit(self.r);
    }
}

// ---------------------------------------------------------------- B. left-right
struct LeftRight {
    copies: [Words; 2],
    read: AtomicUsize,
    epochs: Epochs,
    writer: Mutex<()>,
}
impl LeftRight {
    fn new(readers: usize) -> Self {
        LeftRight {
            copies: [words(0), words(0)],
            read: AtomicUsize::new(0),
            epochs: Epochs::new(readers),
            writer: Mutex::new(()),
        }
    }
    fn read(&self, r: usize) -> (bool, u64) {
        self.epochs.enter(r);
        let out = check(&self.copies[self.read.load(SeqCst)]);
        self.epochs.exit(r);
        out
    }
    fn apply(w: &Words, d: u64) {
        for x in w {
            x.store(x.load(Relaxed) + d, Relaxed);
        }
    }
    fn bump(&self, d: u64) {
        let _g = self.writer.lock().unwrap();
        let live = self.read.load(Relaxed);
        Self::apply(&self.copies[1 - live], d); // 1. write the copy nobody reads
        self.read.store(1 - live, SeqCst); //       2. swap
        self.epochs.synchronize(); //                3. wait for readers still on the old copy
        Self::apply(&self.copies[live], d); //       4. replay the op there: the copies are equal again
    }
}

// ---------------------------------------------------------------- C. Arc cells
type Block = [u64; W];
struct MutexArc(Mutex<Arc<Block>>);
struct CachedArc {
    cur: Mutex<Arc<Block>>,
    version: AtomicU64,
}
struct ArcCache<'a> {
    cell: &'a CachedArc,
    ver: u64,
    arc: Arc<Block>,
    refreshes: u64,
}
impl CachedArc {
    fn new() -> Self {
        CachedArc {
            cur: Mutex::new(Arc::new([0; W])),
            version: AtomicU64::new(0),
        }
    }
    fn bump(&self, d: u64) {
        let mut g = self.cur.lock().unwrap();
        let v = g[0] + d;
        *g = Arc::new([v; W]);
        self.version.fetch_add(1, Release); // after the swap: a reader that sees the new version also finds the new Arc
    }
    fn cache(&self) -> ArcCache<'_> {
        let g = self.cur.lock().unwrap();
        ArcCache {
            cell: self,
            ver: self.version.load(Acquire),
            arc: g.clone(),
            refreshes: 0,
        }
    }
}
impl ArcCache<'_> {
    fn load(&mut self) -> &Block {
        if self.cell.version.load(Acquire) != self.ver {
            // slow path: take the lock, clone, remember the version we cloned under the lock
            let g = self.cell.cur.lock().unwrap();
            self.ver = self.cell.version.load(Acquire);
            self.arc = g.clone();
            self.refreshes += 1;
        }
        &self.arc
    }
}

// ---------------------------------------------------------------- D. pointer-swap RCU with real reclamation
/// A heap object that proves two things: it is internally consistent (all words equal) and it is dropped exactly once.
struct Versioned {
    data: Block,
    freed: Arc<AtomicU64>,
}
impl Drop for Versioned {
    fn drop(&mut self) {
        self.data = [u64::MAX; W]; // poison: a use-after-free that still reads this memory is likely to trip the checksum
        self.freed.fetch_add(1, SeqCst);
    }
}
/// The textbook RCU update: allocate, publish the pointer, wait a grace period, free the old object.
struct PtrRcu {
    cur: AtomicPtr<Versioned>,
    epochs: Epochs,
    writer: Mutex<()>,
    freed: Arc<AtomicU64>,
}
impl PtrRcu {
    fn new(readers: usize) -> Self {
        let freed = Arc::new(AtomicU64::new(0));
        let first = Box::new(Versioned {
            data: [0; W],
            freed: freed.clone(),
        });
        PtrRcu {
            cur: AtomicPtr::new(Box::into_raw(first)),
            epochs: Epochs::new(readers),
            writer: Mutex::new(()),
            freed,
        }
    }
    /// The reference cannot escape: `f` is `for<'a>`, so `R` cannot borrow from the object. That is the whole safety story of the read side.
    /// Contract: slot `r` is used by one thread at a time and `f` does not re-enter `with` on the same slot (the parity trick would break).
    fn with<R>(&self, r: usize, f: impl FnOnce(&Block) -> R) -> R {
        self.epochs.enter(r);
        let p = self.cur.load(SeqCst);
        // SAFETY: `p` was published and not yet freed: a writer frees it only after `synchronize`, which waits for this slot to
        // leave the critical section that began before our load. The contract above keeps the slot's parity meaningful.
        let out = f(unsafe { &(*p).data });
        self.epochs.exit(r);
        out
    }
    fn bump(&self, d: u64) {
        let _g = self.writer.lock().unwrap();
        // SAFETY: only the lock holder frees objects, so the current one is alive while we hold `writer`.
        let v = unsafe { (*self.cur.load(Relaxed)).data[0] } + d;
        let new = Box::into_raw(Box::new(Versioned {
            data: [v; W],
            freed: self.freed.clone(),
        }));
        let old = self.cur.swap(new, SeqCst);
        self.epochs.synchronize();
        // SAFETY: `old` is unreachable for new readers and every earlier reader has left; it came from `Box::into_raw`.
        drop(unsafe { Box::from_raw(old) });
    }
}
impl Drop for PtrRcu {
    fn drop(&mut self) {
        // SAFETY: `&mut self` means no reader or writer is left.
        drop(unsafe { Box::from_raw(*self.cur.get_mut()) });
    }
}
impl Shared for PtrRcu {
    fn name(&self) -> &'static str {
        "AtomicPtr RCU (frees old)"
    }
    fn bump(&self, d: u64) {
        PtrRcu::bump(self, d)
    }
    fn reader<'a>(&'a self, r: usize) -> Box<dyn FnMut() -> (bool, u64) + 'a> {
        Box::new(move || self.with(r, |b| (b.iter().all(|&x| x == b[0]), b[0])))
    }
}

/// Reclamation is exact: nothing leaks, nothing is freed twice, nothing is freed early.
fn reclamation_test() {
    let rcu = PtrRcu::new(READERS);
    let freed = rcu.freed.clone();
    stress(&rcu);
    assert_eq!(
        freed.load(SeqCst),
        6000,
        "every replaced object is freed, once, by the time its bump returns"
    );
    drop(rcu);
    assert_eq!(
        freed.load(SeqCst),
        6001,
        "and the last one when the cell is dropped"
    );
    println!(
        "  AtomicPtr RCU: 6000 objects retired and freed during the stress run, the last one on drop"
    );
}

// ---------------------------------------------------------------- writer progress under readers that are never all idle
/// A writer needs only the readers that were ALREADY inside to leave. Here two readers hand over: the next one enters before the
/// previous one leaves, so there is never an instant with no reader. The schedule is driven by one thread, so it is exact.
fn writer_progress_demo() {
    let e = Epochs::new(2);
    let busy = |e: &Epochs| e.0.iter().any(|c| c.0.load(SeqCst) & 1 == 1);
    e.enter(0);
    let writer_done = AtomicBool::new(false);
    thread::scope(|s| {
        // the snapshot is taken on this thread so we know exactly which readers the writer must wait for
        let snap = e.snapshot();
        assert_eq!(snap, [1, 0]);
        let (e, writer_done) = (&e, &writer_done);
        s.spawn(move || {
            e.wait_for(&snap);
            writer_done.store(true, SeqCst);
        });
        let mut inside = 0;
        let mut handovers = 0u32;
        // keep overlapping until the writer is through (and at least 100 times); an "all readers idle" writer would never get through
        while handovers < 100 || !writer_done.load(SeqCst) {
            e.enter(1 - inside);
            assert!(busy(e));
            e.exit(inside);
            assert!(busy(e), "there is never a moment without a reader");
            inside = 1 - inside;
            handovers += 1;
            thread::yield_now();
        }
        assert!(
            busy(e),
            "the writer finished while a reader was still inside: a LATER one, which it need not wait for"
        );
        e.exit(inside);
    });
    println!("  writer finished its grace period although some reader was inside at every instant");
}

// ---------------------------------------------------------------- uniform harness
trait Shared: Sync {
    fn name(&self) -> &'static str;
    fn bump(&self, d: u64);
    fn reader<'a>(&'a self, r: usize) -> Box<dyn FnMut() -> (bool, u64) + 'a>;
}
impl Shared for Rcu {
    fn name(&self) -> &'static str {
        "RCU (grace period)"
    }
    fn bump(&self, d: u64) {
        Rcu::bump(self, d, true)
    }
    fn reader<'a>(&'a self, r: usize) -> Box<dyn FnMut() -> (bool, u64) + 'a> {
        Box::new(move || check(self.read_lock(r).slot()))
    }
}
impl Shared for LeftRight {
    fn name(&self) -> &'static str {
        "left-right"
    }
    fn bump(&self, d: u64) {
        LeftRight::bump(self, d)
    }
    fn reader<'a>(&'a self, r: usize) -> Box<dyn FnMut() -> (bool, u64) + 'a> {
        Box::new(move || self.read(r))
    }
}
impl Shared for MutexArc {
    fn name(&self) -> &'static str {
        "Mutex<Arc> baseline"
    }
    fn bump(&self, d: u64) {
        let mut g = self.0.lock().unwrap();
        let v = g[0] + d;
        *g = Arc::new([v; W]);
    }
    fn reader<'a>(&'a self, _r: usize) -> Box<dyn FnMut() -> (bool, u64) + 'a> {
        Box::new(move || {
            let a = self.0.lock().unwrap().clone();
            (a.iter().all(|&x| x == a[0]), a[0])
        })
    }
}
impl Shared for CachedArc {
    fn name(&self) -> &'static str {
        "cached Arc (arc-swap style)"
    }
    fn bump(&self, d: u64) {
        CachedArc::bump(self, d)
    }
    fn reader<'a>(&'a self, _r: usize) -> Box<dyn FnMut() -> (bool, u64) + 'a> {
        let mut c = self.cache();
        Box::new(move || {
            let b = c.load();
            (b.iter().all(|&x| x == b[0]), b[0])
        })
    }
}

/// 6 readers + 2 writers; returns total reads.
fn stress(s: &dyn Shared) -> (u64, f64) {
    let writers_done = AtomicUsize::new(0);
    let t0 = Instant::now();
    let reads: u64 = thread::scope(|sc| {
        for d in [1u64, 2] {
            let writers_done = &writers_done;
            sc.spawn(move || {
                for _ in 0..3000 {
                    s.bump(d);
                }
                writers_done.fetch_add(1, SeqCst);
            });
        }
        let hs: Vec<_> = (0..READERS)
            .map(|r| {
                let writers_done = &writers_done;
                sc.spawn(move || {
                    let mut read = s.reader(r);
                    let (mut last, mut n) = (0u64, 0u64);
                    while writers_done.load(SeqCst) < 2 {
                        let (ok, v) = read();
                        assert!(ok, "{}: TORN READ", s.name());
                        assert!(
                            v >= last,
                            "{}: value went backwards {last} -> {v}",
                            s.name()
                        );
                        last = v;
                        n += 1;
                    }
                    n
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    });
    let mut read = s.reader(0);
    assert_eq!(read(), (true, 9000), "{}: lost update", s.name());
    (reads, t0.elapsed().as_secs_f64())
}

// ---------------------------------------------------------------- the grace period, made visible
fn grace_period_demo() {
    // BROKEN: no grace period. Reader pauses halfway through its slot; two writes later the slot is rewritten under it.
    let rcu = Rcu::new(1);
    let (paused_tx, paused_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let torn = thread::scope(|s| {
        let rcu = &rcu;
        let h = s.spawn(move || {
            let g = rcu.read_lock(0);
            let slot = g.slot();
            let first_half: Vec<u64> = slot[..W / 2].iter().map(|x| x.load(Relaxed)).collect();
            paused_tx.send(()).unwrap();
            go_rx.recv().unwrap();
            let second_half: Vec<u64> = slot[W / 2..].iter().map(|x| x.load(Relaxed)).collect();
            first_half
                .iter()
                .chain(&second_half)
                .any(|&x| x != first_half[0])
        });
        paused_rx.recv().unwrap();
        rcu.bump(1, false); // fills slot 1, publishes
        rcu.bump(1, false); // refills slot 0: the one the paused reader is still looking at
        go_tx.send(()).unwrap();
        h.join().unwrap()
    });
    assert!(
        torn,
        "without a grace period the paused reader must observe a torn snapshot"
    );
    println!("  no grace period: paused reader saw a TORN snapshot (as predicted)");

    // CORRECT: the writer's synchronize() cannot return while the reader is inside its critical section.
    let rcu = Rcu::new(1);
    let (paused_tx, paused_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let writer_finished = AtomicBool::new(false);
    thread::scope(|s| {
        let (rcu, writer_finished) = (&rcu, &writer_finished);
        let reader = s.spawn(move || {
            let g = rcu.read_lock(0);
            let slot = g.slot();
            let a = slot[0].load(Relaxed);
            paused_tx.send(()).unwrap();
            go_rx.recv().unwrap();
            let (ok, v) = check(slot);
            assert!(ok && v == a, "reader's snapshot changed under it");
        });
        paused_rx.recv().unwrap();
        let writer = s.spawn(move || {
            rcu.bump(1, true);
            writer_finished.store(true, SeqCst);
        });
        // first make sure the writer really got as far as publishing, otherwise "it has not finished" proves nothing
        wait_until(|| rcu.cur.load(SeqCst) == 1);
        // negative check: correct code can never fail it however slow the machine is (the sleep only gives a BROKEN synchronize time to return)
        thread::sleep(Duration::from_millis(40));
        assert!(
            !writer_finished.load(SeqCst),
            "writer must still be waiting for the grace period"
        );
        go_tx.send(()).unwrap();
        reader.join().unwrap();
        writer.join().unwrap();
    });
    assert!(writer_finished.load(SeqCst));
    println!(
        "  with grace period: writer stayed blocked while the reader was inside, then finished"
    );
}

fn main() {
    println!("== the grace period ==");
    grace_period_demo();
    writer_progress_demo();

    println!(
        "== 6 readers + 2 writers (3000 bumps each), consistency + monotonic + exact total 9000 =="
    );
    let designs: Vec<Box<dyn Shared>> = vec![
        Box::new(MutexArc(Mutex::new(Arc::new([0; W])))),
        Box::new(CachedArc::new()),
        Box::new(Rcu::new(READERS)),
        Box::new(LeftRight::new(READERS)),
    ];
    println!("  {:<30}{:>14}{:>12}", "design", "reads", "seconds");
    for d in &designs {
        let (reads, secs) = stress(&**d);
        println!("  {:<30}{:>14}{:>12.3}", d.name(), reads, secs);
    }
    println!("== pointer swap + reclamation ==");
    reclamation_test();
    // For contrast: RwLock<Arc> lets readers share the lock but still bounces its cache line.
    let rw = RwLock::new(Arc::new([0u64; W]));
    {
        let a = rw.read().unwrap().clone();
        assert_eq!(a[0], 0);
    }
    // arc-swap-style cache: after a few writes a reader refreshes at most once per write it noticed.
    let c = CachedArc::new();
    let mut cache = c.cache();
    for i in 1..=5 {
        c.bump(1);
        assert_eq!(cache.load()[0], i);
        assert_eq!(cache.load()[0], i); // second load: version unchanged => no lock
    }
    assert_eq!(cache.refreshes, 5);
    println!(
        "  cache: 10 loads, 5 refreshes (one per write), the other 5 were a single atomic load"
    );
    println!("all RCU / left-right checks passed");
}
