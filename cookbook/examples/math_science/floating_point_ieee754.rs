//! RECIPE 892 (math_science): IEEE 754 in depth: bit layouts, soft-float rounding, ulps, cancellation, summation, comparison, round-trip printing.
//! LEVEL: advanced
//! Use in: numeric code review, writing float tests, picking a summation algorithm, deciding when NOT to use floats (money).   Run: cargo run --release --example floating_point_ieee754
//! COMMON USES: tolerance-based float tests, reproducible sums in databases/analytics (Kahan/pairwise/exact), sorting floats (total order), serialising floats losslessly (JSON, protobuf text), GPU/ML low-precision formats (binary16, bfloat, 8-bit floats), compilers deciding which float optimisations are legal.
//! Course: Goldberg, "What Every Computer Scientist Should Know About Floating-Point Arithmetic"; Kahan's notes on IEEE 754.
#![allow(dead_code)]
//!
//! WHAT, in order of the demo:
//!   1. Decode/encode by hand for any (exponent bits, mantissa bits) layout and check against `to_bits`/`from_bits`
//!      on random bit patterns of f64 and f32 (including subnormals, infinities, NaNs).
//!   2. SOFT FLOAT: addition and multiplication built from integer arithmetic only, for any small layout, with all
//!      five rounding modes (nearest-even, nearest-away, toward zero, up, down). The 8-bit minifloat (1-4-3) is checked
//!      EXHAUSTIVELY (all 65536 operand pairs, all modes) against an independent reference: exact f64 arithmetic
//!      (exact because operands are tiny) followed by "pick the nearest representable value" from a table of all codes.
//!      binary16 (1-5-10) is checked on random pairs. Soft ADD aligns its operands exactly in an i128, so it refuses
//!      layouts with a wide exponent (bfloat16, f32, f64); soft multiply has no such limit. Ties-to-even is the default
//!      because on varied data ties go up and down about equally often, so they do not push a sum one way (a particular
//!      input can still: the demo adds the same tie again and again and nearest-even never moves).
//!   3. ulp, next_up/next_down by bit twiddling (compared with std), and a monotone integer key that turns
//!      float comparison into integer comparison; ulp distance; why "relative + absolute + ulps" tolerance tests.
//!   4. Catastrophic cancellation: (1-cos x)/x^2 and the quadratic formula, naive vs stable.
//!   5. FMA: exact product error `fma(a,b,-a*b)`, checked against Dekker's FMA-free two_prod; Kahan's accurate 2x2 determinant.
//!   6. Summation: naive, Kahan, Neumaier, pairwise, and exact (Shewchuk partials, as in Python's `math.fsum`),
//!      with the exact one verified against i128 integer arithmetic and the others ranked by measured error. The exact one
//!      PANICS on finite input when a partial sum overflows, also in cases where the exact total would fit.
//!   7. Comparison pitfalls: NaN != NaN, `total_cmp`, signed zero, `min`/`max` with NaN, floats as hash keys.
//!   8. Shortest round-trip printing (Rust's `{}`/`{:?}`): parse(print(x)) == x for random bit patterns and no
//!      shorter digit string round-trips; why 0.1 + 0.2 != 0.3; why money should use integers (see `decimal_fixed_point_money`).
//! WHY: a double is a 53-bit significand scaled by a power of two. Every rounding error is "half an ulp", errors
//! accumulate predictably, and almost every surprise (0.1+0.2, NaN keys, non-associative sums) follows from that.
//! Read: https://docs.oracle.com/cd/E19957-01/806-3568/ncg_goldberg.html (Goldberg) and https://en.wikipedia.org/wiki/IEEE_754 and
//! https://en.wikipedia.org/wiki/Kahan_summation_algorithm and https://www.h-schmidt.net/FloatConverter/IEEE754.html (Shewchuk's "Adaptive Precision Floating-Point Arithmetic" cited by title only, not read)
//! TRY NEXT:
//!   - [S] Add a rounding-mode `Stochastic` variant to `round_pack` driven by a seeded xorshift; predict the mean error of summing 0.1 (in minifloat) 1000 times for it vs nearest-even, then measure with `soft_add` in a loop.
//!   - [S] Predict which of `naive`, `kahan`, `neumaier`, `pairwise` loses when all inputs are positive and equal in magnitude, then change `summation_study` to generate that data and check.
//!   - [M] Add a bfloat16 `Layout` (1-8-7) and verify `soft_mul` against f32 arithmetic rounded by `ref_round` on 20000 random pairs; what breaks about subnormals?
//!   - [M] Implement `soft_fma` (one rounding for a*b+c) with the integer pipeline of `round_pack` and prove it differs from `soft_add(soft_mul(a,b),c)` on an exhaustive 8-bit search.
//!   - [A] PROJECT: combine with `parallel_prefix_scan_reduction`: make a parallel float sum that is bit-identical for any thread count by summing a fixed binary tree (or exact `fsum`) and assert equality across 1..8 threads; compare cost against Neumaier.
use std::cmp::Ordering;
use std::collections::HashMap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    /// uniform in [-1, 1)
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// Did `f` panic? (silently: these are the expected refusals)
fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f);
    std::panic::set_hook(hook);
    r.is_err()
}

