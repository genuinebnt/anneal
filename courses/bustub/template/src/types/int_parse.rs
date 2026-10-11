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
    todo!("3a-c3: trim, sign, digits, checked accumulation")
}
