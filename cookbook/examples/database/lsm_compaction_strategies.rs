//! RECIPE 703 (database): LSM compaction strategies (leveled, size-tiered, lazy leveling, FIFO) with exact amplification accounting and Monkey-style bloom filters.
//! LEVEL: advanced
//! Use in: choosing and tuning the compaction policy of an LSM key-value store or log-structured table.   Run: cargo run --release --example lsm_compaction_strategies
//! COMMON USES: leveled compaction is RocksDB/LevelDB's default, size-tiered is Cassandra's STCS (and RocksDB "universal"),
//! FIFO/TTL tables are used for time-series and cache-like data (RocksDB FIFO compaction, Cassandra TWCS is the windowed
//! cousin), and lazy leveling / Monkey bloom-filter allocation come from the Dostoevsky and Monkey research on tuning LSM-trees.
//! Course: CMU 15-445 Intro to Database Systems, Lecture 05 (Storage Models and the LSM tree; see also CMU 15-746 Storage Systems).
#![allow(dead_code)]
//!
//! WHAT: an LSM tree buffers writes in a memtable, flushes it as an immutable sorted run (SSTable) and relies on COMPACTION to
//! keep the number of runs a read must visit small. Compaction policy trades three amplifications against each other:
//!   WRITE amp  = entries written to storage (flushes + compaction output) / user writes,
//!   READ amp   = table reads (IOs) per point lookup (a run is only read when its key range and its bloom filter admit the key),
//!   SPACE amp  = entries stored / live distinct keys (overwritten values and tombstones that compaction has not yet purged).
//! POLICIES (size ratio T = 4, memtable M = 128 entries; runs here are whole sorted runs, the analysis model of Monkey and
//! Dostoevsky; RocksDB compacts file-sized slices of a level but has the same asymptotics):
//! * LEVELED: one run per level, level i holds up to M*T^(i+1); a run merging into the next level rewrites it, so each entry is
//!   rewritten about T/2 times per level: high write amp, minimal space amp, fewest runs to read.
//! * SIZE-TIERED (Cassandra STCS): runs of similar size are bucketed and merged when a bucket has T of them: each entry is
//!   rewritten about once per size class (low write amp), but overlapping runs pile up: more runs per lookup, more space amp.
//! * LAZY LEVELING (Dostoevsky): tiering at every level except the largest, which is leveled: near-tiered write cost and near
//!   leveled space/point-read cost, because most data (and most space waste) lives in the single last-level run.
//! * FIFO/TTL: never merge; drop the oldest run when over a size cap or older than a TTL. Write amp is the minimum possible (every entry is
//!   written once, at flush), and data disappears: right for time-series or caches, wrong for a database of record.
//! TOMBSTONES: a delete is a record. It may be dropped only when no run outside the merge still holds an older version of
//! the key (otherwise the old value would come back from the dead); here this is checked exactly, usually true only when the
//! merge reaches the bottom. BLOOM FILTERS: one per run. With the same total memory, MONKEY allocates fewer bits per key to
//! the big deep runs and more to the small shallow ones (false-positive rate proportional to run size, found by bisection on
//! the Lagrange multiplier), which lowers the sum of false-positive rates, i.e. the IO of lookups for absent keys.
//!
//! VERIFIED: for every policy, on 20k-operation seeded workloads (uniform and zipf, write- and read-heavy), every get matches a
//! `BTreeMap` model, and after EVERY flush (the main workload; every 4th on the others) all keys (present and absent) and a
//! range scan are re-checked, plus structural invariants (level shapes, sorted runs, unique (key, seq) versions). FIFO is held
//! to the weaker contract that it may forget old data but never returns a stale or deleted value. The textbook ordering is
//! asserted: leveled write amp > tiered, leveled space amp < tiered, lazy write amp < leveled, scans cheaper under leveled,
//! point lookups for absent keys: no bloom > uniform bloom > Monkey bloom at equal memory, leveled < tiered with blooms.
//!
//! Read: https://www.cs.umb.edu/~poneil/lsmtree.pdf (O'Neil et al., The Log-Structured Merge-Tree),
//! https://nivdayan.github.io/dostoevsky.pdf (Dayan & Idreos, Dostoevsky: lazy leveling; its sibling paper Monkey derives the
//! bloom allocation), https://github.com/facebook/rocksdb/wiki/Leveled-Compaction and
//! https://github.com/facebook/rocksdb/wiki/Universal-Compaction (RocksDB's two main policies)
use std::collections::{BTreeMap, HashMap};

const M: usize = 128; // memtable capacity in entries
const T: usize = 4; // size ratio / fan-in
const KEYS: usize = 6000; // distinct key ranks; written keys are even, odd keys are never written (guaranteed misses)
const LN2: f64 = std::f64::consts::LN_2;

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
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

