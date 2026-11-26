//! RECIPE 444 (perf): explicit SIMD with `std::arch`: NEON and SSE2/AVX2 kernels, runtime dispatch, and tail handling.
//! LEVEL: advanced
//! Use in: byte scanning (memchr-like), checksums, validation fast paths, image/pixel math, bitmaps.   Run: cargo run --release --example simd_intrinsics
#![allow(dead_code)]
//!
//! WHAT: ten kernels (sum of u8/u32/f32, f32 dot product, u8 max, find-first-byte, count-byte, ASCII prefix length ->
//! UTF-8 validation, popcount of a bitmap, saturating add of two u8 "images") each written FOUR ways: `scalar` (the
//! obviously-correct reference), `portable` (plain Rust shaped so LLVM auto-vectorises it), `neon` (AArch64, always
//! present there) and `sse2` / `avx2` (x86_64; AVX2 chosen at run time). Every variant is tested against `scalar`.
//!
//! WHY: auto-vectorisation fails on early exits (find), on horizontal tricks (movemask, SAD, popcount tables) and on float
//! reductions (not associative). Intrinsics give you the exact instruction. The price is one copy of the code per ISA.
//!
//! HOW (the pattern every kernel follows):
//!  1. `as_chunks::<N>()` splits the slice into `&[[T; N]]` blocks plus a `< N` remainder. The remainder is handled by a
//!     scalar loop (simple and obviously right; the production trick is one overlapping final load instead).
//!  2. Loads are UNALIGNED (`vld1q_u8`, `_mm_loadu_si128`, `_mm256_loadu_si256`). On every CPU of the last decade an
//!     unaligned load of data that does not straddle a cache line costs the same as an aligned one, so there is no
//!     "peel the unaligned head" loop: the head is just block 0. The test below slides the start 0..64 bytes.
//!  3. Accumulate in narrow lanes and widen BEFORE they overflow (u8 counters flush every 255 blocks, u16 every 64..1024).
//!  4. Horizontal reduction once at the end (`vaddvq_*`, `vmaxvq_*`, SAD against zero, shuffle+add).
//!  5. movemask: x86 has `pmovmskb` (1 bit per byte). NEON has none; the idiom is `vshrn_n_u16::<4>` (shift right and narrow)
//!     which packs the 16 compare bytes into a 64-bit value with 4 bits per byte, so the first match is `trailing_zeros() / 4`.
//!  6. Dispatch: `variants()` lists what this CPU supports (`is_x86_feature_detected!` is a cached atomic read); `best()`
//!     picks once into a `OnceLock<Kernels>`, a struct of plain `fn` pointers. The indirect call costs ~1 ns, so give each
//!     call enough work (hundreds of bytes). A `#[target_feature(enable = "avx2")]` fn is only sound to call after
//!     detection, so the x86 table entries are wrapped in one `unsafe` closure per kernel after the check.
//!
//! `std::simd` (portable SIMD): OBSERVED on this machine (rustc 1.98.1 stable): `use std::simd::f32x4;` fails with
//! `E0658 use of unstable library feature portable_simd` (tracking issue 86656). So on stable you either use
//! `std::arch` per ISA (this file), write auto-vectorisable code (the `portable` variant), or use crates: `wide`
//! (stable portable vector types), `pulp` (safe runtime-dispatched SIMD closures) and `memchr` (production byte search
//! with exactly these NEON/SSE2/AVX2 kernels plus Teddy multi-pattern search).
//!
//! HONEST RESULT (Apple M4 Pro): LLVM already auto-vectorises the plain `scalar` sum_u8, max_u8, popcount and sat_add (they are
//! memory-bound, ~1.0-1.6x for NEON), so intrinsics buy nothing there; they win big (7-30x) where the loop has an early exit
//! (find_byte, ascii_prefix), a compare+count pattern (count_byte) or a float reduction (sum_f32, dot_f32). Measure first.
//!
//! FOOTGUNS: (1) calling an `avx2` function on a CPU without it is undefined behaviour, never skip the detection.
//! (2) `-C target-cpu=native` makes the `portable` variant use AVX2/AVX-512 too but the binary then only runs on that CPU.
//! (3) Float sums change with association order: compare with a tolerance (or with integer-valued floats as here).
//! (4) 256-bit AVX on some older Intel/AMD cores lowers clocks; benchmark on the target hardware.
//!
//! COMMON USES: the `memchr` crate (used by ripgrep, `regex`, `bstr`), simdjson-style parsers and Lemire's SIMD UTF-8
//! validation (arXiv 2010.03090), base64/hex codecs, image blending and alpha compositing, bitmap indexes
//! (popcount over bitsets, Roaring-style), database column scans (sum/min/max/count with predicate), checksum loops.
//! Read: "Algorithms for Modern Hardware" SIMD chapter https://en.algorithmica.org/hpc/simd/ ; https://doc.rust-lang.org/core/arch/index.html ;
//! https://github.com/BurntSushi/memchr ; https://docs.rs/wide ; https://docs.rs/pulp ; https://arxiv.org/abs/2010.03090 ;
//! https://branchfree.org/2019/04/01/fitting-my-head-through-the-arm-holes-or-two-sequences-to-substitute-for-the-missing-pmovmskb-instruction-on-arm-neon/ ;
//! https://github.com/rust-lang/rust/issues/86656
use std::hint::black_box;
use std::sync::OnceLock;
use std::time::Instant;

// ------------------------------------------------------------------------------------------------ dispatch table
type SumU8 = fn(&[u8]) -> u64;
type SumU32 = fn(&[u32]) -> u64;
type SumF32 = fn(&[f32]) -> f32;
type DotF32 = fn(&[f32], &[f32]) -> f32;
type MaxU8 = fn(&[u8]) -> u8;
type FindByte = fn(&[u8], u8) -> Option<usize>;
type CountByte = fn(&[u8], u8) -> usize;
type AsciiPrefix = fn(&[u8]) -> usize;
type Popcount = fn(&[u8]) -> u64;
type SatAdd = fn(&[u8], &[u8], &mut [u8]);

#[derive(Clone, Copy)]
struct Kernels {
    name: &'static str,
    sum_u8: SumU8,
    sum_u32: SumU32,
    sum_f32: SumF32,
    dot_f32: DotF32,
    max_u8: MaxU8,
    find_byte: FindByte,
    count_byte: CountByte,
    ascii_prefix: AsciiPrefix,
    popcount: Popcount,
    sat_add: SatAdd,
}

/// Build a table from a module whose functions are safe to call on this CPU.
macro_rules! table {
    ($name:literal, $m:ident) => {
        Kernels {
            name: $name,
            sum_u8: $m::sum_u8,
            sum_u32: $m::sum_u32,
            sum_f32: $m::sum_f32,
            dot_f32: $m::dot_f32,
            max_u8: $m::max_u8,
            find_byte: $m::find_byte,
            count_byte: $m::count_byte,
            ascii_prefix: $m::ascii_prefix,
            popcount: $m::popcount,
            sat_add: $m::sat_add,
        }
    };
}

fn variants() -> Vec<Kernels> {
    let mut v = vec![table!("scalar", scalar), table!("portable", portable)];
    #[cfg(target_arch = "aarch64")]
    v.push(neon_table()); // NEON (ASIMD) is mandatory on AArch64: no detection needed
    #[cfg(target_arch = "x86_64")]
    {
        v.push(sse2_table()); // SSE2 is part of the x86_64 baseline
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            v.push(avx2_table());
        }
    }
    v
}

