/// Every item as an `i32`, or the error for the first bad one.
pub fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
    todo!()
}

/// The sum of every item as an `i64`, or the error for the first bad one. No intermediate `Vec`.
pub fn sum_all(items: &[&str]) -> Result<i64, String> {
    todo!()
}

/// Every item, or every error in order.
pub fn parse_every_error(items: &[&str]) -> Result<Vec<i32>, Vec<String>> {
    todo!()
}

/// Runs `check` on each item in order and stops at the first error.
pub fn validate(
    items: &[&str],
    mut check: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    todo!()
}
