//! RECIPE 855 (database): modern buffer management: pointer swizzling + cooling FIFO (LeanStore), page-state words + optimistic latches (vmcache)
//! LEVEL: advanced
//! Use in: storage engines and caches that must be as fast as an in-memory structure while the data is bigger than RAM.   Run: cargo run --release --example buffer_management_vmcache_swizzling
#![allow(dead_code)]
//!
//! `buffer_pool_clock` (recipe 173) is the CLASSIC design: every page access goes `page_id -> hash table -> frame`, then pins
//! the frame. With everything cached that translation (plus its latch and the pin counter, both written by every reader) is
//! pure overhead and a scalability bottleneck. Two modern answers:
//! PART A, POINTER SWIZZLING (LeanStore; also used by Umbra). A child reference in a B+tree node is a SWIP: one word that holds
//! EITHER the child's page id (unswizzled) OR a direct reference to the frame that caches it (swizzled; here a tagged frame
//! index, in a real engine a pointer). A hot traversal follows swips and never touches a hash table.
//!   * ONE PARENT: evicting a page means finding the swip that points at it and turning it back into a page id. So every page
//!     must have exactly ONE referencing swip and know where it is. Consequences: no B-link sibling pointers (range scans
//!     re-descend using the parent's separator as the next start key), and no general graphs or shared pages.
//!   * BOTTOM-UP: a page may only start leaving memory if none of its children is swizzled (otherwise its disk image would
//!     contain a memory address). Writing a swizzled swip to disk is asserted against.
//!   * COOLING STAGE instead of LRU bookkeeping on every hit: now and then pick a RANDOM hot page, UNSWIZZLE its parent's swip
//!     and append it to a FIFO. It stays resident. If it is touched again before reaching the end of the FIFO it is found in a
//!     small table (a "rescue": no I/O) and swizzled again, otherwise it is evicted. Hot pages pay nothing; replacement
//!     state is only updated for pages that are already suspected to be cold. It approximates LRU (second chance).
//! PART B, VMCACHE. Reserve a huge VIRTUAL region and use `base + pid * PAGE` as the pointer: the MMU does the translation, so
//! page ids work for any data structure (no one-parent rule). The DBMS still decides what is resident: one 64-bit STATE WORD
//! per page = 8 bits of state | 56 bits of VERSION; it faults a page in with `pread` and drops it with `madvise(DONTNEED)`.
//!   states: 0 Unlocked, 1..=252 Shared(n readers), 253 Locked (exclusive), 254 Marked (clock candidate), 255 Evicted.
//!   * OPTIMISTIC LATCH: a reader writes NOTHING. Read the word, read the data, read the word again; if it changed, retry. Every
//!     exclusive unlock and every eviction bumps the version, so a reader can never accept data that was modified or dropped.
//!   * CLOCK on the state word: first sweep Unlocked -> Marked; any access clears the mark; a page still Marked on the next
//!     visit is locked, written back if dirty, dropped, and published as Evicted with a new version.
//! WHAT IS SIMULATED: std has no mmap. The "virtual region" is an array with one slot per pid, `madvise` overwrites the slot
//! with POISON (so a reader that wrongly accepted it would fail its checksum), and the physical budget is `capacity` resident
//! slots. Page words are `AtomicU64` with relaxed accesses plus fences: that keeps the seqlock race free of undefined
//! behaviour in safe Rust (a plain `&[u8]` read racing a write would be UB). Not modelled: TLB shootdowns, the cost that
//! vmcache's `exmap` kernel module attacks.
//! VERIFICATION: Part A runs random lookups / updates / range scans against a `BTreeMap` oracle at several memory sizes, checking
//! all swizzling invariants, next to a hash-table + CLOCK pool over the same tree. Part B: deterministic interleavings (a write
//! or eviction in the middle of an optimistic read must be rejected; a shared-locked page must survive any eviction pressure),
//! then 5 runs of 8 threads: per-page checksums, no lost update, and no eviction of a locked page.
//! COMMON USES: LeanStore, Umbra (variable-size pages with swizzling), the vmcache/exmap research line, and the same ideas in
//! caches that swap object references between "id" and "pointer" (ORMs, object stores). Seqlocks: Linux kernel time reads.
//! Read: https://db.in.tum.de/~leis/papers/leanstore.pdf (Leis et al., "LeanStore: In-Memory Data Management beyond Main
//! Memory"), https://db.in.tum.de/~freitag/papers/p29-neumann-cidr20.pdf (Neumann, Freitag, "Umbra"),
//! https://www.cs.cit.tum.de/fileadmin/w00cfj/dis/_my_direct_uploads/vmcache.pdf (Leis et al., virtual-memory assisted buffer management).
//!
//! TRY NEXT:
//!   - [S] Set the cooling fraction to 1% and then 50% and predict rescues vs misses before running.
//!   - [S] Delete the re-check of the state word in `optimistic_read` and run: which deterministic test fails?
//!   - [M] Add leaf splits to Part A. Which swips and parent back-references move, and in which order must frames be allocated so none of the pages involved can be evicted mid-split?
//!   - [M] Add optimistic lock COUPLING over two vmcache pages (directory page -> data page): after reading the child, re-validate the parent. Write the interleaving that breaks without it.
//!   - [A] PROJECT: back Part B with real `mmap`/`madvise` through the ffi recipes and measure optimistic-read throughput against a `RwLock<HashMap>` page table at 1, 4 and 8 threads.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hint::black_box;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering, fence};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for _ in 0..4 {
            r.next();
        }
        r
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pm(&mut self, p: u64) -> bool {
        self.below(1000) < p
    }
}

// =============================================================================================== PART A: swizzled B+tree

