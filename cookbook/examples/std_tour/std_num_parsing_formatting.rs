//! RECIPE 1029 (std): number parsing and formatting end to end: FromStr with every error, radix, grouping, hex dumps,
//! human-readable sizes and durations, rounding for display, round-trip properties, a money type.
//! LEVEL: intermediate
//! Use in: config and query-parameter parsing, CSV import, CLI output, logs, invoices.   Run: cargo run --release --example std_num_parsing_formatting
//! COMMON USES: `?limit=50` validation, "1,234,567" display, `hexdump -C` style debugging, "1.5 MiB", "2h 03m",
//! "30s"/"5m" config durations, printing cents without floating point.
//! METHODS: (everyday first) str::parse, std::str::FromStr, std::num::ParseIntError, ParseIntError::kind, std::num::IntErrorKind,
//! METHODS: std::num::ParseFloatError, std::str::ParseBoolError, std::char::ParseCharError, u32::from_str_radix, char::to_digit, std::format,
//! METHODS: f64::to_string, f64::to_bits, f64::from_bits, f64::is_finite, f64::round, u64::ilog10, u64::div_ceil, u64::unsigned_abs,
//! METHODS: i64::checked_mul, i64::checked_add, i64::checked_sub, std::time::Duration, Duration::from_secs, Duration::as_secs,
//! METHODS: Duration::subsec_millis, Duration::from_millis, Duration::checked_add, slice::chunks, str::split_at_checked
//! DOCS: https://doc.rust-lang.org/std/str/trait.FromStr.html
//! DEFINED IN: core (`core::str::FromStr`, `core::num` errors and from_str_radix, float parsing, `core::time::Duration`);
//! `format!`/to_string are alloc.
//! NOT COVERED (and why): int from_ascii / from_ascii_radix (unstable in 1.98.1), locale-aware formatting (std has none),
//! arbitrary-precision decimals (needs a crate). Format spec details: std_fmt_display_debug_deep. Casts: std_integer_methods_overflow.
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   parse a number                        -> s.trim().parse::<u32>()        (parse itself does NOT trim)
//!   know WHY an int failed                -> err.kind() -> IntErrorKind
//!   why a float failed                    -> only Display ("cannot parse float from empty string" / "invalid float literal")
//!   reject inf and NaN                    -> parse then is_finite()
//!   hex / binary input                    -> from_str_radix(stripped, 16)    (strip "0x" yourself)
//!   thousands separators                  -> by hand (no format flag)
//!   fixed decimals for display            -> format!("{x:.2}")
//!   exact round trip                      -> format!("{x}") then parse (shortest digits that read back equal)
//!   money                                 -> integer minor units; split with / and % for display
//!   "30s" style config                    -> split the suffix, checked_mul the unit
//!
//! Read: https://doc.rust-lang.org/std/str/trait.FromStr.html , https://doc.rust-lang.org/std/primitive.f64.html
//! TRY NEXT:
//!   - [S] Remove `.trim()` in `parse_limit` and find every test that changes.
//!   - [S] Parse "1e400" and "-0" as f64 and decide whether your service should accept them; encode the decision in `parse_ratio`.
//!   - [M] Extend `parse_duration` to compound forms ("1h30m"), rejecting repeated or out-of-order units.
//!   - [M] Make `Money` parse input with a thousands separator, and add a round-trip property test for it.
//!   - [A] PROJECT: a CSV importer: split lines (std_str_search_split_parse), parse typed columns here, and accumulate every
//!         error with row and column numbers (std_result_methods).
use std::collections::HashSet;
use std::fmt;
use std::num::{IntErrorKind, NonZeroU32};
use std::str::FromStr;
use std::time::Duration;

