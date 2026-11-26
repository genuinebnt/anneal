//! RECIPE 274 (bits): bit-twiddling toolbox (Hacker's Delight / TAOCP 7.1.3), every trick checked against std or a naive loop.
//! LEVEL: intermediate
//! Use in: bitsets, hashing, compression, spatial indexes, SIMD-within-a-register code, performance hot loops.   Run: cargo run --release --example bit_tricks
#![allow(dead_code)]
//!
//! Each trick below is implemented in the "classic" form, then asserted equal to the standard-library intrinsic or to
//! an obviously-correct loop on the edge cases 0, 1, MAX, powers of two, and thousands of seeded random inputs.
//! The tricks: lowest-set-bit tools, SWAR popcount / parity / bit reversal, smearing for next power of two, zero-byte
//! detection (word-at-a-time strlen), branchless min/max/abs/sign, Gosper's hack, Morton codes with PDEP/PEXT
//! semantics (and a Z-order range query), 8x8 bit-matrix transpose, carry-less multiply, Gray code, rank/select inside
//! a word, division by constants via multiply-high magic numbers, sign extension, and saturating arithmetic.
//!
//! WHAT LLVM ALREADY DOES: modern compilers recognise many of these idioms. `x & (x - 1)` becomes BLSR on x86 with
//! BMI1, a hand-written SWAR popcount is often replaced by a single POPCNT (or by the `ctpop` intrinsic lowered per
//! target), `x / 7` is always compiled into the multiply-high form shown here (never emit a real `div` for a constant),
//! and min/max/abs patterns become cmov or dedicated instructions. So: prefer `count_ones`, `leading_zeros`,
//! `reverse_bits`, `rotate_left`, `isqrt`, `div_ceil` and friends; they map to the best instruction per target. Write a
//! manual trick when std has no equivalent (Morton, bit-matrix transpose, SWAR on bytes) and CHECK the assembly:
//! `cargo install cargo-show-asm`, then `cargo asm --example bit_tricks <function>` (https://github.com/pacak/cargo-show-asm).
//! Use `std::hint::black_box` on inputs/outputs in micro-benchmarks, otherwise the optimiser folds your constant
//! inputs away and you time nothing (see the timing at the end of main).
//!
//! COMMON USES: bitset/bitmap indexes and roaring-style containers, hash functions and PRNGs, chess engines (bitboards),
//! spatial indexing with Z-order/Morton keys (databases, GPUs, quadtrees), fast string scanning (memchr-style
//! zero-byte tests), checksums (carry-less multiply drives CRC/GHASH hardware), compilers (division by constants).
//! Read: Henry S. Warren, Hacker's Delight (2nd ed.); Knuth, The Art of Computer Programming Vol 4A, section 7.1.3
//! (Bitwise Tricks and Techniques); https://graphics.stanford.edu/~seander/bithacks.html
//! https://github.com/pacak/cargo-show-asm  https://www-cs-faculty.stanford.edu/~knuth/taocp.html

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
}

/// Edge cases, powers of two, sparse and dense random words.
fn inputs() -> Vec<u64> {
    let mut v = vec![
        0,
        1,
        2,
        3,
        u64::MAX,
        u64::MAX - 1,
        1 << 63,
        (1 << 63) - 1,
        0x5555_5555_5555_5555,
        0xAAAA_AAAA_AAAA_AAAA,
    ];
    v.extend((0..64).map(|i| 1u64 << i));
    v.extend((0..64).map(|i| (1u64 << i).wrapping_sub(1)));
    let mut r = Rng(0x1234_5678_9abc_def1);
    for _ in 0..3000 {
        let x = r.next();
        v.push(x);
        v.push(x & r.next()); // sparse
        v.push(x | r.next()); // dense
    }
    v
}

// ---------------------------------------------------------------- lowest set bit
fn isolate_lowest(x: u64) -> u64 {
    x & x.wrapping_neg()
}
fn clear_lowest(x: u64) -> u64 {
    x & x.wrapping_sub(1)
}

