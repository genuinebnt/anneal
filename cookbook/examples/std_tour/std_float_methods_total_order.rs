//! RECIPE 1028 (std): f32/f64 look-up page: rounding, special values, comparison and total order, approximate equality,
//! summation error, bit access, Duration conversions.
//! LEVEL: intermediate
//! Use in: metrics, percentiles, geometry, rates and backoff, unit conversion, scoring and ranking.   Run: cargo run --release --example std_float_methods_total_order
//! COMMON USES: latency statistics, requests-per-second, jittered backoff, distance and angle maths, sorting scores
//! that may contain NaN, comparing computed results in tests.
//! METHODS: (everyday first) f64::floor, f64::ceil, f64::round, f64::trunc, f64::fract, f64::round_ties_even, f64::abs, f64::signum, f64::copysign,
//! METHODS: f64::min, f64::max, f64::clamp, f64::is_nan, f64::is_finite, f64::is_infinite, f64::total_cmp, f64::partial_cmp,
//! METHODS: f64::sqrt, f64::cbrt, f64::powi, f64::powf, f64::exp, f64::ln, f64::log, f64::log2, f64::log10, f64::ln_1p, f64::exp_m1,
//! METHODS: f64::hypot, f64::sin, f64::cos, f64::tan, f64::atan2, f64::sin_cos, f64::to_degrees, f64::to_radians, f64::mul_add, f64::recip,
//! METHODS: f64::rem_euclid, f64::midpoint, f64::is_sign_negative, f64::is_sign_positive, f64::is_normal, f64::is_subnormal, f64::classify,
//! METHODS: f64::to_bits, f64::from_bits, f64::to_be_bytes, f64::from_be_bytes, f64::next_up, f64::next_down, f64::EPSILON, f64::MAX, f64::MIN,
//! METHODS: f64::MIN_POSITIVE, f64::INFINITY, f64::NAN, std::f64::consts::PI, std::f64::consts::TAU, std::f64::consts::E,
//! METHODS: Duration::as_secs_f64, Duration::from_secs_f64, Duration::try_from_secs_f64, Duration::mul_f64, Duration::div_duration_f64
//! DOCS: https://doc.rust-lang.org/std/primitive.f64.html
//! DEFINED IN: core: abs, signum, copysign, min, max, clamp, midpoint, total_cmp, classification, bit/byte access, to_degrees,
//! to_radians, recip, next_up/down, constants, Duration. STD ONLY in 1.98.1 (the core versions are unstable "float math in
//! core"): floor, ceil, round, round_ties_even, trunc, fract, sqrt, cbrt, powi, powf, exp, ln, log*, hypot, trig,
//! mul_add, rem_euclid. So no_std code cannot call those without a maths crate.
//! NOT COVERED (and why): parse/format round trips (see std_num_parsing_formatting section 7), hyperbolic and inverse trig (same shape), exp2, div_euclid, the little/native-endian byte forms, algebraic_* (1.98, permits reordering so results are
//! not reproducible), abs_sub (deprecated), f16/f128, gamma/erf/maximum/minimum (unstable), to_int_unchecked (unsafe).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   round half away from zero / half to even   -> round() / round_ties_even()
//!   toward -inf / +inf / zero                  -> floor() / ceil() / trunc()
//!   compare for sorting                        -> total_cmp
//!   keep inside bounds                         -> clamp(lo, hi)        (NaN stays NaN!)
//!   check input is a usable number             -> is_finite()
//!   approximately equal                        -> |a-b| <= max(abs_tol, rel_tol * max(|a|,|b|))
//!   distance without overflow                  -> dx.hypot(dy)
//!   angle of a vector                          -> y.atan2(x)           (note the argument order)
//!   wrap an angle into 0..360                  -> rem_euclid(360.0)
//!   average two values without overflow        -> midpoint
//!   exact bit identity / hashing               -> to_bits
//!   seconds <-> Duration                       -> as_secs_f64 / try_from_secs_f64
//!
//! PRECISION (std docs): sqrt is correctly rounded. Transcendental functions and powi have "unspecified precision" that may
//! vary by platform, Rust version and even between calls, so every assert on them here uses a tolerance.
//!
//! Read: https://doc.rust-lang.org/std/primitive.f64.html , https://docs.oracle.com/cd/E19957-01/806-3568/ncg_goldberg.html
//! TRY NEXT:
//!   - [S] Replace `total_cmp` in `percentile` with `partial_cmp(..).unwrap()` and remove the NaN filter. Run.
//!   - [S] Predict `(0.1 * 3.0 == 0.3)` and `(0.5 * 3.0 == 1.5)`, then explain the difference using to_bits.
//!   - [M] Replace the two-pass variance with Welford and compare errors on data offset by 1e9 (section 5).
//!   - [M] Write `ulp_distance` for f32 and test it across the zero crossing.
//!   - [A] PROJECT: a latency histogram with logarithmic buckets (ilog10 from std_integer_methods_overflow, log from here),
//!         checked against exact percentiles computed with select_nth_unstable_by (std_slice_search_sort_binary).
use std::collections::HashSet;
use std::f64::consts::{E, PI, TAU};
use std::num::FpCategory;
use std::time::Duration;

