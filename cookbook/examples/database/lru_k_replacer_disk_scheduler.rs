//! RECIPE 701 (database): LRU-K replacer, a background disk scheduler with futures, and a concurrent buffer pool with page guards.
//! LEVEL: advanced
//! Use in: database buffer managers, page caches, any cache that must survive one-off sequential scans.   Run: cargo run --release --example lru_k_replacer_disk_scheduler
//! COMMON USES: the LRU-K policy of the CMU 15-445 / BusTub buffer pool project, the scan-resistant midpoint LRU lists of
//! InnoDB's buffer pool, 2Q/ARC-style caches in file systems, and the "submit IO, get a future, a worker thread does the disk
//! work" pattern used by storage engines to batch and order requests.
//! Course: CMU 15-445 Intro to Database Systems, Lecture 04 (Database Storage II / buffer pool) and the buffer-pool project.
#![allow(dead_code)]
//!
//! WHAT: three cooperating pieces.
//! 1. `LruKReplacer`: remembers the last K access timestamps of every frame. BACKWARD K-DISTANCE = now - (timestamp of the K-th
//!    most recent access). Evict the evictable frame with the LARGEST distance; a frame with fewer than K accesses has distance
//!    +infinity and loses first, ties broken by the EARLIEST FIRST ACCESS (classic LRU among the one-hit wonders). A page that a
//!    sequential scan touched once is +inf, so it is evicted before any page that was reused: scan resistance. K=1 is plain LRU.
//! 2. `DiskScheduler`: a worker thread owns the disk and consumes a channel of read/write requests. Each request carries a
//!    oneshot (an `mpsc` channel used once) that is the caller's FUTURE: `schedule_read(p).recv()` blocks until done. The worker
//!    drains everything queued into a BATCH, stable-sorts it by page id (disk-order, and per-page request order is kept so a
//!    read never overtakes an earlier write), and COALESCES back-to-back writes to one page into a single disk write. Dropping
//!    the scheduler sends a Shutdown request behind everything else, so queued work finishes before the thread is joined.
//! 3. `BufferPool`: frames with a `RwLock` each (the page LATCH), one mutex over the metadata (page table, pin counts, dirty
//!    bits, free list, replacer). `fetch_read/fetch_write/new_page` return RAII GUARDS: the page stays pinned (never evictable)
//!    while a guard lives, and dropping releases the latch BEFORE unpinning so a frame can never be recycled under a latch.
//!    A dirty victim is written back through the scheduler before its frame is reused; `flush_all` persists the rest.
//!    Writers mark the frame dirty (and bump a generation) when they acquire the latch, so a concurrent flush can only clear a
//!    dirty bit if nobody wrote after its snapshot.
//!
//! VERIFIED: the replacer against a brute-force oracle (20k random ops, K = 1, 2, 3); the scheduler's ordering, coalescing and
//! shutdown; pool semantics (pin exhaustion, delete of pinned pages, write-back on eviction); a concurrency test with 8 threads
//! doing random fetches on a 12-frame pool over 64 pages (page contents are `[page_id, v, v, v, ...]`, so a torn read or a
//! recycled-while-pinned frame is visible; after `flush_all` the disk must hold exactly the number of writes each page took);
//! and the hit ratios of LRU, CLOCK and LRU-2 on a scan-polluted trace (asserts LRU-2 >= LRU; the real pool must match the
//! standalone simulation exactly).
//!
//! Read: https://15445.courses.cs.cmu.edu/fall2024/project1/ (the buffer pool project: LRU-K, disk scheduler, page guards),
//! https://15445.courses.cs.cmu.edu/fall2024/slides/06-bufferpool.pdf (buffer pool lecture); the LRU-K paper itself is
//! O'Neil, O'Neil and Weikum, "The LRU-K page replacement algorithm for database disk buffering", SIGMOD 1993.
use std::collections::{HashMap, VecDeque};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const PAGE_SIZE: usize = 64;
const WORDS: usize = PAGE_SIZE / 8;
type Page = [u8; PAGE_SIZE];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

