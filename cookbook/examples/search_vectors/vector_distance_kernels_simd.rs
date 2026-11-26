//! RECIPE 870 (vectors): distance kernels for vector search: scalar, multi-accumulator, autovectorised, AVX2/FMA and NEON, f16/bf16/i8 storage.
//! LEVEL: intermediate
//! Use in: the inner loop of every vector index (brute force, IVF, HNSW, PQ re-ranking); all of them spend 60-90% of their time here.   Run: cargo run --release --example vector_distance_kernels_simd
#![allow(dead_code)]
//!
//! WHAT: the five kernels a vector database needs (squared L2, dot / inner product, cosine, Hamming over bit codes, and the
//! "negative inner product" distance) each written several ways, all checked against an f64 scalar ORACLE:
//!   1. `scalar`    one accumulator. The loop-carried dependency on `acc` caps it at one add per FP latency (~4 cycles).
//!   2. `unroll4`   four independent accumulators written by hand. Breaks the dependency chain, so the CPU overlaps adds.
//!   3. `chunked`   `chunks_exact(8)` with an `[f32; 8]` accumulator. LLVM turns the array into one SIMD register (no
//!                  `unsafe`, no intrinsics). Floats are not associative so LLVM will NOT reorder a plain `sum()`; writing the
//!                  lanes yourself is the permission slip.
//!   4. `avx2_fma`  x86_64 intrinsics (two 8-lane FMA accumulators), chosen at run time with `is_x86_feature_detected!`.
//!   5. `neon`      aarch64 intrinsics (four 4-lane FMA accumulators). NEON is baseline on aarch64 so no detection is needed.
//! Only variants that exist on the machine are built (cfg-gated); the scalar fallback always exists. On this machine you see
//! whichever of avx2/neon applies; the other branch is compiled out, so it is written to be obviously correct, not benchmarked here.
//!
//! STORAGE FORMATS (memory bandwidth, not FLOPs, is the real limit once vectors leave the cache):
//!   * f16 (IEEE half) and bf16 (top 16 bits of f32): hand-written converters, verified EXHAUSTIVELY over all 65536 f16 bit
//!     patterns and against a nearest-value oracle. Kernels widen to f32 per element and accumulate in f32.
//!   * i8 with a per-vector scale: integer dot with i32 accumulators is EXACT (no rounding at all), then multiply by the scales.
//! THE NORM TRICK: |a-b|^2 = |a|^2 + |b|^2 - 2 a.b. If norms are precomputed (stored next to each vector), one dot product
//! gives L2 and a matrix-matrix product (GEMM) gives all pairwise distances. The price is CANCELLATION when a is close to b
//! (the answer is tiny, the terms are big). We measure that error and clamp at 0.
//! BATCHING: one query against N contiguous vectors. Row-major means one horizontal sum per row; the BLOCKED (8 rows per block,
//! dimension-major inside a block) layout puts row j in lane j, so a block of 8 distances needs NO horizontal reduction.
//! EARLY ABANDON: when you only want results below a threshold, stop summing once the partial sum already exceeds it
//! (partial sums of squares only grow). Checked every 16 dims so the branch is cheap.
//! ACCURACY: f32 sums depend on order. A sequential sum has worst-case error ~ n*eps*sum|x|; lanes/pairwise ~ log n; Kahan
//! compensation ~ eps independent of n. Tolerances below are the proven bounds, not magic numbers.
//!
//! VERIFIED: 2000 random vector pairs with dimensions that are NOT multiples of the lane width (1,2,3,7,9,...,1000), zero vectors,
//! one-hot, antipodal (cancellation), huge (1e15) and tiny (1e-15) magnitudes, mixed huge x tiny. Timings are printed, never asserted.
//! COMMON USES: faiss `fvec_L2sqr`/`fvec_inner_product` (AVX2/AVX-512/NEON paths), hnswlib `L2SqrSIMD16ExtAVX`, ScaNN and
//! USearch (f16/i8 storage), pgvector (`vector_l2_squared_distance` plus AVX/FMA multiversioning), Lucene `VectorUtil` (Panama Vector API).
//! Read: https://en.algorithmica.org/hpc/simd/ ; https://doc.rust-lang.org/core/arch/index.html ;
//! https://github.com/facebookresearch/faiss/wiki/Fast-accumulation-of-PQ-and-AQ-codes-(FastScan) ;
//! Higham, "The accuracy of floating point summation" (SIAM J. Sci. Comput. 1993; cited by title, URL not checked) ;
//! https://arxiv.org/abs/1603.09320 (HNSW: distance computations dominate search cost)
//!
//! TRY NEXT:
//!   - [S] Predict, then measure: change `LANES` in `dot_chunked` from 8 to 4 and to 16. Which wins on `dot chunked` in the bench table, and why does the answer depend on whether the data fits in cache?
//!   - [S] Add `dot_unroll8` and compare with `dot_unroll4`. At what accumulator count does GFLOP/s stop improving, and what does that say about FP add latency x throughput on your CPU?
//!   - [M] Break it on purpose: make `l2_via_norms` skip the `.max(0.0)` clamp and find which test in `test_norm_trick_cancellation` first sees a negative distance. Then store `|b|^2` in f64 and re-measure the error.
//!   - [M] Port: write `dot_i8` with `std::arch` (NEON `vdotq_s32` needs the `dotprod` feature, AVX2 `_mm256_maddubs_epi16`) behind runtime detection and keep `assert_eq!` against the exact i64 oracle.
//!   - [A] PROJECT: put the blocked layout and `l2_early_abandon` inside the `ivf_flat` list scan (see `ivf_flat`), then add an f16 rescoring pass like `scalar_binary_quantization_rescoring`; report recall@10 and distance-bytes read per query.
use std::hint::black_box;
use std::sync::OnceLock;
use std::time::Instant;

// ---------- shared mini-harness (copied so the file stands alone) ----------
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    /// Box-Muller standard normal.
    fn gauss(&mut self) -> f32 {
        let (u, v) = (self.unit(), self.unit());
        ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
    }
}