const FANOUT: usize = 32;
const LEAF_CAP: usize = 32;
const TAG: u64 = 1 << 63;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Swip(u64);
enum Target {
    Frame(usize),
    Page(u64),
}
impl Swip {
    fn page(pid: u64) -> Swip {
        debug_assert!(pid & TAG == 0);
        Swip(pid)
    }
    fn frame(f: usize) -> Swip {
        Swip(f as u64 | TAG)
    }
    fn get(self) -> Target {
        if self.0 & TAG != 0 {
            Target::Frame((self.0 & !TAG) as usize)
        } else {
            Target::Page(self.0)
        }
    }
    fn is_swizzled(self) -> bool {
        self.0 & TAG != 0
    }
}

#[derive(Clone, Debug)]
enum Node {
    /// `keys[i]` is the smallest key reachable through `children[i + 1]`.
    Inner {
        keys: Vec<u64>,
        children: Vec<Swip>,
    },
    Leaf {
        keys: Vec<u64>,
        vals: Vec<u64>,
    },
}

struct Disk {
    pages: Vec<Node>,
    reads: u64,
    writes: u64,
}
impl Disk {
    fn read(&mut self, pid: u64) -> Node {
        self.reads += 1;
        self.pages[pid as usize].clone()
    }
    fn write(&mut self, pid: u64, n: &Node) {
        if let Node::Inner { children, .. } = n {
            assert!(
                children.iter().all(|c| !c.is_swizzled()),
                "a memory reference would be persisted in page {pid}"
            );
        }
        self.writes += 1;
        self.pages[pid as usize] = n.clone();
    }
}

/// Bulk load keys 0, 2, 4, .. (odd keys are guaranteed misses). Returns the disk and the root page id.
fn build_tree(n: u64) -> (Disk, u64) {
    let mut pages: Vec<Node> = vec![];
    let mut level: Vec<(u64, u64)> = vec![]; // (smallest key, pid)
    for chunk in (0..n).collect::<Vec<_>>().chunks(LEAF_CAP) {
        level.push((chunk[0] * 2, pages.len() as u64));
        pages.push(Node::Leaf {
            keys: chunk.iter().map(|i| i * 2).collect(),
            vals: chunk.iter().map(|i| i * 10).collect(),
        });
    }
    while level.len() > 1 {
        let mut up = vec![];
        for chunk in level.chunks(FANOUT) {
            up.push((chunk[0].0, pages.len() as u64));
            pages.push(Node::Inner {
                keys: chunk[1..].iter().map(|c| c.0).collect(),
                children: chunk.iter().map(|c| Swip::page(c.1)).collect(),
            });
        }
        level = up;
    }
    let root = level[0].1;
    (
        Disk {
            pages,
            reads: 0,
            writes: 0,
        },
        root,
    )
}

#[derive(Default, Clone, Copy, Debug)]
struct PoolStats {
    node_accesses: u64,
    hash_lookups: u64,
    misses: u64,
    evictions: u64,
    rescues: u64,
    swizzles: u64,
}

