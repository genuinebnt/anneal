use std::rc::Rc;

// ---------------------------------------------------------------- positions and diagnostics

/// A piece of the source: byte offsets `start..end`, and the line it starts on (from 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
}

/// What a diagnostic adds after the word `Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    Nowhere,
    Token,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    pub span: Span,
    pub at: At,
    pub msg: String,
    pub warn: bool,
}

impl Diag {
    pub fn error(span: Span, at: At, msg: &str) -> Diag {
        Diag {
            span,
            at,
            msg: msg.to_string(),
            warn: false,
        }
    }

    /// One line, such as `[line 2:7] Error at ';': Expect expression.`
    pub fn short(&self, src: &str) -> String {
        todo!()
    }

    /// The one line, then the source line, then carets under the span.
    pub fn render(&self, src: &str) -> String {
        todo!()
    }
}

/// Line and column of a byte offset, both from 1. The column counts characters.
pub fn line_col(src: &str, pos: usize) -> (usize, usize) {
    todo!()
}

// ---------------------------------------------------------------- tokens

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Dot,
    Minus,
    Plus,
    Semi,
    Slash,
    Star,
    Percent,
    Bang,
    BangEq,
    Eq,
    EqEq,
    Gt,
    Ge,
    Lt,
    Le,
    Ident(Rc<str>),
    /// The text between the quotes, with escapes already replaced.
    Str(Rc<str>),
    Num(f64),
    And,
    Break,
    Class,
    Continue,
    Else,
    False,
    For,
    Fun,
    If,
    Nil,
    Or,
    Return,
    Super,
    This,
    True,
    Var,
    While,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: Kind,
    pub span: Span,
}

/// Cuts the source into tokens. The last token is always `Eof`. Mistakes become diagnostics and scanning goes on.
pub fn scan(src: &str) -> (Vec<Token>, Vec<Diag>) {
    todo!()
}
