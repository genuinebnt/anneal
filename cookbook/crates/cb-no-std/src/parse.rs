//! Integer parsing with only `core` (core also has `str::parse`; this shows
//! the byte-level algorithm and avoids pulling `core::num` error types into APIs).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ParseError {
    Empty,
    InvalidDigit,
    Overflow,
}

pub fn parse_u64(s: &[u8]) -> Result<u64, ParseError> {
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut n: u64 = 0;
    for &b in s {
        if !b.is_ascii_digit() {
            return Err(ParseError::InvalidDigit);
        }
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u64))
            .ok_or(ParseError::Overflow)?;
    }
    Ok(n)
}

pub fn parse_i64(s: &[u8]) -> Result<i64, ParseError> {
    let (neg, digits) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mag = parse_u64(digits)?;
    if neg {
        // i64::MIN has magnitude 2^63, which does not fit in i64 positive.
        if mag == 1u64 << 63 {
            Ok(i64::MIN)
        } else {
            i64::try_from(mag)
                .map(|v| -v)
                .map_err(|_| ParseError::Overflow)
        }
    } else {
        i64::try_from(mag).map_err(|_| ParseError::Overflow)
    }
}
