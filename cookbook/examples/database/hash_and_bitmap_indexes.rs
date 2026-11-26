//! RECIPE 415 (database): hash indexes (extendible, linear) and compressed bitmap indexes (equality/range encoded, WAH-style).
//! LEVEL: advanced
//! Use in: storage engines, OLAP/column stores, data warehouses, any index choice between B+tree, hash and bitmap.   Run: cargo run --release --example hash_and_bitmap_indexes
#![allow(dead_code)]
//!
//! PART A, DYNAMIC HASH INDEXES: a static hash table must be rebuilt when it outgrows its bucket array; disk-based hash indexes
//! grow incrementally instead. Buckets are PAGES holding at most `cap` entries.
//! * EXTENDIBLE HASHING (Fagin et al.): a DIRECTORY of 2^g pointers (g = global depth) indexed by the low g bits of the hash. Each
//!   bucket has a LOCAL depth l <= g and is shared by 2^(g-l) directory slots. A full bucket SPLITS: if l == g the directory first
//!   DOUBLES (copy itself; cheap, just pointers), then entries are redistributed by bit l and the slots whose bit l is 1 are
//!   re-pointed to the new bucket. A lookup is exactly ONE bucket read (directory is in memory). Deletes merge a bucket with its
//!   buddy (same local depth, differing in bit l-1) when they fit in one page, and the directory halves when possible.
//! * LINEAR HASHING (Litwin): no directory. Buckets are numbered 0..N0*2^level + next. A SPLIT POINTER `next` names the bucket to
//!   split NEXT, regardless of which bucket overflowed: it is split in round-robin order using the next hash bit, and the new
//!   bucket is appended at the end. Address = h mod (N0*2^level), or h mod (N0*2^(level+1)) if that bucket already split this round.
//!   Splits are triggered by LOAD FACTOR (controlled splitting) so overflow chains stay short and the file grows by one bucket
//!   at a time, with no directory and no big pause. Cost: some lookups walk an overflow page.
//! Both are checked against `HashMap` over 30 000 mixed operations with structural invariants verified along the way.
//!
//! PART B, BITMAP INDEXES: for a low-cardinality column, keep one bitmap (1 bit per row) for each distinct value. A predicate becomes
//! word-wide bitwise operations: `A = 3 AND (B = 1 OR NOT C = 0)` is two ORs/ANDs over bitmaps, 64 rows per CPU instruction, with
//! no random row access until the final bitmap is turned into row ids.
//! * EQUALITY-ENCODED: bitmap[v] = rows with value v. Great for `=` and small `IN`; a range of k values ORs k bitmaps.
//! * RANGE-ENCODED: bitmap[v] = rows with value <= v (cumulative). Any range `lo..=hi` costs at most two bitmaps:
//!   `R[hi] AND NOT R[lo-1]`. Cost: the cumulative bitmaps are denser, hence compress worse on unsorted data.
//! * COMPRESSION (WAH-style, word aligned on u64): the bitmap is cut into 63-bit groups; a LITERAL word (top bit 0) stores one
//!   group, a FILL word (top bit 1, next bit = fill value, 62-bit count) stands for up to 2^62 identical all-0 or all-1 groups.
//!   AND/OR/XOR/NOT operate directly on the compressed streams: two cursors advance in lockstep, fill-vs-fill is decided for
//!   a whole run at once, and the output builder merges adjacent fills. Sorted/clustered columns compress to a few words per value.
//! The test builds a 3-column table, evaluates 500 random boolean/range queries on compressed bitmaps and asserts the row ids
//! equal a plain table scan.
//!
//! WHEN TO USE WHAT: B+TREE for range scans, ordering, prefix and unique constraints, unknown workloads (the default; log_fanout n
//! page reads, ordered iteration). HASH for equality-only point lookups on high-cardinality keys (exactly ~1 page read; no order,
//! no ranges). BITMAP for low-cardinality columns (status, country, flag) in read-mostly analytics where many predicates are
//! combined; poor for high cardinality (one bitmap per value) and for heavy concurrent updates (an update rewrites compressed
//! bitmaps, and in Oracle-style systems locks a whole run of rows).
//!
//! COMMON USES: extendible hashing in file systems and hash-based database indexes (e.g. ext3 HTree-like directory ideas, GDBM's
//! on-disk hash); linear hashing in Berkeley DB-style hash access methods and Erlang/OTP `dets`; bitmap indexes in data
//! warehouses (Oracle bitmap indexes), PostgreSQL bitmap heap scans (transient bitmaps), Lucene filter caches and search engines
//! (Roaring), FastBit, Druid/Pinot inverted indexes.
//! Read: Linear hashing https://en.wikipedia.org/wiki/Linear_hashing   Extendible hashing https://en.wikipedia.org/wiki/Extendible_hashing
//!       Bitmap index https://en.wikipedia.org/wiki/Bitmap_index   WAH paper https://sdm.lbl.gov/~kewu/ps/LBNL-49627.pdf
//!       EWAH (Lemire et al.) https://arxiv.org/abs/0901.3751   Roaring https://arxiv.org/abs/1402.6407

