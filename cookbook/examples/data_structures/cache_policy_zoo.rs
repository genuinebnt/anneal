//! RECIPE 230 (caching): a cache replacement policy zoo: FIFO, LRU, LFU, CLOCK, 2Q, ARC, SIEVE, W-TinyLFU-lite and Belady's OPT
//! LEVEL: advanced
//! Use in: choosing/evaluating an eviction policy for page caches, CDN/object caches, DB buffer pools, memoization.   Run: cargo run --release --example cache_policy_zoo
//! COMMON USES: OS page replacement (CLOCK variants), database buffer pools (2Q, ARC-like, LRU-K), CDN and object caches (SIEVE, S3-FIFO, TinyLFU),
//! CAVEAT (measured, not fully explained): on the abrupt "shifting" trace SIEVE and W-TinyLFU lose to LRU/CLOCK/ARC at large cache sizes (e.g. 65% and 59% vs 90% at c=1024), and LFU collapses (stale frequencies). Their state scales with the cache size, so they adapt slowly when the whole working set is replaced at once. This was not cross-checked against a reference implementation (try libCacheSim or caffeine on the same trace): treat the numbers as a property of THESE implementations on THIS trace, and the lesson as "no policy wins every workload".
//! in-process application caches (W-TinyLFU is what the Java library Caffeine uses), and offline "what hit ratio could ANY policy reach" analysis (OPT).
#![allow(dead_code)]
//!
//! What: nine policies behind one `trait Cache { fn access(&mut self, key) -> bool /*hit*/; fn len(&self) }`. Only keys matter (no values), because
//! eviction policy is independent of payload. Everything is arena based: one `Arena` of nodes (parallel Vecs) with `usize` links and a free list,
//! many intrusive doubly linked `List`s over it, and a `HashMap<key, node index>`. No unsafe, no Rc (see lru_cache for the single-list version).
//! Why: no policy wins everywhere. LRU collapses on scans and on loops slightly larger than the cache; LFU never forgets; CLOCK approximates LRU
//! cheaply (one bit, no list reordering on a hit); ARC and 2Q resist scans; SIEVE gets scan resistance with a single FIFO queue, a visited bit and a
//! moving hand (hits only set a bit: no lock or reordering, which is why it is attractive for concurrent caches); W-TinyLFU adds ADMISSION: a
//! candidate only displaces the victim if a count-min sketch says it is more frequent. OPT (Belady: evict the key used farthest in the future, bypass
//! a newcomer that would be used later than everything resident) needs the whole trace and is the unreachable upper bound.
//! How: seeded traces (Zipf, hot-set + scan pollution, a loop slightly larger than the cache, a shifting working set) are replayed against every
//! policy at three sizes; the table is printed and then ASSERTED: OPT >= all, capacity never exceeded, ARC/SIEVE/2Q beat LRU under scan pollution,
//! LRU ~0 on the loop while CLOCK/LFU/OPT are not, and ARC's four invariants from the paper are checked after EVERY access.
//! ARC (Megiddo & Modha): T1 = resident seen once, T2 = resident seen 2+ times, B1/B2 = ghost (key only) lists of what was evicted from T1/T2; a
//! ghost hit in B1 means "recency deserved more room" so target p (the wanted size of T1) grows, a ghost hit in B2 shrinks it.
//! Read: https://www.usenix.org/legacy/events/fast03/tech/full_papers/megiddo/megiddo.pdf (ARC, FAST 2003) ,
//! https://www.usenix.org/system/files/nsdi24-zhang-yazhuo.pdf (SIEVE, NSDI 2024) , https://github.com/ben-manes/caffeine (production W-TinyLFU) ,
//! https://en.wikipedia.org/wiki/Cache_replacement_policies , https://en.wikipedia.org/wiki/Count%E2%80%93min_sketch
use std::collections::{BTreeSet, HashMap};

pub trait Cache {
    /// Touch `key`; returns true on a hit. On a miss the policy decides whether/what to admit/evict.
    fn access(&mut self, key: u64) -> bool;
    /// Number of RESIDENT keys (ghost lists do not count).
    fn len(&self) -> usize;
}

