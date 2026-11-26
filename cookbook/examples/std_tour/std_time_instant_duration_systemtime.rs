//! RECIPE 1064 (std_tour): `std::time` by use case: Duration, Instant, SystemTime, deadlines, backoff, testable clocks.
//! LEVEL: intermediate
//! Use in: timeouts, retries, metrics, cache TTLs, log timestamps, token expiry.   Run: cargo run --release --example std_time_instant_duration_systemtime
//! COMMON USES: `Instant` for "how long" and deadlines; `SystemTime` for "what time is it" (persisted or sent to others); `Duration` for every timeout argument; an injected clock for tests.
//! METHODS: Duration::new, Duration::from_secs, Duration::from_millis, Duration::from_micros, Duration::from_nanos, Duration::from_nanos_u128,
//! Duration::from_mins, Duration::from_hours, Duration::from_secs_f64, Duration::try_from_secs_f64, Duration::as_secs, Duration::as_secs_f64,
//! Duration::as_millis, Duration::as_micros, Duration::as_nanos, Duration::subsec_millis, Duration::subsec_micros, Duration::subsec_nanos,
//! Duration::is_zero, Duration::checked_add, Duration::checked_sub, Duration::checked_mul, Duration::checked_div, Duration::saturating_add,
//! Duration::saturating_sub, Duration::saturating_mul, Duration::mul_f64, Duration::div_f64, Duration::div_duration_f64, Duration::abs_diff,
//! Duration::ZERO, Duration::MAX, Instant::now, Instant::elapsed, Instant::duration_since,
//! Instant::checked_duration_since, Instant::saturating_duration_since, Instant::checked_add, Instant::checked_sub, SystemTime::now,
//! SystemTime::UNIX_EPOCH, SystemTime::duration_since, SystemTime::elapsed, SystemTime::checked_add, SystemTime::checked_sub,
//! SystemTimeError::duration, thread::sleep, thread::park_timeout, std::hint::black_box
//! DOCS: https://doc.rust-lang.org/std/time/index.html
//! DEFINED IN: core: `Duration` and `TryFromFloatSecsError` (`core::time`, re-exported by std); `black_box` (`core::hint`).
//!   std: `Instant`, `SystemTime`, `UNIX_EPOCH`, `SystemTimeError` (they need an OS clock); `thread::sleep`, `thread::park_timeout`. Nothing here is in alloc.
//! NOT COVERED (and why): calendar types, time zones, parsing and formatting dates (std has none: use `chrono`, `time` or `jiff`); `Duration::from_days`/`from_weeks`,
//! `as_millis_f64`, `SystemTime::{MIN,MAX,saturating_add}` (unstable in 1.98.1); `Duration::SECOND`/`MILLISECOND` constants (unstable); async timers (tokio::time).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   time a piece of work                        -> `let t = Instant::now(); ..; t.elapsed()`
//!   give an operation a total budget            -> compute `deadline = Instant::now() + budget` once; pass `deadline.saturating_duration_since(Instant::now())`
//!   know whether a deadline passed              -> `Instant::now() >= deadline`
//!   get "now" for a database row or a token     -> `SystemTime::now().duration_since(UNIX_EPOCH)`
//!   compare two wall-clock stamps               -> handle the `Err`: the clock can step backwards
//!   configure a timeout from a number           -> `Duration::from_millis(cfg)`; from user floats: `try_from_secs_f64`
//!   exponential backoff without overflow        -> `base.saturating_mul(1 << n.min(31)).min(cap)`
//!   print a duration for humans                 -> `format!("{d:.2?}")` (Debug), or your own formatter; there is no `Display`
//!   test time-dependent logic                   -> inject a clock trait; advance it by hand
//!   stop the optimiser deleting benchmark work  -> `black_box` the inputs AND the result (best effort only)
//!
//! RULES: monotonic (`Instant`) for intervals, wall clock (`SystemTime`) for points in calendar time. Never subtract wall-clock stamps to time work.
//! PLATFORMS (from the 1.98.1 docs/source): `Instant` uses `CLOCK_MONOTONIC` on Linux, `CLOCK_UPTIME_RAW` on Apple targets, QueryPerformanceCounter on Windows.
//! Whether time spent suspended counts is explicitly unspecified. `SystemTime` precision differs too (Windows stores 100 ns ticks).
//! TRY NEXT:
//!   - [S] Predict then assert: `format!("{:?}", Duration::from_micros(1_500))` and the same with `{:.3?}` for 1 s + 1 ns.
//!   - [S] In section 5, compute the retry budget with `checked_mul` and make the 40th attempt return `None` instead of capping.
//!   - [M] Add a `TtlCache<K, V, C: Clock>` and test expiry exactly at the boundary (decide: is `age == ttl` expired?) with `ManualClock`.
//!   - [M] Extend `civil_from_unix` with the weekday and check it against the naive oracle.
//!   - [A] PROJECT: combine with `std_net_tcp_udp_ip_addr` and `std_sync_primitives_use_cases`: a loopback client with one overall deadline split across connect, write and each read.
//! Read: https://doc.rust-lang.org/std/time/struct.Instant.html ; https://doc.rust-lang.org/std/time/struct.SystemTime.html ; civil-from-days algorithm https://howardhinnant.github.io/date_algorithms.html
#![allow(dead_code)]