/// Chosen once, then a plain load: the "function-pointer table behind a OnceLock" pattern (cf. how `memchr` and glibc ifuncs work).
fn best() -> &'static Kernels {
    static BEST: OnceLock<Kernels> = OnceLock::new();
    BEST.get_or_init(|| *variants().last().unwrap())
}

// ------------------------------------------------------------------------------------------------ scalar reference
mod scalar {
    pub fn sum_u8(a: &[u8]) -> u64 {
        a.iter().map(|&b| b as u64).sum()
    }
    pub fn sum_u32(a: &[u32]) -> u64 {
        a.iter().map(|&b| b as u64).sum()
    }
    pub fn sum_f32(a: &[f32]) -> f32 {
        a.iter().fold(0.0, |s, &x| s + x)
    }
    pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).fold(0.0, |s, (&x, &y)| s + x * y)
    }
    pub fn max_u8(a: &[u8]) -> u8 {
        a.iter().copied().max().unwrap_or(0)
    }
    pub fn find_byte(h: &[u8], n: u8) -> Option<usize> {
        h.iter().position(|&b| b == n)
    }
    pub fn count_byte(h: &[u8], n: u8) -> usize {
        h.iter().filter(|&&b| b == n).count()
    }
    pub fn ascii_prefix(a: &[u8]) -> usize {
        a.iter().position(|&b| b >= 0x80).unwrap_or(a.len())
    }
    pub fn popcount(a: &[u8]) -> u64 {
        a.iter().map(|b| b.count_ones() as u64).sum()
    }
    pub fn sat_add(a: &[u8], b: &[u8], out: &mut [u8]) {
        assert!(a.len() == b.len() && a.len() == out.len());
        for i in 0..a.len() {
            out[i] = a[i].saturating_add(b[i]);
        }
    }
}

// ------------------------------------------------------------------------------------------------ portable (auto-vectorisable)
// Fixed-size lane arrays and branch-free inner loops over `[T; N]` blocks: LLVM turns `for k in 0..N { acc[k] += c[k] }`
// into one vector op. Early-exit loops are NOT vectorised, so `find` ORs a whole block first and only then locates the byte.
mod portable {
    pub fn sum_u8(a: &[u8]) -> u64 {
        let (ch, tail) = a.as_chunks::<16>();
        let mut acc = [0u64; 16];
        for c in ch {
            for k in 0..16 {
                acc[k] += c[k] as u64;
            }
        }
        acc.iter().sum::<u64>() + super::scalar::sum_u8(tail)
    }
    pub fn sum_u32(a: &[u32]) -> u64 {
        let (ch, tail) = a.as_chunks::<8>();
        let mut acc = [0u64; 8];
        for c in ch {
            for k in 0..8 {
                acc[k] += c[k] as u64;
            }
        }
        acc.iter().sum::<u64>() + super::scalar::sum_u32(tail)
    }
    pub fn sum_f32(a: &[f32]) -> f32 {
        let (ch, tail) = a.as_chunks::<16>();
        let mut acc = [0f32; 16];
        for c in ch {
            for k in 0..16 {
                acc[k] += c[k];
            }
        }
        acc.iter().sum::<f32>() + super::scalar::sum_f32(tail)
    }
    pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        let (ca, ta) = a.as_chunks::<16>();
        let (cb, tb) = b.as_chunks::<16>();
        let mut acc = [0f32; 16];
        for (x, y) in ca.iter().zip(cb) {
            for k in 0..16 {
                acc[k] += x[k] * y[k];
            }
        }
        acc.iter().sum::<f32>() + super::scalar::dot_f32(ta, tb)
    }
    pub fn max_u8(a: &[u8]) -> u8 {
        let (ch, tail) = a.as_chunks::<16>();
        let mut acc = [0u8; 16];
        for c in ch {
            for k in 0..16 {
                acc[k] = acc[k].max(c[k]);
            }
        }
        acc.iter()
            .copied()
            .max()
            .unwrap()
            .max(super::scalar::max_u8(tail))
    }
    pub fn find_byte(h: &[u8], n: u8) -> Option<usize> {
        let (ch, tail) = h.as_chunks::<32>();
        for (i, c) in ch.iter().enumerate() {
            let mut any = 0u8;
            for k in 0..32 {
                any |= (c[k] == n) as u8;
            }
            if any != 0 {
                return Some(i * 32 + c.iter().position(|&b| b == n).unwrap());
            }
        }
        super::scalar::find_byte(tail, n).map(|p| ch.len() * 32 + p)
    }
    pub fn count_byte(h: &[u8], n: u8) -> usize {
        let (ch, tail) = h.as_chunks::<16>();
        let mut total = 0usize;
        for blk in ch.chunks(255) {
            let mut acc = [0u8; 16]; // each lane gains <= 1 per block: 255 blocks cannot overflow u8
            for c in blk {
                for k in 0..16 {
                    acc[k] += (c[k] == n) as u8;
                }
            }
            total += acc.iter().map(|&x| x as usize).sum::<usize>();
        }
        total + super::scalar::count_byte(tail, n)
    }
    pub fn ascii_prefix(a: &[u8]) -> usize {
        let (ch, tail) = a.as_chunks::<32>();
        for (i, c) in ch.iter().enumerate() {
            let mut or = 0u8;
            for k in 0..32 {
                or |= c[k];
            }
            if or >= 0x80 {
                return i * 32 + super::scalar::ascii_prefix(c);
            }
        }
        ch.len() * 32 + super::scalar::ascii_prefix(tail)
    }
    pub fn popcount(a: &[u8]) -> u64 {
        let (ch, tail) = a.as_chunks::<8>();
        ch.iter()
            .map(|c| u64::from_le_bytes(*c).count_ones() as u64)
            .sum::<u64>()
            + super::scalar::popcount(tail)
    }
    pub fn sat_add(a: &[u8], b: &[u8], out: &mut [u8]) {
        assert!(a.len() == b.len() && a.len() == out.len());
        for ((o, &x), &y) in out.iter_mut().zip(a).zip(b) {
            *o = x.saturating_add(y);
        }
    }
}

// ------------------------------------------------------------------------------------------------ AArch64 NEON
#[cfg(target_arch = "aarch64")]
mod neon {
    use std::arch::aarch64::*;