use std::collections::HashMap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// A bijective 64-bit mixer (splitmix64 finalizer): distinct keys always have distinct hashes, so a bucket can always be split.
fn mix(k: u64) -> u64 {
    let mut x = k.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

type Item = (u64, u64);

/// The interface both dynamic hash indexes implement, so one test drives them.
trait DynHash {
    fn get(&self, k: u64) -> Option<u64>;
    fn insert(&mut self, k: u64, v: u64) -> Option<u64>;
    fn remove(&mut self, k: u64) -> Option<u64>;
    fn len(&self) -> usize;
    fn check(&self);
    fn items(&self) -> Vec<Item>;
}

// ---------- extendible hashing ----------
struct ExtBucket {
    ld: u32,
    items: Vec<Item>,
}
struct Extendible {
    cap: usize,
    gd: u32,
    dir: Vec<usize>, // 2^gd bucket ids
    buckets: Vec<ExtBucket>,
    free: Vec<usize>,
    len: usize,
    splits: u64,
    doublings: u64,
    merges: u64,
}
impl Extendible {
    fn new(cap: usize) -> Self {
        Extendible {
            cap,
            gd: 0,
            dir: vec![0],
            buckets: vec![ExtBucket {
                ld: 0,
                items: vec![],
            }],
            free: vec![],
            len: 0,
            splits: 0,
            doublings: 0,
            merges: 0,
        }
    }
    fn slot(&self, h: u64) -> usize {
        (h & ((1u64 << self.gd) - 1)) as usize
    }
    fn live_buckets(&self) -> usize {
        self.buckets.len() - self.free.len()
    }
    fn split(&mut self, bi: usize) {
        if self.buckets[bi].ld == self.gd {
            let copy = self.dir.clone();
            self.dir.extend(copy); // doubling: the new half mirrors the old, so every lookup still works
            self.gd += 1;
            self.doublings += 1;
        }
        let ld = self.buckets[bi].ld;
        let nb = match self.free.pop() {
            Some(id) => id,
            None => {
                self.buckets.push(ExtBucket {
                    ld: 0,
                    items: vec![],
                });
                self.buckets.len() - 1
            }
        };
        self.buckets[nb].ld = ld + 1;
        self.buckets[bi].ld = ld + 1;
        for it in std::mem::take(&mut self.buckets[bi].items) {
            let target = if mix(it.0) >> ld & 1 == 1 { nb } else { bi };
            self.buckets[target].items.push(it);
        }
        // Slots that agree with the old bucket on `ld` bits and have bit `ld` set now belong to the new bucket.
        for i in 0..self.dir.len() {
            if self.dir[i] == bi && (i >> ld) & 1 == 1 {
                self.dir[i] = nb;
            }
        }
        self.splits += 1;
    }
    /// Merge with the buddy while both fit one page, then halve the directory if it became redundant.
    fn try_merge(&mut self, h: u64) {
        loop {
            let s = self.slot(h);
            let bi = self.dir[s];
            let ld = self.buckets[bi].ld;
            if ld == 0 {
                break;
            }
            let buddy = self.dir[s ^ (1 << (ld - 1))];
            if buddy == bi
                || self.buckets[buddy].ld != ld
                || self.buckets[bi].items.len() + self.buckets[buddy].items.len() > self.cap
            {
                break;
            }
            let moved = std::mem::take(&mut self.buckets[buddy].items);
            self.buckets[bi].items.extend(moved);
            self.buckets[bi].ld -= 1;
            self.dir
                .iter_mut()
                .filter(|b| **b == buddy)
                .for_each(|b| *b = bi);
            self.free.push(buddy);
            self.merges += 1;
        }
        while self.gd > 0 {
            let half = self.dir.len() / 2;
            if (0..half).all(|i| self.dir[i] == self.dir[i + half]) {
                self.dir.truncate(half);
                self.gd -= 1;
            } else {
                break;
            }
        }
    }
}
impl DynHash for Extendible {
    fn get(&self, k: u64) -> Option<u64> {
        let b = &self.buckets[self.dir[self.slot(mix(k))]];
        b.items.iter().find(|it| it.0 == k).map(|it| it.1)
    }
    fn insert(&mut self, k: u64, v: u64) -> Option<u64> {
        let h = mix(k);
        let bi = self.dir[self.slot(h)];
        if let Some(it) = self.buckets[bi].items.iter_mut().find(|it| it.0 == k) {
            return Some(std::mem::replace(&mut it.1, v));
        }
        loop {
            let bi = self.dir[self.slot(h)];
            if self.buckets[bi].items.len() < self.cap {
                self.buckets[bi].items.push((k, v));
                self.len += 1;
                return None;
            }
            self.split(bi); // may need several rounds if all entries share the next bit
        }
    }
    fn remove(&mut self, k: u64) -> Option<u64> {
        let h = mix(k);
        let bi = self.dir[self.slot(h)];
        let pos = self.buckets[bi].items.iter().position(|it| it.0 == k)?;
        let (_, v) = self.buckets[bi].items.swap_remove(pos);
        self.len -= 1;
        self.try_merge(h);
        Some(v)
    }
    fn len(&self) -> usize {
        self.len
    }
    fn check(&self) {
        assert_eq!(self.dir.len(), 1 << self.gd);
        let mut refs: HashMap<usize, usize> = HashMap::new();
        for (i, &b) in self.dir.iter().enumerate() {
            *refs.entry(b).or_default() += 1;
            let bk = &self.buckets[b];
            assert!(bk.ld <= self.gd);
            // Every entry hashes to a slot that agrees with this directory position on the bucket's low `ld` bits.
            let mask = (1u64 << bk.ld) - 1;
            assert!(
                bk.items
                    .iter()
                    .all(|it| mix(it.0) & mask == i as u64 & mask),
                "entry in the wrong bucket"
            );
            assert!(bk.items.len() <= self.cap);
        }
        for (&b, &n) in &refs {
            assert_eq!(
                n,
                1 << (self.gd - self.buckets[b].ld),
                "bucket shared by 2^(g-l) slots"
            );
        }
        assert_eq!(self.live_buckets(), refs.len());
        assert_eq!(
            self.len,
            refs.keys()
                .map(|&b| self.buckets[b].items.len())
                .sum::<usize>()
        );
    }
    fn items(&self) -> Vec<Item> {
        let mut seen: Vec<usize> = self.dir.clone();
        seen.sort_unstable();
        seen.dedup();
        seen.iter()
            .flat_map(|&b| self.buckets[b].items.iter().copied())
            .collect()
    }
}

// ---------- linear hashing ----------
struct Linear {
    cap: usize,
    n0: usize,
    level: u32,
    next: usize,
    buckets: Vec<Vec<Item>>, // a bucket's overflow pages are modelled by len/cap
    len: usize,
    max_load: f64,
    min_load: f64,
    splits: u64,
    merges: u64,
}
impl Linear {
    fn new(cap: usize) -> Self {
        let n0 = 4;
        Linear {
            cap,
            n0,
            level: 0,
            next: 0,
            buckets: vec![vec![]; n0],
            len: 0,
            max_load: 0.8,
            min_load: 0.35,
            splits: 0,
            merges: 0,
        }
    }
    fn addr(&self, h: u64) -> usize {
        let m = (self.n0 << self.level) as u64;
        let a = (h % m) as usize;
        // Buckets before the split pointer were already split this round: they use one more hash bit.
        if a < self.next {
            (h % (2 * m)) as usize
        } else {
            a
        }
    }
    fn load(&self) -> f64 {
        self.len as f64 / (self.buckets.len() * self.cap) as f64
    }
    fn split_next(&mut self) {
        let m = self.n0 << self.level;
        let old = std::mem::take(&mut self.buckets[self.next]);
        self.buckets.push(vec![]);
        for it in old {
            let a = (mix(it.0) % (2 * m) as u64) as usize;
            self.buckets[a].push(it);
        }
        self.next += 1;
        if self.next == m {
            self.level += 1;
            self.next = 0;
        }
        self.splits += 1;
    }
    fn merge_last(&mut self) {
        if self.next == 0 {
            self.level -= 1;
            self.next = self.n0 << self.level;
        }
        self.next -= 1;
        let moved = self.buckets.pop().unwrap();
        self.buckets[self.next].extend(moved);
        self.merges += 1;
    }
    fn pages(&self) -> usize {
        self.buckets
            .iter()
            .map(|b| b.len().div_ceil(self.cap).max(1))
            .sum()
    }
    fn overflow_pages(&self) -> usize {
        self.pages() - self.buckets.len()
    }
    /// Page reads a lookup of `k` costs: primary page plus the overflow pages scanned before finding it (or all of them).
    fn lookup_pages(&self, k: u64) -> usize {
        let b = &self.buckets[self.addr(mix(k))];
        let found = b.iter().position(|it| it.0 == k).map_or(b.len(), |p| p + 1);
        found.div_ceil(self.cap).max(1)
    }
}
impl DynHash for Linear {
    fn get(&self, k: u64) -> Option<u64> {
        self.buckets[self.addr(mix(k))]
            .iter()
            .find(|it| it.0 == k)
            .map(|it| it.1)
    }
    fn insert(&mut self, k: u64, v: u64) -> Option<u64> {
        let a = self.addr(mix(k));
        if let Some(it) = self.buckets[a].iter_mut().find(|it| it.0 == k) {
            return Some(std::mem::replace(&mut it.1, v));
        }
        self.buckets[a].push((k, v));
        self.len += 1;
        while self.load() > self.max_load {
            self.split_next();
        }
        None
    }
    fn remove(&mut self, k: u64) -> Option<u64> {
        let a = self.addr(mix(k));
        let pos = self.buckets[a].iter().position(|it| it.0 == k)?;
        let (_, v) = self.buckets[a].swap_remove(pos);
        self.len -= 1;
        while self.load() < self.min_load && self.buckets.len() > self.n0 {
            self.merge_last();
        }
        Some(v)
    }
    fn len(&self) -> usize {
        self.len
    }
    fn check(&self) {
        assert_eq!(self.buckets.len(), (self.n0 << self.level) + self.next);
        for (i, b) in self.buckets.iter().enumerate() {
            assert!(
                b.iter().all(|it| self.addr(mix(it.0)) == i),
                "entry in the wrong bucket"
            );
        }
        assert_eq!(self.len, self.buckets.iter().map(Vec::len).sum::<usize>());
    }
    fn items(&self) -> Vec<Item> {
        self.buckets.iter().flatten().copied().collect()
    }
}

fn differential_hash<H: DynHash>(name: &str, h: &mut H, rng: &mut Rng) {
    let mut model: HashMap<u64, u64> = HashMap::new();
    let (mut ins, mut del, mut hits) = (0, 0, 0);
    for step in 0..30_000u64 {
        // First half grows (mostly inserts), second half churns (deletes dominate then balance).
        let del_pct = if step < 15_000 { 15 } else { 55 };
        let k = rng.below(6_000);
        match rng.below(100) {
            r if r < del_pct => {
                assert_eq!(h.remove(k), model.remove(&k), "remove {k}");
                del += 1;
            }
            r if r < del_pct + 45 => {
                assert_eq!(h.insert(k, step), model.insert(k, step), "insert {k}");
                ins += 1;
            }
            _ => {
                assert_eq!(h.get(k), model.get(&k).copied(), "get {k}");
                hits += usize::from(model.contains_key(&k));
            }
        }
        assert_eq!(h.len(), model.len());
        if step % 500 == 0 {
            h.check();
        }
    }
    h.check();
    // Drain phase: delete ~95% of the survivors so merges / reverse splits and directory shrinking run too.
    let mut keys: Vec<u64> = model.keys().copied().collect();
    keys.sort_unstable();
    for (i, k) in keys.iter().take(keys.len() * 95 / 100).enumerate() {
        assert_eq!(h.remove(*k), model.remove(k));
        if i % 200 == 0 {
            h.check();
        }
    }
    h.check();
    let mut got = h.items();
    got.sort_unstable();
    let mut want: Vec<Item> = model.into_iter().collect();
    want.sort_unstable();
    assert_eq!(got, want, "final contents");
    println!(
        "{name}: 30000 ops ({ins} inserts, {del} deletes, {hits} lookup hits) match HashMap, then drained 95%; final size {}",
        want.len()
    );
}

fn growth_stats(rng: &mut Rng) {
    const N: u64 = 50_000;
    println!("\ngrowth to {N} distinct keys (page capacity 32 entries):");
    let mut ext = Extendible::new(32);
    let mut lin = Linear::new(32);
    for i in 0..N {
        let k = rng.next();
        ext.insert(k, i);
        lin.insert(k, i);
    }
    ext.check();
    lin.check();
    let ext_util = ext.len as f64 / (ext.live_buckets() * ext.cap) as f64;
    let lin_util = lin.len as f64 / (lin.pages() * lin.cap) as f64;
    println!(
        "  extendible: {} buckets, directory {} slots (global depth {}), {} splits, {} doublings, utilisation {:.1}%",
        ext.live_buckets(),
        ext.dir.len(),
        ext.gd,
        ext.splits,
        ext.doublings,
        100.0 * ext_util
    );
    let sample: Vec<u64> = (0..2000).map(|_| rng.next()).collect(); // mostly misses: the worst case, scans whole chains
    let keys: Vec<u64> = ext.items().iter().step_by(25).map(|it| it.0).collect();
    let avg = |ks: &[u64]| {
        ks.iter().map(|&k| lin.lookup_pages(k)).sum::<usize>() as f64 / ks.len() as f64
    };
    println!(
        "  linear:     {} buckets + {} overflow pages, {} splits, utilisation {:.1}%, avg page reads: hit {:.3}, miss {:.3}",
        lin.buckets.len(),
        lin.overflow_pages(),
        lin.splits,
        100.0 * lin_util,
        avg(&keys),
        avg(&sample)
    );
    let btree_height = (N as f64).log(100.0).ceil();
    println!(
        "  page reads per point lookup: extendible 1 (directory in RAM), linear ~{:.2}, B+tree with fanout 100 ~{btree_height} (root cached: {})",
        avg(&keys),
        btree_height - 1.0
    );
    // Classic result: both settle near ln 2 = 69% (extendible) and the configured load factor (linear).
    assert!(
        (0.55..0.85).contains(&ext_util),
        "extendible utilisation ~ ln2"
    );
    assert!((0.55..0.85).contains(&lin_util));
    assert!(
        avg(&keys) < 1.2,
        "linear hashing keeps chains short on hits"
    );
}

// ---------- WAH-style compressed bitmaps ----------
const GROUP: u64 = 63;
const MASK: u64 = (1 << 63) - 1; // a full literal group
const FILL: u64 = 1 << 63; // flag: this word is a fill
const ONE: u64 = 1 << 62; // fill value bit
const LEN: u64 = ONE - 1; // fill length (in groups)

#[derive(Clone, Debug, PartialEq)]
struct Wah {
    words: Vec<u64>,
    nbits: usize,
}

#[derive(Default)]
struct Builder {
    words: Vec<u64>,
}
impl Builder {
    fn push_fill(&mut self, value: bool, n: u64) {
        if n == 0 {
            return;
        }
        if let Some(last) = self.words.last_mut()
            && *last & FILL != 0
            && (*last & ONE != 0) == value
            && (*last & LEN) + n < LEN
        {
            *last += n; // extend the previous fill instead of emitting a new word
            return;
        }
        self.words.push(FILL | if value { ONE } else { 0 } | n);
    }
    fn push_group(&mut self, g: u64) {
        match g {
            0 => self.push_fill(false, 1),
            MASK => self.push_fill(true, 1),
            _ => self.words.push(g),
        }
    }
}

/// Walks a compressed bitmap as a sequence of runs of 63-bit groups: (group value, run length, is_fill).
struct Cursor<'a> {
    words: &'a [u64],
    i: usize,
    rem: u64, // groups left in a partially consumed fill (0 = untouched)
}
impl<'a> Cursor<'a> {
    fn new(w: &'a Wah) -> Self {
        Cursor {
            words: &w.words,
            i: 0,
            rem: 0,
        }
    }
    fn run(&self) -> Option<(u64, u64, bool)> {
        let w = *self.words.get(self.i)?;
        Some(if w & FILL != 0 {
            (
                if w & ONE != 0 { MASK } else { 0 },
                if self.rem > 0 { self.rem } else { w & LEN },
                true,
            )
        } else {
            (w, 1, false)
        })
    }
    fn skip(&mut self, n: u64) {
        let w = self.words[self.i];
        if w & FILL == 0 {
            self.i += 1;
            return;
        }
        let len = if self.rem > 0 { self.rem } else { w & LEN };
        if n == len {
            self.i += 1;
            self.rem = 0;
        } else {
            self.rem = len - n;
        }
    }
}