use std::cell::Cell;
use std::hint::black_box;
use std::panic::catch_unwind;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn panics<R>(f: impl FnOnce() -> R + std::panic::UnwindSafe) -> bool {
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = catch_unwind(f).is_err();
    std::panic::set_hook(old);
    r
}

// ------------------------------------------------------------ 1. Duration: construction and accessors
fn s01_construct() {
    let d = Duration::new(3, 1_500_000_000); // nanos >= 1e9 carry into seconds
    assert_eq!((d.as_secs(), d.subsec_nanos()), (4, 500_000_000));
    assert!(
        panics(|| Duration::new(u64::MAX, 1_000_000_000)),
        "the carry can overflow, and that panics"
    );

    let d = Duration::from_micros(2_034_005);
    assert_eq!(d.as_secs(), 2); // as_secs TRUNCATES
    assert_eq!(
        (d.subsec_millis(), d.subsec_micros(), d.subsec_nanos()),
        (34, 34_005, 34_005_000)
    ); // sub-second part only
    assert_eq!(
        (d.as_millis(), d.as_micros(), d.as_nanos()),
        (2_034, 2_034_005, 2_034_005_000)
    ); // whole value, u128
    // PITFALL: `subsec_millis` is not "the duration in ms"; `as_millis` is.
    // PITFALL: `d.as_millis() as u64` silently truncates huge values; use try_from.
    assert!(u64::try_from(Duration::MAX.as_millis()).is_err());
    assert_eq!(u64::try_from(d.as_millis()), Ok(2_034));

    assert_eq!(Duration::from_mins(2), Duration::from_secs(120)); // 1.91
    assert_eq!(Duration::from_hours(1), Duration::from_secs(3_600)); // 1.91
    assert_eq!(Duration::from_millis(1), Duration::from_nanos(1_000_000));
    assert_eq!(
        Duration::from_nanos_u128(u128::from(u64::MAX) + 1).as_nanos(),
        1u128 << 64
    ); // 1.93
    assert_eq!(Duration::default(), Duration::ZERO);
    assert!(Duration::ZERO.is_zero() && !Duration::from_nanos(1).is_zero());
    assert!(Duration::MAX.as_secs() == u64::MAX);

    // floats: from_secs_f64 PANICS on negative, NaN and too large: never feed it user input directly
    assert_eq!(Duration::from_secs_f64(1.5), Duration::from_millis(1_500));
    assert_eq!(Duration::from_millis(250).as_secs_f64(), 0.25);
    for bad in [-1.0, f64::NAN, f64::INFINITY, 1e30] {
        assert!(panics(move || Duration::from_secs_f64(bad)), "{bad}");
        assert!(Duration::try_from_secs_f64(bad).is_err(), "{bad}");
    }
    println!("1. construction ok");
}