    /// Unaligned 16-byte load at byte offset `i` of a fixed-size block.
    #[inline]
    #[target_feature(enable = "neon")]
    fn ld<const N: usize>(b: &[u8; N], i: usize) -> uint8x16_t {
        assert!(i + 16 <= N);
        // SAFETY: the assert proves bytes i..i+16 lie inside the borrowed array; vld1q_u8 has no alignment requirement.
        unsafe { vld1q_u8(b.as_ptr().add(i)) }
    }
    #[inline]
    #[target_feature(enable = "neon")]
    fn ld_f32(b: &[f32; 16], i: usize) -> float32x4_t {
        assert!(i + 4 <= 16);
        // SAFETY: floats i..i+4 are inside the array; vld1q_f32 needs only element alignment, which a `&[f32]` guarantees.
        unsafe { vld1q_f32(b.as_ptr().add(i)) }
    }
    /// movemask emulation: 16 compare bytes (0x00/0xFF) -> u64 with 4 bits per input byte. First hit = tz / 4.
    #[inline]
    #[target_feature(enable = "neon")]
    pub fn nibble_mask(m: uint8x16_t) -> u64 {
        vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(vreinterpretq_u16_u8(
            m,
        ))))
    }

    #[target_feature(enable = "neon")]
    pub fn sum_u8(a: &[u8]) -> u64 {
        // 4 independent accumulators: one chain would be limited by the 3-4 cycle latency of vpadalq, not by memory.
        let (ch, tail) = a.as_chunks::<64>();
        let mut total = 0u64;
        for blk in ch.chunks(64) {
            // vpadalq_u8 adds adjacent byte pairs into u16 lanes: <= 510 per block, 64 blocks <= 32640 < 65536.
            let mut acc = [vdupq_n_u16(0); 4];
            for c in blk {
                for k in 0..4 {
                    acc[k] = vpadalq_u8(acc[k], ld(c, 16 * k));
                }
            }
            total += acc.iter().map(|&x| vaddlvq_u16(x) as u64).sum::<u64>();
        }
        total + super::scalar::sum_u8(tail)
    }
    #[target_feature(enable = "neon")]
    pub fn sum_u32(a: &[u32]) -> u64 {
        let (ch, tail) = a.as_chunks::<16>();
        let mut acc = [vdupq_n_u64(0); 4];
        for c in ch {
            for k in 0..4 {
                // SAFETY: `c` is a &[u32; 16] and k < 4, so elements 4k..4k+4 are in bounds and element-aligned.
                acc[k] = vpadalq_u32(acc[k], unsafe { vld1q_u32(c.as_ptr().add(4 * k)) });
            }
        }
        acc.iter().map(|&x| vaddvq_u64(x)).sum::<u64>() + super::scalar::sum_u32(tail)
    }
    #[target_feature(enable = "neon")]
    pub fn sum_f32(a: &[f32]) -> f32 {
        let (ch, tail) = a.as_chunks::<16>();
        let mut acc = [vdupq_n_f32(0.0); 4]; // 4 independent chains hide the 3-4 cycle add latency
        for c in ch {
            for k in 0..4 {
                acc[k] = vaddq_f32(acc[k], ld_f32(c, 4 * k));
            }
        }
        let s = vaddq_f32(vaddq_f32(acc[0], acc[1]), vaddq_f32(acc[2], acc[3]));
        vaddvq_f32(s) + super::scalar::sum_f32(tail)
    }
    #[target_feature(enable = "neon")]
    pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        let (ca, ta) = a.as_chunks::<16>();
        let (cb, tb) = b.as_chunks::<16>();
        let mut acc = [vdupq_n_f32(0.0); 4];
        for (x, y) in ca.iter().zip(cb) {
            for k in 0..4 {
                acc[k] = vfmaq_f32(acc[k], ld_f32(x, 4 * k), ld_f32(y, 4 * k)); // fused multiply-add
            }
        }
        let s = vaddq_f32(vaddq_f32(acc[0], acc[1]), vaddq_f32(acc[2], acc[3]));
        vaddvq_f32(s) + super::scalar::dot_f32(ta, tb)
    }
    #[target_feature(enable = "neon")]
    pub fn max_u8(a: &[u8]) -> u8 {
        let (ch, tail) = a.as_chunks::<64>();
        let mut acc = [vdupq_n_u8(0); 4];
        for c in ch {
            for k in 0..4 {
                acc[k] = vmaxq_u8(acc[k], ld(c, 16 * k));
            }
        }
        let m = vmaxq_u8(vmaxq_u8(acc[0], acc[1]), vmaxq_u8(acc[2], acc[3]));
        vmaxvq_u8(m).max(super::scalar::max_u8(tail))
    }
    #[target_feature(enable = "neon")]
    pub fn find_byte(h: &[u8], n: u8) -> Option<usize> {
        let nv = vdupq_n_u8(n);
        let (blocks, rest) = h.as_chunks::<64>();
        for (bi, b) in blocks.iter().enumerate() {
            let m = [0, 16, 32, 48].map(|o| vceqq_u8(ld(b, o), nv));
            // One cheap OR + horizontal max tells us whether ANY of the 64 bytes matched; only then pay for locating it.
            if vmaxvq_u8(vorrq_u8(vorrq_u8(m[0], m[1]), vorrq_u8(m[2], m[3]))) != 0 {
                for (j, mj) in m.iter().enumerate() {
                    let mask = nibble_mask(*mj);
                    if mask != 0 {
                        return Some(bi * 64 + j * 16 + (mask.trailing_zeros() / 4) as usize);
                    }
                }
            }
        }
        let base = blocks.len() * 64;
        let (c16, tail) = rest.as_chunks::<16>();
        for (i, c) in c16.iter().enumerate() {
            let mask = nibble_mask(vceqq_u8(ld(c, 0), nv));
            if mask != 0 {
                return Some(base + i * 16 + (mask.trailing_zeros() / 4) as usize);
            }
        }
        super::scalar::find_byte(tail, n).map(|p| base + c16.len() * 16 + p)
    }
    #[target_feature(enable = "neon")]
    pub fn count_byte(h: &[u8], n: u8) -> usize {
        let nv = vdupq_n_u8(n);
        let (ch, tail) = h.as_chunks::<16>();
        let mut total = 0usize;
        for blk in ch.chunks(255) {
            let mut acc = vdupq_n_u8(0);
            for c in blk {
                acc = vsubq_u8(acc, vceqq_u8(ld(c, 0), nv)); // match = 0xFF = -1, so subtracting adds 1
            }
            total += vaddlvq_u8(acc) as usize;
        }
        total + super::scalar::count_byte(tail, n)
    }
    #[target_feature(enable = "neon")]
    pub fn ascii_prefix(a: &[u8]) -> usize {
        let (ch, tail) = a.as_chunks::<32>();
        for (i, c) in ch.iter().enumerate() {
            if vmaxvq_u8(vorrq_u8(ld(c, 0), ld(c, 16))) >= 0x80 {
                return i * 32 + super::scalar::ascii_prefix(c);
            }
        }
        ch.len() * 32 + super::scalar::ascii_prefix(tail)
    }
    #[target_feature(enable = "neon")]
    pub fn popcount(a: &[u8]) -> u64 {
        let (ch, tail) = a.as_chunks::<64>();
        let mut total = 0u64;
        for blk in ch.chunks(1024) {
            let mut acc = [vdupq_n_u16(0); 4]; // per block each u16 lane gains <= 16: 1024 blocks <= 16384
            for c in blk {
                for k in 0..4 {
                    acc[k] = vpadalq_u8(acc[k], vcntq_u8(ld(c, 16 * k))); // CNT: per-byte popcount in one instruction
                }
            }
            total += acc.iter().map(|&x| vaddlvq_u16(x) as u64).sum::<u64>();
        }
        total + super::scalar::popcount(tail)
    }
    #[target_feature(enable = "neon")]
    pub fn sat_add(a: &[u8], b: &[u8], out: &mut [u8]) {
        assert!(a.len() == b.len() && a.len() == out.len());
        let (ca, ta) = a.as_chunks::<16>();
        let (cb, tb) = b.as_chunks::<16>();
        let (co, to) = out.as_chunks_mut::<16>();
        for ((x, y), o) in ca.iter().zip(cb).zip(co.iter_mut()) {
            let r = vqaddq_u8(ld(x, 0), ld(y, 0)); // UQADD: saturating unsigned add
            // SAFETY: `o` is a &mut [u8; 16]: 16 writable bytes; vst1q_u8 needs no alignment.
            unsafe { vst1q_u8(o.as_mut_ptr(), r) };
        }
        super::scalar::sat_add(ta, tb, to);
    }
}

// ------------------------------------------------------------------------------------------------ x86_64 SSE2 (baseline)
#[cfg(target_arch = "x86_64")]
mod sse2 {
    use std::arch::x86_64::*;
    // sum_u32, sum_f32, dot_f32 and popcount have no SSE2 version here (popcount wants SSSE3 pshufb or POPCNT): sse2_table() uses the portable ones.

