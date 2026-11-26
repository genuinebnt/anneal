//! RECIPE 182 (systems): civil dates and time from scratch: days<->civil, RFC 3339, month arithmetic, clocks
//! LEVEL: intermediate
//! Use in: parsing timestamps safely, scheduling, understanding what chrono/time/jiff do.   Run: cargo run --release --example civil_dates_time
#![allow(dead_code)]
//!
//! WHAT: the proleptic Gregorian calendar as pure integer arithmetic.
//!  * `days_from_civil(y, m, d)` / `civil_from_days(z)` (Howard Hinnant's algorithms): count days since 1970-01-01 by
//!    shifting the year to start on March 1, so the leap day is the LAST day of the "year" and era math is trivial.
//!  * weekday = (days + 4) mod 7 (1970-01-01 was a Thursday).
//!  * RFC 3339 / ISO 8601 profile `YYYY-MM-DDTHH:MM:SS[.fff](Z|+HH:MM|-HH:MM)`: strict, byte-based parser returning a
//!    typed `ParseError`, never panicking (fuzzed below). Formatting round-trips.
//!  * LEAP SECOND POLICY: Unix time has no second 60. We accept `:60` ONLY when the UTC instant is the final second
//!    of a June 30 or December 31 (the only places leap seconds have been inserted) and CLAMP it to `:59`; anything else,
//!    such as `2024-02-29T23:59:60Z`, is `ParseError::LeapSecond`. Applications needing real UTC leap data need a table.
//!  * add_months clamps to the end of the month (Jan 31 + 1 month = Feb 28/29); it is NOT associative, which is why
//!    calendar periods (months, years) are different types from exact durations (seconds).
//!  * Instant (monotonic, never goes backwards, arbitrary origin) vs SystemTime (wall clock; NTP/manual steps can move
//!    it backwards). Use Instant for timeouts/elapsed, wall time only for timestamps shown to humans or stored.
//!  * `Clock` trait with a real and a manual implementation so time-dependent logic is testable.
//!
//! COMMON USES: log/trace timestamps, API date fields, token expiry (`exp` claims are Unix seconds), billing periods
//! ("same day next month"), cron-like schedulers, cache TTLs. In production prefer a maintained library: BurntSushi's
//! jiff (https://github.com/BurntSushi/jiff), chrono or time; they add time zones/DST, which this recipe deliberately omits.
//! (Across a DST change "+1 day" and "+24 hours" differ in a zoned type; fixed offsets here never change.)
//! Read: http://howardhinnant.github.io/date_algorithms.html and https://www.rfc-editor.org/rfc/rfc3339
//! and https://github.com/BurntSushi/jiff
use std::cell::Cell;
use std::fmt;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------- calendar core
const fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
const fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Days since 1970-01-01 for a valid civil date (Hinnant). Floor division handles negative years/eras.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = (m as i64 + 9) % 12; // March = 0 ... February = 11
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

/// 0 = Sunday ... 6 = Saturday.
fn weekday(days: i64) -> u32 {
    (days + 4).rem_euclid(7) as u32
}
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Date {
    year: i64,
    month: u32,
    day: u32,
}
impl Date {
    fn new(year: i64, month: u32, day: u32) -> Result<Date, ParseError> {
        if !(1..=12).contains(&month) {
            return Err(ParseError::Month(month));
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(ParseError::Day { month, day });
        }
        Ok(Date { year, month, day })
    }
    fn to_days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }
    fn from_days(z: i64) -> Date {
        let (year, month, day) = civil_from_days(z);
        Date { year, month, day }
    }
    fn weekday(self) -> &'static str {
        WEEKDAYS[weekday(self.to_days()) as usize]
    }
    /// Calendar-period addition with end-of-month clamping.
    fn add_months(self, n: i64) -> Date {
        let total = self.year * 12 + (self.month as i64 - 1) + n;
        let (year, month) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
        Date {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }
    fn add_days(self, n: i64) -> Date {
        Date::from_days(self.to_days() + n)
    }
}
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