// ------------------------------------------------------------ 2. Duration arithmetic
fn s02_arithmetic() {
    let (a, b) = (Duration::from_secs(10), Duration::from_secs(3));
    assert_eq!(a + b, Duration::from_secs(13));
    assert_eq!(a - b, Duration::from_secs(7));
    assert_eq!(a * 3, Duration::from_secs(30));
    assert_eq!(a / 4, Duration::from_millis(2_500));
    // the operators PANIC on overflow / underflow / divide-by-zero; the checked and saturating forms do not
    assert!(panics(move || b - a));
    assert!(panics(|| Duration::MAX + Duration::from_nanos(1)));
    assert!(panics(move || a / black_box(0u32)));
    assert_eq!(b.checked_sub(a), None);
    assert_eq!(b.saturating_sub(a), Duration::ZERO);
    assert_eq!(Duration::MAX.checked_add(a), None);
    assert_eq!(Duration::MAX.saturating_add(a), Duration::MAX);
    assert_eq!(
        a.checked_mul(u32::MAX),
        Some(Duration::from_secs(10 * u64::from(u32::MAX)))
    );
    assert_eq!(Duration::MAX.checked_mul(2), None);
    assert_eq!(Duration::MAX.saturating_mul(2), Duration::MAX);
    assert_eq!(a.checked_div(0), None);
    assert_eq!(a.abs_diff(b), b.abs_diff(a)); // 1.81: |a - b| without choosing an order
    assert_eq!(a.abs_diff(b), Duration::from_secs(7));
    assert_eq!(a.mul_f64(1.5), Duration::from_secs(15));
    assert_eq!(a.div_f64(4.0), Duration::from_millis(2_500));
    assert_eq!(
        Duration::from_secs(1).div_duration_f64(Duration::from_millis(250)),
        4.0
    ); // 1.80: "how many fit"
    // Sum works, and Ord lets you use min/max/clamp
    let parts = [b, a, Duration::from_millis(500)];
    assert_eq!(
        parts.iter().sum::<Duration>(),
        Duration::from_millis(13_500)
    );
    assert_eq!(parts.iter().max(), Some(&a));
    assert_eq!(Duration::from_secs(90).clamp(b, a), a);
    println!("2. arithmetic ok");
}

// ------------------------------------------------------------ 3. Formatting
fn human(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        return format!("{}ms", d.as_millis());
    }
    // Round to tenths FIRST, then split: otherwise 59.96 s prints as "60.0s".
    let tenths = (d.as_nanos() + 50_000_000) / 100_000_000;
    let (h, m, s, t) = (
        tenths / 36_000,
        tenths / 600 % 60,
        tenths / 10 % 60,
        tenths % 10,
    );
    match (h, m) {
        (0, 0) => format!("{s}.{t}s"),
        (0, _) => format!("{m}m {s}.{t}s"),
        _ => format!("{h}h {m}m {s}.{t}s"),
    }
}

fn s03_format() {
    // There is no Display. Debug picks the largest unit that keeps the integer part non-zero and honours precision and width.
    assert_eq!(format!("{:?}", Duration::from_millis(1_500)), "1.5s");
    assert_eq!(format!("{:?}", Duration::from_micros(1_500)), "1.5ms");
    assert_eq!(format!("{:?}", Duration::from_nanos(1_500)), "1.5µs");
    assert_eq!(format!("{:?}", Duration::from_nanos(15)), "15ns");
    assert_eq!(
        format!("{:?}", Duration::from_secs(3_600)),
        "3600s",
        "never minutes or hours"
    );
    assert_eq!(format!("{:.2?}", Duration::from_micros(1_234_567)), "1.23s");
    assert_eq!(format!("[{:>8?}]", Duration::from_millis(5)), "[     5ms]");
    assert_eq!(human(Duration::from_millis(42)), "42ms");
    assert_eq!(human(Duration::from_millis(3_723_500)), "1h 2m 3.5s");
    assert_eq!(human(Duration::from_millis(59_960)), "1m 0.0s");
    assert_eq!(human(Duration::from_secs(125)), "2m 5.0s");
    println!("3. formatting ok");
}

