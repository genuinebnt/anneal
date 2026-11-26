//! RECIPE 886 (hpc): the loop optimisation ladder: measured steps on dot product, prefix sum, convolution, histogram and friends
//! LEVEL: advanced
//! Use in: any hot inner loop: codecs, image filters, database scans, numeric kernels, parsers.   Run: cargo run --release --example loop_optimization_ladder
//! COMMON USES: squeezing the last 2-10x out of scan/aggregate kernels (columnar databases), image/video filters, checksum and hash loops, histogram-based sorting and compression.
//! Course: CMU 15-418 (single-core optimisation), Algorithmica "Algorithms for Modern Hardware".
#![allow(dead_code)]
//!
//! Every optimisation here is a REWRITE that must not change the answer, so every rung of every ladder is asserted to
//! produce a bit-identical result (the data are u32/u8 with wrapping arithmetic, which is an exact ring: reassociating,
//! factoring and reordering are all legal, unlike for floats). Timings are printed (best of several runs) and never asserted.
//! Rungs, in the order a performance engineer tries them:
//!   1. remove bounds checks the right way: iterators, zip, `chunks_exact`, slicing once up front (NOT `get_unchecked`)
//!   2. break dependency chains: several independent accumulators (also the only way for floats, see `float_demo`)
//!   3. fuse loops that read the same memory; fission loops to enable a cheaper formulation (separable convolution)
//!   4. strength reduction: `%` -> mask or wrapping counter, 9 multiplies -> 6
//!   5. branch removal: data-dependent `if` -> mask/select (cmov / csel); sorted vs random input shows the predictor
//!   6. memory layout: SoA vs AoS, 4 sub-histograms against store-forwarding stalls, alignment, software prefetch (x86_64 only; no-op elsewhere)
//!   7. inlining control: `#[inline(always)]` vs `#[inline(never)]`, `#[cold]` for error paths
//! Method: write the baseline, look at what limits it (latency chain? bandwidth? mispredicts? bounds checks?), change ONE thing,
//! re-measure, keep it only if it wins. Use `cargo asm`/Godbolt to confirm the vectoriser did what you think; timing alone lies.
//! Read: https://en.algorithmica.org/hpc/ (esp. pipelining, SIMD reduction) and https://en.algorithmica.org/hpc/simd/reduction/
//! Read: https://en.algorithmica.org/hpc/pipelining/tables/ (instruction latency/throughput) and https://doc.rust-lang.org/std/hint/fn.black_box.html
//! Read: https://doc.rust-lang.org/std/primitive.slice.html#method.chunks_exact (the idiom that gives LLVM a known trip count)
//! TRY NEXT:
//!   - [S] In `dot_acc8` change the 8 accumulators to 2, then 16; predict the knee (latency x throughput of the multiply-add) and measure.
//!   - [S] Replace `sum_branchless` with `sum_select` on SORTED input and explain why the branchy version catches up (see main's second table).
//!   - [M] Predict, then measure: does `hist_4` still win when the input is uniformly random bytes instead of 90% zeros? Change `make_hist_data` and find out.
//!   - [M] Port `prefix_blocked4` to a 2-level scheme (block totals, then add offsets) and make it row-parallel with thread::scope; compare with parallel_prefix_scan_reduction.
//!   - [A] PROJECT: take the byte histogram rungs into parallel_sample_sort_radix: use `hist_4`-style sub-histograms in `lsd_radix_serial`, measure per-pass time,
//!     and add a SoA key/payload variant; cross-check results with cache_locality-style experiments from the perf chapter.
use std::hint::black_box;
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn u32(&mut self) -> u32 {
        (self.next() >> 16) as u32
    }
}

// ================================================================ ladder 1: dot product

/// Baseline: the trip count `n` is not tied to the slice lengths, so every iteration keeps two bounds checks,
/// which also blocks vectorisation.
fn dot_indexed(a: &[u32], b: &[u32], n: usize) -> u32 {
    let mut s = 0u32;
    for i in 0..n {
        s = s.wrapping_add(a[i].wrapping_mul(b[i]));
    }
    s
}

/// zip: the iterator knows both lengths, so no checks remain. For integers LLVM then vectorises on its own.
fn dot_zip(a: &[u32], b: &[u32]) -> u32 {
    a.iter()
        .zip(b)
        .fold(0u32, |s, (&x, &y)| s.wrapping_add(x.wrapping_mul(y)))
}