    #[inline]
    #[target_feature(enable = "sse2")]
    fn ld<const N: usize>(b: &[u8; N], i: usize) -> __m128i {
        assert!(i + 16 <= N);
        // SAFETY: the assert proves bytes i..i+16 are inside the array; `loadu` has no alignment requirement.
        unsafe { _mm_loadu_si128(b.as_ptr().add(i) as *const __m128i) }
    }
    #[inline]
    #[target_feature(enable = "sse2")]
    fn hsum_u64x2(v: __m128i) -> u64 {
        (_mm_cvtsi128_si64(v) as u64)
            .wrapping_add(_mm_cvtsi128_si64(_mm_unpackhi_epi64(v, v)) as u64)
    }
    #[target_feature(enable = "sse2")]
    pub fn sum_u8(a: &[u8]) -> u64 {
        let (ch, tail) = a.as_chunks::<16>();
        let z = _mm_setzero_si128();
        let mut acc = z;
        for c in ch {
            acc = _mm_add_epi64(acc, _mm_sad_epu8(ld(c, 0), z)); // SAD vs 0 = horizontal sum of 8 bytes into a u64 lane
        }
        hsum_u64x2(acc) + super::scalar::sum_u8(tail)
    }
    #[target_feature(enable = "sse2")]
    pub fn max_u8(a: &[u8]) -> u8 {
        let (ch, tail) = a.as_chunks::<16>();
        let mut v = _mm_setzero_si128();
        for c in ch {
            v = _mm_max_epu8(v, ld(c, 0));
        }
        v = _mm_max_epu8(v, _mm_srli_si128::<8>(v));
        v = _mm_max_epu8(v, _mm_srli_si128::<4>(v));
        v = _mm_max_epu8(v, _mm_srli_si128::<2>(v));
        v = _mm_max_epu8(v, _mm_srli_si128::<1>(v));
        (_mm_cvtsi128_si32(v) as u8).max(super::scalar::max_u8(tail))
    }
    #[target_feature(enable = "sse2")]
    pub fn find_byte(h: &[u8], n: u8) -> Option<usize> {
        let nv = _mm_set1_epi8(n as i8);
        let (blocks, rest) = h.as_chunks::<64>();
        for (bi, b) in blocks.iter().enumerate() {
            let m = [0, 16, 32, 48].map(|o| _mm_cmpeq_epi8(ld(b, o), nv));
            if _mm_movemask_epi8(_mm_or_si128(
                _mm_or_si128(m[0], m[1]),
                _mm_or_si128(m[2], m[3]),
            )) != 0
            {
                for (j, mj) in m.iter().enumerate() {
                    let mask = _mm_movemask_epi8(*mj) as u32;
                    if mask != 0 {
                        return Some(bi * 64 + j * 16 + mask.trailing_zeros() as usize);
                    }
                }
            }
        }
        let base = blocks.len() * 64;
        let (c16, tail) = rest.as_chunks::<16>();
        for (i, c) in c16.iter().enumerate() {
            let mask = _mm_movemask_epi8(_mm_cmpeq_epi8(ld(c, 0), nv)) as u32;
            if mask != 0 {
                return Some(base + i * 16 + mask.trailing_zeros() as usize);
            }
        }
        super::scalar::find_byte(tail, n).map(|p| base + c16.len() * 16 + p)
    }
    #[target_feature(enable = "sse2")]
    pub fn count_byte(h: &[u8], n: u8) -> usize {
        let nv = _mm_set1_epi8(n as i8);
        let z = _mm_setzero_si128();
        let (ch, tail) = h.as_chunks::<16>();
        let mut total = 0u64;
        for blk in ch.chunks(255) {
            let mut acc = z;
            for c in blk {
                acc = _mm_sub_epi8(acc, _mm_cmpeq_epi8(ld(c, 0), nv));
            }
            total += hsum_u64x2(_mm_sad_epu8(acc, z));
        }
        total as usize + super::scalar::count_byte(tail, n)
    }
    #[target_feature(enable = "sse2")]
    pub fn ascii_prefix(a: &[u8]) -> usize {
        let (ch, tail) = a.as_chunks::<32>();
        for (i, c) in ch.iter().enumerate() {
            // movemask collects the top bit of every byte: non-zero <=> some byte >= 0x80. No compare needed.
            if _mm_movemask_epi8(_mm_or_si128(ld(c, 0), ld(c, 16))) != 0 {
                return i * 32 + super::scalar::ascii_prefix(c);
            }
        }
        ch.len() * 32 + super::scalar::ascii_prefix(tail)
    }
    #[target_feature(enable = "sse2")]
    pub fn sat_add(a: &[u8], b: &[u8], out: &mut [u8]) {
        assert!(a.len() == b.len() && a.len() == out.len());
        let (ca, ta) = a.as_chunks::<16>();
        let (cb, tb) = b.as_chunks::<16>();
        let (co, to) = out.as_chunks_mut::<16>();
        for ((x, y), o) in ca.iter().zip(cb).zip(co.iter_mut()) {
            let r = _mm_adds_epu8(ld(x, 0), ld(y, 0));
            // SAFETY: `o` is a &mut [u8; 16]; `storeu` needs no alignment.
            unsafe { _mm_storeu_si128(o.as_mut_ptr() as *mut __m128i, r) };
        }
        super::scalar::sat_add(ta, tb, to);
    }
}

// ------------------------------------------------------------------------------------------------ x86_64 AVX2 (+FMA), runtime-detected
// Every fn here is `#[target_feature(enable = ...)]`: it may only be CALLED after `is_x86_feature_detected!` said yes.
#[cfg(target_arch = "x86_64")]
mod avx2 {
    use std::arch::x86_64::*;