// ------------------------------------------------------------ 4. Instant
fn s04_instant() {
    let a = Instant::now();
    let b = Instant::now();
    assert!(
        b >= a,
        "monotonically non-decreasing; two reads MAY be equal"
    );
    let later = a + Duration::from_secs(5);
    assert_eq!(later.duration_since(a), Duration::from_secs(5));
    assert_eq!(later - a, Duration::from_secs(5)); // Instant - Instant = Duration
    assert_eq!(later - Duration::from_secs(5), a);
    // wrong order: duration_since, `-` and elapsed currently SATURATE to zero (docs: may panic again in a future version)
    assert_eq!(a.duration_since(later), Duration::ZERO);
    assert_eq!(a.saturating_duration_since(later), Duration::ZERO); // say what you mean: use this one
    assert_eq!(a.checked_duration_since(later), None); // or detect the mistake
    assert_eq!(
        later.checked_duration_since(a),
        Some(Duration::from_secs(5))
    );
    assert_eq!(
        later.elapsed(),
        Duration::ZERO,
        "a deadline in the future has no elapsed time"
    );
    // Instant is opaque and its range is platform dependent: use checked_add for untrusted durations
    assert_eq!(a.checked_add(Duration::MAX), None);
    assert!(panics(move || a + Duration::MAX));
    assert_eq!(later.checked_sub(Duration::from_secs(5)), Some(a));

    // thread::sleep sleeps for AT LEAST the duration. Only the lower bound is safe to assert; scheduling sets no upper bound.
    let t = Instant::now();
    thread::sleep(Duration::from_millis(5));
    let slept = t.elapsed();
    assert!(slept >= Duration::from_millis(5));
    println!("4. instant ok (asked 5ms, slept {slept:.2?})");
}

// ------------------------------------------------------------ 5. Deadlines, backoff, injected clocks
/// Time as "duration since an arbitrary origin". Unlike `Instant`, a test can construct any value.
trait Clock {
    fn now(&self) -> Duration;
    fn sleep(&self, d: Duration);
}
struct RealClock(Instant);
impl Clock for RealClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, d: Duration) {
        thread::sleep(d)
    }
}
#[derive(Default)]
struct ManualClock(Cell<Duration>);
impl Clock for ManualClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
    fn sleep(&self, d: Duration) {
        self.0.set(self.0.get() + d) // virtual time: instant and exact
    }
}

fn backoff(attempt: u32, base: Duration, cap: Duration) -> Duration {
    // 1u32 << 32 would overflow the shift, so bound the exponent as well as the product.
    base.saturating_mul(1u32 << attempt.min(31)).min(cap)
}

#[derive(Debug, PartialEq)]
enum RetryError {
    DeadlineExceeded { attempts: u32 },
}

/// One overall budget; each attempt is told how much of it remains. Never sleeps past the deadline.
fn retry<T>(
    clock: &impl Clock,
    budget: Duration,
    mut op: impl FnMut(Duration) -> Option<T>,
) -> Result<T, RetryError> {
    let deadline = clock.now().saturating_add(budget);
    for attempt in 0.. {
        let remaining = deadline.saturating_sub(clock.now());
        if let Some(v) = op(remaining) {
            return Ok(v);
        }
        let wait = backoff(attempt, Duration::from_millis(100), Duration::from_secs(2));
        if wait >= deadline.saturating_sub(clock.now()) {
            return Err(RetryError::DeadlineExceeded {
                attempts: attempt + 1,
            });
        }
        clock.sleep(wait);
    }
    unreachable!()
}