// ---------------------------------------------------------------- shared arena of list nodes
const NIL: usize = usize::MAX;
#[derive(Clone, Copy)]
struct List {
    head: usize,
    tail: usize,
    len: usize,
} // head = newest/MRU, tail = oldest/LRU
impl List {
    const fn new() -> Self {
        List {
            head: NIL,
            tail: NIL,
            len: 0,
        }
    }
}

#[derive(Default)]
struct Arena {
    key: Vec<u64>,
    prev: Vec<usize>,
    next: Vec<usize>,
    tag: Vec<u8>,
    bit: Vec<bool>,
    aux: Vec<u32>,
    free: Vec<usize>,
}
impl Arena {
    // `prev` points toward the head (newer), `next` toward the tail (older).
    fn alloc(&mut self, key: u64, tag: u8) -> usize {
        if let Some(i) = self.free.pop() {
            self.key[i] = key;
            self.tag[i] = tag;
            self.bit[i] = false;
            self.aux[i] = 0;
            i
        } else {
            self.key.push(key);
            self.prev.push(NIL);
            self.next.push(NIL);
            self.tag.push(tag);
            self.bit.push(false);
            self.aux.push(0);
            self.key.len() - 1
        }
    }
    fn release(&mut self, i: usize) {
        self.free.push(i);
    }
    fn push_front(&mut self, l: &mut List, i: usize) {
        self.prev[i] = NIL;
        self.next[i] = l.head;
        if l.head != NIL {
            self.prev[l.head] = i
        } else {
            l.tail = i
        }
        l.head = i;
        l.len += 1;
    }
    fn unlink(&mut self, l: &mut List, i: usize) {
        let (p, n) = (self.prev[i], self.next[i]);
        if p != NIL {
            self.next[p] = n
        } else {
            l.head = n
        }
        if n != NIL {
            self.prev[n] = p
        } else {
            l.tail = p
        }
        l.len -= 1;
    }
}

// ---------------------------------------------------------------- FIFO and LRU
struct Fifo {
    cap: usize,
    a: Arena,
    l: List,
    map: HashMap<u64, usize>,
}
impl Fifo {
    fn new(cap: usize) -> Self {
        Fifo {
            cap,
            a: Arena::default(),
            l: List::new(),
            map: HashMap::new(),
        }
    }
}
impl Cache for Fifo {
    fn access(&mut self, k: u64) -> bool {
        if self.map.contains_key(&k) {
            return true;
        }
        if self.l.len == self.cap {
            let v = self.l.tail;
            self.a.unlink(&mut self.l, v);
            self.map.remove(&self.a.key[v]);
            self.a.release(v);
        }
        let i = self.a.alloc(k, 0);
        self.a.push_front(&mut self.l, i);
        self.map.insert(k, i);
        false
    }
    fn len(&self) -> usize {
        self.l.len
    }
}

struct Lru {
    cap: usize,
    a: Arena,
    l: List,
    map: HashMap<u64, usize>,
}
impl Lru {
    fn new(cap: usize) -> Self {
        Lru {
            cap,
            a: Arena::default(),
            l: List::new(),
            map: HashMap::new(),
        }
    }
}
impl Cache for Lru {
    fn access(&mut self, k: u64) -> bool {
        if let Some(&i) = self.map.get(&k) {
            self.a.unlink(&mut self.l, i);
            self.a.push_front(&mut self.l, i);
            return true;
        }
        if self.l.len == self.cap {
            let v = self.l.tail;
            self.a.unlink(&mut self.l, v);
            self.map.remove(&self.a.key[v]);
            self.a.release(v);
        }
        let i = self.a.alloc(k, 0);
        self.a.push_front(&mut self.l, i);
        self.map.insert(k, i);
        false
    }
    fn len(&self) -> usize {
        self.l.len
    }
}

