#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    BadCharacter,
    UnclosedText,
    NumberTooLarge,
}

/// What is wrong, and the bytes `start..end` of the source it is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxError {
    pub problem: Problem,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A word, in small letters. Keywords are words too.
    Word(String),
    Number(i64),
    /// The text between the quotes, with each doubled quote made single.
    Text(String),
    /// One of the symbols, as written, except that `!=` is given as `<>`.
    Symbol(&'static str),
}

/// A token, and the bytes `start..end` of the source it covers.
pub type Lexed = (Token, usize, usize);

/// Every token of the text, or the first bad one.
pub fn lex(sql: &str) -> Result<Vec<Lexed>, SyntaxError> {
    let _ = sql;
    todo!()
}