// ---------------------------------------------------------------- RFC 3339
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseError {
    Length,
    Unexpected { pos: usize, expected: &'static str },
    Month(u32),
    Day { month: u32, day: u32 },
    Hour(u32),
    Minute(u32),
    Second(u32),
    LeapSecond,
    Offset,
    Trailing,
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::Unexpected { pos, expected } => {
                write!(f, "expected {expected} at byte {pos}")
            }
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for ParseError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DateTime {
    date: Date,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
    offset_min: i32, // minutes east of UTC
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}
impl Cursor<'_> {
    fn digits(&mut self, n: usize, what: &'static str) -> Result<u32, ParseError> {
        let mut v = 0;
        for _ in 0..n {
            match self.b.get(self.i) {
                Some(c @ b'0'..=b'9') => v = v * 10 + (c - b'0') as u32,
                _ => {
                    return Err(ParseError::Unexpected {
                        pos: self.i,
                        expected: what,
                    });
                }
            }
            self.i += 1;
        }
        Ok(v)
    }
    fn byte(&mut self, set: &[u8], what: &'static str) -> Result<u8, ParseError> {
        match self.b.get(self.i) {
            Some(c) if set.contains(c) => {
                self.i += 1;
                Ok(*c)
            }
            _ => Err(ParseError::Unexpected {
                pos: self.i,
                expected: what,
            }),
        }
    }
}

impl DateTime {
    fn parse(s: &str) -> Result<DateTime, ParseError> {
        if s.len() < 20 || s.len() > 64 {
            return Err(ParseError::Length);
        }
        let mut c = Cursor {
            b: s.as_bytes(),
            i: 0,
        };
        let year = c.digits(4, "year digit")? as i64;
        c.byte(b"-", "'-'")?;
        let month = c.digits(2, "month digits")?;
        c.byte(b"-", "'-'")?;
        let day = c.digits(2, "day digits")?;
        c.byte(b"Tt", "'T'")?;
        let hour = c.digits(2, "hour digits")?;
        c.byte(b":", "':'")?;
        let minute = c.digits(2, "minute digits")?;
        c.byte(b":", "':'")?;
        let second = c.digits(2, "second digits")?;
        let date = Date::new(year, month, day)?;
        if hour > 23 {
            return Err(ParseError::Hour(hour));
        }
        if minute > 59 {
            return Err(ParseError::Minute(minute));
        }
        if second > 60 {
            return Err(ParseError::Second(second));
        }
        let mut nanos = 0u32;
        if c.b.get(c.i) == Some(&b'.') {
            c.i += 1;
            let start = c.i;
            while let Some(d @ b'0'..=b'9') = c.b.get(c.i) {
                if c.i - start < 9 {
                    nanos = nanos * 10 + (d - b'0') as u32;
                } // digits beyond nanoseconds are truncated
                c.i += 1;
            }
            let n = c.i - start;
            if n == 0 {
                return Err(ParseError::Unexpected {
                    pos: c.i,
                    expected: "fraction digits",
                });
            }
            nanos *= 10u32.pow(9 - n.min(9) as u32);
        }
        let offset_min = match c.byte(b"Zz+-", "'Z' or offset")? {
            b'Z' | b'z' => 0,
            sign => {
                let (h, m) = (c.digits(2, "offset hour")?, {
                    c.byte(b":", "':'")?;
                    c.digits(2, "offset minute")?
                });
                if h > 23 || m > 59 {
                    return Err(ParseError::Offset);
                }
                let v = (h * 60 + m) as i32;
                if sign == b'-' { -v } else { v }
            }
        };
        if c.i != c.b.len() {
            return Err(ParseError::Trailing);
        }
        let mut dt = DateTime {
            date,
            hour,
            minute,
            second,
            nanos,
            offset_min,
        };
        if second == 60 {
            // Leap-second policy: only the very last UTC second of Jun 30 / Dec 31 is plausible; clamp it to :59.
            dt.second = 59;
            let next = dt.unix_seconds() + 1;
            let (_, m, d) = civil_from_days(next.div_euclid(86_400));
            if next.rem_euclid(86_400) != 0 || !matches!((m, d), (1, 1) | (7, 1)) {
                return Err(ParseError::LeapSecond);
            }
        }
        Ok(dt)
    }

    /// Seconds since the Unix epoch (UTC), ignoring nanoseconds.
    fn unix_seconds(&self) -> i64 {
        self.date.to_days() * 86_400 + (self.hour * 3600 + self.minute * 60 + self.second) as i64
            - self.offset_min as i64 * 60
    }

    fn from_unix(secs: i64, nanos: u32, offset_min: i32) -> DateTime {
        let local = secs + offset_min as i64 * 60;
        let (days, sod) = (local.div_euclid(86_400), local.rem_euclid(86_400) as u32);
        DateTime {
            date: Date::from_days(days),
            hour: sod / 3600,
            minute: sod / 60 % 60,
            second: sod % 60,
            nanos,
            offset_min,
        }
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}T{:02}:{:02}:{:02}",
            self.date, self.hour, self.minute, self.second
        )?;
        if self.nanos > 0 {
            write!(f, ".{}", format!("{:09}", self.nanos).trim_end_matches('0'))?;
        }
        if self.offset_min == 0 {
            write!(f, "Z")
        } else {
            let a = self.offset_min.abs();
            write!(
                f,
                "{}{:02}:{:02}",
                if self.offset_min < 0 { '-' } else { '+' },
                a / 60,
                a % 60
            )
        }
    }
}

