//! RECIPE 414 (database): join algorithms compared: nested loop, block NL, index NL, hash, GRACE hash, sort-merge, outer/semi/anti, Bloom.
//! LEVEL: advanced
//! Use in: query executors, ETL engines, data-frame libraries, any "match rows of A with rows of B on a key" code.   Run: cargo run --release --example join_algorithms
#![allow(dead_code)]
//!
//! WHAT: every classic equi-join over relations of (key, payload) tuples, each instrumented with a COST MODEL: `io` counts page
//! transfers (page = `Cfg::page` tuples, buffer = `Cfg::buf_pages` pages) and `cmp` counts key comparisons / hash probes. The
//! algorithms, in the order a planner would consider them:
//! * NESTED LOOP: for every outer tuple scan the whole inner. IO = |R| * pages(S) + pages(R). Simple, no memory, ruinous.
//! * BLOCK NESTED LOOP: load (B-2) pages of outer, scan the inner once per block. IO = pages(R) + ceil(pages(R)/(B-2)) * pages(S).
//! * INDEX NESTED LOOP: probe a sorted index (binary search) or a hash index per outer tuple; pays ~height IO per probe plus
//!   one random IO per match (unclustered). Wins when the outer is tiny and the inner is huge and indexed.
//! * HASH JOIN: build a hash table on the smaller input, stream the other. One pass when the build side fits in memory.
//! * GRACE HASH JOIN: when it does not fit, hash-partition BOTH inputs to (simulated) disk with the same hash, so matching keys land
//!   in the same partition pair, then join pair by pair. A partition still too big is partitioned AGAIN with a different hash seed
//!   (recursive partitioning). SKEW: one hot key can never be split by any hash, so a partition whose smaller side is all one key
//!   (or a recursion depth cap) falls back to block nested loop, which is correct for any skew.
//! * SORT-MERGE: sort both on the key (external-sort IO model; inputs already sorted skip the sort entirely), then merge. Equal
//!   keys form GROUPS on both sides; the cross product of each group pair is emitted (equivalent to the mark/reset trick of
//!   textbook merge joins, done here by slicing the two runs).
//! * SEMI / ANTI / LEFT OUTER / FULL OUTER: hash-based variants. A semi-join emits each outer tuple at most once (`EXISTS`),
//!   anti-join emits those with no match (`NOT EXISTS`), outer joins pad with NULL (u32::MAX).
//! * BLOOM PRE-FILTER: a bit array built from the build side's keys; probe tuples that miss it cannot match, so they are dropped
//!   BEFORE being spilled. It never has false negatives, so results are unchanged; false positives only cost extra work.
//!
//! WHY the table matters: no algorithm dominates. Small inner: hash. Tiny outer + index: index NL. Both inputs already sorted (or
//! an ORDER BY follows): sort-merge. Large unsorted inputs: GRACE hash (2 extra passes over the data, no sort). Skew: GRACE with a
//! fallback, or sort-merge, which is immune to hot keys except for the unavoidable output size.
//!
//! CHECKS: 500 random relation pairs (empty, tiny, skewed, duplicated, pre-sorted, single-key) must give identical sorted result
//! multisets for all algorithms against a naive oracle; measured IO of NL and BNL must equal their closed-form formulas; semi/anti
//! partition R exactly; outer joins equal inner join plus padded unmatched rows.
//!
//! COMMON USES: PostgreSQL (nested loop, hash join with batches, merge join), MySQL 8 hash join, Spark SQL (broadcast hash,
//! shuffle hash, sort-merge), DuckDB and ClickHouse hash joins with Bloom/min-max filters pushed to scans, Grace/hybrid hash in
//! classic System R-descended optimizers; also dataframe `merge`/`join` in pandas and polars.
//! Read: Hash join https://en.wikipedia.org/wiki/Hash_join   Sort-merge join https://en.wikipedia.org/wiki/Sort-merge_join
//!       CMU 15-445 Database Systems (join lectures) https://15445.courses.cs.cmu.edu/fall2024/

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