/// The two pools differ only in how a swip is turned into a frame; the tree code is shared.
trait Pool {
    fn root(&mut self) -> usize;
    /// Resolve child `slot` of the resident inner node in frame `parent`.
    fn child(&mut self, parent: usize, slot: usize) -> usize;
    fn node(&mut self, f: usize) -> &mut Node;
    fn mark_dirty(&mut self, f: usize);
    fn flush(&mut self);
    fn stats(&self) -> PoolStats;
    fn disk(&self) -> &Disk;
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum FState {
    Free,
    Hot,
    Cooling,
}
struct Frame {
    pid: u64,
    node: Node,
    state: FState,
    parent: Option<(usize, usize)>, // (frame, slot): the ONE swip that references this page. Valid only while Hot.
    dirty: bool,
    stamp: u64, // identifies the live FIFO entry of a cooling frame
}
impl Frame {
    fn empty() -> Frame {
        Frame {
            pid: 0,
            node: Node::Leaf {
                keys: vec![],
                vals: vec![],
            },
            state: FState::Free,
            parent: None,
            dirty: false,
            stamp: 0,
        }
    }
    fn has_swizzled_child(&self) -> bool {
        matches!(&self.node, Node::Inner { children, .. } if children.iter().any(|c| c.is_swizzled()))
    }
}

struct SwizzlePool {
    disk: Disk,
    frames: Vec<Frame>,
    free: Vec<usize>,
    root_pid: u64,
    root: Option<usize>, // the root swip lives in the tree handle; the root is never cooled
    fifo: VecDeque<(usize, u64)>,
    cooling: HashMap<u64, usize>, // the ONLY hash table, consulted only for unswizzled swips
    cool_target: usize,
    rng: Rng,
    next_stamp: u64,
    st: PoolStats,
}
impl SwizzlePool {
    fn new(disk: Disk, root_pid: u64, capacity: usize, cool_fraction: f64) -> SwizzlePool {
        assert!(
            capacity >= 8,
            "need room for a root-to-leaf path plus the cooling stage"
        );
        SwizzlePool {
            disk,
            frames: (0..capacity).map(|_| Frame::empty()).collect(),
            free: (0..capacity).rev().collect(),
            root_pid,
            root: None,
            fifo: VecDeque::new(),
            cooling: HashMap::new(),
            cool_target: ((capacity as f64 * cool_fraction) as usize).max(2),
            rng: Rng::new(7),
            next_stamp: 0,
            st: PoolStats::default(),
        }
    }
    fn set_swip(&mut self, parent: usize, slot: usize, s: Swip) {
        match &mut self.frames[parent].node {
            Node::Inner { children, .. } => children[slot] = s,
            Node::Leaf { .. } => unreachable!("leaves have no swips"),
        }
    }
    /// Move one random hot page into the cooling FIFO. `keep` is the node the traversal is standing on: it is PINNED.
    fn cool_one(&mut self, keep: Option<usize>) -> bool {
        let n = self.frames.len();
        let start = self.rng.below(n as u64) as usize;
        for probe in 0..n {
            let mut f = (start + probe) % n;
            if self.frames[f].state != FState::Hot {
                continue;
            }
            // a page with swizzled children cannot leave: walk down to a descendant that has none
            while self.frames[f].has_swizzled_child() {
                let Node::Inner { children, .. } = &self.frames[f].node else {
                    unreachable!()
                };
                let swz: Vec<usize> = children
                    .iter()
                    .filter_map(|c| match c.get() {
                        Target::Frame(x) => Some(x),
                        Target::Page(_) => None,
                    })
                    .collect();
                f = swz[self.rng.below(swz.len() as u64) as usize];
            }
            if Some(f) == keep || Some(f) == self.root {
                continue;
            }
            let (pf, slot) = self.frames[f]
                .parent
                .expect("a hot non-root page has a parent");
            let pid = self.frames[f].pid;
            self.set_swip(pf, slot, Swip::page(pid)); // UNSWIZZLE
            self.next_stamp += 1;
            let fr = &mut self.frames[f];
            fr.state = FState::Cooling;
            fr.parent = None; // the parent may now be evicted before we are
            fr.stamp = self.next_stamp;
            self.fifo.push_back((f, self.next_stamp));
            self.cooling.insert(pid, f);
            return true;
        }
        false
    }
    /// A free frame, evicting the oldest cooling page if necessary.
    fn allocate(&mut self, keep: Option<usize>) -> usize {
        // keep a supply of cooling pages once memory is full, so a victim has had its "second chance" window
        if self.free.is_empty() {
            while self.cooling.len() < self.cool_target && self.cool_one(keep) {}
        }
        if let Some(f) = self.free.pop() {
            return f;
        }
        while let Some((f, stamp)) = self.fifo.pop_front() {
            if self.frames[f].state != FState::Cooling || self.frames[f].stamp != stamp {
                continue; // stale entry: the page was rescued (and maybe cooled again later)
            }
            let pid = self.frames[f].pid;
            if self.frames[f].dirty {
                self.disk.write(pid, &self.frames[f].node);
            }
            self.cooling.remove(&pid);
            self.frames[f] = Frame::empty();
            self.st.evictions += 1;
            return f;
        }
        panic!("every frame is pinned");
    }
    fn load(&mut self, pid: u64, keep: Option<usize>) -> usize {
        let f = self.allocate(keep);
        self.st.misses += 1;
        self.frames[f] = Frame {
            pid,
            node: self.disk.read(pid),
            state: FState::Hot,
            parent: None,
            dirty: false,
            stamp: 0,
        };
        f
    }
    fn check_invariants(&self) {
        let mut refs = vec![0u32; self.frames.len()];
        let mut pids = HashMap::new();
        for (i, f) in self.frames.iter().enumerate() {
            if f.state == FState::Free {
                continue;
            }
            assert!(
                pids.insert(f.pid, i).is_none(),
                "page {} cached twice",
                f.pid
            );
            if f.state == FState::Cooling {
                assert_eq!(
                    self.cooling.get(&f.pid),
                    Some(&i),
                    "cooling page missing from the cooling table"
                );
                assert!(
                    !f.has_swizzled_child(),
                    "a cooling page holds a swizzled swip"
                );
            }
            if let Node::Inner { children, .. } = &f.node {
                for (slot, c) in children.iter().enumerate() {
                    if let Target::Frame(x) = c.get() {
                        assert_eq!(f.state, FState::Hot);
                        assert_eq!(
                            self.frames[x].state,
                            FState::Hot,
                            "swizzled swip to a non-hot frame (dangling)"
                        );
                        assert_eq!(
                            self.frames[x].parent,
                            Some((i, slot)),
                            "back-reference does not match the swip"
                        );
                        refs[x] += 1;
                    }
                }
            }
        }
        for (i, f) in self.frames.iter().enumerate() {
            let want = (f.state == FState::Hot && Some(i) != self.root) as u32;
            assert_eq!(
                refs[i], want,
                "frame {i} ({:?}) must be referenced by exactly {want} swizzled swips",
                f.state
            );
        }
        assert_eq!(
            self.cooling.len(),
            self.frames
                .iter()
                .filter(|f| f.state == FState::Cooling)
                .count()
        );
    }
}
impl Pool for SwizzlePool {
    fn root(&mut self) -> usize {
        self.st.node_accesses += 1;
        if let Some(r) = self.root {
            return r;
        }
        let f = self.load(self.root_pid, None);
        self.root = Some(f);
        f
    }
    fn child(&mut self, parent: usize, slot: usize) -> usize {
        self.st.node_accesses += 1;
        let Node::Inner { children, .. } = &self.frames[parent].node else {
            unreachable!()
        };
        let pid = match children[slot].get() {
            Target::Frame(f) => return f, // HOT PATH: no lookup, no bookkeeping, nothing written
            Target::Page(p) => p,
        };
        self.st.hash_lookups += 1;
        let f = match self.cooling.remove(&pid) {
            Some(f) => {
                self.st.rescues += 1; // second chance: still resident, no I/O
                self.frames[f].state = FState::Hot;
                f
            }
            None => self.load(pid, Some(parent)),
        };
        self.frames[f].parent = Some((parent, slot));
        self.set_swip(parent, slot, Swip::frame(f)); // SWIZZLE
        self.st.swizzles += 1;
        f
    }
    fn node(&mut self, f: usize) -> &mut Node {
        &mut self.frames[f].node
    }
    fn mark_dirty(&mut self, f: usize) {
        self.frames[f].dirty = true;
    }
    fn flush(&mut self) {
        for i in 0..self.frames.len() {
            if self.frames[i].state != FState::Free && self.frames[i].dirty {
                let mut image = self.frames[i].node.clone();
                if let Node::Inner { children, .. } = &mut image {
                    for c in children {
                        if let Target::Frame(x) = c.get() {
                            *c = Swip::page(self.frames[x].pid); // unswizzle the COPY that goes to disk
                        }
                    }
                }
                self.disk.write(self.frames[i].pid, &image);
                self.frames[i].dirty = false;
            }
        }
    }
    fn stats(&self) -> PoolStats {
        self.st
    }
    fn disk(&self) -> &Disk {
        &self.disk
    }
}

/// Baseline: page table (hash map) on EVERY node access plus a CLOCK reference bit written on every hit.
struct HashPool {
    disk: Disk,
    frames: Vec<(Option<u64>, Node, bool, bool)>, // (pid, node, referenced, dirty)
    table: HashMap<u64, usize>,
    hand: usize,
    root_pid: u64,
    st: PoolStats,
}
impl HashPool {
    fn new(disk: Disk, root_pid: u64, capacity: usize) -> HashPool {
        let frames = (0..capacity)
            .map(|_| {
                (
                    None,
                    Node::Leaf {
                        keys: vec![],
                        vals: vec![],
                    },
                    false,
                    false,
                )
            })
            .collect();
        HashPool {
            disk,
            frames,
            table: HashMap::new(),
            hand: 0,
            root_pid,
            st: PoolStats::default(),
        }
    }
    fn fix(&mut self, pid: u64, keep: Option<usize>) -> usize {
        self.st.node_accesses += 1;
        self.st.hash_lookups += 1;
        if let Some(&f) = self.table.get(&pid) {
            self.frames[f].2 = true;
            return f;
        }
        self.st.misses += 1;
        let f = loop {
            let f = self.hand;
            self.hand = (self.hand + 1) % self.frames.len();
            if Some(f) == keep {
                continue;
            }
            if self.frames[f].0.is_some() && std::mem::take(&mut self.frames[f].2) {
                continue; // referenced: second chance
            }
            break f;
        };
        if let Some(old) = self.frames[f].0 {
            if self.frames[f].3 {
                self.disk.write(old, &self.frames[f].1);
            }
            self.table.remove(&old);
            self.st.evictions += 1;
        }
        self.frames[f] = (Some(pid), self.disk.read(pid), true, false);
        self.table.insert(pid, f);
        f
    }
}
impl Pool for HashPool {
    fn root(&mut self) -> usize {
        self.fix(self.root_pid, None)
    }
    fn child(&mut self, parent: usize, slot: usize) -> usize {
        let Node::Inner { children, .. } = &self.frames[parent].1 else {
            unreachable!()
        };
        let Target::Page(pid) = children[slot].get() else {
            unreachable!("this pool never swizzles")
        };
        self.fix(pid, Some(parent))
    }
    fn node(&mut self, f: usize) -> &mut Node {
        &mut self.frames[f].1
    }
    fn mark_dirty(&mut self, f: usize) {
        self.frames[f].3 = true;
    }
    fn flush(&mut self) {
        for f in &mut self.frames {
            if let (Some(pid), true) = (f.0, f.3) {
                self.disk.write(pid, &f.1);
                f.3 = false;
            }
        }
    }
    fn stats(&self) -> PoolStats {
        self.st
    }
    fn disk(&self) -> &Disk {
        &self.disk
    }
}

/// Descend to the leaf that covers `key`. Also returns the smallest key of the NEXT leaf (the upper fence), which replaces
/// sibling pointers in range scans.
fn find_leaf(p: &mut dyn Pool, key: u64) -> (usize, Option<u64>) {
    let mut f = p.root();
    let mut fence_key = None;
    loop {
        let slot = match p.node(f) {
            Node::Leaf { .. } => return (f, fence_key),
            Node::Inner { keys, .. } => {
                let slot = keys.partition_point(|&k| k <= key);
                if slot < keys.len() {
                    fence_key = Some(keys[slot]); // deeper levels give tighter fences
                }
                slot
            }
        };
        f = p.child(f, slot);
    }
}
fn get(p: &mut dyn Pool, key: u64) -> Option<u64> {
    let (f, _) = find_leaf(p, key);
    let Node::Leaf { keys, vals } = p.node(f) else {
        unreachable!()
    };
    keys.binary_search(&key).ok().map(|i| vals[i])
}
fn update(p: &mut dyn Pool, key: u64, v: u64) -> bool {
    let (f, _) = find_leaf(p, key);
    let Node::Leaf { keys, vals } = p.node(f) else {
        unreachable!()
    };
    let Ok(i) = keys.binary_search(&key) else {
        return false;
    };
    vals[i] = v;
    p.mark_dirty(f);
    true
}
fn scan(p: &mut dyn Pool, from: u64, limit: usize) -> Vec<(u64, u64)> {
    let mut out = vec![];
    let mut start = from;
    loop {
        let (f, next) = find_leaf(p, start);
        let Node::Leaf { keys, vals } = p.node(f) else {
            unreachable!()
        };
        let i = keys.partition_point(|&k| k < start);
        out.extend(
            keys[i..]
                .iter()
                .copied()
                .zip(vals[i..].iter().copied())
                .take(limit - out.len()),
        );
        match next {
            Some(n) if out.len() < limit => start = n, // re-descend from the root
            _ => return out,
        }
    }
}

/// 90% of point operations go to 10% of the key range.
fn skewed_key(rng: &mut Rng, n: u64) -> u64 {
    let i = if rng.pm(900) {
        rng.below(n / 10) + n / 3
    } else {
        rng.below(n)
    };
    i * 2 + rng.pm(50) as u64 // 5% odd keys: not present
}

fn tree_workload(
    p: &mut dyn Pool,
    oracle: &mut BTreeMap<u64, u64>,
    n: u64,
    ops: u64,
    seed: u64,
    mut check: impl FnMut(&mut dyn Pool, u64),
) {
    let mut rng = Rng::new(seed);
    for op in 0..ops {
        match rng.below(100) {
            0..=74 => {
                let k = skewed_key(&mut rng, n);
                assert_eq!(get(p, k), oracle.get(&k).copied(), "op {op}: get({k})");
            }
            75..=97 => {
                let k = skewed_key(&mut rng, n);
                let v = rng.next();
                let hit = update(p, k, v);
                assert_eq!(hit, oracle.contains_key(&k));
                if hit {
                    oracle.insert(k, v);
                }
            }
            _ => {
                let from = rng.below(2 * n);
                let limit = 1 + rng.below(100) as usize;
                let want: Vec<(u64, u64)> = oracle
                    .range(from..)
                    .take(limit)
                    .map(|(a, b)| (*a, *b))
                    .collect();
                assert_eq!(scan(p, from, limit), want, "op {op}: scan({from}, {limit})");
            }
        }
        check(p, op);
    }
}
/// After a flush the DISK alone must contain exactly the oracle (no lost write-back, no swizzled swip).
fn check_disk(d: &Disk, root: u64, oracle: &BTreeMap<u64, u64>) {
    let mut got = vec![];
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        match &d.pages[pid as usize] {
            Node::Leaf { keys, vals } => got.extend(keys.iter().copied().zip(vals.iter().copied())),
            Node::Inner { children, .. } => {
                stack.extend(children.iter().rev().map(|c| match c.get() {
                    Target::Page(p) => p,
                    Target::Frame(_) => panic!("swizzled swip on disk"),
                }))
            }
        }
    }
    assert!(
        got.into_iter().eq(oracle.iter().map(|(a, b)| (*a, *b))),
        "disk image differs from the oracle after flush"
    );
}
fn initial_oracle(n: u64) -> BTreeMap<u64, u64> {
    (0..n).map(|i| (i * 2, i * 10)).collect()
}