const COVERED: &[&str] = &[
    "floor",
    "ceil",
    "round",
    "trunc",
    "fract",
    "round_ties_even",
    "abs",
    "signum",
    "copysign",
    "min",
    "max",
    "clamp",
    "is_nan",
    "is_finite",
    "is_infinite",
    "total_cmp",
    "partial_cmp",
    "sqrt",
    "cbrt",
    "powi",
    "powf",
    "exp",
    "ln",
    "log",
    "log2",
    "log10",
    "ln_1p",
    "exp_m1",
    "hypot",
    "sin",
    "cos",
    "tan",
    "atan2",
    "sin_cos",
    "to_degrees",
    "to_radians",
    "mul_add",
    "recip",
    "rem_euclid",
    "midpoint",
    "is_sign_negative",
    "is_sign_positive",
    "is_normal",
    "is_subnormal",
    "classify",
    "to_bits",
    "from_bits",
    "to_be_bytes",
    "from_be_bytes",
    "next_up",
    "next_down",
    "EPSILON",
    "MAX",
    "MIN",
    "MIN_POSITIVE",
    "INFINITY",
    "NAN",
    "PI",
    "TAU",
    "E",
    "as_secs_f64",
    "from_secs_f64",
    "try_from_secs_f64",
    "mul_f64",
    "div_duration_f64",
];

fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

/// Absolute tolerance handles values near zero, relative tolerance handles large ones.
fn approx(a: f64, b: f64, abs_tol: f64, rel_tol: f64) -> bool {
    if a == b {
        return true; // covers equal infinities
    }
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    (a - b).abs() <= abs_tol.max(rel_tol * a.abs().max(b.abs()))
}
fn close(a: f64, b: f64) -> bool {
    approx(a, b, 1e-12, 1e-12)
}
/// Number of representable f64 steps between two finite values.
fn ulp_distance(a: f64, b: f64) -> u64 {
    // Map the sign-magnitude bit pattern onto a monotonic integer line.
    let key = |x: f64| {
        let bits = x.to_bits() as i64;
        if bits < 0 { i64::MIN - bits } else { bits }
    };
    key(a).abs_diff(key(b))
}

// ---- 1. Everyday core: rounding --------------------------------------------------------------------------------
fn section_rounding() {
    let table: [(f64, [f64; 5]); 6] = [
        // x       floor ceil  trunc round ties_even
        (2.5, [2.0, 3.0, 2.0, 3.0, 2.0]),
        (3.5, [3.0, 4.0, 3.0, 4.0, 4.0]),
        (-2.5, [-3.0, -2.0, -2.0, -3.0, -2.0]),
        (2.4, [2.0, 3.0, 2.0, 2.0, 2.0]),
        (-2.6, [-3.0, -2.0, -2.0, -3.0, -3.0]),
        (-0.4, [-1.0, -0.0, -0.0, -0.0, -0.0]),
    ];
    for (x, want) in table {
        assert_eq!(
            [
                x.floor(),
                x.ceil(),
                x.trunc(),
                x.round(),
                x.round_ties_even()
            ],
            want,
            "{x}"
        );
    }
    assert!((-0.4f64).round().is_sign_negative()); // -0.0
    // fract keeps the sign: x == x.trunc() + x.fract().
    assert_eq!((2.75f64.fract(), (-2.75f64).fract()), (0.75, -0.75));
    // Round to 2 decimals for COMPUTATION (display should use {:.2}). The product is itself inexact.
    let r2 = |x: f64| (x * 100.0).round() / 100.0;
    assert_eq!((r2(3.14159), r2(1.005)), (3.14, 1.0));
    // Ceiling division in floats vs integers: prefer integer div_ceil for counts.
    assert_eq!(((10.0f64 / 4.0).ceil() as u32, 10u32.div_ceil(4)), (3, 3));
    println!("1. rounding ok");
}

