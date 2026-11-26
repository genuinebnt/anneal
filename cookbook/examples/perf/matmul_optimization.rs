//! RECIPE 445 (perf): matrix multiplication optimisation ladder: loop order, transposition, tiling, register blocking, SIMD, threads.
//! LEVEL: intermediate
//! Use in: any dense linear algebra, convolution/attention kernels, understanding why "same O(n^3)" can differ 100x.   Run: cargo run --release --example matmul_optimization
#![allow(dead_code)]
//!
//! WHAT: C = A * B for row-major f32/f64, written 8 ways: (1) naive `ijk`, (2) `ikj` (stream rows of B), (3) transposed B
//! (both operands read sequentially), (4) cache-blocked (tile sweep 8..256), (5) register-blocked 4x4 micro-kernel,
//! (6) 4xW micro-kernel shaped for auto-vectorisation, (7) explicit NEON 8x8 FMA kernel (AArch64; an AVX2+FMA 4x16
//! twin is compiled on x86_64 but could not be run here), (8) rayon over blocks of C rows. GFLOPS for N=256/512/1024.
//!
//! WHY (roofline reasoning). The multiply does 2*N^3 flops (one mul + one add per inner step). The ideal traffic is only the
//! three matrices, 3*N^2*sizeof(T) bytes, so the best-case ARITHMETIC INTENSITY is 2N^3 / 12N^2 = N/6 flop/byte for f32
//! (about 170 flop/byte at N=1024): hugely compute-bound, if every loaded value is reused many times from cache/registers.
//! The naive loop reuses nothing: each inner step loads two f32 (8 bytes) for 2 flops = 0.25 flop/byte, and the B access
//! strides by a whole row (N*4 bytes: a new cache line every step, a TLB miss per page). A core with ~100 GB/s of memory
//! bandwidth would then top out near 25 GFLOPS at best; in practice the strided pattern gets 1-3% of peak. Machine balance
//! = peak flops / bandwidth: a modern core retires ~100+ GFLOPS of f32 FMA (e.g. 4 NEON FMA pipes x 4 lanes x 2 flops x ~4 GHz)
//! against ~100 GB/s, so you need roughly >1 flop per byte moved from DRAM, and far more from L2: tiling raises the
//! intensity (each tile of B is reused by every row of the A tile), and registers raise it again (a 4x8 accumulator
//! block does 32 FMAs per 4 + 8 loaded values).
//!
//! HOW each rung helps:
//!  ikj: the innermost loop `c[j] += a_ip * b[p][j]` walks C and B sequentially, so cache lines are fully used and LLVM
//!       vectorises it (no loop-carried dependency: every j is independent). C and B rows (N*4 bytes each) stay in L1/L2.
//!  transposed: the dot product `sum a[i][p] * bt[j][p]` is also sequential but is a reduction: floats are not associative so
//!       LLVM will not vectorise it; we use 8 independent accumulators by hand. Transposition costs O(N^2) and is timed.
//!  blocked: process TxT tiles so the working set (3 tiles) fits L1/L2 and B is not re-streamed from DRAM for every row.
//!  register-blocked: keep a 4x4 (or 4xW) block of C in locals across the whole k loop: C is loaded and stored once per
//!       k-block instead of once per k step, and each A/B value loaded is used 4 times.
//!  NEON/AVX: fused multiply-add on 4/8 lanes with all accumulators in vector registers (8x8: 16 accumulators + 2 B + broadcast A).
//!  rayon: C row blocks are independent (disjoint `&mut` slices via `par_chunks_mut`): no locks, near-linear scaling
//!       until memory bandwidth or the core count is the limit.
//!  Production libraries (BLAS: OpenBLAS, BLIS, MKL, Apple Accelerate) add PACKING of A/B panels into contiguous buffers,
//!       three cache levels of blocking and hand-written assembly micro-kernels; this recipe stops before packing.
//!
//! HONEST RESULT (Apple M4 Pro, big caches): plain tiling (T=64 ikj) is SLOWER than ikj here because B's rows (4 KiB) already
//! fit the large L2 and the tile loops add overhead; tiling pays when matrices or rows exceed the cache, and it is the
//! prerequisite for packing. The big wins are loop order (ikj ~10-15x over naive), register blocking + SIMD, and threads.
//!
//! FOOTGUNS: (1) float results differ by association order: compare against a rigorous error bound (k*eps*sum|a||b|), not ==;
//! the integer-valued test is the exact check. (2) N a power of two (1024 * 4 B = 4 KiB stride) maps column elements to the
//! same cache set (conflict misses); the naive version is worst there. (3) Never benchmark in debug builds. (4) Timings
//! here are single runs of a shared machine: believe the ordering and the orders of magnitude, not the third digit.
//!
//! COMMON USES: the GEMM at the heart of neural-network layers (BLAS `sgemm`/`dgemm`, `ndarray`'s `dot`, the `matrixmultiply`
//! crate), image convolution via im2col, PCA/linear regression normal equations, graphics transforms, scientific simulation.
//! Read: "Algorithms for Modern Hardware", matrix multiplication chapter https://en.algorithmica.org/hpc/algorithms/matmul/ ;
//! "How to Optimize a CUDA/CPU Matmul" style walk-through https://siboehm.com/articles/22/Fast-MMM-on-CPU ;
//! Goto & van de Geijn, "Anatomy of High-Performance Matrix Multiplication" https://www.cs.utexas.edu/~flame/pubs/GotoTOMS_revision.pdf ;
//! Roofline model https://en.wikipedia.org/wiki/Roofline_model
use rayon::prelude::*;
use std::hint::black_box;
use std::time::Instant;

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
    fn unit(&mut self) -> f64 {
        (self.below(2_000_001) as f64 - 1_000_000.0) / 1e6
    } // [-1, 1]
}

