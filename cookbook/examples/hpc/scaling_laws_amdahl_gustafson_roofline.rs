//! RECIPE 882 (hpc): scaling laws: Amdahl, Gustafson, Karp-Flatt, USL fit, STREAM bandwidth and the roofline model
//! LEVEL: intermediate
//! Use in: deciding whether adding cores/nodes will help, capacity planning, explaining why a kernel stops scaling, picking what to optimise.   Run: cargo run --release --example scaling_laws_amdahl_gustafson_roofline
//! COMMON USES: capacity planning for services (USL), HPC strong/weak scaling reports, "is this loop memory- or compute-bound" triage (roofline), STREAM as the standard memory-bandwidth sanity check on a new machine.
//! Course: Berkeley CS267 (Applying Parallel Computing) lectures on performance models; Williams/Waterman/Patterson "Roofline" (CACM 2009).
#![allow(dead_code)]
//!
//! Four models, one file, and one rule: every ASSERT uses a synthetic, deterministic work model (operation counts and
//! planted parameters), never wall-clock time, so the asserts are identical on a laptop and on a CI box. Real timings are
//! measured and PRINTED so you can compare your machine with the models.
//!
//! 1. Amdahl (strong scaling): fixed problem, serial fraction f. S(p) = 1 / (f + (1-f)/p), limit 1/f.
//! 2. Gustafson (weak scaling): problem grows with p, so scaled speedup S(p) = p - f (p-1); no hard limit.
//! 3. Karp-Flatt: given a MEASURED speedup S on p workers, the experimentally determined serial fraction is
//!    e = (1/S - 1/p) / (1 - 1/p). If e is constant in p, the loss is a true serial part; if e grows with p, the
//!    loss is overhead (communication, contention) that Amdahl does not model.
//! 4. Universal Scalability Law (Gunther): C(N) = N / (1 + sigma (N-1) + kappa N (N-1)). sigma = contention (queueing for a
//!    shared resource, Amdahl-like), kappa = coherency (crosstalk, pairwise cost, makes throughput RETROGRADE after
//!    N* = sqrt((1-sigma)/kappa)). Fit trick: N/C - 1 = sigma (N-1) + kappa N (N-1) is LINEAR in (sigma, kappa), so
//!    ordinary least squares (2x2 normal equations) recovers them.
//! 5. STREAM kernels (copy, scale, add, triad) and the roofline: attainable GFLOP/s = min(peak, AI * bandwidth), where
//!    AI (arithmetic intensity) = flops / bytes moved. Kernels left of the ridge point (peak/bandwidth) are memory-bound.
//!
//! Why the synthetic model: a "measured" speedup is computed from an abstract time T(p) = f W + (1-f) W / p with small
//! seeded multiplicative noise. Fitting recovers the planted f within a bound we can prove from the noise level, so the
//! assert tests OUR estimator code, not the machine.
//! Read: https://en.wikipedia.org/wiki/Amdahl%27s_law ; http://www.perfdynamics.com/Manifesto/USLscalability.html ;
//!       https://www.cs.virginia.edu/stream/ref.html ; https://en.wikipedia.org/wiki/Roofline_model ; https://en.algorithmica.org/hpc/
//! TRY NEXT:
//!   - [S] In `fit_amdahl` swap the least-squares estimator for the mean of per-point `karp_flatt` values; predict which one is more
//!         sensitive to noise at p = 2 versus p = 16, then print both errors over the 2000 trials.
//!   - [S] Set the planted kappa in `usl_planted_recovery` to 0 and predict what `usl_peak` returns; then raise sigma to 0.3 and find the
//!         smallest noise level at which the recovered kappa goes negative (a physically meaningless fit).
//!   - [M] Add a fifth STREAM-like kernel (`dot`, 2 flops per 16 bytes) to `kernels()` and its verification; predict its roofline class for
//!         the machine in `Machine::laptop()`, then change `peak_gflops` until the class flips and record the ridge point.
//!   - [M] Port Karp-Flatt to your own code: time any rayon `par_iter().sum()` on 1..N threads with `ThreadPoolBuilder`, feed the speedups
//!         to `karp_flatt`, and decide whether the loss is serial work or growing overhead (does e grow with p?).
//!   - [A] PROJECT: combine with `false_sharing` and `thread_pool_work_stealing`: build a thread-pool benchmark whose per-task counter is
//!         (a) one shared atomic, (b) cache-padded per thread; fit USL to throughput versus threads for both and show kappa drops with padding.