impl Wah {
    fn groups(nbits: usize) -> u64 {
        (nbits as u64).div_ceil(GROUP)
    }
    fn from_positions(pos: impl Iterator<Item = u32>, nbits: usize) -> Wah {
        let mut b = Builder::default();
        let mut next_group = 0u64; // first group not yet emitted
        let mut cur: Option<(u64, u64)> = None; // (group index, bits)
        let flush = |b: &mut Builder, cur: (u64, u64), next_group: &mut u64| {
            b.push_fill(false, cur.0 - *next_group);
            b.push_group(cur.1);
            *next_group = cur.0 + 1;
        };
        for p in pos {
            let (g, bit) = (p as u64 / GROUP, p as u64 % GROUP);
            match cur {
                Some((cg, bits)) if cg == g => cur = Some((cg, bits | 1 << bit)),
                Some(c) => {
                    flush(&mut b, c, &mut next_group);
                    cur = Some((g, 1 << bit));
                }
                None => cur = Some((g, 1 << bit)),
            }
        }
        if let Some(c) = cur {
            flush(&mut b, c, &mut next_group);
        }
        b.push_fill(false, Self::groups(nbits) - next_group);
        Wah {
            words: b.words,
            nbits,
        }
    }
    fn ones(nbits: usize) -> Wah {
        let mut b = Builder::default();
        b.push_fill(true, nbits as u64 / GROUP);
        if nbits as u64 % GROUP > 0 {
            b.push_group((1 << (nbits as u64 % GROUP)) - 1); // the padding bits of the last group stay 0
        }
        Wah {
            words: b.words,
            nbits,
        }
    }
    fn bytes(&self) -> usize {
        self.words.len() * 8
    }
    /// Combines two bitmaps run by run WITHOUT decompressing them.
    fn binary(&self, other: &Wah, f: impl Fn(u64, u64) -> u64) -> Wah {
        assert_eq!(self.nbits, other.nbits);
        let (mut a, mut b, mut out) = (Cursor::new(self), Cursor::new(other), Builder::default());
        while let (Some((va, la, fa)), Some((vb, lb, fb))) = (a.run(), b.run()) {
            let n = if fa && fb { la.min(lb) } else { 1 };
            let g = f(va, vb) & MASK;
            if fa && fb {
                out.push_fill(g == MASK, n); // two fills combine in O(1) regardless of how many groups they cover
            } else {
                out.push_group(g);
            }
            a.skip(n);
            b.skip(n);
        }
        Wah {
            words: out.words,
            nbits: self.nbits,
        }
    }
    fn and(&self, o: &Wah) -> Wah {
        self.binary(o, |x, y| x & y)
    }
    fn or(&self, o: &Wah) -> Wah {
        self.binary(o, |x, y| x | y)
    }
    fn and_not(&self, o: &Wah) -> Wah {
        self.binary(o, |x, y| x & !y)
    }
    fn not(&self) -> Wah {
        Wah::ones(self.nbits).and_not(self) // masked complement: padding bits stay zero
    }
    fn count_ones(&self) -> u64 {
        let mut c = Cursor::new(self);
        let mut total = 0;
        while let Some((v, n, fill)) = c.run() {
            total += if fill {
                n * v.count_ones() as u64
            } else {
                v.count_ones() as u64
            };
            c.skip(n);
        }
        total
    }
    fn positions(&self) -> Vec<u32> {
        let (mut c, mut g, mut out) = (Cursor::new(self), 0u64, Vec::new());
        while let Some((v, n, _)) = c.run() {
            for k in 0..n {
                let mut bits = v;
                while bits != 0 {
                    out.push(((g + k) * GROUP) as u32 + bits.trailing_zeros());
                    bits &= bits - 1;
                }
            }
            g += n;
            c.skip(n);
        }
        out
    }
}