fn part_a_correctness() {
    // tiny pools force constant eviction; invariants are checked after EVERY operation
    for seed in 1..=40 {
        let mut rng = Rng::new(seed ^ 0xABC);
        let n = 600 + rng.below(3000);
        let capacity = 8 + rng.below(40) as usize;
        let cool = [0.05, 0.2, 0.6][rng.below(3) as usize];
        let (disk, root) = build_tree(n);
        let mut sp = SwizzlePool::new(disk, root, capacity, cool);
        let mut oracle = initial_oracle(n);
        let mut rng_ops = Rng::new(seed);
        for _ in 0..400 {
            tree_workload(&mut sp, &mut oracle, n, 1, rng_ops.next(), |_, _| {});
            sp.check_invariants();
        }
        sp.flush();
        sp.check_invariants();
        check_disk(sp.disk(), root, &oracle);
    }
    println!(
        "A: 40 x 400 operations on tiny pools: results == BTreeMap, swizzling invariants hold after every operation, flushed disk == oracle"
    );
}

fn part_a_comparison() {
    const N: u64 = 200_000;
    const OPS: u64 = 200_000;
    let total = build_tree(N).0.pages.len();
    println!(
        "\nA: page-table lookups vs swizzling, {N} keys in {total} pages, {OPS} skewed ops (90% on 10% of keys, 2% range scans)"
    );
    println!(
        "   memory   pool       lookups/node access   misses   evictions   rescues   disk writes"
    );
    for percent in [5usize, 20, 50, 110] {
        let capacity = total * percent / 100;
        let mut rows = vec![];
        for swizzle in [false, true] {
            let (disk, root) = build_tree(N);
            let mut oracle = initial_oracle(N);
            let mut pool: Box<dyn Pool> = if swizzle {
                Box::new(SwizzlePool::new(disk, root, capacity, 0.1))
            } else {
                Box::new(HashPool::new(disk, root, capacity))
            };
            tree_workload(pool.as_mut(), &mut oracle, N, OPS, 99, |_, _| {});
            pool.flush();
            check_disk(pool.disk(), root, &oracle);
            let s = pool.stats();
            let name = if swizzle { "swizzle" } else { "hash+clock" };
            println!(
                "   {percent:>4}%    {name:<10} {:>19.3} {:>8} {:>11} {:>9} {:>13}",
                s.hash_lookups as f64 / s.node_accesses as f64,
                s.misses,
                s.evictions,
                s.rescues,
                pool.disk().writes
            );
            rows.push(s);
        }
        let (h, s) = (rows[0], rows[1]);
        assert_eq!(h.node_accesses, s.node_accesses, "same logical traversals");
        assert_eq!(h.hash_lookups, h.node_accesses);
        assert!(
            s.hash_lookups < h.hash_lookups / 2,
            "most traversals must stay on swizzled swips"
        );
        assert!(
            s.misses < h.misses * 3 / 2 + 100,
            "random cooling + FIFO must stay close to CLOCK"
        );
        if percent == 110 {
            assert_eq!(s.evictions, 0);
            assert_eq!(
                s.hash_lookups,
                s.misses - 1,
                "fully cached: only first touches take the slow path (the root has no lookup)"
            );
        }
    }
    // wall clock, fully cached, point reads only (printed, never asserted)
    let (d1, r1) = build_tree(N);
    let (d2, r2) = build_tree(N);
    let mut pools: [(&str, Box<dyn Pool>); 2] = [
        ("hash+clock", Box::new(HashPool::new(d1, r1, total + 8))),
        (
            "swizzle",
            Box::new(SwizzlePool::new(d2, r2, total + 8, 0.1)),
        ),
    ];
    for (name, p) in &mut pools {
        for k in 0..N {
            black_box(get(p.as_mut(), k * 2)); // warm up
        }
        let mut rng = Rng::new(3);
        let t = Instant::now();
        for _ in 0..1_000_000 {
            black_box(get(p.as_mut(), rng.below(N) * 2));
        }
        println!(
            "   fully cached point lookups, {name:<10}: {:.0} ns/op (timing, informational; std HashMap uses SipHash)",
            t.elapsed().as_nanos() as f64 / 1e6
        );
    }
}