// ---------------------------------------------------------------- LFU, O(1) with one list per frequency + min_freq
struct Lfu {
    cap: usize,
    a: Arena,
    buckets: Vec<List>,
    min_freq: usize,
    map: HashMap<u64, usize>,
}
impl Lfu {
    fn new(cap: usize) -> Self {
        Lfu {
            cap,
            a: Arena::default(),
            buckets: vec![List::new(); 2],
            min_freq: 1,
            map: HashMap::new(),
        }
    }
}
impl Cache for Lfu {
    fn access(&mut self, k: u64) -> bool {
        if let Some(&i) = self.map.get(&k) {
            let f = self.a.aux[i] as usize;
            self.a.unlink(&mut self.buckets[f], i);
            if self.buckets.len() <= f + 1 {
                self.buckets.push(List::new());
            }
            self.a.aux[i] += 1;
            self.a.push_front(&mut self.buckets[f + 1], i);
            if self.buckets[f].len == 0 && self.min_freq == f {
                self.min_freq = f + 1;
            }
            return true;
        }
        if self.map.len() == self.cap {
            let v = self.buckets[self.min_freq].tail; // oldest among least frequent
            self.a.unlink(&mut self.buckets[self.min_freq], v);
            self.map.remove(&self.a.key[v]);
            self.a.release(v);
        }
        let i = self.a.alloc(k, 0);
        self.a.aux[i] = 1;
        self.a.push_front(&mut self.buckets[1], i);
        self.map.insert(k, i);
        self.min_freq = 1;
        false
    }
    fn len(&self) -> usize {
        self.map.len()
    }
}

// ---------------------------------------------------------------- CLOCK: ring of slots, reference bit, hand
struct Clock {
    cap: usize,
    keys: Vec<u64>,
    refb: Vec<bool>,
    hand: usize,
    map: HashMap<u64, usize>,
}
impl Clock {
    fn new(cap: usize) -> Self {
        Clock {
            cap,
            keys: vec![],
            refb: vec![],
            hand: 0,
            map: HashMap::new(),
        }
    }
}
impl Cache for Clock {
    fn access(&mut self, k: u64) -> bool {
        if let Some(&s) = self.map.get(&k) {
            self.refb[s] = true;
            return true;
        }
        if self.keys.len() < self.cap {
            self.map.insert(k, self.keys.len());
            self.keys.push(k);
            self.refb.push(false);
            return false;
        }
        while self.refb[self.hand] {
            self.refb[self.hand] = false;
            self.hand = (self.hand + 1) % self.cap;
        } // second chance
        self.map.remove(&self.keys[self.hand]);
        self.keys[self.hand] = k;
        self.map.insert(k, self.hand);
        self.hand = (self.hand + 1) % self.cap;
        false
    }
    fn len(&self) -> usize {
        self.keys.len()
    }
}

// ---------------------------------------------------------------- 2Q (Johnson & Shasha): A1in FIFO, A1out ghost FIFO, Am LRU
struct TwoQ {
    cap: usize,
    kin: usize,
    kout: usize,
    a: Arena,
    in_: List,
    out: List,
    am: List,
    map: HashMap<u64, usize>,
}
impl TwoQ {
    fn new(cap: usize) -> Self {
        TwoQ {
            cap,
            kin: (cap / 4).max(1),
            kout: (cap / 2).max(1),
            a: Arena::default(),
            in_: List::new(),
            out: List::new(),
            am: List::new(),
            map: HashMap::new(),
        }
    }
    fn make_room(&mut self) {
        if self.in_.len + self.am.len < self.cap {
            return;
        }
        if self.in_.len > self.kin || self.am.len == 0 {
            let v = self.in_.tail;
            self.a.unlink(&mut self.in_, v);
            self.a.tag[v] = 2;
            self.a.push_front(&mut self.out, v);
            if self.out.len > self.kout {
                let g = self.out.tail;
                self.a.unlink(&mut self.out, g);
                self.map.remove(&self.a.key[g]);
                self.a.release(g);
            }
        } else {
            let v = self.am.tail;
            self.a.unlink(&mut self.am, v);
            self.map.remove(&self.a.key[v]);
            self.a.release(v);
        }
    }
}
impl Cache for TwoQ {
    fn access(&mut self, k: u64) -> bool {
        match self.map.get(&k).copied() {
            Some(i) if self.a.tag[i] == 1 => {
                self.a.unlink(&mut self.am, i);
                self.a.push_front(&mut self.am, i);
                true
            }
            Some(i) if self.a.tag[i] == 0 => {
                let _ = i;
                true
            } // A1in hit: FIFO, no reorder
            Some(i) => {
                // ghost hit in A1out: proven re-reference, promote to Am
                self.a.unlink(&mut self.out, i);
                self.make_room();
                self.a.tag[i] = 1;
                self.a.push_front(&mut self.am, i);
                false
            }
            None => {
                self.make_room();
                let i = self.a.alloc(k, 0);
                self.a.push_front(&mut self.in_, i);
                self.map.insert(k, i);
                false
            }
        }
    }
    fn len(&self) -> usize {
        self.in_.len + self.am.len
    }
}