// ---------- bitmap indexes over a table ----------
struct Table {
    rows: usize,
    cols: Vec<Vec<u32>>,
    card: Vec<u32>,
}
struct EqIndex {
    maps: Vec<Wah>,
}
struct RangeIndex {
    cum: Vec<Wah>, // cum[v] = rows with value <= v
}
impl EqIndex {
    fn build(col: &[u32], card: u32) -> EqIndex {
        let mut lists: Vec<Vec<u32>> = vec![vec![]; card as usize];
        col.iter()
            .enumerate()
            .for_each(|(i, &v)| lists[v as usize].push(i as u32));
        EqIndex {
            maps: lists
                .into_iter()
                .map(|l| Wah::from_positions(l.into_iter(), col.len()))
                .collect(),
        }
    }
    fn bytes(&self) -> usize {
        self.maps.iter().map(Wah::bytes).sum()
    }
    /// A range as the OR of every value's bitmap (k bitmaps for k values).
    fn range(&self, lo: u32, hi: u32) -> Wah {
        (lo..=hi).map(|v| &self.maps[v as usize]).fold(
            Wah {
                words: vec![],
                nbits: 0,
            },
            |acc, m| {
                if acc.nbits == 0 { m.clone() } else { acc.or(m) }
            },
        )
    }
}
impl RangeIndex {
    fn build(eq: &EqIndex) -> RangeIndex {
        let mut cum: Vec<Wah> = Vec::with_capacity(eq.maps.len());
        for m in &eq.maps {
            cum.push(match cum.last() {
                Some(prev) => prev.or(m),
                None => m.clone(),
            });
        }
        RangeIndex { cum }
    }
    fn bytes(&self) -> usize {
        self.cum.iter().map(Wah::bytes).sum()
    }
    /// At most two bitmap reads and one AND-NOT, independent of the width of the range.
    fn range(&self, lo: u32, hi: u32) -> Wah {
        let upper = &self.cum[hi as usize];
        if lo == 0 {
            upper.clone()
        } else {
            upper.and_not(&self.cum[lo as usize - 1])
        }
    }
}

