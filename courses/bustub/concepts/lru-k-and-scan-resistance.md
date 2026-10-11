---
title: LRU-K: backward k-distance and scan resistance
summary: The idea in the 1993 LRU-K paper, how to read backward k-distance, why one extra bit of history defeats the sequential scan, and the tie-break the stages test.
minutes: 9
---
LRU judges a page by one number: when it was last used. That one number cannot tell a page used a thousand times from a page used once, and a sequential scan is nothing but pages used once. **LRU-K** (O'Neil, O'Neil and Weikum, SIGMOD 1993) remembers the last **K** uses of every page and judges by the K-th most recent one.

## Backward k-distance

At time *t*, the **backward k-distance** of a page is *how far back its K-th most recent access was*. A page with fewer than K recorded accesses has **infinite** backward k-distance. LRU-K evicts the page with the **largest** backward k-distance.

For K = 2, with the access history (time → frame): `t0:A t1:B t2:A t3:C t4:B`, evaluated at t = 5:

| frame | accesses | 2nd most recent | backward 2-distance |
|---|---|---|---|
| A | t0, t2 | t0 | 5 - 0 = 5 |
| B | t1, t4 | t1 | 5 - 1 = 4 |
| C | t3 only | none | **infinity** |

C has been used once, so it goes first. Between A and B, A's second-most-recent use is older, so A goes next. Plain LRU would have evicted A first too, but for a different reason (last used at t2, the oldest), and the two policies diverge as soon as a hot page and a scanned page have the same recency.

```svg
caption: Backward 2-distances at t = 5. C was used once, so its distance is infinite and it is evicted first; between A and B the larger distance (A, whose second most recent use is oldest) goes next.
<svg viewBox="0 0 760 230" role="img" aria-label="A timeline of accesses to frames A, B and C and their backward 2-distances">
<line class="grid" x1="40" y1="130" x2="720" y2="130"/>
<text class="dim sm" x="40" y="26">time &#8594;</text>
<g>
<circle class="box" cx="80" cy="130" r="20"/><text class="mid fg" x="80" y="135">A</text><text class="mid dim sm" x="80" y="166">t0</text>
<circle class="box" cx="200" cy="130" r="20"/><text class="mid fg" x="200" y="135">B</text><text class="mid dim sm" x="200" y="166">t1</text>
<circle class="box" cx="320" cy="130" r="20"/><text class="mid fg" x="320" y="135">A</text><text class="mid dim sm" x="320" y="166">t2</text>
<circle class="hot" cx="440" cy="130" r="20"/><text class="mid t-a" x="440" y="135">C</text><text class="mid dim sm" x="440" y="166">t3</text>
<circle class="box" cx="560" cy="130" r="20"/><text class="mid fg" x="560" y="135">B</text><text class="mid dim sm" x="560" y="166">t4</text>
<line class="ln-w dash" x1="660" y1="50" x2="660" y2="190"/><text class="t-w sm" x="668" y="46">now (t5)</text>
</g>
<path class="ln-b" d="M80 100 V70 H660" /><text class="t-b sm" x="90" y="64">A: 2nd most recent is t0, distance 5</text>
<path class="ln-b" d="M200 100 V88 H660" /><text class="t-b sm" x="210" y="82">B: 2nd most recent is t1, distance 4</text>
<text class="t-a sm" x="440" y="200">C: one access: distance infinity: evicted first</text>
</svg>
```

## Why it resists scans

A scan touches each of a million pages **once**, so each scanned page has fewer than 2 accesses and infinite distance. A hot page touched twice or more has a *finite* distance. Eviction always prefers infinite to finite, so scanned pages leave before hot ones, however recently the scan touched them. Under LRU the scan flushes the pool; under LRU-2 the hot set survives.

## The tie-break among infinite distances

