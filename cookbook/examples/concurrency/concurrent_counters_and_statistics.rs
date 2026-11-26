//! RECIPE 904 (concurrency): concurrent counters and statistics: sharded, thread-local+flush, sloppy, Morris, histograms, HyperLogLog merge.
//! LEVEL: advanced
//! Use in: metrics libraries, rate counters on hot paths, per-request statistics, cardinality estimation across shards or threads.   Run: cargo run --release --example concurrent_counters_and_statistics
//! COMMON USES: Prometheus/StatsD client libraries (sharded or per-thread counters), the Linux kernel's per-CPU counters and "sloppy counters" (Boyd-Wickizer et al.), Redis `PFADD/PFMERGE` (HyperLogLog), HdrHistogram-style latency recording.
//! Course: CMU 15-445 (statistics for query optimisation) for sketches; MIT 6.S081/OSDI "An Analysis of Linux Scalability to Many Cores" for sloppy counters.
#![allow(dead_code)]
//!
//! THE POINT: a counter looks trivial, but on 8 cores one `AtomicU64::fetch_add` is a cache-line ping-pong; the fix is ALWAYS the same idea,
//! trade read precision/cost for write scalability: keep the write path local and merge on read.
//!   1. `Shared`       : one atomic. Exact, simple, the baseline that stops scaling.
//!   2. `Sharded`      : N cache-line-padded atomics, a thread picks `tid % N`; `read` sums them. Exact at quiescence; while running the sum lies between
//!                       "increments FINISHED before the read started" and "increments STARTED before it ended" (we assert exactly that bracket).
//!   3. Thread-local   : a plain `Cell<u64>` per thread flushed to a global atomic every K increments and on exit. Writes touch no shared line at all.
//!   4. `Sloppy`       : the same with an explicit error bound: reads under-count by at most `threads * (threshold - 1)`; asserted below.
//!   5. `Morris`       : an APPROXIMATE counter in 1 byte: increment with probability (1+a)^-c, estimate ((1+a)^c - 1)/a. Unbiased; relative std ~ sqrt(a/2).
//!   6. `Histogram`    : log-linear buckets (8 sub-buckets per power of two, relative error <= 12.5%) with `AtomicU64` per bucket: exact count and sum,
//!                       quantiles accurate to one bucket, verified against a sorted vector.
//!   7. `Hll`          : HyperLogLog (p = 12, 4096 one-byte registers). Merge = element-wise max, so per-thread sketches combine into EXACTLY the
//!                       sketch a single thread would have built (asserted), and a shared sketch can update registers lock-free with `fetch_max`.
//! FALSE SHARING is measured at the end: the same 8 threads incrementing 8 DIFFERENT counters, adjacent in memory vs padded to 128 bytes. (Times printed, never asserted.)
//! Read: Boyd-Wickizer et al., "An Analysis of Linux Scalability to Many Cores" (OSDI 2010, title only, not re-read). Flajolet et al., "HyperLogLog: the analysis of a near-optimal cardinality estimation algorithm" (2007, title only).
//! Read: Morris, "Counting large numbers of events in small registers" (CACM 1978, title only). https://docs.rs/crossbeam-utils/latest/crossbeam_utils/struct.CachePadded.html and https://hdrhistogram.org/
//!
//! TRY NEXT:
//!   - [S] Change `Sharded`'s shard count to 1, 2, 4, 8, 64 and print time for 8 threads; predict where the curve flattens and why it is not "at 8".
//!   - [S] Predict, then measure, the effect of Morris base `a` = 1.0 (powers of two, the classic) on the mean error over 200 trials versus a = 0.01.
//!   - [M] Add `merge(&mut self, other: &Hll)` support for sparse sketches (store only non-zero registers in a sorted vector until 256 of them are set) and assert the estimate is unchanged by the representation switch.
//!   - [M] Make `Histogram::quantile` run while writers are recording and state exactly which guarantee you can still assert (hint: the bracket idea from `Sharded`).
//!   - [A] PROJECT: wire `Histogram` + `Sharded` into the registry of `metrics_prometheus_histograms_load_testing` as the lock-free write path, and add an HLL-backed "unique users" gauge; verify with the 8-thread harness from `concurrent_hashmap_designs`.

