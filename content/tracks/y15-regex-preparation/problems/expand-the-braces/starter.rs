pub const MAX_WORDS: usize = 10_000;
pub const MAX_OPEN: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceError {
    UnmatchedClose(usize),
    Unclosed(usize),
    TrailingEscape(usize),
    TooDeep(usize),
    TooManyWords,
}

/// Every word the pattern stands for, in order.
pub fn expand(pattern: &str) -> Result<Vec<String>, BraceError> {
    todo!()
}
