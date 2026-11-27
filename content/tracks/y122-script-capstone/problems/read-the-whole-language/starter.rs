use std::cell::Cell;
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
        .chars()
        .count()
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

// ---------------------------------------------------------------- the tree

/// Only this many diagnostics are reported: each one costs a pass over the source to find its line and column.
pub const MAX_DIAGS: usize = 100;
/// How deep the parser lets the tree get. Every later pass walks the tree by recursion.
pub const MAX_NESTING: usize = 200;

/// A name in the source. From step 3 on, `depth` says where a variable lives: `Some(n)` is `n` scopes out from where it
/// is used, `None` is a global.
#[derive(Debug)]
pub struct Name {
    pub name: Rc<str>,
    pub span: Span,
    pub depth: Cell<Option<usize>>,
}

impl Name {
    pub fn new(name: Rc<str>, span: Span) -> Name {
        Name {
            name,
            span,
            depth: Cell::new(None),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug)]
pub enum Expr {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Var(Name),
    Assign(Name, Box<Expr>),
    /// The span is the operator's.
    Unary(UnOp, Box<Expr>, Span),
    /// The span is the operator's.
    Binary(Box<Expr>, BinOp, Box<Expr>, Span),
    /// The flag is true for `and`, false for `or`.
    Logical(Box<Expr>, bool, Box<Expr>),
    /// The span is the closing bracket's: its line is the line of the call.
    Call(Box<Expr>, Vec<Expr>, Span),
    /// `object.name`. The span is the name's.
    Get(Box<Expr>, Rc<str>, Span),
    /// `object.name = value`. The span is the name's.
    Set(Box<Expr>, Rc<str>, Box<Expr>, Span),
    /// Its name is `this`.
    This(Name),
    /// `super.method`. The name is `super`; the span is the method's.
    Super(Name, Rc<str>, Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FnKind {
    Function,
    Method,
    /// A method called `init`.
    Initializer,
}

#[derive(Debug)]
pub struct FunDecl {
    pub name: Name,
    pub params: Vec<Name>,
    pub body: Vec<Stmt>,
    pub kind: FnKind,
}

#[derive(Debug)]
pub struct ClassDecl {
    pub name: Name,
    pub superclass: Option<Name>,
    pub methods: Vec<Rc<FunDecl>>,
}

#[derive(Debug)]
pub enum Stmt {
    Expr(Expr),
    Var(Name, Option<Expr>),
    Block(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    /// Condition, body, and the step of a `for` loop: it runs after the body, and after a `continue`.
    While(Expr, Box<Stmt>, Option<Expr>),
    /// The span is the keyword's.
    Break(Span),
    Continue(Span),
    Return(Span, Option<Expr>),
    Fun(Rc<FunDecl>),
    Class(ClassDecl),
}

/// The tree as text, one top-level statement per line. A name with a depth is written `name@depth`.
pub fn dump(program: &[Stmt]) -> String {
    let lines: Vec<String> = program.iter().map(dump_stmt).collect();
    lines.join("\n")
}

fn dump_name(n: &Name) -> String {
    match n.depth.get() {
        Some(d) => format!("{}@{d}", n.name),
        None => n.name.to_string(),
    }
}

fn node(head: &str, parts: impl IntoIterator<Item = String>) -> String {
    let mut out = format!("({head}");
    for part in parts {
        out.push(' ');
        out.push_str(&part);
    }
    out.push(')');
    out
}

fn dump_fun(head: &str, f: &FunDecl) -> String {
    let params: Vec<String> = f.params.iter().map(dump_name).collect();
    let mut parts = vec![dump_name(&f.name), format!("({})", params.join(" "))];
    parts.extend(f.body.iter().map(dump_stmt));
    node(head, parts)
}

fn dump_stmt(s: &Stmt) -> String {
    match s {
        Stmt::Expr(e) => node("expr", [dump_expr(e)]),
        Stmt::Var(n, init) => node(
            "var",
            std::iter::once(dump_name(n)).chain(init.iter().map(dump_expr)),
        ),
        Stmt::Block(list) => node("block", list.iter().map(dump_stmt)),
        Stmt::If(c, then, other) => node(
            "if",
            [dump_expr(c), dump_stmt(then)]
                .into_iter()
                .chain(other.iter().map(|s| dump_stmt(s))),
        ),
        Stmt::While(c, body, step) => node(
            "while",
            [dump_expr(c), dump_stmt(body)]
                .into_iter()
                .chain(step.iter().map(|e| node("step", [dump_expr(e)]))),
        ),
        Stmt::Break(_) => "(break)".to_string(),
        Stmt::Continue(_) => "(continue)".to_string(),
        Stmt::Return(_, value) => node("return", value.iter().map(dump_expr)),
        Stmt::Fun(f) => dump_fun("fun", f),
        Stmt::Class(c) => {
            let mut parts = vec![dump_name(&c.name)];
            parts.extend(c.superclass.iter().map(|n| node("<", [dump_name(n)])));
            parts.extend(c.methods.iter().map(|m| {
                let head = if m.kind == FnKind::Initializer {
                    "init-method"
                } else {
                    "method"
                };
                dump_fun(head, m)
            }));
            node("class", parts)
        }
    }
}

fn dump_expr(e: &Expr) -> String {
    match e {
        Expr::Nil => "nil".to_string(),
        Expr::Bool(b) => b.to_string(),
        Expr::Num(n) => n.to_string(),
        Expr::Str(s) => format!("{s:?}"),
        Expr::Var(n) | Expr::This(n) => dump_name(n),
        Expr::Assign(n, v) => node("=", [dump_name(n), dump_expr(v)]),
        Expr::Unary(op, x, _) => {
            let op = if *op == UnOp::Neg { "-" } else { "!" };
            node(op, [dump_expr(x)])
        }
        Expr::Binary(l, op, r, _) => {
            let op = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Rem => "%",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
            };
            node(op, [dump_expr(l), dump_expr(r)])
        }
        Expr::Logical(l, is_and, r) => node(
            if *is_and { "and" } else { "or" },
            [dump_expr(l), dump_expr(r)],
        ),
        Expr::Call(callee, args, _) => node(
            "call",
            std::iter::once(dump_expr(callee)).chain(args.iter().map(dump_expr)),
        ),
        Expr::Get(o, name, _) => node(".", [dump_expr(o), name.to_string()]),
        Expr::Set(o, name, v, _) => node(".=", [dump_expr(o), name.to_string(), dump_expr(v)]),
        Expr::Super(n, method, _) => node("super", [dump_name(n), method.to_string()]),
    }
}

// ---------------------------------------------------------------- the parser

/// Builds the tree. `tokens` ends with `Eof`, as `scan` returns them. Statements that had a mistake are left out.
pub fn parse_tokens(tokens: Vec<Token>) -> (Vec<Stmt>, Vec<Diag>) {
    todo!()
}

/// Sorts diagnostics by position and writes the first `MAX_DIAGS` of them.
fn report(src: &str, mut diags: Vec<Diag>) -> Vec<String> {
    diags.sort_by_key(|d| d.span.start);
    diags.iter().take(MAX_DIAGS).map(|d| d.short(src)).collect()
}

/// Scanner and parser together: the tree, or what is wrong with the text.
pub fn parse(src: &str) -> Result<Vec<Stmt>, Vec<String>> {
    let (tokens, mut diags) = scan(src);
    let (program, more) = parse_tokens(tokens);
    diags.extend(more);
    if diags.is_empty() {
        Ok(program)
    } else {
        Err(report(src, diags))
    }
}