// ---------------------------------------------------------------- clocks
trait Clock {
    /// Wall time as (unix seconds, nanos). May jump backwards!
    fn wall(&self) -> (i64, u32);
    /// Monotonic time since an arbitrary origin. Never decreases.
    fn mono(&self) -> Duration;
}
struct SystemClock {
    origin: Instant,
}
impl Clock for SystemClock {
    fn wall(&self) -> (i64, u32) {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => (d.as_secs() as i64, d.subsec_nanos()),
            Err(e) => (-(e.duration().as_secs() as i64), 0), // clock set before 1970
        }
    }
    fn mono(&self) -> Duration {
        self.origin.elapsed()
    }
}
struct ManualClock {
    wall_ns: Cell<i128>,
    mono: Cell<Duration>,
}
impl ManualClock {
    fn new(unix_secs: i64) -> Self {
        ManualClock {
            wall_ns: Cell::new(unix_secs as i128 * 1_000_000_000),
            mono: Cell::new(Duration::ZERO),
        }
    }
    /// Time passes: both clocks advance.
    fn advance(&self, d: Duration) {
        self.wall_ns.set(self.wall_ns.get() + d.as_nanos() as i128);
        self.mono.set(self.mono.get() + d);
    }
    /// An NTP step / admin `date -s`: only the wall clock moves.
    fn step_wall(&self, secs: i64) {
        self.wall_ns
            .set(self.wall_ns.get() + secs as i128 * 1_000_000_000);
    }
}
impl Clock for ManualClock {
    fn wall(&self) -> (i64, u32) {
        let ns = self.wall_ns.get();
        (
            ns.div_euclid(1_000_000_000) as i64,
            ns.rem_euclid(1_000_000_000) as u32,
        )
    }
    fn mono(&self) -> Duration {
        self.mono.get()
    }
}