/// 8 independent accumulators: one per SIMD lane, so the add chain is 8 times shorter.
fn dot_acc8(a: &[u32], b: &[u32]) -> u32 {
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    let mut acc = [0u32; 8];
    for (x, y) in ca.zip(cb) {
        for l in 0..8 {
            acc[l] = acc[l].wrapping_add(x[l].wrapping_mul(y[l]));
        }
    }
    acc.iter().fold(dot_zip(ra, rb), |s, &v| s.wrapping_add(v))
}

/// 4 vectors of 8 lanes: hides the multiply-add latency of one vector register behind three others.
fn dot_acc4x8(a: &[u32], b: &[u32]) -> u32 {
    let (ca, cb) = (a.chunks_exact(32), b.chunks_exact(32));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    let mut acc = [[0u32; 8]; 4];
    for (x, y) in ca.zip(cb) {
        for k in 0..4 {
            for l in 0..8 {
                acc[k][l] = acc[k][l].wrapping_add(x[k * 8 + l].wrapping_mul(y[k * 8 + l]));
            }
        }
    }
    let mut s = dot_zip(ra, rb);
    for row in &acc {
        for &v in row {
            s = s.wrapping_add(v);
        }
    }
    s
}

/// Floats cannot be reassociated by the compiler (it would change the rounding), so multiple accumulators must be
/// written by hand, and the result is NOT bit-identical to the sequential sum: that is the price.
fn float_demo(rng: &mut Rng) {
    let n = 1 << 16;
    let a: Vec<f32> = (0..n).map(|_| rng.u32() as f32 / u32::MAX as f32).collect();
    let seq: f32 = a.iter().sum();
    let mut acc = [0f32; 8];
    let ch = a.chunks_exact(8);
    let rem: f32 = ch.remainder().iter().sum();
    for c in ch {
        for l in 0..8 {
            acc[l] += c[l];
        }
    }
    let par: f32 = acc.iter().sum::<f32>() + rem;
    let exact: f64 = a.iter().map(|&x| x as f64).sum();
    println!(
        "\nfloat caveat: sequential f32 sum {seq}, 8-accumulator sum {par}, f64 reference {exact:.3}; identical bits? {}",
        seq.to_bits() == par.to_bits()
    );
    assert!(
        ((par as f64) - exact).abs() / exact < 1e-4 && ((seq as f64) - exact).abs() / exact < 1e-3
    );
}

// ================================================================ ladder 2: prefix sum

fn prefix_indexed(a: &[u32]) -> Vec<u32> {
    let n = a.len();
    let mut out = vec![0u32; n];
    if n == 0 {
        return out;
    }
    out[0] = a[0];
    for i in 1..n {
        out[i] = out[i - 1].wrapping_add(a[i]); // loop-carried dependency through memory AND a register
    }
    out
}

fn prefix_scan_iter(a: &[u32]) -> Vec<u32> {
    let mut acc = 0u32;
    a.iter()
        .map(|&x| {
            acc = acc.wrapping_add(x);
            acc
        })
        .collect()
}

/// 4 elements per step: the local prefix of the block does not depend on `acc`, so the serial chain through `acc`
/// is one add per FOUR outputs. The idea scales up to SIMD log-steps and to the blocked parallel scan.
fn prefix_blocked4(a: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len());
    let mut acc = 0u32;
    let ch = a.chunks_exact(4);
    let rem = ch.remainder();
    for c in ch {
        let l0 = c[0];
        let l1 = l0.wrapping_add(c[1]);
        let l2 = l1.wrapping_add(c[2]);
        let l3 = l2.wrapping_add(c[3]);
        out.extend_from_slice(&[
            acc.wrapping_add(l0),
            acc.wrapping_add(l1),
            acc.wrapping_add(l2),
            acc.wrapping_add(l3),
        ]);
        acc = acc.wrapping_add(l3);
    }
    for &x in rem {
        acc = acc.wrapping_add(x);
        out.push(acc);
    }
    out
}

// ================================================================ ladder 3: 3x3 convolution (valid region)

type Kernel3 = [[u32; 3]; 3];

