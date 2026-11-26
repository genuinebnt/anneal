//! RECIPE 1027 (std): integer methods look-up page: overflow families, division, bits, bytes, `as` casts, NonZero.
//! LEVEL: intermediate
//! Use in: money and quota arithmetic, pagination, timeouts and backoff, ID packing, binary formats, config parsing.   Run: cargo run --release --example std_integer_methods_overflow
//! COMMON USES: offset = page * size from user input, exponential backoff, packing (shard, sequence) into a u64 ID,
//! big-endian wire headers, ring-buffer index maths, "how many batches do I need".
//! METHODS: (everyday first) i32::checked_add, u32::checked_sub, u64::checked_mul, u32::checked_div, u32::saturating_sub, u32::saturating_add, u64::saturating_mul,
//! METHODS: u32::wrapping_add, u32::wrapping_mul, u32::overflowing_add, u32::pow, u32::checked_pow, u32::abs_diff, i32::abs, i32::checked_abs, i32::unsigned_abs,
//! METHODS: i32::signum, i32::rem_euclid, i32::div_euclid, u32::div_ceil, u32::next_multiple_of, u32::is_multiple_of, u32::isqrt, i32::checked_isqrt,
//! METHODS: u32::ilog, u32::ilog2, u32::ilog10, u32::checked_ilog10, u32::midpoint, i32::midpoint, u32::leading_zeros, u32::trailing_zeros, u32::count_ones,
//! METHODS: u32::count_zeros, u32::rotate_left, u32::rotate_right, u32::swap_bytes, u32::reverse_bits, u32::to_be_bytes, u32::to_le_bytes, u32::from_be_bytes,
//! METHODS: u32::from_le_bytes, u32::from_str_radix, u32::MIN, u32::MAX, u32::BITS, u32::is_power_of_two, u32::next_power_of_two,
//! METHODS: u32::checked_next_power_of_two, u32::cast_signed, i32::cast_unsigned, u32::checked_add_signed, u32::checked_signed_diff,
//! METHODS: u32::strict_add, u32::strict_sub, u32::strict_mul, u32::checked_shl, u32::wrapping_shl, u32::unbounded_shl, u32::overflowing_shl,
//! METHODS: u32::checked_rem, i32::wrapping_neg, i32::checked_neg, i32::overflowing_neg, i32::saturating_neg, i32::saturating_abs, i32::is_negative,
//! METHODS: u32::try_from, u64::from, std::num::NonZero, std::num::NonZeroU32, std::num::Wrapping, std::num::Saturating, std::num::IntErrorKind,
//! METHODS: std::num::ParseIntError, std::num::TryFromIntError
//! DOCS: https://doc.rust-lang.org/std/primitive.i32.html
//! DEFINED IN: core (`core::num`; the primitive methods, NonZero, Wrapping, Saturating, IntErrorKind), re-exported by std.
//! NOT COVERED (and why): not every family x operation pair is written out (for example checked_shr, wrapping_div,
//! saturating_pow): they follow the shapes shown. unchecked_* (unsafe), carrying_*/borrowing_*/carryless_mul (multi-word arithmetic), the to_ne/from_ne/to_be/from_le
//! variants (same shape as shown), leading_ones/trailing_ones, highest_one/lowest_one/isolate_* (bit tricks), bit_width, *_exact_div,
//! the *_signed/_unsigned mixed-sign twins other than checked_add_signed; signed div_ceil, widening_mul and checked_cast_signed
//! (UNSTABLE on signed types / u64 in 1.98.1: widening is shown through u128).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   detect overflow and fail                 -> checked_*  (returns Option)
//!   clamp at the type's limits               -> saturating_*
//!   modular arithmetic (hashes, sequences)   -> wrapping_*
//!   result AND a "did it overflow" flag      -> overflowing_*
//!   overflow is a bug: panic in every build  -> strict_*  (since 1.91)
//!   distance between two unsigned values     -> abs_diff
//!   non-negative modulo with negative input  -> rem_euclid
//!   pages needed for n items                 -> n.div_ceil(per_page)
//!   round up to alignment                    -> next_multiple_of
//!   average without overflow                 -> midpoint
//!   number of decimal digits - 1             -> ilog10
//!   narrow a type safely                     -> u32::try_from(x)
//!   reinterpret bits as signed/unsigned      -> cast_signed / cast_unsigned
//!   wire format                              -> to_be_bytes / from_be_bytes
//!   "never zero" in the type                 -> NonZeroU32 (Option<NonZeroU32> is 4 bytes)
//!
//! OVERFLOW RULES. Plain `+ - *` that overflows is a bug: with `overflow-checks` on (default in dev/test) it panics; with it off
//! (default in release) it wraps in two's complement. Never rely on either; pick a method family. Division and remainder are
//! different: `/ 0`, `% 0` and `MIN / -1` panic in EVERY build. This page asserts nothing that depends on the build profile.
//!
//! Read: https://doc.rust-lang.org/std/primitive.i32.html , https://doc.rust-lang.org/reference/expressions/operator-expr.html (Overflow; Numeric cast),
//!       https://doc.rust-lang.org/std/num/index.html
//! TRY NEXT:
//!   - [S] In `page_bounds` replace checked_mul with `*` and call it with page = u64::MAX under `cargo run` and `--release`. Predict both.
//!   - [S] Change `Backoff::delay` to use wrapping_shl and find the first attempt number that gives a nonsense delay.
//!   - [M] Extend `Id` to (timestamp 41 bits, shard 10, seq 12) and write a differential test of pack/unpack on random inputs.
//!   - [M] Enable `overflow-checks = true` for the release profile in a scratch crate and measure a summing loop with and without.
//!   - [A] PROJECT: a binary record reader: from_be_bytes (this file) over slice chunks (std_slice_chunks_windows_split), with every
//!         length field validated by checked arithmetic and errors as a typed enum (std_result_methods).
use std::collections::HashSet;
use std::num::{IntErrorKind, NonZero, NonZeroU32, Saturating, Wrapping};