#[derive(Clone, Debug)]
enum Q {
    Eq(usize, u32),
    Range(usize, u32, u32),
    And(Box<Q>, Box<Q>),
    Or(Box<Q>, Box<Q>),
    Not(Box<Q>),
}
struct Indexes {
    eq: Vec<EqIndex>,
    range: Vec<RangeIndex>,
}
impl Indexes {
    fn eval(&self, q: &Q) -> Wah {
        match q {
            Q::Eq(c, v) => self.eq[*c].maps[*v as usize].clone(),
            Q::Range(c, lo, hi) => self.range[*c].range(*lo, *hi),
            Q::And(a, b) => self.eval(a).and(&self.eval(b)),
            Q::Or(a, b) => self.eval(a).or(&self.eval(b)),
            Q::Not(a) => self.eval(a).not(),
        }
    }
}
fn eval_row(q: &Q, t: &Table, row: usize) -> bool {
    match q {
        Q::Eq(c, v) => t.cols[*c][row] == *v,
        Q::Range(c, lo, hi) => (*lo..=*hi).contains(&t.cols[*c][row]),
        Q::And(a, b) => eval_row(a, t, row) && eval_row(b, t, row),
        Q::Or(a, b) => eval_row(a, t, row) || eval_row(b, t, row),
        Q::Not(a) => !eval_row(a, t, row),
    }
}
fn random_query(rng: &mut Rng, t: &Table, depth: u32) -> Q {
    if depth == 0 || rng.below(4) == 0 {
        let c = rng.below(t.cols.len() as u64) as usize;
        let card = t.card[c] as u64;
        return if rng.below(2) == 0 {
            Q::Eq(c, rng.below(card) as u32)
        } else {
            let (a, b) = (rng.below(card) as u32, rng.below(card) as u32);
            Q::Range(c, a.min(b), a.max(b))
        };
    }
    let kind = rng.below(3);
    let a = Box::new(random_query(rng, t, depth - 1));
    match kind {
        0 => Q::And(a, Box::new(random_query(rng, t, depth - 1))),
        1 => Q::Or(a, Box::new(random_query(rng, t, depth - 1))),
        _ => Q::Not(a),
    }
}