const COVERED: &[&str] = &[
    "parse",
    "FromStr",
    "ParseIntError",
    "kind",
    "IntErrorKind",
    "ParseFloatError",
    "ParseBoolError",
    "ParseCharError",
    "from_str_radix",
    "to_digit",
    "format",
    "to_string",
    "to_bits",
    "from_bits",
    "is_finite",
    "round",
    "ilog10",
    "div_ceil",
    "unsigned_abs",
    "checked_mul",
    "checked_add",
    "checked_sub",
    "Duration",
    "from_secs",
    "as_secs",
    "subsec_millis",
    "from_millis",
    "chunks",
    "split_at_checked",
];

struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// ---- 1. Everyday core: validated integer input -----------------------------------------------------------------
#[derive(Debug, PartialEq)]
enum LimitError {
    Empty,
    NotANumber,
    TooLarge,
    Negative,
    Zero,
}

/// A `?limit=` query parameter: 1..=1000, default handled by the caller.
fn parse_limit(raw: &str) -> Result<NonZeroU32, LimitError> {
    let s = raw.trim();
    // Parse signed and wider than needed, so "-5" gives a better message than "invalid digit".
    let n: i64 = s
        .parse()
        .map_err(|e: std::num::ParseIntError| match e.kind() {
            IntErrorKind::Empty => LimitError::Empty,
            IntErrorKind::InvalidDigit => LimitError::NotANumber,
            IntErrorKind::PosOverflow => LimitError::TooLarge,
            IntErrorKind::NegOverflow => LimitError::Negative,
            IntErrorKind::Zero => LimitError::Zero, // only produced by NonZero parsing
            _ => LimitError::NotANumber, // the enum is #[non_exhaustive], so a wildcard is required
        })?;
    if n < 0 {
        return Err(LimitError::Negative);
    }
    if n > 1000 {
        return Err(LimitError::TooLarge);
    }
    NonZeroU32::new(n as u32).ok_or(LimitError::Zero)
}

fn section_ints() {
    assert_eq!(parse_limit(" 50\n").map(NonZeroU32::get), Ok(50));
    assert_eq!(parse_limit("+7").map(NonZeroU32::get), Ok(7));
    assert_eq!(parse_limit("007").map(NonZeroU32::get), Ok(7)); // leading zeros are accepted (decimal, not octal)
    for (input, want) in [
        ("", LimitError::Empty),
        ("   ", LimitError::Empty),
        ("ten", LimitError::NotANumber),
        ("1_000", LimitError::NotANumber), // underscores are Rust literal syntax only
        ("1,000", LimitError::NotANumber),
        ("0x10", LimitError::NotANumber),
        ("1.0", LimitError::NotANumber),
        ("+", LimitError::NotANumber),
        ("99999999999999999999", LimitError::TooLarge),
        ("-99999999999999999999", LimitError::Negative),
        ("-5", LimitError::Negative),
        ("1001", LimitError::TooLarge),
        ("0", LimitError::Zero),
    ] {
        assert_eq!(parse_limit(input), Err(want), "{input:?}");
    }
    assert_eq!(
        "0".parse::<NonZeroU32>().unwrap_err().kind(),
        &IntErrorKind::Zero
    );
    let msg = "x".parse::<u8>().unwrap_err().to_string();
    assert_eq!(msg, "invalid digit found in string"); // generic: add the field name and the input yourself
    println!("1. integer parsing ok");
}

// ---- 2. Floats, bool, char -------------------------------------------------------------------------------------
/// A ratio in 0..=1. f64's parser happily accepts "NaN", "inf" and huge exponents.
fn parse_ratio(s: &str) -> Result<f64, String> {
    let x: f64 = s
        .trim()
        .parse()
        .map_err(|e: std::num::ParseFloatError| format!("ratio {s:?}: {e}"))?;
    if !x.is_finite() || !(0.0..=1.0).contains(&x) {
        return Err(format!("ratio {s:?} is outside 0..=1"));
    }
    Ok(x)
}