const COVERED: &[&str] = &[
    "checked_add",
    "checked_sub",
    "checked_mul",
    "checked_div",
    "saturating_sub",
    "saturating_add",
    "saturating_mul",
    "wrapping_add",
    "wrapping_mul",
    "overflowing_add",
    "pow",
    "checked_pow",
    "abs_diff",
    "abs",
    "checked_abs",
    "unsigned_abs",
    "signum",
    "rem_euclid",
    "div_euclid",
    "div_ceil",
    "next_multiple_of",
    "is_multiple_of",
    "isqrt",
    "checked_isqrt",
    "ilog",
    "ilog2",
    "ilog10",
    "checked_ilog10",
    "midpoint",
    "leading_zeros",
    "trailing_zeros",
    "count_ones",
    "count_zeros",
    "rotate_left",
    "rotate_right",
    "swap_bytes",
    "reverse_bits",
    "to_be_bytes",
    "to_le_bytes",
    "from_be_bytes",
    "from_le_bytes",
    "from_str_radix",
    "MIN",
    "MAX",
    "BITS",
    "is_power_of_two",
    "next_power_of_two",
    "checked_next_power_of_two",
    "cast_signed",
    "cast_unsigned",
    "checked_add_signed",
    "checked_signed_diff",
    "strict_add",
    "strict_sub",
    "strict_mul",
    "checked_shl",
    "wrapping_shl",
    "unbounded_shl",
    "overflowing_shl",
    "checked_rem",
    "wrapping_neg",
    "checked_neg",
    "overflowing_neg",
    "saturating_neg",
    "saturating_abs",
    "is_negative",
    "try_from",
    "from",
];

fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

// ---- 1. Everyday core: arithmetic on untrusted numbers ---------------------------------------------------------
/// SQL-style pagination from query parameters. Returns None instead of wrapping to a nonsense offset.
fn page_bounds(page: u64, size: u64, total: u64) -> Option<(u64, u64)> {
    if size == 0 || size > 1000 {
        return None;
    }
    let start = page.checked_mul(size)?;
    let start = start.min(total);
    let end = start.saturating_add(size).min(total);
    Some((start, end))
}