// ============================================================ 1. layouts
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    ebits: u32,
    mbits: u32,
}
const F64: Layout = Layout {
    ebits: 11,
    mbits: 52,
};
const F32: Layout = Layout {
    ebits: 8,
    mbits: 23,
};
const F16: Layout = Layout {
    ebits: 5,
    mbits: 10,
};
const MINI8: Layout = Layout { ebits: 4, mbits: 3 };

impl Layout {
    fn bias(self) -> i32 {
        (1 << (self.ebits - 1)) - 1
    }
    fn total_bits(self) -> u32 {
        1 + self.ebits + self.mbits
    }
    fn exp_max_field(self) -> u64 {
        (1 << self.ebits) - 1
    }
    fn inf_code(self) -> u64 {
        self.exp_max_field() << self.mbits
    }
    fn nan_code(self) -> u64 {
        self.inf_code() | (1 << (self.mbits - 1))
    }
    fn emin(self) -> i32 {
        1 - self.bias()
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Class {
    Zero,
    Subnormal,
    Normal,
    Inf,
    Nan,
}

#[derive(Debug, Clone, Copy)]
struct Parts {
    sign: bool,
    exp_field: u64,
    frac: u64,
    class: Class,
}

fn decode(l: Layout, bits: u64) -> Parts {
    let frac = bits & ((1 << l.mbits) - 1);
    let exp_field = (bits >> l.mbits) & l.exp_max_field();
    let sign = bits >> (l.ebits + l.mbits) & 1 == 1;
    let class = match (exp_field, frac) {
        (0, 0) => Class::Zero,
        (0, _) => Class::Subnormal,
        (e, 0) if e == l.exp_max_field() => Class::Inf,
        (e, _) if e == l.exp_max_field() => Class::Nan,
        _ => Class::Normal,
    };
    Parts {
        sign,
        exp_field,
        frac,
        class,
    }
}

fn encode(l: Layout, p: &Parts) -> u64 {
    (p.sign as u64) << (l.ebits + l.mbits) | p.exp_field << l.mbits | p.frac
}

fn pow2(e: i32) -> f64 {
    // exact power of two for e in the normal f64 range
    assert!((-1022..=1023).contains(&e));
    f64::from_bits(((e + 1023) as u64) << 52)
}

/// sig * 2^e, exact when representable (every layout up to f64 is)
fn ldexp(sig: u64, e: i32) -> f64 {
    if e < -1000 {
        sig as f64 * pow2(e + 100) * pow2(-100) // first product is normal and exact; second lands exactly
    } else {
        sig as f64 * pow2(e)
    }
}

/// Exact value of a code as f64 (all layouts with <= 11 exponent bits and <= 52 mantissa bits fit).
fn value(l: Layout, bits: u64) -> f64 {
    let p = decode(l, bits);
    let mag = match p.class {
        Class::Zero => 0.0,
        Class::Subnormal => ldexp(p.frac, l.emin() - l.mbits as i32),
        Class::Normal => ldexp(
            (1 << l.mbits) | p.frac,
            p.exp_field as i32 - l.bias() - l.mbits as i32,
        ),
        Class::Inf => f64::INFINITY,
        Class::Nan => return f64::NAN,
    };
    if p.sign { -mag } else { mag }
}

fn check_layouts(rng: &mut Rng) {
    for _ in 0..20_000 {
        let bits = rng.next();
        let x = f64::from_bits(bits);
        let p = decode(F64, bits);
        assert_eq!(encode(F64, &p), bits, "encode(decode(bits)) == bits");
        let v = value(F64, bits);
        assert!(
            v.to_bits() == bits || (v.is_nan() && x.is_nan()),
            "hand decode == hardware for {bits:#x}"
        );
        assert_eq!(
            p.class == Class::Subnormal,
            x.classify() == std::num::FpCategory::Subnormal
        );
        assert_eq!(p.class == Class::Nan, x.is_nan());
        let b32 = (bits >> 32) as u32;
        let y = f32::from_bits(b32);
        let v32 = value(F32, b32 as u64);
        assert!(
            v32 == y as f64 || (v32.is_nan() && y.is_nan()),
            "f32 decode {b32:#x}"
        );
    }
    // special encodings
    assert_eq!(value(F64, 0x7FF0_0000_0000_0000), f64::INFINITY);
    assert_eq!(value(F64, 1), 5e-324); // smallest subnormal
    assert_eq!(value(F64, 0x0010_0000_0000_0000), f64::MIN_POSITIVE);
    assert_eq!(value(F16, 0x7BFF), 65504.0); // largest finite binary16
    assert_eq!(value(MINI8, 0x77), 240.0); // largest finite 1-4-3 value
    // binary16 codes are monotone in their bit pattern for positive values
    for c in 1..F16.inf_code() {
        assert!(value(F16, c) > value(F16, c - 1));
    }
    println!(
        "layouts: hand decode/encode == hardware on 20000 random f64 and f32 patterns; f16 monotone; 1-4-3 max = 240"
    );
    // gradual underflow: x - y == 0 only when x == y, because subnormals fill the gap around zero
    for _ in 0..2000 {
        let a = f64::from_bits(
            rng.next() & 0x000F_FFFF_FFFF_FFFF | 0x0010_0000_0000_0000 * (rng.next() % 3),
        );
        let b = f64::from_bits(
            rng.next() & 0x000F_FFFF_FFFF_FFFF | 0x0010_0000_0000_0000 * (rng.next() % 3),
        );
        assert_eq!(
            a - b == 0.0,
            a == b,
            "gradual underflow keeps x-y==0 <=> x==y"
        );
    }
}

// ============================================================ 2. soft float
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    NearestEven,
    NearestAway,
    TowardZero,
    Up,
    Down,
}
const MODES: [Mode; 5] = [
    Mode::NearestEven,
    Mode::NearestAway,
    Mode::TowardZero,
    Mode::Up,
    Mode::Down,
];

fn make(l: Layout, sign: bool, code: u64) -> u64 {
    (sign as u64) << (l.ebits + l.mbits) | code
}

/// Round `mag * 2^q` (an exact non-negative integer times a power of two) into layout `l`.
/// This is the heart of every FPU: keep m+1 significant bits, look at what was shifted out, decide by mode.
fn round_pack(l: Layout, sign: bool, mag: u128, q: i32, mode: Mode) -> u64 {
    if mag == 0 {
        return make(l, sign, 0);
    }
    let m = l.mbits as i32;
    let p = 127 - mag.leading_zeros() as i32; // index of the top set bit
    let e_top = p + q; // exponent of the leading bit
    let mut ulp_exp = e_top.max(l.emin()) - m; // weight of the last kept bit (subnormals share emin's ulp)
    let shift = ulp_exp - q;
    let (mut sig, inexact, cmp_half) = if shift <= 0 {
        (mag << (-shift), false, Ordering::Less)
    } else if shift > p + 2 {
        (0, true, Ordering::Less) // everything shifted out, far below half an ulp
    } else {
        let rem = mag & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        (mag >> shift, rem != 0, rem.cmp(&half))
    };
    let round_up = match mode {
        Mode::NearestEven => {
            cmp_half == Ordering::Greater || (cmp_half == Ordering::Equal && sig & 1 == 1)
        }
        Mode::NearestAway => cmp_half != Ordering::Less && inexact,
        Mode::TowardZero => false,
        Mode::Up => inexact && !sign,
        Mode::Down => inexact && sign,
    };
    if round_up {
        sig += 1;
    }
    if sig == 0 {
        return make(l, sign, 0);
    }
    if sig >= 1u128 << (m + 1) {
        sig >>= 1; // carried into a new binade; the dropped bit is 0
        ulp_exp += 1;
    }
    if sig < 1u128 << m {
        return make(l, sign, sig as u64); // subnormal
    }
    let e_field = ulp_exp + m + l.bias();
    if e_field >= l.exp_max_field() as i32 {
        let to_inf = match mode {
            Mode::NearestEven | Mode::NearestAway => true,
            Mode::TowardZero => false,
            Mode::Up => !sign,
            Mode::Down => sign,
        };
        return make(
            l,
            sign,
            if to_inf {
                l.inf_code()
            } else {
                l.inf_code() - 1
            },
        );
    }
    make(
        l,
        sign,
        (e_field as u64) << l.mbits | (sig as u64 & ((1 << m) - 1)),
    )
}

/// finite operand as (signed significand, exponent) with value = sig * 2^exp
fn unpack(l: Layout, p: &Parts) -> (i128, i32) {
    let m = l.mbits as i32;
    let (sig, e) = match p.class {
        Class::Zero => (0, l.emin() - m),
        Class::Subnormal => (p.frac as i128, l.emin() - m),
        _ => (
            ((1u64 << m) | p.frac) as i128,
            p.exp_field as i32 - l.bias() - m,
        ),
    };
    (if p.sign { -sig } else { sig }, e)
}

/// Only for layouts whose whole exponent range fits beside the significand in an i128 (up to 6 exponent bits always does).
fn soft_add(l: Layout, a: u64, b: u64, mode: Mode) -> u64 {
    assert!(
        l.exp_max_field() + u64::from(l.mbits) <= 125,
        "soft_add aligns exactly in an i128: {l:?} is too wide"
    );
    let (pa, pb) = (decode(l, a), decode(l, b));
    if pa.class == Class::Nan || pb.class == Class::Nan {
        return l.nan_code();
    }
    match (pa.class, pb.class) {
        (Class::Inf, Class::Inf) => return if pa.sign == pb.sign { a } else { l.nan_code() },
        (Class::Inf, _) => return a,
        (_, Class::Inf) => return b,
        _ => {}
    }
    let ((sa, ea), (sb, eb)) = (unpack(l, &pa), unpack(l, &pb));
    let q = ea.min(eb);
    let total = (sa << (ea - q)) + (sb << (eb - q));
    if total == 0 {
        // exact zero: same signs keep the sign, opposite signs give +0 (or -0 when rounding down)
        let sign = if pa.sign == pb.sign {
            pa.sign
        } else {
            mode == Mode::Down
        };
        return make(l, sign, 0);
    }
    round_pack(l, total < 0, total.unsigned_abs(), q, mode)
}

fn soft_mul(l: Layout, a: u64, b: u64, mode: Mode) -> u64 {
    let (pa, pb) = (decode(l, a), decode(l, b));
    let sign = pa.sign ^ pb.sign;
    if pa.class == Class::Nan || pb.class == Class::Nan {
        return l.nan_code();
    }
    let zero = |p: &Parts| p.class == Class::Zero;
    let inf = |p: &Parts| p.class == Class::Inf;
    if (inf(&pa) && zero(&pb)) || (zero(&pa) && inf(&pb)) {
        return l.nan_code();
    }
    if inf(&pa) || inf(&pb) {
        return make(l, sign, l.inf_code());
    }
    let ((sa, ea), (sb, eb)) = (unpack(l, &pa), unpack(l, &pb));
    round_pack(
        l,
        sign,
        (sa.unsigned_abs()) * (sb.unsigned_abs()),
        ea + eb,
        mode,
    )
}

/// Reference rounding with no shared code: table of every finite magnitude (code order == value order),
/// binary-search the neighbours of |x|, choose by mode. `inf_sentinel` stands in for "the value after max".
struct RefTable {
    l: Layout,
    mags: Vec<f64>, // index == code, last entry is the sentinel for infinity
}
impl RefTable {
    fn new(l: Layout) -> RefTable {
        let mut mags: Vec<f64> = (0..l.inf_code()).map(|c| value(l, c)).collect();
        let max = *mags.last().unwrap();
        let ulp = max - mags[mags.len() - 2];
        mags.push(max + ulp); // 2^(emax+1): the value an overflow would "round to" for tie purposes
        RefTable { l, mags }
    }
    fn round(&self, x: f64, mode: Mode) -> u64 {
        let l = self.l;
        let sign = x.is_sign_negative();
        let a = x.abs();
        let max_finite = l.inf_code() - 1;
        let pos = self.mags.partition_point(|&v| v <= a); // number of table values <= a
        let lo = (pos - 1).min(max_finite as usize);
        if self.mags[lo] == a {
            return make(l, sign, lo as u64);
        }
        let hi = lo + 1; // hi may be the infinity code
        let beyond = a >= self.mags[hi];
        let (dl, dh) = (a - self.mags[lo], self.mags[hi] - a);
        let pick_hi = match mode {
            Mode::TowardZero => false,
            Mode::Up => !sign,
            Mode::Down => sign,
            Mode::NearestAway => beyond || dh <= dl,
            Mode::NearestEven => beyond || dh < dl || (dh == dl && lo % 2 == 1),
        };
        make(l, sign, if pick_hi { hi as u64 } else { lo as u64 })
    }
}

fn same_code(l: Layout, got: u64, want: u64) -> bool {
    got == want || (decode(l, got).class == Class::Nan && decode(l, want).class == Class::Nan)
}

fn exhaustive_minifloat() {
    let l = MINI8;
    let table = RefTable::new(l);
    let mut checked = 0u64;
    for mode in MODES {
        for a in 0..256u64 {
            for b in 0..256u64 {
                let (x, y) = (value(l, a), value(l, b));
                // reference: exact f64 op (exact for 4-bit significands), then table rounding
                let mut sum = x + y;
                if sum == 0.0 && sum.is_finite() && !x.is_nan() && !y.is_nan() {
                    sum = if x.is_sign_negative() == y.is_sign_negative() {
                        x
                    } else if mode == Mode::Down {
                        -0.0
                    } else {
                        0.0
                    };
                }
                let want_add = if sum.is_nan() {
                    l.nan_code()
                } else if sum.is_infinite() {
                    make(l, sum < 0.0, l.inf_code())
                } else {
                    table.round(sum, mode)
                };
                let got_add = soft_add(l, a, b, mode);
                assert!(
                    same_code(l, got_add, want_add),
                    "add {a:#x}+{b:#x} mode {mode:?}: got {got_add:#x} want {want_add:#x}"
                );
                let prod = x * y;
                let want_mul = if prod.is_nan() {
                    l.nan_code()
                } else if prod.is_infinite() {
                    make(l, prod < 0.0, l.inf_code())
                } else {
                    table.round(prod, mode)
                };
                let got_mul = soft_mul(l, a, b, mode);
                assert!(
                    same_code(l, got_mul, want_mul),
                    "mul {a:#x}*{b:#x} mode {mode:?}: got {got_mul:#x} want {want_mul:#x}"
                );
                checked += 2;
            }
        }
    }
    println!(
        "minifloat 1-4-3: {checked} soft add/mul results (65536 pairs x 5 modes x 2 ops) equal the table reference"
    );
    // bfloat16 (1-8-7) needs far more than 128 bits to align exactly: refused, not silently wrong
    let bfloat16 = Layout { ebits: 8, mbits: 7 };
    assert!(panics(|| {
        soft_add(bfloat16, 127 << 7, 1, Mode::NearestEven);
    }));
    // tie-to-even in action: 1 + 2^-4 is a tie between 1.0 and 1.125 -> even mantissa (1.0); 1.125 + 2^-4 -> 1.25
    let one = 0x38; // exp field 7, frac 0
    let tiny = soft_mul(l, one, make(l, false, 0x18), Mode::NearestEven); // 2^-4 is code exp field 3
    assert_eq!(value(l, tiny), 0.0625);
    assert_eq!(value(l, soft_add(l, one, tiny, Mode::NearestEven)), 1.0);
    assert_eq!(value(l, soft_add(l, one, tiny, Mode::NearestAway)), 1.125);
    assert_eq!(value(l, soft_add(l, one, tiny, Mode::Up)), 1.125);
    // bias of nearest-away vs nearest-even when repeatedly adding a tie
    let (mut even, mut away) = (one, one);
    for _ in 0..8 {
        even = soft_add(l, even, tiny, Mode::NearestEven);
        away = soft_add(l, away, tiny, Mode::NearestAway);
    }
    println!(
        "  adding 2^-4 eight times to 1.0 (exact answer 1.5): nearest-even {} (stalls on ties), nearest-away {}",
        value(l, even),
        value(l, away)
    );
}

fn random_binary16(rng: &mut Rng) {
    let l = F16;
    let table = RefTable::new(l);
    for _ in 0..30_000 {
        let (a, b) = (rng.next() & 0xFFFF, rng.next() & 0xFFFF);
        let mode = MODES[(rng.next() % 5) as usize];
        let (x, y) = (value(l, a), value(l, b));
        let want = |v: f64| {
            if v.is_nan() {
                l.nan_code()
            } else if v.is_infinite() {
                make(l, v < 0.0, l.inf_code())
            } else {
                table.round(v, mode)
            }
        };
        let s = x + y; // exact in f64: binary16 values span < 53 bits
        if s != 0.0 {
            assert!(
                same_code(l, soft_add(l, a, b, mode), want(s)),
                "f16 add {a:#x} {b:#x} {mode:?}"
            );
        }
        let p = x * y;
        if p != 0.0 {
            assert!(
                same_code(l, soft_mul(l, a, b, mode), want(p)),
                "f16 mul {a:#x} {b:#x} {mode:?}"
            );
        }
    }
    println!(
        "binary16: 30000 random pairs/modes soft add+mul == table reference (65504 max, 2^-24 smallest subnormal)"
    );
}

// ============================================================ 3. ulp, nextafter
fn next_up(x: f64) -> f64 {
    if x.is_nan() || x == f64::INFINITY {
        return x;
    }
    if x == 0.0 {
        return f64::from_bits(1);
    }
    let b = x.to_bits();
    f64::from_bits(if x > 0.0 { b + 1 } else { b - 1 })
}
fn next_down(x: f64) -> f64 {
    -next_up(-x)
}
fn ulp(x: f64) -> f64 {
    next_up(x.abs()) - x.abs()
}
/// monotone key: float order == integer order, -0.0 and +0.0 share key 0
fn order_key(x: f64) -> i64 {
    let b = x.to_bits() as i64;
    if b < 0 { i64::MIN - b } else { b }
}
fn ulps_between(a: f64, b: f64) -> u64 {
    (order_key(a) as i128 - order_key(b) as i128).unsigned_abs() as u64
}
/// the tolerance recipe: absolute for values near zero, relative elsewhere, ulps as a stricter alternative
fn approx_eq(a: f64, b: f64, rel: f64, abs: f64) -> bool {
    if a == b {
        return true; // includes equal infinities, whose difference is NaN
    }
    // otherwise an infinity must never pass: `rel * inf` is an infinite tolerance
    a.is_finite() && b.is_finite() && (a - b).abs() <= abs.max(rel * a.abs().max(b.abs()))
}

fn check_ulps(rng: &mut Rng) {
    for _ in 0..5000 {
        let x = f64::from_bits(rng.next());
        if !x.is_finite() {
            continue;
        }
        assert_eq!(next_up(x).to_bits(), x.next_up().to_bits());
        assert_eq!(next_down(x).to_bits(), x.next_down().to_bits());
        if x != f64::MAX && x.abs() > f64::MIN_POSITIVE {
            assert_eq!(ulps_between(x, next_up(x)), 1);
            assert!(next_up(x) - x == ulp(x) || x < 0.0 && x - next_down(x) >= ulp(x) / 2.0);
        }
    }
    assert_eq!(ulps_between(-0.0, 0.0), 0);
    assert_eq!(ulps_between(-f64::from_bits(1), f64::from_bits(1)), 2); // across zero: two steps
    assert_eq!(ulp(1.0), f64::EPSILON);
    assert_eq!(ulp(2.0), 2.0 * f64::EPSILON);
    let s = 0.1 + 0.2;
    println!(
        "0.1 + 0.2 = {s:?} = {:.20}; 0.3 = {:.20}; ulps apart: {}",
        s,
        0.3,
        ulps_between(s, 0.3)
    );
    println!("  exact value of the double nearest 0.1: {:.60}", 0.1f64);
    assert!(s != 0.3 && ulps_between(s, 0.3) == 1 && approx_eq(s, 0.3, 1e-12, 1e-15));
    let (inf, nan) = (f64::INFINITY, f64::NAN);
    assert!(approx_eq(inf, inf, 1e-12, 1e-15));
    for (a, b) in [
        (1.0, inf),
        (-inf, 1.0),
        (inf, -inf),
        (f64::MAX, inf),
        (nan, nan),
    ] {
        assert!(!approx_eq(a, b, 1e-12, 1e-15), "{a} is not about {b}");
    }
    // the same sum in f32 happens to round differently: do not generalise from one example
    println!("  in f32: 0.1+0.2 == 0.3 is {}", 0.1f32 + 0.2f32 == 0.3f32);
    // ten additions of 0.1 do not make 1.0, but ten cents as integers do
    let mut acc = 0.0;
    let mut cents = 0i64;
    for _ in 0..10 {
        acc += 0.1;
        cents += 10;
    }
    assert!(acc != 1.0 && cents == 100);
    println!(
        "  0.1 added ten times = {acc:?} ({} ulps from 1.0); ten times 10 cents = {cents}",
        ulps_between(acc, 1.0)
    );
    println!(
        "ulp/nextafter: bit-twiddled next_up/next_down == std on 5000 random doubles; ulp(1) = 2^-52"
    );
}

// ============================================================ 4. cancellation
fn cancellation() {
    let x = 1e-8f64;
    let naive = (1.0 - x.cos()) / (x * x);
    let stable = 2.0 * (x / 2.0).sin().powi(2) / (x * x);
    println!("(1-cos x)/x^2 at x=1e-8 (true 0.5): naive {naive}, stable {stable}");
    assert!((naive - 0.5).abs() > 0.1 && (stable - 0.5).abs() < 1e-12);
    // x^2 - 1e8 x + 1 = 0: roots near 1e8 and 1e-8
    let (a, b, c) = (1.0f64, -1e8, 1.0);
    let d = (b * b - 4.0 * a * c).sqrt();
    let naive_small = (-b - d) / (2.0 * a);
    let q = -0.5 * (b + b.signum() * d);
    let stable_small = c / q;
    let truth = 1e-8 * (1.0 + 1e-16);
    let (en, es) = (
        (naive_small - truth).abs() / truth,
        (stable_small - truth).abs() / truth,
    );
    println!(
        "small root of x^2-1e8x+1: naive {naive_small:e} (rel err {en:.1e}), stable {stable_small:e} (rel err {es:.1e})"
    );
    assert!(en > 1e-9 && es < 1e-14);
    // (a+b)-a loses b entirely when b is below half an ulp of a
    assert_eq!((1e16 + 1.0) - 1e16, 0.0);
}

// ============================================================ 5. fma
fn two_prod_dekker(a: f64, b: f64) -> (f64, f64) {
    fn split(a: f64) -> (f64, f64) {
        let t = 134_217_729.0 * a; // 2^27 + 1
        let hi = t - (t - a);
        (hi, a - hi)
    }
    let p = a * b;
    let ((ah, al), (bh, bl)) = (split(a), split(b));
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}
fn two_prod_fma(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p)) // single rounding: exactly the error of the product
}
fn naive_det(a: f64, b: f64, c: f64, d: f64) -> f64 {
    a * d - b * c
}
/// Kahan's algorithm: error at most ~1.5 ulp regardless of cancellation
fn kahan_det(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let w = b * c;
    let e = (-b).mul_add(c, w);
    let f = a.mul_add(d, -w);
    f + e
}