    #[inline]
    #[target_feature(enable = "avx2")]
    fn ld<const N: usize>(b: &[u8; N], i: usize) -> __m256i {
        assert!(i + 32 <= N);
        // SAFETY: the assert proves bytes i..i+32 are inside the array; `loadu` has no alignment requirement.
        unsafe { _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i) }
    }
    #[inline]
    #[target_feature(enable = "avx2")]
    fn hsum_u64x4(v: __m256i) -> u64 {
        let s = _mm_add_epi64(_mm256_castsi256_si128(v), _mm256_extracti128_si256::<1>(v));
        (_mm_cvtsi128_si64(s) as u64)
            .wrapping_add(_mm_cvtsi128_si64(_mm_unpackhi_epi64(s, s)) as u64)
    }
    #[inline]
    #[target_feature(enable = "avx")]
    fn hsum_ps(v: __m256) -> f32 {
        let s = _mm_add_ps(_mm256_castps256_ps128(v), _mm256_extractf128_ps::<1>(v));
        let s = _mm_add_ps(s, _mm_movehl_ps(s, s));
        _mm_cvtss_f32(_mm_add_ss(s, _mm_shuffle_ps::<0x55>(s, s)))
    }

    #[target_feature(enable = "avx2")]
    pub fn sum_u8(a: &[u8]) -> u64 {
        let (ch, tail) = a.as_chunks::<32>();
        let z = _mm256_setzero_si256();
        let mut acc = z;
        for c in ch {
            acc = _mm256_add_epi64(acc, _mm256_sad_epu8(ld(c, 0), z));
        }
        hsum_u64x4(acc) + super::scalar::sum_u8(tail)
    }
    #[target_feature(enable = "avx2")]
    pub fn sum_u32(a: &[u32]) -> u64 {
        let (ch, tail) = a.as_chunks::<8>();
        let z = _mm256_setzero_si256();
        let mut acc = z;
        for c in ch {
            // SAFETY: `c` is a &[u32; 8] = 32 readable bytes; `loadu` needs no alignment.
            let v = unsafe { _mm256_loadu_si256(c.as_ptr() as *const __m256i) };
            // interleave with zero = zero-extend u32 -> u64 (lane order is irrelevant for a sum)
            acc = _mm256_add_epi64(acc, _mm256_unpacklo_epi32(v, z));
            acc = _mm256_add_epi64(acc, _mm256_unpackhi_epi32(v, z));
        }
        hsum_u64x4(acc) + super::scalar::sum_u32(tail)
    }
    #[target_feature(enable = "avx2")]
    fn ld_f32(b: &[f32; 32], i: usize) -> __m256 {
        assert!(i + 8 <= 32);
        // SAFETY: floats i..i+8 are inside the array; `loadu_ps` needs no alignment.
        unsafe { _mm256_loadu_ps(b.as_ptr().add(i)) }
    }
    #[target_feature(enable = "avx2")]
    pub fn sum_f32(a: &[f32]) -> f32 {
        let (ch, tail) = a.as_chunks::<32>();
        let mut acc = [_mm256_setzero_ps(); 4];
        for c in ch {
            for k in 0..4 {
                acc[k] = _mm256_add_ps(acc[k], ld_f32(c, 8 * k));
            }
        }
        let s = _mm256_add_ps(_mm256_add_ps(acc[0], acc[1]), _mm256_add_ps(acc[2], acc[3]));
        hsum_ps(s) + super::scalar::sum_f32(tail)
    }
    #[target_feature(enable = "avx2,fma")]
    pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len());
        let (ca, ta) = a.as_chunks::<32>();
        let (cb, tb) = b.as_chunks::<32>();
        let mut acc = [_mm256_setzero_ps(); 4];
        for (x, y) in ca.iter().zip(cb) {
            for k in 0..4 {
                acc[k] = _mm256_fmadd_ps(ld_f32(x, 8 * k), ld_f32(y, 8 * k), acc[k]);
            }
        }
        let s = _mm256_add_ps(_mm256_add_ps(acc[0], acc[1]), _mm256_add_ps(acc[2], acc[3]));
        hsum_ps(s) + super::scalar::dot_f32(ta, tb)
    }
    #[target_feature(enable = "avx2")]
    pub fn max_u8(a: &[u8]) -> u8 {
        let (ch, tail) = a.as_chunks::<32>();
        let mut v = _mm256_setzero_si256();
        for c in ch {
            v = _mm256_max_epu8(v, ld(c, 0));
        }
        let mut s = _mm_max_epu8(_mm256_castsi256_si128(v), _mm256_extracti128_si256::<1>(v));
        s = _mm_max_epu8(s, _mm_srli_si128::<8>(s));
        s = _mm_max_epu8(s, _mm_srli_si128::<4>(s));
        s = _mm_max_epu8(s, _mm_srli_si128::<2>(s));
        s = _mm_max_epu8(s, _mm_srli_si128::<1>(s));
        (_mm_cvtsi128_si32(s) as u8).max(super::scalar::max_u8(tail))
    }
    #[target_feature(enable = "avx2")]
    pub fn find_byte(h: &[u8], n: u8) -> Option<usize> {
        let nv = _mm256_set1_epi8(n as i8);
        let (blocks, rest) = h.as_chunks::<64>();
        for (bi, b) in blocks.iter().enumerate() {
            let m0 = _mm256_cmpeq_epi8(ld(b, 0), nv);
            let m1 = _mm256_cmpeq_epi8(ld(b, 32), nv);
            if _mm256_movemask_epi8(_mm256_or_si256(m0, m1)) != 0 {
                let a = _mm256_movemask_epi8(m0) as u32;
                if a != 0 {
                    return Some(bi * 64 + a.trailing_zeros() as usize);
                }
                return Some(
                    bi * 64 + 32 + (_mm256_movemask_epi8(m1) as u32).trailing_zeros() as usize,
                );
            }
        }
        let base = blocks.len() * 64;
        let (c32, tail) = rest.as_chunks::<32>();
        for (i, c) in c32.iter().enumerate() {
            let mask = _mm256_movemask_epi8(_mm256_cmpeq_epi8(ld(c, 0), nv)) as u32;
            if mask != 0 {
                return Some(base + i * 32 + mask.trailing_zeros() as usize);
            }
        }
        super::scalar::find_byte(tail, n).map(|p| base + c32.len() * 32 + p)
    }
    #[target_feature(enable = "avx2")]
    pub fn count_byte(h: &[u8], n: u8) -> usize {
        let nv = _mm256_set1_epi8(n as i8);
        let z = _mm256_setzero_si256();
        let (ch, tail) = h.as_chunks::<32>();
        let mut total = 0u64;
        for blk in ch.chunks(255) {
            let mut acc = z;
            for c in blk {
                acc = _mm256_sub_epi8(acc, _mm256_cmpeq_epi8(ld(c, 0), nv));
            }
            total += hsum_u64x4(_mm256_sad_epu8(acc, z));
        }
        total as usize + super::scalar::count_byte(tail, n)
    }
    #[target_feature(enable = "avx2")]
    pub fn ascii_prefix(a: &[u8]) -> usize {
        let (ch, tail) = a.as_chunks::<64>();
        for (i, c) in ch.iter().enumerate() {
            if _mm256_movemask_epi8(_mm256_or_si256(ld(c, 0), ld(c, 32))) != 0 {
                return i * 64 + super::scalar::ascii_prefix(c);
            }
        }
        ch.len() * 64 + super::scalar::ascii_prefix(tail)
    }
    #[target_feature(enable = "avx2")]
    pub fn popcount(a: &[u8]) -> u64 {
        let (ch, tail) = a.as_chunks::<32>();
        // nibble lookup table: popcount of 0..15, replicated in both 128-bit lanes because vpshufb works per lane
        let lut = _mm256_setr_epi8(
            0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4, 0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2,
            3, 3, 4,
        );
        let low = _mm256_set1_epi8(0x0f);
        let z = _mm256_setzero_si256();
        let mut acc = z;
        for c in ch {
            let v = ld(c, 0);
            let lo = _mm256_and_si256(v, low);
            let hi = _mm256_and_si256(_mm256_srli_epi16::<4>(v), low);
            let cnt = _mm256_add_epi8(_mm256_shuffle_epi8(lut, lo), _mm256_shuffle_epi8(lut, hi)); // <= 8 per byte
            acc = _mm256_add_epi64(acc, _mm256_sad_epu8(cnt, z));
        }
        hsum_u64x4(acc) + super::scalar::popcount(tail)
    }
    #[target_feature(enable = "avx2")]
    pub fn sat_add(a: &[u8], b: &[u8], out: &mut [u8]) {
        assert!(a.len() == b.len() && a.len() == out.len());
        let (ca, ta) = a.as_chunks::<32>();
        let (cb, tb) = b.as_chunks::<32>();
        let (co, to) = out.as_chunks_mut::<32>();
        for ((x, y), o) in ca.iter().zip(cb).zip(co.iter_mut()) {
            let r = _mm256_adds_epu8(ld(x, 0), ld(y, 0));
            // SAFETY: `o` is a &mut [u8; 32]; `storeu` needs no alignment.
            unsafe { _mm256_storeu_si256(o.as_mut_ptr() as *mut __m256i, r) };
        }
        super::scalar::sat_add(ta, tb, to);
    }
}

