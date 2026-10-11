//! SQL `LIKE`.

#[derive(Debug, PartialEq, Eq)]
pub enum LikeError {
    TrailingEscape,
}

/// Whether `text` matches `pattern`. `escape`, when present, makes the following pattern character literal.
pub fn like(text: &str, pattern: &str, escape: Option<char>) -> Result<bool, LikeError> {
    todo!("3a-c5: tokenise the pattern, then match with one backtrack point for the latest %")
}
