//! What a LIKE pattern says about the *range* of strings it can match.

#[derive(Debug, PartialEq, Eq)]
pub enum LikeRange {
    /// The pattern starts with a wildcard: no usable prefix.
    Everything,
    /// No wildcard at all: only this string matches.
    Exact(String),
    /// Every match starts with `prefix`; all of them are `< upper` (if there is one). `recheck`: the matches of the range still have to
    /// be tested against the pattern.
    Range { prefix: String, upper: Option<String>, recheck: bool },
}

/// The smallest string greater than every string that starts with `prefix`, if there is one.
pub fn successor(prefix: &str) -> Option<String> {
    todo!("3i-c4: increase the last character; if it cannot be increased, drop it and increase the previous one; None when nothing is left")
}

/// What `pattern` says about the strings it matches: the literal prefix before the first wildcard (escapes resolved), then `Everything` for no
/// prefix, `Exact` for no wildcard at all, or a `Range` of strings that start with the prefix.
pub fn prefix_range(pattern: &str) -> LikeRange {
    todo!("3i-c4: the literal prefix (escapes resolved), then Everything / Exact / Range")
}