// ---------- scalar kernels (reference f32 implementations) ----------
fn dot_scalar(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0f32;
    for i in 0..a.len() {
        acc += a[i] * b[i];
    }
    acc
}
fn l2_scalar(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0f32;
    for i in 0..a.len() {
        let d = a[i] - b[i];
        acc += d * d;
    }
    acc
}

// ---------- 4 independent accumulators ----------
fn dot_unroll4(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len();
    let (mut s0, mut s1, mut s2, mut s3) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut i = 0;
    while i + 4 <= n {
        s0 += a[i] * b[i];
        s1 += a[i + 1] * b[i + 1];
        s2 += a[i + 2] * b[i + 2];
        s3 += a[i + 3] * b[i + 3];
        i += 4;
    }
    while i < n {
        s0 += a[i] * b[i];
        i += 1;
    }
    (s0 + s1) + (s2 + s3)
}
fn l2_unroll4(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len();
    let (mut s0, mut s1, mut s2, mut s3) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut i = 0;
    while i + 4 <= n {
        let (d0, d1, d2, d3) = (
            a[i] - b[i],
            a[i + 1] - b[i + 1],
            a[i + 2] - b[i + 2],
            a[i + 3] - b[i + 3],
        );
        s0 += d0 * d0;
        s1 += d1 * d1;
        s2 += d2 * d2;
        s3 += d3 * d3;
        i += 4;
    }
    while i < n {
        let d = a[i] - b[i];
        s0 += d * d;
        i += 1;
    }
    (s0 + s1) + (s2 + s3)
}

// ---------- chunks_exact + array lanes: autovectorisation-friendly, safe ----------
const LANES: usize = 8;
fn dot_chunked(a: &[f32], b: &[f32]) -> f32 {
    let mut lanes = [0.0f32; LANES];
    let (ca, cb) = (a.chunks_exact(LANES), b.chunks_exact(LANES));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..LANES {
            lanes[l] += x[l] * y[l];
        }
    }
    let mut tail = 0.0f32;
    for (x, y) in ra.iter().zip(rb) {
        tail += x * y;
    }
    lanes.iter().sum::<f32>() + tail
}
fn l2_chunked(a: &[f32], b: &[f32]) -> f32 {
    let mut lanes = [0.0f32; LANES];
    let (ca, cb) = (a.chunks_exact(LANES), b.chunks_exact(LANES));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..LANES {
            let d = x[l] - y[l];
            lanes[l] += d * d;
        }
    }
    let mut tail = 0.0f32;
    for (x, y) in ra.iter().zip(rb) {
        let d = x - y;
        tail += d * d;
    }
    lanes.iter().sum::<f32>() + tail
}

// ---------- x86_64: AVX2 + FMA ----------
// The whole body of each kernel is one unsafe region (SAFETY comments inside); per-call unsafe blocks would only add noise.
#[cfg(target_arch = "x86_64")]
#[allow(unsafe_op_in_unsafe_fn)]
mod x86 {
    use std::arch::x86_64::*;

    #[target_feature(enable = "avx2,fma")]
    unsafe fn hsum(v: __m256) -> f32 {
        let s = _mm_add_ps(_mm256_castps256_ps128(v), _mm256_extractf128_ps(v, 1));
        let s = _mm_add_ps(s, _mm_movehl_ps(s, s));
        let s = _mm_add_ss(s, _mm_shuffle_ps(s, s, 1));
        _mm_cvtss_f32(s)
    }
    #[target_feature(enable = "avx2,fma")]
    unsafe fn dot_impl(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len();
        let (pa, pb) = (a.as_ptr(), b.as_ptr());
        let (mut s0, mut s1) = (_mm256_setzero_ps(), _mm256_setzero_ps());
        let mut i = 0;
        // SAFETY: every load reads 8 floats at offset i with i + 16 <= n (or i + 8 <= n), inside both slices.
        while i + 16 <= n {
            s0 = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i)), _mm256_loadu_ps(pb.add(i)), s0);
            s1 = _mm256_fmadd_ps(
                _mm256_loadu_ps(pa.add(i + 8)),
                _mm256_loadu_ps(pb.add(i + 8)),
                s1,
            );
            i += 16;
        }
        if i + 8 <= n {
            s0 = _mm256_fmadd_ps(_mm256_loadu_ps(pa.add(i)), _mm256_loadu_ps(pb.add(i)), s0);
            i += 8;
        }
        let mut r = hsum(_mm256_add_ps(s0, s1));
        while i < n {
            r += a[i] * b[i];
            i += 1;
        }
        r
    }
    #[target_feature(enable = "avx2,fma")]
    unsafe fn l2_impl(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len();
        let (pa, pb) = (a.as_ptr(), b.as_ptr());
        let (mut s0, mut s1) = (_mm256_setzero_ps(), _mm256_setzero_ps());
        let mut i = 0;
        // SAFETY: same bounds argument as dot_impl.
        while i + 16 <= n {
            let d0 = _mm256_sub_ps(_mm256_loadu_ps(pa.add(i)), _mm256_loadu_ps(pb.add(i)));
            let d1 = _mm256_sub_ps(
                _mm256_loadu_ps(pa.add(i + 8)),
                _mm256_loadu_ps(pb.add(i + 8)),
            );
            s0 = _mm256_fmadd_ps(d0, d0, s0);
            s1 = _mm256_fmadd_ps(d1, d1, s1);
            i += 16;
        }
        if i + 8 <= n {
            let d = _mm256_sub_ps(_mm256_loadu_ps(pa.add(i)), _mm256_loadu_ps(pb.add(i)));
            s0 = _mm256_fmadd_ps(d, d, s0);
            i += 8;
        }
        let mut r = hsum(_mm256_add_ps(s0, s1));
        while i < n {
            let d = a[i] - b[i];
            r += d * d;
            i += 1;
        }
        r
    }
    pub fn available() -> bool {
        is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma")
    }
    // Safe wrappers re-check the CPU (a cached atomic load), so they are sound for any caller.
    pub fn dot(a: &[f32], b: &[f32]) -> f32 {
        assert!(available() && a.len() == b.len());
        // SAFETY: AVX2+FMA verified just above; equal lengths asserted.
        unsafe { dot_impl(a, b) }
    }
    pub fn l2(a: &[f32], b: &[f32]) -> f32 {
        assert!(available() && a.len() == b.len());
        // SAFETY: as above.
        unsafe { l2_impl(a, b) }
    }
    #[target_feature(enable = "popcnt")]
    unsafe fn ham_impl(a: &[u8], b: &[u8]) -> u32 {
        let mut total = 0u32;
        let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
        let (ra, rb) = (ca.remainder(), cb.remainder());
        for (x, y) in ca.zip(cb) {
            let w = u64::from_le_bytes(x.try_into().unwrap())
                ^ u64::from_le_bytes(y.try_into().unwrap());
            total += w.count_ones(); // compiles to the POPCNT instruction under this target_feature
        }
        for (x, y) in ra.iter().zip(rb) {
            total += (x ^ y).count_ones();
        }
        total
    }
    pub fn hamming(a: &[u8], b: &[u8]) -> u32 {
        assert!(is_x86_feature_detected!("popcnt") && a.len() == b.len());
        // SAFETY: popcnt verified above.
        unsafe { ham_impl(a, b) }
    }
}