// ---------------------------------------------------------------- popcount, parity, reversal
fn popcount_kernighan(mut x: u64) -> u32 {
    let mut c = 0;
    while x != 0 {
        x = clear_lowest(x); // each iteration removes exactly one set bit
        c += 1;
    }
    c
}
fn popcount_swar(mut x: u64) -> u32 {
    x -= (x >> 1) & 0x5555_5555_5555_5555; // pairs
    x = (x & 0x3333_3333_3333_3333) + ((x >> 2) & 0x3333_3333_3333_3333); // nibbles
    x = (x + (x >> 4)) & 0x0f0f_0f0f_0f0f_0f0f; // bytes
    (x.wrapping_mul(0x0101_0101_0101_0101) >> 56) as u32 // horizontal sum via multiply
}
fn parity(mut x: u64) -> u32 {
    for s in [32, 16, 8, 4, 2, 1] {
        x ^= x >> s;
    }
    (x & 1) as u32
}
fn reverse_bits(mut x: u64) -> u64 {
    x = (x >> 1 & 0x5555_5555_5555_5555) | (x & 0x5555_5555_5555_5555) << 1;
    x = (x >> 2 & 0x3333_3333_3333_3333) | (x & 0x3333_3333_3333_3333) << 2;
    x = (x >> 4 & 0x0f0f_0f0f_0f0f_0f0f) | (x & 0x0f0f_0f0f_0f0f_0f0f) << 4;
    x.swap_bytes() // the last three swap steps (8, 16, 32) are just a byte swap
}
fn reverse_bits_halves(mut x: u64) -> u64 {
    // literal "swap halves" recursion: 32, 16, 8, 4, 2, 1
    for (s, m) in [
        (32, 0x0000_0000_ffff_ffffu64),
        (16, 0x0000_ffff_0000_ffff),
        (8, 0x00ff_00ff_00ff_00ff),
        (4, 0x0f0f_0f0f_0f0f_0f0f),
        (2, 0x3333_3333_3333_3333),
        (1, 0x5555_5555_5555_5555),
    ] {
        x = (x >> s & m) | (x & m) << s;
    }
    x
}

// ---------------------------------------------------------------- powers of two
fn next_pow2(x: u64) -> u64 {
    let mut v = x.wrapping_sub(1);
    for s in [1, 2, 4, 8, 16, 32] {
        v |= v >> s; // smear the top set bit downwards
    }
    v.wrapping_add(1) + (x == 0) as u64 // x = 0 -> v = MAX -> 0 -> fix to 1; x > 2^63 wraps to 0
}
fn is_pow2(x: u64) -> bool {
    x != 0 && x & (x - 1) == 0
}

// ---------------------------------------------------------------- zero byte / strlen
const LO: u64 = 0x0101_0101_0101_0101;
const HI: u64 = 0x8080_8080_8080_8080;
fn haszero(v: u64) -> u64 {
    v.wrapping_sub(LO) & !v & HI
}
fn hasvalue(v: u64, byte: u8) -> u64 {
    haszero(v ^ (LO * byte as u64))
}
/// Word-at-a-time strlen over a byte slice (no unsafe: 8-byte chunks via from_le_bytes).
fn strlen_swar(s: &[u8]) -> usize {
    let mut i = 0;
    while i + 8 <= s.len() {
        let w = u64::from_le_bytes(s[i..i + 8].try_into().unwrap());
        let z = haszero(w);
        if z != 0 {
            return i + (z.trailing_zeros() / 8) as usize; // lowest flag is always exact
        }
        i += 8;
    }
    while i < s.len() && s[i] != 0 {
        i += 1;
    }
    i
}

// ---------------------------------------------------------------- branchless arithmetic
fn min_bl(x: i64, y: i64) -> i64 {
    y ^ ((x ^ y) & -((x < y) as i64))
}
fn max_bl(x: i64, y: i64) -> i64 {
    x ^ ((x ^ y) & -((x < y) as i64))
}
fn abs_bl(x: i64) -> i64 {
    let m = x >> 63; // all ones if negative
    (x ^ m).wrapping_sub(m)
}
fn sign_bl(x: i64) -> i64 {
    (x >> 63) | ((x.wrapping_neg() as u64) >> 63) as i64
}