// ---------------------------------------------------------------- ARC (Megiddo & Modha, FAST 2003, Figure 4)
const T1: usize = 0;
const T2: usize = 1;
const B1: usize = 2;
const B2: usize = 3;
struct Arc {
    c: usize,
    p: usize,
    a: Arena,
    l: [List; 4],
    map: HashMap<u64, usize>,
}
impl Arc {
    fn new(c: usize) -> Self {
        Arc {
            c,
            p: 0,
            a: Arena::default(),
            l: [List::new(); 4],
            map: HashMap::new(),
        }
    }
    fn mv(&mut self, i: usize, to: usize) {
        let from = self.a.tag[i] as usize;
        self.a.unlink(&mut self.l[from], i);
        self.a.tag[i] = to as u8;
        self.a.push_front(&mut self.l[to], i);
    }
    fn drop_lru(&mut self, t: usize) {
        let v = self.l[t].tail;
        self.a.unlink(&mut self.l[t], v);
        self.map.remove(&self.a.key[v]);
        self.a.release(v);
    }
    /// REPLACE(x, p): demote one resident page into the matching ghost list.
    fn replace(&mut self, x_in_b2: bool) {
        let t1 = self.l[T1].len;
        if t1 >= 1 && ((x_in_b2 && t1 == self.p) || t1 > self.p) {
            let v = self.l[T1].tail;
            self.mv(v, B1);
        } else if self.l[T2].len > 0 {
            let v = self.l[T2].tail;
            self.mv(v, B2);
        } else {
            let v = self.l[T1].tail;
            self.mv(v, B1);
        }
    }
    /// The four invariants from section III of the paper.
    fn check_invariants(&self) {
        let (t1, t2, b1, b2) = (
            self.l[T1].len,
            self.l[T2].len,
            self.l[B1].len,
            self.l[B2].len,
        );
        assert!(t1 + t2 <= self.c, "|T1|+|T2| <= c");
        assert!(t1 + b1 <= self.c, "|T1|+|B1| <= c");
        assert!(t2 + b2 <= 2 * self.c, "|T2|+|B2| <= 2c");
        assert!(t1 + t2 + b1 + b2 <= 2 * self.c, "total <= 2c");
        assert!(self.p <= self.c, "0 <= p <= c");
        assert_eq!(self.map.len(), t1 + t2 + b1 + b2);
        // once the cache has been full, it stays exactly full
        if b1 + b2 > 0 {
            assert_eq!(t1 + t2, self.c);
        }
    }
}
impl Cache for Arc {
    fn access(&mut self, k: u64) -> bool {
        let hit = match self.map.get(&k).copied() {
            Some(i) if (self.a.tag[i] as usize) < B1 => {
                self.mv(i, T2);
                true
            } // case I
            Some(i) if self.a.tag[i] as usize == B1 => {
                // case II: ghost hit, favour recency
                let d = (self.l[B2].len / self.l[B1].len).max(1);
                self.p = (self.p + d).min(self.c);
                self.replace(false);
                self.mv(i, T2);
                false
            }
            Some(i) => {
                // case III: ghost hit in B2, favour frequency
                let d = (self.l[B1].len / self.l[B2].len).max(1);
                self.p = self.p.saturating_sub(d);
                self.replace(true);
                self.mv(i, T2);
                false
            }
            None => {
                // case IV: complete miss
                let l1 = self.l[T1].len + self.l[B1].len;
                let total = l1 + self.l[T2].len + self.l[B2].len;
                if l1 == self.c {
                    if self.l[T1].len < self.c {
                        self.drop_lru(B1);
                        self.replace(false);
                    } else {
                        self.drop_lru(T1);
                    }
                } else if total >= self.c {
                    if total == 2 * self.c {
                        self.drop_lru(B2);
                    }
                    self.replace(false);
                }
                let i = self.a.alloc(k, T1 as u8);
                self.a.push_front(&mut self.l[T1], i);
                self.map.insert(k, i);
                false
            }
        };
        self.check_invariants();
        hit
    }
    fn len(&self) -> usize {
        self.l[T1].len + self.l[T2].len
    }
}