// ---------- aarch64: NEON ----------
#[cfg(target_arch = "aarch64")]
#[allow(unsafe_op_in_unsafe_fn)]
mod arm {
    use std::arch::aarch64::*;

    #[target_feature(enable = "neon")]
    unsafe fn dot_impl(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len();
        let (pa, pb) = (a.as_ptr(), b.as_ptr());
        let mut s = [vdupq_n_f32(0.0); 4];
        let mut i = 0;
        // SAFETY: each iteration loads 4 floats at i, i+4, i+8, i+12 with i + 16 <= n, inside both slices.
        while i + 16 <= n {
            for j in 0..4 {
                s[j] = vfmaq_f32(
                    s[j],
                    vld1q_f32(pa.add(i + 4 * j)),
                    vld1q_f32(pb.add(i + 4 * j)),
                );
            }
            i += 16;
        }
        while i + 4 <= n {
            s[0] = vfmaq_f32(s[0], vld1q_f32(pa.add(i)), vld1q_f32(pb.add(i)));
            i += 4;
        }
        let mut r = vaddvq_f32(vaddq_f32(vaddq_f32(s[0], s[1]), vaddq_f32(s[2], s[3])));
        while i < n {
            r += a[i] * b[i];
            i += 1;
        }
        r
    }
    #[target_feature(enable = "neon")]
    unsafe fn l2_impl(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len();
        let (pa, pb) = (a.as_ptr(), b.as_ptr());
        let mut s = [vdupq_n_f32(0.0); 4];
        let mut i = 0;
        // SAFETY: same bounds argument as dot_impl.
        while i + 16 <= n {
            for j in 0..4 {
                let d = vsubq_f32(vld1q_f32(pa.add(i + 4 * j)), vld1q_f32(pb.add(i + 4 * j)));
                s[j] = vfmaq_f32(s[j], d, d);
            }
            i += 16;
        }
        while i + 4 <= n {
            let d = vsubq_f32(vld1q_f32(pa.add(i)), vld1q_f32(pb.add(i)));
            s[0] = vfmaq_f32(s[0], d, d);
            i += 4;
        }
        let mut r = vaddvq_f32(vaddq_f32(vaddq_f32(s[0], s[1]), vaddq_f32(s[2], s[3])));
        while i < n {
            let d = a[i] - b[i];
            r += d * d;
            i += 1;
        }
        r
    }
    #[target_feature(enable = "neon")]
    unsafe fn ham_impl(a: &[u8], b: &[u8]) -> u32 {
        let n = a.len();
        let (pa, pb) = (a.as_ptr(), b.as_ptr());
        let mut total = 0u32;
        let mut i = 0;
        // SAFETY: loads 16 bytes at i with i + 16 <= n. vcntq_u8 = per-byte popcount, vaddvq_u8 sums 16 bytes (max 128, fits u8).
        while i + 16 <= n {
            let x = veorq_u8(vld1q_u8(pa.add(i)), vld1q_u8(pb.add(i)));
            total += vaddvq_u8(vcntq_u8(x)) as u32;
            i += 16;
        }
        while i < n {
            total += (a[i] ^ b[i]).count_ones();
            i += 1;
        }
        total
    }
    pub fn dot(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        // SAFETY: NEON is part of the aarch64 baseline; equal lengths asserted.
        unsafe { dot_impl(a, b) }
    }
    pub fn l2(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        // SAFETY: as above.
        unsafe { l2_impl(a, b) }
    }
    pub fn hamming(a: &[u8], b: &[u8]) -> u32 {
        assert_eq!(a.len(), b.len());
        // SAFETY: as above.
        unsafe { ham_impl(a, b) }
    }
}

