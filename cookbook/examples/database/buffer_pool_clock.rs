//! RECIPE 173 (database): a buffer pool: page frames, pin counts with an RAII guard, dirty write-back, CLOCK / LRU / LRU-2.
//! LEVEL: advanced
//! Use in: database storage engines, file/page caches, anything with "RAM is a cache of a bigger slower store".   Run: cargo run --release --example buffer_pool_clock
#![allow(dead_code)]
//!
//! A DBMS reads and writes fixed-size PAGES. The buffer pool owns a fixed number of in-memory FRAMES and a PAGE TABLE
//! (`page_id -> frame`). `fetch(page_id)` returns a `PageGuard`: a hit pins the frame; a miss picks a free frame or asks the
//! REPLACEMENT POLICY for a victim, writes the victim back to disk if its DIRTY bit is set, then reads the wanted page.
//! * PIN COUNT: a pinned page is in use and must never be evicted. The guard's `Drop` unpins, so a forgotten unpin (a leak that
//!   slowly makes the pool unevictable) is impossible by construction. If every frame is pinned, `fetch` returns `None`.
//! * DIRTY BIT: `with_data_mut` sets it; eviction and `flush_all` write the page back first (write-back, not write-through).
//!   A real engine also needs the write-ahead log flushed up to the page's LSN before the page itself; omitted here.
//! * POLICIES behind a trait: plain LRU (the baseline), CLOCK (second chance: a reference bit per frame and a sweeping hand,
//!   an O(1) approximation of LRU used where exact recency lists are too costly), and LRU-2 (O'Neil et al.: evict the page whose
//!   2nd-most-recent access is oldest; a page touched only once has infinite distance and goes first). SEQUENTIAL SCANS touch
//!   each page once, so LRU/CLOCK let a big scan flush the hot set ("cache pollution") while LRU-2 keeps it. Simplification:
//!   real LRU-K retains history for recently evicted pages; here a frame's history is dropped on eviction. 2Q, ARC and
//!   "scan-resistant" ring buffers (PostgreSQL uses a small ring for big sequential scans) attack the same problem.
//! * The simulated `Disk` is a `Vec` of pages with read/write counters, so the test can assert exactly what was persisted.
//!
//! Checked after EVERY operation: no pinned page is evictable, page table <-> frames agree, free frames are empty, the
//! policy's evictable set equals {resident, unpinned frames}, and every CLEAN resident page equals its disk copy (so dirty
//! pages really are persisted before their frame is reused). Reads verify data against an expected-version model, so a lost
//! write-back would show up as a wrong value.
//!
//! COMMON USES: the buffer manager of relational databases (PostgreSQL's shared buffers use a clock-sweep variant; InnoDB
//! has a buffer pool with an LRU variant), embedded stores, OS page caches, and caches of disk blocks in file systems.
//! Read: https://www.cs.cmu.edu/~15445/ (CMU database course, buffer pool project) and
//! https://dl.acm.org/doi/10.1145/170036.170081 (the LRU-K paper)
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

const PAGE_SIZE: usize = 64;
type Page = [u8; PAGE_SIZE];

struct Disk {
    pages: Vec<Page>,
    reads: u64,
    writes: u64,
}
impl Disk {
    fn new(n: usize) -> Disk {
        Disk {
            pages: vec![[0; PAGE_SIZE]; n],
            reads: 0,
            writes: 0,
        }
    }
    fn read(&mut self, id: u64, out: &mut Page) {
        self.reads += 1;
        *out = self.pages[id as usize];
    }
    fn write(&mut self, id: u64, data: &Page) {
        self.writes += 1;
        self.pages[id as usize] = *data;
    }
}

// ---------- replacement policies ----------
trait Replacer {
    fn name(&self) -> String;
    fn access(&mut self, f: usize);
    fn set_evictable(&mut self, f: usize, e: bool);
    /// Pick an evictable frame, stop tracking it (and forget its history).
    fn victim(&mut self) -> Option<usize>;
    fn evictable_count(&self) -> usize;
    fn is_evictable(&self, f: usize) -> bool;
}

