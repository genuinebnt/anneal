use std::num::ParseIntError;

/// No input is `Ok(None)`; a number is `Ok(Some(n))`; anything else is an error.
pub fn parse_optional(s: Option<&str>) -> Result<Option<i32>, ParseIntError> {
    s.map(str::parse).transpose()
}

/// After trimming: a blank line or a `#` comment is `Ok(None)`; anything else must be a number.
pub fn parse_line(line: &str) -> Result<Option<i64>, ParseIntError> {
    let t = line.trim();
    (!t.is_empty() && !t.starts_with('#'))
        .then(|| t.parse())
        .transpose()
}

/// Every number in `text`, in order, or the 1-based line number and error of the first bad line.
pub fn parse_file(text: &str) -> Result<Vec<i64>, (usize, ParseIntError)> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| parse_line(line).map_err(|e| (i + 1, e)).transpose())
        .collect()
}