// =============================================================================================== PART B: vmcache

const WORDS: usize = 16;
const POISON: u64 = 0xDEAD_DEAD_DEAD_DEAD;
const MAX_SHARED: u64 = 252;
const LOCKED: u64 = 253;
const MARKED: u64 = 254;
const EVICTED: u64 = 255;

fn state_of(w: u64) -> u64 {
    w >> 56
}
fn version_of(w: u64) -> u64 {
    w & ((1 << 56) - 1)
}
fn word(state: u64, version: u64) -> u64 {
    state << 56 | version
}

type PageData = [u64; WORDS];
fn checksum(d: &PageData) -> u64 {
    d[..WORDS - 1].iter().fold(0xcbf2_9ce4_8422_2325, |h, &x| {
        (h ^ x).wrapping_mul(0x100_0000_01b3)
    })
}
/// [pid, update counter, 13 words derived from both, checksum]
fn make_page(pid: u64, counter: u64) -> PageData {
    let mut d = [0u64; WORDS];
    d[0] = pid;
    d[1] = counter;
    for i in 2..WORDS - 1 {
        d[i] = (pid ^ counter.rotate_left(i as u32)).wrapping_mul(0x9E37_79B9_7F4A_7C15 + i as u64);
    }
    d[WORDS - 1] = checksum(&d);
    d
}
fn verify_page(pid: u64, d: &PageData) -> u64 {
    assert_eq!(
        *d,
        make_page(pid, d[1]),
        "page {pid}: torn, stale-slot or poisoned content"
    );
    d[1]
}