/// Wrap a `#[target_feature]` fn in a closure that coerces to a plain `fn` pointer (such fns are `unsafe` to call from
/// code that does not itself enable the feature, even when the build already does, so the check is made once, by the caller).
macro_rules! tf {
    ($f:path; $($a:ident),+) => {
        // SAFETY: only reachable through a `*_table()` fn that `variants()` calls when the CPU has the feature: NEON is
        // mandatory on AArch64, SSE2 on x86_64, and AVX2+FMA are checked with `is_x86_feature_detected!` first.
        |$($a),+| unsafe { $f($($a),+) }
    };
}
#[cfg(target_arch = "aarch64")]
fn neon_table() -> Kernels {
    Kernels {
        name: "neon",
        sum_u8: tf!(neon::sum_u8; a),
        sum_u32: tf!(neon::sum_u32; a),
        sum_f32: tf!(neon::sum_f32; a),
        dot_f32: tf!(neon::dot_f32; a, b),
        max_u8: tf!(neon::max_u8; a),
        find_byte: tf!(neon::find_byte; a, n),
        count_byte: tf!(neon::count_byte; a, n),
        ascii_prefix: tf!(neon::ascii_prefix; a),
        popcount: tf!(neon::popcount; a),
        sat_add: tf!(neon::sat_add; a, b, o),
    }
}
#[cfg(target_arch = "x86_64")]
fn sse2_table() -> Kernels {
    Kernels {
        name: "sse2",
        sum_u8: tf!(sse2::sum_u8; a),
        sum_u32: portable::sum_u32,
        sum_f32: portable::sum_f32,
        dot_f32: portable::dot_f32,
        max_u8: tf!(sse2::max_u8; a),
        find_byte: tf!(sse2::find_byte; a, n),
        count_byte: tf!(sse2::count_byte; a, n),
        ascii_prefix: tf!(sse2::ascii_prefix; a),
        popcount: portable::popcount,
        sat_add: tf!(sse2::sat_add; a, b, o),
    }
}
#[cfg(target_arch = "x86_64")]
fn avx2_table() -> Kernels {
    Kernels {
        name: "avx2",
        sum_u8: tf!(avx2::sum_u8; a),
        sum_u32: tf!(avx2::sum_u32; a),
        sum_f32: tf!(avx2::sum_f32; a),
        dot_f32: tf!(avx2::dot_f32; a, b),
        max_u8: tf!(avx2::max_u8; a),
        find_byte: tf!(avx2::find_byte; a, n),
        count_byte: tf!(avx2::count_byte; a, n),
        ascii_prefix: tf!(avx2::ascii_prefix; a),
        popcount: tf!(avx2::popcount; a),
        sat_add: tf!(avx2::sat_add; a, b, o),
    }
}

// ------------------------------------------------------------------------------------------------ UTF-8 validation on top of ascii_prefix
/// Length of the multi-byte sequence starting at `s[0]` (which is >= 0x80), or None when invalid (RFC 3629: no overlongs,
/// no surrogates D800..DFFF, nothing above U+10FFFF).
fn multibyte_len(s: &[u8]) -> Option<usize> {
    let cont = |i: usize, lo: u8, hi: u8| s.get(i).is_some_and(|&b| (lo..=hi).contains(&b));
    match s[0] {
        0xC2..=0xDF => cont(1, 0x80, 0xBF).then_some(2),
        0xE0 => (cont(1, 0xA0, 0xBF) && cont(2, 0x80, 0xBF)).then_some(3),
        0xE1..=0xEC | 0xEE..=0xEF => (cont(1, 0x80, 0xBF) && cont(2, 0x80, 0xBF)).then_some(3),
        0xED => (cont(1, 0x80, 0x9F) && cont(2, 0x80, 0xBF)).then_some(3),
        0xF0 => (cont(1, 0x90, 0xBF) && cont(2, 0x80, 0xBF) && cont(3, 0x80, 0xBF)).then_some(4),
        0xF1..=0xF3 => {
            (cont(1, 0x80, 0xBF) && cont(2, 0x80, 0xBF) && cont(3, 0x80, 0xBF)).then_some(4)
        }
        0xF4 => (cont(1, 0x80, 0x8F) && cont(2, 0x80, 0xBF) && cont(3, 0x80, 0xBF)).then_some(4),
        _ => None,
    }
}
/// ASCII runs are skipped 32-64 bytes at a time by the SIMD kernel; only non-ASCII bytes take the scalar decoder.
fn validate_utf8(k: &Kernels, s: &[u8]) -> bool {
    let mut i = 0;
    loop {
        i += (k.ascii_prefix)(&s[i..]);
        if i >= s.len() {
            return true;
        }
        match multibyte_len(&s[i..]) {
            Some(n) => i += n,
            None => return false,
        }
    }
}

// ------------------------------------------------------------------------------------------------ tests
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
}

fn rel_close(got: f32, want: f32, scale: f32) -> bool {
    (got - want).abs() <= 1e-4 * scale + 1e-6
}

/// Compare every kernel of `k` with the scalar reference `r` on one byte input pair.
fn check_bytes(k: &Kernels, r: &Kernels, d: &[u8], e: &[u8], needle: u8) {
    let ctx = || {
        format!(
            "{} len={} ptr%64={}",
            k.name,
            d.len(),
            d.as_ptr() as usize % 64
        )
    };
    assert_eq!((k.sum_u8)(d), (r.sum_u8)(d), "sum_u8 {}", ctx());
    assert_eq!((k.max_u8)(d), (r.max_u8)(d), "max_u8 {}", ctx());
    assert_eq!(
        (k.find_byte)(d, needle),
        (r.find_byte)(d, needle),
        "find_byte {}",
        ctx()
    );
    assert_eq!(
        (k.count_byte)(d, needle),
        (r.count_byte)(d, needle),
        "count_byte {}",
        ctx()
    );
    assert_eq!(
        (k.ascii_prefix)(d),
        (r.ascii_prefix)(d),
        "ascii_prefix {}",
        ctx()
    );
    assert_eq!((k.popcount)(d), (r.popcount)(d), "popcount {}", ctx());
    let (mut o1, mut o2) = (vec![0u8; d.len()], vec![0u8; d.len()]);
    (k.sat_add)(d, &e[..d.len()], &mut o1);
    (r.sat_add)(d, &e[..d.len()], &mut o2);
    assert_eq!(o1, o2, "sat_add {}", ctx());
}

fn random_char(rng: &mut Rng) -> char {
    loop {
        let cp = match rng.below(10) {
            0..=6 => rng.below(0x80),
            7 => 0x80 + rng.below(0x780),
            8 => 0x800 + rng.below(0xF800),
            _ => 0x10000 + rng.below(0x100000),
        } as u32;
        if let Some(c) = char::from_u32(cp) {
            return c;
        }
    }
}