fn word(p: &Page, i: usize) -> u64 {
    u64::from_le_bytes(p[i * 8..i * 8 + 8].try_into().unwrap())
}
fn set_word(p: &mut Page, i: usize, v: u64) {
    p[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
}
/// Page layout used by the tests: word 0 = page id, words 1.. all equal to a version number.
fn make_page(id: u64, version: u64) -> Page {
    let mut p = [0; PAGE_SIZE];
    set_word(&mut p, 0, id);
    (1..WORDS).for_each(|i| set_word(&mut p, i, version));
    p
}
fn check_page(p: &Page, id: u64) -> u64 {
    assert_eq!(
        word(p, 0),
        id,
        "frame holds the wrong page: recycled while pinned?"
    );
    let v = word(p, 1);
    assert!((1..WORDS).all(|i| word(p, i) == v), "torn page {id}");
    v
}

// ======================= LRU-K replacer =======================
struct Node {
    history: VecDeque<u64>, // at most K timestamps, oldest first
    evictable: bool,
}

struct LruKReplacer {
    k: usize,
    now: u64,
    nodes: HashMap<usize, Node>,
    evictable: usize,
}

impl LruKReplacer {
    fn new(k: usize) -> Self {
        LruKReplacer {
            k,
            now: 0,
            nodes: HashMap::new(),
            evictable: 0,
        }
    }
    fn record_access(&mut self, f: usize) {
        self.now += 1;
        let (k, now) = (self.k, self.now);
        let n = self.nodes.entry(f).or_insert_with(|| Node {
            history: VecDeque::new(),
            evictable: false,
        });
        n.history.push_back(now);
        if n.history.len() > k {
            n.history.pop_front();
        }
    }
    fn set_evictable(&mut self, f: usize, e: bool) {
        if let Some(n) = self.nodes.get_mut(&f) {
            if n.evictable != e {
                n.evictable = e;
                if e {
                    self.evictable += 1
                } else {
                    self.evictable -= 1
                }
            }
        }
    }
    /// Smallest key loses: (has K accesses?, front timestamp). Front is the K-th most recent access when the
    /// frame has K of them (largest backward distance == smallest timestamp) and the first access otherwise.
    fn evict(&mut self) -> Option<usize> {
        let k = self.k;
        let victim = self
            .nodes
            .iter()
            .filter(|(_, n)| n.evictable)
            .min_by_key(|(_, n)| (n.history.len() >= k, n.history[0]))
            .map(|(&f, _)| f)?;
        self.nodes.remove(&victim);
        self.evictable -= 1;
        Some(victim)
    }
    fn remove(&mut self, f: usize) {
        if let Some(n) = self.nodes.get(&f) {
            assert!(n.evictable, "cannot remove a pinned frame");
            self.nodes.remove(&f);
            self.evictable -= 1;
        }
    }
    fn size(&self) -> usize {
        self.evictable
    }
}

/// Brute force: keeps the FULL history and recomputes backward k-distance from the definition.
struct Oracle {
    k: usize,
    now: u64,
    hist: HashMap<usize, (Vec<u64>, bool)>,
}
impl Oracle {
    fn record_access(&mut self, f: usize) {
        self.now += 1;
        self.hist
            .entry(f)
            .or_insert((Vec::new(), false))
            .0
            .push(self.now);
    }
    fn set_evictable(&mut self, f: usize, e: bool) {
        if let Some(h) = self.hist.get_mut(&f) {
            h.1 = e;
        }
    }
    fn evict(&mut self) -> Option<usize> {
        let cands: Vec<usize> = self
            .hist
            .iter()
            .filter(|(_, h)| h.1)
            .map(|(&f, _)| f)
            .collect();
        let inf: Vec<usize> = cands
            .iter()
            .copied()
            .filter(|f| self.hist[f].0.len() < self.k)
            .collect();
        let victim = if !inf.is_empty() {
            *inf.iter().min_by_key(|f| self.hist[f].0[0]).unwrap() // earliest first access
        } else {
            *cands.iter().max_by_key(|f| {
                let h = &self.hist[f].0;
                self.now - h[h.len() - self.k] // backward k-distance
            })?
        };
        self.hist.remove(&victim);
        Some(victim)
    }
    fn remove(&mut self, f: usize) {
        self.hist.remove(&f);
    }
    fn size(&self) -> usize {
        self.hist.values().filter(|h| h.1).count()
    }
}

fn replacer_oracle_test() {
    for k in 1..=3 {
        let mut rng = Rng(0xabcd + k as u64);
        let (mut a, mut o) = (
            LruKReplacer::new(k),
            Oracle {
                k,
                now: 0,
                hist: HashMap::new(),
            },
        );
        let mut evictions = 0;
        for _ in 0..20_000 {
            let f = rng.below(12);
            match rng.below(10) {
                0..=4 => {
                    a.record_access(f);
                    o.record_access(f);
                }
                5..=6 => {
                    let e = rng.below(2) == 0;
                    a.set_evictable(f, e);
                    o.set_evictable(f, e);
                }
                7..=8 => {
                    let (x, y) = (a.evict(), o.evict());
                    assert_eq!(x, y, "K={k}: evict disagrees");
                    evictions += x.is_some() as u32;
                }
                _ => {
                    if o.hist.get(&f).is_some_and(|h| h.1) {
                        a.remove(f);
                        o.remove(f);
                    }
                }
            }
            assert_eq!(a.size(), o.size());
        }
        assert!(evictions > 500);
        println!(
            "replacer K={k}: 20000 random ops match the brute-force oracle ({evictions} evictions)"
        );
    }
    // Hand-checked textbook case: frame 1 has 2 accesses, frames 2 and 3 have one each.
    let mut r = LruKReplacer::new(2);
    for f in [1, 2, 3, 1] {
        r.record_access(f);
    }
    (1..=3).for_each(|f| r.set_evictable(f, true));
    assert_eq!(
        r.evict(),
        Some(2),
        "+inf frames go first, earliest first access first"
    );
    assert_eq!(r.evict(), Some(3));
    assert_eq!(r.evict(), Some(1));
    assert_eq!(r.evict(), None);
}

// ======================= disk and scheduler =======================
struct Disk {
    pages: Mutex<Vec<Page>>,
    reads: AtomicU64,
    writes: AtomicU64,
}
impl Disk {
    fn with_pages(n: u64) -> Arc<Disk> {
        Arc::new(Disk {
            pages: Mutex::new((0..n).map(|i| make_page(i, 0)).collect()),
            reads: 0.into(),
            writes: 0.into(),
        })
    }
    fn read(&self, id: u64) -> Page {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.pages
            .lock()
            .unwrap()
            .get(id as usize)
            .copied()
            .unwrap_or([0; PAGE_SIZE])
    }
    fn write(&self, id: u64, p: &Page) {
        self.writes.fetch_add(1, Ordering::Relaxed);
        let mut v = self.pages.lock().unwrap();
        if v.len() <= id as usize {
            v.resize(id as usize + 1, [0; PAGE_SIZE]);
        }
        v[id as usize] = *p;
    }
}

enum Request {
    Read {
        page: u64,
        done: Sender<Page>,
    },
    Write {
        page: u64,
        data: Page,
        done: Sender<()>,
    },
    Shutdown,
}
impl Request {
    fn page(&self) -> u64 {
        match self {
            Request::Read { page, .. } | Request::Write { page, .. } => *page,
            Request::Shutdown => u64::MAX,
        }
    }
}

#[derive(Default)]
struct SchedStats {
    requests: AtomicU64,
    batches: AtomicU64,
    coalesced: AtomicU64,
}

struct DiskScheduler {
    tx: Option<Sender<Request>>,
    worker: Option<JoinHandle<()>>,
    stats: Arc<SchedStats>,
}

impl DiskScheduler {
    fn new(disk: Arc<Disk>) -> Self {
        let (tx, rx) = mpsc::channel();
        let stats = Arc::new(SchedStats::default());
        let s = stats.clone();
        let worker = thread::spawn(move || Self::run(rx, disk, s));
        DiskScheduler {
            tx: Some(tx),
            worker: Some(worker),
            stats,
        }
    }
    fn run(rx: Receiver<Request>, disk: Arc<Disk>, stats: Arc<SchedStats>) {
        loop {
            let Ok(first) = rx.recv() else { return };
            let mut batch = vec![first];
            while let Ok(r) = rx.try_recv() {
                batch.push(r); // everything queued while we were busy forms one batch
            }
            let stop = batch.iter().any(|r| matches!(r, Request::Shutdown));
            batch.retain(|r| !matches!(r, Request::Shutdown));
            stats.batches.fetch_add(1, Ordering::Relaxed);
            stats
                .requests
                .fetch_add(batch.len() as u64, Ordering::Relaxed);
            // Stable sort: disk order across pages, submission order within one page (read-your-writes holds).
            batch.sort_by_key(Request::page);
            let mut pending: Vec<Sender<()>> = Vec::new();
            let mut i = 0;
            while i < batch.len() {
                match &batch[i] {
                    Request::Read { page, done } => {
                        let _ = done.send(disk.read(*page));
                    }
                    Request::Write { page, data, done } => {
                        pending.push(done.clone());
                        let same_next = matches!(batch.get(i + 1), Some(Request::Write { page: p2, .. }) if p2 == page);
                        if same_next {
                            stats.coalesced.fetch_add(1, Ordering::Relaxed); // superseded by the next write
                        } else {
                            disk.write(*page, data);
                            pending.drain(..).for_each(|d| {
                                let _ = d.send(());
                            });
                        }
                    }
                    Request::Shutdown => unreachable!(),
                }
                i += 1;
            }
            if stop {
                return;
            }
        }
    }
    fn schedule_read(&self, page: u64) -> Receiver<Page> {
        let (done, fut) = mpsc::channel();
        self.tx
            .as_ref()
            .unwrap()
            .send(Request::Read { page, done })
            .unwrap();
        fut
    }
    fn schedule_write(&self, page: u64, data: Page) -> Receiver<()> {
        let (done, fut) = mpsc::channel();
        self.tx
            .as_ref()
            .unwrap()
            .send(Request::Write { page, data, done })
            .unwrap();
        fut
    }
}
impl Drop for DiskScheduler {
    fn drop(&mut self) {
        // Shutdown goes behind all queued requests, so they complete before the worker exits.
        let _ = self.tx.as_ref().unwrap().send(Request::Shutdown);
        self.tx = None;
        self.worker.take().unwrap().join().unwrap();
    }
}

fn scheduler_test() {
    let disk = Disk::with_pages(8);
    let sched = DiskScheduler::new(disk.clone());
    // Submit without waiting so the worker sees batches: 3 writes to page 2 (only the last must land), then a read of it.
    let futs: Vec<_> = (1..=3)
        .map(|v| sched.schedule_write(2, make_page(2, v)))
        .collect();
    let rd = sched.schedule_read(2);
    let other = sched.schedule_write(5, make_page(5, 9));
    futs.iter().for_each(|f| f.recv().unwrap());
    other.recv().unwrap();
    assert_eq!(
        check_page(&rd.recv().unwrap(), 2),
        3,
        "the read queued after three writes sees the last one"
    );
    assert_eq!(check_page(&disk.read(2), 2), 3);
    // Many fire-and-forget writes, then drop: shutdown must drain the queue.
    for i in 0..200u64 {
        let _ = sched.schedule_write(8 + i % 4, make_page(8 + i % 4, i + 1));
    }
    let (reqs, batches, co) = (
        sched.stats.requests.load(Ordering::Relaxed),
        sched.stats.batches.load(Ordering::Relaxed),
        sched.stats.coalesced.load(Ordering::Relaxed),
    );
    drop(sched);
    for p in 8..12u64 {
        let last = (0..200u64).filter(|i| 8 + i % 4 == p).max().unwrap() + 1;
        assert_eq!(
            check_page(&disk.read(p), p),
            last,
            "graceful shutdown lost a queued write"
        );
    }
    println!(
        "scheduler: ordering, read-your-writes and shutdown drain OK ({reqs} reqs in {batches} batches, {co} writes coalesced)"
    );
}

// ======================= buffer pool =======================
struct State {
    table: HashMap<u64, usize>,
    page_of: Vec<Option<u64>>,
    pins: Vec<usize>,
    dirty: Vec<bool>,
    generation: Vec<u64>,
    free: Vec<usize>,
    rep: LruKReplacer,
    next_page: u64,
    hits: u64,
    misses: u64,
    writebacks: u64,
}

struct BufferPool {
    frames: Vec<RwLock<Page>>,
    st: Mutex<State>,
    sched: DiskScheduler,
}

struct ReadGuard<'a> {
    pool: &'a BufferPool,
    frame: usize,
    g: Option<RwLockReadGuard<'a, Page>>,
}
struct WriteGuard<'a> {
    pool: &'a BufferPool,
    frame: usize,
    g: Option<RwLockWriteGuard<'a, Page>>,
}
impl Deref for ReadGuard<'_> {
    type Target = Page;
    fn deref(&self) -> &Page {
        self.g.as_ref().unwrap()
    }
}
impl Deref for WriteGuard<'_> {
    type Target = Page;
    fn deref(&self) -> &Page {
        self.g.as_ref().unwrap()
    }
}
impl DerefMut for WriteGuard<'_> {
    fn deref_mut(&mut self) -> &mut Page {
        self.g.as_mut().unwrap()
    }
}
impl Drop for ReadGuard<'_> {
    fn drop(&mut self) {
        self.g.take(); // unlatch first, then unpin
        self.pool.unpin(self.frame);
    }
}
impl Drop for WriteGuard<'_> {
    fn drop(&mut self) {
        self.g.take();
        self.pool.unpin(self.frame);
    }
}
impl ReadGuard<'_> {
    fn frame(&self) -> usize {
        self.frame
    }
}