fn section_floats() {
    for ok in ["1", "1.", ".5", "+0.5", "5e-1", "5E-1"] {
        assert!(parse_ratio(ok).is_ok(), "{ok}");
    }
    assert!("NaN".parse::<f64>().unwrap().is_nan());
    assert_eq!("-iNfInItY".parse::<f64>(), Ok(f64::NEG_INFINITY)); // case-insensitive
    assert_eq!("1e400".parse::<f64>(), Ok(f64::INFINITY)); // too large is NOT an error
    assert_eq!("1e-400".parse::<f64>(), Ok(0.0)); // too small rounds to zero
    for bad in ["", ".", "e5", "1e", "1,5", " 1", "1f64", "0x1", "1_0"] {
        assert!(bad.parse::<f64>().is_err(), "{bad:?}");
    }
    assert_eq!(
        "".parse::<f64>().unwrap_err().to_string(),
        "cannot parse float from empty string"
    );
    assert_eq!(
        "x".parse::<f64>().unwrap_err().to_string(),
        "invalid float literal"
    );
    assert!(
        parse_ratio("NaN").is_err() && parse_ratio("inf").is_err() && parse_ratio("1.5").is_err()
    );

    // bool accepts exactly "true" and "false".
    assert_eq!(
        ("true".parse::<bool>(), "false".parse::<bool>()),
        (Ok(true), Ok(false))
    );
    for bad in ["TRUE", "1", "yes", " true"] {
        let e: std::str::ParseBoolError = bad.parse::<bool>().unwrap_err();
        assert_eq!(e.to_string(), "provided string was not `true` or `false`");
    }
    // char needs exactly one scalar value.
    assert_eq!("é".parse::<char>(), Ok('é'));
    let two: std::char::ParseCharError = "ab".parse::<char>().unwrap_err();
    assert!("".parse::<char>().is_err() && "e\u{301}".parse::<char>().is_err());
    assert_ne!(two, "".parse::<char>().unwrap_err()); // distinct kinds, though the kind enum is private
    println!("2. float/bool/char parsing ok");
}

// ---- 3. Your own FromStr: durations from config ----------------------------------------------------------------
#[derive(Debug, PartialEq)]
struct Timeout(Duration);
#[derive(Debug, PartialEq)]
enum TimeoutError {
    NoUnit,
    BadNumber,
    BadUnit(String),
    Overflow,
}
impl FromStr for Timeout {
    type Err = TimeoutError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split = s
            .find(|c: char| !c.is_ascii_digit())
            .ok_or(TimeoutError::NoUnit)?;
        let (num, unit) = s.split_at(split);
        let n: u64 = num.parse().map_err(|_| TimeoutError::BadNumber)?;
        let d = match unit {
            "ms" => Duration::from_millis(n),
            "s" => Duration::from_secs(n),
            "m" => Duration::from_secs(n.checked_mul(60).ok_or(TimeoutError::Overflow)?),
            "h" => Duration::from_secs(n.checked_mul(3600).ok_or(TimeoutError::Overflow)?),
            other => return Err(TimeoutError::BadUnit(other.into())),
        };
        Ok(Timeout(d))
    }
}

fn section_from_str() {
    assert_eq!("250ms".parse(), Ok(Timeout(Duration::from_millis(250))));
    assert_eq!("2m".parse(), Ok(Timeout(Duration::from_secs(120))));
    assert_eq!("30".parse::<Timeout>(), Err(TimeoutError::NoUnit)); // a bare number is ambiguous: refuse it
    assert_eq!("s".parse::<Timeout>(), Err(TimeoutError::BadNumber));
    assert_eq!("-1s".parse::<Timeout>(), Err(TimeoutError::BadNumber));
    assert_eq!(
        "1.5s".parse::<Timeout>(),
        Err(TimeoutError::BadUnit(".5s".into()))
    );
    assert_eq!(
        "18446744073709551615h".parse::<Timeout>(),
        Err(TimeoutError::Overflow)
    );
    assert!(
        Duration::MAX
            .checked_add(Duration::from_millis(1))
            .is_none()
    );
    // Radix input.
    let color = |s: &str| -> Option<(u8, u8, u8)> {
        let h = s.strip_prefix('#')?;
        // from_str_radix would accept a leading '+', so check the digits first.
        if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        let [_, r, g, b] = v.to_be_bytes();
        Some((r, g, b))
    };
    assert_eq!(color("#ff8000"), Some((255, 128, 0)));
    assert_eq!(
        (color("#+f8000"), color("#ff80"), color("ff8000")),
        (None, None, None)
    );
    assert_eq!(u8::from_str_radix("+f", 16), Ok(15)); // the reason for the check above
    assert_eq!(
        ('7'.to_digit(10), 'z'.to_digit(36), 'z'.to_digit(10)),
        (Some(7), Some(35), None)
    );
    println!("3. custom FromStr and radix ok");
}