fn test_wah(rng: &mut Rng) {
    // Edge cases: empty, exactly one group, group boundary, long all-ones run, nbits not a multiple of 63.
    for nbits in [0usize, 1, 62, 63, 64, 126, 127, 1000, 63 * 5000 + 7] {
        let pos: Vec<u32> = (0..nbits as u32).filter(|_| rng.below(10) == 0).collect();
        let w = Wah::from_positions(pos.iter().copied(), nbits);
        assert_eq!(w.positions(), pos);
        assert_eq!(w.count_ones(), pos.len() as u64);
        let all = Wah::ones(nbits);
        assert_eq!(all.positions(), (0..nbits as u32).collect::<Vec<_>>());
        assert_eq!(
            w.not().positions(),
            (0..nbits as u32)
                .filter(|p| !pos.contains(p))
                .collect::<Vec<_>>()
        );
        assert_eq!(w.not().not(), w, "double negation is canonical");
    }
    // Random pairs with mixed density: dense, sparse, clustered.
    for _ in 0..300 {
        let nbits = 1 + rng.below(5000) as usize;
        let mk = |rng: &mut Rng| -> Vec<u32> {
            let (mode, p) = (rng.below(3), 1 + rng.below(60));
            (0..nbits as u32)
                .filter(|&i| {
                    if mode == 0 {
                        rng.below(100) < p
                    } else {
                        (i / 200) % 2 == (mode % 2) as u32 && rng.below(100) < p
                    }
                })
                .collect()
        };
        let (pa, pb) = (mk(rng), mk(rng));
        let (a, b) = (
            Wah::from_positions(pa.iter().copied(), nbits),
            Wah::from_positions(pb.iter().copied(), nbits),
        );
        let inter: Vec<u32> = pa.iter().copied().filter(|p| pb.contains(p)).collect();
        let mut uni = [pa.clone(), pb.clone()].concat();
        uni.sort_unstable();
        uni.dedup();
        assert_eq!(a.and(&b).positions(), inter);
        assert_eq!(a.or(&b).positions(), uni);
        assert_eq!(
            a.and_not(&b).positions(),
            pa.iter()
                .copied()
                .filter(|p| !pb.contains(p))
                .collect::<Vec<_>>()
        );
    }
    println!(
        "WAH: positions/count/NOT/AND/OR/AND-NOT verified on 9 edge sizes and 300 random bitmap pairs"
    );
}