// ---- 2. Signs, bounds, special values --------------------------------------------------------------------------
fn section_special() {
    assert_eq!(
        ((-3.0f64).abs(), (-3.0f64).signum(), 3.0f64.copysign(-0.0)),
        (3.0, -1.0, -3.0)
    );
    // signum is never 0: zeros give +-1, NaN gives NaN.
    assert_eq!((0.0f64.signum(), (-0.0f64).signum()), (1.0, -1.0));
    assert!(f64::NAN.signum().is_nan());
    assert!(0.0 == -0.0 && (-0.0f64).is_sign_negative() && 0.0f64.is_sign_positive());

    // Division by zero does not panic.
    let zero = std::hint::black_box(0.0f64);
    assert_eq!(
        (1.0 / zero, -1.0 / zero),
        (f64::INFINITY, f64::NEG_INFINITY)
    );
    assert!(
        (zero / zero).is_nan()
            && (f64::INFINITY - f64::INFINITY).is_nan()
            && (-1.0f64).sqrt().is_nan()
    );
    assert!((f64::MAX * 2.0).is_infinite()); // overflow saturates to infinity
    assert_eq!(f64::MIN, -f64::MAX); // MIN is the most negative value, not the smallest positive one
    assert!(
        f64::MIN_POSITIVE > 0.0
            && !f64::NAN.is_finite()
            && !f64::INFINITY.is_finite()
            && !f64::INFINITY.is_nan()
    );
    let sub = f64::MIN_POSITIVE / 2.0;
    assert!(sub > 0.0 && sub.is_subnormal() && !sub.is_normal() && !0.0f64.is_normal());
    let classes = [0.0, 1.0, sub, f64::INFINITY, f64::NAN].map(f64::classify);
    assert_eq!(
        classes,
        [
            FpCategory::Zero,
            FpCategory::Normal,
            FpCategory::Subnormal,
            FpCategory::Infinite,
            FpCategory::Nan
        ]
    );

    // min/max ignore ONE NaN. For -0.0 vs +0.0 either may be returned.
    assert_eq!((f64::NAN.max(2.0), 2.0f64.min(f64::NAN)), (2.0, 2.0));
    assert!(f64::NAN.max(f64::NAN).is_nan());
    // clamp keeps NaN, and panics for min > max or NaN bounds.
    assert_eq!(
        (1.5f64.clamp(0.0, 1.0), (-1.0f64).clamp(0.0, 1.0)),
        (1.0, 0.0)
    );
    assert!(f64::NAN.clamp(0.0, 1.0).is_nan());
    assert!(panics(|| {
        let _ = 0.0f64.clamp(std::hint::black_box(1.0), 0.0);
    }));
    // midpoint (since 1.85): no intermediate overflow.
    assert!(((f64::MAX + f64::MAX) / 2.0).is_infinite());
    assert_eq!(f64::MAX.midpoint(f64::MAX), f64::MAX);
    println!("2. signs, bounds, special values ok");
}