// ---------------------------------------------------------------- SIEVE (NSDI'24): FIFO queue + visited bit + moving hand
struct Sieve {
    cap: usize,
    a: Arena,
    q: List,
    hand: usize,
    map: HashMap<u64, usize>,
}
impl Sieve {
    fn new(cap: usize) -> Self {
        Sieve {
            cap,
            a: Arena::default(),
            q: List::new(),
            hand: NIL,
            map: HashMap::new(),
        }
    }
}
impl Cache for Sieve {
    fn access(&mut self, k: u64) -> bool {
        if let Some(&i) = self.map.get(&k) {
            self.a.bit[i] = true;
            return true;
        } // hit: only set a bit, nothing moves
        if self.q.len == self.cap {
            let mut o = if self.hand == NIL {
                self.q.tail
            } else {
                self.hand
            };
            while self.a.bit[o] {
                self.a.bit[o] = false; // retained, but a second chance used up; the hand sweeps toward the head (newer)
                o = if self.a.prev[o] != NIL {
                    self.a.prev[o]
                } else {
                    self.q.tail
                };
            }
            self.hand = self.a.prev[o]; // NIL means: restart from the tail next time
            self.a.unlink(&mut self.q, o);
            self.map.remove(&self.a.key[o]);
            self.a.release(o);
        }
        let i = self.a.alloc(k, 0);
        self.a.push_front(&mut self.q, i);
        self.map.insert(k, i);
        false // new objects enter unvisited at the head
    }
    fn len(&self) -> usize {
        self.q.len
    }
}

// ---------------------------------------------------------------- W-TinyLFU-lite: window LRU + doorkeeper + count-min sketch + segmented LRU
fn mix(mut x: u64, seed: u64) -> u64 {
    x = x.wrapping_add(seed).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}