fn fma_study(rng: &mut Rng) {
    let (mut naive_err, mut kahan_err) = (0.0f64, 0.0f64);
    for _ in 0..2000 {
        let a = rng.unit() * 1e3;
        let b = rng.unit() * 1e3;
        assert_eq!(
            two_prod_dekker(a, b),
            two_prod_fma(a, b),
            "Dekker == FMA error term"
        );
        // ill-conditioned determinant: rows nearly parallel
        let (a1, b1) = (rng.unit(), rng.unit());
        let eps = 1e-9 * rng.unit();
        let (c1, d1) = (a1 * (1.0 + eps), b1 * (1.0 + eps));
        // exact value from the exact product expansions
        let (p1, e1) = two_prod_fma(a1, d1);
        let (p2, e2) = two_prod_fma(b1, c1);
        let exact = fsum(&[p1, e1, -p2, -e2]);
        naive_err += (naive_det(a1, b1, c1, d1) - exact).abs();
        kahan_err += (kahan_det(a1, b1, c1, d1) - exact).abs();
    }
    println!(
        "fma: Dekker two_prod == fma two_prod on 2000 pairs; 2x2 det abs error sum naive {naive_err:.3e} vs Kahan/fma {kahan_err:.3e}"
    );
    assert!(kahan_err < naive_err / 10.0);
}