impl BufferPool {
    fn new(frames: usize, k: usize, disk: Arc<Disk>, first_new_page: u64) -> BufferPool {
        BufferPool {
            frames: (0..frames).map(|_| RwLock::new([0; PAGE_SIZE])).collect(),
            st: Mutex::new(State {
                table: HashMap::new(),
                page_of: vec![None; frames],
                pins: vec![0; frames],
                dirty: vec![false; frames],
                generation: vec![0; frames],
                free: (0..frames).rev().collect(),
                rep: LruKReplacer::new(k),
                next_page: first_new_page,
                hits: 0,
                misses: 0,
                writebacks: 0,
            }),
            sched: DiskScheduler::new(disk),
        }
    }
    fn unpin(&self, f: usize) {
        let mut s = self.st.lock().unwrap();
        assert!(s.pins[f] > 0);
        s.pins[f] -= 1;
        if s.pins[f] == 0 {
            s.rep.set_evictable(f, true);
        }
    }
    /// Find a frame for a new resident page: free list first, then the replacer; write back a dirty victim.
    fn take_frame(&self, s: &mut State) -> Option<usize> {
        let f = s.free.pop().or_else(|| s.rep.evict())?;
        if let Some(old) = s.page_of[f].take() {
            if s.dirty[f] {
                let data = *self.frames[f].read().unwrap(); // pins == 0, so no guard can hold this latch
                self.sched.schedule_write(old, data).recv().unwrap();
                s.writebacks += 1;
            }
            s.table.remove(&old);
        }
        s.dirty[f] = false;
        Some(f)
    }
    /// Pin `id` (loading it on a miss). Returns None when every frame is pinned.
    fn pin(&self, id: u64, for_write: bool) -> Option<usize> {
        let mut s = self.st.lock().unwrap();
        let f = if let Some(&f) = s.table.get(&id) {
            s.hits += 1;
            f
        } else {
            let f = self.take_frame(&mut s)?;
            s.misses += 1;
            *self.frames[f].write().unwrap() = self.sched.schedule_read(id).recv().unwrap();
            s.table.insert(id, f);
            s.page_of[f] = Some(id);
            f
        };
        s.pins[f] += 1;
        s.rep.record_access(f);
        s.rep.set_evictable(f, false);
        if for_write {
            s.dirty[f] = true;
            s.generation[f] += 1;
        }
        Some(f)
    }
    fn fetch_read(&self, id: u64) -> Option<ReadGuard<'_>> {
        let frame = self.pin(id, false)?;
        Some(ReadGuard {
            pool: self,
            frame,
            g: Some(self.frames[frame].read().unwrap()),
        })
    }
    fn fetch_write(&self, id: u64) -> Option<WriteGuard<'_>> {
        let frame = self.pin(id, true)?;
        Some(WriteGuard {
            pool: self,
            frame,
            g: Some(self.frames[frame].write().unwrap()),
        })
    }
    /// Allocate a fresh zeroed page id and return it pinned for writing.
    fn new_page(&self) -> Option<(u64, WriteGuard<'_>)> {
        let id = {
            let mut s = self.st.lock().unwrap();
            let f = self.take_frame(&mut s)?;
            let id = s.next_page;
            s.next_page += 1;
            *self.frames[f].write().unwrap() = [0; PAGE_SIZE];
            s.table.insert(id, f);
            s.page_of[f] = Some(id);
            s.free.push(f); // hand the frame back; `pin` below finds it as a hit
            s.table.insert(id, f);
            id
        };
        // The frame sits on the free list only for an instant while we hold no one else's pin; re-pin as a hit.
        let mut s = self.st.lock().unwrap();
        let f = s.table[&id];
        s.free.retain(|&x| x != f);
        s.pins[f] += 1;
        s.rep.record_access(f);
        s.rep.set_evictable(f, false);
        s.dirty[f] = true;
        s.generation[f] += 1;
        drop(s);
        Some((
            id,
            WriteGuard {
                pool: self,
                frame: f,
                g: Some(self.frames[f].write().unwrap()),
            },
        ))
    }
    fn delete_page(&self, id: u64) -> bool {
        let mut s = self.st.lock().unwrap();
        let Some(&f) = s.table.get(&id) else {
            return true;
        };
        if s.pins[f] > 0 {
            return false;
        }
        s.rep.remove(f);
        s.table.remove(&id);
        s.page_of[f] = None;
        s.dirty[f] = false; // a deleted page is not written back
        s.free.push(f);
        true
    }
    fn flush_page(&self, id: u64) {
        let (f, g0) = {
            let mut s = self.st.lock().unwrap();
            let Some(&f) = s.table.get(&id) else { return };
            if !s.dirty[f] {
                return;
            }
            s.pins[f] += 1; // keep it resident while we copy
            s.rep.set_evictable(f, false);
            (f, s.generation[f])
        };
        let data = *self.frames[f].read().unwrap(); // waits for any active writer
        self.sched.schedule_write(id, data).recv().unwrap();
        {
            let mut s = self.st.lock().unwrap();
            if s.generation[f] == g0 {
                s.dirty[f] = false; // nobody wrote since our snapshot
            }
            s.writebacks += 1;
        }
        self.unpin(f);
    }
    fn flush_all(&self) {
        let ids: Vec<u64> = self.st.lock().unwrap().table.keys().copied().collect();
        ids.into_iter().for_each(|id| self.flush_page(id));
    }
    fn dirty_count(&self) -> usize {
        self.st.lock().unwrap().dirty.iter().filter(|&&d| d).count()
    }
    fn counters(&self) -> (u64, u64, u64) {
        let s = self.st.lock().unwrap();
        (s.hits, s.misses, s.writebacks)
    }
    /// Metadata consistency (call when quiescent).
    fn check(&self) {
        let s = self.st.lock().unwrap();
        for (f, p) in s.page_of.iter().enumerate() {
            match p {
                Some(id) => assert_eq!(s.table.get(id), Some(&f)),
                None => assert!(s.free.contains(&f) && s.pins[f] == 0 && !s.dirty[f]),
            }
        }
        assert_eq!(s.table.len(), s.page_of.iter().flatten().count());
        let unpinned_resident = (0..s.pins.len())
            .filter(|&f| s.page_of[f].is_some() && s.pins[f] == 0)
            .count();
        assert_eq!(
            s.rep.size(),
            unpinned_resident,
            "replacer evictable set == resident unpinned frames"
        );
    }
}