// ---------- bloom filter ----------
#[derive(Clone)]
struct Bloom {
    bits: Vec<u64>,
    m: usize,
    k: u32,
}
impl Bloom {
    fn build(keys: &[u64], bpk: f64) -> Option<Bloom> {
        if bpk <= 0.05 || keys.is_empty() {
            return None;
        }
        let m = ((keys.len() as f64 * bpk).ceil() as usize).max(64);
        let k = ((bpk * LN2).round() as u32).max(1);
        let mut b = Bloom {
            bits: vec![0; m.div_ceil(64)],
            m,
            k,
        };
        for &key in keys {
            let (h1, h2) = Bloom::hashes(key);
            for i in 0..k as u64 {
                let bit = (h1.wrapping_add(i.wrapping_mul(h2)) % m as u64) as usize;
                b.bits[bit / 64] |= 1 << (bit % 64);
            }
        }
        Some(b)
    }
    fn hashes(key: u64) -> (u64, u64) {
        let h = mix(key ^ 0x5bd1_e995);
        (h, mix(h) | 1)
    }
    fn maybe(&self, key: u64) -> bool {
        let (h1, h2) = Bloom::hashes(key);
        (0..self.k as u64).all(|i| {
            let bit = (h1.wrapping_add(i.wrapping_mul(h2)) % self.m as u64) as usize;
            self.bits[bit / 64] >> (bit % 64) & 1 == 1
        })
    }
}

/// Monkey: minimise the sum of false-positive rates p_i for a fixed memory budget (bits). Optimal Bloom filters need
/// n_i*ln(1/p_i)/ln2^2 bits; the Lagrangian gives p_i = min(1, lambda*n_i). Bisect lambda until the budget is spent.
fn monkey_bits_per_key(sizes: &[usize], budget_bits: f64) -> Vec<f64> {
    let spent = |lambda: f64| -> f64 {
        sizes
            .iter()
            .map(|&n| {
                let p = (lambda * n as f64).min(1.0);
                n as f64 * (1.0 / p).ln() / (LN2 * LN2)
            })
            .sum()
    };
    let (mut lo, mut hi) = (1e-15f64, 1.0f64);
    for _ in 0..200 {
        let mid = (lo * hi).sqrt();
        if spent(mid) > budget_bits {
            lo = mid
        } else {
            hi = mid
        }
    }
    sizes
        .iter()
        .map(|&n| {
            let p = (hi * n as f64).min(1.0);
            (1.0 / p).ln() / (LN2 * LN2)
        })
        .collect()
}

// ---------- runs ----------
#[derive(Clone, Copy, Debug)]
struct Entry {
    key: u64,
    seq: u64,
    val: Option<u64>, // None = tombstone
}

#[derive(Clone)]
struct Sst {
    entries: Vec<Entry>,
    min: u64,
    max: u64,
    min_seq: u64,
    max_seq: u64,
    bloom: Option<Bloom>,
}
impl Sst {
    fn find(&self, key: u64) -> Option<Entry> {
        self.entries
            .binary_search_by_key(&key, |e| e.key)
            .ok()
            .map(|i| self.entries[i])
    }
    fn covers(&self, key: u64) -> bool {
        self.min <= key && key <= self.max
    }
    fn keys(&self) -> Vec<u64> {
        self.entries.iter().map(|e| e.key).collect()
    }
}

#[derive(Clone, Copy)]
enum FilterMode {
    None,
    Uniform(f64),
    /// Same total memory as `Uniform(b)` over the current data, spent the Monkey way.
    Monkey(f64),
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Policy {
    Leveled,
    SizeTiered,
    Lazy,
    Fifo { cap: usize, ttl: Option<u64> },
}

#[derive(Clone, Default, Debug)]
struct Stats {
    user_writes: u64,
    flush_written: u64,
    compact_written: u64,
    compact_read: u64,
    compactions: u64,
    table_reads: u64,
    bloom_probes: u64,
    bloom_fp: u64,
    tombstones_dropped: u64,
    tombstones_kept: u64,
    scan_seeks: u64,
    scan_entries: u64,
}

#[derive(Clone)]
struct Tree {
    policy: Policy,
    mem: BTreeMap<u64, Entry>,
    levels: Vec<Vec<Sst>>,
    seq: u64,
    flushes: u64,
    cutoff: u64, // FIFO: highest seq ever dropped
    dropped_tables: u64,
    mode: FilterMode,
    st: Stats,
}

impl Tree {
    fn new(policy: Policy) -> Tree {
        Tree {
            policy,
            mem: BTreeMap::new(),
            levels: vec![Vec::new()],
            seq: 0,
            flushes: 0,
            cutoff: 0,
            dropped_tables: 0,
            mode: FilterMode::Uniform(5.0),
            st: Stats::default(),
        }
    }
    fn cap(i: usize) -> usize {
        M * T.pow(i as u32 + 1)
    }
    fn make_sst(&self, entries: Vec<Entry>) -> Sst {
        let keys: Vec<u64> = entries.iter().map(|e| e.key).collect();
        let bloom = match self.mode {
            FilterMode::Uniform(b) => Bloom::build(&keys, b),
            _ => None, // Monkey/None are applied by `apply_filters` once the shape is known
        };
        Sst {
            min: entries[0].key,
            max: entries.last().unwrap().key,
            min_seq: entries.iter().map(|e| e.seq).min().unwrap(),
            max_seq: entries.iter().map(|e| e.seq).max().unwrap(),
            entries,
            bloom,
        }
    }
    fn runs(&self) -> impl Iterator<Item = &Sst> {
        self.levels.iter().flatten()
    }
    fn total_entries(&self) -> usize {
        self.mem.len() + self.runs().map(|s| s.entries.len()).sum::<usize>()
    }
    fn apply_filters(&mut self, mode: FilterMode) {
        self.mode = mode;
        let sizes: Vec<usize> = self.runs().map(|s| s.entries.len()).collect();
        let n: usize = sizes.iter().sum();
        let bpk: Vec<f64> = match mode {
            FilterMode::None => vec![0.0; sizes.len()],
            FilterMode::Uniform(b) => vec![b; sizes.len()],
            FilterMode::Monkey(b) => monkey_bits_per_key(&sizes, b * n as f64),
        };
        let mut it = bpk.into_iter();
        for s in self.levels.iter_mut().flatten() {
            s.bloom = Bloom::build(&s.keys(), it.next().unwrap());
        }
    }
    fn filter_bits(&self) -> usize {
        self.runs()
            .filter_map(|s| s.bloom.as_ref())
            .map(|b| b.m)
            .sum()
    }