#[derive(Default)]
struct VmStats {
    faults: AtomicU64,
    evictions: AtomicU64,
    writebacks: AtomicU64,
    optimistic_ok: AtomicU64,
    optimistic_retry: AtomicU64,
}
struct VmCache {
    state: Vec<AtomicU64>,
    mem: Vec<[AtomicU64; WORDS]>, // the "virtual region": slot i is the address of page i
    dirty: Vec<AtomicU64>,
    disk: Vec<Mutex<PageData>>,
    resident: Vec<AtomicUsize>, // PHYSICAL budget: slot holds pid + 1, or 0. vmcache uses a lock-free hash set here.
    hand: AtomicUsize,
    shared_holders: Vec<AtomicUsize>, // instrumentation only: who is really reading under a shared latch
    st: VmStats,
}
impl VmCache {
    fn new(pages: usize, capacity: usize) -> VmCache {
        VmCache {
            state: (0..pages)
                .map(|_| AtomicU64::new(word(EVICTED, 0)))
                .collect(),
            mem: (0..pages)
                .map(|_| std::array::from_fn(|_| AtomicU64::new(POISON)))
                .collect(),
            dirty: (0..pages).map(|_| AtomicU64::new(0)).collect(),
            disk: (0..pages)
                .map(|p| Mutex::new(make_page(p as u64, 0)))
                .collect(),
            resident: (0..capacity).map(|_| AtomicUsize::new(0)).collect(),
            hand: AtomicUsize::new(0),
            shared_holders: (0..pages).map(|_| AtomicUsize::new(0)).collect(),
            st: VmStats::default(),
        }
    }
    fn copy_out(&self, pid: usize) -> PageData {
        std::array::from_fn(|i| self.mem[pid][i].load(Ordering::Relaxed))
    }
    fn copy_in(&self, pid: usize, d: &PageData) {
        for (dst, &v) in self.mem[pid].iter().zip(d) {
            dst.store(v, Ordering::Relaxed);
        }
    }
    fn cas(&self, pid: usize, old: u64, new: u64) -> bool {
        let ok = self.state[pid]
            .compare_exchange(old, new, Ordering::Acquire, Ordering::Relaxed)
            .is_ok();
        if ok && state_of(new) == LOCKED {
            fence(Ordering::Release); // data stores made under the latch must not be seen before the Locked word
        }
        ok
    }
    /// Exclusive latch; faults the page in if necessary.
    fn lock_exclusive(&self, pid: usize) {
        let mut spins = 0u32;
        loop {
            let w = self.state[pid].load(Ordering::Relaxed);
            match state_of(w) {
                0 | MARKED if self.cas(pid, w, word(LOCKED, version_of(w))) => return,
                EVICTED if self.cas(pid, w, word(LOCKED, version_of(w))) => {
                    self.fault_in(pid);
                    return;
                }
                _ => backoff(&mut spins),
            }
        }
    }
    fn unlock_exclusive(&self, pid: usize) {
        let w = self.state[pid].load(Ordering::Relaxed);
        assert_eq!(state_of(w), LOCKED);
        self.state[pid].store(word(0, version_of(w) + 1), Ordering::Release); // NEW VERSION
    }
    /// Caller holds the exclusive latch on `pid`, whose memory is currently not backed.
    fn fault_in(&self, pid: usize) {
        self.claim_physical_slot(pid);
        let d = *self.disk[pid].lock().unwrap(); // pread
        self.copy_in(pid, &d);
        self.dirty[pid].store(0, Ordering::Relaxed);
        self.st.faults.fetch_add(1, Ordering::Relaxed);
    }
    /// Take a free physical slot, or evict somebody and take theirs. This bounds the resident set by construction.
    fn claim_physical_slot(&self, pid: usize) {
        let mut spins = 0u32;
        loop {
            let i = self.hand.fetch_add(1, Ordering::Relaxed) % self.resident.len();
            let occupant = self.resident[i].load(Ordering::Acquire);
            if occupant == 0 {
                if self.resident[i]
                    .compare_exchange(0, pid + 1, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    return;
                }
            } else if self.clock_visit(i, occupant - 1, pid + 1) {
                return;
            }
            if i == 0 {
                backoff(&mut spins);
            }
        }
    }
    /// One clock step on the page in physical slot `i`. Returns true if it was evicted and the slot now holds `replacement`.
    fn clock_visit(&self, i: usize, victim: usize, replacement: usize) -> bool {
        let w = self.state[victim].load(Ordering::Relaxed);
        match state_of(w) {
            0 => {
                self.cas(victim, w, word(MARKED, version_of(w))); // first visit: same version, optimistic readers unaffected
                false
            }
            MARKED => {
                if !self.cas(victim, w, word(LOCKED, version_of(w))) {
                    return false;
                }
                // `occupant` was read before we owned the page: it may have been evicted and faulted into another slot since
                if self.resident[i].load(Ordering::Acquire) != victim + 1 {
                    self.state[victim].store(word(MARKED, version_of(w)), Ordering::Release);
                    return false;
                }
                assert_eq!(
                    self.shared_holders[victim].load(Ordering::SeqCst),
                    0,
                    "evicting page {victim} while it is latched"
                );
                if self.dirty[victim].swap(0, Ordering::Relaxed) != 0 {
                    *self.disk[victim].lock().unwrap() = self.copy_out(victim);
                    self.st.writebacks.fetch_add(1, Ordering::Relaxed);
                }
                self.copy_in(victim, &[POISON; WORDS]); // madvise(DONTNEED)
                self.resident[i].store(replacement, Ordering::Release);
                self.state[victim].store(word(EVICTED, version_of(w) + 1), Ordering::Release);
                self.st.evictions.fetch_add(1, Ordering::Relaxed);
                true
            }
            _ => false, // shared, locked or already on its way out: not a candidate
        }
    }
    fn write(&self, pid: usize, f: impl FnOnce(&mut PageData)) {
        self.lock_exclusive(pid);
        let mut d = self.copy_out(pid);
        f(&mut d);
        self.copy_in(pid, &d);
        self.dirty[pid].store(1, Ordering::Relaxed);
        self.unlock_exclusive(pid);
    }
    fn increment(&self, pid: usize) {
        self.write(pid, |d| {
            let c = verify_page(pid as u64, d);
            *d = make_page(pid as u64, c + 1);
        });
    }
    /// One optimistic attempt. `between` runs after the data was copied and before validation (a hook for tests).
    /// The copy may be garbage; it is returned only if the state word proves nothing happened meanwhile.
    fn try_optimistic(&self, pid: usize, between: impl FnOnce()) -> Option<PageData> {
        let before = self.state[pid].load(Ordering::Acquire);
        match state_of(before) {
            LOCKED | EVICTED => return None,
            MARKED => {
                self.cas(pid, before, word(0, version_of(before))); // "I used it": clears the clock mark
                return None;
            }
            _ => {}
        }
        let copy = self.copy_out(pid);
        between();
        fence(Ordering::Acquire); // the data loads happen before the re-check
        let after = self.state[pid].load(Ordering::Relaxed);
        // shared latches change the state byte but not the data, so only version + "not locked / evicted" matter
        (version_of(after) == version_of(before) && !matches!(state_of(after), LOCKED | EVICTED))
            .then_some(copy)
    }
    fn optimistic_read(&self, pid: usize) -> PageData {
        let mut spins = 0u32;
        loop {
            if let Some(d) = self.try_optimistic(pid, || {}) {
                self.st.optimistic_ok.fetch_add(1, Ordering::Relaxed);
                return d;
            }
            self.st.optimistic_retry.fetch_add(1, Ordering::Relaxed);
            if state_of(self.state[pid].load(Ordering::Relaxed)) == EVICTED {
                self.lock_exclusive(pid); // fault it in
                self.unlock_exclusive(pid);
            } else {
                backoff(&mut spins);
            }
        }
    }
    fn lock_shared(&self, pid: usize) {
        let mut spins = 0u32;
        loop {
            let w = self.state[pid].load(Ordering::Relaxed);
            match state_of(w) {
                s @ 0..MAX_SHARED if self.cas(pid, w, word(s + 1, version_of(w))) => break,
                MARKED if self.cas(pid, w, word(1, version_of(w))) => break,
                EVICTED => {
                    self.lock_exclusive(pid);
                    self.unlock_exclusive(pid);
                }
                _ => backoff(&mut spins),
            }
        }
        self.shared_holders[pid].fetch_add(1, Ordering::SeqCst);
    }
    fn unlock_shared(&self, pid: usize) {
        self.shared_holders[pid].fetch_sub(1, Ordering::SeqCst);
        loop {
            let w = self.state[pid].load(Ordering::Relaxed);
            assert!(
                (1..=MAX_SHARED).contains(&state_of(w)),
                "unlock_shared in state {}",
                state_of(w)
            );
            if self.state[pid]
                .compare_exchange(
                    w,
                    word(state_of(w) - 1, version_of(w)),
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return;
            }
        }
    }
    fn resident_pages(&self) -> Vec<usize> {
        self.resident
            .iter()
            .filter_map(|s| s.load(Ordering::SeqCst).checked_sub(1))
            .collect()
    }
    /// Quiescent check, then write every dirty page back.
    fn check_and_flush(&self) {
        let mut res = self.resident_pages();
        res.sort_unstable();
        let by_state: Vec<usize> = (0..self.state.len())
            .filter(|&p| state_of(self.state[p].load(Ordering::SeqCst)) != EVICTED)
            .collect();
        assert_eq!(res, by_state, "physical slots and page states disagree");
        for p in 0..self.state.len() {
            let s = state_of(self.state[p].load(Ordering::SeqCst));
            assert!(
                s == 0 || s == MARKED || s == EVICTED,
                "page {p} left latched (state {s})"
            );
            if s != EVICTED && self.dirty[p].swap(0, Ordering::SeqCst) != 0 {
                *self.disk[p].lock().unwrap() = self.copy_out(p);
            }
        }
    }
}
fn backoff(spins: &mut u32) {
    *spins += 1;
    if *spins < 16 {
        std::hint::spin_loop();
    } else {
        std::thread::yield_now();
    }
}

