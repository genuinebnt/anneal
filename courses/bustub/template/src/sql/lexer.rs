//! Splits SQL text into tokens. Unquoted words are lower-cased (PostgreSQL folds identifiers and keywords to lower case); `--` and
//! `/* */` comments are skipped.

use crate::common::exception::{Exception, ExceptionType, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// An unquoted word: a keyword or an identifier, lower-cased.
    Word(String),
    /// A `"quoted identifier"`, exactly as written.
    Quoted(String),
    /// A number as written (`42`, `1.5`, `1e3`).
    Number(String),
    /// A `'string'` with `''` unescaped.
    Str(String),
    /// An operator or punctuation: `+ - * / % = < > <= >= <> != || , ( ) ; . ::`.
    Symbol(&'static str),
}

pub fn parse_error(msg: impl std::fmt::Display) -> Exception {
    Exception::new(ExceptionType::Invalid, format!("Query failed to parse: {msg}"))
}

/// Splits `sql` into tokens. Whitespace and comments are skipped, words are lower-cased, `'strings'` use `''` for a quote, and a two-character
/// symbol wins over its first character. Text that is no token (an unterminated string or quoted identifier, an unknown character) is an error.
pub fn tokenize(sql: &str) -> Result<Vec<Token>> {
    todo!("3d-04: walk the characters: skip whitespace and comments; words (lower-cased), quoted names, numbers, 'strings' with '' for a quote, two-character symbols before one-character ones; anything else is a parse error")
}