// ---- 4. Grouping, sizes, durations -----------------------------------------------------------------------------
fn group_thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string(); // unsigned_abs: i64::MIN has no positive i64
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Binary units with one decimal, using integers only (no float rounding surprises). Rounds DOWN.
fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let exp = (n.ilog2() / 10) as usize;
    let scale = 1u64 << (10 * exp);
    let tenths = (u128::from(n) * 10 / u128::from(scale)) as u64;
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[exp])
}

fn human_duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0 => format!("{}ms", d.subsec_millis()),
        1..60 => format!("{s}.{:03}s", d.subsec_millis()),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        3600..86400 => format!("{}h {:02}m", s / 3600, s % 3600 / 60),
        _ => format!("{}d {:02}h", s / 86400, s % 86400 / 3600),
    }
}

fn section_human() {
    for (n, want) in [
        (0, "0"),
        (999, "999"),
        (1000, "1,000"),
        (-1234567, "-1,234,567"),
        (100_000, "100,000"),
    ] {
        assert_eq!(group_thousands(n), want);
    }
    assert_eq!(group_thousands(i64::MIN), "-9,223,372,036,854,775,808");
    let mut rng = XorShift(99);
    for _ in 0..5000 {
        let n = (rng.next() as i64) >> (rng.next() % 60);
        let g = group_thousands(n);
        assert_eq!(g.replace(',', "").parse(), Ok(n));
        let body = g.trim_start_matches('-');
        assert!(body.split(',').enumerate().all(|(i, p)| if i == 0 {
            (1..=3).contains(&p.len())
        } else {
            p.len() == 3
        }));
    }
    for (n, want) in [
        (1023, "1023 B"),
        (1024, "1.0 KiB"),
        (1536, "1.5 KiB"),
        (1048575, "1023.9 KiB"),
        (1 << 20, "1.0 MiB"),
        (u64::MAX, "15.9 EiB"),
    ] {
        assert_eq!(human_bytes(n), want);
    }
    // The float version can print "1024.0 KiB": rounding happens after the unit was chosen.
    assert_eq!(format!("{:.1} KiB", 1048575.0 / 1024.0), "1024.0 KiB");
    for (ms, want) in [
        (7, "7ms"),
        (1_500, "1.500s"),
        (61_000, "1m 01s"),
        (3_720_000, "1h 02m"),
        (90_000_000, "1d 01h"),
    ] {
        assert_eq!(human_duration(Duration::from_millis(ms)), want);
    }
    // std's own Debug for Duration is handy in logs.
    assert_eq!(
        format!(
            "{:?} {:?} {:.1?}",
            Duration::from_millis(1500),
            Duration::from_micros(7),
            Duration::from_millis(1250)
        ),
        "1.5s 7µs 1.2s"
    );
    // "How many pages" and "how many digits".
    assert_eq!((101u64.div_ceil(25), 12345u64.ilog10() + 1), (5, 5));
    println!("4. human-readable output ok");
}