struct Sketch {
    rows: [Vec<u8>; 4],
    mask: usize,
    door: Vec<u64>,
    dmask: usize,
    adds: usize,
    sample: usize,
}
impl Sketch {
    fn new(cap: usize) -> Self {
        let w = (cap * 4).next_power_of_two();
        let bits = (cap * 8).next_power_of_two().max(64);
        Sketch {
            rows: std::array::from_fn(|_| vec![0u8; w]),
            mask: w - 1,
            door: vec![0; bits / 64],
            dmask: bits - 1,
            adds: 0,
            sample: cap * 10,
        }
    }
    fn door_idx(&self, k: u64) -> [usize; 2] {
        [
            mix(k, 11) as usize & self.dmask,
            mix(k, 12) as usize & self.dmask,
        ]
    }
    fn door_has(&self, k: u64) -> bool {
        self.door_idx(k)
            .iter()
            .all(|&b| self.door[b / 64] >> (b % 64) & 1 == 1)
    }
    fn record(&mut self, k: u64) {
        if !self.door_has(k) {
            // first sighting only sets the doorkeeper: one-hit wonders never reach the sketch
            for b in self.door_idx(k) {
                self.door[b / 64] |= 1 << (b % 64);
            }
        } else {
            for r in 0..4 {
                let j = mix(k, r as u64) as usize & self.mask;
                if self.rows[r][j] < 15 {
                    self.rows[r][j] += 1;
                }
            }
        }
        self.adds += 1;
        if self.adds >= self.sample {
            // aging: halve everything so stale popularity decays
            for row in self.rows.iter_mut() {
                for c in row.iter_mut() {
                    *c >>= 1;
                }
            }
            self.door.iter_mut().for_each(|w| *w = 0);
            self.adds = 0;
        }
    }
    fn estimate(&self, k: u64) -> u32 {
        let m = (0..4)
            .map(|r| self.rows[r][mix(k, r as u64) as usize & self.mask])
            .min()
            .unwrap() as u32;
        m + self.door_has(k) as u32
    }
}
const WIN: u8 = 0;
const PROB: u8 = 1;
const PROT: u8 = 2;
struct TinyLfu {
    cap: usize,
    win_cap: usize,
    prot_cap: usize,
    a: Arena,
    win: List,
    prob: List,
    prot: List,
    sk: Sketch,
    map: HashMap<u64, usize>,
}
impl TinyLfu {
    fn new(cap: usize) -> Self {
        let win_cap = (cap / 100).max(1);
        let main = cap - win_cap;
        TinyLfu {
            cap,
            win_cap,
            prot_cap: main * 8 / 10,
            a: Arena::default(),
            win: List::new(),
            prob: List::new(),
            prot: List::new(),
            sk: Sketch::new(cap),
            map: HashMap::new(),
        }
    }
    fn evict(&mut self, v: usize, from: u8) {
        match from {
            WIN => self.a.unlink(&mut self.win, v),
            PROB => self.a.unlink(&mut self.prob, v),
            _ => self.a.unlink(&mut self.prot, v),
        }
        self.map.remove(&self.a.key[v]);
        self.a.release(v);
    }
}
impl Cache for TinyLfu {
    fn access(&mut self, k: u64) -> bool {
        self.sk.record(k);
        if let Some(&i) = self.map.get(&k) {
            match self.a.tag[i] {
                WIN => {
                    self.a.unlink(&mut self.win, i);
                    self.a.push_front(&mut self.win, i);
                }
                PROT => {
                    self.a.unlink(&mut self.prot, i);
                    self.a.push_front(&mut self.prot, i);
                }
                _ => {
                    // probation hit: promote; if protected overflows, demote its LRU back to probation
                    self.a.unlink(&mut self.prob, i);
                    self.a.tag[i] = PROT;
                    self.a.push_front(&mut self.prot, i);
                    if self.prot.len > self.prot_cap {
                        let d = self.prot.tail;
                        self.a.unlink(&mut self.prot, d);
                        self.a.tag[d] = PROB;
                        self.a.push_front(&mut self.prob, d);
                    }
                }
            }
            return true;
        }
        let i = self.a.alloc(k, WIN);
        self.a.push_front(&mut self.win, i);
        self.map.insert(k, i);
        if self.win.len > self.win_cap {
            // window overflow: its LRU becomes the admission candidate
            let c = self.win.tail;
            self.a.unlink(&mut self.win, c);
            if self.prob.len + self.prot.len < self.cap - self.win_cap {
                self.a.tag[c] = PROB;
                self.a.push_front(&mut self.prob, c);
            } else {
                let (v, from) = if self.prob.len > 0 {
                    (self.prob.tail, PROB)
                } else {
                    (self.prot.tail, PROT)
                };
                if self.sk.estimate(self.a.key[c]) > self.sk.estimate(self.a.key[v]) {
                    self.evict(v, from);
                    self.a.tag[c] = PROB;
                    self.a.push_front(&mut self.prob, c);
                } else {
                    self.map.remove(&self.a.key[c]);
                    self.a.release(c);
                } // rejected: candidate dies, victim stays
            }
        }
        false
    }
    fn len(&self) -> usize {
        self.win.len + self.prob.len + self.prot.len
    }
}