// ---- 3. Comparing: NaN, total_cmp, approximate equality --------------------------------------------------------
fn section_compare() {
    let nan = f64::NAN;
    #[allow(clippy::eq_op)]
    {
        assert!(!(nan == nan));
    }
    assert_eq!(nan.partial_cmp(&0.0), None);
    let mut v = vec![3.0, nan, 1.0, f64::INFINITY, 2.0];
    v.sort_by(f64::total_cmp);
    assert_eq!(v[..4], [1.0, 2.0, 3.0, f64::INFINITY]);
    assert!(v[4].is_nan()); // a positive NaN sorts last; a NaN with the sign bit set sorts FIRST
    assert!((-nan).total_cmp(&f64::NEG_INFINITY).is_lt());
    // The sign of a NaN produced by arithmetic is not something to rely on: filter NaN before sorting.

    // Exact equality fails for computed values.
    let sum = 0.1 + 0.2;
    assert!(sum != 0.3 && ulp_distance(sum, 0.3) == 1);
    assert!(close(sum, 0.3));
    // EPSILON is the gap between 1.0 and the next float, NOT a universal tolerance.
    assert_eq!(1.0f64.next_up() - 1.0, f64::EPSILON);
    let big = 1e9f64;
    assert!(
        (big.next_up() - big).abs() > f64::EPSILON,
        "neighbours at 1e9 differ by far more than EPSILON"
    );
    assert!(approx(big, big.next_up(), 0.0, 1e-12));
    let tiny = 1e-20f64;
    assert!(
        (tiny - 2.0 * tiny).abs() < f64::EPSILON,
        "EPSILON calls values a factor 2 apart equal"
    );
    assert!(!approx(tiny, 2.0 * tiny, 0.0, 1e-12));
    assert!(!approx(nan, nan, 1.0, 1.0) && approx(f64::INFINITY, f64::INFINITY, 0.0, 0.0));
    assert_eq!(
        (
            ulp_distance(1.0, 1.0f64.next_down()),
            ulp_distance(0.0f64.next_up(), 0.0f64.next_down())
        ),
        (1, 2)
    );
    // Floats are neither Eq nor Hash. For exact-identity keys use the bits (note: -0.0 and 0.0 differ).
    let keys: HashSet<u64> = [1.0f64, 1.0, 0.0, -0.0]
        .into_iter()
        .map(f64::to_bits)
        .collect();
    assert_eq!(keys.len(), 3);
    println!("3. comparison ok");
}

// ---- 4. Roots, powers, logs, geometry --------------------------------------------------------------------------
fn section_maths() {
    assert_eq!(9.0f64.sqrt(), 3.0); // correctly rounded
    assert!(close(27.0f64.cbrt(), 3.0) && close((-8.0f64).cbrt(), -2.0));
    assert!((-8.0f64).powf(1.0 / 3.0).is_nan()); // powf of a negative base with a fractional exponent
    assert!(close(2.0f64.powi(10), 1024.0) && close(2.0f64.powf(0.5), 2.0f64.sqrt()));
    assert!(close(E.ln(), 1.0) && close(1.0f64.exp(), E));
    assert!(
        close(8.0f64.log2(), 3.0) && close(1000.0f64.log10(), 3.0) && close(81.0f64.log(3.0), 4.0)
    );
    assert!(0.0f64.ln() == f64::NEG_INFINITY && (-1.0f64).ln().is_nan());
    // ln_1p / exp_m1 keep precision for tiny x, where 1.0 + x loses most digits.
    let x = 1e-10f64;
    let series = x - x * x / 2.0;
    assert!(approx(x.ln_1p(), series, 0.0, 1e-14));
    assert!(!approx((1.0 + x).ln(), series, 0.0, 1e-9));
    assert!(approx(x.exp_m1(), x + x * x / 2.0, 0.0, 1e-14));

    // hypot avoids the overflow in sqrt(x*x + y*y).
    assert!(close(3.0f64.hypot(4.0), 5.0));
    let huge = 1e200f64;
    assert!((huge * huge + huge * huge).sqrt().is_infinite());
    assert!(huge.hypot(huge).is_finite());
    // Heading from (0,0) to (x,y), in degrees 0..360. atan2 is y.atan2(x).
    let heading = |x: f64, y: f64| y.atan2(x).to_degrees().rem_euclid(360.0);
    for (x, y, deg) in [
        (1.0, 0.0, 0.0),
        (0.0, 1.0, 90.0),
        (-1.0, 0.0, 180.0),
        (0.0, -1.0, 270.0),
    ] {
        assert!(close(heading(x, y), deg), "{x},{y}");
    }
    assert!(
        close(180.0f64.to_radians(), PI) && close(PI.to_degrees(), 180.0) && close(TAU, 2.0 * PI)
    );
    let (s, c) = (PI / 6.0).sin_cos();
    assert!(close(s, 0.5) && close(c, 3f64.sqrt() / 2.0));
    assert!(
        close((PI / 6.0).sin(), s) && close((PI / 6.0).cos(), c) && close((PI / 4.0).tan(), 1.0)
    );
    assert!(PI.sin() != 0.0 && approx(PI.sin(), 0.0, 1e-12, 0.0)); // near zero you NEED an absolute tolerance
    // rem_euclid is non-negative; % keeps the sign of the left side.
    assert_eq!(
        ((-90.0f64) % 360.0, (-90.0f64).rem_euclid(360.0)),
        (-90.0, 270.0)
    );
    assert_eq!(4.0f64.recip(), 0.25);
    // mul_add computes a*b+c with ONE rounding, so it can recover what the product rounded away.
    let a = 1.0 + f64::EPSILON;
    let product = a * a;
    assert_eq!(a.mul_add(a, -product), f64::EPSILON * f64::EPSILON);
    println!("4. maths and geometry ok");
}