// ============================================================ 6. summation
fn naive_sum(v: &[f64]) -> f64 {
    v.iter().sum()
}
fn kahan_sum(v: &[f64]) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for &x in v {
        let y = x - c;
        let t = s + y;
        c = (t - s) - y; // what was lost (algebraically zero)
        s = t;
    }
    s
}
/// Neumaier: also handles the case where the next term is larger than the running sum
fn neumaier_sum(v: &[f64]) -> f64 {
    let (mut s, mut c) = (0.0f64, 0.0f64);
    for &x in v {
        let t = s + x;
        c += if s.abs() >= x.abs() {
            (s - t) + x
        } else {
            (x - t) + s
        };
        s = t;
    }
    s + c
}
fn pairwise_sum(v: &[f64]) -> f64 {
    if v.len() <= 8 {
        return v.iter().sum();
    }
    let (l, r) = v.split_at(v.len() / 2);
    pairwise_sum(l) + pairwise_sum(r) // error grows like log n instead of n
}
/// Exact sum rounded once (Shewchuk's non-overlapping partials; same algorithm as Python's math.fsum)
/// An infinity or NaN among the terms decides the result. Panics if a partial sum of finite terms overflows, even when
/// the exact total would fit (math.fsum raises there too): the error terms would silently become NaN.
fn fsum(v: &[f64]) -> f64 {
    if v.iter().any(|x| !x.is_finite()) {
        return v.iter().filter(|x| !x.is_finite()).sum(); // inf + -inf and anything with NaN give NaN
    }
    let mut partials: Vec<f64> = vec![];
    for &x0 in v {
        let mut x = x0;
        let mut next = vec![];
        for &y0 in &partials {
            let (mut a, mut b) = (x, y0);
            if a.abs() < b.abs() {
                std::mem::swap(&mut a, &mut b);
            }
            let hi = a + b;
            assert!(hi.is_finite(), "fsum: intermediate overflow");
            let lo = b - (hi - a);
            if lo != 0.0 {
                next.push(lo);
            }
            x = hi;
        }
        next.push(x);
        partials = next;
    }
    let mut n = partials.len();
    if n == 0 {
        return 0.0;
    }
    n -= 1;
    let mut hi = partials[n];
    let mut lo = 0.0;
    while n > 0 {
        let x = hi;
        n -= 1;
        let y = partials[n];
        hi = x + y;
        lo = y - (hi - x);
        if lo != 0.0 {
            break;
        }
    }
    if n > 0 && ((lo < 0.0 && partials[n - 1] < 0.0) || (lo > 0.0 && partials[n - 1] > 0.0)) {
        let y = lo * 2.0;
        let x = hi + y;
        if y == x - hi {
            hi = x;
        }
    }
    hi
}