fn section_everyday() {
    assert_eq!(page_bounds(2, 10, 25), Some((20, 25)));
    assert_eq!(page_bounds(9, 10, 25), Some((25, 25)));
    assert_eq!(page_bounds(u64::MAX, 10, 25), None);
    assert_eq!(page_bounds(0, 0, 25), None);

    assert_eq!(250u8.checked_add(5), Some(255));
    assert_eq!(250u8.checked_add(6), None);
    assert_eq!(3u32.checked_sub(4), None);
    assert_eq!(200u8.checked_mul(2), None);
    assert_eq!((10u32.checked_div(0), 10u32.checked_rem(0)), (None, None));
    assert_eq!(
        (i32::MIN.checked_div(-1), i32::MIN.checked_rem(-1)),
        (None, None)
    );

    // saturating: "remaining quota" and "time left" should stop at 0, not underflow.
    let (limit, used) = (100u32, 130u32);
    assert_eq!(limit.saturating_sub(used), 0);
    assert_eq!(u32::MAX.saturating_add(1), u32::MAX);
    assert_eq!(i32::MIN.saturating_add(-1), i32::MIN); // signed saturates at BOTH ends
    assert_eq!(u64::MAX.saturating_mul(2), u64::MAX);

    // abs_diff never underflows, whatever the order; it returns the UNSIGNED type even for signed inputs.
    assert_eq!(3u32.abs_diff(10), 7);
    let widest: u32 = i32::MIN.abs_diff(i32::MAX);
    assert_eq!(widest, u32::MAX);

    // div_ceil (unsigned, since 1.73): batches needed. The hand-written (n + k - 1) / k can overflow.
    assert_eq!(
        (25u32.div_ceil(10), 20u32.div_ceil(10), 0u32.div_ceil(10)),
        (3, 2, 0)
    );
    assert_eq!(u32::MAX.div_ceil(2), 1 << 31);
    assert!(panics(|| {
        std::hint::black_box(std::hint::black_box(1u32).div_ceil(std::hint::black_box(0)));
    }));

    // min/max/clamp come from Ord.
    assert_eq!(5000u32.clamp(1, 1000), 1000);
    println!("1. everyday arithmetic ok");
}