use std::thread;
use std::time::Instant;

// ---------- tiny deterministic RNG ----------

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// uniform in [0,1)
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

// ---------- 1-3. Amdahl, Gustafson, Karp-Flatt ----------

/// Strong-scaling speedup with serial fraction `f` on `p` workers.
fn amdahl(f: f64, p: f64) -> f64 {
    1.0 / (f + (1.0 - f) / p)
}

/// Weak-scaling ("scaled") speedup: serial part stays fixed, parallel part grows with p.
fn gustafson(f: f64, p: f64) -> f64 {
    p - f * (p - 1.0)
}

/// Experimentally determined serial fraction from a measured speedup `s` on `p` workers (p >= 2).
fn karp_flatt(s: f64, p: f64) -> f64 {
    (1.0 / s - 1.0 / p) / (1.0 - 1.0 / p)
}

/// Least-squares serial fraction from several (p, speedup) points.
/// Model: 1/S - 1/p = f * (1 - 1/p). With a_i = 1 - 1/p_i, b_i = 1/S_i - 1/p_i the OLS slope through the origin is
/// sum(a b) / sum(a a). WHY through the origin: at p = 1 both sides are exactly 0, so no intercept is needed.
fn fit_amdahl(points: &[(f64, f64)]) -> f64 {
    let (mut sab, mut saa) = (0.0, 0.0);
    for &(p, s) in points {
        let a = 1.0 - 1.0 / p;
        let b = 1.0 / s - 1.0 / p;
        sab += a * b;
        saa += a * a;
    }
    sab / saa
}

/// Synthetic "measured" times: T(p) = (f + (1-f)/p) * W * (1 + noise), W = 1 unit of abstract work.
/// Noise is seeded and bounded by `rel_noise`, so the fit error has a provable bound.
fn synthetic_speedups(f: f64, ps: &[f64], rel_noise: f64, rng: &mut Rng) -> Vec<(f64, f64)> {
    let t1 = 1.0; // p = 1 is the reference, noise-free by definition
    ps.iter()
        .map(|&p| {
            let noise = 1.0 + rng.range(-rel_noise, rel_noise);
            let tp = (f + (1.0 - f) / p) * noise;
            (p, t1 / tp)
        })
        .collect()
}

fn amdahl_properties() {
    // Exact identities on a grid.
    for fi in 0..=20 {
        let f = fi as f64 / 20.0;
        assert!((amdahl(f, 1.0) - 1.0).abs() < 1e-12, "S(1) must be 1");
        assert!((gustafson(f, 1.0) - 1.0).abs() < 1e-12);
        if f > 0.0 {
            // never exceeds the asymptote 1/f
            for p in [2.0, 8.0, 64.0, 1e6] {
                assert!(amdahl(f, p) <= 1.0 / f + 1e-9);
            }
        }
        for p in [2.0, 4.0, 16.0, 128.0] {
            // Gustafson (scaled) is always at least Amdahl (fixed size) for the same f
            assert!(gustafson(f, p) + 1e-12 >= amdahl(f, p));
            if f > 0.0 && f < 1.0 {
                // Karp-Flatt inverts Amdahl exactly
                let e = karp_flatt(amdahl(f, p), p);
                assert!(
                    (e - f).abs() < 1e-9,
                    "karp_flatt must invert amdahl: {e} vs {f}"
                );
            }
        }
    }
    println!("amdahl/gustafson identities hold on a 21 x 4 grid");
}