fn summation_study(rng: &mut Rng) {
    // the classic: Kahan loses to Neumaier when a huge term arrives between small ones
    let v = [1.0, 1e100, 1.0, -1e100];
    println!(
        "sum of [1, 1e100, 1, -1e100] (true 2): naive {}, kahan {}, neumaier {}, pairwise {}, exact {}",
        naive_sum(&v),
        kahan_sum(&v),
        neumaier_sum(&v),
        pairwise_sum(&v),
        fsum(&v)
    );
    assert_eq!(
        (naive_sum(&v), kahan_sum(&v), neumaier_sum(&v), fsum(&v)),
        (0.0, 0.0, 2.0, 2.0)
    );

    let inf = f64::INFINITY;
    assert_eq!(
        (fsum(&[inf, 1.0]), fsum(&[1.0, -inf, f64::MAX, f64::MAX])),
        (inf, -inf)
    );
    assert!(fsum(&[inf, -inf]).is_nan() && fsum(&[f64::NAN, 1.0]).is_nan());
    assert!(panics(|| {
        fsum(&[f64::MAX, f64::MAX, -f64::MAX]);
    }));

    // exact oracle: integers m * 2^s with 52-bit m are exact doubles; sum them in i128 and round once
    for case in 0..2000 {
        let n = 1 + (rng.next() % 40) as usize;
        let mut ints = 0i128;
        let mut xs = vec![];
        for _ in 0..n {
            let m = rng.range(-(1 << 52), 1 << 52) as i128;
            let s = (rng.next() % 50) as u32;
            ints += m << s;
            xs.push((m << s) as f64);
        }
        assert_eq!(
            fsum(&xs),
            ints as f64,
            "fsum is the correctly rounded exact sum (case {case})"
        );
    }
    println!("fsum: 2000 random integer-valued sets equal the i128 oracle rounded once");

    // measured error: heavy cancellation, mixed magnitudes
    let sets = 300;
    let mut err = [0.0f64; 4];
    for _ in 0..sets {
        let xs: Vec<f64> = (0..4000)
            .map(|_| rng.unit() * 10f64.powi(rng.range(-3, 8) as i32))
            .collect();
        let exact = fsum(&xs);
        let scale: f64 = xs.iter().map(|x| x.abs()).sum();
        for (i, f) in [naive_sum, kahan_sum, neumaier_sum, pairwise_sum]
            .iter()
            .enumerate()
        {
            err[i] += (f(&xs) - exact).abs() / scale;
        }
    }
    let names = ["naive", "kahan", "neumaier", "pairwise"];
    println!("mean |error| / sum|x| over {sets} sets of 4000 terms (1e-3..1e8 magnitudes):");
    for i in 0..4 {
        println!("  {:<9} {:.3e}", names[i], err[i] / sets as f64);
    }
    assert!(
        err[1] <= err[0] && err[2] <= err[0] && err[3] <= err[0],
        "every compensated/tree variant beats naive"
    );
    assert!(
        err[2] <= err[1] * 1.5,
        "Neumaier is at least as good as Kahan up to noise"
    );
    // order dependence: reversing the data changes the naive result (floating addition is not associative)
    let xs: Vec<f64> = (0..4000)
        .map(|_| rng.unit() * 10f64.powi(rng.range(-3, 8) as i32))
        .collect();
    let mut r = xs.clone();
    r.reverse();
    println!(
        "  naive forward vs reversed differ: {}; exact sums equal: {}",
        naive_sum(&xs) != naive_sum(&r),
        fsum(&xs) == fsum(&r)
    );
    assert_eq!(fsum(&xs), fsum(&r));
}