fn gen_column(rng: &mut Rng, n: usize, card: u32, order: &str) -> Vec<u32> {
    match order {
        "random" => (0..n).map(|_| rng.below(card as u64) as u32).collect(),
        "sorted" => (0..n)
            .map(|i| (i as u64 * card as u64 / n as u64) as u32)
            .collect(),
        _ => {
            // clustered: the value changes only every ~1000 rows
            let mut cur = 0;
            (0..n)
                .map(|i| {
                    if i % 1000 == 0 {
                        cur = rng.below(card as u64) as u32;
                    }
                    cur
                })
                .collect()
        }
    }
}

fn test_queries(rng: &mut Rng) {
    const N: usize = 20_000;
    let card = vec![8u32, 20, 4];
    let cols: Vec<Vec<u32>> = card
        .iter()
        .zip(["random", "clustered", "sorted"])
        .map(|(&c, o)| gen_column(rng, N, c, o))
        .collect();
    let t = Table {
        rows: N,
        cols,
        card,
    };
    let eq: Vec<EqIndex> = (0..3)
        .map(|c| EqIndex::build(&t.cols[c], t.card[c]))
        .collect();
    let range = eq.iter().map(RangeIndex::build).collect();
    let ix = Indexes { eq, range };
    // The headline query from the docs: A = 3 AND (B = 1 OR NOT C = 0).
    let q = Q::And(
        Box::new(Q::Eq(0, 3)),
        Box::new(Q::Or(
            Box::new(Q::Eq(1, 1)),
            Box::new(Q::Not(Box::new(Q::Eq(2, 0)))),
        )),
    );
    let got = ix.eval(&q).positions();
    let want: Vec<u32> = (0..N)
        .filter(|&r| eval_row(&q, &t, r))
        .map(|r| r as u32)
        .collect();
    assert_eq!(got, want);
    println!(
        "A=3 AND (B=1 OR NOT C=0): {} of {N} rows, identical to a scan",
        got.len()
    );
    let (mut total, mut nonempty) = (0usize, 0);
    for _ in 0..500 {
        let q = random_query(rng, &t, 3);
        let res = ix.eval(&q);
        let want: Vec<u32> = (0..N)
            .filter(|&r| eval_row(&q, &t, r))
            .map(|r| r as u32)
            .collect();
        assert_eq!(res.positions(), want, "query {q:?}");
        assert_eq!(res.count_ones() as usize, want.len());
        total += want.len();
        nonempty += usize::from(!want.is_empty());
    }
    println!(
        "500 random boolean/range queries (depth <= 3): all equal a table scan ({nonempty} non-empty, {total} rows selected in total)"
    );
    assert!(nonempty > 100, "queries must not be trivially empty");
    // Range query cost: equality-encoded ORs k bitmaps, range-encoded touches two.
    let (e, r) = (ix.eq[1].range(3, 16), ix.range[1].range(3, 16));
    assert_eq!(e, r, "same answer, canonical compressed form");
}