fn outer(v: [u32; 3], u: [u32; 3]) -> Kernel3 {
    let mut k = [[0u32; 3]; 3];
    for dy in 0..3 {
        for dx in 0..3 {
            k[dy][dx] = v[dy].wrapping_mul(u[dx]);
        }
    }
    k
}

/// Baseline: four nested loops, index arithmetic recomputed, kernel read from memory each time.
fn conv_naive(img: &[u32], w: usize, h: usize, k: &Kernel3) -> Vec<u32> {
    if w < 3 || h < 3 {
        return Vec::new();
    }
    let mut out = vec![0u32; (w - 2) * (h - 2)];
    for y in 0..h - 2 {
        for x in 0..w - 2 {
            let mut s = 0u32;
            for dy in 0..3 {
                for dx in 0..3 {
                    s = s.wrapping_add(k[dy][dx].wrapping_mul(img[(y + dy) * w + x + dx]));
                }
            }
            out[y * (w - 2) + x] = s;
        }
    }
    out
}

/// Unrolled taps over three row slices: constants hoisted into registers, no 2-D index arithmetic, no bounds checks.
fn conv_unrolled(img: &[u32], w: usize, h: usize, k: &Kernel3) -> Vec<u32> {
    if w < 3 || h < 3 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity((w - 2) * (h - 2));
    let [k0, k1, k2] = *k;
    for y in 0..h - 2 {
        let r0 = &img[y * w..(y + 1) * w];
        let r1 = &img[(y + 1) * w..(y + 2) * w];
        let r2 = &img[(y + 2) * w..(y + 3) * w];
        for ((a, b), c) in r0.windows(3).zip(r1.windows(3)).zip(r2.windows(3)) {
            let mut s = 0u32;
            for t in 0..3 {
                s = s.wrapping_add(k0[t].wrapping_mul(a[t]));
                s = s.wrapping_add(k1[t].wrapping_mul(b[t]));
                s = s.wrapping_add(k2[t].wrapping_mul(c[t]));
            }
            out.push(s);
        }
    }
    out
}

/// Loop FISSION + strength reduction: a rank-1 kernel k = v u^T is a horizontal 1-D pass then a vertical 1-D pass.
/// 6 multiplies instead of 9 and both passes are simple streaming loops (the temp buffer is the price).
fn conv_separable(img: &[u32], w: usize, h: usize, v: [u32; 3], u: [u32; 3]) -> Vec<u32> {
    if w < 3 || h < 3 {
        return Vec::new();
    }
    let ow = w - 2;
    let mut tmp = Vec::with_capacity(ow * h);
    for row in img.chunks_exact(w) {
        for t in row.windows(3) {
            tmp.push(
                u[0].wrapping_mul(t[0])
                    .wrapping_add(u[1].wrapping_mul(t[1]))
                    .wrapping_add(u[2].wrapping_mul(t[2])),
            );
        }
    }
    let mut out = Vec::with_capacity(ow * (h - 2));
    for y in 0..h - 2 {
        let (a, b, c) = (
            &tmp[y * ow..(y + 1) * ow],
            &tmp[(y + 1) * ow..(y + 2) * ow],
            &tmp[(y + 2) * ow..(y + 3) * ow],
        );
        out.extend((0..ow).map(|x| {
            v[0].wrapping_mul(a[x])
                .wrapping_add(v[1].wrapping_mul(b[x]))
                .wrapping_add(v[2].wrapping_mul(c[x]))
        }));
    }
    out
}

// ================================================================ ladder 4: histogram

fn hist_naive(d: &[u8]) -> Vec<u32> {
    let mut h = vec![0u32; 256];
    for &b in d {
        h[b as usize] += 1; // if the same byte repeats, each increment waits for the previous store (store-to-load forwarding)
    }
    h
}

/// 4 sub-histograms: consecutive bytes touch different tables, so repeated values no longer serialise.
fn hist_4(d: &[u8]) -> Vec<u32> {
    let mut h = [[0u32; 256]; 4];
    let ch = d.chunks_exact(4);
    let rem = ch.remainder();
    for c in ch {
        for k in 0..4 {
            h[k][c[k] as usize] += 1;
        }
    }
    for &b in rem {
        h[0][b as usize] += 1;
    }
    (0..256)
        .map(|i| h[0][i] + h[1][i] + h[2][i] + h[3][i])
        .collect()
}

