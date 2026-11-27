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
        let (line, col) = line_col(src, self.span.start);
        let kind = if self.warn { "Warning" } else { "Error" };
        let at = match self.at {
            At::Nowhere => String::new(),
            At::End => " at end".to_string(),
            At::Token => format!(" at '{}'", &src[self.span.start..self.span.end]),
        };
        format!("[line {line}:{col}] {kind}{at}: {}", self.msg)
    }

    /// The one line, then the source line, then carets under the span.
    pub fn render(&self, src: &str) -> String {
        let (line, col) = line_col(src, self.span.start);
        let line_start = src[..self.span.start].rfind('\n').map_or(0, |i| i + 1);
        let rest = &src[line_start..];
        let text = rest.split('\n').next().unwrap_or("").trim_end_matches('\r');
        let first_line_end = (line_start + text.len()).max(self.span.start);
        let end = self.span.end.min(first_line_end);
        let width = src[self.span.start..end].chars().count().max(1);
        format!(
            "{}\n{line:>4} | {text}\n     | {}{}",
            self.short(src),
            " ".repeat(col - 1),
            "^".repeat(width)
        )
    }
}

/// Line and column of a byte offset, both from 1. The column counts characters.
pub fn line_col(src: &str, pos: usize) -> (usize, usize) {
    let before = &src[..pos.min(src.len())];
    let line = before.matches('\n').count() + 1;
    let col = before
        .rfind('\n')
        .map_or(before, |i| &before[i + 1..])
        .len()
        + 1;
    (line, col)
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

fn keyword(word: &str) -> Option<Kind> {
    Some(match word {
        "and" => Kind::And,
        "break" => Kind::Break,
        "class" => Kind::Class,
        "continue" => Kind::Continue,
        "else" => Kind::Else,
        "false" => Kind::False,
        "for" => Kind::For,
        "fun" => Kind::Fun,
        "if" => Kind::If,
        "nil" => Kind::Nil,
        "or" => Kind::Or,
        "return" => Kind::Return,
        "super" => Kind::Super,
        "this" => Kind::This,
        "true" => Kind::True,
        "var" => Kind::Var,
        "while" => Kind::While,
        _ => return None,
    })
}

/// Cuts the source into tokens. The last token is always `Eof`. Mistakes become diagnostics and scanning goes on.
pub fn scan(src: &str) -> (Vec<Token>, Vec<Diag>) {
    let b = src.as_bytes();
    let (mut tokens, mut diags) = (Vec::new(), Vec::new());
    let (mut i, mut line) = (0usize, 1u32);
    while i < b.len() {
        let (start, start_line) = (i, line);
        let c = b[i];
        i += 1;
        let mut two = |next: u8, yes: Kind, no: Kind| {
            if b.get(i) == Some(&next) {
                i += 1;
                yes
            } else {
                no
            }
        };
        let kind = match c {
            b'(' => Kind::LParen,
            b')' => Kind::RParen,
            b'{' => Kind::LBrace,
            b'}' => Kind::RBrace,
            b',' => Kind::Comma,
            b'.' => Kind::Dot,
            b'-' => Kind::Minus,
            b'+' => Kind::Plus,
            b';' => Kind::Semi,
            b'*' => Kind::Star,
            b'%' => Kind::Percent,
            b'!' => two(b'=', Kind::BangEq, Kind::Bang),
            b'=' => two(b'=', Kind::EqEq, Kind::Eq),
            b'<' => two(b'=', Kind::Le, Kind::Lt),
            b'>' => two(b'=', Kind::Ge, Kind::Gt),
            b'\n' => {
                line += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' => continue,
            b'/' if b.get(i) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i) == Some(&b'*') => {
                i += 1;
                let mut depth = 1;
                while i < b.len() && depth > 0 {
                    if b[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if b[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        line += u32::from(b[i] == b'\n');
                        i += 1;
                    }
                }
                if depth > 0 {
                    let span = Span {
                        start,
                        end: start + 2,
                        line: start_line,
                    };
                    diags.push(Diag::error(
                        span,
                        At::Nowhere,
                        "Unterminated block comment.",
                    ));
                }
                continue;
            }
            b'/' => Kind::Slash,
            b'"' => {
                let mut text = String::new();
                let mut closed = false;
                while let Some(ch) = src[i..].chars().next() {
                    i += ch.len_utf8();
                    match ch {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' => {
                            let Some(escaped) = src[i..].chars().next() else {
                                break;
                            };
                            let at = i - 1;
                            i += escaped.len_utf8();
                            match escaped {
                                'n' => text.push('\n'),
                                't' => text.push('\t'),
                                '\\' => text.push('\\'),
                                '"' => text.push('"'),
                                other => {
                                    let span = Span {
                                        start: at,
                                        end: i,
                                        line,
                                    };
                                    diags.push(Diag::error(
                                        span,
                                        At::Nowhere,
                                        "Invalid escape sequence.",
                                    ));
                                    line += u32::from(other == '\n');
                                }
                            }
                        }
                        other => {
                            line += u32::from(other == '\n');
                            text.push(other);
                        }
                    }
                }
                if !closed {
                    let span = Span {
                        start,
                        end: start + 1,
                        line: start_line,
                    };
                    diags.push(Diag::error(span, At::Nowhere, "Unterminated string."));
                    continue;
                }
                Kind::Str(text.into())
            }
            b'0'..=b'9' => {
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                Kind::Num(src[start..i].parse().unwrap_or(0.0))
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &src[start..i];
                keyword(word).unwrap_or_else(|| Kind::Ident(word.into()))
            }
            _ => {
                let ch = src[start..].chars().next().unwrap_or('?');
                i = start + ch.len_utf8();
                let span = Span {
                    start,
                    end: i,
                    line: start_line,
                };
                let msg = format!("Unexpected character '{ch}'.");
                diags.push(Diag::error(span, At::Nowhere, &msg));
                continue;
            }
        };
        let span = Span {
            start,
            end: i,
            line: start_line,
        };
        tokens.push(Token { kind, span });
    }
    let span = Span {
        start: b.len(),
        end: b.len(),
        line,
    };
    tokens.push(Token {
        kind: Kind::Eof,
        span,
    });
    (tokens, diags)
}
