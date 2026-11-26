//! `"1d2h3m4s250ms"` <-> milliseconds. Parsing is lenient (any unit order, repeats, no
//! separators); formatting is canonical (largest unit first, no zero parts, `0ms` for zero),
//! so `format(parse(s))` is idempotent and `parse(format(d)) == d`.
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurationError {
    Empty,
    MissingNumber(usize),
    MissingUnit(usize),
    UnknownUnit(String),
    Overflow,
}

const UNITS: [(&str, u64); 5] = [
    ("d", 86_400_000),
    ("h", 3_600_000),
    ("m", 60_000),
    ("s", 1000),
    ("ms", 1),
];

pub fn parse_duration(s: &str) -> Result<Duration, DurationError> {
    if s.is_empty() {
        return Err(DurationError::Empty);
    }
    let b = s.as_bytes();
    let (mut i, mut total) = (0usize, 0u64);
    while i < b.len() {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if start == i {
            return Err(DurationError::MissingNumber(i));
        }
        let n: u64 = s[start..i].parse().map_err(|_| DurationError::Overflow)?;
        let ustart = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        if ustart == i {
            return Err(DurationError::MissingUnit(i));
        }
        let unit = &s[ustart..i];
        let mult = UNITS.iter().find(|(u, _)| *u == unit).map(|(_, m)| *m);
        let mult = mult.ok_or_else(|| DurationError::UnknownUnit(unit.to_string()))?;
        total = n
            .checked_mul(mult)
            .and_then(|v| total.checked_add(v))
            .ok_or(DurationError::Overflow)?;
    }
    Ok(Duration::from_millis(total))
}

/// Canonical text for a duration (sub-millisecond precision is truncated).
pub fn format_duration(d: Duration) -> String {
    let mut ms = u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
    if ms == 0 {
        return "0ms".into();
    }
    let mut out = String::new();
    for (name, mult) in UNITS {
        if ms >= mult {
            out.push_str(&format!("{}{name}", ms / mult));
            ms %= mult;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table() {
        for (s, ms) in [
            ("1s", 1000),
            ("1m30s", 90_000),
            ("90s", 90_000),
            ("2d", 172_800_000),
            ("5ms", 5),
        ] {
            assert_eq!(parse_duration(s), Ok(Duration::from_millis(ms)), "{s}");
        }
        assert_eq!(
            format_duration(Duration::from_millis(90_061_001)),
            "1d1h1m1s1ms"
        );
        assert_eq!(
            parse_duration("10x"),
            Err(DurationError::UnknownUnit("x".into()))
        );
    }
}