// ---------------------------------------------------------------- Belady's OPT (offline; with bypass so it is a true upper bound)
struct Opt {
    cap: usize,
    next_use: Vec<usize>,
    pos: usize,
    res: HashMap<u64, usize>,
    order: BTreeSet<(usize, u64)>,
}
impl Opt {
    fn new(cap: usize, trace: &[u64]) -> Self {
        let mut nu = vec![usize::MAX; trace.len()];
        let mut last: HashMap<u64, usize> = HashMap::new();
        for t in (0..trace.len()).rev() {
            if let Some(&n) = last.get(&trace[t]) {
                nu[t] = n;
            }
            last.insert(trace[t], t);
        }
        Opt {
            cap,
            next_use: nu,
            pos: 0,
            res: HashMap::new(),
            order: BTreeSet::new(),
        }
    }
}
impl Cache for Opt {
    fn access(&mut self, k: u64) -> bool {
        let nu = self.next_use[self.pos];
        self.pos += 1;
        if let Some(old) = self.res.insert(k, nu) {
            self.order.remove(&(old, k));
            self.order.insert((nu, k));
            return true;
        }
        if self.res.len() > self.cap {
            // we over-inserted above; resolve: evict the farthest (maybe the newcomer itself = bypass)
            let &(far, fk) = self.order.iter().next_back().unwrap();
            if far <= nu && fk != k {
                self.res.remove(&k);
            }
            // everyone resident returns sooner than the newcomer: bypass
            else {
                self.order.remove(&(far, fk));
                self.res.remove(&fk);
                self.order.insert((nu, k));
            }
        } else {
            self.order.insert((nu, k));
        }
        false
    }
    fn len(&self) -> usize {
        self.res.len()
    }
}

// ---------------------------------------------------------------- seeded traces
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
const LEN: usize = 150_000;
fn zipf_trace(_c: usize, seed: u64) -> Vec<u64> {
    let n = 20_000usize;
    let mut cdf = Vec::with_capacity(n);
    let mut s = 0.0;
    for r in 1..=n {
        s += 1.0 / (r as f64).powf(0.95);
        cdf.push(s);
    }
    let mut g = Rng(seed);
    (0..LEN)
        .map(|_| {
            let u = g.f() * s;
            cdf.partition_point(|&x| x < u) as u64
        })
        .collect()
}
fn scan_trace(c: usize, seed: u64) -> Vec<u64> {
    // hot set of 3c/4 keys, half the accesses are never-repeating scan keys
    let (hot, mut g, mut scan) = ((c * 3 / 4) as u64, Rng(seed), 1_000_000u64);
    (0..LEN)
        .map(|_| {
            if g.below(2) == 0 {
                g.below(hot)
            } else {
                scan += 1;
                scan
            }
        })
        .collect()
}
fn loop_trace(c: usize, seed: u64) -> Vec<u64> {
    // loop 12% larger than the cache, 2% random touches inside the loop
    let (l, mut g, mut i) = ((c + c / 8) as u64, Rng(seed), 0u64);
    (0..LEN)
        .map(|_| {
            if g.below(100) < 2 {
                g.below(l)
            } else {
                i += 1;
                i % l
            }
        })
        .collect()
}
fn shift_trace(c: usize, seed: u64) -> Vec<u64> {
    // working set of 3c/4 keys that moves to fresh keys every 15k accesses, 5% noise
    let (w, mut g) = ((c * 3 / 4) as u64, Rng(seed));
    (0..LEN)
        .map(|t| {
            if g.below(100) < 5 {
                500_000 + g.below(50_000)
            } else {
                (t / 15_000) as u64 * w + g.below(w)
            }
        })
        .collect()
}

fn run(cache: &mut dyn Cache, cap: usize, trace: &[u64]) -> f64 {
    let mut hits = 0usize;
    for &k in trace {
        if cache.access(k) {
            hits += 1;
        }
        assert!(cache.len() <= cap, "capacity exceeded");
    }
    hits as f64 / trace.len() as f64
}
const NAMES: [&str; 9] = [
    "FIFO",
    "LRU",
    "LFU",
    "CLOCK",
    "2Q",
    "ARC",
    "SIEVE",
    "W-TinyLFU",
    "OPT",
];
fn make(i: usize, c: usize, trace: &[u64]) -> Box<dyn Cache> {
    match i {
        0 => Box::new(Fifo::new(c)),
        1 => Box::new(Lru::new(c)),
        2 => Box::new(Lfu::new(c)),
        3 => Box::new(Clock::new(c)),
        4 => Box::new(TwoQ::new(c)),
        5 => Box::new(Arc::new(c)),
        6 => Box::new(Sieve::new(c)),
        7 => Box::new(TinyLfu::new(c)),
        _ => Box::new(Opt::new(c, trace)),
    }
}