fn compression_table(rng: &mut Rng) {
    const N: usize = 200_000;
    println!(
        "\nbitmap index size over {N} rows (raw = one uncompressed bitmap per value = card * N / 8 bytes):"
    );
    println!(
        "{:>7} {:>10} {:>12} {:>12} {:>9} {:>14}",
        "card", "order", "raw bytes", "WAH equality", "ratio", "WAH range-enc"
    );
    let mut ratio: HashMap<(u32, &str), f64> = HashMap::new();
    for card in [2u32, 16, 256, 4096] {
        for order in ["random", "clustered", "sorted"] {
            let col = gen_column(rng, N, card, order);
            let eq = EqIndex::build(&col, card);
            for (v, m) in eq
                .maps
                .iter()
                .enumerate()
                .step_by((card as usize / 16).max(1))
            {
                let expect: Vec<u32> = (0..N as u32)
                    .filter(|&i| col[i as usize] == v as u32)
                    .collect();
                assert_eq!(m.positions(), expect);
            }
            let raw = card as usize * N / 8;
            let r = eq.bytes() as f64 / raw as f64;
            ratio.insert((card, order), r);
            let range = if card <= 256 {
                format!("{:>12}", RangeIndex::build(&eq).bytes())
            } else {
                format!("{:>12}", "(skipped)")
            };
            println!(
                "{card:>7} {order:>10} {raw:>12} {:>12} {:>8.1}% {range:>14}",
                eq.bytes(),
                100.0 * r
            );
        }
    }
    assert!(
        ratio[&(16, "sorted")] < 0.01,
        "sorted columns compress to a few words per value"
    );
    assert!(ratio[&(16, "clustered")] < ratio[&(16, "random")]);
    assert!(
        (0.9..1.2).contains(&ratio[&(2, "random")]),
        "dense random bitmaps are incompressible (~1 bit/row)"
    );
    assert!(
        ratio[&(4096, "random")] < 0.1,
        "sparse bitmaps compress hard"
    );
    assert!(ratio[&(256, "random")] < 0.6);
    println!(
        "  rule of thumb: ratio falls with sortedness/clustering and with cardinality (each bitmap gets sparser)"
    );
}

fn main() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut ext = Extendible::new(8);
    differential_hash("extendible hashing (page cap 8)", &mut ext, &mut rng);
    println!(
        "  splits {}, doublings {}, merges {}, final directory 2^{} = {} slots over {} buckets",
        ext.splits,
        ext.doublings,
        ext.merges,
        ext.gd,
        ext.dir.len(),
        ext.live_buckets()
    );
    assert!(
        ext.splits > 100 && ext.merges > 10,
        "test must exercise split and merge"
    );
    let mut lin = Linear::new(8);
    differential_hash("linear hashing (page cap 8)", &mut lin, &mut rng);
    println!(
        "  splits {}, merges {}, final {} buckets (+{} overflow pages), level {}, split pointer {}",
        lin.splits,
        lin.merges,
        lin.buckets.len(),
        lin.overflow_pages(),
        lin.level,
        lin.next
    );
    assert!(lin.splits > 100 && lin.merges > 10);
    growth_stats(&mut rng);

    println!();
    test_wah(&mut rng);
    test_queries(&mut rng);
    compression_table(&mut rng);
    println!("ok");
}
