//! Strict text to integer conversion.

#[derive(Debug, PartialEq, Eq)]
pub enum IntError {
    Empty,
    Invalid,
    OutOfRange,
}

/// Parses a decimal integer: optional surrounding whitespace, an optional sign, then digits. `Err` for empty text, any other character or a value
/// that does not fit an `i64`.
pub fn parse_sql_int(s: &str) -> Result<i64, IntError> {
    // @begin 3a-c3
    let t = s.trim_matches(|c: char| c.is_ascii_whitespace());
    if t.is_empty() {
        return Err(IntError::Empty);
    }
    let (neg, digits) = match t.as_bytes()[0] {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(IntError::Invalid);
    }
    let mut acc: i64 = 0;
    for b in digits.bytes() {
        let d = (b - b'0') as i64;
        acc = acc.checked_mul(10).and_then(|a| if neg { a.checked_sub(d) } else { a.checked_add(d) }).ok_or(IntError::OutOfRange)?;
    }
    Ok(acc)
    //~ todo!("3a-c3: trim, sign, digits, checked accumulation")
    // @end
}