// ---------- dispatch: a table of plain fn pointers, picked once ----------
type F32Kernel = fn(&[f32], &[f32]) -> f32;
type HamKernel = fn(&[u8], &[u8]) -> u32;
#[derive(Clone, Copy)]
struct Kernels {
    name: &'static str,
    dot: F32Kernel,
    l2: F32Kernel,
}
fn variants() -> Vec<Kernels> {
    let mut v = vec![
        Kernels {
            name: "scalar",
            dot: dot_scalar,
            l2: l2_scalar,
        },
        Kernels {
            name: "unroll4",
            dot: dot_unroll4,
            l2: l2_unroll4,
        },
        Kernels {
            name: "chunked",
            dot: dot_chunked,
            l2: l2_chunked,
        },
    ];
    #[cfg(target_arch = "x86_64")]
    if x86::available() {
        v.push(Kernels {
            name: "avx2_fma",
            dot: x86::dot,
            l2: x86::l2,
        });
    }
    #[cfg(target_arch = "aarch64")]
    v.push(Kernels {
        name: "neon",
        dot: arm::dot,
        l2: arm::l2,
    });
    v
}
fn best() -> &'static Kernels {
    static BEST: OnceLock<Kernels> = OnceLock::new();
    BEST.get_or_init(|| *variants().last().unwrap())
}
fn hamming_variants() -> Vec<(&'static str, HamKernel)> {
    let mut v: Vec<(&'static str, HamKernel)> =
        vec![("scalar", hamming_scalar), ("u64_words", hamming_u64)];
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("popcnt") {
        v.push(("popcnt", x86::hamming));
    }
    #[cfg(target_arch = "aarch64")]
    v.push(("neon_cnt", arm::hamming));
    v
}
fn hamming_scalar(a: &[u8], b: &[u8]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}
fn hamming_u64(a: &[u8], b: &[u8]) -> u32 {
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    let mut t = 0u32;
    for (x, y) in ca.zip(cb) {
        t += (u64::from_le_bytes(x.try_into().unwrap())
            ^ u64::from_le_bytes(y.try_into().unwrap()))
        .count_ones();
    }
    t + hamming_scalar(ra, rb)
}

// ---------- derived distances ----------
/// Negative inner product: smaller is better, so every metric is "minimise a distance".
fn neg_ip(k: &Kernels, a: &[f32], b: &[f32]) -> f32 {
    -(k.dot)(a, b)
}
/// Cosine distance; a zero vector has no direction, so by convention distance 1 (similarity 0) instead of NaN.
fn cosine(k: &Kernels, a: &[f32], b: &[f32]) -> f32 {
    let (d, na, nb) = ((k.dot)(a, b), (k.dot)(a, a), (k.dot)(b, b));
    if na == 0.0 || nb == 0.0 {
        1.0
    } else {
        1.0 - d / (na.sqrt() * nb.sqrt())
    }
}
/// L2 through the dot product with precomputed squared norms; clamped because cancellation can make it slightly negative.
fn l2_via_norms(k: &Kernels, a: &[f32], na: f32, b: &[f32], nb: f32) -> f32 {
    (na + nb - 2.0 * (k.dot)(a, b)).max(0.0)
}

// ---------- f64 oracles ----------
fn dot_f64(a: &[f32], b: &[f32]) -> (f64, f64) {
    let mut s = 0.0f64;
    let mut abs = 0.0f64;
    for (x, y) in a.iter().zip(b) {
        let p = *x as f64 * *y as f64;
        s += p;
        abs += p.abs();
    }
    (s, abs)
}
fn l2_f64(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as f64 - *y as f64).powi(2))
        .sum()
}
fn cos_f64(a: &[f32], b: &[f32]) -> f64 {
    let (d, _) = dot_f64(a, b);
    let (na, nb) = (dot_f64(a, a).0, dot_f64(b, b).0);
    if na == 0.0 || nb == 0.0 {
        1.0
    } else {
        1.0 - d / (na.sqrt() * nb.sqrt())
    }
}

// ---------- half precision and int8 storage ----------
fn f32_to_f16(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32;
    let man = b & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if man != 0 { 0x200 } else { 0 }; // inf or quiet NaN
    }
    let e = exp - 127 + 15; // rebias
    if e >= 0x1f {
        return sign | 0x7c00; // overflow to infinity
    }
    if e <= 0 {
        // f16 subnormal (or zero): shift the 24-bit significand right, rounding to nearest even.
        if e < -10 {
            return sign;
        }
        let m = man | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = m >> shift;
        let rem = m & ((1u32 << shift) - 1);
        let halfway = 1u32 << (shift - 1);
        let r = half + u32::from(rem > halfway || (rem == halfway && half & 1 == 1));
        return sign | r as u16;
    }
    let mut h = ((e as u32) << 10) | (man >> 13);
    let rem = man & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && h & 1 == 1) {
        h += 1; // a carry out of the mantissa correctly bumps the exponent (and may reach infinity)
    }
    sign | h as u16
}
fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as u32;
    let man = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if man == 0 {
            sign
        } else {
            let (mut e, mut m) = (113u32, man); // subnormal: normalise so the leading 1 is implicit
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | (e << 23) | ((m & 0x3ff) << 13)
        }
    } else if exp == 0x1f {
        sign | 0x7f80_0000 | (man << 13)
    } else {
        sign | ((exp + 112) << 23) | (man << 13)
    };
    f32::from_bits(bits)
}
fn f32_to_bf16(x: f32) -> u16 {
    let b = x.to_bits();
    if x.is_nan() {
        return ((b >> 16) | 0x40) as u16;
    }
    (b.wrapping_add(0x7fff + ((b >> 16) & 1)) >> 16) as u16 // round to nearest even on the dropped 16 bits
}
fn bf16_to_f32(h: u16) -> f32 {
    f32::from_bits((h as u32) << 16)
}
fn dot_f16(a: &[u16], b: &[u16]) -> f32 {
    let mut lanes = [0.0f32; LANES];
    let (ca, cb) = (a.chunks_exact(LANES), b.chunks_exact(LANES));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..LANES {
            lanes[l] += f16_to_f32(x[l]) * f16_to_f32(y[l]); // widen per element: the memory read is half the size
        }
    }
    let tail: f32 = ra
        .iter()
        .zip(rb)
        .map(|(x, y)| f16_to_f32(*x) * f16_to_f32(*y))
        .sum();
    lanes.iter().sum::<f32>() + tail
}
fn dot_bf16(a: &[u16], b: &[u16]) -> f32 {
    let mut lanes = [0.0f32; LANES];
    let (ca, cb) = (a.chunks_exact(LANES), b.chunks_exact(LANES));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..LANES {
            lanes[l] += bf16_to_f32(x[l]) * bf16_to_f32(y[l]);
        }
    }
    let tail: f32 = ra
        .iter()
        .zip(rb)
        .map(|(x, y)| bf16_to_f32(*x) * bf16_to_f32(*y))
        .sum();
    lanes.iter().sum::<f32>() + tail
}