struct Lru {
    t: u64,
    last: Vec<u64>,
    ev: Vec<bool>,
}
impl Lru {
    fn new(n: usize) -> Self {
        Lru {
            t: 0,
            last: vec![0; n],
            ev: vec![false; n],
        }
    }
}
impl Replacer for Lru {
    fn name(&self) -> String {
        "LRU".into()
    }
    fn access(&mut self, f: usize) {
        self.t += 1;
        self.last[f] = self.t;
    }
    fn set_evictable(&mut self, f: usize, e: bool) {
        self.ev[f] = e;
    }
    fn victim(&mut self) -> Option<usize> {
        let f = (0..self.ev.len())
            .filter(|&f| self.ev[f])
            .min_by_key(|&f| self.last[f])?;
        self.ev[f] = false;
        Some(f)
    }
    fn evictable_count(&self) -> usize {
        self.ev.iter().filter(|&&e| e).count()
    }
    fn is_evictable(&self, f: usize) -> bool {
        self.ev[f]
    }
}

struct Clock {
    refbit: Vec<bool>,
    ev: Vec<bool>,
    hand: usize,
}
impl Clock {
    fn new(n: usize) -> Self {
        Clock {
            refbit: vec![false; n],
            ev: vec![false; n],
            hand: 0,
        }
    }
}
impl Replacer for Clock {
    fn name(&self) -> String {
        "CLOCK".into()
    }
    fn access(&mut self, f: usize) {
        self.refbit[f] = true;
    }
    fn set_evictable(&mut self, f: usize, e: bool) {
        self.ev[f] = e;
    }
    fn victim(&mut self) -> Option<usize> {
        let n = self.ev.len();
        for _ in 0..2 * n {
            let f = self.hand;
            self.hand = (self.hand + 1) % n;
            if self.ev[f] {
                if self.refbit[f] {
                    self.refbit[f] = false; // second chance
                } else {
                    self.ev[f] = false;
                    return Some(f);
                }
            }
        }
        None // first sweep clears every ref bit, so reaching here means nothing is evictable
    }
    fn evictable_count(&self) -> usize {
        self.ev.iter().filter(|&&e| e).count()
    }
    fn is_evictable(&self, f: usize) -> bool {
        self.ev[f]
    }
}

struct LruK {
    k: usize,
    t: u64,
    hist: Vec<VecDeque<u64>>, // most recent last, at most k entries
    ev: Vec<bool>,
}
impl LruK {
    fn new(n: usize, k: usize) -> Self {
        LruK {
            k,
            t: 0,
            hist: vec![VecDeque::new(); n],
            ev: vec![false; n],
        }
    }
}
impl Replacer for LruK {
    fn name(&self) -> String {
        format!("LRU-{}", self.k)
    }
    fn access(&mut self, f: usize) {
        self.t += 1;
        let h = &mut self.hist[f];
        h.push_back(self.t);
        if h.len() > self.k {
            h.pop_front();
        }
    }
    fn set_evictable(&mut self, f: usize, e: bool) {
        self.ev[f] = e;
    }
    fn victim(&mut self) -> Option<usize> {
        // fewer than k accesses => infinite backward K-distance => evicted first (oldest first); else oldest K-th access
        let f = (0..self.ev.len())
            .filter(|&f| self.ev[f])
            .min_by_key(|&f| {
                (
                    self.hist[f].len() >= self.k,
                    self.hist[f].front().copied().unwrap_or(0),
                )
            })?;
        self.ev[f] = false;
        self.hist[f].clear();
        Some(f)
    }
    fn evictable_count(&self) -> usize {
        self.ev.iter().filter(|&&e| e).count()
    }
    fn is_evictable(&self, f: usize) -> bool {
        self.ev[f]
    }
}

// ---------- buffer pool ----------
struct Frame {
    page: Option<u64>,
    pin: u32,
    dirty: bool,
    data: Page,
}