// ---- 5. Statistics: summation error, mean, variance, percentile ------------------------------------------------
/// Neumaier's compensated sum: carries the low-order bits that plain addition drops.
fn compensated_sum(xs: &[f64]) -> f64 {
    let (mut sum, mut c) = (0.0f64, 0.0f64);
    for &x in xs {
        let t = sum + x;
        c += if sum.abs() >= x.abs() {
            (sum - t) + x
        } else {
            (x - t) + sum
        };
        sum = t;
    }
    sum + c
}

/// (mean, sample variance) of the finite values, by Welford's one-pass method.
fn mean_var(xs: &[f64]) -> Option<(f64, f64)> {
    let (mut n, mut mean, mut m2) = (0u64, 0.0f64, 0.0f64);
    for &x in xs.iter().filter(|x| x.is_finite()) {
        n += 1;
        let d = x - mean;
        mean += d / n as f64;
        m2 += d * (x - mean);
    }
    (n > 0).then(|| (mean, if n > 1 { m2 / (n - 1) as f64 } else { 0.0 }))
}

/// Nearest-rank percentile, p in 0..=100, ignoring non-finite samples.
fn percentile(xs: &[f64], p: f64) -> Option<f64> {
    let mut v: Vec<f64> = xs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() || !(0.0..=100.0).contains(&p) {
        return None;
    }
    let rank = ((p / 100.0 * v.len() as f64).ceil() as usize).max(1);
    let (_, x, _) = v.select_nth_unstable_by(rank - 1, f64::total_cmp);
    Some(*x)
}

fn section_stats() {
    assert!((0..10).map(|_| 0.1).sum::<f64>() != 1.0);
    assert!(compensated_sum(&[1.0, 1e100, 1.0, -1e100]) == 2.0);
    assert_eq!([1.0, 1e100, 1.0, -1e100].iter().sum::<f64>(), 0.0);
    // Float addition is not associative: parallel reductions may change the last bits.
    assert_ne!((0.1 + 0.2) + 0.3, 0.1 + (0.2 + 0.3));
    let xs = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0, f64::NAN];
    let (mean, var) = mean_var(&xs).unwrap();
    assert!(close(mean, 5.0) && close(var, 32.0 / 7.0));
    assert_eq!(mean_var(&[f64::NAN]), None);
    // The textbook formula E[x^2] - E[x]^2 collapses when the mean is large.
    let shifted: Vec<f64> = [1.0, 2.0, 3.0].iter().map(|x| x + 1e9).collect();
    let naive = shifted.iter().map(|x| x * x).sum::<f64>() / 3.0
        - (shifted.iter().sum::<f64>() / 3.0).powi(2);
    // The true population variance is 2/3; squares near 1e18 are spaced 128 apart, so that signal is lost.
    assert!((naive - 2.0 / 3.0).abs() > 0.5, "naive gave {naive}");
    assert!(approx(mean_var(&shifted).unwrap().1, 1.0, 1e-6, 0.0));
    assert_eq!(
        (
            percentile(&xs, 50.0),
            percentile(&xs, 100.0),
            percentile(&xs, 0.0)
        ),
        (Some(4.0), Some(9.0), Some(2.0))
    );
    assert_eq!(
        (
            percentile(&[], 50.0),
            percentile(&xs, 101.0),
            percentile(&xs, f64::NAN)
        ),
        (None, None, None)
    );
    // fold(NAN, max) is a compact "maximum ignoring NaN"; the result is NaN only if everything is.
    assert_eq!(xs.iter().copied().fold(f64::NAN, f64::max), 9.0);
    println!("5. statistics ok");
}