/// i8 vector with its dequantisation scale: x ~ scale * q.
struct QVec {
    q: Vec<i8>,
    scale: f32,
}
fn quantize_i8(x: &[f32]) -> QVec {
    let m = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let scale = if m == 0.0 { 1.0 } else { m / 127.0 };
    QVec {
        q: x.iter()
            .map(|v| (v / scale).round().clamp(-127.0, 127.0) as i8)
            .collect(),
        scale,
    }
}
/// Integer dot, i32 accumulators: exact as long as dim * 127 * 127 < 2^31 (dim < 133k).
fn dot_i8(a: &[i8], b: &[i8]) -> i32 {
    let mut lanes = [0i32; 16];
    let (ca, cb) = (a.chunks_exact(16), b.chunks_exact(16));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..16 {
            lanes[l] += x[l] as i32 * y[l] as i32; // LLVM maps this to widening multiply-add (pmaddwd / sdot-like)
        }
    }
    lanes.iter().sum::<i32>()
        + ra.iter()
            .zip(rb)
            .map(|(x, y)| *x as i32 * *y as i32)
            .sum::<i32>()
}
fn l2_i8(a: &[i8], b: &[i8]) -> i32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - *y as i32).pow(2))
        .sum()
}

// ---------- batch: one query vs a contiguous matrix ----------
fn batch_dot_rowmajor(k: &Kernels, q: &[f32], mat: &[f32], dim: usize, out: &mut [f32]) {
    for (o, row) in out.iter_mut().zip(mat.chunks_exact(dim)) {
        *o = (k.dot)(q, row);
    }
}
/// Blocks of 8 rows stored dimension-major: block[d * 8 + j] = row j, dim d. Row j lives in SIMD lane j, so
/// 8 dot products need no horizontal sum at all. Padding rows (zeros) are ignored by the caller.
struct BlockedMat {
    dim: usize,
    rows: usize,
    data: Vec<f32>,
}
impl BlockedMat {
    fn from_rows(mat: &[f32], dim: usize) -> Self {
        let rows = mat.len() / dim;
        let blocks = rows.div_ceil(8);
        let mut data = vec![0.0f32; blocks * 8 * dim];
        for r in 0..rows {
            for d in 0..dim {
                data[(r / 8) * 8 * dim + d * 8 + r % 8] = mat[r * dim + d];
            }
        }
        BlockedMat { dim, rows, data }
    }
    fn dot_all(&self, q: &[f32], out: &mut [f32]) {
        for (b, blk) in self.data.chunks_exact(8 * self.dim).enumerate() {
            let mut acc = [0.0f32; 8];
            for (d, lane) in blk.chunks_exact(8).enumerate() {
                let qd = q[d]; // one broadcast, 8 FMAs worth of work
                for j in 0..8 {
                    acc[j] += qd * lane[j];
                }
            }
            for j in 0..8 {
                if b * 8 + j < self.rows {
                    out[b * 8 + j] = acc[j];
                }
            }
        }
    }
}

// ---------- early abandon ----------
/// Squared L2 that gives up once the partial sum exceeds `bound`. Returns the partial sum when abandoned
/// (always > bound and <= the true distance), or the exact distance when it finished (always <= bound is NOT implied
/// for the very last block, so callers compare with `<= bound` themselves).
fn l2_early_abandon(a: &[f32], b: &[f32], bound: f32) -> (f32, usize) {
    let mut acc = 0.0f32;
    let mut i = 0;
    let mut dims_read = 0;
    while i < a.len() {
        let end = (i + 16).min(a.len());
        acc += l2_chunked(&a[i..end], &b[i..end]);
        dims_read = end;
        if acc > bound {
            break;
        }
        i = end;
    }
    (acc, dims_read)
}

// ---------- summation accuracy ----------
fn sum_naive(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |a, v| a + v)
}
fn sum_lanes(x: &[f32]) -> f32 {
    let mut l = [0.0f32; 16];
    let c = x.chunks_exact(16);
    let rem = c.remainder();
    for ch in c {
        for i in 0..16 {
            l[i] += ch[i];
        }
    }
    l.iter().sum::<f32>() + rem.iter().sum::<f32>()
}
fn sum_kahan(x: &[f32]) -> f32 {
    let (mut s, mut c) = (0.0f32, 0.0f32);
    for &v in x {
        let y = v - c;
        let t = s + y;
        c = (t - s) - y; // the low bits that were lost when adding y to s
        s = t;
    }
    s
}

// ---------- test data ----------
#[derive(Clone, Copy, Debug)]
enum Mag {
    Normal,
    Huge,
    Tiny,
    HugeTiny,
    Zero,
    OneHot,
    Antipodal,
}
fn make_pair(rng: &mut Rng, dim: usize, mag: Mag) -> (Vec<f32>, Vec<f32>) {
    let mut v = |s: f32| -> Vec<f32> { (0..dim).map(|_| rng.gauss() * s).collect() };
    match mag {
        Mag::Normal => (v(1.0), v(1.0)),
        Mag::Huge => (v(1e15), v(1e15)),
        Mag::Tiny => (v(1e-15), v(1e-15)),
        Mag::HugeTiny => (v(1e15), v(1e-15)),
        Mag::Zero => (vec![0.0; dim], v(1.0)),
        Mag::OneHot => {
            let mut a = vec![0.0; dim];
            let mut b = vec![0.0; dim];
            a[0] = 3.0;
            b[dim - 1] = 4.0;
            (a, b)
        }
        Mag::Antipodal => {
            let a = v(1.0);
            let b = a.iter().map(|x| -x).collect();
            (a, b)
        }
    }
}

const EPS: f64 = f32::EPSILON as f64;
fn assert_close(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got:e}, want {want:e}, tol {tol:e}"
    );
}