/// Elapsed time measured both ways.
fn elapsed_wall_secs<C: Clock>(c: &C, start: (i64, u32)) -> f64 {
    let now = c.wall();
    (now.0 - start.0) as f64 + (now.1 as f64 - start.1 as f64) / 1e9
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn main() {
    // 1. known values
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    assert_eq!(days_from_civil(1969, 12, 31), -1);
    assert!(Date::new(2000, 2, 29).is_ok() && Date::new(2024, 2, 29).is_ok());
    assert!(
        Date::new(1900, 2, 29).is_err()
            && Date::new(2100, 2, 29).is_err()
            && Date::new(2023, 2, 29).is_err()
    );
    assert!(is_leap(2000) && is_leap(2024) && !is_leap(1900) && !is_leap(2100));
    assert_eq!(Date::new(1970, 1, 1).unwrap().weekday(), "Thu");
    assert_eq!(Date::new(2000, 1, 1).unwrap().weekday(), "Sat");
    assert_eq!(Date::new(2024, 2, 29).unwrap().weekday(), "Thu");
    assert_eq!(Date::new(1969, 7, 20).unwrap().weekday(), "Sun");
    assert_eq!(
        DateTime::from_unix(1_700_000_000, 0, 0).to_string(),
        "2023-11-14T22:13:20Z"
    );
    assert_eq!(
        DateTime::from_unix(2_147_483_647, 0, 0).to_string(),
        "2038-01-19T03:14:07Z"
    );
    assert_eq!(
        DateTime::parse("2000-02-29T00:00:00Z")
            .unwrap()
            .unix_seconds(),
        951_782_400
    );

    // 2. round trip over a wide range, and the civil sequence advances one day at a time
    let mut prev = days_from_civil(-400, 1, 1) - 1;
    for y in -400..=2800i64 {
        for m in 1..=12 {
            for d in 1..=days_in_month(y, m) {
                let z = days_from_civil(y, m, d);
                assert_eq!(
                    z,
                    prev + 1,
                    "consecutive civil dates must be consecutive day numbers ({y}-{m}-{d})"
                );
                assert_eq!(civil_from_days(z), (y, m, d));
                prev = z;
            }
        }
    }
    for z in (-3_000_000..3_000_000).step_by(7) {
        let (y, m, d) = civil_from_days(z);
        assert_eq!(days_from_civil(y, m, d), z);
    }
    assert_eq!(
        days_from_civil(2400, 1, 1) - days_from_civil(2000, 1, 1),
        146_097,
        "400 years = 146097 days"
    );
    println!("days<->civil: round trip ok for years -400..=2800 and +-3M days around the epoch");

    // 3. RFC 3339 parse/format
    let dt = DateTime::parse("2024-03-10T12:30:45.5+05:30").unwrap();
    assert_eq!(
        dt.unix_seconds(),
        DateTime::parse("2024-03-10T07:00:45.5Z")
            .unwrap()
            .unix_seconds()
    );
    assert_eq!(dt.to_string(), "2024-03-10T12:30:45.5+05:30");
    assert_eq!(
        DateTime::from_unix(dt.unix_seconds(), 0, -480).to_string(),
        "2024-03-09T23:00:45-08:00"
    );
    let bad = [
        (
            "2023-02-29T00:00:00Z",
            ParseError::Day { month: 2, day: 29 },
        ),
        ("2024-13-01T00:00:00Z", ParseError::Month(13)),
        ("2024-01-01T24:00:00Z", ParseError::Hour(24)),
        ("2024-01-01T00:60:00Z", ParseError::Minute(60)),
        ("2024-01-01T00:00:61Z", ParseError::Second(61)),
        ("2024-02-29T23:59:60Z", ParseError::LeapSecond),
        ("2024-01-01T00:00:00+24:00", ParseError::Offset),
        ("2024-01-01T00:00:00Zjunk", ParseError::Trailing),
        ("short", ParseError::Length),
    ];
    for (s, want) in bad {
        assert_eq!(DateTime::parse(s), Err(want.clone()), "{s}");
    }
    // a real leap second (2016-12-31 23:59:60 UTC) is clamped to :59; same instant seen from +01:00 also accepted
    assert_eq!(
        DateTime::parse("2016-12-31T23:59:60Z").unwrap().to_string(),
        "2016-12-31T23:59:59Z"
    );
    assert!(DateTime::parse("2017-01-01T00:59:60+01:00").is_ok());
    assert!(DateTime::parse("2016-12-31T23:59:60+01:00").is_err());
    println!(
        "parse: {} -> unix {}; policy: 2024-02-29T23:59:60Z is rejected: {}",
        dt,
        dt.unix_seconds(),
        DateTime::parse("2024-02-29T23:59:60Z").unwrap_err()
    );

    // 4. fuzz: random and mutated strings never panic; every Ok value re-formats and re-parses identically
    let mut rng = Rng(0xC0FFEE);
    let alphabet: Vec<char> = "0123456789-:TtZz+. \u{e9}\u{1F600}x".chars().collect();
    let seeds = [
        "2024-02-29T23:59:60Z",
        "2016-12-31T23:59:60Z",
        "2024-03-10T12:30:45.123456789-07:00",
        "1999-12-31T23:59:59Z",
    ];
    let (mut ok, mut err) = (0u32, 0u32);
    for i in 0..200_000 {
        let s: String = if i % 2 == 0 {
            (0..rng.next() % 40)
                .map(|_| alphabet[(rng.next() % alphabet.len() as u64) as usize])
                .collect()
        } else {
            let mut cs: Vec<char> = seeds[(rng.next() % 4) as usize].chars().collect();
            for _ in 0..1 + rng.next() % 3 {
                let k = (rng.next() % cs.len() as u64) as usize;
                cs[k] = alphabet[(rng.next() % alphabet.len() as u64) as usize];
            }
            cs.into_iter().collect()
        };
        match DateTime::parse(&s) {
            Ok(d) => {
                ok += 1;
                let again = DateTime::parse(&d.to_string()).expect("formatted output must parse");
                assert_eq!(again.unix_seconds(), d.unix_seconds());
                assert_eq!(again.nanos, d.nanos);
            }
            Err(_) => err += 1,
        }
    }
    assert!(ok > 100 && err > 100_000);
    println!("fuzz: 200000 strings, {ok} parsed, {err} rejected, 0 panics");

    // 5. month arithmetic and calendar vs exact durations
    let jan31 = Date::new(2024, 1, 31).unwrap();
    assert_eq!(jan31.add_months(1).to_string(), "2024-02-29");
    assert_eq!(
        Date::new(2023, 1, 31).unwrap().add_months(1).to_string(),
        "2023-02-28"
    );
    assert_eq!(jan31.add_months(-2).to_string(), "2023-11-30");
    assert_eq!(jan31.add_months(12).to_string(), "2025-01-31");
    assert_eq!(
        Date::new(2024, 2, 29).unwrap().add_months(12).to_string(),
        "2025-02-28"
    );
    assert_ne!(
        jan31.add_months(1).add_months(1),
        jan31.add_months(2),
        "add_months is not associative: 03-29 vs 03-31"
    );
    assert_eq!(jan31.add_days(30).to_string(), "2024-03-01");
    let (a, b) = (
        Date::new(2024, 1, 1).unwrap(),
        Date::new(2025, 1, 1).unwrap(),
    );
    println!(
        "months: Jan 31 + 1mo = {}, + 2 x 1mo = {}, + 2mo = {}; 2024 has {} days = {} s (a 'year' is not a fixed Duration)",
        jan31.add_months(1),
        jan31.add_months(1).add_months(1),
        jan31.add_months(2),
        b.to_days() - a.to_days(),
        (b.to_days() - a.to_days()) * 86_400
    );

    // 6. clocks: the wall clock can go backwards, the monotonic clock cannot
    let clock = ManualClock::new(1_700_000_000);
    let (w0, m0) = (clock.wall(), clock.mono());
    clock.advance(Duration::from_secs(10));
    clock.step_wall(-3600); // NTP/admin step back by one hour
    clock.advance(Duration::from_secs(5));
    let wall_elapsed = elapsed_wall_secs(&clock, w0);
    let mono_elapsed = clock.mono() - m0;
    assert!(
        wall_elapsed < 0.0,
        "wall-clock elapsed went negative: {wall_elapsed}"
    );
    assert_eq!(mono_elapsed, Duration::from_secs(15));
    println!(
        "after a -1h step: wall says {wall_elapsed} s elapsed, monotonic says {mono_elapsed:?}"
    );
    let sys = SystemClock {
        origin: Instant::now(),
    };
    let (m1, m2) = (sys.mono(), sys.mono());
    assert!(m2 >= m1, "Instant is monotonic");
    let w = sys.wall().0;
    assert!(w > 1_700_000_000, "system clock should be after Nov 2023");
    println!("now (system wall clock): {}", DateTime::from_unix(w, 0, 0));
    println!("civil_dates_time: all assertions passed");
}