// ============================================================ 7. comparison pitfalls
fn comparison_pitfalls() {
    let nan = f64::NAN;
    assert!(nan != nan && !(nan < 1.0) && !(nan > 1.0) && !(nan == nan));
    assert_eq!(nan.partial_cmp(&1.0), None);
    assert_eq!(0.0f64, -0.0);
    assert!(0.0f64.to_bits() != (-0.0f64).to_bits());
    assert!(
        (1.0 / 0.0f64).is_infinite() && 1.0 / 0.0f64 > 0.0 && 1.0 / -0.0f64 < 0.0,
        "signed zero is visible through division"
    );
    assert_eq!(0.0f64.total_cmp(&-0.0), Ordering::Greater);
    // sorting needs a total order: total_cmp places -NaN < -inf < ... < -0 < +0 < ... < inf < +NaN
    let mut v = vec![
        2.0,
        f64::NAN,
        -0.0,
        0.0,
        f64::NEG_INFINITY,
        -1.5,
        f64::INFINITY,
    ];
    v.sort_by(|a, b| a.total_cmp(b));
    let s: Vec<String> = v.iter().map(|x| format!("{x:?}")).collect();
    println!("sorted with total_cmp: {}", s.join(" "));
    assert!(
        v[0] == f64::NEG_INFINITY
            && v[2].to_bits() == (-0.0f64).to_bits()
            && v[3].to_bits() == 0.0f64.to_bits()
            && v[6].is_nan()
    );
    // min/max ignore NaN (return the other operand); `<` based hand-written max does not
    assert_eq!(f64::NAN.max(1.0), 1.0);
    let hand_max = |a: f64, b: f64| if a > b { a } else { b };
    assert!(
        hand_max(1.0, f64::NAN).is_nan() && hand_max(f64::NAN, 1.0) == 1.0,
        "hand-written max is order dependent with NaN"
    );
    // floats are not Hash/Eq in Rust: use to_bits (and normalise -0.0 and NaN first) when you need a key
    let canon = |x: f64| {
        if x == 0.0 {
            0.0f64.to_bits()
        } else if x.is_nan() {
            f64::NAN.to_bits()
        } else {
            x.to_bits()
        }
    };
    let mut m: HashMap<u64, i32> = HashMap::new();
    *m.entry(canon(0.0)).or_default() += 1;
    *m.entry(canon(-0.0)).or_default() += 1;
    *m.entry(canon(f64::NAN)).or_default() += 1;
    *m.entry(canon(-f64::NAN)).or_default() += 1;
    assert_eq!(m.len(), 2);
    // `as` casts saturate and send NaN to 0
    assert_eq!(f64::NAN as i32, 0);
    assert_eq!(1e20f64 as i32, i32::MAX);
    assert_eq!(-1e20f64 as i32, i32::MIN);
    // (x == y) does not imply 1/x == 1/y
    assert!(0.0 == -0.0 && 1.0 / 0.0 != 1.0 / -0.0);
    println!(
        "comparison pitfalls ok: NaN, signed zero, total_cmp order, min/max, hash keys, saturating casts"
    );
}