/// Touch other pages until `pid` has been evicted (or give up after `rounds` faults).
fn pressure(vm: &VmCache, avoid: usize, rounds: usize) {
    for k in 0..rounds {
        let other = k % vm.state.len();
        if other != avoid {
            vm.lock_exclusive(other);
            vm.unlock_exclusive(other);
        }
    }
}

fn part_b_deterministic() {
    let vm = VmCache::new(32, 4);
    vm.increment(5);
    // 1. quiet page: the optimistic read succeeds and writes nothing to the state word
    let w = vm.state[5].load(Ordering::SeqCst);
    assert_eq!(
        verify_page(
            5,
            &vm.try_optimistic(5, || {}).expect("no concurrent change")
        ),
        1
    );
    assert_eq!(vm.state[5].load(Ordering::SeqCst), w);
    // 2. a writer in the middle of the read
    assert!(
        vm.try_optimistic(5, || vm.increment(5)).is_none(),
        "a concurrent write must invalidate the read"
    );
    // 3. eviction in the middle of the read (the copy may be poisoned)
    let r = vm.try_optimistic(5, || {
        pressure(&vm, 5, 200);
        assert_eq!(state_of(vm.state[5].load(Ordering::SeqCst)), EVICTED);
    });
    assert!(r.is_none(), "eviction must invalidate the read");
    // 4. ABA: evicted AND faulted back in during the read: the state looks the same but the version moved
    vm.optimistic_read(5);
    let r = vm.try_optimistic(5, || {
        pressure(&vm, 5, 200);
        vm.lock_exclusive(5);
        vm.unlock_exclusive(5);
        assert_eq!(state_of(vm.state[5].load(Ordering::SeqCst)), 0);
    });
    assert!(r.is_none(), "the version must catch evict + reload");
    // 5. a shared latch pins the page against any pressure; releasing it makes the page evictable again
    vm.lock_shared(5);
    vm.lock_shared(5);
    pressure(&vm, 5, 500);
    assert_eq!(state_of(vm.state[5].load(Ordering::SeqCst)), 2);
    assert_eq!(verify_page(5, &vm.copy_out(5)), 2);
    vm.unlock_shared(5);
    pressure(&vm, 5, 500);
    assert!(
        vm.resident_pages().contains(&5),
        "one reader left: still pinned"
    );
    vm.unlock_shared(5);
    pressure(&vm, 5, 500);
    assert!(!vm.resident_pages().contains(&5));
    assert_eq!(
        verify_page(5, &vm.optimistic_read(5)),
        2,
        "the update survived write-back and reload"
    );
    println!(
        "\nB: deterministic interleavings OK: write / eviction / evict+reload during an optimistic read are all rejected; shared latches pin"
    );
}