fn amdahl_fit_trials() {
    let ps: Vec<f64> = (2..=16).map(|p| p as f64).collect();
    let mut rng = Rng::new(882);
    let mut worst = 0.0f64;
    for _ in 0..2000 {
        let f = rng.range(0.01, 0.4);
        let pts = synthetic_speedups(f, &ps, 0.01, &mut rng);
        let fit = fit_amdahl(&pts);
        worst = worst.max((fit - f).abs());
    }
    // Bound: relative noise 1% on T gives |1/S error| <= 0.01 * T(p) <= 0.01 * (f + (1-f)/p) <= 0.01 * (0.4 + 0.3) < 0.007;
    // each a_i >= 1/2, so the OLS slope error is below 0.007 / 0.5 = 0.014. We assert 0.02.
    assert!(worst < 0.02, "fit error {worst} exceeds the provable bound");
    println!(
        "amdahl fit: planted f recovered within {worst:.4} over 2000 noisy trials (bound 0.02)"
    );

    // Overhead detection: add a per-worker overhead term c*p to the time. Karp-Flatt's e must then GROW with p,
    // whereas a pure Amdahl workload keeps e flat.
    let f = 0.05;
    let c = 0.002;
    let t = |p: f64| f + (1.0 - f) / p + c * (p - 1.0);
    let e: Vec<f64> = [2.0, 4.0, 8.0, 16.0]
        .iter()
        .map(|&p| karp_flatt(1.0 / t(p), p))
        .collect();
    assert!(
        e.windows(2).all(|w| w[1] > w[0]),
        "overhead should make e grow: {e:?}"
    );
    let flat: Vec<f64> = [2.0, 4.0, 8.0, 16.0]
        .iter()
        .map(|&p| karp_flatt(amdahl(f, p), p))
        .collect();
    assert!(flat.iter().all(|x| (x - f).abs() < 1e-9));
    println!("karp-flatt e with overhead: {e:.4?} (grows) vs pure serial: {flat:.4?} (flat)");
}

// ---------- real kernel measured on 1..N threads (printed only) ----------

/// A compute-bound kernel: iterate an LCG-ish integer mix `iters` times per element. Result is deterministic and
/// independent of the thread count, which we assert; the SPEEDUP is only printed.
fn work_chunk(lo: usize, hi: usize, iters: u32) -> u64 {
    let mut acc = 0u64;
    for i in lo..hi {
        let mut x = i as u64 + 1;
        for _ in 0..iters {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            x ^= x >> 29;
        }
        acc = acc.wrapping_add(x);
    }
    acc
}

fn parallel_work(n: usize, iters: u32, threads: usize) -> u64 {
    let chunk = n.div_ceil(threads);
    thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|t| {
                let lo = (t * chunk).min(n);
                let hi = ((t + 1) * chunk).min(n);
                s.spawn(move || work_chunk(lo, hi, iters))
            })
            .collect();
        hs.into_iter()
            .fold(0u64, |a, h| a.wrapping_add(h.join().unwrap()))
    })
}

fn measure_real_scaling() {
    let n = 400_000;
    let iters = 60;
    let max_t = thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(2)
        .min(8);
    let expected = work_chunk(0, n, iters);
    let t0 = Instant::now();
    let _ = parallel_work(n, iters, 1);
    let t1 = t0.elapsed().as_secs_f64();
    println!("real kernel (printed, never asserted): threads speedup  karp-flatt-e");
    let mut pts = vec![];
    for th in 1..=max_t {
        let t = Instant::now();
        let r = parallel_work(n, iters, th);
        let dt = t.elapsed().as_secs_f64();
        assert_eq!(
            r, expected,
            "wrapping sum must not depend on the thread count"
        );
        let s = t1 / dt;
        if th > 1 {
            pts.push((th as f64, s));
            println!(
                "   {th:>2} threads   {s:>5.2}x   e = {:>6.3}",
                karp_flatt(s, th as f64)
            );
        } else {
            println!("   {th:>2} threads   {s:>5.2}x");
        }
    }
    if pts.len() >= 2 {
        println!(
            "   fitted serial fraction on this machine: {:.3}",
            fit_amdahl(&pts).max(0.0)
        );
    }
}

// ---------- 4. Universal Scalability Law ----------

fn usl_capacity(sigma: f64, kappa: f64, n: f64) -> f64 {
    n / (1.0 + sigma * (n - 1.0) + kappa * n * (n - 1.0))
}

/// Peak of the USL curve: N* = sqrt((1 - sigma) / kappa).
fn usl_peak(sigma: f64, kappa: f64) -> f64 {
    ((1.0 - sigma) / kappa).sqrt()
}