// ============================================================ 8. round-trip formatting
fn digits_of(x: f64) -> usize {
    // number of significant digits in the shortest `{:e}` form, e.g. "1.2345e-3" -> 5
    let s = format!("{x:e}");
    s.split('e')
        .next()
        .unwrap()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .count()
}

fn roundtrip(rng: &mut Rng) {
    let (mut checked, mut max_digits) = (0, 0);
    for _ in 0..20_000 {
        let x = f64::from_bits(rng.next());
        if !x.is_finite() {
            continue;
        }
        let s = format!("{x:?}");
        let y: f64 = s.parse().unwrap();
        assert_eq!(x.to_bits(), y.to_bits(), "parse(print(x)) == x for {s}");
        // shortest: with one digit fewer (correctly rounded) the value no longer round-trips
        let d = digits_of(x);
        max_digits = max_digits.max(d);
        if d > 1 {
            let shorter = format!("{:.*e}", d - 2, x);
            assert!(
                shorter.parse::<f64>().unwrap().to_bits() != x.to_bits(),
                "{s} is not shortest: {shorter}"
            );
        }
        let f = f32::from_bits(rng.next() as u32);
        if f.is_finite() {
            assert_eq!(
                format!("{f:?}").parse::<f32>().unwrap().to_bits(),
                f.to_bits()
            );
        }
        checked += 1;
    }
    println!(
        "round trip: {checked} random doubles print to the shortest string that parses back bit-exactly (max {max_digits} digits; 17 always suffice)"
    );
    assert!(max_digits <= 17);
    assert_eq!(format!("{}", 0.1 + 0.2), "0.30000000000000004");
    assert_eq!(format!("{}", 0.1f32 + 0.2f32), "0.3"); // f32 prints shorter: fewer bits to identify
    assert_eq!(format!("{:?}", 1e21), "1e21");
    assert_eq!(format!("{:?}", 5e-324), "5e-324");
}

fn main() {
    let mut rng = Rng(0x2545F4914F6CDD1D);
    check_layouts(&mut rng);
    exhaustive_minifloat();
    random_binary16(&mut rng);
    check_ulps(&mut rng);
    cancellation();
    fma_study(&mut rng);
    summation_study(&mut rng);
    comparison_pitfalls();
    roundtrip(&mut rng);
    println!("all IEEE 754 checks passed");
}