fn test_float_kernels() {
    let dims = [
        1usize, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 127, 128, 129, 255,
        300, 384, 513, 768, 1000,
    ];
    let mags = [
        Mag::Normal,
        Mag::Normal,
        Mag::Normal,
        Mag::Huge,
        Mag::Tiny,
        Mag::HugeTiny,
        Mag::Zero,
        Mag::OneHot,
        Mag::Antipodal,
    ];
    let mut rng = Rng(0x870_870_870);
    let ks = variants();
    let mut worst = 0.0f64;
    for _ in 0..2000 {
        let dim = dims[rng.below(dims.len())];
        let mag = mags[rng.below(mags.len())];
        let (a, b) = make_pair(&mut rng, dim, mag);
        let (d64, abs64) = dot_f64(&a, &b);
        let l64 = l2_f64(&a, &b);
        let c64 = cos_f64(&a, &b);
        let (na64, nb64) = (dot_f64(&a, &a).0, dot_f64(&b, &b).0);
        let n = dim as f64 + 8.0;
        for k in &ks {
            let d = (k.dot)(&a, &b) as f64;
            assert_close(
                &format!("{} dot {mag:?} dim {dim}", k.name),
                d,
                d64,
                n * EPS * abs64 + 1e-37,
            );
            worst = worst.max((d - d64).abs() / (n * EPS * abs64 + 1e-37));
            let l = (k.l2)(&a, &b) as f64;
            assert_close(
                &format!("{} l2 {mag:?} dim {dim}", k.name),
                l,
                l64,
                4.0 * n * EPS * l64 + 1e-37,
            );
            let c = cosine(k, &a, &b) as f64;
            assert_close(
                &format!("{} cosine {mag:?} dim {dim}", k.name),
                c,
                c64,
                8.0 * n * EPS + 1e-6,
            );
            assert_close(
                &format!("{} neg_ip", k.name),
                neg_ip(k, &a, &b) as f64,
                -d64,
                n * EPS * abs64 + 1e-37,
            );
            // norm trick: error is relative to the norms, not to the (possibly tiny) answer
            let v = l2_via_norms(k, &a, (k.dot)(&a, &a), &b, (k.dot)(&b, &b)) as f64;
            assert!(v >= 0.0);
            assert_close(
                &format!("{} l2_via_norms {mag:?} dim {dim}", k.name),
                v,
                l64,
                8.0 * n * EPS * (na64 + nb64) + 1e-37,
            );
        }
    }
    println!(
        "float kernels ({} variants): 2000 pairs OK; worst dot error = {:.3} of the proven bound",
        ks.len(),
        worst
    );
}

fn test_hamming() {
    let mut rng = Rng(0x1234_5678_9abc);
    let vs = hamming_variants();
    for _ in 0..2000 {
        let len = rng.below(200) + 1; // bytes: includes every remainder modulo 8 and 16
        let a: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
        let b: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
        let oracle: u32 = (0..len * 8)
            .filter(|&i| (a[i / 8] >> (i % 8)) & 1 != (b[i / 8] >> (i % 8)) & 1)
            .count() as u32;
        for (name, f) in &vs {
            assert_eq!(f(&a, &b), oracle, "hamming {name} len {len}");
        }
    }
    assert_eq!(hamming_scalar(&[0xff; 5], &[0x00; 5]), 40);
    println!(
        "hamming variants {:?}: 2000 random byte strings OK",
        vs.iter().map(|v| v.0).collect::<Vec<_>>()
    );
}

fn test_half_and_int8() {
    // f16: every bit pattern survives f16 -> f32 -> f16 (NaN only needs to stay NaN)
    for h in 0..=u16::MAX {
        let f = f16_to_f32(h);
        if f.is_nan() {
            assert!(f16_to_f32(f32_to_f16(f)).is_nan());
        } else {
            assert_eq!(f32_to_f16(f), h, "f16 roundtrip of {h:#06x}");
        }
    }
    // f32 -> f16 must be the NEAREST representable half (check the two neighbours).
    let mut rng = Rng(0xf16f16);
    let mut worst_rel = 0.0f64;
    for _ in 0..200_000 {
        let x = rng.gauss() * 10f32.powi(rng.below(7) as i32 - 3);
        let h = f32_to_f16(x);
        let y = f16_to_f32(h);
        for nb in [h.wrapping_add(1), h.wrapping_sub(1)] {
            let z = f16_to_f32(nb);
            if z.is_finite() && (nb ^ h) & 0x8000 == 0 {
                assert!(
                    (x - y).abs() <= (x - z).abs(),
                    "f16 rounding of {x} chose {y}, neighbour {z} is nearer"
                );
            }
        }
        if x.abs() > 6.2e-5 && x.abs() < 60000.0 {
            worst_rel = worst_rel.max(((x - y) / x).abs() as f64);
        }
    }
    assert!(
        worst_rel <= 2f64.powi(-11) * 1.0001,
        "f16 relative error {worst_rel:e} exceeds 2^-11"
    );
    assert_eq!(f16_to_f32(f32_to_f16(1e6)), f32::INFINITY);
    assert_eq!(f16_to_f32(f32_to_f16(65504.0)), 65504.0); // largest finite half
    // bf16: 8 exponent bits keep f32 range; 8 bits of precision give relative error <= 2^-8
    let mut worst_bf = 0.0f64;
    for _ in 0..200_000 {
        let x = rng.gauss() * 10f32.powi(rng.below(60) as i32 - 30);
        let y = bf16_to_f32(f32_to_bf16(x));
        worst_bf = worst_bf.max(((x - y) / x).abs() as f64);
    }
    assert!(
        worst_bf <= 2f64.powi(-8) * 1.0001,
        "bf16 relative error {worst_bf:e}"
    );
    println!(
        "f16: 65536 patterns round-trip, nearest-rounding verified, rel err {:.2e} (<= 2^-11); bf16 rel err {:.2e} (<= 2^-8)",
        worst_rel, worst_bf
    );

    // dot products on stored formats are checked against the f64 dot of the DECODED values (exact kernel check) ...
    let mut rng = Rng(0xdd);
    for _ in 0..500 {
        let dim = [1usize, 7, 8, 9, 33, 100, 129, 300, 768][rng.below(9)];
        let a: Vec<f32> = (0..dim).map(|_| rng.gauss()).collect();
        let b: Vec<f32> = (0..dim).map(|_| rng.gauss()).collect();
        let (ah, bh): (Vec<u16>, Vec<u16>) = (
            a.iter().map(|x| f32_to_f16(*x)).collect(),
            b.iter().map(|x| f32_to_f16(*x)).collect(),
        );
        let (da, db): (Vec<f32>, Vec<f32>) = (
            ah.iter().map(|h| f16_to_f32(*h)).collect(),
            bh.iter().map(|h| f16_to_f32(*h)).collect(),
        );
        let (want, abs) = dot_f64(&da, &db);
        assert_close(
            "dot_f16",
            dot_f16(&ah, &bh) as f64,
            want,
            (dim as f64 + 8.0) * EPS * abs + 1e-30,
        );
        let (ab, bb): (Vec<u16>, Vec<u16>) = (
            a.iter().map(|x| f32_to_bf16(*x)).collect(),
            b.iter().map(|x| f32_to_bf16(*x)).collect(),
        );
        let (da, db): (Vec<f32>, Vec<f32>) = (
            ab.iter().map(|h| bf16_to_f32(*h)).collect(),
            bb.iter().map(|h| bf16_to_f32(*h)).collect(),
        );
        let (want, abs) = dot_f64(&da, &db);
        assert_close(
            "dot_bf16",
            dot_bf16(&ab, &bb) as f64,
            want,
            (dim as f64 + 8.0) * EPS * abs + 1e-30,
        );
        // ... and the i8 integer dot is EXACT; the dequantised result obeys a rigorous quantisation bound.
        let (qa, qb) = (quantize_i8(&a), quantize_i8(&b));
        let exact: i64 =
            qa.q.iter()
                .zip(&qb.q)
                .map(|(x, y)| *x as i64 * *y as i64)
                .sum();
        assert_eq!(dot_i8(&qa.q, &qb.q) as i64, exact);
        let l2exact: i64 =
            qa.q.iter()
                .zip(&qb.q)
                .map(|(x, y)| (*x as i64 - *y as i64).pow(2))
                .sum();
        assert_eq!(l2_i8(&qa.q, &qb.q) as i64, l2exact);
        let approx = dot_i8(&qa.q, &qb.q) as f64 * qa.scale as f64 * qb.scale as f64;
        let (truth, _) = dot_f64(&a, &b);
        let (ea, eb) = (qa.scale as f64 / 2.0, qb.scale as f64 / 2.0);
        let (sa, sb): (f64, f64) = (
            a.iter().map(|x| x.abs() as f64).sum(),
            b.iter().map(|x| x.abs() as f64).sum(),
        );
        let bound = ea * sb + eb * sa + dim as f64 * ea * eb;
        assert!(
            (approx - truth).abs() <= bound * 1.001 + 1e-9,
            "i8 dot error {} > bound {bound}",
            (approx - truth).abs()
        );
    }
    println!(
        "dot_f16 / dot_bf16 match f64 oracle on decoded values; i8 dot and L2 are exact in i32; dequantised error within the proven bound"
    );
}