fn run_tests(vars: &[Kernels]) {
    let r = &vars[0];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    const ITERS: usize = 5000;
    let mut base = vec![0u8; 64 + 300];
    let mut base2 = vec![0u8; 64 + 300];
    let u32s: Vec<u32> = (0..400).map(|_| rng.next() as u32).collect();
    let fi: Vec<f32> = (0..400).map(|_| rng.below(16) as f32).collect(); // integer-valued: every association order is exact
    let fj: Vec<f32> = (0..400).map(|_| rng.below(16) as f32).collect();
    let fr: Vec<f32> = (0..400)
        .map(|_| (rng.below(2_000_001) as f32 - 1e6) / 1e6)
        .collect();
    let fq: Vec<f32> = (0..400)
        .map(|_| (rng.below(2_000_001) as f32 - 1e6) / 1e6)
        .collect();
    let mut checks = 0usize;
    for it in 0..ITERS {
        let len = rng.below(300) as usize;
        let off = it % 64; // sweeps every start alignment 0..64
        let mode = rng.below(7);
        for i in 0..len + 64 {
            let rnd = rng.next() as u8;
            base[i] = match mode {
                0 => rnd,
                1 => rnd & 0x7F, // all ASCII
                2 => rnd & 0x7F, // ASCII with one high byte planted below
                3 => 0x41,       // all equal
                4 => 0,          // all zero
                5 => 0xFF,       // all ones: saturation everywhere
                _ => rnd | 0x80, // all non-ASCII
            };
            base2[i] = if mode == 5 { 0xFF } else { rng.next() as u8 };
        }
        if mode == 2 && len > 0 {
            base[off + rng.below(len as u64) as usize] = 0x80 | rng.next() as u8;
        }
        let d = &base[off..off + len];
        let needle = if len > 0 && rng.below(2) == 0 {
            d[rng.below(len as u64) as usize]
        } else {
            rng.next() as u8
        };
        for k in vars {
            check_bytes(k, r, d, &base2[off..], needle);
            let (s32, fa, fb) = (
                &u32s[off..off + len],
                &fi[off..off + len],
                &fj[off..off + len],
            );
            assert_eq!((k.sum_u32)(s32), (r.sum_u32)(s32), "sum_u32 {}", k.name);
            assert_eq!((k.sum_f32)(fa), (r.sum_f32)(fa), "sum_f32 exact {}", k.name);
            assert_eq!(
                (k.dot_f32)(fa, fb),
                (r.dot_f32)(fa, fb),
                "dot_f32 exact {}",
                k.name
            );
            // random floats: only close, because the order of additions differs
            let (xa, xb) = (&fr[off..off + len], &fq[off..off + len]);
            let sabs: f32 = xa.iter().map(|x| x.abs()).sum();
            assert!(
                rel_close((k.sum_f32)(xa), (r.sum_f32)(xa), sabs),
                "sum_f32 rand {}",
                k.name
            );
            let dabs: f32 = xa.iter().zip(xb).map(|(x, y)| (x * y).abs()).sum();
            assert!(
                rel_close((k.dot_f32)(xa, xb), (r.dot_f32)(xa, xb), dabs),
                "dot_f32 rand {}",
                k.name
            );
            checks += 1;
        }
    }
    // all-equal / all-zero / boundary patterns at every length 0..=130 and every offset 0..64
    for pat in [0x00u8, 0x7F, 0x80, 0xFF, 0x41] {
        base.iter_mut().for_each(|b| *b = pat);
        base2.iter_mut().for_each(|b| *b = pat);
        for len in 0..=130 {
            for off in 0..64 {
                for k in vars {
                    check_bytes(k, r, &base[off..off + len], &base2[off..], pat);
                    check_bytes(k, r, &base[off..off + len], &base2[off..], pat ^ 1);
                    checks += 2;
                }
            }
        }
    }
    // a single planted non-ASCII/needle byte at EVERY position of every length (block-boundary off-by-ones)
    for len in 0..=130usize {
        for pos in 0..len {
            for off in [0usize, 1, 7, 15, 16, 31, 32, 33, 63] {
                base.iter_mut().for_each(|b| *b = 0x61);
                base[off + pos] = 0xC3;
                let d = &base[off..off + len];
                for k in vars {
                    assert_eq!(
                        (k.find_byte)(d, 0xC3),
                        Some(pos),
                        "{} find pos {pos} len {len}",
                        k.name
                    );
                    assert_eq!(
                        (k.ascii_prefix)(d),
                        pos,
                        "{} ascii pos {pos} len {len}",
                        k.name
                    );
                    assert_eq!((k.count_byte)(d, 0xC3), 1);
                    assert_eq!((k.max_u8)(d), 0xC3);
                    assert_eq!((k.find_byte)(d, 0x62), None);
                    checks += 5;
                }
            }
        }
    }
    // 255-block counter flush (u8 lanes) and long runs: 70_000 equal bytes
    let big = vec![7u8; 70_000];
    for k in vars {
        assert_eq!((k.count_byte)(&big, 7), 70_000, "{}", k.name);
        assert_eq!((k.sum_u8)(&big), 490_000);
        assert_eq!((k.popcount)(&big), 3 * 70_000);
        assert_eq!((k.ascii_prefix)(&big), 70_000);
    }
    println!(
        "byte/float/int kernels: {checks} random+boundary checks across {} variants, all equal to scalar",
        vars.len()
    );

    // ---- UTF-8: every variant vs std::str::from_utf8
    let mut n_utf = 0usize;
    for it in 0..ITERS {
        let mut buf: Vec<u8> = Vec::new();
        let kind = rng.below(5);
        let target = rng.below(300) as usize;
        if kind == 0 {
            buf.extend((0..target).map(|_| rng.next() as u8));
        } else {
            let mut s = String::new();
            while s.len() < target {
                s.push(random_char(&mut rng));
            }
            buf = s.into_bytes();
            match kind {
                2 if !buf.is_empty() => {
                    let p = rng.below(buf.len() as u64) as usize;
                    buf[p] = rng.next() as u8; // mutate one byte
                }
                3 if !buf.is_empty() => buf.truncate(rng.below(buf.len() as u64) as usize), // maybe cut mid-char
                _ => {}
            }
        }
        let off = it % 64;
        base[off..off + buf.len()].copy_from_slice(&buf);
        let s = &base[off..off + buf.len()];
        let want = std::str::from_utf8(s).is_ok();
        for k in vars {
            assert_eq!(validate_utf8(k, s), want, "utf8 {} {:02x?}", k.name, s);
            n_utf += 1;
        }
    }
    for bad in [
        &[0xED, 0xA0, 0x80][..],
        &[0xF4, 0x90, 0x80, 0x80],
        &[0xC0, 0x80],
        &[0xE0, 0x80, 0x80],
        &[0xF0, 0x80, 0x80, 0x80],
        &[0xC2],
        &[0xF5, 0x80, 0x80, 0x80],
        &[0x80],
    ] {
        for k in vars {
            assert!(!validate_utf8(k, bad), "{} accepted {:02x?}", k.name, bad);
        }
    }
    for good in ["", "a", "é", "€", "𐍈", "\u{10FFFF}", "\u{D7FF}", "\u{E000}"] {
        for k in vars {
            assert!(validate_utf8(k, good.as_bytes()));
        }
    }
    // exhaustive: every 1-, 2-byte sequence and every 3-byte sequence starting E0..EF, with an ASCII prefix of 31 bytes so the SIMD block boundary is crossed
    let best = best();
    let mut v = vec![b'x'; 31];
    for b0 in 0..=255u8 {
        for b1 in 0..=255u8 {
            v.truncate(31);
            v.extend([b0, b1]);
            assert_eq!(
                validate_utf8(best, &v),
                std::str::from_utf8(&v).is_ok(),
                "{v:02x?}"
            );
            if (0xE0..=0xF4).contains(&b0) {
                for b2 in [0x00, 0x7F, 0x80, 0x8F, 0x90, 0x9F, 0xA0, 0xBF, 0xC0] {
                    for tail in [&[][..], &[0x80, 0x80]] {
                        v.truncate(31);
                        v.extend([b0, b1, b2]);
                        v.extend(tail);
                        assert_eq!(
                            validate_utf8(best, &v),
                            std::str::from_utf8(&v).is_ok(),
                            "{v:02x?}"
                        );
                        n_utf += 1;
                    }
                }
            }
            n_utf += 1;
        }
    }
    println!(
        "utf-8 validation: {n_utf} cases (random, mutated, truncated, exhaustive 2-byte, 3/4-byte structure) agree with std::str::from_utf8"
    );
}