Many pages can have infinite distance at once (every one-touch page). The paper says to break ties by plain LRU among them, and BusTub specifies it as: **among frames with fewer than K accesses, evict the one whose earliest recorded access is oldest.** A frame with K or more accesses is ordered by its K-th most recent access (older is larger). That gives a total order; add the frame id as a last tie-break and it is deterministic.

```rust
fn eviction_key(node: &LruKNode) -> (u8, usize, FrameId) {
    match node.kth_timestamp() {
        None    => (0, node.first_timestamp().unwrap_or(0), node.fid),   // infinite: first, oldest first access first
        Some(t) => (1, t, node.fid),                                      // finite: after them, oldest k-th access first
    }
}
```

The smallest key is the next victim: a tuple compares lexicographically, so `0` sorts before `1`.

## What to remember about the history

Only the **last K** timestamps matter, so each node keeps a `VecDeque<usize>` and pops the front when it grows past K. Memory is O(K) per frame, not O(accesses). The paper adds a refinement, the **correlated reference period** (ignore re-references within a short window, so a burst of accesses by one transaction counts once); BusTub does not require it.

## Choosing K

| K | behaviour |
|---|---|
| 1 | exactly LRU |
| 2 | the usual choice: distinguishes "once" from "more than once", little memory, adapts quickly |
| large | slower to adapt to a change of working set; remembers more |

> [!NOTE] What LRU-K does not fix
> A page referenced twice by a scan (a join that revisits a block) looks hot. And LRU-K's tuning knob K is fixed: ARC (module 1e) adapts to the workload without one.

## Invariants: what must always be true

- **Fewer than K accesses means infinite distance,** and an infinite-distance frame is evicted before any frame with K or more accesses.
- **The victim order is total.** Among infinite frames the oldest first access goes first; among finite ones the oldest K-th most recent access; the frame id is the last tie-break. Two runs over the same calls evict the same frames.
- **Only the last K timestamps are kept,** so memory per frame is O(K), however many accesses it has.
- **A frame that is not evictable is never chosen.** Pinned frames are outside the competition until they are unpinned.
- **Timestamps come from a logical clock that only increases,** so "older" is always well defined.

## In real code

### Using it: a complete, runnable LRU-K

The whole policy in about forty lines. The one trick is that `history.front()` is *both* the earliest access (when the frame has fewer than K) and the K-th most recent (when it has exactly K), so one expression gives the key for both cases.

