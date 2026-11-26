//! RECIPE 124 (probabilistic): Count-Min Sketch + heavy hitters (top-k) in sublinear memory.
//! LEVEL: intermediate
//! Use in: frequency estimation over huge streams, trending keys, hot-key detection.   Run: cargo run --release --example count_min_sketch
//!
//! WHAT: a `depth x width` grid of counters. Each row has its own hash function; `add(key)` bumps one
//! cell per row, `estimate(key)` returns the MINIMUM of the key's cells.
//! WHY min: collisions only ever ADD to a cell, so every cell >= true count, and the least-polluted row
//! is the best guess. Hence `estimate >= true` ALWAYS (one-sided error, no underestimates).
//! GUARANTEE: choose width = ceil(e/eps), depth = ceil(ln(1/delta)). Then with probability >= 1-delta,
//! `estimate <= true + eps * N` where N is the total count added. Memory is independent of key count.
//! VARIANTS: conservative update only raises cells that are below (current min + c); still never
//! underestimates, and in practice the error is much smaller (but it cannot support deletions).
//! TOP-K: a sketch cannot enumerate keys, so we pair it with a tiny candidate set + min-heap: on each
//! event, estimate the key and keep it if it beats the smallest candidate (lazy-deletion heap).
//!
//! COMMON USES: per-key rate estimation and hot-key / heavy-hitter detection in network and ad-click
//! streams, "trending" counters, approximate frequency tables inside query engines and stream
//! processors, rate limiting by key without storing every key, and as the building block of
//! sketch-based telemetry. Mergeable by cell-wise addition, so shards can be combined.
//! Read: https://en.wikipedia.org/wiki/Count%E2%80%93min_sketch
#![allow(dead_code)]
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// splitmix64 finalizer: a fast, well-mixed 64-bit hash for integers.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// (width, depth) for the (epsilon, delta) guarantee.
fn dimensions(eps: f64, delta: f64) -> (usize, usize) {
    let width = (std::f64::consts::E / eps).ceil() as usize;
    let depth = (1.0 / delta).ln().ceil() as usize;
    (width, depth)
}

struct CountMin {
    width: usize,
    depth: usize,
    cells: Vec<u32>, // row-major; u32 is plenty and halves memory vs u64
    seeds: Vec<u64>, // one independent-ish hash per row
    total: u64,
}

impl CountMin {
    fn new(eps: f64, delta: f64) -> Self {
        let (width, depth) = dimensions(eps, delta);
        let seeds = (0..depth as u64).map(|r| mix(r + 1)).collect();
        CountMin {
            width,
            depth,
            cells: vec![0; width * depth],
            seeds,
            total: 0,
        }
    }
    fn slot(&self, row: usize, key: u64) -> usize {
        row * self.width + (mix(key ^ self.seeds[row]) % self.width as u64) as usize
    }
    fn add(&mut self, key: u64, c: u32) {
        self.total += c as u64;
        for r in 0..self.depth {
            let i = self.slot(r, key);
            self.cells[i] += c;
        }
    }
    /// Conservative update: only lift cells that are below the new minimum.
    fn add_conservative(&mut self, key: u64, c: u32) {
        self.total += c as u64;
        let target = self.estimate(key) + c;
        for r in 0..self.depth {
            let i = self.slot(r, key);
            self.cells[i] = self.cells[i].max(target);
        }
    }
    fn estimate(&self, key: u64) -> u32 {
        (0..self.depth)
            .map(|r| self.cells[self.slot(r, key)])
            .min()
            .unwrap()
    }
    fn bytes(&self) -> usize {
        self.cells.len() * std::mem::size_of::<u32>()
    }
}

/// Keeps the k keys with the largest estimates. Heap entries can go stale (a key's count grows);
/// we validate against the map when we pop ("lazy deletion") instead of doing decrease-key.
struct TopK {
    k: usize,
    best: HashMap<u64, u32>,
    heap: BinaryHeap<Reverse<(u32, u64)>>, // min-heap on (estimate, key)
}

