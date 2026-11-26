//! Tokens and comments produced by the lexer.

use crate::source::Span;

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Ident,
    Int(i64),
    Str(String),
    // keywords
    Fn,
    Struct,
    Enum,
    Const,
    Let,
    Mut,
    If,
    Else,
    Match,
    While,
    Loop,
    Break,
    Continue,
    Return,
    As,
    True,
    False,
    // punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Dot,
    DotDot,
    Arrow,
    FatArrow,
    Eq,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AndAnd,
    OrOr,
    Bang,
    Pipe,
    Underscore,
    /// A lexical error (already reported); the parser skips these silently.
    Error,
    Eof,
}

impl TokenKind {
    /// Source spelling of fixed tokens (keywords and punctuation).
    pub fn symbol(&self) -> Option<&'static str> {
        use TokenKind::*;
        Some(match self {
            Fn => "fn",
            Struct => "struct",
            Enum => "enum",
            Const => "const",
            Let => "let",
            Mut => "mut",
            If => "if",
            Else => "else",
            Match => "match",
            While => "while",
            Loop => "loop",
            Break => "break",
            Continue => "continue",
            Return => "return",
            As => "as",
            True => "true",
            False => "false",
            LParen => "(",
            RParen => ")",
            LBrace => "{",
            RBrace => "}",
            LBracket => "[",
            RBracket => "]",
            Comma => ",",
            Semi => ";",
            Colon => ":",
            ColonColon => "::",
            Dot => ".",
            DotDot => "..",
            Arrow => "->",
            FatArrow => "=>",
            Eq => "=",
            EqEq => "==",
            NotEq => "!=",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
            Plus => "+",
            Minus => "-",
            Star => "*",
            Slash => "/",
            Percent => "%",
            PlusEq => "+=",
            MinusEq => "-=",
            StarEq => "*=",
            SlashEq => "/=",
            PercentEq => "%=",
            AndAnd => "&&",
            OrOr => "||",
            Bang => "!",
            Pipe => "|",
            Underscore => "_",
            Ident | Int(_) | Str(_) | Error | Eof => return None,
        })
    }

    pub fn is_keyword(&self) -> bool {
        use TokenKind::*;
        matches!(
            self,
            Fn | Struct
                | Enum
                | Const
                | Let
                | Mut
                | If
                | Else
                | Match
                | While
                | Loop
                | Break
                | Continue
                | Return
                | As
                | True
                | False
        )
    }

    /// Human description used in "expected X, found Y" messages.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Ident => "identifier".into(),
            TokenKind::Int(_) => "integer literal".into(),
            TokenKind::Str(_) => "string literal".into(),
            TokenKind::Eof => "end of file".into(),
            TokenKind::Error => "invalid token".into(),
            k if k.is_keyword() => format!("keyword `{}`", k.symbol().unwrap_or("?")),
            k => format!("`{}`", k.symbol().unwrap_or("?")),
        }
    }
}

pub fn keyword(s: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match s {
        "fn" => Fn,
        "struct" => Struct,
        "enum" => Enum,
        "const" => Const,
        "let" => Let,
        "mut" => Mut,
        "if" => If,
        "else" => Else,
        "match" => Match,
        "while" => While,
        "loop" => Loop,
        "break" => Break,
        "continue" => Continue,
        "return" => Return,
        "as" => As,
        "true" => True,
        "false" => False,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentKind {
    /// `// ...`
    Line,
    /// `/// ...`
    DocLine,
    /// `/* ... */` (may nest)
    Block,
}

/// A comment kept in a side table (not in the token stream) so a formatter can
/// re-attach it by position. `text` is the raw source including delimiters.
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub kind: CommentKind,
    pub span: Span,
    pub text: String,
}