// ---- 2. The four families side by side -------------------------------------------------------------------------
fn section_families() {
    let (a, b) = (250u8, 10u8);
    assert_eq!(a.checked_add(b), None);
    assert_eq!(a.saturating_add(b), 255);
    assert_eq!(a.wrapping_add(b), 4);
    assert_eq!(a.overflowing_add(b), (4, true));
    // strict_* (since 1.91): panic on overflow in debug AND release.
    assert_eq!(
        (250u8.strict_add(5), 5u8.strict_sub(5), 16u8.strict_mul(15)),
        (255, 0, 240)
    );
    assert!(panics(|| {
        std::hint::black_box(std::hint::black_box(250u8).strict_add(10));
    }));
    // wrapping_mul: the core of multiplicative hashes (here FNV-1a, checked against its published test value).
    let fnv = b"a".iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &c| {
        (h ^ u64::from(c)).wrapping_mul(0x100_0000_01b3)
    });
    assert_eq!(fnv, 0xaf63_dc4c_8601_ec8c);

    // Wrapping<T> / Saturating<T>: apply one family to every operator in an expression.
    let mut seq = Wrapping(u32::MAX);
    seq += Wrapping(2);
    assert_eq!(seq.0, 1);
    let mut gauge = Saturating(250u8);
    gauge += Saturating(100);
    gauge -= Saturating(3);
    assert_eq!(gauge.0, 252);

    // Wrap-around sequence comparison (TCP style): "is a newer than b" when counters may wrap.
    let newer = |a: u32, b: u32| a.wrapping_sub(b).cast_signed() > 0;
    assert!(newer(3, u32::MAX - 2) && !newer(u32::MAX - 2, 3));

    // Negation and abs: only MIN is special, because -MIN does not fit.
    let m = i32::MIN;
    assert_eq!((m.checked_neg(), m.checked_abs()), (None, None));
    assert_eq!((m.wrapping_neg(), m.overflowing_neg()), (m, (m, true)));
    assert_eq!(
        (m.saturating_neg(), m.saturating_abs()),
        (i32::MAX, i32::MAX)
    );
    assert_eq!(m.unsigned_abs(), 2_147_483_648u32); // always correct: use this for magnitudes
    assert_eq!(
        (
            (-7i32).abs(),
            (-7i32).signum(),
            0i32.signum(),
            7i32.signum()
        ),
        (7, -1, 0, 1)
    );
    assert!((-1i32).is_negative() && !0i32.is_negative());

    // pow: plain pow follows the overflow rules above; checked_pow reports it.
    assert_eq!(3u32.pow(4), 81);
    assert_eq!(
        (2u32.checked_pow(31), 2u32.checked_pow(32)),
        (Some(1 << 31), None)
    );

    // Shifts: the amount is what overflows. checked_shl does NOT report bits pushed off the top.
    assert_eq!(1u32.checked_shl(32), None);
    assert_eq!(3u32.checked_shl(31), Some(1 << 31)); // one bit lost, still Some
    assert_eq!(1u32.wrapping_shl(33), 2); // amount is masked: 33 % 32 = 1
    assert_eq!(1u32.overflowing_shl(33), (2, true));
    assert_eq!(1u32.unbounded_shl(33), 0); // since 1.87: mathematically "shifted out"
    println!("2. overflow families ok");
}

/// Capped exponential backoff that cannot overflow for any attempt number.
struct Backoff {
    base_ms: u64,
    cap_ms: u64,
}
impl Backoff {
    fn delay(&self, attempt: u32) -> u64 {
        let factor = 2u64.checked_pow(attempt).unwrap_or(u64::MAX);
        self.base_ms.saturating_mul(factor).min(self.cap_ms)
    }
}

// ---- 3. Division, remainder, rounding --------------------------------------------------------------------------
fn section_division() {
    let b = Backoff {
        base_ms: 100,
        cap_ms: 30_000,
    };
    assert_eq!(
        [b.delay(0), b.delay(3), b.delay(20), b.delay(u32::MAX)],
        [100, 800, 30_000, 30_000]
    );

    // `/` truncates toward zero and `%` takes the sign of the dividend.
    assert_eq!((-7 / 4, -7 % 4), (-1, -3));
    // Euclidean versions: the remainder is always in 0..|d|.
    assert_eq!(((-7i32).div_euclid(4), (-7i32).rem_euclid(4)), (-2, 1));
    // Ring index from a possibly negative offset.
    let ring = ['a', 'b', 'c', 'd'];
    let at = |i: i64| ring[i.rem_euclid(ring.len() as i64) as usize];
    assert_eq!((at(-1), at(4), at(-6)), ('d', 'a', 'c'));
    for n in -40i32..=40 {
        for d in (-7i32..=7).filter(|d| *d != 0) {
            let (q, r) = (n.div_euclid(d), n.rem_euclid(d));
            assert!(q * d + r == n && r >= 0 && r < d.abs(), "{n} {d}");
        }
    }
    assert!(panics(|| {
        std::hint::black_box(std::hint::black_box(i32::MIN) / std::hint::black_box(-1));
    }));

    // next_multiple_of (unsigned, since 1.73) rounds UP to a multiple; panics if the argument is 0.
    assert_eq!(
        (
            13u32.next_multiple_of(8),
            16u32.next_multiple_of(8),
            0u32.next_multiple_of(8)
        ),
        (16, 16, 0)
    );
    assert!(16u32.is_multiple_of(8) && !13u32.is_multiple_of(8)); // since 1.87
    assert!(0u32.is_multiple_of(0) && !1u32.is_multiple_of(0)); // never panics

    // midpoint (unsigned since 1.85, signed since 1.87): (a+b)/2 with no overflow, rounded toward zero.
    assert_eq!(u32::MAX.midpoint(u32::MAX - 2), u32::MAX - 1);
    assert_eq!(((-7i32).midpoint(0), 7i32.midpoint(0)), (-3, 3));

    // isqrt (since 1.84) and integer logs: exact, unlike going through f64 for large values.
    assert_eq!(
        (24u32.isqrt(), 25u32.isqrt(), u64::MAX.isqrt()),
        (4, 5, u64::from(u32::MAX))
    );
    assert_eq!(
        ((-1i32).checked_isqrt(), 9i32.checked_isqrt()),
        (None, Some(3))
    );
    assert_eq!(
        (
            1024u32.ilog2(),
            999u32.ilog10(),
            1000u32.ilog10(),
            80u32.ilog(3)
        ),
        (10, 2, 3, 3)
    );
    let digits = |n: u64| n.checked_ilog10().map_or(1, |d| d + 1);
    assert_eq!(
        [digits(0), digits(9), digits(10), digits(u64::MAX)],
        [1, 1, 2, 20]
    );
    assert!(panics(|| {
        std::hint::black_box(std::hint::black_box(0u32).ilog10()); // log of zero
    }));
    println!("3. division, rounding, roots, logs ok");
}