use std::cell::Cell;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering::*};
use std::thread;
use std::time::Instant;

const THREADS: usize = 8;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// ---------------------------------------------------------------- 2. sharded
#[repr(align(128))] // 128: Apple M-series and some Intel parts prefetch line PAIRS
struct Padded(AtomicU64);
struct Sharded {
    shards: Vec<Padded>,
}
impl Sharded {
    fn new(n: usize) -> Self {
        Sharded {
            shards: (0..n).map(|_| Padded(AtomicU64::new(0))).collect(),
        }
    }
    fn add(&self, tid: usize, d: u64) {
        // Release/Acquire (not Relaxed) ONLY so the bracket test below is formally valid: a pure statistics counter can use Relaxed.
        self.shards[tid % self.shards.len()].0.fetch_add(d, Release);
    }
    fn read(&self) -> u64 {
        self.shards.iter().map(|s| s.0.load(Acquire)).sum()
    }
}

// ---------------------------------------------------------------- 3. thread-local with flush (and the Drop trap)
static TLS_GLOBAL: AtomicU64 = AtomicU64::new(0);
struct Local(Cell<u64>);
impl Local {
    fn flush(&self) {
        TLS_GLOBAL.fetch_add(self.0.replace(0), Relaxed);
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        self.flush(); // runs at thread exit; only std::thread::JoinHandle::join waits for it, a scope may NOT
    }
}
thread_local! {
    static LOCAL: Local = const { Local(Cell::new(0)) };
}
fn tls_inc() {
    LOCAL.with(|l| {
        l.0.set(l.0.get() + 1);
        if l.0.get() >= 1024 {
            l.flush();
        }
    });
}

// ---------------------------------------------------------------- 4. sloppy counter with a proven bound
struct Sloppy {
    global: AtomicU64,
    threshold: u64,
}
struct SloppyHandle<'a> {
    c: &'a Sloppy,
    local: u64,
}
impl Sloppy {
    fn handle(&self) -> SloppyHandle<'_> {
        SloppyHandle { c: self, local: 0 }
    }
}
impl SloppyHandle<'_> {
    fn inc(&mut self) {
        self.local += 1;
        if self.local >= self.c.threshold {
            self.c.global.fetch_add(self.local, Relaxed);
            self.local = 0;
        }
    }
}
impl Drop for SloppyHandle<'_> {
    fn drop(&mut self) {
        self.c.global.fetch_add(self.local, Relaxed);
    }
}

// ---------------------------------------------------------------- 5. Morris counter
struct Morris {
    c: u8,
    a: f64,
}
impl Morris {
    fn new(a: f64) -> Self {
        Morris { c: 0, a }
    }
    fn inc(&mut self, rng: &mut Rng) {
        // P(increment) = (1+a)^-c ; the exponent keeps the register tiny
        if rng.unit() < (1.0 + self.a).powi(-(self.c as i32)) {
            self.c = self.c.checked_add(1).expect("register overflow");
        }
    }
    fn estimate(&self) -> f64 {
        ((1.0 + self.a).powi(self.c as i32) - 1.0) / self.a
    }
}