/// Least-squares fit of (sigma, kappa) from (N, C(N)) points, C normalised so that C(1) = 1.
/// Linearisation: y = N/C - 1 = sigma * x1 + kappa * x2 with x1 = N-1, x2 = N(N-1). Solve the 2x2 normal equations.
fn fit_usl(points: &[(f64, f64)]) -> (f64, f64) {
    let (mut s11, mut s12, mut s22, mut t1, mut t2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for &(n, c) in points {
        let x1 = n - 1.0;
        let x2 = n * (n - 1.0);
        let y = n / c - 1.0;
        s11 += x1 * x1;
        s12 += x1 * x2;
        s22 += x2 * x2;
        t1 += x1 * y;
        t2 += x2 * y;
    }
    let det = s11 * s22 - s12 * s12;
    ((t1 * s22 - t2 * s12) / det, (s11 * t2 - s12 * t1) / det)
}

fn usl_planted_recovery() {
    let ns: Vec<f64> = [1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64]
        .iter()
        .map(|&n| n as f64)
        .collect();
    // Noise-free: exact recovery.
    let (sigma, kappa) = (0.05, 0.0008);
    let clean: Vec<(f64, f64)> = ns
        .iter()
        .map(|&n| (n, usl_capacity(sigma, kappa, n)))
        .collect();
    let (s, k) = fit_usl(&clean);
    assert!(
        (s - sigma).abs() < 1e-9 && (k - kappa).abs() < 1e-9,
        "exact data must recover exactly: {s} {k}"
    );
    println!(
        "usl exact recovery: sigma={s:.6} kappa={k:.6}; peak N* = {:.1}",
        usl_peak(sigma, kappa)
    );

    // Noisy: 0.5% multiplicative noise on throughput, many planted pairs.
    let mut rng = Rng::new(8820);
    let (mut worst_s, mut worst_k) = (0.0f64, 0.0f64);
    for _ in 0..2000 {
        let sigma = rng.range(0.01, 0.15);
        let kappa = rng.range(0.0002, 0.003);
        let pts: Vec<(f64, f64)> = ns
            .iter()
            .map(|&n| {
                (
                    n,
                    usl_capacity(sigma, kappa, n) * (1.0 + rng.range(-0.005, 0.005)),
                )
            })
            .collect();
        let (s, k) = fit_usl(&pts);
        worst_s = worst_s.max((s - sigma).abs());
        worst_k = worst_k.max((k - kappa).abs());
    }
    // Empirical bounds (loose multiples of what the seeded run produces, stable because the seed is fixed).
    assert!(worst_s < 0.02, "sigma error {worst_s}");
    assert!(worst_k < 0.0004, "kappa error {worst_k}");
    println!(
        "usl noisy recovery over 2000 trials: max |d sigma| = {worst_s:.4}, max |d kappa| = {worst_k:.5}"
    );

    // Retrograde behaviour: capacity must fall after N*, rise before it.
    let nstar = usl_peak(sigma, kappa);
    let below = usl_capacity(sigma, kappa, (nstar * 0.5).floor());
    let at = usl_capacity(sigma, kappa, nstar.round());
    let above = usl_capacity(sigma, kappa, nstar * 2.0);
    assert!(
        at > below && at > above,
        "peak must beat both sides: {below} {at} {above}"
    );
    // kappa = 0 degenerates to Amdahl with f = sigma.
    for n in [2.0, 7.0, 40.0] {
        assert!((usl_capacity(sigma, 0.0, n) - amdahl(sigma, n)).abs() < 1e-9);
    }
    println!(
        "usl: capacity {below:.2} at N*/2, {at:.2} at N*, {above:.2} at 2N* (retrograde); kappa=0 equals Amdahl"
    );
}

// ---------- 5. STREAM kernels and the roofline ----------

/// The four STREAM kernels. Returns bytes moved per element (8-byte f64) for the bandwidth computation.
struct Stream {
    a: Vec<f64>,
    b: Vec<f64>,
    c: Vec<f64>,
}

impl Stream {
    fn new(n: usize) -> Self {
        Stream {
            a: vec![1.0; n],
            b: vec![2.0; n],
            c: vec![0.0; n],
        }
    }
    fn copy(&mut self) {
        for i in 0..self.a.len() {
            self.c[i] = self.a[i];
        }
    }
    fn scale(&mut self, q: f64) {
        for i in 0..self.a.len() {
            self.b[i] = q * self.c[i];
        }
    }
    fn add(&mut self) {
        for i in 0..self.a.len() {
            self.c[i] = self.a[i] + self.b[i];
        }
    }
    fn triad(&mut self, q: f64) {
        for i in 0..self.a.len() {
            self.a[i] = self.b[i] + q * self.c[i];
        }
    }
}

/// Run the canonical STREAM sequence once and return the expected scalar values, computed analytically the same way
/// the reference STREAM `checkSTREAMresults` does (replay the recurrence on three scalars).
fn stream_expected(reps: usize, q: f64) -> (f64, f64, f64) {
    let (mut a, mut b, mut c) = (1.0f64, 2.0f64, 0.0f64);
    for _ in 0..reps {
        c = a;
        b = q * c;
        c = a + b;
        a = b + q * c;
    }
    (a, b, c)
}

fn stream_bench() {
    let n = 2_000_000; // 16 MB per array: larger than most L2/L3 slices, so DRAM-ish
    let q = 3.0;
    let reps = 5;
    let mut s = Stream::new(n);
    let mut best = [f64::INFINITY; 4];
    for _ in 0..reps {
        let t = Instant::now();
        s.copy();
        best[0] = best[0].min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        s.scale(q);
        best[1] = best[1].min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        s.add();
        best[2] = best[2].min(t.elapsed().as_secs_f64());
        let t = Instant::now();
        s.triad(q);
        best[3] = best[3].min(t.elapsed().as_secs_f64());
    }
    // Verification (assert): replay on scalars; all elements of each array must match.
    let (ea, eb, ec) = stream_expected(reps, q);
    assert!(
        s.a.iter()
            .all(|&x| (x - ea).abs() <= 1e-9 * ea.abs().max(1.0)),
        "a mismatch"
    );
    assert!(
        s.b.iter()
            .all(|&x| (x - eb).abs() <= 1e-9 * eb.abs().max(1.0)),
        "b mismatch"
    );
    assert!(
        s.c.iter()
            .all(|&x| (x - ec).abs() <= 1e-9 * ec.abs().max(1.0)),
        "c mismatch"
    );
    // Bytes moved per element: copy/scale read 1 + write 1 = 16; add/triad read 2 + write 1 = 24 (STREAM's convention,
    // ignoring write-allocate traffic).
    let bytes = [16.0, 16.0, 24.0, 24.0];
    let names = ["copy", "scale", "add", "triad"];
    println!("STREAM (n = {n}, best of {reps}; printed, never asserted):");
    for k in 0..4 {
        println!(
            "   {:<6} {:>7.2} GB/s",
            names[k],
            bytes[k] * n as f64 / best[k] / 1e9
        );
    }
}

#[derive(Clone, Copy)]
struct Machine {
    peak_gflops: f64,
    bw_gbs: f64,
}

impl Machine {
    /// A made-up but typical laptop core cluster: 200 GFLOP/s peak, 50 GB/s DRAM. Fixed so classification asserts are
    /// machine independent.
    fn laptop() -> Self {
        Machine {
            peak_gflops: 200.0,
            bw_gbs: 50.0,
        }
    }
    /// Ridge point in flops/byte: where the memory roof meets the compute roof.
    fn ridge(&self) -> f64 {
        self.peak_gflops / self.bw_gbs
    }
    fn attainable(&self, ai: f64) -> f64 {
        (ai * self.bw_gbs).min(self.peak_gflops)
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Bound {
    Memory,
    Compute,
}

struct Kernel {
    name: &'static str,
    flops: f64,
    bytes: f64,
}

impl Kernel {
    fn ai(&self) -> f64 {
        self.flops / self.bytes
    }
}

fn classify(m: &Machine, k: &Kernel) -> Bound {
    if k.ai() < m.ridge() {
        Bound::Memory
    } else {
        Bound::Compute
    }
}

/// Flops and compulsory bytes for the kernels that appear elsewhere in this cookbook. "Compulsory" means each operand
/// is moved between DRAM and the core once (perfect cache reuse beyond that).
fn kernels() -> Vec<(Kernel, Bound)> {
    let n = 1024.0f64; // matrix side, FFT size, etc.
    vec![
        (
            Kernel {
                name: "stream copy",
                flops: 0.0,
                bytes: 16.0,
            },
            Bound::Memory,
        ),
        (
            Kernel {
                name: "stream scale",
                flops: 1.0,
                bytes: 16.0,
            },
            Bound::Memory,
        ),
        (
            Kernel {
                name: "stream add",
                flops: 1.0,
                bytes: 24.0,
            },
            Bound::Memory,
        ),
        (
            Kernel {
                name: "stream triad",
                flops: 2.0,
                bytes: 24.0,
            },
            Bound::Memory,
        ),
        (
            Kernel {
                name: "dot product",
                flops: 2.0,
                bytes: 16.0,
            },
            Bound::Memory,
        ),
        // 5-point Jacobi: 4 adds + 1 mul (5 flops); with a good cache each grid point is read once and written once.
        (
            Kernel {
                name: "jacobi 5-pt (jacobi_stencil_tiling)",
                flops: 5.0,
                bytes: 16.0,
            },
            Bound::Memory,
        ),
        // CSR SpMV: 2 flops per nonzero; moves 8B value + 4B column index (+ amortised x and y traffic).
        (
            Kernel {
                name: "SpMV CSR (sparse_kernels_spmv_csr)",
                flops: 2.0,
                bytes: 12.0,
            },
            Bound::Memory,
        ),
        // naive matmul with no reuse: 2 flops per 16B loaded (a[i][k], b[k][j]) -> memory-bound
        (
            Kernel {
                name: "matmul naive (no reuse)",
                flops: 2.0,
                bytes: 16.0,
            },
            Bound::Memory,
        ),
        // blocked matmul compulsory: 2 n^3 flops, 3 n^2 doubles moved once: AI = 2n/24
        (
            Kernel {
                name: "matmul ideal reuse n=1024",
                flops: 2.0 * n * n * n,
                bytes: 24.0 * n * n,
            },
            Bound::Compute,
        ),
        // FFT: 5 n log2 n flops, 16n bytes in + 16n out compulsory (complex f64)
        (
            Kernel {
                name: "FFT n=2^20 compulsory",
                flops: 5.0 * 1_048_576.0 * 20.0,
                bytes: 32.0 * 1_048_576.0,
            },
            Bound::Memory,
        ), // AI 3.1 < ridge 4: even an FFT is memory-bound here
    ]
}

fn roofline_report() {
    let m = Machine::laptop();
    println!(
        "roofline for peak {} GFLOP/s, {} GB/s: ridge = {:.1} flop/byte",
        m.peak_gflops,
        m.bw_gbs,
        m.ridge()
    );
    for (k, expect) in kernels() {
        let got = classify(&m, &k);
        assert_eq!(
            got,
            expect,
            "{} classified {:?}, expected {:?} (AI {:.3})",
            k.name,
            got,
            expect,
            k.ai()
        );
        println!(
            "   {:<40} AI {:>8.3}  attainable {:>7.1} GFLOP/s  {:?}-bound",
            k.name,
            k.ai(),
            m.attainable(k.ai()),
            got
        );
    }
}

fn roofline_properties() {
    let mut rng = Rng::new(88200);
    for _ in 0..2000 {
        let m = Machine {
            peak_gflops: rng.range(10.0, 2000.0),
            bw_gbs: rng.range(5.0, 500.0),
        };
        let k = Kernel {
            name: "r",
            flops: rng.range(0.0, 100.0),
            bytes: rng.range(1.0, 64.0),
        };
        let att = m.attainable(k.ai());
        assert!(att <= m.peak_gflops + 1e-9 && att <= k.ai() * m.bw_gbs + 1e-9);
        // exactly one roof is active
        match classify(&m, &k) {
            Bound::Memory => assert!((att - k.ai() * m.bw_gbs).abs() < 1e-9),
            Bound::Compute => assert!((att - m.peak_gflops).abs() < 1e-9),
        }
        // doubling bandwidth never hurts, and helps only memory-bound kernels
        let m2 = Machine {
            bw_gbs: m.bw_gbs * 2.0,
            ..m
        };
        assert!(m2.attainable(k.ai()) + 1e-9 >= att);
        if classify(&m, &k) == Bound::Compute {
            assert!((m2.attainable(k.ai()) - att).abs() < 1e-9);
        }
    }
    println!("roofline properties hold on 2000 random machines/kernels");
}

// ---------- Gustafson vs Amdahl comparison table ----------

fn print_tables() {
    println!("   p   Amdahl(f=.05) Gustafson(f=.05) USL(s=.05,k=.0008)");
    for p in [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0] {
        println!(
            "  {p:>3}   {:>10.2}   {:>12.2}   {:>12.2}",
            amdahl(0.05, p),
            gustafson(0.05, p),
            usl_capacity(0.05, 0.0008, p)
        );
    }
}

fn main() {
    println!("== Amdahl / Gustafson / Karp-Flatt ==");
    amdahl_properties();
    amdahl_fit_trials();
    print_tables();
    println!("== real kernel on this machine ==");
    measure_real_scaling();
    println!("== USL ==");
    usl_planted_recovery();
    println!("== STREAM + roofline ==");
    stream_bench();
    roofline_report();
    roofline_properties();
    let _ = Rng::new(1).below(2);
    println!("OK: all deterministic assertions passed");
}
