//! RECIPE 760 (database): optimizer statistics: histograms (equi-width, equi-depth, V-optimal, MaxDiff), MCV lists, NDV sketches, sampling, multi-column stats.
//! LEVEL: advanced
//! Use in: cost-based optimizers, ANALYZE / UPDATE STATISTICS implementations, approximate query engines, data catalogs.   Run: cargo run --release --example statistics_histograms_sketches
//! COMMON USES: PostgreSQL `ANALYZE` (most-common-values list plus an equi-depth histogram of the rest, `n_distinct`, extended
//! statistics for column groups and functional dependencies), SQL Server and Oracle histograms (equi-height, hybrid, frequency),
//! Spark/Trino column statistics (NDV via HyperLogLog, min/max, null counts), BigQuery and Redis `PFCOUNT` style HLL merging, Count-Min
//! sketches for heavy hitters in stream processors, auto-analyze triggers based on modification counters.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lecture 12 (Cost Models: Statistics)
#![allow(dead_code)]
//!
//! WHAT: the statistics an optimizer keeps per column and how each one turns into a SELECTIVITY (fraction of rows) for `=`, `<` and
//! range predicates. Everything is checked against ground truth obtained by executing the predicate on the generated column.
//! * EQUI-WIDTH: buckets cover equal slices of the value domain. Cheap, but a hot value drags a whole wide bucket's mass onto its
//!   neighbours (uniformity-within-bucket assumption: every value of a bucket gets count / ndv).
//! * EQUI-DEPTH (equi-height): buckets hold ~equal row counts, so dense regions get narrow buckets and skew is isolated.
//! * V-OPTIMAL: the partition into B buckets that MINIMISES the within-bucket sum of squared deviations of the value frequencies
//!   (dynamic programming, O(B * D^2) with prefix sums). Provably the best B-bucket histogram for that error metric.
//! * MAXDIFF: put the B-1 bucket boundaries between the neighbouring values whose frequencies differ the most. Linear time, close
//!   to V-optimal in practice.
//! * MCV + HISTOGRAM (PostgreSQL style): remember the k most common values EXACTLY, and keep an equi-depth histogram of the rest.
//!   The head of a Zipf distribution is where the damage of bucket averaging is largest, and MCVs remove it.
//! * NDV: sampling estimators (naive, GEE = sqrt(n/r) * f1 + sum_{j>=2} f_j, Chao1 = d + f1^2 / (2 f2)) against HyperLogLog, which
//!   sees every row once, and whose sketches MERGE by register-wise max, so per-partition sketches combine into the global one.
//! * COUNT-MIN + top-k candidates for skew, Hoeffding confidence bounds for sampled selectivities (sample size vs error curve).
//! * MULTI-COLUMN: independence multiplies marginals and fails on correlated columns; a 2-D grid histogram, a column-group NDV
//!   (HLL on the pair) and a functional-dependency strength ndv(a)/ndv(a,b) repair it.
//! * MAINTENANCE: incremental bucket counters on insert/delete, an auto-analyze staleness rule (mods > 50 + 10% of rows) and a
//!   skew check (largest bucket / mean bucket) deciding between "keep patching" and "rebuild".
//!
//! WHY: every plan choice multiplies estimated selectivities; a factor-100 error on one predicate flips join orders and access
//! paths. Statistics are a space/accuracy/maintenance trade-off, so each structure here is measured, not just described.
//!
//! CHECKS (asserted): equi-depth beats equi-width on Zipf equality estimates; V-optimal SSE <= equi-depth, MaxDiff and equi-width SSE
//! for the same bucket count; MCV makes the Zipf head exact; HLL error < 6% and its partition merge equals the one-pass sketch;
//! Count-Min never underestimates and finds the true heavy hitters; Hoeffding bound coverage >= 95% and error shrinks ~1/sqrt(m);
//! 2-D histogram beats independence on correlated columns; FD strength is exactly 1.0 for a true FD; maintained histograms beat stale ones.
//! Read: Cost Models: Statistics, CMU 15-799 lecture 12 https://15799.courses.cs.cmu.edu/spring2025/schedule.html
//!       Every Row Counts: Combining Sketches and Sampling for Accurate Group-By Result Estimates (Freitag et al., CIDR 2019)
//!       Effective Use of Block-Level Sampling in Statistics Estimation (Chaudhuri et al., SIGMOD 2004)
//!       PostgreSQL planner statistics https://www.postgresql.org/docs/current/planner-stats.html
//!       HyperLogLog https://en.wikipedia.org/wiki/HyperLogLog   Count-min sketch https://en.wikipedia.org/wiki/Count%E2%80%93min_sketch

use std::collections::HashMap;

/// Value domain of the integer columns used in the histogram experiments: values are 0..D.
const D: usize = 1000;
const ROWS: usize = 100_000;
const BUCKETS: usize = 16;

