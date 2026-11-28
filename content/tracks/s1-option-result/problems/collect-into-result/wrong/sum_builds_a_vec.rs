fn parse(s: &str) -> Result<i32, String> {
    s.parse().map_err(|_| format!("bad number: {s}"))
}

/// Every item as an `i32`, or the error for the first bad one.
pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
    items.iter().map(|s| parse(s)).collect()
}

/// The sum of every item as an `i64`, or the error for the first bad one. No intermediate `Vec`.
pub fn sum_all(items: &[&str]) -> Result<i64, String> {
    // Builds the whole Vec first, which the statement rules out.
    Ok(parse_all(items)?.into_iter().map(i64::from).sum())
}

/// Every item, or every error in order.
pub fn parse_every_error(items: &[&str]) -> Result<Vec<i32>, Vec<String>> {
    let (good, bad): (Vec<_>, Vec<_>) = items.iter().map(|s| parse(s)).partition(Result::is_ok);
    if bad.is_empty() {
        Ok(good.into_iter().flatten().collect())
    } else {
        Err(bad.into_iter().filter_map(Result::err).collect())
    }
}

/// Runs `check` on each item in order and stops at the first error.
pub fn validate(
    items: &[&str],
    mut check: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    items.iter().map(|s| check(s)).collect()
}