// ---- 4. Bits and bytes -----------------------------------------------------------------------------------------
/// 64-bit ID: 16 bits of shard, 48 bits of sequence.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Id(u64);
impl Id {
    const SEQ_BITS: u32 = 48;
    const SEQ_MASK: u64 = (1 << Self::SEQ_BITS) - 1;
    fn new(shard: u16, seq: u64) -> Option<Id> {
        (seq <= Self::SEQ_MASK).then(|| Id((u64::from(shard) << Self::SEQ_BITS) | seq))
    }
    fn parts(self) -> (u16, u64) {
        ((self.0 >> Self::SEQ_BITS) as u16, self.0 & Self::SEQ_MASK)
    }
}

fn section_bits() {
    let id = Id::new(7, 12345).unwrap();
    assert_eq!(id.parts(), (7, 12345));
    assert_eq!(
        Id::new(1, 1 << 48),
        None,
        "reject instead of corrupting the shard field"
    );
    // Big-endian bytes sort like the numbers: good for ordered KV store keys.
    assert!(Id::new(1, 999).unwrap().0.to_be_bytes() < Id::new(2, 0).unwrap().0.to_be_bytes());

    let x = 0x1234_5678u32;
    assert_eq!(x.to_be_bytes(), [0x12, 0x34, 0x56, 0x78]);
    assert_eq!(x.to_le_bytes(), [0x78, 0x56, 0x34, 0x12]);
    assert_eq!(u32::from_be_bytes([0x12, 0x34, 0x56, 0x78]), x);
    assert_eq!(u32::from_le_bytes([0x78, 0x56, 0x34, 0x12]), x);
    // Reading from a longer buffer: slice -> array via first_chunk, so the length is checked once.
    let frame: &[u8] = &[0, 0, 1, 0, 0xFF];
    assert_eq!(
        frame.first_chunk::<4>().map(|a| u32::from_be_bytes(*a)),
        Some(256)
    );
    assert_eq!([1u8, 2].first_chunk::<4>(), None);

    assert_eq!(
        (
            1u32.leading_zeros(),
            8u32.trailing_zeros(),
            0u32.trailing_zeros()
        ),
        (31, 3, 32)
    );
    assert_eq!((0b1011u8.count_ones(), 0b1011u8.count_zeros()), (3, 5));
    assert_eq!((0x80u8.rotate_left(1), 1u8.rotate_right(1)), (1, 0x80));
    assert_eq!(
        (x.swap_bytes(), 0b0000_0001u8.reverse_bits()),
        (0x7856_3412, 0b1000_0000)
    );
    assert_eq!(
        (u32::BITS, u8::MIN, u8::MAX, i8::MIN, i8::MAX),
        (32, 0, 255, -128, 127)
    );
    // Bit width of a value: BITS - leading_zeros.
    assert_eq!(u32::BITS - 255u32.leading_zeros(), 8);

    // Powers of two: ring buffers use `i & (cap - 1)` when cap is a power of two.
    assert!(64u32.is_power_of_two() && !0u32.is_power_of_two() && !65u32.is_power_of_two());
    assert_eq!(
        (
            0u32.next_power_of_two(),
            1u32.next_power_of_two(),
            65u32.next_power_of_two()
        ),
        (1, 1, 128)
    );
    // PITFALL: above 2^31, u32::next_power_of_two panics with overflow checks and returns 0 without them.
    assert_eq!(((1u32 << 31) + 1).checked_next_power_of_two(), None);
    println!("4. bits and bytes ok");
}

