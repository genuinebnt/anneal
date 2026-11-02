#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A `(` without its `)`, or a `)` without its `(`.
    UnbalancedParen,
    /// A `*`, `+` or `?` with nothing before it (at the start, after `(` or `|`, or directly after another quantifier).
    NothingToRepeat,
    /// A `\` at the very end of the pattern.
    DanglingEscape,
}

/// A compiled regular expression over `char`s: literals, `.`, `*`, `+`, `?`, `|`, `(` `)` and `\` to escape a metacharacter.
pub struct Regex {
    // your compiled program
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, ParseError> {
        todo!()
    }

    /// Does the pattern match the **whole** text?
    pub fn is_match(&self, text: &str) -> bool {
        todo!()
    }

    /// The leftmost match, and the longest one starting there, as `(start, end)` in `char` indices (`end` exclusive).
    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        todo!()
    }
}