#[derive(Default, Clone, Copy, Debug)]
struct Stats {
    hits: u64,
    misses: u64,
    evictions: u64,
    writebacks: u64,
}
impl Stats {
    fn hit_ratio(&self) -> f64 {
        self.hits as f64 / (self.hits + self.misses).max(1) as f64
    }
}

struct Inner {
    frames: Vec<Frame>,
    table: HashMap<u64, usize>,
    free: Vec<usize>,
    replacer: Box<dyn Replacer>,
    disk: Disk,
    stats: Stats,
}

struct BufferPool {
    inner: Rc<RefCell<Inner>>,
}

struct PageGuard {
    pool: Rc<RefCell<Inner>>,
    frame: usize,
    page: u64,
}

impl BufferPool {
    fn new(frames: usize, disk: Disk, replacer: Box<dyn Replacer>) -> BufferPool {
        let fr = (0..frames)
            .map(|_| Frame {
                page: None,
                pin: 0,
                dirty: false,
                data: [0; PAGE_SIZE],
            })
            .collect();
        let inner = Inner {
            frames: fr,
            table: HashMap::new(),
            free: (0..frames).rev().collect(),
            replacer,
            disk,
            stats: Stats::default(),
        };
        BufferPool {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    /// Pin a page, loading it if needed. `None` when every frame is pinned.
    fn fetch(&self, page: u64) -> Option<PageGuard> {
        let mut g = self.inner.borrow_mut();
        let s = &mut *g;
        let frame = if let Some(&f) = s.table.get(&page) {
            s.stats.hits += 1;
            f
        } else {
            s.stats.misses += 1;
            let f = match s.free.pop() {
                Some(f) => f,
                None => {
                    let f = s.replacer.victim()?;
                    let fr = &mut s.frames[f];
                    assert_eq!(fr.pin, 0, "policy chose a pinned frame");
                    let old = fr.page.take().unwrap();
                    if fr.dirty {
                        s.disk.write(old, &fr.data); // write-back BEFORE the frame is reused
                        s.stats.writebacks += 1;
                        fr.dirty = false;
                    }
                    s.table.remove(&old);
                    s.stats.evictions += 1;
                    f
                }
            };
            let fr = &mut s.frames[f];
            s.disk.read(page, &mut fr.data);
            fr.page = Some(page);
            fr.dirty = false;
            s.table.insert(page, f);
            f
        };
        s.frames[frame].pin += 1;
        s.replacer.access(frame);
        s.replacer.set_evictable(frame, false);
        Some(PageGuard {
            pool: self.inner.clone(),
            frame,
            page,
        })
    }

    fn flush_all(&self) {
        let mut g = self.inner.borrow_mut();
        let s = &mut *g;
        for fr in s.frames.iter_mut().filter(|f| f.dirty) {
            s.disk.write(fr.page.unwrap(), &fr.data);
            s.stats.writebacks += 1;
            fr.dirty = false;
        }
    }

    fn stats(&self) -> Stats {
        self.inner.borrow().stats
    }

    fn check_invariants(&self) {
        let s = self.inner.borrow();
        let mut evictable = 0;
        for (&pid, &f) in &s.table {
            assert_eq!(
                s.frames[f].page,
                Some(pid),
                "page table points at the wrong frame"
            );
        }
        for (f, fr) in s.frames.iter().enumerate() {
            match fr.page {
                Some(pid) => {
                    assert_eq!(
                        s.table.get(&pid),
                        Some(&f),
                        "resident page missing from table"
                    );
                    assert_eq!(
                        s.replacer.is_evictable(f),
                        fr.pin == 0,
                        "evictable flag != (pin == 0) for frame {f}"
                    );
                    evictable += (fr.pin == 0) as usize;
                    if !fr.dirty {
                        assert_eq!(
                            fr.data, s.disk.pages[pid as usize],
                            "clean page differs from disk: lost write-back"
                        );
                    }
                }
                None => assert!(
                    fr.pin == 0 && !fr.dirty && s.free.contains(&f),
                    "free frame {f} not clean/empty"
                ),
            }
        }
        assert_eq!(s.replacer.evictable_count(), evictable);
        assert_eq!(s.table.len() + s.free.len(), s.frames.len());
    }
}

impl PageGuard {
    fn page_id(&self) -> u64 {
        self.page
    }
    fn with_data<R>(&self, f: impl FnOnce(&Page) -> R) -> R {
        f(&self.pool.borrow().frames[self.frame].data)
    }
    /// Mutable access; marks the frame dirty.
    fn with_data_mut<R>(&self, f: impl FnOnce(&mut Page) -> R) -> R {
        let mut g = self.pool.borrow_mut();
        let fr = &mut g.frames[self.frame];
        fr.dirty = true;
        f(&mut fr.data)
    }
}
impl Drop for PageGuard {
    fn drop(&mut self) {
        let mut g = self.pool.borrow_mut();
        let s = &mut *g;
        let fr = &mut s.frames[self.frame];
        fr.pin -= 1;
        if fr.pin == 0 {
            s.replacer.set_evictable(self.frame, true);
        }
    }
}

// ---------- workloads ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

type Op = (u64, bool); // (page, is_write)

/// One hot-set access (48 pages) interleaved with one step of a long sequential scan.
fn scan_pollution(n: usize) -> Vec<Op> {
    let mut r = Rng(11);
    (0..n)
        .flat_map(|i| {
            [
                (r.below(48) as u64, r.below(10) == 0),
                (100 + (i % 4900) as u64, false),
            ]
        })
        .collect()
}
fn zipf(n: usize, pages: usize, s: f64) -> Vec<Op> {
    let mut cdf: Vec<f64> = (1..=pages).map(|i| 1.0 / (i as f64).powf(s)).collect();
    let total: f64 = cdf.iter().sum();
    let mut acc = 0.0;
    for c in &mut cdf {
        acc += *c / total;
        *c = acc;
    }
    let mut r = Rng(22);
    (0..n)
        .map(|_| {
            (
                cdf.partition_point(|&c| c < r.unit()).min(pages - 1) as u64,
                r.below(10) == 0,
            )
        })
        .collect()
}
fn hot_only(n: usize) -> Vec<Op> {
    let mut r = Rng(33);
    (0..n)
        .map(|_| (r.below(48) as u64, r.below(5) == 0))
        .collect()
}

fn run(ops: &[Op], frames: usize, pages: usize, replacer: Box<dyn Replacer>) -> Stats {
    let pool = BufferPool::new(frames, Disk::new(pages), replacer);
    let mut expected = vec![0u64; pages]; // model: version counter stored in each page's first 8 bytes
    for &(pid, write) in ops {
        let g = pool.fetch(pid).expect("some frame is unpinned");
        assert_eq!(g.page_id(), pid);
        let v = g.with_data(|d| u64::from_le_bytes(d[..8].try_into().unwrap()));
        assert_eq!(
            v, expected[pid as usize],
            "stale or lost data for page {pid}"
        );
        if write {
            expected[pid as usize] += 1;
            g.with_data_mut(|d| d[..8].copy_from_slice(&expected[pid as usize].to_le_bytes()));
        }
        drop(g);
        pool.check_invariants();
    }
    pool.flush_all();
    pool.check_invariants();
    let i = pool.inner.borrow();
    for (pid, e) in expected.iter().enumerate() {
        assert_eq!(
            u64::from_le_bytes(i.disk.pages[pid][..8].try_into().unwrap()),
            *e,
            "page {pid} not persisted"
        );
    }
    assert_eq!(i.disk.reads, i.stats.misses);
    assert_eq!(i.disk.writes, i.stats.writebacks);
    i.stats
}

fn policies(n: usize) -> Vec<Box<dyn Replacer>> {
    vec![
        Box::new(Lru::new(n)),
        Box::new(Clock::new(n)),
        Box::new(LruK::new(n, 2)),
    ]
}

fn main() {
    // 1. pinning, RAII unpin, exhaustion
    let pool = BufferPool::new(3, Disk::new(10), Box::new(Clock::new(3)));
    let g0 = pool.fetch(0).unwrap();
    let g0b = pool.fetch(0).unwrap(); // second pin on the same frame
    let g1 = pool.fetch(1).unwrap();
    let g2 = pool.fetch(2).unwrap();
    assert!(pool.fetch(3).is_none(), "all frames pinned");
    drop(g1);
    let g3 = pool.fetch(3).unwrap(); // evicts page 1 (the only unpinned frame)
    assert!(pool.inner.borrow().table.get(&1).is_none());
    g3.with_data_mut(|d| d[0] = 7);
    drop(g0);
    assert!(
        pool.fetch(4).is_none(),
        "page 0 still pinned once, so no victim"
    );
    drop(g0b);
    drop(g3); // dirty page 3 becomes evictable
    let _g4 = pool.fetch(4).unwrap();
    pool.check_invariants();
    drop(g2);
    println!(
        "pinning: exhausted pool returns None, guard drop unpins, stats {:?}",
        pool.stats()
    );

    // 2. dirty write-back on eviction
    let pool = BufferPool::new(2, Disk::new(10), Box::new(Lru::new(2)));
    pool.fetch(0).unwrap().with_data_mut(|d| d[0] = 42);
    pool.fetch(1).unwrap();
    assert_eq!(
        pool.inner.borrow().disk.pages[0][0],
        0,
        "write-back is lazy"
    );
    pool.fetch(2).unwrap(); // evicts page 0 (LRU) and writes it back
    assert_eq!(pool.inner.borrow().disk.pages[0][0], 42);
    assert_eq!(pool.stats().writebacks, 1);
    pool.check_invariants();

    // 3. policies on three workloads, every op checked by invariants + data model
    let frames = 64;
    let workloads: Vec<(&str, Vec<Op>, usize)> = vec![
        ("hot set fits (48 of 64 frames)", hot_only(30_000), 48),
        (
            "zipf s=1.0 over 2000 pages    ",
            zipf(30_000, 2000, 1.0),
            2000,
        ),
        (
            "scan pollution (48 hot + scan)",
            scan_pollution(15_000),
            5000,
        ),
    ];
    let mut ratios: HashMap<(usize, String), f64> = HashMap::new();
    println!(
        "\n{:<32} {:>8} {:>8} {:>8}",
        "hit ratio, 64 frames", "LRU", "CLOCK", "LRU-2"
    );
    for (wi, (name, ops, pages)) in workloads.iter().enumerate() {
        let mut row = format!("{name:<32}");
        for p in policies(frames) {
            let pn = p.name();
            let st = run(ops, frames, *pages, p);
            row += &format!(" {:>7.1}%", 100.0 * st.hit_ratio());
            ratios.insert((wi, pn), st.hit_ratio());
        }
        println!("{row}");
    }
    let r = |w: usize, p: &str| ratios[&(w, p.to_string())];
    assert!(
        ["LRU", "CLOCK", "LRU-2"].iter().all(|p| r(0, p) > 0.99),
        "working set fits: everything hits"
    );
    assert!(
        ["LRU", "CLOCK", "LRU-2"].iter().all(|p| r(1, p) > 0.2),
        "zipf has locality"
    );
    assert!(
        r(2, "LRU-2") > r(2, "LRU") + 0.2,
        "LRU-2 must resist scan pollution (LRU {:.2}, LRU-2 {:.2})",
        r(2, "LRU"),
        r(2, "LRU-2")
    );
    assert!(
        r(2, "LRU-2") > r(2, "CLOCK") + 0.2,
        "CLOCK behaves like LRU under a scan"
    );
    assert!(r(1, "LRU-2") > r(1, "LRU"));
    println!("\nscan-resistant LRU-2 keeps the hot set; LRU and CLOCK are flushed by the scan.");
    println!("ok");
}