// ---- 5. Conversions: From, TryFrom, `as` -----------------------------------------------------------------------
fn section_casts() {
    // Lossless widening has From. It documents (and the compiler proves) that nothing is lost.
    assert_eq!(u64::from(u32::MAX), 4_294_967_295);
    assert_eq!(i64::from(-1i32), -1);
    // Anything that can fail has TryFrom.
    let too_big: Result<u8, std::num::TryFromIntError> = u8::try_from(256u32);
    assert!(too_big.is_err() && u32::try_from(-1i32).is_err());
    assert_eq!(u8::try_from(255u32), Ok(255u8));
    // usize is 16, 32 or 64 bits depending on the target: there is no From<u64> for usize.
    assert_eq!(usize::try_from(10u64), Ok(10));
    assert!(usize::BITS == 32 || usize::BITS == 64 || usize::BITS == 16);

    // `as` NEVER fails. Table of what it does.
    // int -> narrower int: keep the low bits.
    assert_eq!((300u32 as u8, 0x1_0000_0001u64 as u32), (44, 1));
    // same width, other signedness: same bits.
    assert_eq!((-1i32 as u32, u32::MAX as i32), (u32::MAX, -1));
    // widening: sign- or zero-extension is decided by the SOURCE type.
    assert_eq!(
        (-1i8 as u32, 255u8 as i32, -1i8 as i16, 0xFFu8 as i8 as i32),
        (u32::MAX, 255, -1, -1)
    );
    // float -> int: truncates toward zero, saturates at the bounds, NaN becomes 0.
    assert_eq!(
        (
            3.9f64 as i32,
            -3.9f64 as i32,
            1e20f64 as i32,
            -1.5f64 as u8,
            f64::NAN as i32
        ),
        (3, -3, i32::MAX, 0, 0)
    );
    assert_eq!(f64::INFINITY as u64, u64::MAX);
    // int -> float: rounds to nearest (ties to even) when not representable.
    assert_eq!(16_777_217u32 as f32, 16_777_216.0);
    assert_eq!(((1u64 << 53) + 1) as f64, (1u64 << 53) as f64);

    // cast_signed / cast_unsigned (since 1.87): "same bits, other sign", without letting `as` silently change width too.
    assert_eq!(
        (u32::MAX.cast_signed(), (-1i32).cast_unsigned()),
        (-1, u32::MAX)
    );
    // Mixed sign arithmetic without casts.
    assert_eq!(
        (10u32.checked_add_signed(-3), 2u32.checked_add_signed(-3)),
        (Some(7), None)
    );
    assert_eq!(3u32.checked_signed_diff(10), Some(-7)); // since 1.91

    // Widening multiply through the next size up (u64::widening_mul is unstable).
    let wide = u128::from(u64::MAX) * u128::from(u64::MAX);
    assert_eq!((wide >> 64) as u64, u64::MAX - 1);
    assert_eq!(i128::MAX.checked_mul(2), None);
    println!("5. conversions ok");
}