fn make_hist_data(rng: &mut Rng, n: usize) -> Vec<u8> {
    // 90% zeros: the adversarial case for the naive histogram
    (0..n)
        .map(|_| {
            if rng.below(10) == 0 {
                rng.next() as u8
            } else {
                0
            }
        })
        .collect()
}

// ================================================================ ladder 5: branch removal

fn sum_branchy(d: &[u32], t: u32) -> u64 {
    let mut s = 0u64;
    for &x in d {
        if x >= t {
            s += x as u64;
        }
    }
    s
}

/// All-ones/all-zeros mask from the comparison: no branch, data-independent timing.
fn sum_branchless(d: &[u32], t: u32) -> u64 {
    let mut s = 0u64;
    for &x in d {
        let mask = 0u32.wrapping_sub((x >= t) as u32);
        s += (x & mask) as u64;
    }
    s
}

/// Plain select: LLVM usually lowers this to cmov/csel (or vector blend). Often the idiomatic way to say it.
fn sum_select(d: &[u32], t: u32) -> u64 {
    d.iter().map(|&x| if x >= t { x as u64 } else { 0 }).sum()
}

// ================================================================ ladder 6: strength reduction, fusion, layout, prefetch

fn strided_mod(a: &[u32], t: &[u32], m: usize) -> u32 {
    a.iter()
        .enumerate()
        .fold(0u32, |s, (i, &x)| s.wrapping_add(x.wrapping_mul(t[i % m]))) // runtime divisor: a real division
}

fn strided_mask(a: &[u32], t: &[u32], m: usize) -> u32 {
    debug_assert!(m.is_power_of_two());
    a.iter().enumerate().fold(0u32, |s, (i, &x)| {
        s.wrapping_add(x.wrapping_mul(t[i & (m - 1)]))
    })
}

fn strided_counter(a: &[u32], t: &[u32], m: usize) -> u32 {
    let (mut s, mut j) = (0u32, 0usize);
    for &x in a {
        s = s.wrapping_add(x.wrapping_mul(t[j]));
        j += 1;
        if j == m {
            j = 0; // works for ANY m, not only powers of two
        }
    }
    s
}

fn minmaxsum_three_passes(a: &[u32]) -> (u32, u32, u64) {
    let mn = a.iter().copied().min().unwrap_or(u32::MAX);
    let mx = a.iter().copied().max().unwrap_or(0);
    let s = a.iter().map(|&x| x as u64).sum();
    (mn, mx, s)
}

/// Fusion: one pass over memory instead of three. Wins whenever the data does not fit in cache (bandwidth bound).
fn minmaxsum_fused(a: &[u32]) -> (u32, u32, u64) {
    let (mut mn, mut mx, mut s) = (u32::MAX, 0u32, 0u64);
    for &x in a {
        mn = mn.min(x);
        mx = mx.max(x);
        s += x as u64;
    }
    (mn, mx, s)
}

#[derive(Clone, Copy)]
struct Particle {
    x: u32,
    y: u32,
    z: u32,
    vx: u32,
    vy: u32,
    vz: u32,
    id: u32,
    flags: u32,
}

struct Particles {
    x: Vec<u32>,
    y: Vec<u32>,
    z: Vec<u32>,
    vx: Vec<u32>,
    vy: Vec<u32>,
    vz: Vec<u32>,
    id: Vec<u32>,
    flags: Vec<u32>,
}

fn to_soa(p: &[Particle]) -> Particles {
    Particles {
        x: p.iter().map(|q| q.x).collect(),
        y: p.iter().map(|q| q.y).collect(),
        z: p.iter().map(|q| q.z).collect(),
        vx: p.iter().map(|q| q.vx).collect(),
        vy: p.iter().map(|q| q.vy).collect(),
        vz: p.iter().map(|q| q.vz).collect(),
        id: p.iter().map(|q| q.id).collect(),
        flags: p.iter().map(|q| q.flags).collect(),
    }
}

/// AoS: to read 4 useful bytes the cache pulls in the whole 32-byte particle: 8x the traffic.
fn sum_x_aos(p: &[Particle]) -> u32 {
    p.iter().fold(0u32, |s, q| s.wrapping_add(q.x))
}

