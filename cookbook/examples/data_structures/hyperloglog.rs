//! RECIPE 125 (probabilistic): HyperLogLog cardinality estimation from scratch.
//! LEVEL: advanced
//! Use in: COUNT(DISTINCT) at scale, unique visitors, distinct-key stats.   Run: cargo run --release --example hyperloglog
//!
//! WHAT: estimate the number of DISTINCT items in a few KB, with relative error ~ 1.04/sqrt(m), m = 2^p.
//! HOW: hash each item to 64 bits. The top p bits pick a register; in the remaining bits count the
//! leading zeros and store rank = lz + 1 (keep the max per register). Seeing a rank-r hash suggests
//! about 2^r distinct items were tried, but one register is noisy, so we average over m registers
//! with a HARMONIC mean (it damps outliers from lucky registers):
//!     E = alpha_m * m^2 / sum_j 2^(-M[j])
//! alpha_m is a bias constant (0.7213/(1+1.079/m) for m >= 128, small tabulated values below).
//! SMALL RANGE: when many registers are still 0, E is biased high, so switch to linear counting
//! `m * ln(m / zeros)` (treat registers as a Bloom-like bitmap) if E <= 2.5 m.
//! LARGE RANGE: the original paper corrects near 2^32 because of 32-bit hashes; with a good 64-bit
//! hash this is not needed at any practical cardinality.
//! MERGE: register-wise MAX. The merged sketch equals the sketch of the union, exactly, so sketches
//! can be computed per shard / per day and combined. (Intersection is NOT directly supported.)
//! SPARSE VS DENSE: dense = all m registers (here m bytes). Production implementations start SPARSE
//! (a small sorted list of (index, rank) pairs) while few items were seen, then convert to dense once
//! the list would outgrow the dense array; same estimator, much less memory for tiny sets.
//!
//! COMMON USES: approximate distinct counts in analytics databases and caches (unique users per page,
//! distinct IPs per window), per-dimension distinct counters pre-aggregated per hour/day and merged
//! at query time, and cardinality statistics for query planners. Redis (PFADD/PFCOUNT/PFMERGE) and
//! many SQL engines expose HyperLogLog-based approximate distinct counts.
//! Read: https://en.wikipedia.org/wiki/HyperLogLog
//! Read: https://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf
#![allow(dead_code)]

/// splitmix64 finalizer; a good integer -> 64-bit hash. (HLL quality depends on hash quality.)
fn hash64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, PartialEq, Debug)]
struct Hll {
    p: u32,
    regs: Vec<u8>,
}

impl Hll {
    fn new(p: u32) -> Self {
        assert!((4..=18).contains(&p));
        Hll {
            p,
            regs: vec![0; 1 << p],
        }
    }
    fn m(&self) -> usize {
        self.regs.len()
    }
    fn add_hash(&mut self, h: u64) {
        let idx = (h >> (64 - self.p)) as usize;
        // Shift the index bits out; the sentinel bit caps the rank at 64-p+1 when the rest is all zero.
        let rest = (h << self.p) | (1u64 << (self.p - 1));
        let rank = rest.leading_zeros() as u8 + 1;
        if rank > self.regs[idx] {
            self.regs[idx] = rank;
        }
    }
    fn add(&mut self, item: u64) {
        self.add_hash(hash64(item));
    }
    fn alpha(&self) -> f64 {
        match self.m() {
            16 => 0.673,
            32 => 0.697,
            64 => 0.709,
            m => 0.7213 / (1.0 + 1.079 / m as f64),
        }
    }
    /// The raw harmonic-mean estimate with no range corrections.
    fn raw_estimate(&self) -> f64 {
        let m = self.m() as f64;
        let sum: f64 = self.regs.iter().map(|&r| 2f64.powi(-(r as i32))).sum();
        self.alpha() * m * m / sum
    }
    fn estimate(&self) -> f64 {
        let raw = self.raw_estimate();
        let m = self.m() as f64;
        let zeros = self.regs.iter().filter(|&&r| r == 0).count();
        if raw <= 2.5 * m && zeros > 0 {
            m * (m / zeros as f64).ln() // linear counting is far more accurate for small n
        } else {
            raw
        }
    }
    fn merge(&mut self, other: &Hll) {
        assert_eq!(
            self.p, other.p,
            "can only merge sketches with the same precision"
        );
        for (a, &b) in self.regs.iter_mut().zip(&other.regs) {
            *a = (*a).max(b);
        }
    }
    fn std_error(&self) -> f64 {
        1.04 / (self.m() as f64).sqrt()
    }
}

/// Sketch of items `lo..hi` (distinct by construction).
fn sketch_range(p: u32, lo: u64, hi: u64) -> Hll {
    let mut h = Hll::new(p);
    for i in lo..hi {
        h.add(i);
    }
    h
}

fn main() {
    // Accuracy at p = 12 (4096 registers = 4 KiB, std error ~1.6%): assert within 4 standard errors.
    let p = 12;
    println!(
        "p={p}: m={} registers, memory {} B, std error {:.2}%",
        1 << p,
        1 << p,
        Hll::new(p).std_error() * 100.0
    );
    for &n in &[10u64, 1_000, 100_000, 1_000_000] {
        let h = sketch_range(p, 0, n);
        let est = h.estimate();
        let rel = (est - n as f64).abs() / n as f64;
        println!(
            "n={n:>8}  estimate={est:>12.1}  rel.err={:.3}%  raw(no correction)={:.0}",
            rel * 100.0,
            h.raw_estimate()
        );
        assert!(
            rel <= 4.0 * h.std_error(),
            "n={n}: relative error {rel} exceeds 4 sigma"
        );
    }

    // Why the small-range correction exists: the raw estimator is wildly off for n=10.
    let tiny = sketch_range(p, 0, 10);
    assert!(
        tiny.raw_estimate() > 100.0 * 10.0,
        "raw estimate should be badly biased high"
    );
    assert!((tiny.estimate() - 10.0).abs() < 1.0);

    // Precision trades memory for error: each +2 bits of p halves the error and quadruples memory.
    for p in [10, 12, 14] {
        let h = sketch_range(p, 0, 200_000);
        let rel = (h.estimate() - 200_000.0).abs() / 200_000.0;
        println!(
            "p={p:>2}  bytes={:>6}  std err {:.2}%  observed {:.2}%",
            h.m(),
            h.std_error() * 100.0,
            rel * 100.0
        );
        assert!(rel <= 4.0 * h.std_error());
    }

    // Merge == sketch of the union (overlapping ranges: 0..60k and 40k..100k, union = 0..100k).
    let a = sketch_range(p, 0, 60_000);
    let b = sketch_range(p, 40_000, 100_000);
    let union = sketch_range(p, 0, 100_000);
    let mut merged = a.clone();
    merged.merge(&b);
    assert_eq!(
        merged, union,
        "register-wise max must reproduce the union sketch exactly"
    );
    let (ea, eb, em) = (a.estimate(), b.estimate(), merged.estimate());
    println!(
        "|A|~{ea:.0}  |B|~{eb:.0}  |A u B|~{em:.0} (true 100000; naive sum {:.0} double-counts the overlap)",
        ea + eb
    );
    assert!((em - 100_000.0).abs() / 100_000.0 <= 4.0 * merged.std_error());
    assert!(ea + eb > em * 1.15);

    // Duplicates are free: adding the same items again never changes the sketch.
    let mut again = union.clone();
    for i in 0..100_000 {
        again.add(i);
    }
    assert_eq!(again, union);
    println!("idempotent under duplicates: ok");
}
