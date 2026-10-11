//! A small SQL lexer.

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Tok {
    Ident(String),
    QuotedIdent(String),
    Int(i64),
    Str(String),
    Sym(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum LexErrorKind {
    UnterminatedString,
    UnterminatedIdent,
    UnterminatedComment,
    BadChar,
    IntegerOverflow,
}

#[derive(Debug, PartialEq, Eq)]
pub struct LexError {
    pub pos: usize,
    pub kind: LexErrorKind,
}

/// Splits `sql` into tokens: strings and quoted identifiers use a doubled quote for a literal quote, and comments are skipped. An error carries
/// the byte position and the kind of the mistake.
pub fn tokenize(sql: &str) -> Result<Vec<Tok>, LexError> {
    todo!("3d-c1: scan the bytes; strings and quoted identifiers with doubled quotes; comments; numbers; identifiers; symbols")
}