// ---------------------------------------------------------------- 6. histogram
const SUB: u64 = 8;
const NB: usize = 8 * 62;
struct Histogram {
    buckets: Vec<AtomicU64>,
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
}
fn bucket_of(v: u64) -> usize {
    if v < SUB {
        return v as usize;
    }
    let m = 63 - v.leading_zeros() as u64; // floor(log2 v) >= 3
    let sub = (v >> (m - 3)) & (SUB - 1);
    ((m - 2) * SUB + sub) as usize
}
fn bucket_lower(i: usize) -> u64 {
    if (i as u64) < SUB {
        return i as u64;
    }
    let (m, sub) = (i as u64 / SUB + 2, i as u64 % SUB);
    (SUB + sub) << (m - 3)
}
impl Histogram {
    fn new() -> Self {
        Histogram {
            buckets: (0..NB).map(|_| AtomicU64::new(0)).collect(),
            count: AtomicU64::new(0),
            sum: AtomicU64::new(0),
            max: AtomicU64::new(0),
        }
    }
    fn record(&self, v: u64) {
        self.buckets[bucket_of(v)].fetch_add(1, Relaxed);
        self.count.fetch_add(1, Relaxed);
        self.sum.fetch_add(v, Relaxed);
        self.max.fetch_max(v, Relaxed);
    }
    /// Returns the bucket index holding the q-quantile (nearest-rank).
    fn quantile_bucket(&self, q: f64) -> usize {
        let n = self.buckets.iter().map(|b| b.load(Relaxed)).sum::<u64>();
        let rank = ((q * n as f64).ceil() as u64).max(1);
        let mut cum = 0;
        for (i, b) in self.buckets.iter().enumerate() {
            cum += b.load(Relaxed);
            if cum >= rank {
                return i;
            }
        }
        NB - 1
    }
}

// ---------------------------------------------------------------- 7. HyperLogLog
const P: u32 = 12;
const M: usize = 1 << P;
struct Hll {
    regs: Vec<AtomicU8>,
}
impl Hll {
    fn new() -> Self {
        Hll {
            regs: (0..M).map(|_| AtomicU8::new(0)).collect(),
        }
    }
    fn add(&self, item: u64) {
        let h = mix(item);
        let idx = (h >> (64 - P)) as usize;
        let rest = (h << P) | (1 << (P - 1)); // sentinel bit bounds the leading-zero count
        let rho = rest.leading_zeros() as u8 + 1;
        self.regs[idx].fetch_max(rho, Relaxed); // lock-free, commutative, idempotent
    }
    fn merge(&self, other: &Hll) {
        for (a, b) in self.regs.iter().zip(&other.regs) {
            a.fetch_max(b.load(Relaxed), Relaxed);
        }
    }
    fn snapshot(&self) -> Vec<u8> {
        self.regs.iter().map(|r| r.load(Relaxed)).collect()
    }
    fn estimate(&self) -> f64 {
        let m = M as f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let (mut sum, mut zeros) = (0.0, 0);
        for r in &self.regs {
            let v = r.load(Relaxed);
            sum += 2f64.powi(-(v as i32));
            zeros += (v == 0) as usize;
        }
        let e = alpha * m * m / sum;
        if e <= 2.5 * m && zeros > 0 {
            m * (m / zeros as f64).ln()
        } else {
            e
        } // small-range: linear counting
    }
}

// ---------------------------------------------------------------- demos / verification
fn sharded_bracket_test() {
    let c = Sharded::new(THREADS);
    let started = AtomicU64::new(0);
    let finished = AtomicU64::new(0);
    let stop = AtomicU64::new(0);
    thread::scope(|s| {
        for t in 0..THREADS - 1 {
            let (c, started, finished) = (&c, &started, &finished);
            s.spawn(move || {
                for _ in 0..100_000 {
                    started.fetch_add(1, SeqCst);
                    c.add(t, 1);
                    finished.fetch_add(1, SeqCst);
                }
            });
        }
        let (c, started, finished, stop) = (&c, &started, &finished, &stop);
        s.spawn(move || {
            while stop.load(Relaxed) == 0 {
                let lo = finished.load(SeqCst);
                let r = c.read();
                let hi = started.load(SeqCst);
                assert!(
                    lo <= r && r <= hi,
                    "sharded read {r} escaped its bracket [{lo}, {hi}]"
                );
                if lo == 700_000 {
                    break;
                }
            }
        });
    });
    stop.store(1, Relaxed);
    assert_eq!(c.read(), 700_000);
    println!(
        "  sharded: 7 writers x 100k, concurrent reads always inside [finished, started]; final exact"
    );
}