fn test_norm_trick_cancellation() {
    // Near-duplicate vectors with a large common offset: the true distance is tiny, the norms are huge.
    let mut rng = Rng(0xc0ffee);
    let dim = 256;
    let base: Vec<f32> = (0..dim).map(|_| 100.0 + rng.gauss()).collect();
    let k = best();
    let (mut direct_err, mut trick_err) = (0.0f64, 0.0f64);
    for _ in 0..200 {
        let b: Vec<f32> = base.iter().map(|x| x + rng.gauss() * 1e-3).collect();
        let truth = l2_f64(&base, &b);
        direct_err = direct_err.max(((k.l2)(&base, &b) as f64 - truth).abs() / truth);
        let v = l2_via_norms(k, &base, (k.dot)(&base, &base), &b, (k.dot)(&b, &b)) as f64;
        trick_err = trick_err.max((v - truth).abs() / truth);
    }
    println!(
        "near-duplicates at offset 100: relative error of squared L2 -> direct {direct_err:.1e}, norm trick {trick_err:.1e}"
    );
    assert!(direct_err < 1e-3);
    assert!(
        trick_err > direct_err * 10.0,
        "cancellation should visibly hurt the norm trick"
    );
}

fn test_batch_and_early_abandon() {
    let mut rng = Rng(0xba7c4);
    let k = best();
    for &(rows, dim) in &[
        (1usize, 5usize),
        (7, 33),
        (8, 64),
        (9, 100),
        (100, 129),
        (257, 768),
    ] {
        let mat: Vec<f32> = (0..rows * dim).map(|_| rng.gauss()).collect();
        let q: Vec<f32> = (0..dim).map(|_| rng.gauss()).collect();
        let mut row_out = vec![0.0f32; rows];
        batch_dot_rowmajor(k, &q, &mat, dim, &mut row_out);
        let mut blk_out = vec![f32::NAN; rows];
        BlockedMat::from_rows(&mat, dim).dot_all(&q, &mut blk_out);
        for r in 0..rows {
            let (want, abs) = dot_f64(&q, &mat[r * dim..(r + 1) * dim]);
            let tol = (dim as f64 + 8.0) * EPS * abs;
            assert_close("row-major batch", row_out[r] as f64, want, tol);
            assert_close("blocked batch", blk_out[r] as f64, want, tol);
        }
    }
    // early abandon: exact when the true distance is under the bound, > bound (and <= truth) when it is over
    let mut saved = 0usize;
    let mut total = 0usize;
    for _ in 0..2000 {
        let dim = [16usize, 20, 100, 129, 768][rng.below(5)];
        let a: Vec<f32> = (0..dim).map(|_| rng.gauss()).collect();
        let b: Vec<f32> = (0..dim).map(|_| rng.gauss()).collect();
        let truth = l2_f64(&a, &b);
        let bound = (truth * (0.2 + 1.6 * rng.unit())) as f32;
        let (got, read) = l2_early_abandon(&a, &b, bound);
        total += dim;
        saved += dim - read;
        if truth <= bound as f64 * 0.999 {
            assert_close(
                "early abandon (finished)",
                got as f64,
                truth,
                4.0 * (dim as f64 + 8.0) * EPS * truth,
            );
            assert_eq!(read, dim);
        } else if truth > bound as f64 * 1.001 {
            assert!(
                got > bound || read == dim,
                "abandoned result must exceed the bound"
            );
            assert!(
                got as f64 <= truth * (1.0 + 1e-4),
                "partial sum cannot exceed the full distance"
            );
        }
    }
    println!(
        "blocked and row-major batch kernels agree with the oracle; early abandon skipped {:.0}% of the dimension reads",
        100.0 * saved as f64 / total as f64
    );
    assert!(saved > 0);
}