```rust test
use std::collections::{BTreeSet, HashMap, VecDeque};

type FrameId = u32;

struct Node { history: VecDeque<usize>, evictable: bool }

struct LruK {
    k: usize,
    clock: usize,                                   // the logical clock: one tick per recorded access
    nodes: HashMap<FrameId, Node>,
    order: BTreeSet<(u8, usize, FrameId)>,          // evictable frames only; the smallest key is the victim
}

impl LruK {
    fn new(k: usize) -> Self { LruK { k, clock: 0, nodes: HashMap::new(), order: BTreeSet::new() } }

    fn key(&self, fid: FrameId) -> (u8, usize, FrameId) {
        let n = &self.nodes[&fid];
        let infinite = n.history.len() < self.k;                 // fewer than K accesses: infinite backward k-distance
        (if infinite { 0 } else { 1 }, *n.history.front().unwrap(), fid)
    }

    fn record_access(&mut self, fid: FrameId) {
        let now = self.clock;
        self.clock += 1;
        self.nodes.entry(fid).or_insert(Node { history: VecDeque::new(), evictable: false });
        if self.nodes[&fid].evictable { let old = self.key(fid); self.order.remove(&old); }   // 1. remove under the OLD key
        let k = self.k;
        let n = self.nodes.get_mut(&fid).unwrap();
        n.history.push_back(now);                                                              // 2. change the node
        if n.history.len() > k { n.history.pop_front(); }
        if self.nodes[&fid].evictable { let new = self.key(fid); self.order.insert(new); }    // 3. insert under the NEW key
    }

    fn set_evictable(&mut self, fid: FrameId, on: bool) {
        let Some(n) = self.nodes.get(&fid) else { return };
        if n.evictable == on { return; }
        if on { let key = self.key(fid); self.order.insert(key); } else { let key = self.key(fid); self.order.remove(&key); }
        self.nodes.get_mut(&fid).unwrap().evictable = on;
    }

    fn evict(&mut self) -> Option<FrameId> {
        let (_, _, fid) = self.order.pop_first()?;               // O(log n)
        self.nodes.remove(&fid);                                  // the history is forgotten
        Some(fid)
    }

    fn size(&self) -> usize { self.order.len() }
}

#[test]
fn the_worked_example_from_the_figure() {
    let mut r = LruK::new(2);
    for fid in [1, 2, 1, 3, 2] { r.record_access(fid); }          // A=1 B=2 C=3: t0:A t1:B t2:A t3:C t4:B
    for fid in [1, 2, 3] { r.set_evictable(fid, true); }
    assert_eq!(r.evict(), Some(3));                               // C: one access, infinite distance
    assert_eq!(r.evict(), Some(1));                               // A: 2nd most recent is t0 (distance 5)
    assert_eq!(r.evict(), Some(2));                               // B: 2nd most recent is t1 (distance 4)
    assert_eq!(r.evict(), None);
}

#[test]
fn a_scan_does_not_flush_the_hot_set_but_k_equal_one_does() {
    fn run(k: usize) -> Vec<FrameId> {
        let mut r = LruK::new(k);
        for _ in 0..2 { for hot in [1, 2] { r.record_access(hot); } }   // two hot frames, touched twice each
        for scan in 100..110 { r.record_access(scan); }                  // a scan, once each, AFTER the hot frames
        for fid in r.nodes.keys().copied().collect::<Vec<_>>() { r.set_evictable(fid, true); }
        (0..4).map(|_| r.evict().unwrap()).collect()                     // the first four victims
    }
    assert_eq!(run(2), vec![100, 101, 102, 103]);                         // LRU-2: the scan leaves first
    assert_eq!(run(1), vec![1, 2, 100, 101]);                             // LRU-1 = LRU: the hot set goes first
}

#[test]
fn pinned_frames_are_never_victims_and_size_counts_only_evictable() {
    let mut r = LruK::new(2);
    for fid in 0..4 { r.record_access(fid); r.set_evictable(fid, true); }
    r.set_evictable(0, false);                                            // pinned
    assert_eq!(r.size(), 3);
    assert_eq!(r.evict(), Some(1));                                       // 0 is oldest but pinned
    r.record_access(2);                                                   // key changes while evictable: remove-old / insert-new
    assert_eq!(r.evict(), Some(3));                                       // 2 now has two accesses (finite), 3 has one (infinite)
    assert_eq!(r.size(), 1);
}
```

### In the exercises

- **1d-01 to 1d-03:** the bookkeeping (a `VecDeque` of the last K timestamps, a logical clock; see *Logical clocks and timestamps*), then the ordering rule (infinite distances first, oldest first access first; then the oldest K-th access), then the `BTreeSet` that makes eviction O(log n) (see *Ordered sets as priority queues*). The first test above is the worked trace.
- **1f-01, 1f-02:** the buffer pool calls `record_access` on every access and `set_evictable(false)` while a frame is pinned.

### Where it is used

- **Research and teaching systems**: it is the policy BusTub asks for; the original paper evaluated it on database buffer traces.
- **Descendants of the idea**: 2Q and MySQL InnoDB's midpoint insertion (a new page enters the middle of the LRU list, so a scan never reaches the hot end) are cheaper ways to demand "seen more than once"; **TinyLFU** in the Java Caffeine cache and Rust's `moka` keeps approximate frequency counts for the same reason.
- **Any cache in front of scan-heavy work**: log processing, analytics queries and backups all read each page once; a policy that tells that from reuse protects the interactive working set.