/// Generates one module per element type with the whole ladder, tests and benchmark.
macro_rules! matmul_impl {
    ($m:ident, $t:ty, $w:expr, $extras:expr) => {
        mod $m {
            use super::*;
            pub type T = $t;
            pub type Kern = fn(&[T], &[T], &mut [T], usize, usize, usize);
            /// Width (columns) of the auto-vectorisable micro-kernel: 4 rows x W columns of accumulators = 16 vector registers.
            pub const W: usize = $w;
            const KC: usize = 256; // k-block: a B panel of KC rows x W columns stays in L1

            // (1) naive: B is read with stride n
            pub fn naive_ijk(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) {
                for i in 0..m {
                    for j in 0..n {
                        let mut s: T = 0.0;
                        for p in 0..k {
                            s += a[i * k + p] * b[p * n + j];
                        }
                        c[i * n + j] = s;
                    }
                }
            }
            // (2) loop interchange: the inner loop is a saxpy over contiguous rows
            pub fn ikj(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) {
                c[..m * n].fill(0.0);
                for i in 0..m {
                    let crow = &mut c[i * n..(i + 1) * n];
                    for p in 0..k {
                        let aip = a[i * k + p];
                        for (cv, &bv) in crow.iter_mut().zip(&b[p * n..(p + 1) * n]) {
                            *cv += aip * bv;
                        }
                    }
                }
            }
            // (3) transposed B: dot products of two contiguous rows, 8 independent accumulators
            fn dot8(x: &[T], y: &[T]) -> T {
                let (cx, tx) = x.as_chunks::<8>();
                let (cy, ty) = y.as_chunks::<8>();
                let mut acc = [0.0 as T; 8];
                for (u, v) in cx.iter().zip(cy) {
                    for l in 0..8 {
                        acc[l] += u[l] * v[l];
                    }
                }
                acc.iter().sum::<T>() + tx.iter().zip(ty).map(|(u, v)| u * v).sum::<T>()
            }
            pub fn transposed(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) {
                let mut bt = vec![0.0 as T; k * n];
                for p in 0..k {
                    for j in 0..n {
                        bt[j * k + p] = b[p * n + j];
                    }
                }
                for i in 0..m {
                    let ar = &a[i * k..(i + 1) * k];
                    for j in 0..n {
                        c[i * n + j] = dot8(ar, &bt[j * k..(j + 1) * k]);
                    }
                }
            }
            // (4) cache blocking with TxT tiles (tile size is a const generic so the sweep can monomorphise it)
            pub fn blocked<const TILE: usize>(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) {
                c[..m * n].fill(0.0);
                for ii in (0..m).step_by(TILE) {
                    for pp in (0..k).step_by(TILE) {
                        for jj in (0..n).step_by(TILE) {
                            let (ie, pe, je) = ((ii + TILE).min(m), (pp + TILE).min(k), (jj + TILE).min(n));
                            for i in ii..ie {
                                let crow = &mut c[i * n + jj..i * n + je];
                                for p in pp..pe {
                                    let aip = a[i * k + p];
                                    for (cv, &bv) in crow.iter_mut().zip(&b[p * n + jj..p * n + je]) {
                                        *cv += aip * bv;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            /// Compute (assign) every C element outside the `rows x cols` region the micro-kernels covered.
            pub fn edges(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize, rows: usize, cols: usize) {
                for i in 0..m {
                    for j in 0..n {
                        if i >= rows || j >= cols {
                            let mut s: T = 0.0;
                            for p in 0..k {
                                s += a[i * k + p] * b[p * n + j];
                            }
                            c[i * n + j] = s;
                        }
                    }
                }
            }
            // (5)+(6) register-blocked micro-kernel: 4 x WD accumulators live in locals across the k loop.
            // WD = 4 is the textbook 4x4; WD = W lets LLVM turn the inner `for s in 0..WD` into whole-vector FMAs.
            pub fn micro<const WD: usize>(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) {
                c[..m * n].fill(0.0);
                let (mr, nc) = (m / 4 * 4, n / WD * WD);
                for pp in (0..k).step_by(KC) {
                    let pe = (pp + KC).min(k);
                    for i in (0..mr).step_by(4) {
                        for j in (0..nc).step_by(WD) {
                            let mut acc = [[0.0 as T; WD]; 4];
                            for p in pp..pe {
                                let av = [a[i * k + p], a[(i + 1) * k + p], a[(i + 2) * k + p], a[(i + 3) * k + p]];
                                let bv: &[T; WD] = b[p * n + j..p * n + j + WD].try_into().unwrap();
                                for r in 0..4 {
                                    for s in 0..WD {
                                        acc[r][s] += av[r] * bv[s];
                                    }
                                }
                            }
                            for r in 0..4 {
                                for s in 0..WD {
                                    c[(i + r) * n + j + s] += acc[r][s];
                                }
                            }
                        }
                    }
                }
                edges(a, b, c, m, k, n, mr, nc);
            }
            pub fn reg4x4(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) { micro::<4>(a, b, c, m, k, n) }
            pub fn autovec(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) { micro::<W>(a, b, c, m, k, n) }
            pub fn blocked64(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) { blocked::<64>(a, b, c, m, k, n) }

            // (8) rayon: independent blocks of 32 rows of C (a multiple of the micro-kernel's 4 or 8 rows)
            pub fn par(kern: Kern, a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) {
                const RB: usize = 32;
                if m == 0 || n == 0 {
                    return;
                }
                c[..m * n].par_chunks_mut(RB * n).enumerate().for_each(|(bi, cb)| {
                    let r0 = bi * RB;
                    let rows = cb.len() / n;
                    kern(&a[r0 * k..(r0 + rows) * k], b, cb, rows, k, n);
                });
            }
            pub fn par_autovec(a: &[T], b: &[T], c: &mut [T], m: usize, k: usize, n: usize) { par(autovec, a, b, c, m, k, n) }

            pub fn variants() -> Vec<(&'static str, Kern)> {
                let mut v: Vec<(&'static str, Kern)> = vec![
                    ("naive ijk", naive_ijk),
                    ("ikj", ikj),
                    ("transposed B", transposed),
                    ("blocked T=64", blocked64),
                    ("reg 4x4", reg4x4),
                    ("autovec 4xW", autovec),
                ];
                v.extend($extras);
                v.push(("rayon autovec", par_autovec));
                v
            }

            fn fill_random(rng: &mut Rng, len: usize) -> Vec<T> { (0..len).map(|_| rng.unit() as T).collect() }
            fn fill_ints(rng: &mut Rng, len: usize) -> Vec<T> { (0..len).map(|_| (rng.below(7) as i32 - 3) as T).collect() }

            pub fn test(all: &[(&'static str, Kern)]) {
                let mut rng = Rng(0xC0FF_EE00_1234_5678);
                let sizes = [1usize, 3, 17, 65, 100, 129];
                let eps = <T>::EPSILON as f64;
                let mut checks = 0;
                for &m in &sizes {
                    for &k in &sizes {
                        for &n in &sizes {
                            // exact check: small integers => every partial sum is exactly representable => any order gives the same bits
                            let (ai, bi) = (fill_ints(&mut rng, m * k), fill_ints(&mut rng, k * n));
                            let mut want = vec![0.0 as T; m * n];
                            naive_ijk(&ai, &bi, &mut want, m, k, n);
                            // rigorous check: |c - ref| <= (k + 8) * eps * sum |a||b| (standard forward error bound)
                            let (ar, br) = (fill_random(&mut rng, m * k), fill_random(&mut rng, k * n));
                            let mut want_r = vec![0.0 as T; m * n];
                            naive_ijk(&ar, &br, &mut want_r, m, k, n);
                            let mut bound = vec![0.0f64; m * n];
                            for i in 0..m {
                                for p in 0..k {
                                    for j in 0..n {
                                        bound[i * n + j] += (ar[i * k + p] as f64 * br[p * n + j] as f64).abs();
                                    }
                                }
                            }
                            for (name, f) in all {
                                let mut got = vec![7.0 as T; m * n]; // garbage: kernels must overwrite C
                                f(&ai, &bi, &mut got, m, k, n);
                                assert!(got == want, "{name}: integer matrices differ for {m}x{k}x{n}");
                                f(&ar, &br, &mut got, m, k, n);
                                for idx in 0..m * n {
                                    let err = (got[idx] as f64 - want_r[idx] as f64).abs();
                                    assert!(err <= (k as f64 + 8.0) * eps * bound[idx] + 1e-30, "{name}: {m}x{k}x{n} at {idx}: err {err:e}");
                                }
                                checks += 2;
                            }
                        }
                    }
                }
                // identity and zero
                for &s in &sizes {
                    let id: Vec<T> = (0..s * s).map(|x| if x / s == x % s { 1.0 } else { 0.0 }).collect();
                    let zero = vec![0.0 as T; s * s];
                    let b = fill_random(&mut rng, s * s);
                    for (name, f) in all {
                        let mut c = vec![9.0 as T; s * s];
                        f(&id, &b, &mut c, s, s, s);
                        assert!(c == b, "{name}: I*B != B at {s}");
                        f(&b, &id, &mut c, s, s, s);
                        assert!(c == b, "{name}: B*I != B at {s}");
                        f(&zero, &b, &mut c, s, s, s);
                        assert!(c.iter().all(|&x| x == 0.0), "{name}: 0*B != 0 at {s}");
                        checks += 3;
                    }
                }
                // every tile size of the sweep is correct on a size that is not a multiple of any of them
                let (a, b) = (fill_ints(&mut rng, 100 * 100), fill_ints(&mut rng, 100 * 100));
                let mut want = vec![0.0 as T; 100 * 100];
                naive_ijk(&a, &b, &mut want, 100, 100, 100);
                for (name, f) in tile_sweep() {
                    let mut got = vec![0.0 as T; 100 * 100];
                    f(&a, &b, &mut got, 100, 100, 100);
                    assert!(got == want, "{name}");
                    checks += 1;
                }
                println!("{:<4} {} variants x sizes {{1,3,17,65,100,129}}^3 + identity/zero + tile sweep: {checks} checks passed", stringify!($m), all.len());
            }

            pub fn tile_sweep() -> Vec<(&'static str, Kern)> {
                vec![("T=8", blocked::<8>), ("T=16", blocked::<16>), ("T=32", blocked::<32>), ("T=48", blocked::<48>), ("T=64", blocked::<64>), ("T=96", blocked::<96>), ("T=128", blocked::<128>), ("T=256", blocked::<256>)]
            }

            /// Time `f` on an N x N problem: best of `reps`, but a single run when the first one is already slow.
            fn time_one(f: Kern, a: &[T], b: &[T], c: &mut [T], n: usize) -> f64 {
                let mut best = f64::MAX;
                for r in 0..3 {
                    let t = Instant::now();
                    f(black_box(a), black_box(b), black_box(c), n, n, n);
                    best = best.min(t.elapsed().as_secs_f64());
                    if best > 0.4 || r == 1 && best > 0.15 {
                        break;
                    }
                }
                best
            }

            pub fn bench(all: &[(&'static str, Kern)]) {
                let mut rng = Rng(99);
                let mut last: Vec<Option<f64>> = vec![None; all.len()];
                let mut prev_n = 0usize;
                for &n in &[256usize, 512, 1024] {
                    let (a, b) = (fill_random(&mut rng, n * n), fill_random(&mut rng, n * n));
                    let mut c = vec![0.0 as T; n * n];
                    let mut want = vec![0.0 as T; n * n];
                    let flops = 2.0 * (n as f64).powi(3);
                    let ai = n as f64 / (3.0 * std::mem::size_of::<T>() as f64 / 2.0);
                    println!("\n{} N={n}: {:.2} GFLOP per multiply, ideal arithmetic intensity {:.0} flop/byte", stringify!($m), flops / 1e9, ai);
                    let mut have_ref = false;
                    for (idx, (name, f)) in all.iter().enumerate() {
                        if let Some(prev) = last[idx] {
                            let est = prev * (n as f64 / prev_n as f64).powi(3);
                            if est > 3.0 {
                                println!("  {name:<16} skipped (estimated {est:.1} s)");
                                continue;
                            }
                        }
                        let secs = time_one(*f, &a, &b, &mut c, n);
                        last[idx] = Some(secs);
                        // correctness at full size against the first variant that ran (naive or ikj)
                        if !have_ref {
                            want.copy_from_slice(&c);
                            have_ref = true;
                        } else {
                            let worst = c.iter().zip(&want).map(|(x, y)| (x - y).abs() as f64).fold(0.0, f64::max);
                            assert!(worst < 1e-3 * (n as f64).sqrt() * if std::mem::size_of::<T>() == 4 { 1.0 } else { 1e-6 }, "{name} at N={n}: max abs diff {worst:e}");
                        }
                        println!("  {name:<16} {:>8.2} ms {:>8.1} GFLOPS", secs * 1e3, flops / secs / 1e9);
                    }
                    prev_n = n;
                }
            }
        }
    };
}

matmul_impl!(f32m, f32, 16, f32_extras());
matmul_impl!(f64m, f64, 8, Vec::<(&'static str, Kern)>::new());

type F32Kern = fn(&[f32], &[f32], &mut [f32], usize, usize, usize);

// ------------------------------------------------------------------------------------------------ explicit SIMD (f32)
#[cfg(target_arch = "aarch64")]
mod simd32 {
    use std::arch::aarch64::*;
    const KC: usize = 256;

    /// 8 rows x 8 columns of C in 16 NEON accumulators; per k step: 2 vector loads of B, 8 broadcasts of A, 16 FMAs.
    #[target_feature(enable = "neon")]
    fn kernel(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
        assert!(a.len() >= m * k && b.len() >= k * n && c.len() >= m * n);
        c[..m * n].fill(0.0);
        let (mr, nc) = (m / 8 * 8, n / 8 * 8);
        for pp in (0..k).step_by(KC) {
            let pe = (pp + KC).min(k);
            for i in (0..mr).step_by(8) {
                for j in (0..nc).step_by(8) {
                    let mut acc = [[vdupq_n_f32(0.0); 2]; 8];
                    for p in pp..pe {
                        // SAFETY: p < k and j + 8 <= nc <= n, so b[p*n+j .. p*n+j+8] is inside b (len >= k*n); unaligned loads are fine.
                        let (b0, b1) = unsafe {
                            (
                                vld1q_f32(b.as_ptr().add(p * n + j)),
                                vld1q_f32(b.as_ptr().add(p * n + j + 4)),
                            )
                        };
                        for r in 0..8 {
                            // SAFETY: i + r < mr <= m and p < k, so (i+r)*k + p < m*k <= a.len().
                            let av = vdupq_n_f32(unsafe { *a.get_unchecked((i + r) * k + p) });
                            acc[r][0] = vfmaq_f32(acc[r][0], av, b0);
                            acc[r][1] = vfmaq_f32(acc[r][1], av, b1);
                        }
                    }
                    for r in 0..8 {
                        for h in 0..2 {
                            // SAFETY: row i + r < m and columns j + 4h .. +4 <= n, so the 4 floats are inside c (len >= m*n).
                            unsafe {
                                let ptr = c.as_mut_ptr().add((i + r) * n + j + 4 * h);
                                vst1q_f32(ptr, vaddq_f32(vld1q_f32(ptr), acc[r][h]));
                            }
                        }
                    }
                }
            }
        }
        super::f32m::edges(a, b, c, m, k, n, mr, nc);
    }
    pub fn neon8x8(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
        // SAFETY: NEON is mandatory on AArch64, so the target_feature precondition always holds.
        unsafe { kernel(a, b, c, m, k, n) }
    }
}

#[cfg(target_arch = "x86_64")]
mod simd32 {
    use std::arch::x86_64::*;
    const KC: usize = 256;

    /// 4 rows x 16 columns in 8 YMM accumulators (AVX2 + FMA). Compiled on x86_64 only; not executed on the Apple Silicon dev machine.
    #[target_feature(enable = "avx2,fma")]
    fn kernel(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
        assert!(a.len() >= m * k && b.len() >= k * n && c.len() >= m * n);
        c[..m * n].fill(0.0);
        let (mr, nc) = (m / 4 * 4, n / 16 * 16);
        for pp in (0..k).step_by(KC) {
            let pe = (pp + KC).min(k);
            for i in (0..mr).step_by(4) {
                for j in (0..nc).step_by(16) {
                    let mut acc = [[_mm256_setzero_ps(); 2]; 4];
                    for p in pp..pe {
                        // SAFETY: p < k and j + 16 <= nc <= n, so both 8-float loads are inside b; unaligned loads are fine.
                        let (b0, b1) = unsafe {
                            (
                                _mm256_loadu_ps(b.as_ptr().add(p * n + j)),
                                _mm256_loadu_ps(b.as_ptr().add(p * n + j + 8)),
                            )
                        };
                        for r in 0..4 {
                            // SAFETY: i + r < m and p < k, so the index is below m*k <= a.len().
                            let av = _mm256_set1_ps(unsafe { *a.get_unchecked((i + r) * k + p) });
                            acc[r][0] = _mm256_fmadd_ps(av, b0, acc[r][0]);
                            acc[r][1] = _mm256_fmadd_ps(av, b1, acc[r][1]);
                        }
                    }
                    for r in 0..4 {
                        for h in 0..2 {
                            // SAFETY: row i + r < m, columns j + 8h .. +8 <= n: inside c (len >= m*n).
                            unsafe {
                                let ptr = c.as_mut_ptr().add((i + r) * n + j + 8 * h);
                                _mm256_storeu_ps(
                                    ptr,
                                    _mm256_add_ps(_mm256_loadu_ps(ptr), acc[r][h]),
                                );
                            }
                        }
                    }
                }
            }
        }
        super::f32m::edges(a, b, c, m, k, n, mr, nc);
    }
    pub fn avx2_4x16(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // SAFETY: the CPU features required by `kernel` were just detected at run time.
            unsafe { kernel(a, b, c, m, k, n) }
        } else {
            super::f32m::autovec(a, b, c, m, k, n)
        }
    }
}

fn simd_kernel() -> Option<(&'static str, F32Kern)> {
    #[cfg(target_arch = "aarch64")]
    return Some(("neon 8x8 FMA", simd32::neon8x8));
    #[cfg(target_arch = "x86_64")]
    return Some(("avx2 4x16 FMA", simd32::avx2_4x16));
    #[allow(unreachable_code)]
    None
}

fn par_simd(a: &[f32], b: &[f32], c: &mut [f32], m: usize, k: usize, n: usize) {
    f32m::par(
        simd_kernel().map_or(f32m::autovec as F32Kern, |x| x.1),
        a,
        b,
        c,
        m,
        k,
        n,
    )
}

fn f32_extras() -> Vec<(&'static str, F32Kern)> {
    let mut v: Vec<(&'static str, F32Kern)> = Vec::new();
    if let Some(s) = simd_kernel() {
        v.push(s);
        v.push(("rayon simd", par_simd));
    }
    v
}

fn main() {
    println!(
        "rayon threads: {}   arch: {}",
        rayon::current_num_threads(),
        std::env::consts::ARCH
    );
    let v32 = f32m::variants();
    let v64 = f64m::variants();
    f32m::test(&v32);
    f64m::test(&v64);

    println!("\n== tile-size sweep, f32 N=512 (blocked ikj; best tile depends on L1/L2 size) ==");
    let n = 512;
    let mut rng = Rng(5);
    let a: Vec<f32> = (0..n * n).map(|_| rng.unit() as f32).collect();
    let b: Vec<f32> = (0..n * n).map(|_| rng.unit() as f32).collect();
    let mut c = vec![0f32; n * n];
    let mut reference = vec![0f32; n * n];
    f32m::autovec(&a, &b, &mut reference, n, n, n);
    for (name, f) in f32m::tile_sweep() {
        let mut best = f64::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            f(black_box(&a), black_box(&b), black_box(&mut c), n, n, n);
            best = best.min(t.elapsed().as_secs_f64());
        }
        let worst = c
            .iter()
            .zip(&reference)
            .map(|(x, y)| (x - y).abs())
            .fold(0f32, f32::max);
        assert!(worst < 1e-3, "tile {name}: {worst}");
        println!(
            "  {name:<7} {:>8.2} ms {:>7.1} GFLOPS",
            best * 1e3,
            2.0 * (n as f64).powi(3) / best / 1e9
        );
    }

    println!("\n== optimisation ladder ==");
    f32m::bench(&v32);
    f64m::bench(&v64);
    println!(
        "\nA variant is skipped when its extrapolated time exceeds 3 s (none were on this machine). Which paths ran: SIMD kernel = {}; the x86_64 AVX2 kernel is only type-checked.",
        simd_kernel().map_or("none", |s| s.0)
    );
}