// ---- 5. Hex dump -----------------------------------------------------------------------------------------------
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").expect("String write");
    }
    s
}
fn unhex(s: &str) -> Option<Vec<u8>> {
    let mut rest = s.as_bytes();
    let mut out = Vec::with_capacity(rest.len() / 2);
    while !rest.is_empty() {
        let (pair, tail) = rest.split_at_checked(2)?; // None on odd length
        let hi = char::from(pair[0]).to_digit(16)?;
        let lo = char::from(pair[1]).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        rest = tail;
    }
    Some(out)
}
/// offset, 16 bytes in two groups of 8, printable ASCII column.
fn hexdump(data: &[u8]) -> String {
    data.chunks(16)
        .enumerate()
        .map(|(row, chunk)| {
            let cells: Vec<String> = (0..16)
                .map(|i| {
                    chunk
                        .get(i)
                        .map_or("  ".to_string(), |b| format!("{b:02x}"))
                })
                .collect();
            let ascii: String = chunk
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        char::from(b)
                    } else {
                        '.'
                    }
                })
                .collect();
            format!(
                "{:08x}  {}  {}  |{ascii}|\n",
                row * 16,
                cells[..8].join(" "),
                cells[8..].join(" ")
            )
        })
        .collect()
}

fn section_hex() {
    assert_eq!(hex(&[0, 15, 255]), "000fff");
    assert_eq!(unhex("000FfF"), Some(vec![0, 15, 255]));
    assert_eq!((unhex("abc"), unhex("zz"), unhex("+1")), (None, None, None));
    assert_eq!(unhex("é!"), None); // non-ASCII input is rejected, not sliced mid-character
    let d = hexdump(b"Hello, world!\n\x00\xffabc");
    let lines: Vec<&str> = d.lines().collect();
    assert_eq!(
        lines[0],
        "00000000  48 65 6c 6c 6f 2c 20 77  6f 72 6c 64 21 0a 00 ff  |Hello, world!...|"
    );
    assert!(lines[1].starts_with("00000010  61 62 63 ") && lines[1].ends_with("|abc|"));
    assert_eq!(lines[1].find('|'), lines[0].find('|')); // short rows stay aligned
    println!("5. hex ok");
}

// ---- 6. Money without floats -----------------------------------------------------------------------------------
/// Cents. Parsing and printing never touch f64.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Money(i64);
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let a = self.0.unsigned_abs();
        let sign = if self.0 < 0 { "-" } else { "" };
        // The sign is printed separately: -5 / 100 is 0, which would lose it.
        f.pad(&format!("{sign}{}.{:02}", a / 100, a % 100))
    }
}
impl FromStr for Money {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (neg, body) = match s.strip_prefix('-') {
            Some(b) => (true, b),
            None => (false, s),
        };
        let (whole, frac) = body.split_once('.').unwrap_or((body, "00"));
        let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
        if !digits(whole) || !digits(frac) || frac.len() != 2 {
            return Err("expected [-]digits[.dd]");
        }
        let whole: i64 = whole.parse().map_err(|_| "out of range")?;
        let frac: i64 = frac.parse().map_err(|_| "out of range")?;
        // Accumulate on the negative side, which has one more value than the positive side.
        let neg_total = whole
            .checked_mul(-100)
            .and_then(|w| w.checked_sub(frac))
            .ok_or("out of range")?;
        let cents = if neg {
            neg_total
        } else {
            neg_total.checked_mul(-1).ok_or("out of range")?
        };
        Ok(Money(cents))
    }
}

fn section_money() {
    assert_eq!(
        [Money(0), Money(5), Money(-5), Money(123456)].map(|m| m.to_string()),
        ["0.00", "0.05", "-0.05", "1234.56"]
    );
    assert_eq!(format!("[{:>9}]", Money(-1999)), "[   -19.99]");
    assert_eq!("12".parse(), Ok(Money(1200)));
    assert_eq!("-0.05".parse(), Ok(Money(-5)));
    for bad in [
        "",
        "-",
        ".50",
        "1.",
        "1.5",
        "1.505",
        "+1.00",
        "--1",
        "1e3",
        " 1.00",
        "999999999999999999.00",
    ] {
        assert!(bad.parse::<Money>().is_err(), "{bad:?}");
    }
    let mut rng = XorShift(7);
    let edge = [i64::MIN, i64::MAX, i64::MIN + 1, -100, -1, 0, 99, 100];
    for m in edge
        .into_iter()
        .chain((0..5000).map(|_| (rng.next() as i64) >> (rng.next() % 60)))
    {
        assert_eq!(Money(m).to_string().parse(), Ok(Money(m)), "{m}");
    }
    println!("6. money ok (5008 round trips)");
}