fn main() {
    let sizes = [64usize, 256, 1024];
    let gens: [(&str, fn(usize, u64) -> Vec<u64>); 4] = [
        ("zipf", zipf_trace),
        ("scan+hot", scan_trace),
        ("loop", loop_trace),
        ("shifting", shift_trace),
    ];
    let mut res: HashMap<(&str, usize, usize), f64> = HashMap::new(); // (trace, size, policy) -> hit ratio
    for (tn, gen_) in gens {
        println!("\ntrace: {tn}   (hit ratio %, {LEN} accesses)");
        print!("{:<10}", "policy");
        for c in sizes {
            print!("{:>9}", format!("c={c}"));
        }
        println!();
        let traces: Vec<Vec<u64>> = sizes
            .iter()
            .map(|&c| gen_(c, 0xC0FFEE ^ c as u64))
            .collect();
        for p in 0..NAMES.len() {
            print!("{:<10}", NAMES[p]);
            for (si, &c) in sizes.iter().enumerate() {
                let mut cache = make(p, c, &traces[si]);
                let r = run(cache.as_mut(), c, &traces[si]);
                res.insert((tn, c, p), r);
                print!("{:>8.1}%", r * 100.0);
            }
            println!();
        }
    }
    let h = |t: &str, c: usize, name: &str| {
        res[&(t, c, NAMES.iter().position(|n| *n == name).unwrap())]
    };

    // OPT dominates every policy everywhere
    for (tn, _) in gens {
        for c in sizes {
            for p in 0..8 {
                assert!(
                    h(tn, c, "OPT") + 1e-12 >= res[&(tn, c, p)],
                    "OPT must dominate {} on {tn} c={c}",
                    NAMES[p]
                );
            }
        }
    }
    // scan pollution: scan-resistant policies beat LRU
    for c in sizes {
        for n in ["ARC", "SIEVE", "2Q"] {
            assert!(
                h("scan+hot", c, n) > h("scan+hot", c, "LRU") + 0.02,
                "{n} should beat LRU under scan pollution at c={c}"
            );
        }
    }
    // loop slightly larger than the cache: LRU is the textbook worst case
    for c in sizes {
        assert!(h("loop", c, "LRU") < 0.10, "LRU ~0 on loop");
        for n in ["CLOCK", "LFU", "OPT"] {
            assert!(
                h("loop", c, n) > h("loop", c, "LRU") + 0.05,
                "{n} should beat LRU on the loop at c={c}"
            );
        }
    }
    // ARC invariants after EVERY access (check_invariants runs inside access) on adversarial mixed input, plus p must actually move
    let mut arc = Arc::new(32);
    let mut g = Rng(7);
    let mut pmax = 0;
    let mut pmin = 32;
    for t in 0..200_000u64 {
        let k = match (t / 5000) % 4 {
            0 => g.below(40),
            1 => 100 + t,
            2 => g.below(24) * 3,
            _ => t % 40,
        };
        arc.access(k);
        arc.check_invariants();
        pmax = pmax.max(arc.p);
        pmin = pmin.min(arc.p);
    }
    assert!(pmax > pmin, "adaptive target p must move");
    // sanity: deterministic tiny example for LRU/FIFO
    let (mut l, mut f) = (Lru::new(2), Fifo::new(2));
    let seq = [1, 2, 1, 3, 1, 2];
    let (lh, fh): (Vec<bool>, Vec<bool>) = seq.iter().map(|&k| (l.access(k), f.access(k))).unzip();
    assert_eq!(lh, [false, false, true, false, true, false]);
    assert_eq!(fh, [false, false, true, false, false, false]);
    println!("\nARC p range seen on the mixed trace: {pmin}..={pmax}; all assertions passed");
}