fn sum_x_soa(p: &Particles) -> u32 {
    p.x.iter().fold(0u32, |s, &x| s.wrapping_add(x))
}

/// Software prefetch: only helps when the access pattern is unpredictable but KNOWN early (like indices in an array).
/// x86_64 gets a real `prefetcht0`; other targets compile to nothing, so the speedup column will read ~1.0x there.
#[inline(always)]
#[allow(unused_unsafe)]
fn prefetch<T>(p: *const T) {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: a prefetch is only a hint; it never faults, even for addresses that are not mapped or not valid for reads.
        unsafe {
            core::arch::x86_64::_mm_prefetch::<{ core::arch::x86_64::_MM_HINT_T0 }>(p as *const i8);
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = p;
    }
}

fn gather_plain(table: &[u32], idx: &[u32]) -> u64 {
    idx.iter().map(|&i| table[i as usize] as u64).sum()
}

fn gather_prefetch(table: &[u32], idx: &[u32]) -> u64 {
    const AHEAD: usize = 16;
    let mut s = 0u64;
    for (k, &i) in idx.iter().enumerate() {
        if let Some(&future) = idx.get(k + AHEAD) {
            prefetch(table.as_ptr().wrapping_add(future as usize)); // wrapping_add: computing the address is safe, never dereferenced
        }
        s += table[i as usize] as u64;
    }
    s
}

// ================================================================ ladder 7: inlining and cold paths

#[inline(always)]
fn mix_inline(x: u32) -> u32 {
    x.rotate_left(5) ^ 0x9E37_79B9
}

#[inline(never)]
fn mix_call(x: u32) -> u32 {
    x.rotate_left(5) ^ 0x9E37_79B9
}

fn sum_map<F: Fn(u32) -> u32>(a: &[u32], f: F) -> u64 {
    a.iter().map(|&x| f(x) as u64).sum()
}

const SENTINEL: u32 = 0xDEAD_BEEF;

/// Error construction (format! allocates, pulls in formatting code) sits INSIDE the hot loop's body: bigger code,
/// worse register allocation and layout, even though it never runs.
fn checked_sum_inline(a: &[u32]) -> Result<u64, String> {
    let mut s = 0u64;
    for (i, &x) in a.iter().enumerate() {
        if x == SENTINEL {
            return Err(format!("sentinel {x:#x} found at index {i}"));
        }
        s += x as u64;
    }
    Ok(s)
}

#[cold]
#[inline(never)]
fn fail(i: usize, x: u32) -> String {
    format!("sentinel {x:#x} found at index {i}")
}

/// `#[cold]` tells LLVM the call is unlikely: the branch is laid out out-of-line and the hot path stays tight.
/// The effect is often small; this rung exists to show how to ask for it, then to measure honestly.
fn checked_sum_cold(a: &[u32]) -> Result<u64, String> {
    let mut s = 0u64;
    for (i, &x) in a.iter().enumerate() {
        if x == SENTINEL {
            return Err(fail(i, x));
        }
        s += x as u64;
    }
    Ok(s)
}

// ================================================================ harness

const REPS: usize = 9;

struct Ladder {
    name: &'static str,
    elems: f64,
    expect: Option<u64>,
    base_ns: Option<f64>,
}

impl Ladder {
    fn new(name: &'static str, elems: usize) -> Self {
        println!("\n== {name} ({elems} elements) ==");
        println!(
            "  {:<30} {:>9} {:>9} {:>8}   why",
            "step", "ms", "ns/elem", "speedup"
        );
        Ladder {
            name,
            elems: elems as f64,
            expect: None,
            base_ns: None,
        }
    }

    /// `f` returns a checksum of its full output. Every rung must reproduce the baseline's checksum exactly.
    fn step(&mut self, label: &str, why: &str, mut f: impl FnMut() -> u64) {
        let first = f();
        match self.expect {
            None => self.expect = Some(first),
            Some(e) => assert_eq!(first, e, "{}: step '{label}' changed the output", self.name),
        }
        let mut best = f64::MAX;
        for _ in 0..REPS {
            let t0 = Instant::now();
            let r = black_box(f());
            best = best.min(t0.elapsed().as_secs_f64());
            assert_eq!(r, first, "{}: '{label}' is not deterministic", self.name);
        }
        let ns = best * 1e9;
        let base = *self.base_ns.get_or_insert(ns);
        println!(
            "  {label:<30} {:>9.3} {:>9.3} {:>7.2}x   {why}",
            best * 1e3,
            ns / self.elems,
            base / ns
        );
    }
}