fn test_summation() {
    let n = 2_000_000;
    let x: Vec<f32> = vec![0.1; n];
    let exact = 0.1f32 as f64 * n as f64;
    let (e_naive, e_lanes, e_kahan) = (
        (sum_naive(&x) as f64 - exact).abs() / exact,
        (sum_lanes(&x) as f64 - exact).abs() / exact,
        (sum_kahan(&x) as f64 - exact).abs() / exact,
    );
    println!(
        "sum of 2e6 x 0.1f32, relative error: naive {e_naive:.1e}, 16 lanes {e_lanes:.1e}, Kahan {e_kahan:.1e}"
    );
    assert!(e_kahan <= e_lanes + 1e-12 && e_lanes < e_naive && e_kahan < 1e-6);
    // order dependence: the same multiset summed forwards and backwards is not bit-identical in general
    let mut rng = Rng(0x0dd);
    let v: Vec<f32> = (0..10_000)
        .map(|_| rng.gauss() * 10f32.powi(rng.below(6) as i32 - 3))
        .collect();
    let rev: Vec<f32> = v.iter().rev().copied().collect();
    let (f, r) = (sum_naive(&v), sum_naive(&rev));
    println!(
        "same numbers, forward vs reverse order: {f:e} vs {r:e} (bit-identical: {})",
        f.to_bits() == r.to_bits()
    );
    let oracle: f64 = v.iter().map(|x| *x as f64).sum();
    let abs: f64 = v.iter().map(|x| x.abs() as f64).sum();
    for s in [f, r, sum_lanes(&v), sum_kahan(&v)] {
        assert!(
            (s as f64 - oracle).abs() <= 10_000.0 * EPS * abs,
            "sum outside the n*eps*sum|x| bound"
        );
    }
    // consequence for search: two kernels can disagree in the last bits, so equal-distance ties must be broken by id, not by float luck.
}

// ---------- printed benchmark (never asserted) ----------
fn bench() {
    let (dim, rows) = (768usize, 20_000usize);
    let mut rng = Rng(0xbe9c);
    let mat: Vec<f32> = (0..rows * dim).map(|_| rng.gauss()).collect();
    let q: Vec<f32> = (0..dim).map(|_| rng.gauss()).collect();
    let mut out = vec![0.0f32; rows];
    println!(
        "\nbench: 1 query vs {rows} x {dim}-dim f32 ({} MB, larger than L2 so this is bandwidth-bound)",
        rows * dim * 4 / (1 << 20)
    );
    println!("{:<22} {:>10} {:>10}", "kernel", "GFLOP/s", "GB/s");
    let report = |name: &str, secs: f64, reps: usize, flops_per: f64, bytes_per: f64| {
        let n = (rows * reps) as f64;
        println!(
            "{name:<22} {:>10.2} {:>10.2}",
            n * flops_per / secs / 1e9,
            n * bytes_per / secs / 1e9
        );
    };
    let reps = 20;
    for k in variants() {
        let t = Instant::now();
        for _ in 0..reps {
            for (o, row) in out.iter_mut().zip(mat.chunks_exact(dim)) {
                *o = (k.dot)(black_box(&q), row);
            }
            black_box(&out);
        }
        report(
            &format!("dot {}", k.name),
            t.elapsed().as_secs_f64(),
            reps,
            2.0 * dim as f64,
            4.0 * dim as f64,
        );
    }
    let blocked = BlockedMat::from_rows(&mat, dim);
    let t = Instant::now();
    for _ in 0..reps {
        blocked.dot_all(black_box(&q), &mut out);
        black_box(&out);
    }
    report(
        "dot blocked8 layout",
        t.elapsed().as_secs_f64(),
        reps,
        2.0 * dim as f64,
        4.0 * dim as f64,
    );
    let half: Vec<u16> = mat.iter().map(|x| f32_to_f16(*x)).collect();
    let qh: Vec<u16> = q.iter().map(|x| f32_to_f16(*x)).collect();
    let t = Instant::now();
    for _ in 0..reps {
        for (o, row) in out.iter_mut().zip(half.chunks_exact(dim)) {
            *o = dot_f16(black_box(&qh), row);
        }
        black_box(&out);
    }
    report(
        "dot f16 (widen)",
        t.elapsed().as_secs_f64(),
        reps,
        2.0 * dim as f64,
        2.0 * dim as f64,
    );
    let q8 = quantize_i8(&q).q;
    let m8: Vec<i8> = mat
        .iter()
        .map(|x| (x * 40.0).clamp(-127.0, 127.0) as i8)
        .collect();
    let t = Instant::now();
    let mut acc = 0i64;
    for _ in 0..reps {
        for row in m8.chunks_exact(dim) {
            acc += dot_i8(black_box(&q8), row) as i64;
        }
    }
    black_box(acc);
    report(
        "dot i8 (i32 acc)",
        t.elapsed().as_secs_f64(),
        reps,
        2.0 * dim as f64,
        dim as f64,
    );
    let codes: Vec<u8> = mat
        .iter()
        .map(|x| (*x > 0.0) as u8)
        .collect::<Vec<_>>()
        .chunks(8)
        .map(|c| c.iter().enumerate().fold(0u8, |a, (i, b)| a | (b << i)))
        .collect();
    let cb = dim / 8;
    for (name, f) in hamming_variants() {
        let t = Instant::now();
        let mut tot = 0u64;
        for _ in 0..reps {
            for row in codes.chunks_exact(cb) {
                tot += f(black_box(&codes[..cb]), row) as u64;
            }
        }
        black_box(tot);
        report(
            &format!("hamming {name} ({} bit)", dim),
            t.elapsed().as_secs_f64(),
            reps,
            dim as f64,
            cb as f64,
        );
    }
}

fn main() {
    println!(
        "kernels on this machine: {:?}; dispatch picks '{}'",
        variants().iter().map(|k| k.name).collect::<Vec<_>>(),
        best().name
    );
    test_float_kernels();
    test_hamming();
    test_half_and_int8();
    test_norm_trick_cancellation();
    test_batch_and_early_abandon();
    test_summation();
    bench();
    println!("\nall kernel checks passed");
}