// ---------------------------------------------------------------- Gosper, Gray
fn gosper(x: u64) -> u64 {
    let c = x & x.wrapping_neg();
    let r = x + c;
    (((r ^ x) >> 2) / c) | r
}
fn gray(x: u64) -> u64 {
    x ^ (x >> 1)
}
fn gray_inv(mut g: u64) -> u64 {
    for s in [1, 2, 4, 8, 16, 32] {
        g ^= g >> s; // prefix XOR in log steps
    }
    g
}

// ---------------------------------------------------------------- Morton / PDEP / PEXT
fn spread_bits(mut x: u64) -> u64 {
    // 32 -> 64 bits: a zero between every bit, by magic masks
    x &= 0xffff_ffff;
    x = (x | x << 16) & 0x0000_ffff_0000_ffff;
    x = (x | x << 8) & 0x00ff_00ff_00ff_00ff;
    x = (x | x << 4) & 0x0f0f_0f0f_0f0f_0f0f;
    x = (x | x << 2) & 0x3333_3333_3333_3333;
    (x | x << 1) & 0x5555_5555_5555_5555
}
fn compact_bits(mut x: u64) -> u64 {
    x &= 0x5555_5555_5555_5555;
    x = (x | x >> 1) & 0x3333_3333_3333_3333;
    x = (x | x >> 2) & 0x0f0f_0f0f_0f0f_0f0f;
    x = (x | x >> 4) & 0x00ff_00ff_00ff_00ff;
    x = (x | x >> 8) & 0x0000_ffff_0000_ffff;
    (x | x >> 16) & 0xffff_ffff
}
fn morton(x: u32, y: u32) -> u64 {
    spread_bits(x as u64) | spread_bits(y as u64) << 1
}
fn unmorton(z: u64) -> (u32, u32) {
    (compact_bits(z) as u32, compact_bits(z >> 1) as u32)
}
/// Software PDEP: scatter the low bits of `src` into the set-bit positions of `mask`.
fn pdep(src: u64, mut mask: u64) -> u64 {
    let (mut out, mut k) = (0, 0);
    while mask != 0 {
        let low = isolate_lowest(mask);
        if src >> k & 1 == 1 {
            out |= low;
        }
        k += 1;
        mask = clear_lowest(mask);
    }
    out
}
/// Software PEXT: gather the bits of `src` at the set-bit positions of `mask` into the low bits.
fn pext(src: u64, mut mask: u64) -> u64 {
    let (mut out, mut k) = (0, 0);
    while mask != 0 {
        let low = isolate_lowest(mask);
        if src & low != 0 {
            out |= 1 << k;
        }
        k += 1;
        mask = clear_lowest(mask);
    }
    out
}

// ---------------------------------------------------------------- 8x8 bit matrix, clmul
/// Bit (8*row + col) moves to bit (8*col + row): three delta-swap steps.
fn transpose8x8(mut x: u64) -> u64 {
    let mut t = (x ^ (x >> 7)) & 0x00AA_00AA_00AA_00AA;
    x ^= t ^ (t << 7);
    t = (x ^ (x >> 14)) & 0x0000_CCCC_0000_CCCC;
    x ^= t ^ (t << 14);
    t = (x ^ (x >> 28)) & 0x0000_0000_F0F0_F0F0;
    x ^ t ^ (t << 28)
}
/// Carry-less (GF(2)[x]) multiply, 64x64 -> 128 bits: shift-and-XOR instead of shift-and-add.
fn clmul(a: u64, b: u64) -> u128 {
    let mut r = 0u128;
    let mut b = b;
    let mut shift = 0;
    while b != 0 {
        let tz = b.trailing_zeros();
        shift += tz;
        b >>= tz;
        r ^= (a as u128) << shift;
        b &= !1;
    }
    r
}