fn digest(v: &[u32]) -> u64 {
    v.iter().fold(0xCBF2_9CE4_8422_2325u64, |h, &x| {
        (h ^ x as u64).wrapping_mul(0x0100_0000_01B3)
    })
}

// ================================================================ verification

fn rand_vec(rng: &mut Rng, n: usize) -> Vec<u32> {
    (0..n).map(|_| rng.u32() ^ (rng.u32() << 16)).collect()
}

fn verify(rng: &mut Rng) {
    for case in 0..2000 {
        let n = if case % 10 == 0 {
            rng.below(5)
        } else {
            rng.below(300)
        };
        let a = rand_vec(rng, n);
        let b = rand_vec(rng, n);
        let want = dot_indexed(&a, &b, n);
        assert_eq!(dot_zip(&a, &b), want);
        assert_eq!(dot_acc8(&a, &b), want);
        assert_eq!(dot_acc4x8(&a, &b), want);

        let p = prefix_indexed(&a);
        assert_eq!(prefix_scan_iter(&a), p);
        assert_eq!(prefix_blocked4(&a), p);
        let mut seq = 0u32;
        for (i, &x) in a.iter().enumerate() {
            seq = seq.wrapping_add(x);
            assert_eq!(p[i], seq, "prefix oracle");
        }

        let (w, h) = (rng.below(24), rng.below(24)); // includes degenerate sizes < 3
        let img = rand_vec(rng, w * h);
        let (v, u) = (
            [rng.u32(), rng.u32(), rng.u32()],
            [rng.u32(), rng.u32(), rng.u32()],
        );
        let k = outer(v, u);
        let want = conv_naive(&img, w, h, &k);
        assert_eq!(
            want.len(),
            if w < 3 || h < 3 { 0 } else { (w - 2) * (h - 2) }
        );
        assert_eq!(conv_unrolled(&img, w, h, &k), want);
        assert_eq!(
            conv_separable(&img, w, h, v, u),
            want,
            "separable == full 2-D kernel (exact in a ring)"
        );

        let bytes = make_hist_data(rng, n);
        let hn = hist_naive(&bytes);
        assert_eq!(hist_4(&bytes), hn);
        assert_eq!(hn.iter().sum::<u32>() as usize, n);

        let t = rng.u32();
        let want = sum_branchy(&a, t);
        assert_eq!(sum_branchless(&a, t), want);
        assert_eq!(sum_select(&a, t), want);

        let m = 1usize << rng.below(7);
        let table = rand_vec(rng, m);
        let want = strided_mod(&a, &table, m);
        assert_eq!(strided_mask(&a, &table, m), want);
        assert_eq!(strided_counter(&a, &table, m), want);
        let m3 = 1 + rng.below(9); // counter version handles non powers of two
        let table3 = rand_vec(rng, m3);
        assert_eq!(
            strided_counter(&a, &table3, m3),
            strided_mod(&a, &table3, m3)
        );

        assert_eq!(minmaxsum_fused(&a), minmaxsum_three_passes(&a));

        let parts: Vec<Particle> = (0..n)
            .map(|i| Particle {
                x: a[i],
                y: b[i],
                z: 1,
                vx: 2,
                vy: 3,
                vz: 4,
                id: i as u32,
                flags: 0,
            })
            .collect();
        assert_eq!(sum_x_aos(&parts), sum_x_soa(&to_soa(&parts)));

        if n > 0 {
            let tl = 1 + rng.below(64);
            let table = rand_vec(rng, tl);
            let idx: Vec<u32> = (0..n).map(|_| rng.below(table.len()) as u32).collect();
            assert_eq!(gather_prefetch(&table, &idx), gather_plain(&table, &idx));
        }

        assert_eq!(sum_map(&a, mix_inline), sum_map(&a, mix_call));
        let mut c = a.clone();
        c.retain(|&x| x != SENTINEL);
        assert_eq!(checked_sum_inline(&c), checked_sum_cold(&c));
        if !c.is_empty() {
            let pos = rng.below(c.len());
            c[pos] = SENTINEL;
            let e = checked_sum_cold(&c).unwrap_err();
            assert_eq!(checked_sum_inline(&c), Err(e));
        }
    }
    println!(
        "2000 random cases (lengths 0..300, degenerate images, duplicates, sentinels): every rung equals its baseline exactly"
    );
}