fn pool_semantics_test() {
    let disk = Disk::with_pages(10);
    let pool = BufferPool::new(3, 2, disk.clone(), 10);
    let (a, b, c) = (
        pool.fetch_read(0).unwrap(),
        pool.fetch_read(1).unwrap(),
        pool.fetch_read(2).unwrap(),
    );
    assert!(pool.fetch_read(3).is_none(), "all frames pinned: no victim");
    assert!(!pool.delete_page(1), "cannot delete a pinned page");
    drop(b);
    {
        let mut w = pool.fetch_write(3).expect("freed pin -> evictable frame");
        let v = check_page(&w, 3);
        *w = make_page(3, v + 41);
    }
    assert_eq!(
        check_page(&disk.read(3), 3),
        0,
        "write-back is lazy: not on disk yet"
    );
    drop((a, c));
    // Touch other pages until page 3's frame is the victim; its dirty data must reach the disk.
    for p in [4, 5, 6, 7] {
        drop(pool.fetch_read(p).unwrap());
    }
    assert_eq!(
        check_page(&disk.read(3), 3),
        41,
        "dirty victim written back on eviction"
    );
    let (np, mut g) = pool.new_page().unwrap();
    assert_eq!(np, 10);
    set_word(&mut g, 0, np);
    (1..WORDS).for_each(|i| set_word(&mut g, i, 5));
    drop(g);
    assert!(
        pool.delete_page(np) && pool.fetch_read(np).map(|g| word(&g, 0)) != Some(np),
        "deleted page is gone and was not written back"
    );
    pool.flush_all();
    assert_eq!(pool.dirty_count(), 0);
    pool.check();
    println!(
        "pool semantics: pin exhaustion, delete of pinned, lazy dirty write-back, new/delete page OK"
    );
}