// ---------------------------------------------------------------- rank / select
fn rank(x: u64, i: u32) -> u32 {
    (if i >= 64 { x } else { x & ((1u64 << i) - 1) }).count_ones() // ones strictly below bit i
}
fn select(x: u64, k: u32) -> Option<u32> {
    if k >= x.count_ones() {
        return None;
    }
    Some(pdep(1 << k, x).trailing_zeros()) // PDEP deposits a single 1 at the k-th set bit
}

// ---------------------------------------------------------------- division by constants
/// Round-up magic (Granlund-Montgomery): q = (x * m) >> (32 + l), m = ceil(2^(32+l) / d), l = ceil(log2 d). Exact for all u32 x.
fn magic_u32(d: u32) -> (u128, u32) {
    assert!(d >= 2);
    let l = 32 - (d - 1).leading_zeros();
    ((1u128 << (32 + l)).div_ceil(d as u128), 32 + l)
}
fn div_magic(x: u32, (m, s): (u128, u32)) -> u32 {
    ((x as u128 * m) >> s) as u32
}

// ---------------------------------------------------------------- sign extension, saturation
fn sign_extend(x: u64, bits: u32) -> i64 {
    ((x << (64 - bits)) as i64) >> (64 - bits)
}
fn sat_add_u8(a: u8, b: u8) -> u8 {
    let s = a.wrapping_add(b);
    let carry = ((a & b) | ((a | b) & !s)) >> 7; // carry out of bit 7
    s | 0u8.wrapping_sub(carry) // all ones on overflow
}
fn sat_sub_u8(a: u8, b: u8) -> u8 {
    let d = a.wrapping_sub(b);
    let borrow = ((!a & b) | (!(a ^ b) & d)) >> 7;
    d & !0u8.wrapping_sub(borrow) // zero on underflow
}