// ---------------------------------------------------------------------------------------------------------------------
// Deterministic randomness (splitmix64) and a Zipf sampler
// ---------------------------------------------------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// splitmix64 finaliser: also used as the hash function of the sketches (good avalanche, no deps).
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Zipf over ranks 0..n via inverse CDF and binary search.
struct Zipf(Vec<f64>);
impl Zipf {
    fn new(n: usize, s: f64) -> Self {
        let mut acc = 0.0;
        let mut cdf: Vec<f64> = (1..=n)
            .map(|k| {
                acc += 1.0 / (k as f64).powf(s);
                acc
            })
            .collect();
        for c in cdf.iter_mut() {
            *c /= acc;
        }
        Zipf(cdf)
    }
    fn sample(&self, r: &mut Rng) -> usize {
        let u = r.unit();
        self.0.partition_point(|&c| c < u).min(self.0.len() - 1)
    }
}

/// q-error: symmetric multiplicative error, with both sides clamped to one row (an estimate of 0 rows is never "exact").
fn qerr(est: f64, actual: f64) -> f64 {
    let (e, a) = (est.max(1.0), actual.max(1.0));
    (e / a).max(a / e)
}

fn percentile(v: &mut Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

// ---------------------------------------------------------------------------------------------------------------------
// Column generators
// ---------------------------------------------------------------------------------------------------------------------

fn gen_uniform(r: &mut Rng, n: usize) -> Vec<u32> {
    (0..n).map(|_| r.below(D as u64) as u32).collect()
}

/// Zipf(1.1) with the hottest value at 0 (like a status code or a low surrogate id): the head sits inside one equi-width bucket.
fn gen_zipf(r: &mut Rng, n: usize) -> Vec<u32> {
    let z = Zipf::new(D, 1.1);
    (0..n).map(|_| z.sample(r) as u32).collect()
}

/// Five tight clusters separated by empty gaps (timestamps of batch jobs, geographic hot spots).
fn gen_clustered(r: &mut Rng, n: usize) -> Vec<u32> {
    let centres = [60i64, 230, 410, 640, 880];
    (0..n)
        .map(|_| {
            let c = centres[r.below(5) as usize];
            let jitter = (r.below(15) + r.below(15) + r.below(15)) as i64 - 21;
            (c + jitter).clamp(0, D as i64 - 1) as u32
        })
        .collect()
}

fn freq_of(col: &[u32]) -> Vec<u64> {
    let mut f = vec![0u64; D];
    for &v in col {
        f[v as usize] += 1;
    }
    f
}

fn prefix(f: &[u64]) -> Vec<u64> {
    let mut p = vec![0u64; f.len() + 1];
    for i in 0..f.len() {
        p[i + 1] = p[i] + f[i];
    }
    p
}

// ---------------------------------------------------------------------------------------------------------------------
// Histograms over the dense domain 0..D. A histogram is a partition of the domain into contiguous buckets.
// ---------------------------------------------------------------------------------------------------------------------

/// Anything that can answer selectivity questions (returned as an estimated ROW COUNT).
trait Estimator {
    fn eq(&self, v: usize) -> f64;
    fn range(&self, lo: usize, hi: usize) -> f64; // inclusive both ends
}

#[derive(Clone, Debug)]
struct Bucket {
    lo: usize,  // inclusive
    hi: usize,  // exclusive
    count: f64, // rows in the bucket
    ndv: usize, // distinct values that occur in the bucket
}

#[derive(Clone, Debug)]
struct Hist {
    buckets: Vec<Bucket>,
}

impl Hist {
    /// Build from a partition given as boundaries [0, b1, ..., D].
    fn from_bounds(f: &[u64], bounds: &[usize]) -> Hist {
        let buckets = bounds
            .windows(2)
            .map(|w| Bucket {
                lo: w[0],
                hi: w[1],
                count: f[w[0]..w[1]].iter().sum::<u64>() as f64,
                ndv: f[w[0]..w[1]].iter().filter(|&&c| c > 0).count(),
            })
            .collect();
        Hist { buckets }
    }

    fn bucket_of(&self, v: usize) -> usize {
        self.buckets
            .partition_point(|b| b.hi <= v)
            .min(self.buckets.len() - 1)
    }

    /// Total squared deviation of the true frequencies from their bucket mean: the V-optimal objective.
    fn sse(&self, f: &[u64]) -> f64 {
        self.buckets.iter().map(|b| sse_range(f, b.lo, b.hi)).sum()
    }

    // Incremental maintenance: a single counter bump, no re-sorting. ndv is left alone (it is refreshed by ANALYZE).
    fn insert(&mut self, v: usize) {
        let i = self.bucket_of(v);
        self.buckets[i].count += 1.0;
    }
    fn delete(&mut self, v: usize) {
        let i = self.bucket_of(v);
        self.buckets[i].count = (self.buckets[i].count - 1.0).max(0.0);
    }
    fn total(&self) -> f64 {
        self.buckets.iter().map(|b| b.count).sum()
    }
    /// Largest bucket over mean bucket: an equi-depth histogram should sit near 1.0; drift past ~2 means it no longer fits the data.
    fn imbalance(&self) -> f64 {
        let mx = self.buckets.iter().map(|b| b.count).fold(0.0, f64::max);
        mx / (self.total() / self.buckets.len() as f64)
    }
}

impl Estimator for Hist {
    /// Uniformity within the bucket over its DISTINCT values: count / ndv.
    fn eq(&self, v: usize) -> f64 {
        let b = &self.buckets[self.bucket_of(v)];
        if b.ndv == 0 {
            0.0
        } else {
            b.count / b.ndv as f64
        }
    }
    /// Fully covered buckets contribute their count, partially covered ones the overlapping fraction of their width.
    fn range(&self, lo: usize, hi: usize) -> f64 {
        self.buckets
            .iter()
            .map(|b| {
                let (l, h) = (lo.max(b.lo), (hi + 1).min(b.hi));
                if h > l {
                    b.count * (h - l) as f64 / (b.hi - b.lo) as f64
                } else {
                    0.0
                }
            })
            .sum()
    }
}

fn sse_range(f: &[u64], i: usize, j: usize) -> f64 {
    let w = (j - i) as f64;
    let s: f64 = f[i..j].iter().map(|&x| x as f64).sum();
    let s2: f64 = f[i..j].iter().map(|&x| (x as f64) * (x as f64)).sum();
    (s2 - s * s / w).max(0.0)
}

fn equi_width(f: &[u64], b: usize) -> Hist {
    let bounds: Vec<usize> = (0..=b).map(|i| i * f.len() / b).collect();
    Hist::from_bounds(f, &bounds)
}

/// Cut after a value as soon as the running count reaches the next multiple of n/b. A value heavier than several targets still
/// gets ONE bucket (it cannot be split), and the next target is recomputed so the remaining buckets stay balanced.
fn equi_depth(f: &[u64], b: usize) -> Hist {
    let n: u64 = f.iter().sum();
    let target = n as f64 / b as f64;
    let (mut bounds, mut cum, mut k) = (vec![0usize], 0.0, 1.0);
    for (i, &c) in f.iter().enumerate() {
        cum += c as f64;
        if bounds.len() < b && cum >= target * k && i + 1 < f.len() {
            bounds.push(i + 1);
            k = (cum / target).floor() + 1.0;
        }
    }
    bounds.push(f.len());
    Hist::from_bounds(f, &bounds)
}

/// Exact dynamic program: dp[k][j] = min SSE of splitting the first j values into k buckets.
fn v_optimal(f: &[u64], b: usize) -> Hist {
    let d = f.len();
    let (mut s1, mut s2) = (vec![0.0; d + 1], vec![0.0; d + 1]);
    for i in 0..d {
        s1[i + 1] = s1[i] + f[i] as f64;
        s2[i + 1] = s2[i] + (f[i] as f64) * (f[i] as f64);
    }
    // O(1) bucket cost from prefix sums: sum x^2 - (sum x)^2 / width.
    let cost = |i: usize, j: usize| {
        let s = s1[j] - s1[i];
        (s2[j] - s2[i] - s * s / (j - i) as f64).max(0.0)
    };
    let mut dp = vec![vec![f64::INFINITY; d + 1]; b + 1];
    let mut back = vec![vec![0usize; d + 1]; b + 1];
    dp[0][0] = 0.0;
    for k in 1..=b {
        for j in k..=d {
            for i in (k - 1)..j {
                let c = dp[k - 1][i] + cost(i, j);
                if c < dp[k][j] {
                    dp[k][j] = c;
                    back[k][j] = i;
                }
            }
        }
    }
    let mut bounds = vec![d];
    let mut j = d;
    for k in (1..=b).rev() {
        j = back[k][j];
        bounds.push(j);
    }
    bounds.reverse();
    Hist::from_bounds(f, &bounds)
}

/// Boundaries go where the frequency jumps the most between adjacent values.
fn max_diff(f: &[u64], b: usize) -> Hist {
    let mut diffs: Vec<(u64, usize)> = (1..f.len()).map(|i| (f[i].abs_diff(f[i - 1]), i)).collect();
    diffs.sort_by(|a, b| b.cmp(a));
    let mut bounds: Vec<usize> = diffs.iter().take(b - 1).map(|&(_, i)| i).collect();
    bounds.push(0);
    bounds.push(f.len());
    bounds.sort();
    Hist::from_bounds(f, &bounds)
}

/// PostgreSQL-style: exact counts for the k most common values, equi-depth histogram over everything else.
struct McvHist {
    mcv: HashMap<usize, f64>,
    rest: Hist,
}

impl McvHist {
    fn build(f: &[u64], k: usize, b: usize) -> McvHist {
        let mut order: Vec<usize> = (0..f.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(f[i]));
        let mcv: HashMap<usize, f64> = order.iter().take(k).map(|&i| (i, f[i] as f64)).collect();
        let mut rest = f.to_vec();
        for &i in mcv.keys() {
            rest[i] = 0; // the histogram must not double count what the MCV list already holds
        }
        McvHist {
            mcv,
            rest: equi_depth(&rest, b),
        }
    }
}

impl Estimator for McvHist {
    fn eq(&self, v: usize) -> f64 {
        self.mcv.get(&v).copied().unwrap_or_else(|| self.rest.eq(v))
    }
    fn range(&self, lo: usize, hi: usize) -> f64 {
        let head: f64 = self
            .mcv
            .iter()
            .filter(|&(&v, _)| v >= lo && v <= hi)
            .map(|(_, &c)| c)
            .sum();
        head + self.rest.range(lo, hi)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Section 1: histogram comparison on uniform / zipf / clustered columns
// ---------------------------------------------------------------------------------------------------------------------

struct Workload {
    eq: Vec<usize>,
    ranges: Vec<(usize, usize)>,
}

/// Equality probes are drawn from the rows (so hot values are probed in proportion to their frequency), ranges are random.
fn make_workload(col: &[u32], r: &mut Rng) -> Workload {
    let eq = (0..1000)
        .map(|_| col[r.below(col.len() as u64) as usize] as usize)
        .collect();
    let ranges = (0..1000)
        .map(|_| {
            let lo = r.below(D as u64) as usize;
            (lo, (lo + 1 + r.below(D as u64 / 4) as usize).min(D - 1))
        })
        .collect();
    Workload { eq, ranges }
}

struct Score {
    eq_mean: f64,
    eq_max: f64,
    rg_mean: f64,
    rg_max: f64,
}

fn score(e: &dyn Estimator, w: &Workload, f: &[u64], pre: &[u64]) -> Score {
    let eq: Vec<f64> = w.eq.iter().map(|&v| qerr(e.eq(v), f[v] as f64)).collect();
    let rg: Vec<f64> = w
        .ranges
        .iter()
        .map(|&(l, h)| qerr(e.range(l, h), (pre[h + 1] - pre[l]) as f64))
        .collect();
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let max = |v: &[f64]| v.iter().cloned().fold(0.0, f64::max);
    Score {
        eq_mean: mean(&eq),
        eq_max: max(&eq),
        rg_mean: mean(&rg),
        rg_max: max(&rg),
    }
}

fn section_histograms() {
    println!(
        "== 1. histograms: q-error of =, range estimates ({BUCKETS} buckets, {ROWS} rows, domain {D}) =="
    );
    let mut r = Rng(760);
    let datasets: Vec<(&str, Vec<u32>)> = vec![
        ("uniform", gen_uniform(&mut r, ROWS)),
        ("zipf", gen_zipf(&mut r, ROWS)),
        ("clustered", gen_clustered(&mut r, ROWS)),
    ];
    println!(
        "{:<10} {:<12} {:>9} {:>9} {:>9} {:>9} {:>14}",
        "data", "method", "eq mean", "eq max", "rng mean", "rng max", "SSE"
    );
    for (name, col) in &datasets {
        let (f, mut rq) = (freq_of(col), Rng(7));
        let pre = prefix(&f);
        let w = make_workload(col, &mut rq);
        let (ew, ed, vo, md) = (
            equi_width(&f, BUCKETS),
            equi_depth(&f, BUCKETS),
            v_optimal(&f, BUCKETS),
            max_diff(&f, BUCKETS),
        );
        let mcv = McvHist::build(&f, 10, BUCKETS - 2);
        let rows: Vec<(&str, &dyn Estimator, Option<f64>)> = vec![
            ("equi-width", &ew, Some(ew.sse(&f))),
            ("equi-depth", &ed, Some(ed.sse(&f))),
            ("v-optimal", &vo, Some(vo.sse(&f))),
            ("maxdiff", &md, Some(md.sse(&f))),
            ("mcv10+depth", &mcv, None),
        ];
        let mut scores = HashMap::new();
        for (m, e, sse) in rows {
            let s = score(e, &w, &f, &pre);
            let sse_txt = sse.map_or("-".to_string(), |x| format!("{x:.3e}"));
            println!(
                "{:<10} {:<12} {:>9.2} {:>9.1} {:>9.2} {:>9.1} {:>14}",
                name, m, s.eq_mean, s.eq_max, s.rg_mean, s.rg_max, sse_txt
            );
            scores.insert(m, s);
        }
        // V-optimal is optimal for SSE among ALL B-bucket partitions, so it can never lose to the others.
        let tol = 1e-6 * ew.sse(&f).max(1.0);
        assert!(
            vo.sse(&f) <= ed.sse(&f) + tol
                && vo.sse(&f) <= md.sse(&f) + tol
                && vo.sse(&f) <= ew.sse(&f) + tol,
            "{name}: v-optimal SSE"
        );
        if *name == "zipf" {
            assert!(
                scores["equi-depth"].eq_mean < scores["equi-width"].eq_mean,
                "equi-depth must beat equi-width on skew"
            );
            // The head: top-10 hottest values. MCV answers them exactly, a histogram averages them with their bucket.
            let head: Vec<usize> = (0..10).collect();
            let worst = |e: &dyn Estimator| {
                head.iter()
                    .map(|&v| qerr(e.eq(v), f[v] as f64))
                    .fold(0.0, f64::max)
            };
            let (wm, we) = (worst(&mcv), worst(&ed));
            println!(
                "  zipf head (10 hottest values) worst q-error: mcv {wm:.3}, equi-depth {we:.2}, equi-width {:.2}",
                worst(&ew)
            );
            assert!(
                wm < 1.0001 && we > wm * 1.1 && worst(&ew) > 5.0,
                "MCV must fix the zipf head"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Section 2: distinct-value estimation: sampling estimators vs HyperLogLog
// ---------------------------------------------------------------------------------------------------------------------

const HLL_P: u32 = 12;

#[derive(Clone, PartialEq)]
struct Hll {
    regs: Vec<u8>,
}

impl Hll {
    fn new() -> Self {
        Hll {
            regs: vec![0; 1 << HLL_P],
        }
    }
    fn add(&mut self, x: u64) {
        let h = mix(x ^ 0xA5A5_A5A5);
        let idx = (h >> (64 - HLL_P)) as usize;
        // rank = position of the first 1-bit in the remaining bits (+1)
        let rank = ((h << HLL_P) | (1 << (HLL_P - 1))).leading_zeros() as u8 + 1;
        self.regs[idx] = self.regs[idx].max(rank);
    }
    /// Union of two multisets' sketches is the register-wise max: this is why per-partition NDV sketches are mergeable.
    fn merge(&mut self, o: &Hll) {
        for (a, b) in self.regs.iter_mut().zip(&o.regs) {
            *a = (*a).max(*b);
        }
    }
    fn estimate(&self) -> f64 {
        let m = self.regs.len() as f64;
        let z: f64 = self.regs.iter().map(|&r| 2f64.powi(-(r as i32))).sum();
        let raw = 0.7213 / (1.0 + 1.079 / m) * m * m / z;
        let zeros = self.regs.iter().filter(|&&r| r == 0).count() as f64;
        if raw <= 2.5 * m && zeros > 0.0 {
            m * (m / zeros).ln()
        } else {
            raw
        } // small-range: linear counting
    }
}

/// Frequency-of-frequencies of a sample: f[j] = number of values seen exactly j times.
fn fof(sample: &[u64]) -> (usize, HashMap<u64, u64>) {
    let mut cnt: HashMap<u64, u64> = HashMap::new();
    for &v in sample {
        *cnt.entry(v).or_default() += 1;
    }
    let mut f: HashMap<u64, u64> = HashMap::new();
    for &c in cnt.values() {
        *f.entry(c).or_default() += 1;
    }
    (cnt.len(), f)
}

fn section_ndv() -> (Vec<u64>, usize) {
    println!("\n== 2. distinct values: sampling estimators vs HyperLogLog ==");
    const N: usize = 200_000;
    let mut r = Rng(11);
    let z = Zipf::new(60_000, 0.8);
    let col: Vec<u64> = (0..N).map(|_| z.sample(&mut r) as u64).collect();
    let truth = col.iter().collect::<std::collections::HashSet<_>>().len();
    // A 5% uniform row sample, as ANALYZE would take (PostgreSQL samples 300 * statistics_target rows).
    let m = N / 20;
    let sample: Vec<u64> = (0..m).map(|_| col[r.below(N as u64) as usize]).collect();
    let (d, f) = fof(&sample);
    let f1 = *f.get(&1).unwrap_or(&0) as f64;
    let f2 = *f.get(&2).unwrap_or(&0) as f64;
    let rest: f64 = f
        .iter()
        .filter(|&(&j, _)| j >= 2)
        .map(|(_, &c)| c as f64)
        .sum();
    let gee = (N as f64 / m as f64).sqrt() * f1 + rest;
    let chao = d as f64 + f1 * f1 / (2.0 * f2.max(1.0));
    let mut hll = Hll::new();
    col.iter().for_each(|&v| hll.add(v));
    let hll_est = hll.estimate();
    println!("true NDV {truth}   (rows {N}, sample {m})");
    for (name, e) in [
        ("naive (d in sample)", d as f64),
        ("GEE", gee),
        ("Chao1", chao),
        ("HyperLogLog p=12", hll_est),
    ] {
        println!(
            "  {:<20} est {:>9.0}   q-error {:.3}",
            name,
            e,
            qerr(e, truth as f64)
        );
    }
    assert!(
        d <= truth,
        "a sample cannot show more distinct values than exist"
    );
    assert!(
        qerr(hll_est, truth as f64) < 1.06,
        "HLL within 6% (std error 1.04/sqrt(4096) = 1.6%)"
    );
    assert!(
        qerr(gee, truth as f64) < 3.0,
        "GEE has a guaranteed ratio error of about sqrt(n/r)"
    );
    assert!(
        qerr(gee, truth as f64) < qerr(d as f64, truth as f64),
        "GEE must correct the naive underestimate"
    );

    // Per-partition sketches: hash-partition the rows 8 ways, sketch each, merge. Summing partition NDVs would double count.
    let mut parts: Vec<Hll> = (0..8).map(|_| Hll::new()).collect();
    for (i, &v) in col.iter().enumerate() {
        parts[i % 8].add(v);
    }
    let sum_of_parts: f64 = parts.iter().map(|p| p.estimate()).sum();
    let mut merged = Hll::new();
    parts.iter().for_each(|p| merged.merge(p));
    println!(
        "  8 partitions: sum of per-partition NDV {sum_of_parts:.0} (double counts), merged sketch {:.0}",
        merged.estimate()
    );
    assert!(
        merged == hll,
        "merged sketch must equal the one-pass sketch register for register"
    );
    assert!(
        sum_of_parts > 1.3 * truth as f64,
        "partition NDVs overlap, so a plain sum overshoots"
    );
    (col, truth)
}

// ---------------------------------------------------------------------------------------------------------------------
// Section 3: skew with Count-Min and top-k candidates
// ---------------------------------------------------------------------------------------------------------------------

struct CountMin {
    w: usize,
    rows: Vec<Vec<u32>>,
}

impl CountMin {
    fn new(w: usize, d: usize) -> Self {
        CountMin {
            w,
            rows: vec![vec![0; w]; d],
        }
    }
    fn slot(&self, row: usize, x: u64) -> usize {
        (mix(x ^ (row as u64 + 1).wrapping_mul(0x9E37_79B9)) % self.w as u64) as usize
    }
    fn add(&mut self, x: u64) {
        for r in 0..self.rows.len() {
            let s = self.slot(r, x);
            self.rows[r][s] += 1;
        }
    }
    /// Minimum over rows: collisions only ADD, so the estimate never undershoots the true count.
    fn query(&self, x: u64) -> u32 {
        (0..self.rows.len())
            .map(|r| self.rows[r][self.slot(r, x)])
            .min()
            .unwrap()
    }
}

fn section_skew(col: &[u64]) {
    println!("\n== 3. skew: Count-Min sketch + top-k candidates ==");
    let (w, k) = (2048usize, 20usize);
    let mut cm = CountMin::new(w, 5);
    let mut cand: HashMap<u64, u32> = HashMap::new();
    for &x in col {
        cm.add(x);
        let est = cm.query(x);
        if cand.contains_key(&x) || cand.len() < k {
            cand.insert(x, est);
        } else if let Some((&min_key, &min_est)) = cand.iter().min_by_key(|&(_, &e)| e) {
            if est > min_est {
                cand.remove(&min_key); // evict the weakest candidate
                cand.insert(x, est);
            }
        }
    }
    let mut exact: HashMap<u64, u32> = HashMap::new();
    col.iter().for_each(|&x| *exact.entry(x).or_default() += 1);
    let mut under = 0;
    for (&x, &c) in &exact {
        if cm.query(x) < c {
            under += 1;
        }
    }
    let mut top: Vec<(u64, u32)> = exact.iter().map(|(&x, &c)| (x, c)).collect();
    top.sort_by_key(|&(x, c)| (std::cmp::Reverse(c), x));
    let bound = (std::f64::consts::E / w as f64 * col.len() as f64) as u32; // eps * N additive error (w.h.p.)
    println!(
        "  width {w}, depth 5, additive error bound eps*N = {bound}; sketch memory {} KiB",
        w * 5 * 4 / 1024
    );
    println!("  {:>8} {:>8} {:>8}", "value", "true", "CM est");
    for &(x, c) in top.iter().take(5) {
        println!("  {:>8} {:>8} {:>8}", x, c, cm.query(x));
        assert!(
            cand.contains_key(&x),
            "true top-5 value {x} missing from the candidate set"
        );
        assert!(
            cm.query(x) - c <= 2 * bound,
            "overestimate beyond twice the CM bound"
        );
    }
    assert_eq!(under, 0, "Count-Min must never underestimate");
}

// ---------------------------------------------------------------------------------------------------------------------
// Section 4: sampling-based selectivity with confidence bounds
// ---------------------------------------------------------------------------------------------------------------------

fn section_sampling() {
    println!("\n== 4. sampling: selectivity error vs sample size (Hoeffding, delta = 0.05) ==");
    let mut r = Rng(4);
    let col = gen_zipf(&mut r, ROWS);
    let p = col.iter().filter(|&&v| v < 30).count() as f64 / ROWS as f64; // predicate: v < 30
    println!("  true selectivity of (v < 30) = {p:.4}");
    println!(
        "  {:>8} {:>12} {:>12} {:>10}",
        "sample", "mean |err|", "bound eps", "coverage"
    );
    let (mut first, mut last) = (0.0, 0.0);
    for &m in &[100usize, 400, 1600, 6400, 25600] {
        let eps = ((2.0f64 / 0.05).ln() / (2.0 * m as f64)).sqrt(); // P(|p_hat - p| > eps) <= delta
        let (mut sum_err, mut covered, trials) = (0.0, 0, 300);
        for _ in 0..trials {
            let hits = (0..m)
                .filter(|_| col[r.below(ROWS as u64) as usize] < 30)
                .count();
            let err = (hits as f64 / m as f64 - p).abs();
            sum_err += err;
            covered += (err <= eps) as usize;
        }
        let (mean, cov) = (sum_err / trials as f64, covered as f64 / trials as f64);
        println!(
            "  {m:>8} {mean:>12.5} {eps:>12.5} {cov:>9.1}%",
            cov = cov * 100.0
        );
        assert!(cov >= 0.95, "Hoeffding coverage {cov}");
        if m == 100 {
            first = mean
        }
        if m == 25600 {
            last = mean
        }
    }
    // 256x more rows => ~16x smaller error (error ~ 1/sqrt(m)); allow slack for randomness.
    assert!(
        first / last > 8.0,
        "error must shrink like 1/sqrt(m), ratio {}",
        first / last
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Section 5: multi-column statistics
// ---------------------------------------------------------------------------------------------------------------------

const DB: usize = 101; // domain of b = a/10 + {0,1}

/// 2-D equi-width grid: counts per (a-cell, b-cell), rectangle selectivity by fractional overlap per axis.
struct Grid2d {
    ga: usize,
    gb: usize,
    cells: Vec<f64>,
}

impl Grid2d {
    fn build(rows: &[(usize, usize)], ga: usize, gb: usize) -> Grid2d {
        let mut cells = vec![0.0; ga * gb];
        for &(a, b) in rows {
            cells[(a * ga / D) * gb + b * gb / DB] += 1.0;
        }
        Grid2d { ga, gb, cells }
    }
    fn overlap(lo: usize, hi: usize, cell: usize, g: usize, dom: usize) -> f64 {
        let (cl, ch) = (
            cell as f64 * dom as f64 / g as f64,
            (cell + 1) as f64 * dom as f64 / g as f64,
        );
        let (l, h) = ((lo as f64).max(cl), ((hi + 1) as f64).min(ch));
        ((h - l) / (ch - cl)).max(0.0)
    }
    fn est(&self, a: (usize, usize), b: (usize, usize)) -> f64 {
        let mut s = 0.0;
        for i in 0..self.ga {
            let fa = Self::overlap(a.0, a.1, i, self.ga, D);
            if fa == 0.0 {
                continue;
            }
            for j in 0..self.gb {
                s += self.cells[i * self.gb + j] * fa * Self::overlap(b.0, b.1, j, self.gb, DB);
            }
        }
        s
    }
}

fn hll_pair_ndv(rows: impl Iterator<Item = (u64, u64)>) -> f64 {
    let mut h = Hll::new();
    rows.for_each(|(x, y)| h.add(x.wrapping_mul(0x1_0000_0001) ^ y)); // pair -> one 64-bit key
    h.estimate()
}

fn section_multicolumn() {
    println!(
        "\n== 5. multi-column statistics: correlated columns a, b = a/10 + {{0,1}}, c = a % 7 =="
    );
    let mut r = Rng(5);
    let rows: Vec<(usize, usize)> = (0..ROWS)
        .map(|_| {
            let a = r.below(D as u64) as usize;
            (a, a / 10 + r.below(2) as usize)
        })
        .collect();
    let fa = freq_of(&rows.iter().map(|x| x.0 as u32).collect::<Vec<_>>());
    let mut fb = vec![0u64; DB];
    rows.iter().for_each(|x| fb[x.1] += 1);
    let (pa, pb) = (prefix(&fa), prefix(&fb));
    let grid = Grid2d::build(&rows, 25, 25);
    let (mut indep, mut twod) = (vec![], vec![]);
    for _ in 0..500 {
        let (a, b) = rows[r.below(ROWS as u64) as usize]; // queries centred on real rows, as a user filtering a dependent pair would
        let (wa, wb) = (10 + r.below(90) as usize, 1 + r.below(8) as usize);
        let ar = (a.saturating_sub(wa), (a + wa).min(D - 1));
        let br = (b.saturating_sub(wb), (b + wb).min(DB - 1));
        let actual = rows
            .iter()
            .filter(|x| x.0 >= ar.0 && x.0 <= ar.1 && x.1 >= br.0 && x.1 <= br.1)
            .count() as f64;
        // Independence with PERFECT marginals: any remaining error is due to the independence assumption alone.
        let est_ind =
            (pa[ar.1 + 1] - pa[ar.0]) as f64 * (pb[br.1 + 1] - pb[br.0]) as f64 / ROWS as f64;
        indep.push(qerr(est_ind, actual));
        twod.push(qerr(grid.est(ar, br), actual));
    }
    let (mi, m2) = (percentile(&mut indep, 0.5), percentile(&mut twod, 0.5));
    println!(
        "  range-pair queries (500): median q-error independence {mi:.2} (p90 {:.2}), 2-D grid 25x25 {m2:.2} (p90 {:.2})",
        percentile(&mut indep, 0.9),
        percentile(&mut twod, 0.9)
    );
    assert!(
        m2 < mi,
        "2-D histogram must beat independence on correlated columns"
    );

    // Column-group NDV: the product of per-column NDVs is capped by the row count and far too big.
    let ndv_a = fa.iter().filter(|&&c| c > 0).count();
    let ndv_b = fb.iter().filter(|&&c| c > 0).count();
    let true_ab = rows.iter().collect::<std::collections::HashSet<_>>().len();
    let naive = (ndv_a * ndv_b).min(ROWS) as f64;
    let sketch = hll_pair_ndv(rows.iter().map(|&(a, b)| (a as u64, b as u64)));
    println!(
        "  ndv(a)={ndv_a} ndv(b)={ndv_b}: true ndv(a,b)={true_ab}, naive min(n, product)={naive:.0}, HLL on pair={sketch:.0}"
    );
    assert!(qerr(sketch, true_ab as f64) < 1.1 && qerr(naive, true_ab as f64) > 10.0);

    // Functional dependency strength: ndv(determinant) / ndv(determinant, dependent); 1.0 iff the FD holds exactly.
    let ndv_pair = |f: &dyn Fn(&(usize, usize)) -> (usize, usize)| {
        rows.iter()
            .map(f)
            .collect::<std::collections::HashSet<_>>()
            .len() as f64
    };
    let ndv_c = 7.0;
    let s_ac = ndv_a as f64 / ndv_pair(&|x| (x.0, x.0 % 7));
    let s_ab = ndv_a as f64 / true_ab as f64;
    let s_ca = ndv_c / ndv_pair(&|x| (x.0 % 7, x.0));
    println!(
        "  FD strength a->c {s_ac:.3} (exact FD), a->b {s_ab:.3} (noisy), c->a {s_ca:.3} (not a FD)"
    );
    assert!((s_ac - 1.0).abs() < 1e-12 && s_ab < 0.6 && s_ca < 0.01);
}

// ---------------------------------------------------------------------------------------------------------------------
// Section 6: incremental maintenance and staleness
// ---------------------------------------------------------------------------------------------------------------------

/// PostgreSQL auto-analyze rule: analyze when modifications exceed 50 + 10% of the rows present at the last ANALYZE.
struct Staleness {
    rows_at_analyze: u64,
    mods: u64,
}

impl Staleness {
    fn stale(&self) -> bool {
        self.mods as f64 > 50.0 + 0.1 * self.rows_at_analyze as f64
    }
}

fn mean_range_q(e: &dyn Estimator, pre: &[u64], w: &[(usize, usize)]) -> f64 {
    w.iter()
        .map(|&(l, h)| qerr(e.range(l, h), (pre[h + 1] - pre[l]) as f64))
        .sum::<f64>()
        / w.len() as f64
}

fn section_maintenance() {
    println!(
        "\n== 6. maintenance: stale vs incrementally maintained vs rebuilt equi-depth histogram =="
    );
    let mut r = Rng(6);
    let base = gen_uniform(&mut r, 50_000);
    let mut f = freq_of(&base);
    let stale = equi_depth(&f, BUCKETS);
    let mut maintained = stale.clone();
    let mut track = Staleness {
        rows_at_analyze: 50_000,
        mods: 0,
    };
    // Scripted staleness boundary: exactly at 50 + 10% = 5050 modifications the rule must still say "fresh".
    track.mods = 5050;
    assert!(!track.stale());
    track.mods = 5051;
    assert!(track.stale());
    track.mods = 0;
    // 50k new rows land in the last 20% of the domain (e.g. recent timestamps); each one bumps a bucket counter.
    let mut flagged_at = None;
    for i in 0..50_000u64 {
        let v = 800 + r.below(200) as usize;
        f[v] += 1;
        maintained.insert(v);
        track.mods += 1;
        if flagged_at.is_none() && track.stale() {
            flagged_at = Some(i + 1);
        }
    }
    println!(
        "  auto-analyze would fire after {} modifications",
        flagged_at.unwrap()
    );
    assert_eq!(flagged_at, Some(5051));
    let pre = prefix(&f);
    let rebuilt = equi_depth(&f, BUCKETS);
    let w: Vec<(usize, usize)> = (0..1000)
        .map(|_| {
            let lo = r.below(D as u64) as usize;
            (lo, (lo + 1 + r.below(250) as usize).min(D - 1))
        })
        .collect();
    let (qs, qm, qr) = (
        mean_range_q(&stale, &pre, &w),
        mean_range_q(&maintained, &pre, &w),
        mean_range_q(&rebuilt, &pre, &w),
    );
    println!(
        "  mean range q-error: stale {qs:.2}, maintained {qm:.2}, rebuilt {qr:.2}; imbalance maintained {:.2}, rebuilt {:.2}",
        maintained.imbalance(),
        rebuilt.imbalance()
    );
    assert!(
        qm < qs && qr < qm,
        "maintained must beat stale, rebuild must beat maintained"
    );
    assert!(
        maintained.imbalance() > 2.0 && rebuilt.imbalance() < 1.5,
        "skew check must demand a rebuild only for the maintained one"
    );
    // Deletes undo inserts exactly.
    let before = maintained.total();
    (0..100).for_each(|i| maintained.insert(i));
    (0..100).for_each(|i| maintained.delete(i));
    assert_eq!(before, maintained.total());
}

fn main() {
    section_histograms();
    let (col, _truth) = section_ndv();
    section_skew(&col);
    section_sampling();
    section_multicolumn();
    section_maintenance();
    println!("\nall statistics checks passed");
}