// ---- 7. Rounding for display and float round trips -------------------------------------------------------------
fn section_rounding() {
    // `{:.N}` rounds the exact binary value; round() rounds half AWAY from zero. They differ on exact ties.
    assert_eq!(
        (format!("{:.0}", 2.5), 2.5f64.round()),
        ("2".to_string(), 3.0)
    );
    assert_eq!(
        (format!("{:.0}", -2.5), (-2.5f64).round()),
        ("-2".to_string(), -3.0)
    );
    assert_eq!(format!("{:.2}", 1.005), "1.00"); // the stored value is slightly below 1.005
    assert_eq!(format!("{:.1}", -0.01), "-0.0"); // a negative zero can appear in output
    assert_eq!(format!("{:e} {:.2e}", 0.000123, 123456.0), "1.23e-4 1.23e5");
    // Percent: guard the zero denominator instead of printing NaN.
    let percent = |a: u64, b: u64| (b != 0).then(|| format!("{:.1}%", a as f64 * 100.0 / b as f64));
    assert_eq!((percent(1, 3), percent(1, 0)), (Some("33.3%".into()), None));

    // Display prints enough digits to read back the same bits (checked here on random bit patterns).
    let mut rng = XorShift(0xABCD);
    let (mut finite, mut ints) = (0, 0);
    for _ in 0..5000 {
        let x = f64::from_bits(rng.next());
        if x.is_finite() {
            finite += 1;
            assert_eq!(x.to_string().parse::<f64>().unwrap().to_bits(), x.to_bits());
            assert_eq!(
                format!("{x:e}").parse::<f64>().unwrap().to_bits(),
                x.to_bits()
            );
        }
        let y = f32::from_bits(rng.next() as u32);
        if y.is_finite() {
            assert_eq!(y.to_string().parse::<f32>().unwrap().to_bits(), y.to_bits());
        }
        let i = rng.next() as i64;
        assert_eq!(i.to_string().parse(), Ok(i));
        assert_eq!(
            i64::from_str_radix(&format!("{:x}", i.unsigned_abs()), 16).ok(),
            i64::try_from(i.unsigned_abs()).ok()
        );
        ints += 1;
    }
    assert!(finite > 4900 && ints == 5000);
    println!("7. rounding and round trips ok");
}

// ---- 8. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Parse at the edge into a type that cannot be invalid (NonZeroU32, Money, Timeout); pass types inward.
    // 2. Error messages need the field name and the offending input: std's messages have neither.
    let err = parse_ratio("abc").unwrap_err();
    assert!(err.contains("ratio") && err.contains("abc"));
    // 3. Always range-check after parsing: "fits in u64" is not "is a sane batch size".
    assert!(parse_limit("1000000").is_err());
    // 4. Trim deliberately. Values from files and env vars often end in "\n".
    assert!("8080\n".parse::<u16>().is_err() && "8080\n".trim().parse::<u16>().is_ok());
    // 5. Do not `unwrap_or(default)` a parse of a PRESENT value: a typo silently becomes the default.
    // 6. Reject non-finite floats from outside: one NaN poisons sums, min/max and sort order.
    assert!((1.0 + f64::NAN).is_nan());
    // 7. Large integers in JSON: values above 2^53 lose precision in f64-based readers; send IDs as strings.
    assert_eq!((((1u64 << 53) + 1) as f64) as u64, 1u64 << 53);
    // 8. Machine formats must not be localised or grouped; group only in human output.
    // 9. Prefer from_str_radix over hand-written digit loops, but validate sign characters (section 3).
    println!("8. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_ints();
    section_floats();
    section_from_str();
    section_human();
    section_hex();
    section_money();
    section_rounding();
    section_production_tips();
    println!(
        "covered {} parsing/formatting names; all asserts passed",
        COVERED.len()
    );
}