fn tls_test() {
    // std::thread::spawn + join: join waits for the thread to fully exit, so TLS destructors have flushed the remainder.
    let hs: Vec<_> = (0..THREADS)
        .map(|_| {
            thread::spawn(|| {
                for _ in 0..100_000 {
                    tls_inc();
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(TLS_GLOBAL.load(Relaxed), 800_000);
    println!("  thread-local + flush: 800000 exactly (flush every 1024 and in Drop)");
}

fn sloppy_test() {
    let c = Sloppy {
        global: AtomicU64::new(0),
        threshold: 100,
    };
    let handles: Vec<SloppyHandle> = thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS)
            .map(|t| {
                let c = &c;
                s.spawn(move || {
                    let mut h = c.handle();
                    for _ in 0..(10_000 + t * 37) {
                        h.inc();
                    }
                    h // return it UNflushed so we can inspect the error
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let truth: u64 = (0..THREADS).map(|t| 10_000 + t as u64 * 37).sum();
    let read = c.global.load(Relaxed);
    let bound = THREADS as u64 * (c.threshold - 1);
    assert!(
        read <= truth && truth - read <= bound,
        "sloppy error {} exceeds bound {bound}",
        truth - read
    );
    drop(handles);
    assert_eq!(c.global.load(Relaxed), truth);
    println!(
        "  sloppy: read {read} vs truth {truth} (error {} <= bound {bound}); exact after flush",
        truth - read
    );
}

fn morris_test() {
    let (a, n, trials) = (0.1, 10_000u64, 300);
    let mut rng = Rng(0xDEAD_BEEF);
    let mut est = Vec::new();
    let mut max_reg = 0;
    for _ in 0..trials {
        let mut m = Morris::new(a);
        for _ in 0..n {
            m.inc(&mut rng);
        }
        max_reg = max_reg.max(m.c);
        est.push(m.estimate());
    }
    let mean = est.iter().sum::<f64>() / trials as f64;
    let rel = (mean - n as f64).abs() / n as f64;
    assert!(rel < 0.05, "Morris estimator biased? mean {mean}");
    let sd =
        (est.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / trials as f64).sqrt() / n as f64;
    assert!(
        sd < 0.35,
        "variance far above sqrt(a/2) = {:.2}",
        (a / 2.0f64).sqrt()
    );
    println!(
        "  morris(a=0.1): n={n}, mean estimate {mean:.0} (bias {:.1}%), relative sd {sd:.2}, register max {max_reg} (fits in a u8)",
        rel * 100.0
    );
}

fn histogram_test() {
    // bucket arithmetic first
    for v in [
        0u64,
        1,
        7,
        8,
        9,
        15,
        16,
        17,
        1000,
        123_456_789,
        u64::MAX / 2,
    ] {
        let i = bucket_of(v);
        assert!(
            bucket_lower(i) <= v && (i + 1 >= NB || v < bucket_lower(i + 1)),
            "bucket arithmetic wrong at {v}"
        );
    }
    let h = Histogram::new();
    let all: Vec<Vec<u64>> = thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS as u64)
            .map(|t| {
                let h = &h;
                s.spawn(move || {
                    let mut r = Rng(t + 11);
                    let vals: Vec<u64> = (0..50_000)
                        .map(|_| 1 + (r.unit().powi(3) * 5_000_000.0) as u64)
                        .collect(); // heavy-ish tail
                    vals.iter().for_each(|&v| h.record(v));
                    vals
                })
            })
            .collect();
        hs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    let mut sorted: Vec<u64> = all.into_iter().flatten().collect();
    sorted.sort_unstable();
    assert_eq!(h.count.load(Relaxed), sorted.len() as u64);
    assert_eq!(h.sum.load(Relaxed), sorted.iter().sum::<u64>());
    assert_eq!(h.max.load(Relaxed), *sorted.last().unwrap());
    let mut line = String::new();
    for q in [0.5, 0.9, 0.99, 0.999] {
        let rank = ((q * sorted.len() as f64).ceil() as usize).max(1);
        let truth = sorted[rank - 1];
        let b = h.quantile_bucket(q);
        assert_eq!(
            b,
            bucket_of(truth),
            "q={q}: histogram bucket differs from the true quantile's bucket"
        );
        let (lo, hi) = (bucket_lower(b), bucket_lower(b + 1));
        assert!(
            (hi - lo) as f64 <= 0.126 * lo.max(8) as f64 + 1.0,
            "bucket wider than 12.5%"
        );
        line += &format!(" p{}={} in [{lo},{hi})", q * 100.0, truth);
    }
    println!("  histogram: 400000 samples from 8 threads, exact count/sum/max;{line}");
}

fn hll_test() {
    let items_per_thread = 60_000u64;
    // Overlapping ranges: thread t sees ids [t*30000, t*30000 + 60000): union size = 7*30000 + 60000.
    let truth = 7 * 30_000 + items_per_thread;
    let ids = |t: u64| t * 30_000..t * 30_000 + items_per_thread;

    let sequential = Hll::new();
    for t in 0..THREADS as u64 {
        ids(t).for_each(|i| sequential.add(i));
    }
    // (a) per-thread sketches merged
    let locals: Vec<Hll> = thread::scope(|s| {
        let hs: Vec<_> = (0..THREADS as u64)
            .map(|t| {
                s.spawn(move || {
                    let h = Hll::new();
                    ids(t).for_each(|i| h.add(i));
                    h
                })
            })
            .collect();
        hs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    let merged = Hll::new();
    locals.iter().for_each(|l| merged.merge(l));
    // (b) one shared sketch updated by all threads at once
    let shared = Hll::new();
    thread::scope(|s| {
        for t in 0..THREADS as u64 {
            let shared = &shared;
            s.spawn(move || ids(t).for_each(|i| shared.add(i)));
        }
    });
    assert_eq!(
        merged.snapshot(),
        sequential.snapshot(),
        "merge must equal the sequential sketch"
    );
    assert_eq!(
        shared.snapshot(),
        sequential.snapshot(),
        "concurrent fetch_max must equal the sequential sketch"
    );
    let est = merged.estimate();
    let err = (est - truth as f64).abs() / truth as f64;
    assert!(
        err < 3.0 * 1.04 / (M as f64).sqrt(),
        "HLL error {err:.4} above 3 sigma"
    );
    // tiny cardinalities use linear counting
    let small = Hll::new();
    (0..100).for_each(|i| small.add(i));
    assert!((small.estimate() - 100.0).abs() < 5.0);
    println!(
        "  hll: truth {truth}, estimate {est:.0} (error {:.2}%, sigma {:.2}%); merged == shared == sequential registers",
        err * 100.0,
        104.0 / (M as f64).sqrt()
    );
}

fn false_sharing_bench() {
    const ITERS: usize = 3_000_000;
    struct Packed([AtomicU64; THREADS]);
    let packed = Packed(std::array::from_fn(|_| AtomicU64::new(0)));
    let padded: Vec<Padded> = (0..THREADS).map(|_| Padded(AtomicU64::new(0))).collect();
    let single = AtomicU64::new(0);
    let run = |f: &(dyn Fn(usize) + Sync)| {
        let t0 = Instant::now();
        thread::scope(|s| {
            for t in 0..THREADS {
                s.spawn(move || {
                    for _ in 0..ITERS {
                        f(t)
                    }
                });
            }
        });
        t0.elapsed().as_secs_f64() * 1000.0
    };
    let a = run(&|_| {
        single.fetch_add(1, Relaxed);
    });
    let b = run(&|t| {
        packed.0[t].fetch_add(1, Relaxed);
    });
    let c = run(&|t| {
        padded[t].0.fetch_add(1, Relaxed);
    });
    assert_eq!(single.load(Relaxed), (THREADS * ITERS) as u64);
    assert_eq!(
        packed.0.iter().map(|x| x.load(Relaxed)).sum::<u64>(),
        (THREADS * ITERS) as u64
    );
    println!(
        "  8 threads x {ITERS} increments: one shared atomic {a:.0} ms | 8 adjacent atomics (false sharing) {b:.0} ms | 8 padded atomics {c:.0} ms"
    );
}

fn main() {
    println!("== exact counters ==");
    sharded_bracket_test();
    tls_test();
    sloppy_test();
    println!("== approximate / statistical ==");
    morris_test();
    histogram_test();
    hll_test();
    println!("== cache-line effects (printed only) ==");
    false_sharing_bench();
    let _ = AtomicUsize::new(0);
    println!("all counter/statistics checks passed");
}