fn s05_deadlines() {
    let (base, cap) = (Duration::from_millis(100), Duration::from_secs(2));
    let seq: Vec<u128> = (0..7).map(|n| backoff(n, base, cap).as_millis()).collect();
    assert_eq!(seq, [100, 200, 400, 800, 1600, 2000, 2000]);
    assert_eq!(
        backoff(u32::MAX, base, cap),
        cap,
        "no overflow at absurd attempt counts"
    );

    let clock = ManualClock::default();
    let mut seen = Vec::new();
    let r: Result<u32, _> = retry(&clock, Duration::from_secs(10), |remaining| {
        seen.push(remaining.as_millis());
        (seen.len() == 4).then_some(7)
    });
    assert_eq!(r, Ok(7));
    assert_eq!(seen, [10_000, 9_900, 9_700, 9_300]);
    assert_eq!(clock.now(), Duration::from_millis(700));

    let clock = ManualClock::default();
    let r: Result<(), _> = retry(&clock, Duration::from_secs(1), |_| None);
    // waits 100 + 200 + 400 = 700; the next wait (800) would cross the 1000 ms deadline
    assert_eq!(r, Err(RetryError::DeadlineExceeded { attempts: 4 }));
    assert_eq!(clock.now(), Duration::from_millis(700));

    let real = RealClock(Instant::now());
    assert_eq!(retry(&real, Duration::from_secs(60), |_| Some(1)), Ok(1));

    // park_timeout may return early (spurious wake-up or unpark), so a timed wait must loop on the remaining time.
    let deadline = Instant::now() + Duration::from_millis(5);
    thread::current().unpark(); // leaves a token: the first park_timeout returns immediately
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        thread::park_timeout(left);
    }
    assert!(Instant::now() >= deadline);
    println!("5. deadlines / backoff / clock injection ok");
}

// ------------------------------------------------------------ 6. SystemTime and the Unix epoch
fn unix_millis(t: SystemTime) -> i128 {
    // total function: times before 1970 become negative instead of panicking on `.unwrap()`
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i128,
        Err(e) => -(e.duration().as_millis() as i128),
    }
}

/// (year, month, day, hour, minute, second) in UTC, proleptic Gregorian, leap seconds ignored (as Unix time does).
/// Day-to-date arithmetic after Howard Hinnant's `civil_from_days`.
fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400) as u32); // euclid: correct before 1970
    let z = days + 719_468; // shift the origin to 0000-03-01 so the leap day ends the "year"
    let era = z.div_euclid(146_097); // 400-year cycle
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, rem / 3600, rem / 60 % 60, rem % 60)
}