fn part_b_stress() {
    const PAGES: usize = 512;
    const CAPACITY: usize = 64;
    const THREADS: usize = 8;
    const OPS: usize = 40_000;
    println!("B: stress, {THREADS} threads x {OPS} ops, {PAGES} pages, {CAPACITY} physical slots");
    for run in 1..=5u64 {
        let vm = VmCache::new(PAGES, CAPACITY);
        let expected: Vec<AtomicU64> = (0..PAGES).map(|_| AtomicU64::new(0)).collect();
        std::thread::scope(|s| {
            for t in 0..THREADS {
                let (vm, expected) = (&vm, &expected);
                s.spawn(move || {
                    let mut rng = Rng::new(run * 100 + t as u64);
                    // per-page counters only grow, so a thread must never see one go backwards (a stale reload would)
                    let mut seen = vec![0u64; PAGES];
                    for _ in 0..OPS {
                        let pid = if rng.pm(700) {
                            rng.below(24)
                        } else {
                            rng.below(PAGES as u64)
                        } as usize;
                        let c = match rng.below(100) {
                            0..=29 => {
                                vm.increment(pid);
                                expected[pid].fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            30..=79 => verify_page(pid as u64, &vm.optimistic_read(pid)),
                            _ => {
                                vm.lock_shared(pid);
                                let a = vm.copy_out(pid);
                                if rng.pm(100) {
                                    std::thread::yield_now(); // hold the latch while others run the clock
                                }
                                let b = vm.copy_out(pid);
                                vm.unlock_shared(pid);
                                assert_eq!(a, b, "content changed under a shared latch");
                                verify_page(pid as u64, &a)
                            }
                        };
                        assert!(
                            c >= seen[pid],
                            "page {pid} went back in time: {c} < {}",
                            seen[pid]
                        );
                        seen[pid] = c;
                    }
                });
            }
        });
        vm.check_and_flush();
        let mut total = 0;
        for p in 0..PAGES {
            let c = verify_page(p as u64, &vm.disk[p].lock().unwrap());
            assert_eq!(
                c,
                expected[p].load(Ordering::SeqCst),
                "run {run}: lost or duplicated update on page {p}"
            );
            total += c;
        }
        let g = |a: &AtomicU64| a.load(Ordering::SeqCst);
        assert!(
            g(&vm.st.evictions) > 1000 && total > 0,
            "the stress must actually evict"
        );
        // thread-schedule dependent counters: informational only
        println!(
            "   run {run}: {total} updates intact; faults {} evictions {} write-backs {} optimistic ok {} retries {}",
            g(&vm.st.faults),
            g(&vm.st.evictions),
            g(&vm.st.writebacks),
            g(&vm.st.optimistic_ok),
            g(&vm.st.optimistic_retry)
        );
    }
}

fn main() {
    part_a_correctness();
    part_a_comparison();
    part_b_deterministic();
    part_b_stress();
}
