//! The lexer. It never fails: problems become diagnostics plus
//! [`TokenKind::Error`] tokens, and lexing always continues after them.

use super::token::{Comment, CommentKind, Token, TokenKind, keyword};
use crate::diag::Diagnostic;
use crate::source::Span;

/// Tokenise `src`. The returned tokens always end with one `Eof` token.
pub fn lex(src: &str) -> (Vec<Token>, Vec<Comment>, Vec<Diagnostic>) {
    let mut lx = Lexer {
        src,
        pos: 0,
        toks: Vec::new(),
        comments: Vec::new(),
        diags: Vec::new(),
    };
    lx.run();
    (lx.toks, lx.comments, lx.diags)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    toks: Vec<Token>,
    comments: Vec<Comment>,
    diags: Vec<Diagnostic>,
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}
fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }
    fn peek2(&self) -> Option<char> {
        self.src[self.pos..].chars().nth(1)
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn push(&mut self, kind: TokenKind, start: usize) {
        self.toks.push(Token {
            kind,
            span: Span::new(start, self.pos),
        });
    }

    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.pos;
            match c {
                c if c.is_whitespace() => {
                    self.bump();
                }
                '/' if self.peek2() == Some('/') => self.line_comment(),
                '/' if self.peek2() == Some('*') => self.block_comment(),
                '"' => self.string(),
                c if c.is_ascii_digit() => self.number(),
                c if is_ident_start(c) => self.ident(),
                _ => {
                    self.bump();
                    match self.punct(c) {
                        Some(k) => self.push(k, start),
                        None => {
                            self.diags.push(
                                Diagnostic::error(
                                    "E0001",
                                    format!("unknown start of token: `{}`", c.escape_debug()),
                                )
                                .primary(
                                    Span::new(start, self.pos),
                                    "this character is not part of the language",
                                ),
                            );
                            self.push(TokenKind::Error, start);
                        }
                    }
                }
            }
        }
        let end = self.src.len();
        self.toks.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(end, end),
        });
    }

    /// Longest-match punctuation; `c` has already been consumed.
    fn punct(&mut self, c: char) -> Option<TokenKind> {
        use TokenKind::*;
        let two = |this: &mut Self, next: char, yes: TokenKind, no: Option<TokenKind>| {
            if this.peek() == Some(next) {
                this.bump();
                Some(yes)
            } else {
                no
            }
        };
        match c {
            '(' => Some(LParen),
            ')' => Some(RParen),
            '{' => Some(LBrace),
            '}' => Some(RBrace),
            '[' => Some(LBracket),
            ']' => Some(RBracket),
            ',' => Some(Comma),
            ';' => Some(Semi),
            ':' => two(self, ':', ColonColon, Some(Colon)),
            '.' => two(self, '.', DotDot, Some(Dot)),
            '-' => {
                if self.peek() == Some('>') {
                    self.bump();
                    Some(Arrow)
                } else {
                    two(self, '=', MinusEq, Some(Minus))
                }
            }
            '=' => {
                if self.peek() == Some('>') {
                    self.bump();
                    Some(FatArrow)
                } else {
                    two(self, '=', EqEq, Some(Eq))
                }
            }
            '!' => two(self, '=', NotEq, Some(Bang)),
            '<' => two(self, '=', Le, Some(Lt)),
            '>' => two(self, '=', Ge, Some(Gt)),
            '+' => two(self, '=', PlusEq, Some(Plus)),
            '*' => two(self, '=', StarEq, Some(Star)),
            '/' => two(self, '=', SlashEq, Some(Slash)),
            '%' => two(self, '=', PercentEq, Some(Percent)),
            '&' => two(self, '&', AndAnd, None),
            '|' => two(self, '|', OrOr, Some(Pipe)),
            _ => None,
        }
    }

    fn line_comment(&mut self) {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
        let text = &self.src[start..self.pos];
        let kind = if text.starts_with("///") && !text.starts_with("////") {
            CommentKind::DocLine
        } else {
            CommentKind::Line
        };
        self.comments.push(Comment {
            kind,
            span: Span::new(start, self.pos),
            text: text.to_string(),
        });
    }

    fn block_comment(&mut self) {
        let start = self.pos;
        self.pos += 2;
        let mut depth = 1;
        while depth > 0 {
            match (self.peek(), self.peek2()) {
                (Some('/'), Some('*')) => {
                    self.pos += 2;
                    depth += 1;
                }
                (Some('*'), Some('/')) => {
                    self.pos += 2;
                    depth -= 1;
                }
                (Some(_), _) => {
                    self.bump();
                }
                (None, _) => break,
            }
        }
        if depth > 0 {
            self.diags.push(
                Diagnostic::error("E0003", "unterminated block comment")
                    .primary(Span::new(start, start + 2), "comment starts here")
                    .help("block comments nest: every `/*` needs a matching `*/`"),
            );
        }
        let text = self.src[start..self.pos].to_string();
        self.comments.push(Comment {
            kind: CommentKind::Block,
            span: Span::new(start, self.pos),
            text,
        });
    }

    fn ident(&mut self) {
        let start = self.pos;
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        let text = &self.src[start..self.pos];
        let kind = if text == "_" {
            TokenKind::Underscore
        } else {
            keyword(text).unwrap_or(TokenKind::Ident)
        };
        self.push(kind, start);
    }

    fn number(&mut self) {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '_') {
            self.bump();
        }
        if self.peek().is_some_and(is_ident_start) {
            while self.peek().is_some_and(is_ident_continue) {
                self.bump();
            }
            let text = &self.src[start..self.pos];
            self.diags.push(
                Diagnostic::error("E0006", format!("malformed number literal `{text}`"))
                    .primary(Span::new(start, self.pos), "letters cannot follow digits")
                    .help("integer literals contain only digits and `_` separators"),
            );
            self.push(TokenKind::Error, start);
            return;
        }
        let text: String = self.src[start..self.pos]
            .chars()
            .filter(|&c| c != '_')
            .collect();
        match text.parse::<i64>() {
            Ok(v) => self.push(TokenKind::Int(v), start),
            Err(_) => {
                self.diags.push(
                    Diagnostic::error("E0005", "integer literal is too large")
                        .primary(Span::new(start, self.pos), "does not fit in a 64-bit `int`")
                        .note(format!("the largest `int` is {}", i64::MAX)),
                );
                self.push(TokenKind::Error, start);
            }
        }
    }

    fn string(&mut self) {
        let start = self.pos;
        self.bump();
        let mut value = String::new();
        loop {
            match self.peek() {
                Some('"') => {
                    self.bump();
                    self.push(TokenKind::Str(value), start);
                    return;
                }
                None | Some('\n') => {
                    self.diags.push(
                        Diagnostic::error("E0002", "unterminated string literal")
                            .primary(Span::new(start, start + 1), "string starts here")
                            .help("add a closing `\"`; strings cannot span lines (use `\\n`)"),
                    );
                    self.push(TokenKind::Error, start);
                    return;
                }
                Some('\\') => self.escape(&mut value),
                Some(c) => {
                    self.bump();
                    value.push(c);
                }
            }
        }
    }

    /// Parse one escape; on error keep going so the rest of the string still lexes.
    fn escape(&mut self, value: &mut String) {
        let start = self.pos;
        self.bump();
        let bad = |this: &mut Self, why: String| {
            this.diags.push(
                Diagnostic::error("E0004", why)
                    .primary(Span::new(start, this.pos), "invalid escape")
                    .help("valid escapes: \\n \\t \\r \\0 \\\\ \\\" and \\u{hex}"),
            );
        };
        match self.peek() {
            Some('n') => {
                self.bump();
                value.push('\n')
            }
            Some('t') => {
                self.bump();
                value.push('\t')
            }
            Some('r') => {
                self.bump();
                value.push('\r')
            }
            Some('0') => {
                self.bump();
                value.push('\0')
            }
            Some('\\') => {
                self.bump();
                value.push('\\')
            }
            Some('"') => {
                self.bump();
                value.push('"')
            }
            Some('u') => {
                self.bump();
                if self.peek() != Some('{') {
                    return bad(self, "incorrect unicode escape sequence".into());
                }
                self.bump();
                let hex_start = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    self.bump();
                }
                let hex = &self.src[hex_start..self.pos];
                let closed = self.peek() == Some('}');
                if closed {
                    self.bump();
                }
                match u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                    Some(c) if closed => value.push(c),
                    _ => bad(self, "invalid unicode escape".into()),
                }
            }
            Some('\n') | None => bad(self, "unfinished escape sequence".into()),
            Some(c) => {
                self.bump();
                bad(self, format!("unknown escape sequence `\\{c}`"));
            }
        }
    }
}