// ---- 6. Parsing ------------------------------------------------------------------------------------------------
#[derive(Debug, PartialEq)]
enum PortError {
    Missing,
    NotANumber,
    OutOfRange,
}
fn parse_port(s: &str) -> Result<NonZero<u16>, PortError> {
    match s.parse::<NonZero<u16>>() {
        Ok(p) => Ok(p),
        Err(e) => Err(match e.kind() {
            IntErrorKind::Empty => PortError::Missing,
            IntErrorKind::InvalidDigit => PortError::NotANumber,
            IntErrorKind::PosOverflow | IntErrorKind::NegOverflow | IntErrorKind::Zero => {
                PortError::OutOfRange
            }
            _ => PortError::NotANumber, // IntErrorKind is #[non_exhaustive]
        }),
    }
}

fn section_parse() {
    assert_eq!(parse_port("8080").map(NonZero::get), Ok(8080));
    assert_eq!(parse_port(""), Err(PortError::Missing));
    assert_eq!(parse_port("80a"), Err(PortError::NotANumber));
    assert_eq!(parse_port(" 80"), Err(PortError::NotANumber)); // parse does NOT trim
    assert_eq!(parse_port("70000"), Err(PortError::OutOfRange));
    assert_eq!(parse_port("0"), Err(PortError::OutOfRange));
    let bad: std::num::ParseIntError = "x".parse::<u8>().unwrap_err();
    assert_eq!(bad.kind(), &IntErrorKind::InvalidDigit);
    assert_eq!("+42".parse::<u8>(), Ok(42)); // a leading + is accepted
    assert_eq!(
        "-1".parse::<u8>().unwrap_err().kind(),
        &IntErrorKind::InvalidDigit
    ); // unsigned: '-' is just a bad digit
    assert_eq!(
        "-200".parse::<i8>().unwrap_err().kind(),
        &IntErrorKind::NegOverflow
    );

    // from_str_radix: radix 2..=36, no "0x" prefix; panics for a radix outside that range.
    assert_eq!(u32::from_str_radix("ff", 16), Ok(255));
    assert_eq!(i32::from_str_radix("-101", 2), Ok(-5));
    assert!(u32::from_str_radix("0xff", 16).is_err());
    assert!(panics(|| {
        let _ = u32::from_str_radix("1", std::hint::black_box(37));
    }));
    let with_prefix = |s: &str| match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    };
    assert_eq!((with_prefix("0x1F"), with_prefix("31")), (Ok(31), Ok(31)));
    println!("6. parsing ok");
}

// ---- 7. Less known: NonZero ------------------------------------------------------------------------------------
fn section_nonzero() {
    let n: NonZeroU32 = NonZeroU32::new(8).unwrap(); // NonZeroU32 = NonZero<u32> (generic form since 1.79)
    assert_eq!(NonZeroU32::new(0), None);
    // Division by a NonZero unsigned cannot panic.
    assert_eq!(100u32 / n, 12);
    assert_eq!(100u32 % n, 4);
    assert_eq!(
        (
            n.get(),
            n.saturating_mul(NonZeroU32::MAX).get(),
            n.checked_add(u32::MAX)
        ),
        (8, u32::MAX, None)
    );
    // The forbidden value is the niche for None.
    assert_eq!(size_of::<Option<NonZeroU32>>(), size_of::<u32>());
    assert!(size_of::<Option<u32>>() > size_of::<u32>());
    const BATCH: NonZeroU32 = NonZeroU32::new(500).unwrap(); // checked at compile time
    assert_eq!(1200u32.div_ceil(BATCH.get()), 3);
    println!("7. NonZero ok");
}