impl TopK {
    fn new(k: usize) -> Self {
        TopK {
            k,
            best: HashMap::new(),
            heap: BinaryHeap::new(),
        }
    }
    fn observe(&mut self, key: u64, est: u32) {
        if self.best.contains_key(&key) || self.best.len() < self.k {
            self.best.insert(key, est);
            self.heap.push(Reverse((est, key)));
            return;
        }
        // Drop stale heap tops until the smallest entry reflects a live candidate.
        while let Some(&Reverse((c, k))) = self.heap.peek() {
            if self.best.get(&k) == Some(&c) {
                break;
            }
            self.heap.pop();
        }
        let Reverse((min_c, min_k)) = *self.heap.peek().unwrap();
        if est > min_c {
            self.heap.pop();
            self.best.remove(&min_k);
            self.best.insert(key, est);
            self.heap.push(Reverse((est, key)));
        }
    }
    fn ranked(&self) -> Vec<(u64, u32)> {
        let mut v: Vec<_> = self.best.iter().map(|(&k, &c)| (k, c)).collect();
        v.sort_by_key(|&(k, c)| (Reverse(c), k));
        v
    }
}

/// Zipf(s) stream over `universe` keys: key of rank i is drawn with probability ~ 1/i^s.
fn zipf_stream(rng: &mut Rng, universe: usize, s: f64, n: usize) -> Vec<u64> {
    let mut cdf = Vec::with_capacity(universe);
    let mut acc = 0.0;
    for i in 1..=universe {
        acc += 1.0 / (i as f64).powf(s);
        cdf.push(acc);
    }
    (0..n)
        .map(|_| {
            let u = rng.unit() * acc;
            cdf.partition_point(|&c| c < u).min(universe - 1) as u64
        })
        .collect()
}

fn main() {
    let (eps, delta) = (0.001, 0.01);
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let stream = zipf_stream(&mut rng, 50_000, 1.1, 300_000);
    let n = stream.len() as f64;

    let mut exact: HashMap<u64, u32> = HashMap::new();
    let mut plain = CountMin::new(eps, delta);
    let mut cons = CountMin::new(eps, delta);
    let mut top = TopK::new(10);
    for &k in &stream {
        *exact.entry(k).or_default() += 1;
        plain.add(k, 1);
        cons.add_conservative(k, 1);
        top.observe(k, plain.estimate(k));
    }
    println!(
        "eps={eps} delta={delta} -> width={} depth={}, N={}",
        plain.width,
        plain.depth,
        stream.len()
    );

    // 1) Never underestimates; 2) the eps*N bound holds for >= (1-delta) of keys (generous margin).
    let bound = (eps * n) as u32;
    let (mut within, mut err_plain, mut err_cons) = (0usize, 0u64, 0u64);
    for (&k, &t) in &exact {
        let (p, c) = (plain.estimate(k), cons.estimate(k));
        assert!(
            p >= t && c >= t,
            "underestimate for key {k}: true {t}, plain {p}, conservative {c}"
        );
        within += (p <= t + bound) as usize;
        err_plain += (p - t) as u64;
        err_cons += (c - t) as u64;
    }
    let frac = within as f64 / exact.len() as f64;
    println!(
        "distinct keys {}: {:.2}% within eps*N={bound} (need >= {:.0}%)",
        exact.len(),
        frac * 100.0,
        (1.0 - delta) * 100.0 - 1.0
    );
    assert!(frac >= 1.0 - delta - 0.01);
    println!("total overestimate: plain {err_plain}, conservative {err_cons}");
    assert!(
        err_cons <= err_plain,
        "conservative update must not be worse"
    );

    // Heavy hitters: the true top-5 must all appear in the tracker's top-10.
    let mut truth: Vec<_> = exact.iter().map(|(&k, &c)| (k, c)).collect();
    truth.sort_by_key(|&(k, c)| (Reverse(c), k));
    let ranked = top.ranked();
    for (k, c) in &truth[..5] {
        assert!(
            ranked.iter().any(|(rk, _)| rk == k),
            "heavy hitter {k} (count {c}) missed"
        );
    }
    println!("top-3 tracked: {:?}", &ranked[..3]);
    println!("top-3 exact:   {:?}", &truth[..3]);

    // Memory: the sketch is fixed-size; the exact table grows with distinct keys.
    let exact_lower_bound = exact.len() * (8 + 4);
    println!(
        "memory: sketch {} KiB (fixed) vs exact table >= {} KiB for {} keys",
        plain.bytes() / 1024,
        exact_lower_bound / 1024,
        exact.len()
    );
    assert!(plain.bytes() < exact_lower_bound);
}