// ---- 6. Bits, bytes, Duration ----------------------------------------------------------------------------------
fn section_bits_and_time() {
    let bits = 1.0f64.to_bits();
    assert_eq!(bits, 0x3FF0_0000_0000_0000);
    assert_eq!(f64::from_bits(bits), 1.0);
    let be = (-2.5f64).to_be_bytes();
    assert_eq!(f64::from_be_bytes(be), -2.5);
    // Integers are exact only up to 2^53.
    assert_eq!(9_007_199_254_740_992.0f64 + 1.0, 9_007_199_254_740_992.0);
    assert_eq!(0.1f32 as f64, 0.100000001490116119384765625); // widening keeps the f32 error

    let d = Duration::from_millis(1500);
    assert_eq!(d.as_secs_f64(), 1.5);
    assert_eq!(Duration::from_secs_f64(2.5), Duration::from_millis(2500));
    assert_eq!(d.mul_f64(2.0), Duration::from_secs(3));
    assert_eq!(d.div_duration_f64(Duration::from_secs(3)), 0.5); // since 1.80: ratio of two durations
    // from_secs_f64 PANICS for negative, non-finite or too-large input. Use the try_ form (since 1.66) on computed values.
    for bad in [-1.0, f64::NAN, f64::INFINITY, 1e30] {
        assert!(Duration::try_from_secs_f64(bad).is_err(), "{bad}");
    }
    assert!(panics(|| {
        let _ = Duration::from_secs_f64(std::hint::black_box(-0.5));
    }));
    assert!(panics(move || {
        let _ = d.mul_f64(std::hint::black_box(-1.0));
    }));
    // Unit conversion: keep one canonical unit and convert only at the edges.
    let c_to_f = |c: f64| c * 9.0 / 5.0 + 32.0;
    assert_eq!((c_to_f(100.0), c_to_f(-40.0)), (212.0, -40.0));
    println!("6. bits and durations ok");
}

// ---- 7. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Validate at the edge with is_finite(); one NaN poisons every later sum and mean.
    assert!([1.0, f64::NAN].iter().sum::<f64>().is_nan());
    // 2. Never `==` computed floats (clippy::float_cmp, pedantic).
    // 3. Money is integers (std_num_parsing_formatting), not floats.
    // 4. Rates: guard the denominator; 0/0 is NaN and serialises badly (JSON has no NaN or Infinity).
    let rate = |ok: u64, total: u64| (total > 0).then(|| ok as f64 / total as f64);
    assert_eq!((rate(1, 4), rate(0, 0)), (Some(0.25), None));
    // 5. Backoff: cap in the float domain BEFORE building a Duration.
    let delay = |attempt: i32| Duration::from_secs_f64((0.1 * 2f64.powi(attempt)).min(30.0));
    assert_eq!(delay(10_000), Duration::from_secs(30));
    // 6. Loop with integers and derive the float; adding 0.1 repeatedly drifts.
    let grid: Vec<f64> = (0..=10).map(|i| f64::from(i) / 10.0).collect();
    assert_eq!(grid[10], 1.0);
    // 7. f32 holds about 7 decimal digits: use it for bulk storage, not for accumulating.
    assert_eq!(16_777_216.0f32 + 1.0, 16_777_216.0);
    // 8. clippy::manual_clamp, clippy::suboptimal_flops (nursery) point at clamp, hypot, mul_add and ln_1p.
    // 9. Use total_cmp for any float sort or max_by in production paths; unwrap on partial_cmp is a latent crash.
    assert_eq!(
        [1.0, 3.0, 2.0].into_iter().max_by(f64::total_cmp),
        Some(3.0)
    );
    println!("7. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_rounding();
    section_special();
    section_compare();
    section_maths();
    section_stats();
    section_bits_and_time();
    section_production_tips();
    println!("covered {} float names; all asserts passed", COVERED.len());
}