// ================================================================ main: the ladders

fn main() {
    let mut rng = Rng::new(886);
    verify(&mut rng);
    float_demo(&mut rng);

    // ---- dot product (4 MB x 2: bigger than L2 on many CPUs, small enough to run fast)
    let n = 1 << 20;
    let (a, b) = (rand_vec(&mut rng, n), rand_vec(&mut rng, n));
    let mut l = Ladder::new("dot product", n);
    l.step(
        "indexed, runtime n",
        "2 bounds checks per element block vectorisation",
        || dot_indexed(&a, &b, black_box(n)) as u64,
    );
    l.step(
        "zip iterators",
        "iterator lengths are known: checks gone, LLVM vectorises",
        || dot_zip(black_box(&a), black_box(&b)) as u64,
    );
    l.step(
        "chunks_exact(8), 8 accumulators",
        "one lane per accumulator, shorter add chain",
        || dot_acc8(black_box(&a), black_box(&b)) as u64,
    );
    l.step(
        "chunks_exact(32), 4x8 accumulators",
        "hides multiply latency behind 4 independent registers",
        || dot_acc4x8(black_box(&a), black_box(&b)) as u64,
    );

    // ---- alignment: same data, start address shifted by 4 bytes
    let mut buf = vec![0u32; n + 16];
    let mut al = Ladder::new("dot product vs alignment", n);
    for off in [0usize, 1] {
        buf[off..off + n].copy_from_slice(&a);
        let s = &buf[off..off + n];
        let label = format!("slice offset {off} words");
        al.step(
            &label,
            if off == 0 {
                "start on a 16-byte boundary (typical allocator)"
            } else {
                "unaligned vector loads: cache-line splits cost a little"
            },
            || dot_acc4x8(black_box(s), black_box(&b)) as u64,
        );
    }

    // ---- prefix sum
    let mut l = Ladder::new("prefix sum", n);
    l.step(
        "out[i] = out[i-1] + a[i]",
        "serial chain, load after store",
        || digest(&prefix_indexed(black_box(&a))),
    );
    l.step(
        "scan with running accumulator",
        "chain kept in a register",
        || digest(&prefix_scan_iter(black_box(&a))),
    );
    l.step(
        "blocked by 4",
        "chain through acc is 1 add per 4 outputs",
        || digest(&prefix_blocked4(black_box(&a))),
    );

    // ---- convolution
    let (w, h) = (1024, 512);
    let img = rand_vec(&mut rng, w * h);
    let (v, u) = ([1u32, 2, 1], [3u32, 0, 7]);
    let k = outer(v, u);
    let mut l = Ladder::new("3x3 convolution", (w - 2) * (h - 2));
    l.step(
        "4 nested loops",
        "index math + kernel loads in the hot loop",
        || digest(&conv_naive(&img, w, h, &k)),
    );
    l.step(
        "unrolled taps, row slices",
        "taps in registers, windows() removes checks",
        || digest(&conv_unrolled(&img, w, h, &k)),
    );
    l.step(
        "separable: 2 passes (fission)",
        "6 multiplies instead of 9, streaming loops",
        || digest(&conv_separable(&img, w, h, v, u)),
    );

    // ---- histogram
    let data = make_hist_data(&mut rng, 1 << 22);
    let hsum = |h: Vec<u32>| digest(&h);
    let mut l = Ladder::new("byte histogram (90% zeros)", data.len());
    l.step(
        "single table",
        "repeated byte => increments serialise on one counter",
        || hsum(hist_naive(&data)),
    );
    l.step(
        "4 sub-histograms",
        "neighbouring bytes hit different tables",
        || hsum(hist_4(&data)),
    );

    // ---- branch removal, random then sorted
    let vals = rand_vec(&mut rng, 1 << 22);
    let thr = u32::MAX / 2;
    let mut l = Ladder::new("threshold sum, RANDOM data", vals.len());
    l.step(
        "if x >= t",
        "50% unpredictable branch: ~half mispredict",
        || sum_branchy(&vals, thr),
    );
    l.step("mask arithmetic", "no branch at all", || {
        sum_branchless(&vals, thr)
    });
    l.step(
        "select (cmov/csel)",
        "compiler-chosen select or vector blend",
        || sum_select(&vals, thr),
    );
    let mut sorted = vals.clone();
    sorted.sort_unstable();
    let mut l = Ladder::new("threshold sum, SORTED data", sorted.len());
    l.step(
        "if x >= t",
        "predictor learns the single transition: branch is free",
        || sum_branchy(&sorted, thr),
    );
    l.step(
        "mask arithmetic",
        "same speed as before: it never depended on data",
        || sum_branchless(&sorted, thr),
    );
    l.step("select (cmov/csel)", "same", || sum_select(&sorted, thr));

    // ---- strength reduction
    let m = 64;
    let table = rand_vec(&mut rng, m);
    let mut l = Ladder::new("strided table lookup", n);
    l.step(
        "i % m (runtime m)",
        "a hardware division per element",
        || strided_mod(&a, &table, black_box(m)) as u64,
    );
    l.step("i & (m-1)", "power-of-two modulo is one AND", || {
        strided_mask(&a, &table, black_box(m)) as u64
    });
    l.step("wrapping counter", "no division, works for any m", || {
        strided_counter(&a, &table, black_box(m)) as u64
    });

    // ---- fusion
    let big = rand_vec(&mut rng, 1 << 23);
    let pack =
        |(mn, mx, s): (u32, u32, u64)| mn as u64 ^ (mx as u64).rotate_left(21) ^ s.rotate_left(42);
    let mut l = Ladder::new("min + max + sum", big.len());
    l.step("three passes", "reads 32 MB x 3 from memory", || {
        pack(minmaxsum_three_passes(&big))
    });
    l.step(
        "fused single pass",
        "reads it once: bandwidth bound => about 3x less traffic",
        || pack(minmaxsum_fused(&big)),
    );

    // ---- SoA vs AoS
    let parts: Vec<Particle> = (0..1 << 21)
        .map(|i| Particle {
            x: rng.u32(),
            y: 0,
            z: 0,
            vx: 0,
            vy: 0,
            vz: 0,
            id: i,
            flags: 0,
        })
        .collect();
    let soa = to_soa(&parts);
    let mut l = Ladder::new("sum of one field of 32-byte particles", parts.len());
    l.step(
        "array of structs",
        "streams 32 B per 4 useful bytes",
        || sum_x_aos(&parts) as u64,
    );
    l.step("struct of arrays", "streams only the 4-byte column", || {
        sum_x_soa(&soa) as u64
    });

    // ---- gather with prefetch
    let table = rand_vec(&mut rng, 1 << 22);
    let idx: Vec<u32> = (0..1 << 20)
        .map(|_| rng.below(table.len()) as u32)
        .collect();
    let mut l = Ladder::new("random gather (16 MB table)", idx.len());
    l.step(
        "table[idx[i]]",
        "each load can miss all the way to DRAM",
        || gather_plain(&table, &idx),
    );
    l.step(
        "prefetch idx[i+16]",
        "overlap misses with work (x86_64 only; no-op elsewhere)",
        || gather_prefetch(&table, &idx),
    );

    // ---- inline / cold
    let mut l = Ladder::new("inlining control", n);
    l.step(
        "#[inline(always)] helper",
        "call disappears, loop vectorises",
        || sum_map(&a, mix_inline),
    );
    l.step(
        "#[inline(never)] helper",
        "a real call per element: no vectorisation",
        || sum_map(&a, mix_call),
    );
    let mut l = Ladder::new("error path placement", n);
    l.step(
        "format! inside the loop",
        "cold code mixed with hot code",
        || checked_sum_inline(&a).unwrap(),
    );
    l.step(
        "#[cold] #[inline(never)] fail()",
        "unlikely path moved out of line (effect is often small)",
        || checked_sum_cold(&a).unwrap(),
    );

    println!(
        "\nthe `why` column is the expectation; the numbers are what THIS machine did (modern LLVM already vectorises integer loops, and some rungs lose)."
    );
    println!("all ladder checks passed (timings above are for reading, not asserting)");
}