    // ----- writes -----
    fn put(&mut self, key: u64) {
        self.write(key, true);
    }
    fn delete(&mut self, key: u64) {
        self.write(key, false);
    }
    fn write(&mut self, key: u64, put: bool) -> bool {
        self.seq += 1;
        self.st.user_writes += 1;
        self.mem.insert(
            key,
            Entry {
                key,
                seq: self.seq,
                val: put.then_some(self.seq),
            },
        );
        if self.mem.len() >= M {
            self.flush();
            return true;
        }
        false
    }
    fn flush(&mut self) {
        let entries: Vec<Entry> = std::mem::take(&mut self.mem).into_values().collect();
        self.st.flush_written += entries.len() as u64;
        self.flushes += 1;
        let s = self.make_sst(entries);
        match self.policy {
            Policy::Leveled => self.leveled_push(0, s),
            Policy::Lazy => self.lazy_push(0, s),
            Policy::SizeTiered => self.tiered_add(s),
            Policy::Fifo { cap, ttl } => self.fifo_add(s, cap, ttl),
        }
    }
    /// A tombstone may go only if no run left in the tree holds an older version of its key.
    fn older_exists(&self, e: &Entry) -> bool {
        self.runs()
            .any(|s| s.covers(e.key) && s.find(e.key).is_some_and(|o| o.seq < e.seq))
    }
    /// Merge runs that have been taken out of the tree; the newest version of each key wins.
    fn merge_to_sst(&mut self, runs: Vec<Sst>) -> Option<Sst> {
        let mut best: BTreeMap<u64, Entry> = BTreeMap::new();
        for e in runs.iter().flat_map(|s| s.entries.iter()) {
            self.st.compact_read += 1;
            best.entry(e.key)
                .and_modify(|b| {
                    if e.seq > b.seq {
                        *b = *e
                    }
                })
                .or_insert(*e);
        }
        let mut out = Vec::with_capacity(best.len());
        for e in best.into_values() {
            if e.val.is_none() {
                if self.older_exists(&e) {
                    self.st.tombstones_kept += 1;
                } else {
                    self.st.tombstones_dropped += 1;
                    continue;
                }
            }
            out.push(e);
        }
        self.st.compactions += 1;
        self.st.compact_written += out.len() as u64;
        (!out.is_empty()).then(|| self.make_sst(out))
    }