// ======================= concurrency test =======================
fn concurrency_test() -> (f64, u64, u64) {
    const PAGES: u64 = 64;
    let disk = Disk::with_pages(PAGES);
    for p in 0..PAGES {
        disk.write(p, &make_page(p, 1)); // version starts at 1
    }
    let pool = Arc::new(BufferPool::new(12, 2, disk.clone(), PAGES));
    let writes: Arc<Vec<AtomicU64>> = Arc::new((0..PAGES).map(|_| AtomicU64::new(0)).collect());
    let done = Arc::new(AtomicUsize::new(0));
    let t0 = Instant::now();
    // Watchdog: a deadlock must fail the run, not hang it. Progress-based (finished operations), never wall-clock: a slow run on
    // an oversubscribed machine keeps advancing and is left alone; only 30 s with ZERO finished operations aborts.
    let progress = Arc::new(AtomicU64::new(0));
    {
        let (done, progress) = (done.clone(), progress.clone());
        thread::spawn(move || {
            let (mut last, mut stalled) = (u64::MAX, 0u32);
            while done.load(Ordering::SeqCst) < 8 {
                let now = progress.load(Ordering::Relaxed);
                stalled = if now == last { stalled + 1 } else { 0 };
                last = now;
                if stalled >= 600 {
                    eprintln!("DEADLOCK: no buffer-pool operation finished for 30 s ({now} done)");
                    std::process::exit(1);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }
    let handles: Vec<_> = (0..8u64)
        .map(|t| {
            let (pool, writes, done, progress) =
                (pool.clone(), writes.clone(), done.clone(), progress.clone());
            thread::spawn(move || {
                let mut rng = Rng(0x1234_5678 + t * 7919);
                for _ in 0..4000 {
                    progress.fetch_add(1, Ordering::Relaxed);
                    // Skewed: half the accesses hit 8 hot pages.
                    let id = if rng.below(2) == 0 {
                        rng.below(8)
                    } else {
                        rng.below(PAGES as usize)
                    } as u64;
                    if rng.below(10) < 3 {
                        let mut g = loop {
                            if let Some(g) = pool.fetch_write(id) {
                                break g;
                            }
                            thread::yield_now();
                        };
                        let v = check_page(&g, id);
                        thread::yield_now(); // widen the window in which a bad latch would tear the page
                        (1..WORDS).for_each(|i| set_word(&mut g, i, v + 1));
                        writes[id as usize].fetch_add(1, Ordering::SeqCst);
                    } else {
                        let g = loop {
                            if let Some(g) = pool.fetch_read(id) {
                                break g;
                            }
                            thread::yield_now();
                        };
                        let v1 = check_page(&g, id);
                        thread::yield_now(); // pinned: the frame must still hold this very page afterwards
                        assert_eq!(check_page(&g, id), v1, "page changed under a read latch");
                        let _ = g.frame();
                    }
                }
                done.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();
    handles
        .into_iter()
        .for_each(|h| h.join().expect("worker panicked"));
    let secs = t0.elapsed().as_secs_f64();
    pool.flush_all();
    assert_eq!(pool.dirty_count(), 0, "flush_all must leave no dirty frame");
    pool.check();
    let mut total_writes = 0;
    for p in 0..PAGES {
        let n = writes[p as usize].load(Ordering::SeqCst);
        total_writes += n;
        assert_eq!(
            check_page(&disk.read(p), p),
            1 + n,
            "page {p}: disk version != 1 + committed writes (lost update)"
        );
    }
    let (h, m, wb) = pool.counters();
    println!(
        "concurrency: 8 threads x 4000 ops, 12 frames / 64 pages: {total_writes} writes all persisted, hits={h} misses={m} write-backs={wb}, {secs:.2}s"
    );
    (h as f64 / (h + m) as f64, total_writes, wb)
}

// ======================= hit-ratio comparison =======================
/// Scan-polluted trace: half the accesses go to 20 hot pages, half walk sequentially through a big cold range.
fn scan_trace(n: usize) -> Vec<u64> {
    let mut rng = Rng(77);
    let mut scan = 100u64;
    (0..n)
        .map(|_| {
            if rng.below(2) == 0 {
                rng.below(20) as u64
            } else {
                scan += 1;
                100 + (scan - 100) % 5000
            }
        })
        .collect()
}

trait Policy {
    fn name(&self) -> String;
    fn hit(&mut self, f: usize);
    fn load(&mut self, f: usize);
    fn victim(&mut self) -> usize;
}

struct KPolicy(LruKReplacer);
impl Policy for KPolicy {
    fn name(&self) -> String {
        if self.0.k == 1 {
            "LRU".into()
        } else {
            format!("LRU-{}", self.0.k)
        }
    }
    fn hit(&mut self, f: usize) {
        self.0.record_access(f);
    }
    fn load(&mut self, f: usize) {
        self.0.record_access(f);
        self.0.set_evictable(f, true);
    }
    fn victim(&mut self) -> usize {
        self.0.evict().unwrap()
    }
}
struct ClockPolicy {
    refbit: Vec<bool>,
    hand: usize,
}
impl Policy for ClockPolicy {
    fn name(&self) -> String {
        "CLOCK".into()
    }
    fn hit(&mut self, f: usize) {
        self.refbit[f] = true;
    }
    fn load(&mut self, f: usize) {
        self.refbit[f] = true;
    }
    fn victim(&mut self) -> usize {
        loop {
            let f = self.hand;
            self.hand = (self.hand + 1) % self.refbit.len();
            if self.refbit[f] {
                self.refbit[f] = false; // second chance
            } else {
                return f;
            }
        }
    }
}

fn simulate(trace: &[u64], frames: usize, p: &mut dyn Policy) -> f64 {
    let (mut table, mut used, mut hits): (HashMap<u64, usize>, usize, u64) = (HashMap::new(), 0, 0);
    let mut owner: Vec<u64> = vec![0; frames];
    for &page in trace {
        if let Some(&f) = table.get(&page) {
            hits += 1;
            p.hit(f);
            continue;
        }
        let f = if used < frames {
            used += 1;
            used - 1
        } else {
            let f = p.victim();
            table.remove(&owner[f]);
            f
        };
        owner[f] = page;
        table.insert(page, f);
        p.load(f);
    }
    hits as f64 / trace.len() as f64
}

fn hit_ratio_test() {
    let trace = scan_trace(60_000);
    let frames = 30;
    let mut rows = Vec::new();
    let mut pols: Vec<Box<dyn Policy>> = vec![
        Box::new(KPolicy(LruKReplacer::new(1))),
        Box::new(ClockPolicy {
            refbit: vec![false; frames],
            hand: 0,
        }),
        Box::new(KPolicy(LruKReplacer::new(2))),
        Box::new(KPolicy(LruKReplacer::new(3))),
    ];
    println!(
        "hit ratio on a scan-polluted trace (50% hot set of 20 pages, 50% sequential scan), {frames} frames:"
    );
    for p in pols.iter_mut() {
        let r = simulate(&trace, frames, p.as_mut());
        println!("  {:<7} {:>6.2}%", p.name(), 100.0 * r);
        rows.push(r);
    }
    assert!(
        rows[2] >= rows[0],
        "LRU-2 must be at least as good as LRU on a scan-polluted workload"
    );
    assert!(
        rows[2] > rows[0] + 0.05,
        "and clearly better here: {} vs {}",
        rows[2],
        rows[0]
    );
    // The real pool (replacer + page table + disk scheduler) must reproduce the simulation's hit counts exactly.
    for (k, expect) in [(1usize, rows[0]), (2, rows[2])] {
        let disk = Disk::with_pages(6000);
        let pool = BufferPool::new(frames, k, disk, 6000);
        for &p in &trace {
            drop(pool.fetch_read(p).unwrap());
        }
        let (h, m, _) = pool.counters();
        let got = h as f64 / (h + m) as f64;
        assert!(
            (got - expect).abs() < 1e-12,
            "pool K={k}: {got} != simulated {expect}"
        );
        pool.check();
    }
    println!(
        "  (the real buffer pool with K=1 and K=2 reproduces the simulated LRU and LRU-2 hit counts exactly)"
    );
}

fn main() {
    replacer_oracle_test();
    scheduler_test();
    pool_semantics_test();
    hit_ratio_test();
    let (ratio, writes, _) = concurrency_test();
    assert!(writes > 5000 && ratio > 0.0);
    println!("all LRU-K / disk scheduler / buffer pool checks passed");
}