// ------------------------------------------------------------------------------------------------ benchmark
fn best_of<R>(reps: usize, mut f: impl FnMut() -> R) -> (f64, R) {
    let mut best = f64::MAX;
    let mut out = None;
    for _ in 0..reps {
        let t = Instant::now();
        let r = black_box(f());
        best = best.min(t.elapsed().as_secs_f64());
        out = Some(r);
    }
    (best, out.unwrap())
}

fn bench_row<R: PartialEq + std::fmt::Debug>(
    name: &str,
    bytes: usize,
    vars: &[Kernels],
    mut f: impl FnMut(&Kernels) -> R,
) {
    let mut line = format!("{name:<22}");
    let (mut first, mut scalar_gbs, mut top) = (None, 0.0, 0.0f64);
    for k in vars {
        let (secs, r) = best_of(5, || f(k));
        match &first {
            None => first = Some(r),
            Some(e) => assert_eq!(&r, e, "{} {name}", k.name),
        }
        let gbs = bytes as f64 / secs / 1e9;
        if k.name == "scalar" {
            scalar_gbs = gbs;
        }
        top = top.max(gbs);
        line += &format!("{gbs:>10.1}");
    }
    println!("{line}   best/scalar = {:.1}x", top / scalar_gbs);
}

fn bench(vars: &[Kernels]) {
    const N: usize = 16 << 20; // 16 MiB per input: larger than L1/L2, so this measures streaming throughput
    let mut rng = Rng(12345);
    let ascii: Vec<u8> = (0..N).map(|_| (rng.next() as u8) & 0x7F).collect();
    let other: Vec<u8> = (0..N).map(|_| rng.next() as u8).collect();
    let u32s: Vec<u32> = (0..N / 4).map(|_| rng.next() as u32).collect();
    let fa: Vec<f32> = (0..N / 4).map(|_| rng.below(3) as f32).collect(); // 0..2: every partial sum < 2^24 stays exact
    let fb: Vec<f32> = (0..N / 4).map(|_| rng.below(2) as f32).collect();
    let mut out = vec![0u8; N];
    let mut hdr = format!("{:<22}", "GB/s (input bytes)");
    for k in vars {
        hdr += &format!("{:>10}", k.name);
    }
    println!("\n{hdr}");
    bench_row("sum_u8", N, vars, |k| (k.sum_u8)(&ascii));
    bench_row("sum_u32", N, vars, |k| (k.sum_u32)(&u32s));
    bench_row("sum_f32", N, vars, |k| (k.sum_f32)(&fa).to_bits());
    bench_row("dot_f32 (2 inputs)", 2 * N, vars, |k| {
        (k.dot_f32)(&fa, &fb).to_bits()
    });
    bench_row("max_u8", N, vars, |k| (k.max_u8)(&other));
    bench_row("find_byte (absent)", N, vars, |k| {
        (k.find_byte)(&ascii, 0xC3)
    });
    bench_row("count_byte", N, vars, |k| (k.count_byte)(&ascii, 0x41));
    bench_row("ascii_prefix", N, vars, |k| (k.ascii_prefix)(&ascii));
    bench_row("popcount", N, vars, |k| (k.popcount)(&other));
    bench_row("sat_add (r+r+w)", 3 * N, vars, |k| {
        (k.sat_add)(&ascii, &other, &mut out);
        out.iter().step_by(4099).map(|&b| b as u32).sum::<u32>()
    });
    // UTF-8: mostly-ASCII text with a few multi-byte characters per KiB, and pure ASCII
    let unit = "The quick brown fox jumps over the lazy dog; caf\u{e9} cr\u{e8}me br\u{fb}l\u{e9}e costs \u{20ac}5 and 100 bytes of plain text follow here. ";
    let text: String = unit.repeat(N / unit.len());
    let ascii_text = "all plain ascii text, nothing fancy here at all. ".repeat(N / 49);
    for (label, s) in [
        ("utf8 mostly-ASCII", &text),
        ("utf8 pure ASCII", &ascii_text),
    ] {
        bench_row(label, s.len(), vars, |k| validate_utf8(k, s.as_bytes()));
        let (secs, ok) = best_of(5, || std::str::from_utf8(black_box(s.as_bytes())).is_ok());
        assert!(ok);
        println!(
            "{:<22}{:>10.1}   (std::str::from_utf8, for comparison)",
            "",
            s.len() as f64 / secs / 1e9
        );
    }
    let (secs, p) = best_of(5, || ascii.iter().position(|&b| b == 0xC3));
    assert_eq!(p, None);
    println!(
        "{:<22}{:>10.1}   (iter().position, the naive search, for comparison)",
        "find_byte",
        N as f64 / secs / 1e9
    );
    let b = best();
    let (secs, v) = best_of(5, || (b.find_byte)(&ascii, 0xC3));
    assert_eq!(v, None);
    println!(
        "{:<22}{:>10.1}   (through the OnceLock dispatch table: '{}')",
        "find_byte dispatched",
        N as f64 / secs / 1e9,
        b.name
    );
}

fn main() {
    println!("arch = {}", std::env::consts::ARCH);
    #[cfg(target_arch = "x86_64")]
    println!(
        "avx2={} fma={} sse4.2={} popcnt={}",
        is_x86_feature_detected!("avx2"),
        is_x86_feature_detected!("fma"),
        is_x86_feature_detected!("sse4.2"),
        is_x86_feature_detected!("popcnt")
    );
    #[cfg(target_arch = "aarch64")]
    println!(
        "neon={} (baseline on AArch64) dotprod={}",
        std::arch::is_aarch64_feature_detected!("neon"),
        std::arch::is_aarch64_feature_detected!("dotprod")
    );
    let vars = variants();
    println!(
        "variants: {:?}; dispatch table picked: {}",
        vars.iter().map(|k| k.name).collect::<Vec<_>>(),
        best().name
    );

    // tiny demo of the horizontal-reduction / movemask ideas
    let demo = b"hello, simd world! find the X here";
    println!(
        "demo: find 'X' in {:?} -> {:?}; sum = {}; max = {}; popcount = {}",
        std::str::from_utf8(demo).unwrap(),
        (best().find_byte)(demo, b'X'),
        (best().sum_u8)(demo),
        (best().max_u8)(demo) as char,
        (best().popcount)(demo)
    );
    #[cfg(target_arch = "aarch64")]
    {
        use std::arch::aarch64::*;
        let mut bytes = [0u8; 16];
        bytes[3] = 9;
        bytes[10] = 9;
        // SAFETY: `bytes` is 16 readable bytes; vld1q_u8 has no alignment requirement.
        let v = unsafe { vld1q_u8(bytes.as_ptr()) };
        // SAFETY: NEON is mandatory on AArch64.
        let mask = unsafe { neon::nibble_mask(vceqq_u8(v, vdupq_n_u8(9))) };
        println!(
            "NEON movemask emulation: matches at bytes 3 and 10 -> mask {mask:#018x}; first = tz/4 = {}",
            mask.trailing_zeros() / 4
        );
        assert_eq!(mask, 0x0000_0f00_0000_f000);
    }

    run_tests(&vars);
    bench(&vars);
    println!(
        "\nstd::simd is unstable on stable rustc 1.98 (E0658 portable_simd); use std::arch (above), `wide`, `pulp` or `memchr`."
    );
}