    fn ensure(&mut self, i: usize) {
        while self.levels.len() <= i {
            self.levels.push(Vec::new());
        }
    }
    fn leveled_push(&mut self, i: usize, run: Sst) {
        self.ensure(i);
        let merged = match self.levels[i].pop() {
            None => Some(run), // moving a run into an empty level is free (a "trivial move")
            Some(old) => self.merge_to_sst(vec![run, old]),
        };
        if let Some(s) = merged {
            if s.entries.len() > Tree::cap(i) {
                self.leveled_push(i + 1, s)
            } else {
                self.levels[i].push(s)
            }
        }
    }
    fn lazy_push(&mut self, i: usize, run: Sst) {
        self.ensure(i + 1);
        let is_last = self.levels[i + 1..].iter().all(|l| l.is_empty());
        if is_last {
            let merged = match self.levels[i].pop() {
                None => Some(run),
                Some(old) => self.merge_to_sst(vec![run, old]),
            };
            if let Some(s) = merged {
                if s.entries.len() > Tree::cap(i) {
                    self.lazy_push(i + 1, s)
                } else {
                    self.levels[i].push(s)
                }
            }
        } else {
            self.levels[i].insert(0, run); // tiered level: just accumulate runs
            if self.levels[i].len() >= T {
                let all = std::mem::take(&mut self.levels[i]);
                if let Some(s) = self.merge_to_sst(all) {
                    self.lazy_push(i + 1, s);
                }
            }
        }
    }
    /// Cassandra-style buckets: sizes within [0.5, 1.5] of the bucket average are "similar".
    fn pick_bucket(&self) -> Option<Vec<usize>> {
        let mut idx: Vec<usize> = (0..self.levels[0].len()).collect();
        idx.sort_by_key(|&i| self.levels[0][i].entries.len());
        let mut bucket: Vec<usize> = Vec::new();
        let mut buckets: Vec<Vec<usize>> = Vec::new();
        for i in idx {
            let n = self.levels[0][i].entries.len() as f64;
            let avg = bucket
                .iter()
                .map(|&j| self.levels[0][j].entries.len() as f64)
                .sum::<f64>()
                / bucket.len().max(1) as f64;
            if bucket.is_empty() || (n >= 0.5 * avg && n <= 1.5 * avg) {
                bucket.push(i);
            } else {
                buckets.push(std::mem::take(&mut bucket));
                bucket.push(i);
            }
        }
        buckets.push(bucket);
        buckets.into_iter().find(|b| b.len() >= T).map(|mut b| {
            b.truncate(32);
            b
        })
    }
    fn tiered_add(&mut self, s: Sst) {
        self.levels[0].push(s);
        while let Some(mut group) = self.pick_bucket() {
            group.sort_unstable_by(|a, b| b.cmp(a));
            let runs: Vec<Sst> = group
                .into_iter()
                .map(|i| self.levels[0].swap_remove(i))
                .collect();
            if let Some(s) = self.merge_to_sst(runs) {
                self.levels[0].push(s);
            }
        }
    }
    fn fifo_add(&mut self, s: Sst, cap: usize, ttl: Option<u64>) {
        self.levels[0].push(s);
        loop {
            let total: usize = self.levels[0].iter().map(|s| s.entries.len()).sum();
            let oldest = (0..self.levels[0].len())
                .min_by_key(|&i| self.levels[0][i].max_seq)
                .unwrap();
            let expired = ttl.is_some_and(|t| self.seq - self.levels[0][oldest].max_seq > t);
            if self.levels[0].len() > 1 && (total > cap || expired) {
                let gone = self.levels[0].swap_remove(oldest);
                self.cutoff = self.cutoff.max(gone.max_seq);
                self.dropped_tables += 1;
            } else {
                break;
            }
        }
    }
    fn major_compact(&mut self) {
        let all: Vec<Sst> = std::mem::take(&mut self.levels)
            .into_iter()
            .flatten()
            .collect();
        self.levels = vec![Vec::new()];
        if let Some(s) = self.merge_to_sst(all) {
            self.levels[0].push(s);
        }
    }

    // ----- reads -----
    /// Newest version of `key`. Runs are visited newest-first (by max seq) and the search stops as soon as the best
    /// version found is newer than everything remaining. A run costs one table read only if range and bloom admit the key.
    fn lookup(&mut self, key: u64, count: bool) -> Option<Entry> {
        if let Some(e) = self.mem.get(&key) {
            return Some(*e);
        }
        let mut cands: Vec<(u64, usize, usize)> = Vec::new();
        for (l, lv) in self.levels.iter().enumerate() {
            for (i, s) in lv.iter().enumerate() {
                if s.covers(key) {
                    cands.push((s.max_seq, l, i));
                }
            }
        }
        cands.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        let mut best: Option<Entry> = None;
        for (max_seq, l, i) in cands {
            if best.is_some_and(|b| b.seq > max_seq) {
                break;
            }
            let s = &self.levels[l][i];
            if let Some(b) = &s.bloom {
                if count {
                    self.st.bloom_probes += 1;
                }
                if !b.maybe(key) {
                    continue;
                }
            }
            if count {
                self.st.table_reads += 1;
            }
            match s.find(key) {
                Some(e) if best.is_none_or(|b| e.seq > b.seq) => best = Some(e),
                Some(_) => {}
                None => {
                    if count && s.bloom.is_some() {
                        self.st.bloom_fp += 1;
                    }
                }
            }
        }
        best
    }
    fn get(&mut self, key: u64) -> Option<u64> {
        self.lookup(key, true).and_then(|e| e.val)
    }
    fn peek(&mut self, key: u64) -> Option<u64> {
        self.lookup(key, false).and_then(|e| e.val)
    }
    /// Range scan [lo, hi]: every overlapping run costs a seek; entries in range are read.
    fn scan(&mut self, lo: u64, hi: u64, count: bool) -> Vec<(u64, u64)> {
        let mut best: BTreeMap<u64, Entry> =
            self.mem.range(lo..=hi).map(|(&k, &e)| (k, e)).collect();
        for s in self.levels.iter().flatten() {
            if s.max < lo || s.min > hi {
                continue;
            }
            let a = s.entries.partition_point(|e| e.key < lo);
            let b = s.entries.partition_point(|e| e.key <= hi);
            if count {
                self.st.scan_seeks += 1;
                self.st.scan_entries += (b - a) as u64;
            }
            for e in &s.entries[a..b] {
                best.entry(e.key)
                    .and_modify(|x| {
                        if e.seq > x.seq {
                            *x = *e
                        }
                    })
                    .or_insert(*e);
            }
        }
        best.into_values()
            .filter_map(|e| e.val.map(|v| (e.key, v)))
            .collect()
    }
    fn tombstones(&self) -> usize {
        self.runs()
            .flat_map(|s| s.entries.iter())
            .filter(|e| e.val.is_none())
            .count()
            + self.mem.values().filter(|e| e.val.is_none()).count()
    }
    fn run_count(&self) -> usize {
        self.runs().count()
    }