fn main() {
    let xs = inputs();
    for &x in &xs {
        // lowest set bit
        assert_eq!(
            isolate_lowest(x),
            if x == 0 { 0 } else { 1 << x.trailing_zeros() }
        );
        assert_eq!(
            clear_lowest(x),
            if x == 0 {
                0
            } else {
                x ^ (1 << x.trailing_zeros())
            }
        );
        // popcount, parity, reversal
        assert_eq!(popcount_kernighan(x), x.count_ones());
        assert_eq!(popcount_swar(x), x.count_ones());
        assert_eq!(parity(x), x.count_ones() & 1);
        assert_eq!(reverse_bits(x), x.reverse_bits());
        assert_eq!(reverse_bits_halves(x), x.reverse_bits());
        // powers of two
        assert_eq!(is_pow2(x), x.is_power_of_two());
        assert_eq!(next_pow2(x), x.checked_next_power_of_two().unwrap_or(0));
        // zero-byte tests
        let has0 = x.to_le_bytes().contains(&0);
        assert_eq!(haszero(x) != 0, has0);
        for b in [0u8, 1, 0x80, 0xff, x as u8] {
            assert_eq!(hasvalue(x, b) != 0, x.to_le_bytes().contains(&b));
        }
        // Gray
        assert_eq!(gray_inv(gray(x)), x);
        assert!((gray(x) ^ gray(x.wrapping_add(1))).count_ones() == 1 || x == u64::MAX);
        // Morton: round trip and equal to PDEP of the interleave masks
        let (a, b) = (x as u32, (x >> 32) as u32);
        assert_eq!(unmorton(morton(a, b)), (a, b));
        assert_eq!(
            morton(a, b),
            pdep(a as u64, 0x5555_5555_5555_5555) | pdep(b as u64, 0xAAAA_AAAA_AAAA_AAAA)
        );
        assert_eq!(pext(morton(a, b), 0x5555_5555_5555_5555), a as u64);
        // transpose is an involution and moves bit (r,c) to (c,r)
        let t = transpose8x8(x);
        assert_eq!(transpose8x8(t), x);
        for (r, c) in [(0, 1), (3, 5), (7, 0), (2, 2), (6, 7)] {
            assert_eq!(t >> (8 * c + r) & 1, x >> (8 * r + c) & 1);
        }
        // sign extension
        for bits in [1u32, 5, 8, 13, 32, 63] {
            let field = x & ((1u64 << bits) - 1);
            let naive = if field >> (bits - 1) & 1 == 1 {
                field as i64 - (1i64 << bits)
            } else {
                field as i64
            };
            assert_eq!(sign_extend(field, bits), naive);
        }
        // rank / select
        for i in [0u32, 1, 17, 63] {
            let naive = (0..i).filter(|&j| x >> j & 1 == 1).count() as u32;
            assert_eq!(rank(x, i), naive);
        }
        let positions: Vec<u32> = (0..64).filter(|&j| x >> j & 1 == 1).collect();
        for k in [0u32, 1, 5, 63, 64] {
            assert_eq!(select(x, k), positions.get(k as usize).copied());
        }
        // branchless signed ops
        let (s, t) = (x as i64, (x.rotate_left(17) ^ 0x9e37) as i64);
        assert_eq!((min_bl(s, t), max_bl(s, t)), (s.min(t), s.max(t)));
        assert_eq!(abs_bl(s), s.wrapping_abs());
        assert_eq!(sign_bl(s), s.signum());
    }
    println!(
        "[verified] {} inputs through lowest-bit, popcount, parity, reverse, pow2, zero-byte, Gray, Morton, transpose, sign-extend, rank/select, min/max/abs/sign",
        xs.len()
    );

    // Gosper's hack against brute force
    for k in 1..=6u32 {
        let mut x = (1u64 << k) - 1;
        let mut seen = 0;
        while x < 1 << 12 {
            assert_eq!(x.count_ones(), k);
            seen += 1;
            let next = gosper(x);
            let brute = (x + 1..).find(|y| y.count_ones() == k).unwrap();
            assert_eq!(next, brute);
            x = next;
        }
        assert!(seen > 0);
    }

    // PDEP / PEXT identities on random masks
    let mut r = Rng(0xfeed_f00d_dead_beef);
    for _ in 0..2000 {
        let (src, mask) = (r.next(), r.next() & r.next());
        let packed = pext(src, mask);
        assert_eq!(
            pdep(packed, mask),
            src & mask,
            "pdep inverts pext on the masked bits"
        );
        assert!(mask.count_ones() == 64 || packed >> mask.count_ones() == 0);
    }

    // strlen word-at-a-time
    for len in 0..70usize {
        for pad in [0usize, 3, 9] {
            let mut buf: Vec<u8> = (0..len).map(|i| 1 + (i * 37 % 250) as u8).collect();
            buf.push(0);
            buf.extend(std::iter::repeat_n(7, pad));
            assert_eq!(strlen_swar(&buf), len);
        }
    }
    let no_nul = vec![9u8; 21];
    assert_eq!(strlen_swar(&no_nul), 21);

    // clmul: small known values and algebraic laws
    assert_eq!(clmul(0b11, 0b11), 0b101);
    assert_eq!(clmul(0b101, 0b11), 0b1111);
    for _ in 0..1000 {
        let (a, b, c) = (r.next(), r.next(), r.next());
        assert_eq!(clmul(a, b), clmul(b, a));
        assert_eq!(
            clmul(a ^ b, c),
            clmul(a, c) ^ clmul(b, c),
            "distributes over XOR"
        );
        assert_eq!(clmul(a, 1), a as u128);
        assert_eq!(clmul(a, 2), (a as u128) << 1);
        let naive = (0..64)
            .filter(|&i| b >> i & 1 == 1)
            .fold(0u128, |acc, i| acc ^ (a as u128) << i);
        assert_eq!(clmul(a, b), naive);
    }

    // 8x8 transpose on a recognisable matrix: the identity is its own transpose, a single row becomes a column
    assert_eq!(transpose8x8(0x8040_2010_0804_0201), 0x8040_2010_0804_0201);
    assert_eq!(transpose8x8(0xff), 0x0101_0101_0101_0101);

    // division by constants
    println!("[division by constants] magic multiplier and shift for x / d on u32:");
    let mut r2 = Rng(0xd1f1_d1f1_1234_4321);
    for d in [
        3u32,
        5,
        6,
        7,
        10,
        11,
        13,
        100,
        641,
        1000,
        65537,
        1_000_000_007,
        u32::MAX,
    ] {
        let magic = magic_u32(d);
        let mut cases: Vec<u32> = vec![0, 1, d - 1, d, d + 1, u32::MAX, u32::MAX - 1, u32::MAX / 2];
        cases.extend((0..2000).map(|_| r2.next() as u32));
        cases.extend((1..200).map(|k| d.wrapping_mul(k))); // exact multiples are the classic failure points
        for x in cases {
            assert_eq!(div_magic(x, magic), x / d, "{x} / {d}");
        }
        if [3, 7, 10, 100].contains(&d) {
            println!("   d={d:<4} m=0x{:x} shift={}", magic.0, magic.1);
        }
    }
    for x in 0..=u16::MAX as u32 * 4 {
        assert_eq!(div_magic(x, magic_u32(7)), x / 7);
    }

    // saturating arithmetic exhaustive over u8 pairs
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            assert_eq!(sat_add_u8(a, b), a.saturating_add(b));
            assert_eq!(sat_sub_u8(a, b), a.saturating_sub(b));
        }
    }

    // Z-order range query: sort points by Morton key, scan only the key interval of the query box
    let mut pr = Rng(0x2020_0202_abab_cdcd);
    let pts: Vec<(u32, u32)> = (0..20_000)
        .map(|_| ((pr.next() % 1024) as u32, (pr.next() % 1024) as u32))
        .collect();
    let mut keyed: Vec<(u64, (u32, u32))> = pts.iter().map(|&p| (morton(p.0, p.1), p)).collect();
    keyed.sort();
    for q in 0..50 {
        let (x0, y0) = ((pr.next() % 900) as u32, (pr.next() % 900) as u32);
        let (x1, y1) = (
            x0 + 1 + (pr.next() % (20 + q * 4)) as u32,
            y0 + 1 + (pr.next() % (20 + q * 4)) as u32,
        );
        let (lo, hi) = (morton(x0, y0), morton(x1, y1)); // monotone: every point in the box has a key in [lo, hi]
        let start = keyed.partition_point(|k| k.0 < lo);
        let end = keyed.partition_point(|k| k.0 <= hi);
        let mut hits: Vec<(u32, u32)> = keyed[start..end]
            .iter()
            .map(|k| k.1)
            .filter(|p| (x0..=x1).contains(&p.0) && (y0..=y1).contains(&p.1))
            .collect();
        let mut brute: Vec<(u32, u32)> = pts
            .iter()
            .cloned()
            .filter(|p| (x0..=x1).contains(&p.0) && (y0..=y1).contains(&p.1))
            .collect();
        hits.sort();
        brute.sort();
        assert_eq!(hits, brute);
        if q == 0 {
            println!(
                "[z-order] box ({x0},{y0})-({x1},{y1}): scanned {} of {} keys for {} hits",
                end - start,
                keyed.len(),
                hits.len()
            );
        }
    }

    // micro-benchmark with black_box (numbers vary by machine; nothing is asserted)
    let n = 20_000_000u64;
    let t0 = Instant::now();
    let mut acc = 0u64;
    for i in 0..n {
        acc += popcount_swar(black_box(i.wrapping_mul(0x9e37_79b9_7f4a_7c15))) as u64;
    }
    let swar = t0.elapsed();
    let t1 = Instant::now();
    let mut acc2 = 0u64;
    for i in 0..n {
        acc2 += black_box(i.wrapping_mul(0x9e37_79b9_7f4a_7c15)).count_ones() as u64;
    }
    let intr = t1.elapsed();
    assert_eq!(acc, acc2);
    println!(
        "[bench] {n} popcounts: SWAR {swar:?}, count_ones {intr:?} (LLVM may even turn the SWAR form into POPCNT)"
    );
    println!("all bit tricks verified");
}