// ---- 8. Differential test against i128 -------------------------------------------------------------------------
fn section_differential() {
    let mut s = 0x1234_5678_9ABC_DEF1u64;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let (lo, hi) = (i128::from(i64::MIN), i128::from(i64::MAX));
    for i in 0..30_000 {
        // Mix extremes in, otherwise random i64 pairs almost never hit the edges.
        let pick =
            |r: u64| [i64::MIN, i64::MAX, -1, 0, 1, r as i64, (r % 1000) as i64][(r % 7) as usize];
        let (a, b) = if i % 2 == 0 {
            (next() as i64, next() as i64)
        } else {
            (pick(next()), pick(next()))
        };
        for (exact, checked, saturating, wrapping, overflowing) in [
            (
                i128::from(a) + i128::from(b),
                a.checked_add(b),
                a.saturating_add(b),
                a.wrapping_add(b),
                a.overflowing_add(b),
            ),
            (
                i128::from(a) - i128::from(b),
                a.checked_sub(b),
                a.saturating_sub(b),
                a.wrapping_sub(b),
                a.overflowing_sub(b),
            ),
            (
                i128::from(a) * i128::from(b),
                a.checked_mul(b),
                a.saturating_mul(b),
                a.wrapping_mul(b),
                a.overflowing_mul(b),
            ),
        ] {
            let fits = (lo..=hi).contains(&exact);
            assert_eq!(checked, fits.then_some(exact as i64));
            assert_eq!(saturating, exact.clamp(lo, hi) as i64);
            assert_eq!(wrapping, exact as i64);
            assert_eq!(overflowing, (exact as i64, !fits));
        }
        assert_eq!(a.midpoint(b), ((i128::from(a) + i128::from(b)) / 2) as i64);
    }
    println!("8. differential test against i128 ok (30000 pairs)");
}

// ---- 9. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Every number from a request, file or socket goes through checked_* before it sizes or indexes anything.
    let (count, width) = (u32::MAX, 16u32);
    assert_eq!(count.checked_mul(width), None);
    // 2. Timeouts and lengths: saturating_sub, so "already past the deadline" is 0 and not 584 million years.
    let (deadline, now) = (1_000u64, 1_500u64);
    assert_eq!(deadline.saturating_sub(now), 0);
    // 3. `as` hides bugs. Prefer From for widening and try_from for narrowing (clippy::cast_possible_truncation, cast_lossless: pedantic).
    let big: u64 = 5_000_000_000;
    assert_eq!(big as u32, 705_032_704);
    assert!(u32::try_from(big).is_err());
    // 4. Clamp instead of truncating when a limit is acceptable.
    assert_eq!(u32::try_from(big).unwrap_or(u32::MAX), u32::MAX);
    // 5. If wrapping is a BUG in your service, set `[profile.release] overflow-checks = true` so it panics loudly.
    // 6. Money: integer minor units (cents) in i64 with checked ops; never f64.
    let total_cents = [1999i64, 1, 250]
        .iter()
        .try_fold(0i64, |acc, x| acc.checked_add(*x));
    assert_eq!(total_cents, Some(2250));
    assert!((0.1f64 + 0.2 - 0.3).abs() > 0.0);
    // 7. `iter().sum()` follows the same overflow rules as `+`: fold with checked_add for untrusted data (above).
    // 8. Binary search midpoints: lo + (hi - lo) / 2, never (lo + hi) / 2 (clippy::manual_midpoint, pedantic).
    let (lo, hi) = (u32::MAX - 10, u32::MAX);
    assert_eq!(lo + (hi - lo) / 2, lo.midpoint(hi));
    // 9. Manual div_ceil is a bug source: clippy::manual_div_ceil.
    // 10. Newtype your IDs and units (`struct Millis(u64)`) so a seconds value cannot be passed as milliseconds.
    // 11. Store NonZeroU64 for database IDs that start at 1: Option<Id> costs nothing extra.
    assert_eq!(size_of::<Option<NonZero<u64>>>(), 8);
    // 12. Serialise u64/usize with an explicit width: never write usize to disk or the wire.
    assert_eq!(
        u64::try_from(vec![0u8; 3].len())
            .map(u64::to_le_bytes)
            .ok()
            .map(|b| b.len()),
        Some(8)
    );
    println!("9. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_families();
    section_division();
    section_bits();
    section_casts();
    section_parse();
    section_nonzero();
    section_differential();
    section_production_tips();
    println!(
        "covered {} integer method names; all asserts passed",
        COVERED.len()
    );
}