    fn check_invariants(&self) {
        let mut seen: HashMap<(u64, u64), ()> = HashMap::new();
        for s in self.runs() {
            assert!(
                s.entries.windows(2).all(|w| w[0].key < w[1].key),
                "run not strictly sorted"
            );
            assert_eq!(
                (s.min, s.max),
                (s.entries[0].key, s.entries.last().unwrap().key)
            );
            assert!(
                s.entries
                    .iter()
                    .all(|e| s.min_seq <= e.seq && e.seq <= s.max_seq)
            );
            for e in &s.entries {
                assert!(
                    seen.insert((e.key, e.seq), ()).is_none(),
                    "version stored twice"
                );
            }
        }
        match self.policy {
            Policy::Leveled => {
                for (i, l) in self.levels.iter().enumerate() {
                    assert!(l.len() <= 1, "leveled: one run per level");
                    assert!(l.iter().all(|s| s.entries.len() <= Tree::cap(i)));
                }
            }
            Policy::Lazy => {
                let last = self.levels.iter().rposition(|l| !l.is_empty()).unwrap_or(0);
                for (i, l) in self.levels.iter().enumerate() {
                    if i == last {
                        assert!(l.len() <= 1 && l.iter().all(|s| s.entries.len() <= Tree::cap(i)));
                    } else {
                        assert!(l.len() < T, "lazy: tiered levels hold < T runs");
                    }
                }
            }
            Policy::SizeTiered => assert!(
                self.pick_bucket().is_none(),
                "a compactable bucket was left behind"
            ),
            Policy::Fifo { cap, .. } => assert!(
                self.levels[0].len() <= 1
                    || self.levels[0]
                        .iter()
                        .map(|s| s.entries.len())
                        .sum::<usize>()
                        <= cap
            ),
        }
    }
}

// ---------- workloads ----------
#[derive(Clone, Copy)]
struct Workload {
    name: &'static str,
    zipf: bool,
    put_pct: usize,
    del_pct: usize, // remainder are gets
}
const WORKLOADS: [Workload; 4] = [
    Workload {
        name: "zipf write-heavy",
        zipf: true,
        put_pct: 88,
        del_pct: 7,
    },
    Workload {
        name: "uniform write-heavy",
        zipf: false,
        put_pct: 88,
        del_pct: 7,
    },
    Workload {
        name: "zipf read-heavy",
        zipf: true,
        put_pct: 12,
        del_pct: 3,
    },
    Workload {
        name: "uniform read-heavy",
        zipf: false,
        put_pct: 12,
        del_pct: 3,
    },
];

struct KeyGen {
    cdf: Vec<f64>,
    zipf: bool,
}
impl KeyGen {
    fn new(zipf: bool) -> KeyGen {
        let w: Vec<f64> = (1..=KEYS).map(|r| 1.0 / (r as f64).powf(0.99)).collect();
        let total: f64 = w.iter().sum();
        let mut acc = 0.0;
        KeyGen {
            cdf: w
                .iter()
                .map(|x| {
                    acc += x / total;
                    acc
                })
                .collect(),
            zipf,
        }
    }
    fn next(&self, rng: &mut Rng) -> u64 {
        let rank = if self.zipf {
            let u = rng.unit();
            self.cdf.partition_point(|&c| c < u).min(KEYS - 1)
        } else {
            rng.below(KEYS)
        };
        2 * ((rank * 7919) % KEYS) as u64 // scatter hot ranks over the key space; only even keys are ever written
    }
}

struct Model {
    live: BTreeMap<u64, u64>,
    last_seq: HashMap<u64, u64>,
}

/// Checks the tree against the model. Exact for every policy except FIFO, which may only forget (never invent or resurrect).
fn verify(tree: &mut Tree, model: &Model, with_scan: bool) {
    let fifo = matches!(tree.policy, Policy::Fifo { .. });
    for r in 0..KEYS as u64 {
        for k in [2 * r, 2 * r + 1] {
            let (got, want) = (tree.peek(k), model.live.get(&k).copied());
            if got == want {
                continue;
            }
            let forgotten =
                got.is_none() && model.last_seq.get(&k).is_some_and(|&s| s <= tree.cutoff);
            assert!(
                fifo && forgotten,
                "{:?}: key {k}: got {got:?}, model {want:?}",
                tree.policy
            );
        }
    }
    if with_scan && !fifo {
        let (lo, hi) = (300, 900);
        let want: Vec<(u64, u64)> = model.live.range(lo..=hi).map(|(&k, &v)| (k, v)).collect();
        assert_eq!(
            tree.scan(lo, hi, false),
            want,
            "{:?}: scan differs",
            tree.policy
        );
    }
}

struct Report {
    tree: Tree,
    model: Model,
    wa: f64,
    sa: f64,
    max_sa: f64,
    forgotten: usize,
}

fn space_amp(tree: &Tree, model: &Model) -> f64 {
    tree.total_entries() as f64 / model.live.len().max(1) as f64
}

fn run_workload(policy: Policy, w: Workload, ops: usize, verify_every: u64, seed: u64) -> Report {
    let mut tree = Tree::new(policy);
    let mut model = Model {
        live: BTreeMap::new(),
        last_seq: HashMap::new(),
    };
    let (mut rng, kg) = (Rng(seed), KeyGen::new(w.zipf));
    let (mut max_sa, mut verified) = (0.0f64, 0u64);
    let fifo = matches!(policy, Policy::Fifo { .. });
    for _ in 0..ops {
        let k = kg.next(&mut rng);
        let r = rng.below(100);
        if r < w.put_pct || r < w.put_pct + w.del_pct {
            let put = r < w.put_pct;
            let flushed = tree.write(k, put);
            model.last_seq.insert(k, tree.seq);
            if put {
                model.live.insert(k, tree.seq);
            } else {
                model.live.remove(&k);
            }
            if flushed {
                tree.check_invariants();
                if tree.flushes % verify_every == 0 {
                    verify(&mut tree, &model, true);
                    verified += 1;
                }
                if tree.flushes % 8 == 0 && model.live.len() > 100 {
                    max_sa = max_sa.max(space_amp(&tree, &model));
                }
            }
        } else {
            let got = tree.get(k);
            let want = model.live.get(&k).copied();
            let forgotten =
                fifo && got.is_none() && model.last_seq.get(&k).is_some_and(|&s| s <= tree.cutoff);
            assert!(
                got == want || forgotten,
                "{policy:?}: get({k}) = {got:?}, model {want:?}"
            );
        }
    }
    verify(&mut tree, &model, true);
    assert!(verified > 0);
    let forgotten = model
        .live
        .iter()
        .filter(|&(&k, _)| tree.peek(k).is_none())
        .count();
    let wa = (tree.st.flush_written + tree.st.compact_written) as f64 / tree.st.user_writes as f64;
    let sa = space_amp(&tree, &model);
    max_sa = max_sa.max(sa);
    Report {
        tree,
        model,
        wa,
        sa,
        max_sa,
        forgotten,
    }
}

#[derive(Default, Clone, Copy)]
struct ReadCost {
    hit_io: f64,
    miss_io: f64,
    scan_seeks: f64,
}
/// Point lookups for present keys, for guaranteed-absent odd keys, and 16-key range scans, on the final tree.
fn measure(tree: &mut Tree, model: &Model) -> ReadCost {
    let before = tree.st.clone();
    let live: Vec<u64> = model.live.keys().copied().collect();
    let mut rng = Rng(4242);
    let n = 2000;
    for _ in 0..n {
        let _ = tree.get(live[rng.below(live.len())]);
    }
    let hit = tree.st.table_reads - before.table_reads;
    let mid = tree.st.table_reads;
    for _ in 0..n {
        assert!(tree.get(2 * rng.below(KEYS) as u64 + 1).is_none());
    }
    let miss = tree.st.table_reads - mid;
    let s0 = tree.st.scan_seeks;
    for _ in 0..200 {
        let lo = 2 * rng.below(KEYS - 20) as u64;
        let _ = tree.scan(lo, lo + 32, true);
    }
    ReadCost {
        hit_io: hit as f64 / n as f64,
        miss_io: miss as f64 / n as f64,
        scan_seeks: (tree.st.scan_seeks - s0) as f64 / 200.0,
    }
}

fn unit_tests() {
    // Bloom filter: measured false-positive rate near the textbook (0.6185)^bpk.
    let keys: Vec<u64> = (0..20_000).map(|i| mix(i) | 1).collect();
    let b = Bloom::build(&keys, 10.0).unwrap();
    assert!(keys.iter().all(|&k| b.maybe(k)), "no false negatives");
    let fp = (0..50_000u64)
        .filter(|&i| b.maybe(mix(i + 1_000_000) & !1))
        .count() as f64
        / 50_000.0;
    println!(
        "bloom: 10 bits/key, k={}, measured false-positive rate {:.3}% (theory {:.3}%)",
        b.k,
        fp * 100.0,
        100.0 * 0.6185f64.powf(10.0)
    );
    assert!(fp < 0.02);
    // Monkey allocation: same memory, strictly smaller sum of false-positive rates, deeper runs get fewer bits per key.
    let sizes = [512usize, 2048, 8192, 32768];
    let budget = 5.0 * sizes.iter().sum::<usize>() as f64;
    let bpk = monkey_bits_per_key(&sizes, budget);
    let used: f64 = sizes.iter().zip(&bpk).map(|(&n, &b)| n as f64 * b).sum();
    assert!(
        (used - budget).abs() / budget < 1e-6,
        "monkey spends exactly the budget"
    );
    assert!(
        bpk.windows(2).all(|w| w[0] >= w[1]),
        "bigger (deeper) runs get fewer bits per key: {bpk:?}"
    );
    let sum_p = |v: &[f64]| v.iter().map(|b| (-b * LN2 * LN2).exp()).sum::<f64>();
    let (pm, pu) = (sum_p(&bpk), sum_p(&[5.0; 4]));
    println!(
        "monkey allocation over runs {sizes:?}: bits/key {:?} -> sum of FPRs {pm:.3} vs uniform {pu:.3}",
        bpk.iter()
            .map(|b| (b * 100.0).round() / 100.0)
            .collect::<Vec<_>>()
    );
    assert!(pm < pu);
}

fn main() {
    unit_tests();
    let policies = [
        Policy::Leveled,
        Policy::SizeTiered,
        Policy::Lazy,
        Policy::Fifo {
            cap: 1500,
            ttl: None,
        },
    ];
    let names = ["leveled", "size-tiered", "lazy-leveling", "fifo(cap 1500)"];
    let mut reports: Vec<Vec<(Report, ReadCost)>> = Vec::new();
    println!(
        "\n20000 ops per cell, M={M} entries/memtable, T={T}, {KEYS} key ranks, 5 bits/key uniform bloom, every get checked against a BTreeMap"
    );
    for (wi, w) in WORKLOADS.iter().enumerate() {
        let mut row = Vec::new();
        println!(
            "\nworkload: {}  ({}% put, {}% delete, rest get)",
            w.name, w.put_pct, w.del_pct
        );
        println!(
            "{:<15} {:>6} {:>7} {:>7} {:>5} {:>5} {:>9} {:>9} {:>8} {:>9}",
            "policy",
            "WA",
            "SA end",
            "SA max",
            "runs",
            "lvls",
            "IO/hit",
            "IO/miss",
            "scan IO",
            "forgot"
        );
        for (pi, &p) in policies.iter().enumerate() {
            let verify_every = if wi == 0 { 1 } else { 4 };
            let mut rep = run_workload(p, *w, 20_000, verify_every, 0x15a + wi as u64);
            let rc = measure(&mut rep.tree, &rep.model);
            let lv = rep.tree.levels.iter().filter(|l| !l.is_empty()).count();
            println!(
                "{:<15} {:>6.2} {:>7.2} {:>7.2} {:>5} {:>5} {:>9.2} {:>9.2} {:>8.1} {:>9}",
                names[pi],
                rep.wa,
                rep.sa,
                rep.max_sa,
                rep.tree.run_count(),
                lv,
                rc.hit_io,
                rc.miss_io,
                rc.scan_seeks,
                rep.forgotten
            );
            row.push((rep, rc));
        }
        reports.push(row);
    }

    // ----- the textbook trade-offs, on the write-heavy workloads -----
    for wi in 0..2 {
        let r = &reports[wi];
        let (lev, tier, lazy, fifo) = (&r[0], &r[1], &r[2], &r[3]);
        let n = WORKLOADS[wi].name;
        assert!(
            lev.0.wa > tier.0.wa,
            "{n}: leveled write amp {:.2} must exceed tiered {:.2}",
            lev.0.wa,
            tier.0.wa
        );
        assert!(
            lev.0.sa < tier.0.sa,
            "{n}: leveled space amp {:.2} must be below tiered {:.2}",
            lev.0.sa,
            tier.0.sa
        );
        assert!(lev.0.max_sa < tier.0.max_sa);
        assert!(
            lazy.0.wa < lev.0.wa,
            "{n}: lazy leveling writes less than leveled"
        );
        assert!(
            lev.1.scan_seeks < tier.1.scan_seeks,
            "{n}: scans visit fewer runs under leveling"
        );
        assert_eq!(
            fifo.0.tree.st.compact_written, 0,
            "FIFO never rewrites anything"
        );
        assert!(
            fifo.0.wa < tier.0.wa && fifo.0.wa <= 1.0,
            "so its write amp (only memtable-absorbed flushes) is the lowest"
        );
        assert!(fifo.0.forgotten > 0, "and it forgets old data");
        assert!(lev.0.forgotten == 0 && tier.0.forgotten == 0 && lazy.0.forgotten == 0);
    }
    println!(
        "\nasserted on both write-heavy workloads: WA leveled > lazy and > tiered; SA leveled < tiered; scan IO leveled < tiered; FIFO never compacts and is lossy"
    );

    // ----- bloom filters: none vs uniform vs Monkey at equal memory, on the zipf write-heavy final trees -----
    println!(
        "\nIO per lookup of an ABSENT key (zipf write-heavy final trees); uniform and Monkey use the same total memory:"
    );
    println!(
        "{:<15} {:>9} {:>12} {:>12} {:>14}",
        "policy", "no bloom", "uniform 5b", "monkey 5b", "filter Kbit"
    );
    let mut cost = Vec::new();
    for (pi, &name) in names.iter().take(3).enumerate() {
        let (rep, _) = &mut reports[0][pi];
        let mut c = [0.0; 3];
        let mut bits = 0;
        for (mi, mode) in [
            FilterMode::None,
            FilterMode::Uniform(5.0),
            FilterMode::Monkey(5.0),
        ]
        .into_iter()
        .enumerate()
        {
            rep.tree.apply_filters(mode);
            if mi == 2 {
                bits = rep.tree.filter_bits();
            }
            c[mi] = measure(&mut rep.tree, &rep.model).miss_io;
        }
        let ub = {
            rep.tree.apply_filters(FilterMode::Uniform(5.0));
            rep.tree.filter_bits()
        };
        println!(
            "{:<15} {:>9.2} {:>12.2} {:>12.2} {:>7.1}/{:<6.1}",
            name,
            c[0],
            c[1],
            c[2],
            bits as f64 / 1000.0,
            ub as f64 / 1000.0
        );
        assert!(
            c[0] > c[1] && c[1] > 0.0,
            "{name}: a bloom filter must cut absent-key IO ({:.2} -> {:.2})",
            c[0],
            c[1]
        );
        assert!(
            (bits as f64) < ub as f64 * 1.05,
            "{name}: Monkey must not use more memory than uniform"
        );
        cost.push(c);
    }
    assert!(
        cost[0][2] < cost[0][1],
        "leveled: Monkey beats uniform at equal memory ({:.3} vs {:.3})",
        cost[0][2],
        cost[0][1]
    );
    assert!(
        cost[2][2] < cost[2][1],
        "lazy: Monkey beats uniform at equal memory ({:.3} vs {:.3})",
        cost[2][2],
        cost[2][1]
    );
    assert!(
        cost[0][1] < cost[1][1],
        "with blooms, leveled does fewer absent-key reads than tiered (more runs)"
    );
    assert!(cost[0][0] < cost[1][0], "and without blooms too");

    // ----- tombstones -----
    println!("\ntombstones: zipf write-heavy run, leveled");
    let (rep, _) = &mut reports[0][0];
    let (dropped, kept) = (rep.tree.st.tombstones_dropped, rep.tree.st.tombstones_kept);
    let before = rep.tree.tombstones();
    println!(
        "  merges dropped {dropped} tombstones and had to keep {kept} (older versions still lived deeper); {before} tombstones in the tree now"
    );
    assert!(dropped > 0 && kept > 0);
    let mut full = rep.tree.clone();
    let live = rep.model.live.len();
    full.apply_filters(FilterMode::Uniform(5.0));
    full.flush();
    full.major_compact();
    let after = full.tombstones();
    println!(
        "  after a major compaction to one run: {after} tombstones, {} entries for {live} live keys",
        full.total_entries()
    );
    assert_eq!(after, 0, "at the bottom of the tree every tombstone can go");
    assert_eq!(
        full.total_entries(),
        live,
        "and space amplification is exactly 1"
    );
    verify(&mut full, &rep.model, true);

    // ----- TTL flavour of FIFO -----
    let mut t = Tree::new(Policy::Fifo {
        cap: usize::MAX,
        ttl: Some(1500),
    });
    let mut rng = Rng(9);
    for i in 0..10_000u64 {
        t.put(2 * rng.below(KEYS) as u64);
        let _ = i;
    }
    println!(
        "\nFIFO with TTL of 1500 writes: {} tables expired whole, {} remain, write amp {:.2}",
        t.dropped_tables,
        t.run_count(),
        t.st.flush_written as f64 / t.st.user_writes as f64
    );
    assert!(t.dropped_tables > 0 && t.runs().all(|s| t.seq - s.max_seq <= 1500 + M as u64));
    println!("\nall LSM compaction checks passed");
}
