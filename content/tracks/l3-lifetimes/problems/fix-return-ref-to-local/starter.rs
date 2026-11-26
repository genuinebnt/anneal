use std::borrow::Cow;

/// `text` lowercased, with its words joined by "-". When `text` is already in that form (no whitespace, no
/// char that lowercasing changes), the result borrows `text` instead of allocating.
pub fn slug(text: &str) -> &str {
    let s = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    &s
}

/// The non-blank lines of `text`, trimmed, in order.
pub fn clean_lines(text: &str) -> &[&str] {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    &lines
}

/// The file name (the part after the last '/') of the path with the longest file name; the first on a tie.
pub fn longest_file_name(paths: Vec<String>) -> Option<&str> {
    paths
        .iter()
        .map(|p| p.rsplit('/').next().unwrap())
        .max_by_key(|n| n.len())
}