type T = (u32, u32); // (join key, payload)
type Out = (u32, u32, u32); // (key, outer payload, inner payload)
const NULL: u32 = u32::MAX;

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

#[derive(Clone, Copy)]
struct Cfg {
    page: usize,      // tuples per page
    buf_pages: usize, // memory in pages (>= 3)
}
impl Cfg {
    fn mem(&self) -> usize {
        (self.buf_pages - 2) * self.page // tuples held in memory (one page in, one page out reserved)
    }
    fn pages(&self, n: usize) -> u64 {
        n.div_ceil(self.page) as u64
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct Cost {
    io: u64,
    cmp: u64,
}
impl Cost {
    /// A random page IO is worth about 100 CPU comparisons in this toy weighting.
    fn total(&self) -> u64 {
        self.io * 100 + self.cmp
    }
}

fn mix(k: u32, seed: u64) -> u64 {
    let mut x = k as u64 ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

// ---------- Bloom filter ----------
struct Bloom {
    bits: Vec<u64>,
    m: usize,
    k: u64,
}
impl Bloom {
    fn new(n_keys: usize, bits_per_key: usize) -> Bloom {
        let m = (n_keys * bits_per_key).max(64);
        // Optimal k = ln2 * bits/key ~ 0.69 * bpk.
        Bloom {
            bits: vec![0; m.div_ceil(64)],
            m,
            k: ((bits_per_key as f64 * 0.69) as u64).max(1),
        }
    }
    fn insert(&mut self, key: u32) {
        for i in 0..self.k {
            let h = (mix(key, 7000 + i) % self.m as u64) as usize;
            self.bits[h / 64] |= 1 << (h % 64);
        }
    }
    fn maybe_contains(&self, key: u32) -> bool {
        (0..self.k).all(|i| {
            let h = (mix(key, 7000 + i) % self.m as u64) as usize;
            self.bits[h / 64] >> (h % 64) & 1 == 1
        })
    }
}

// ---------- nested loop family ----------
fn nested_loop(r: &[T], s: &[T], cfg: Cfg, c: &mut Cost) -> Vec<Out> {
    // Tuple-at-a-time: the inner relation is re-read from scratch for every outer tuple.
    c.io += cfg.pages(r.len()) + r.len() as u64 * cfg.pages(s.len());
    let mut out = Vec::new();
    for &(k, rp) in r {
        for &(k2, sp) in s {
            c.cmp += 1;
            if k == k2 {
                out.push((k, rp, sp));
            }
        }
    }
    out
}
fn model_nested_loop(r: usize, s: usize, cfg: Cfg) -> Cost {
    Cost {
        io: cfg.pages(r) + r as u64 * cfg.pages(s),
        cmp: (r * s) as u64,
    }
}

/// Block NL with an explicit orientation: `outer` is chunked by memory, `inner` is scanned once per chunk.
fn bnl_core(outer: &[T], inner: &[T], swapped: bool, cfg: Cfg, c: &mut Cost, out: &mut Vec<Out>) {
    for chunk in outer.chunks(cfg.mem()) {
        c.io += cfg.pages(chunk.len()) + cfg.pages(inner.len());
        for &(k, op) in chunk {
            for &(k2, ip) in inner {
                c.cmp += 1;
                if k == k2 {
                    out.push(if swapped { (k, ip, op) } else { (k, op, ip) });
                }
            }
        }
    }
}
fn block_nested_loop(r: &[T], s: &[T], cfg: Cfg, c: &mut Cost) -> Vec<Out> {
    let mut out = Vec::new();
    bnl_core(r, s, false, cfg, c, &mut out);
    out
}
fn model_block_nl(r: usize, s: usize, cfg: Cfg) -> Cost {
    Cost {
        io: cfg.pages(r) + r.div_ceil(cfg.mem()) as u64 * cfg.pages(s),
        cmp: (r * s) as u64,
    }
}

// ---------- index nested loop ----------
fn index_height(n: usize) -> u64 {
    // Fanout-100 tree: height = ceil(log100(n)), at least the leaf level.
    let (mut h, mut cap) = (1, 100usize);
    while cap < n {
        cap *= 100;
        h += 1;
    }
    h
}
/// Probes a sorted index (built beforehand, so build cost is not charged: the point of an index is that it already exists).
fn index_nl_sorted(r: &[T], s: &[T], cfg: Cfg, c: &mut Cost) -> Vec<Out> {
    let mut idx = s.to_vec();
    idx.sort_unstable();
    let mut out = Vec::new();
    c.io += cfg.pages(r.len());
    for &(k, rp) in r {
        c.io += index_height(s.len());
        let mut probes = 0u64;
        let lo = idx.partition_point(|t| {
            probes += 1;
            t.0 < k
        });
        c.cmp += probes;
        for &(k2, sp) in idx[lo..].iter().take_while(|t| t.0 == k) {
            c.io += 1; // fetching an unclustered tuple costs a random page read
            out.push((k2, rp, sp));
        }
    }
    out
}
fn index_nl_hash(r: &[T], s: &[T], cfg: Cfg, c: &mut Cost) -> Vec<Out> {
    let mut idx: HashMap<u32, Vec<u32>> = HashMap::new();
    s.iter()
        .for_each(|&(k, p)| idx.entry(k).or_default().push(p));
    let mut out = Vec::new();
    c.io += cfg.pages(r.len());
    for &(k, rp) in r {
        c.io += 1; // one bucket page per probe
        c.cmp += 1;
        for &sp in idx.get(&k).into_iter().flatten() {
            c.io += 1;
            out.push((k, rp, sp));
        }
    }
    out
}

// ---------- hash join and GRACE ----------
/// Core in-memory hash join: build on `build`, stream `probe`. `swapped` says build is the R side (for output orientation).
fn hash_core(build: &[T], probe: &[T], swapped: bool, c: &mut Cost, out: &mut Vec<Out>) {
    let mut ht: HashMap<u32, Vec<u32>> = HashMap::with_capacity(build.len());
    build
        .iter()
        .for_each(|&(k, p)| ht.entry(k).or_default().push(p));
    c.cmp += build.len() as u64;
    for &(k, p) in probe {
        c.cmp += 1;
        for &q in ht.get(&k).into_iter().flatten() {
            out.push(if swapped { (k, q, p) } else { (k, p, q) });
        }
    }
}
/// Builds on whichever side is smaller (the optimizer's most basic hash-join decision).
fn hash_pair(r: &[T], s: &[T], c: &mut Cost, out: &mut Vec<Out>) {
    if r.len() < s.len() {
        hash_core(r, s, true, c, out)
    } else {
        hash_core(s, r, false, c, out)
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct GraceStats {
    spilled_pages: u64,
    max_level: usize,
    fallbacks: usize,
    bloom_dropped: usize,
}
const MAX_LEVEL: usize = 6;

fn grace_rec(
    r: &[T],
    s: &[T],
    cfg: Cfg,
    level: usize,
    c: &mut Cost,
    out: &mut Vec<Out>,
    st: &mut GraceStats,
) {
    if r.is_empty() || s.is_empty() {
        return; // nothing can match; a real system never even reads the partner partition
    }
    let (small, small_is_r) = if r.len() < s.len() {
        (r, true)
    } else {
        (s, false)
    };
    if small.len() <= cfg.mem() {
        c.io += cfg.pages(r.len()) + cfg.pages(s.len());
        return hash_pair(r, s, c, out);
    }
    // A hash cannot split one repeated key: only the hot-key case (or runaway depth) needs the fallback.
    if level >= MAX_LEVEL || small.iter().all(|t| t.0 == small[0].0) {
        st.fallbacks += 1;
        return if small_is_r {
            bnl_core(r, s, false, cfg, c, out)
        } else {
            bnl_core(s, r, true, cfg, c, out)
        };
    }
    // Partition both inputs with the SAME hash (a fresh seed per level), writing each partition to "disk".
    let fanout = cfg.buf_pages - 1;
    let part = |x: &[T]| {
        let mut parts = vec![Vec::new(); fanout];
        x.iter()
            .for_each(|&t| parts[(mix(t.0, level as u64 + 1) % fanout as u64) as usize].push(t));
        parts
    };
    let (pr, ps) = (part(r), part(s));
    c.io += cfg.pages(r.len()) + cfg.pages(s.len()); // read the input once
    let written: u64 = pr.iter().chain(&ps).map(|p| cfg.pages(p.len())).sum();
    c.io += written; // write every partition
    st.spilled_pages += written;
    st.max_level = st.max_level.max(level + 1);
    for (a, b) in pr.iter().zip(&ps) {
        grace_rec(a, b, cfg, level + 1, c, out, st); // each recursive call re-reads its pair from disk
    }
}

/// GRACE hash join; with `bloom`, R tuples that cannot match any S key are dropped before spilling.
fn grace_join(
    r: &[T],
    s: &[T],
    cfg: Cfg,
    bloom: bool,
    c: &mut Cost,
    st: &mut GraceStats,
) -> Vec<Out> {
    let mut out = Vec::new();
    if bloom {
        let mut bf = Bloom::new(s.len(), 10);
        s.iter().for_each(|t| bf.insert(t.0));
        c.cmp += (s.len() + r.len()) as u64;
        let kept: Vec<T> = r
            .iter()
            .copied()
            .filter(|t| bf.maybe_contains(t.0))
            .collect();
        st.bloom_dropped = r.len() - kept.len();
        c.io += cfg.pages(r.len()) - cfg.pages(kept.len()); // dropped tuples were still read once (pipelined with the scan)
        grace_rec(&kept, s, cfg, 0, c, &mut out, st);
    } else {
        grace_rec(r, s, cfg, 0, c, &mut out, st);
    }
    out
}

// ---------- sort-merge ----------
fn log_ceil(x: u64, base: u64) -> u64 {
    let (mut n, mut cap) = (0, 1);
    while cap < x {
        cap *= base;
        n += 1;
    }
    n
}
/// Returns the sorted copy and charges the external sort. IO per side = initial read + extra spill/merge passes.
fn sort_side(x: &[T], cfg: Cfg, c: &mut Cost) -> Vec<T> {
    let p = cfg.pages(x.len());
    c.io += p; // the one read every algorithm pays
    if x.is_sorted_by_key(|t| t.0) {
        c.cmp += x.len() as u64;
        return x.to_vec();
    }
    let mut v = x.to_vec();
    v.sort_by(|a, b| {
        c.cmp += 1;
        a.0.cmp(&b.0)
    });
    if p > cfg.buf_pages as u64 {
        let runs = p.div_ceil(cfg.buf_pages as u64);
        let merge_passes = log_ceil(runs, cfg.buf_pages as u64 - 1).max(1);
        // Write initial runs, intermediate passes read+write, the final merge feeds the join directly (read only).
        c.io += p + 2 * p * (merge_passes - 1) + p;
    }
    v
}
fn run_len(x: &[T], i: usize) -> usize {
    x[i..].iter().take_while(|t| t.0 == x[i].0).count()
}
fn sort_merge(r: &[T], s: &[T], cfg: Cfg, c: &mut Cost) -> Vec<Out> {
    let (r, s) = (sort_side(r, cfg, c), sort_side(s, cfg, c));
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < r.len() && j < s.len() {
        c.cmp += 1;
        match r[i].0.cmp(&s[j].0) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                // Group buffering: both runs of this key are contiguous, so emit their cross product and skip past both.
                let (ie, je) = (i + run_len(&r, i), j + run_len(&s, j));
                for &(k, rp) in &r[i..ie] {
                    out.extend(s[j..je].iter().map(|&(_, sp)| (k, rp, sp)));
                }
                (i, j) = (ie, je);
            }
        }
    }
    out
}

// ---------- semi / anti / outer ----------
fn semi_join(r: &[T], s: &[T], anti: bool) -> Vec<T> {
    let keys: HashSet<u32> = s.iter().map(|t| t.0).collect();
    r.iter()
        .copied()
        .filter(|t| keys.contains(&t.0) != anti)
        .collect()
}
fn outer_join(r: &[T], s: &[T], full: bool) -> Vec<Out> {
    let mut ht: HashMap<u32, Vec<(u32, usize)>> = HashMap::new();
    s.iter()
        .enumerate()
        .for_each(|(i, &(k, p))| ht.entry(k).or_default().push((p, i)));
    let mut s_matched = vec![false; s.len()];
    let mut out = Vec::new();
    for &(k, rp) in r {
        match ht.get(&k) {
            Some(m) => m.iter().for_each(|&(sp, i)| {
                s_matched[i] = true;
                out.push((k, rp, sp));
            }),
            None => out.push((k, rp, NULL)), // preserved outer row padded with NULL
        }
    }
    if full {
        out.extend(
            s.iter()
                .zip(&s_matched)
                .filter(|(_, m)| !**m)
                .map(|(&(k, sp), _)| (k, NULL, sp)),
        );
    }
    out
}

// ---------- data generation and checks ----------
fn gen_rel(rng: &mut Rng, n: usize, domain: u64, mode: u64) -> Vec<T> {
    let hot = rng.below(domain) as u32;
    let mut v: Vec<T> = (0..n)
        .map(|_| {
            let k = match mode {
                1 if rng.below(100) < 35 => hot, // 35% of tuples share one key
                3 => hot,                        // single-key relation
                _ => rng.below(domain) as u32,
            };
            (k, rng.below(12) as u32) // small payload domain: identical duplicate tuples occur
        })
        .collect();
    if mode == 2 {
        v.sort_unstable();
    }
    v
}
fn sorted<X: Ord>(mut v: Vec<X>) -> Vec<X> {
    v.sort_unstable();
    v
}

fn differential_test(rng: &mut Rng) {
    let cfg = Cfg {
        page: 8,
        buf_pages: 4,
    }; // 16 tuples of memory: forces spilling and recursion
    let (mut agg, mut results, mut st_sum) = (0usize, 0usize, GraceStats::default());
    for round in 0..500 {
        let domain = [1, 3, 10, 50, 400][rng.below(5) as usize];
        let (nr, ns) = (rng.below(260) as usize, rng.below(260) as usize);
        let (ra, sb) = (rng.below(4), rng.below(4));
        let (r, s) = (gen_rel(rng, nr, domain, ra), gen_rel(rng, ns, domain, sb));
        // Keep the quadratic oracle's output bounded when both sides are single-key.
        let (r, s) = if ra == 3 && sb == 3 {
            (r[..r.len().min(60)].to_vec(), s[..s.len().min(60)].to_vec())
        } else {
            (r, s)
        };
        let mut c = Cost::default();
        let oracle = sorted(nested_loop(&r, &s, cfg, &mut c));
        assert_eq!(
            c,
            model_nested_loop(r.len(), s.len(), cfg),
            "NL cost must match its formula"
        );
        results += oracle.len();

        let mut c = Cost::default();
        assert_eq!(
            sorted(block_nested_loop(&r, &s, cfg, &mut c)),
            oracle,
            "bnl, round {round}"
        );
        assert_eq!(
            c,
            model_block_nl(r.len(), s.len(), cfg),
            "BNL cost must match its formula"
        );
        let mut c = Cost::default();
        assert_eq!(
            sorted(index_nl_sorted(&r, &s, cfg, &mut c)),
            oracle,
            "index nl (sorted)"
        );
        assert_eq!(
            sorted(index_nl_hash(&r, &s, cfg, &mut c)),
            oracle,
            "index nl (hash)"
        );
        let mut out = Vec::new();
        hash_pair(&r, &s, &mut c, &mut out);
        assert_eq!(sorted(out), oracle, "in-memory hash");
        for bloom in [false, true] {
            let mut st = GraceStats::default();
            assert_eq!(
                sorted(grace_join(&r, &s, cfg, bloom, &mut c, &mut st)),
                oracle,
                "grace bloom={bloom}, round {round}"
            );
            st_sum.spilled_pages += st.spilled_pages;
            st_sum.max_level = st_sum.max_level.max(st.max_level);
            st_sum.fallbacks += st.fallbacks;
            st_sum.bloom_dropped += st.bloom_dropped;
        }
        assert_eq!(
            sorted(sort_merge(&r, &s, cfg, &mut c)),
            oracle,
            "sort-merge, round {round}"
        );

        // Semi / anti / outer joins against naive definitions.
        let has = |t: &T| s.iter().any(|u| u.0 == t.0);
        let (semi, anti) = (semi_join(&r, &s, false), semi_join(&r, &s, true));
        assert_eq!(
            sorted(semi.clone()),
            sorted(r.iter().copied().filter(has).collect())
        );
        assert_eq!(
            sorted(anti.clone()),
            sorted(r.iter().copied().filter(|t| !has(t)).collect())
        );
        assert_eq!(
            sorted([semi, anti].concat()),
            sorted(r.clone()),
            "semi + anti partition R"
        );
        let left_expect = [
            oracle.clone(),
            r.iter()
                .filter(|t| !has(t))
                .map(|&(k, p)| (k, p, NULL))
                .collect(),
        ]
        .concat();
        assert_eq!(
            sorted(outer_join(&r, &s, false)),
            sorted(left_expect.clone()),
            "left outer"
        );
        let right_only = s
            .iter()
            .filter(|u| !r.iter().any(|t| t.0 == u.0))
            .map(|&(k, p)| (k, NULL, p));
        assert_eq!(
            sorted(outer_join(&r, &s, true)),
            sorted([left_expect, right_only.collect()].concat()),
            "full outer"
        );
        agg += 1;
    }
    println!(
        "differential: {agg} random pairs, all {} algorithms agree with the oracle ({results} result rows total)",
        8
    );
    println!(
        "  GRACE across tests: {} pages spilled, max recursion depth {}, hot-key fallbacks {}, Bloom dropped {} tuples",
        st_sum.spilled_pages, st_sum.max_level, st_sum.fallbacks, st_sum.bloom_dropped
    );
    assert!(
        st_sum.max_level >= 2 && st_sum.fallbacks > 0,
        "tests must exercise recursion and the skew fallback"
    );
    // Bloom filter: no false negatives by construction (checked via join equality); measure the false-positive rate.
    let mut bf = Bloom::new(10_000, 10);
    (0..10_000u32).for_each(|k| bf.insert(k * 2));
    assert!((0..10_000u32).all(|k| bf.maybe_contains(k * 2)));
    let fp = (0..100_000u32)
        .filter(|&k| bf.maybe_contains(k * 2 + 1 + 1_000_000))
        .count();
    println!(
        "  Bloom (10 bits/key, k=6): false positive rate {:.2}%",
        fp as f64 / 1000.0
    );
    assert!(fp < 3000, "FP rate should be about 1%");
}

// ---------- the cost table ----------
fn run_all(r: &[T], s: &[T], cfg: Cfg, pairs: u64) -> Vec<(&'static str, Cost, usize, bool)> {
    let mut rows = Vec::new();
    // NL and BNL are quadratic: run them for real only when affordable, otherwise use the verified closed-form model.
    let real_quadratic = pairs <= 20_000_000;
    let expect = {
        let mut c = Cost::default();
        let mut o = Vec::new();
        hash_pair(r, s, &mut c, &mut o);
        o.len()
    };
    let mut push = |name: &'static str, c: Cost, n: usize, model: bool| {
        assert!(model || n == expect, "{name} result size {n} != {expect}");
        rows.push((name, c, n, model));
    };
    if real_quadratic {
        let mut c = Cost::default();
        let n = nested_loop(r, s, cfg, &mut c).len();
        push("nested loop", c, n, false);
        let mut c = Cost::default();
        let n = block_nested_loop(r, s, cfg, &mut c).len();
        push("block nested loop", c, n, false);
    } else {
        push(
            "nested loop",
            model_nested_loop(r.len(), s.len(), cfg),
            expect,
            true,
        );
        push(
            "block nested loop",
            model_block_nl(r.len(), s.len(), cfg),
            expect,
            true,
        );
    }
    let mut c = Cost::default();
    let n = index_nl_sorted(r, s, cfg, &mut c).len();
    push("index NL (sorted idx)", c, n, false);
    let mut c = Cost::default();
    let n = index_nl_hash(r, s, cfg, &mut c).len();
    push("index NL (hash idx)", c, n, false);
    let mut c = Cost::default();
    let mut st = GraceStats::default();
    let n = grace_join(r, s, cfg, false, &mut c, &mut st).len();
    push("hash / GRACE", c, n, false);
    let mut c = Cost::default();
    let n = grace_join(r, s, cfg, true, &mut c, &mut st).len();
    push("hash / GRACE + Bloom", c, n, false);
    let mut c = Cost::default();
    let n = sort_merge(r, s, cfg, &mut c).len();
    push("sort-merge", c, n, false);
    rows
}

fn print_scenario(
    title: &str,
    r: &[T],
    s: &[T],
    cfg: Cfg,
) -> Vec<(&'static str, Cost, usize, bool)> {
    let rows = run_all(r, s, cfg, r.len() as u64 * s.len() as u64);
    let best = rows.iter().map(|x| x.1.total()).min().unwrap();
    println!(
        "\n{title}: |R|={} ({} pages), |S|={} ({} pages), memory {} tuples, {} result rows",
        r.len(),
        cfg.pages(r.len()),
        s.len(),
        cfg.pages(s.len()),
        cfg.mem(),
        rows[0].2
    );
    println!(
        "  {:<24}{:>12}{:>14}{:>16}",
        "algorithm", "IO pages", "cmp/probes", "weighted cost"
    );
    for (name, c, _, model) in &rows {
        let mark = if c.total() == best { " <== wins" } else { "" };
        println!(
            "  {:<24}{:>12}{:>14}{:>16}{}{}",
            name,
            c.io,
            c.cmp,
            c.total(),
            if *model { " (model)" } else { "" },
            mark
        );
    }
    rows
}
fn winner(rows: &[(&'static str, Cost, usize, bool)]) -> &'static str {
    rows.iter().min_by_key(|x| x.1.total()).unwrap().0
}
fn cost_of(rows: &[(&'static str, Cost, usize, bool)], name: &str) -> Cost {
    rows.iter().find(|x| x.0 == name).unwrap().1
}

fn scenarios(rng: &mut Rng) {
    let cfg = Cfg {
        page: 100,
        buf_pages: 40,
    }; // 3800 tuples: one partitioning level suffices for 600-page inputs
    // A: small inner fits in memory.
    let r = gen_rel(rng, 60_000, 100_000, 0);
    let s: Vec<T> = (0..800u32).map(|i| (i * 97 % 100_000, i)).collect();
    let a = print_scenario("A. small inner", &r, &s, cfg);
    assert!(
        winner(&a).starts_with("hash"),
        "small inner: hash join wins"
    );
    assert!(
        cost_of(&a, "hash / GRACE").io == cfg.pages(r.len()) + cfg.pages(s.len()),
        "single pass, no spill"
    );

    // B: tiny outer probing a big indexed inner.
    let r = gen_rel(rng, 40, 100_000, 0);
    let s = gen_rel(rng, 100_000, 100_000, 0);
    let b = print_scenario("B. tiny outer, big indexed inner", &r, &s, cfg);
    assert!(
        winner(&b).starts_with("index NL"),
        "tiny outer: index nested loop wins"
    );

    // C: both inputs sorted already.
    let (r, s) = (
        gen_rel(rng, 60_000, 30_000, 2),
        gen_rel(rng, 60_000, 30_000, 2),
    );
    let c = print_scenario("C. both inputs sorted", &r, &s, cfg);
    assert_eq!(
        winner(&c),
        "sort-merge",
        "sorted inputs: merge needs no sort and no spill"
    );
    assert_eq!(
        cost_of(&c, "sort-merge").io,
        cfg.pages(r.len()) + cfg.pages(s.len())
    );

    // D: large unsorted uniform.
    let (r, s) = (
        gen_rel(rng, 60_000, 60_000, 0),
        gen_rel(rng, 60_000, 60_000, 0),
    );
    let d = print_scenario("D. large uniform inputs", &r, &s, cfg);
    assert!(
        winner(&d).starts_with("hash"),
        "large unsorted: GRACE hash beats sorting both"
    );
    assert!(
        cost_of(&d, "hash / GRACE").io * 100 <= cost_of(&d, "sort-merge").io * 105,
        "equal IO class (3 passes) within page rounding"
    );

    // E: skewed build side: a hot key of 1200 tuples exceeds the 1000-tuple memory and cannot be partitioned away.
    let cfg = Cfg {
        page: 100,
        buf_pages: 12,
    }; // small memory (1000 tuples) so the hot key cannot fit
    let mut r = gen_rel(rng, 58_500, 20_000, 0);
    r.extend((0..1500).map(|i| (7, i))); // hot on BOTH sides: building on the smaller side would otherwise dodge the skew
    let mut s = gen_rel(rng, 38_800, 20_000, 0);
    s.extend((0..1200).map(|i| (7, i)));
    let mut st = GraceStats::default();
    let mut c = Cost::default();
    let n = grace_join(&r, &s, cfg, false, &mut c, &mut st).len();
    let e = print_scenario(
        "E. large skewed (hot key on both sides, > memory)",
        &r,
        &s,
        cfg,
    );
    println!(
        "  GRACE on E: depth {}, spilled {} pages, hot-key fallbacks {}, result {n}",
        st.max_level, st.spilled_pages, st.fallbacks
    );
    assert!(st.fallbacks >= 1 && winner(&e) != "nested loop");

    // F: Bloom filter benefit when few probe tuples match.
    let s = gen_rel(rng, 20_000, 20_000, 0);
    let r = gen_rel(rng, 80_000, 400_000, 0); // ~95% of R keys cannot match
    let (mut c0, mut c1) = (Cost::default(), Cost::default());
    let (mut s0, mut s1) = (GraceStats::default(), GraceStats::default());
    let n0 = grace_join(&r, &s, cfg, false, &mut c0, &mut s0).len();
    let n1 = grace_join(&r, &s, cfg, true, &mut c1, &mut s1).len();
    assert_eq!(n0, n1);
    println!(
        "\nF. Bloom pre-filter (R 80k tuples, ~95% non-matching): IO {} -> {} pages, R tuples dropped before spilling: {} ({:.0}%)",
        c0.io,
        c1.io,
        s1.bloom_dropped,
        100.0 * s1.bloom_dropped as f64 / r.len() as f64
    );
    assert!(c1.io < c0.io, "Bloom filter must cut spill IO");
}

fn main() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    // Tiny worked example first, so the semantics of each variant are visible.
    let r: Vec<T> = vec![(1, 10), (2, 20), (2, 21), (4, 40)];
    let s: Vec<T> = vec![(2, 200), (2, 201), (3, 300), (4, 400)];
    let cfg = Cfg {
        page: 2,
        buf_pages: 4,
    };
    let mut c = Cost::default();
    println!("inner:      {:?}", sorted(sort_merge(&r, &s, cfg, &mut c)));
    println!("left outer: {:?}", sorted(outer_join(&r, &s, false)));
    println!(
        "semi: {:?}  anti: {:?}",
        semi_join(&r, &s, false),
        semi_join(&r, &s, true)
    );
    assert_eq!(
        sort_merge(&r, &s, cfg, &mut c).len(),
        2 * 2 + 1,
        "duplicates on both sides: 2x2 for key 2, plus key 4"
    );

    differential_test(&mut rng);
    scenarios(&mut rng);
    println!("ok");
}