fn rfc3339(t: SystemTime) -> String {
    let secs = unix_millis(t).div_euclid(1000) as i64;
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
fn month_len(y: i64, m: u32) -> u32 {
    match m {
        2 => 28 + u32::from(is_leap(y)),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn s06_systemtime() {
    assert_eq!(SystemTime::UNIX_EPOCH, UNIX_EPOCH);
    assert!(
        SystemTime::now().duration_since(UNIX_EPOCH).is_ok(),
        "assumes the machine clock is after 1970"
    );

    // duration_since returns Result because `earlier` may be later: the error carries the size of the gap
    let before = UNIX_EPOCH.checked_sub(Duration::from_secs(10)).unwrap();
    let err = before.duration_since(UNIX_EPOCH).unwrap_err();
    assert_eq!(err.duration(), Duration::from_secs(10));
    assert_eq!(unix_millis(before), -10_000);
    assert_eq!(
        unix_millis(UNIX_EPOCH + Duration::from_millis(1_500)),
        1_500
    );
    // elapsed() on a future stamp (clock skew between machines) is an Err too
    let future = SystemTime::now() + Duration::from_secs(3_600);
    assert!(future.elapsed().is_err());
    assert_eq!(UNIX_EPOCH.checked_add(Duration::MAX), None);

    assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    assert_eq!(
        rfc3339(UNIX_EPOCH + Duration::from_secs(951_782_400)),
        "2000-02-29T00:00:00Z"
    );
    assert_eq!(
        rfc3339(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        "2023-11-14T22:13:20Z"
    );
    assert_eq!(rfc3339(before), "1969-12-31T23:59:50Z");
    assert_eq!(
        civil_from_unix(i64::from(i32::MAX)),
        (2038, 1, 19, 3, 14, 7)
    ); // the 32-bit time_t limit

    // differential test: walk the calendar one day at a time with the naive rules and compare, from about 1600 to about 2399
    let (mut y, mut m, mut d) = (1970i64, 1u32, 1u32);
    for day in 0..157_000i64 {
        assert_eq!(
            civil_from_unix(day * 86_400 + 43_200),
            (y, m, d, 12, 0, 0),
            "day {day}"
        );
        d += 1;
        if d > month_len(y, m) {
            (m, d) = (m + 1, 1);
            if m > 12 {
                (y, m) = (y + 1, 1);
            }
        }
    }
    assert!(y >= 2399);
    let (mut y, mut m, mut d) = (1969i64, 12u32, 31u32); // and backwards
    for day in 1..135_000i64 {
        assert_eq!(
            civil_from_unix(-day * 86_400),
            (y, m, d, 0, 0, 0),
            "day -{day}"
        );
        d -= 1;
        if d == 0 {
            m -= 1;
            if m == 0 {
                (y, m) = (y - 1, 12);
            }
            d = month_len(y, m);
        }
    }
    assert!(y <= 1601);

    // persisted expiry: wall clock, because the deadline must survive a restart; compare as integers
    let issued = unix_millis(UNIX_EPOCH + Duration::from_secs(1_000));
    let is_expired = |now_ms: i128| now_ms - issued >= 60_000;
    assert!(!is_expired(issued + 59_999) && is_expired(issued + 60_000));
    assert!(
        !is_expired(issued - 5_000),
        "a clock stepped backwards must not expire the token"
    );
    println!("6. systemtime ok");
}

// ------------------------------------------------------------ 7. Rates, stopwatch, benchmarking pitfalls
fn per_second(items: u64, elapsed: Duration) -> Option<f64> {
    (!elapsed.is_zero()).then(|| items as f64 / elapsed.as_secs_f64()) // a zero interval is possible with coarse clocks
}

/// Accumulates only the running intervals. Takes timestamps as arguments, so it is testable without sleeping.
#[derive(Default)]
struct Stopwatch {
    total: Duration,
    started: Option<Duration>,
}
impl Stopwatch {
    fn start(&mut self, now: Duration) {
        self.started.get_or_insert(now); // starting twice keeps the first start
    }
    fn stop(&mut self, now: Duration) {
        if let Some(s) = self.started.take() {
            self.total += now.saturating_sub(s);
        }
    }
    fn elapsed(&self, now: Duration) -> Duration {
        self.total
            + self
                .started
                .map_or(Duration::ZERO, |s| now.saturating_sub(s))
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    assert!(!sorted.is_empty());
    // nearest-rank definition
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn s07_measure() {
    assert_eq!(per_second(500, Duration::from_millis(250)), Some(2_000.0));
    assert_eq!(per_second(500, Duration::ZERO), None);
    let s = Duration::from_secs;
    let mut w = Stopwatch::default();
    w.start(s(10));
    w.start(s(12));
    w.stop(s(15));
    w.stop(s(100));
    assert_eq!(w.elapsed(s(200)), s(5));
    w.start(s(200));
    assert_eq!(w.elapsed(s(202)), s(7));

    let lat: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
    assert_eq!(percentile(&lat, 50.0), Duration::from_millis(50));
    assert_eq!(percentile(&lat, 99.0), Duration::from_millis(99));
    assert_eq!(percentile(&lat, 100.0), Duration::from_millis(100));

    // PITFALLS: (1) in release builds unused work can be deleted: black_box inputs and outputs (documented as best effort, not a guarantee).
    // (2) timing ONE short call measures the clock: time a batch and divide. (3) report a median/percentile of several samples, not one run.
    // (4) For real benchmarks use criterion or divan; this is only a sanity probe.
    let mut samples: Vec<Duration> = (0..7)
        .map(|_| {
            let t = Instant::now();
            let mut acc = 0u64;
            for i in 0..10_000u64 {
                acc = acc.wrapping_add(black_box(i) * i);
            }
            black_box(acc);
            t.elapsed()
        })
        .collect();
    samples.sort();
    println!(
        "7. measuring ok (median batch {:.2?}; timing is printed, never asserted)",
        samples[3]
    );
}

fn main() {
    s01_construct();
    s02_arithmetic();
    s03_format();
    s04_instant();
    s05_deadlines();
    s06_systemtime();
    s07_measure();
    println!("all time sections passed");
}
