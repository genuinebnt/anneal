#![allow(dead_code, unused_imports)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;
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

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    depth: usize,
    diags: Vec<Diag>,
}

/// `Err(Reported)`: the diagnostic is already recorded; unwind to the nearest declaration and carry on from there.
struct Reported;
type Parsed<T> = Result<T, Reported>;

enum Infix {
    Logical(bool),
    Binary(BinOp),
}

/// How tightly an infix operator binds, and what it builds. All of them group to the left.
fn infix(kind: &Kind) -> Option<(u8, Infix)> {
    use Infix::{Binary, Logical};
    Some(match kind {
        Kind::Or => (1, Logical(false)),
        Kind::And => (2, Logical(true)),
        Kind::EqEq => (3, Binary(BinOp::Eq)),
        Kind::BangEq => (3, Binary(BinOp::Ne)),
        Kind::Lt => (4, Binary(BinOp::Lt)),
        Kind::Le => (4, Binary(BinOp::Le)),
        Kind::Gt => (4, Binary(BinOp::Gt)),
        Kind::Ge => (4, Binary(BinOp::Ge)),
        Kind::Plus => (5, Binary(BinOp::Add)),
        Kind::Minus => (5, Binary(BinOp::Sub)),
        Kind::Star => (6, Binary(BinOp::Mul)),
        Kind::Slash => (6, Binary(BinOp::Div)),
        Kind::Percent => (6, Binary(BinOp::Rem)),
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn at_end(&self) -> bool {
        self.peek().kind == Kind::Eof
    }

    /// Takes the current token. `Eof` is never passed.
    fn advance(&mut self) -> Token {
        let token = self.tokens[self.current].clone();
        if !self.at_end() {
            self.current += 1;
        }
        token
    }

    fn is(&self, kind: &Kind) -> bool {
        self.peek().kind == *kind
    }

    fn eat(&mut self, kind: &Kind) -> bool {
        let hit = self.is(kind);
        if hit {
            self.advance();
        }
        hit
    }

    fn report(&mut self, token: &Token, msg: &str) {
        let at = if token.kind == Kind::Eof {
            At::End
        } else {
            At::Token
        };
        self.diags.push(Diag::error(token.span, at, msg));
    }

    fn fail<T>(&mut self, msg: &str) -> Parsed<T> {
        let token = self.peek().clone();
        self.report(&token, msg);
        Err(Reported)
    }

    fn expect(&mut self, kind: Kind, msg: &str) -> Parsed<Token> {
        if self.is(&kind) {
            Ok(self.advance())
        } else {
            self.fail(msg)
        }
    }

    fn name(&mut self, msg: &str) -> Parsed<Name> {
        match &self.peek().kind {
            Kind::Ident(s) => {
                let name = Name::new(s.clone(), self.peek().span);
                self.advance();
                Ok(name)
            }
            _ => self.fail(msg),
        }
    }

    fn deeper(&mut self) -> Parsed<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return self.fail("Too deeply nested.");
        }
        Ok(())
    }

    /// After a mistake: skip to a place where a new statement can start. Always takes at least one token.
    fn skip_to_statement(&mut self) {
        let mut last = self.advance();
        while !self.at_end() && last.kind != Kind::Semi {
            use Kind::*;
            if matches!(
                self.peek().kind,
                Class | Fun | Var | For | If | While | Return
            ) {
                return;
            }
            last = self.advance();
        }
    }

    fn declaration(&mut self) -> Option<Stmt> {
        let depth = self.depth;
        let result = if self.eat(&Kind::Class) {
            self.class_decl()
        } else if self.eat(&Kind::Fun) {
            self.function(false).map(|f| Stmt::Fun(Rc::new(f)))
        } else if self.eat(&Kind::Var) {
            self.var_decl()
        } else {
            self.statement()
        };
        match result {
            Ok(statement) => Some(statement),
            Err(Reported) => {
                self.depth = depth;
                self.skip_to_statement();
                None
            }
        }
    }

    fn var_decl(&mut self) -> Parsed<Stmt> {
        let name = self.name("Expect variable name.")?;
        let init = if self.eat(&Kind::Eq) {
            Some(self.expression()?)
        } else {
            None
        };
        self.expect(Kind::Semi, "Expect ';' after variable declaration.")?;
        Ok(Stmt::Var(name, init))
    }

    fn function(&mut self, is_method: bool) -> Parsed<FunDecl> {
        let what = if is_method { "method" } else { "function" };
        let name = self.name(&format!("Expect {what} name."))?;
        self.expect(Kind::LParen, &format!("Expect '(' after {what} name."))?;
        let mut params = Vec::new();
        if !self.is(&Kind::RParen) {
            loop {
                if params.len() == 255 {
                    let token = self.peek().clone();
                    self.report(&token, "Can't have more than 255 parameters.");
                }
                params.push(self.name("Expect parameter name.")?);
                if !self.eat(&Kind::Comma) {
                    break;
                }
            }
        }
        self.expect(Kind::RParen, "Expect ')' after parameters.")?;
        self.expect(Kind::LBrace, &format!("Expect '{{' before {what} body."))?;
        let body = self.block()?;
        let kind = match (is_method, &*name.name) {
            (false, _) => FnKind::Function,
            (true, "init") => FnKind::Initializer,
            (true, _) => FnKind::Method,
        };
        Ok(FunDecl {
            name,
            params,
            body,
            kind,
        })
    }

    fn class_decl(&mut self) -> Parsed<Stmt> {
        let name = self.name("Expect class name.")?;
        let superclass = if self.eat(&Kind::Lt) {
            Some(self.name("Expect superclass name.")?)
        } else {
            None
        };
        self.expect(Kind::LBrace, "Expect '{' before class body.")?;
        let mut methods = Vec::new();
        while !self.is(&Kind::RBrace) && !self.at_end() {
            methods.push(Rc::new(self.function(true)?));
        }
        self.expect(Kind::RBrace, "Expect '}' after class body.")?;
        Ok(Stmt::Class(ClassDecl {
            name,
            superclass,
            methods,
        }))
    }

    /// The statements after a `{`, and the closing `}`.
    fn block(&mut self) -> Parsed<Vec<Stmt>> {
        self.deeper()?;
        let mut list = Vec::new();
        while !self.is(&Kind::RBrace) && !self.at_end() {
            list.extend(self.declaration());
        }
        self.expect(Kind::RBrace, "Expect '}' after block.")?;
        self.depth -= 1;
        Ok(list)
    }

    fn statement(&mut self) -> Parsed<Stmt> {
        self.deeper()?;
        let statement = self.plain_statement()?;
        self.depth -= 1;
        Ok(statement)
    }

    fn plain_statement(&mut self) -> Parsed<Stmt> {
        if self.eat(&Kind::If) {
            return self.if_statement();
        }
        if self.eat(&Kind::While) {
            self.expect(Kind::LParen, "Expect '(' after 'while'.")?;
            let condition = self.expression()?;
            self.expect(Kind::RParen, "Expect ')' after condition.")?;
            let body = Box::new(self.statement()?);
            return Ok(Stmt::While(condition, body, None));
        }
        if self.eat(&Kind::For) {
            return self.for_statement();
        }
        if self.eat(&Kind::LBrace) {
            return Ok(Stmt::Block(self.block()?));
        }
        let keyword = self.peek().span;
        if self.eat(&Kind::Return) {
            let value = if self.is(&Kind::Semi) {
                None
            } else {
                Some(self.expression()?)
            };
            self.expect(Kind::Semi, "Expect ';' after return value.")?;
            return Ok(Stmt::Return(keyword, value));
        }
        if self.eat(&Kind::Break) {
            self.expect(Kind::Semi, "Expect ';' after 'break'.")?;
            return Ok(Stmt::Break(keyword));
        }
        if self.eat(&Kind::Continue) {
            self.expect(Kind::Semi, "Expect ';' after 'continue'.")?;
            return Ok(Stmt::Continue(keyword));
        }
        let e = self.expression()?;
        self.expect(Kind::Semi, "Expect ';' after expression.")?;
        Ok(Stmt::Expr(e))
    }

    fn if_statement(&mut self) -> Parsed<Stmt> {
        self.expect(Kind::LParen, "Expect '(' after 'if'.")?;
        let condition = self.expression()?;
        self.expect(Kind::RParen, "Expect ')' after if condition.")?;
        let then = Box::new(self.statement()?);
        let other = if self.eat(&Kind::Else) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };
        Ok(Stmt::If(condition, then, other))
    }

    /// `for (init; condition; step) body` is rewritten as `{ init; while (condition) body }`, with the step kept apart.
    fn for_statement(&mut self) -> Parsed<Stmt> {
        self.expect(Kind::LParen, "Expect '(' after 'for'.")?;
        let init = if self.eat(&Kind::Semi) {
            None
        } else if self.eat(&Kind::Var) {
            Some(self.var_decl()?)
        } else {
            let e = self.expression()?;
            self.expect(Kind::Semi, "Expect ';' after loop initializer.")?;
            Some(Stmt::Expr(e))
        };
        let condition = if self.is(&Kind::Semi) {
            Expr::Bool(true)
        } else {
            self.expression()?
        };
        self.expect(Kind::Semi, "Expect ';' after loop condition.")?;
        let step = if self.is(&Kind::RParen) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(Kind::RParen, "Expect ')' after for clauses.")?;
        let body = Box::new(self.statement()?);
        let looped = Stmt::While(condition, body, step);
        Ok(match init {
            Some(init) => Stmt::Block(vec![init, looped]),
            None => looped,
        })
    }

    fn expression(&mut self) -> Parsed<Expr> {
        self.deeper()?;
        let e = self.assignment()?;
        self.depth -= 1;
        Ok(e)
    }

    fn assignment(&mut self) -> Parsed<Expr> {
        let target = self.binary(1)?;
        if !self.is(&Kind::Eq) {
            return Ok(target);
        }
        let equals = self.advance();
        let value = Box::new(self.expression()?);
        Ok(match target {
            Expr::Var(n) => Expr::Assign(n, value),
            Expr::Get(object, name, span) => Expr::Set(object, name, value, span),
            other => {
                // The text is still readable, so report it and carry on without skipping.
                self.report(&equals, "Invalid assignment target.");
                other
            }
        })
    }

    /// Reads an operand, then takes operators that bind at least as tightly as `min`.
    fn binary(&mut self, min: u8) -> Parsed<Expr> {
        let depth = self.depth;
        let mut left = self.unary()?;
        while let Some((power, what)) = infix(&self.peek().kind) {
            if power < min {
                break;
            }
            self.deeper()?; // a long chain is a deep tree that leans to the left
            let span = self.advance().span;
            let right = Box::new(self.binary(power + 1)?);
            left = match what {
                Infix::Logical(is_and) => Expr::Logical(Box::new(left), is_and, right),
                Infix::Binary(op) => Expr::Binary(Box::new(left), op, right, span),
            };
        }
        self.depth = depth;
        Ok(left)
    }

    fn unary(&mut self) -> Parsed<Expr> {
        let op = match self.peek().kind {
            Kind::Minus => UnOp::Neg,
            Kind::Bang => UnOp::Not,
            _ => return self.call(),
        };
        self.deeper()?;
        let span = self.advance().span;
        let operand = Box::new(self.unary()?);
        self.depth -= 1;
        Ok(Expr::Unary(op, operand, span))
    }

    fn call(&mut self) -> Parsed<Expr> {
        let depth = self.depth;
        let mut e = self.primary()?;
        while matches!(self.peek().kind, Kind::LParen | Kind::Dot) {
            self.deeper()?;
            if self.eat(&Kind::Dot) {
                let n = self.name("Expect property name after '.'.")?;
                e = Expr::Get(Box::new(e), n.name, n.span);
                continue;
            }
            self.advance();
            let mut args = Vec::new();
            if !self.is(&Kind::RParen) {
                loop {
                    if args.len() == 255 {
                        let token = self.peek().clone();
                        self.report(&token, "Can't have more than 255 arguments.");
                    }
                    args.push(self.expression()?);
                    if !self.eat(&Kind::Comma) {
                        break;
                    }
                }
            }
            let close = self.expect(Kind::RParen, "Expect ')' after arguments.")?;
            e = Expr::Call(Box::new(e), args, close.span);
        }
        self.depth = depth;
        Ok(e)
    }

    fn primary(&mut self) -> Parsed<Expr> {
        let token = self.peek().clone();
        let e = match &token.kind {
            Kind::False => Expr::Bool(false),
            Kind::True => Expr::Bool(true),
            Kind::Nil => Expr::Nil,
            Kind::Num(n) => Expr::Num(*n),
            Kind::Str(s) => Expr::Str(s.clone()),
            Kind::This => Expr::This(Name::new("this".into(), token.span)),
            Kind::Ident(s) => Expr::Var(Name::new(s.clone(), token.span)),
            Kind::Super => {
                self.advance();
                self.expect(Kind::Dot, "Expect '.' after 'super'.")?;
                let method = self.name("Expect superclass method name.")?;
                let this_super = Name::new("super".into(), token.span);
                return Ok(Expr::Super(this_super, method.name, method.span));
            }
            Kind::LParen => {
                self.advance();
                let inner = self.expression()?;
                self.expect(Kind::RParen, "Expect ')' after expression.")?;
                return Ok(inner);
            }
            _ => return self.fail("Expect expression."),
        };
        self.advance();
        Ok(e)
    }
}

/// Builds the tree. `tokens` ends with `Eof`, as `scan` returns them. Statements that had a mistake are left out.
pub fn parse_tokens(tokens: Vec<Token>) -> (Vec<Stmt>, Vec<Diag>) {
    let mut parser = Parser {
        tokens,
        current: 0,
        depth: 0,
        diags: Vec::new(),
    };
    let mut program = Vec::new();
    while !parser.at_end() {
        program.extend(parser.declaration());
    }
    (program, parser.diags)
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

// ---------------------------------------------------------------- names

struct Local {
    defined: bool,
    used: bool,
    span: Span,
    warn: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InClass {
    No,
    Plain,
    Subclass,
}

struct Resolver {
    scopes: Vec<HashMap<Rc<str>, Local>>,
    function: Option<FnKind>,
    class: InClass,
    loops: usize,
    diags: Vec<Diag>,
}

impl Resolver {
    fn error(&mut self, span: Span, msg: &str) {
        self.diags.push(Diag::error(span, At::Token, msg));
    }

    fn open(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn close(&mut self) {
        for (_, local) in self.scopes.pop().into_iter().flatten() {
            if local.warn && !local.used {
                self.diags.push(Diag {
                    span: local.span,
                    at: At::Token,
                    msg: "Local variable is never used.".into(),
                    warn: true,
                });
            }
        }
    }

    /// The name exists from here on, but may not be touched until `define`.
    fn declare(&mut self, n: &Name, warn: bool) {
        let Some(scope) = self.scopes.last_mut() else {
            return; // globals are looked up when the program runs
        };
        let local = Local {
            defined: false,
            used: false,
            span: n.span,
            warn,
        };
        let twice = scope.insert(n.name.clone(), local).is_some();
        n.depth.set(Some(0));
        if twice {
            self.error(n.span, "Already a variable with this name in this scope.");
        }
    }

    fn define(&mut self, n: &Name) {
        if let Some(local) = self.scopes.last_mut().and_then(|s| s.get_mut(&n.name)) {
            local.defined = true;
        }
    }

    /// A name that exists without being written in the source: `this` and `super`.
    fn implicit(&mut self, name: &str) {
        let local = Local {
            defined: true,
            used: true,
            span: Span::default(),
            warn: false,
        };
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into(), local);
        }
    }

    fn in_own_initializer(&self, n: &Name) -> bool {
        let here = self.scopes.last().and_then(|s| s.get(&n.name));
        here.is_some_and(|local| !local.defined)
    }

    fn find(&mut self, n: &Name, is_read: bool) {
        for (distance, scope) in self.scopes.iter_mut().rev().enumerate() {
            if let Some(local) = scope.get_mut(&n.name) {
                local.used |= is_read;
                n.depth.set(Some(distance));
                return;
            }
        }
        n.depth.set(None);
    }

    fn statements(&mut self, list: &[Stmt]) {
        list.iter().for_each(|s| self.statement(s));
    }

    fn statement(&mut self, s: &Stmt) {
        match s {
            Stmt::Expr(e) => self.expr(e),
            Stmt::Var(n, init) => {
                self.declare(n, true);
                if let Some(e) = init {
                    self.expr(e);
                }
                self.define(n);
            }
            Stmt::Block(list) => {
                self.open();
                self.statements(list);
                self.close();
            }
            Stmt::If(c, then, other) => {
                self.expr(c);
                self.statement(then);
                if let Some(other) = other {
                    self.statement(other);
                }
            }
            Stmt::While(c, body, step) => {
                self.expr(c);
                self.loops += 1;
                self.statement(body);
                self.loops -= 1;
                if let Some(step) = step {
                    self.expr(step);
                }
            }
            Stmt::Break(span) => {
                if self.loops == 0 {
                    self.error(*span, "Can't use 'break' outside of a loop.");
                }
            }
            Stmt::Continue(span) => {
                if self.loops == 0 {
                    self.error(*span, "Can't use 'continue' outside of a loop.");
                }
            }
            Stmt::Return(span, value) => {
                if self.function.is_none() {
                    self.error(*span, "Can't return from top-level code.");
                }
                if let Some(value) = value {
                    if self.function == Some(FnKind::Initializer) {
                        self.error(*span, "Can't return a value from an initializer.");
                    }
                    self.expr(value);
                }
            }
            Stmt::Fun(f) => {
                self.declare(&f.name, false);
                self.define(&f.name);
                self.function(f);
            }
            Stmt::Class(c) => self.class(c),
        }
    }

    fn function(&mut self, f: &FunDecl) {
        let outer = (self.function, self.loops);
        (self.function, self.loops) = (Some(f.kind), 0); // a loop outside is out of reach
        self.open();
        for p in &f.params {
            self.declare(p, false);
            self.define(p);
        }
        self.statements(&f.body);
        self.close();
        (self.function, self.loops) = outer;
    }

    fn class(&mut self, c: &ClassDecl) {
        let outer = self.class;
        self.class = InClass::Plain;
        self.declare(&c.name, false);
        self.define(&c.name);
        if let Some(parent) = &c.superclass {
            if parent.name == c.name.name {
                self.error(parent.span, "A class can't inherit from itself.");
            }
            self.class = InClass::Subclass;
            self.find(parent, true);
            self.open();
            self.implicit("super");
        }
        self.open();
        self.implicit("this");
        c.methods.iter().for_each(|m| self.function(m));
        self.close();
        if c.superclass.is_some() {
            self.close();
        }
        self.class = outer;
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Nil | Expr::Bool(_) | Expr::Num(_) | Expr::Str(_) => {}
            Expr::Var(n) => {
                if self.in_own_initializer(n) {
                    self.error(n.span, "Can't read local variable in its own initializer.");
                }
                self.find(n, true);
            }
            Expr::Assign(n, value) => {
                self.expr(value);
                if self.in_own_initializer(n) {
                    self.error(
                        n.span,
                        "Can't assign to local variable in its own initializer.",
                    );
                }
                self.find(n, false); // writing a variable is not using it
            }
            Expr::Unary(_, x, _) | Expr::Get(x, _, _) => self.expr(x),
            Expr::Binary(l, _, r, _) | Expr::Logical(l, _, r) | Expr::Set(l, _, r, _) => {
                self.expr(l);
                self.expr(r);
            }
            Expr::Call(callee, args, _) => {
                self.expr(callee);
                args.iter().for_each(|a| self.expr(a));
            }
            Expr::This(n) => {
                if self.class == InClass::No {
                    self.error(n.span, "Can't use 'this' outside of a class.");
                } else {
                    self.find(n, true);
                }
            }
            Expr::Super(n, _, _) => match self.class {
                InClass::No => self.error(n.span, "Can't use 'super' outside of a class."),
                InClass::Plain => {
                    self.error(n.span, "Can't use 'super' in a class with no superclass.")
                }
                InClass::Subclass => self.find(n, true),
            },
        }
    }
}

/// Fills in `depth` for every name, and returns errors and warnings in any order.
pub fn resolve(program: &[Stmt]) -> Vec<Diag> {
    let mut resolver = Resolver {
        scopes: Vec::new(),
        function: None,
        class: InClass::No,
        loops: 0,
        diags: Vec::new(),
    };
    resolver.statements(program);
    resolver.diags
}

/// Everything before running. `Ok` holds the checked tree and the warnings; `Err` holds the errors only.
pub fn front_end(src: &str) -> Result<(Vec<Stmt>, Vec<String>), Vec<String>> {
    let program = parse(src)?;
    let mut diags = resolve(&program);
    if diags.iter().any(|d| !d.warn) {
        diags.retain(|d| !d.warn);
        Err(report(src, diags))
    } else {
        Ok((program, report(src, diags)))
    }
}

// ---------------------------------------------------------------- values

/// Bytes in one string, whether made by `+` or by turning a value into text.
pub const MAX_TEXT: usize = 1 << 20;
/// Items in one list.
pub const MAX_LIST: usize = 100_000;
/// Bytes printed by one run, line ends included.
pub const MAX_OUTPUT: usize = 1 << 22;
/// Lists, functions, classes and objects alive at once.
pub const MAX_LIVE: usize = 100_000;

/// What running a program gave.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// What the program printed, up to where it stopped.
    pub out: String,
    /// A runtime error: the message, then the trace, one line each.
    pub error: Option<String>,
    /// Errors found before running. When there are any, nothing ran.
    pub compile_errors: Vec<String>,
    pub warnings: Vec<String>,
}

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
}

/// One live object. Every endless chain of values passes through objects that hold one of these, so the limit also
/// bounds how deep Rust recurses when it frees a chain.
struct Alive;

impl Alive {
    fn new() -> Result<Alive, String> {
        let live = LIVE.get() + 1;
        if live > MAX_LIVE {
            return Err("Out of memory: too many live objects.".into());
        }
        LIVE.set(live);
        Ok(Alive)
    }
}

impl Drop for Alive {
    fn drop(&mut self) {
        LIVE.set(LIVE.get().saturating_sub(1));
    }
}

pub struct List {
    items: RefCell<Vec<Value>>,
    _alive: Alive,
}

type Native = Result<Value, String>;

pub struct NativeFn {
    name: &'static str,
    min: usize,
    max: usize,
    run: fn(&mut String, &[Value]) -> Native,
}

pub struct Function {
    decl: Rc<FunDecl>,
    closure: Rc<Env>,
    is_init: bool,
    _alive: Alive,
}

pub struct Class {
    name: Rc<str>,
    superclass: Option<Rc<Class>>,
    methods: HashMap<Rc<str>, Rc<Function>>,
    _alive: Alive,
}

impl Class {
    fn find_method(&self, name: &str) -> Option<Rc<Function>> {
        std::iter::successors(Some(self), |c| c.superclass.as_deref())
            .find_map(|c| c.methods.get(name).cloned())
    }
}

pub struct Instance {
    class: Rc<Class>,
    fields: RefCell<HashMap<Rc<str>, Value>>,
    _alive: Alive,
}

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    List(Rc<List>),
    Native(&'static NativeFn),
    Function(Rc<Function>),
    Class(Rc<Class>),
    Instance(Rc<Instance>),
    Closure(Rc<Closure>),
}

/// Only `nil` and `false` count as false.
fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Bool(false))
}

/// Numbers, strings, booleans and nil compare by value; everything else is equal only to itself.
fn equals(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Nil, Nil) => true,
        (Bool(x), Bool(y)) => x == y,
        (Num(x), Num(y)) => x == y,
        (Str(x), Str(y)) => x == y,
        (List(x), List(y)) => Rc::ptr_eq(x, y),
        (Native(x), Native(y)) => std::ptr::eq(*x, *y),
        (Function(x), Function(y)) => Rc::ptr_eq(x, y),
        (Class(x), Class(y)) => Rc::ptr_eq(x, y),
        (Instance(x), Instance(y)) => Rc::ptr_eq(x, y),
        (Closure(x), Closure(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

fn room(bytes: usize) -> Result<(), String> {
    if bytes > MAX_TEXT {
        return Err("String too long.".into());
    }
    Ok(())
}

/// A value as `print` writes it. A list may hold one list many times, so the text can be exponentially longer than the
/// value: the length is checked as it grows.
fn text_of(v: &Value) -> Result<String, String> {
    let mut out = String::new();
    write_value(&mut out, v, 0)?;
    Ok(out)
}

fn write_value(out: &mut String, v: &Value, depth: usize) -> Result<(), String> {
    match v {
        Value::Nil => out.push_str("nil"),
        Value::Bool(b) => {
            let _ = write!(out, "{b}");
        }
        Value::Num(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Str(s) => {
            room(out.len() + s.len())?;
            out.push_str(s);
        }
        Value::List(_) if depth > 8 => out.push_str("[...]"),
        Value::List(list) => {
            out.push('[');
            for (i, item) in list.items.borrow().iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_value(out, item, depth + 1)?;
                room(out.len())?;
            }
            out.push(']');
        }
        Value::Native(n) => {
            let _ = write!(out, "<native {}>", n.name);
        }
        Value::Function(f) => {
            let _ = write!(out, "<fn {}>", f.decl.name.name);
        }
        Value::Class(c) => {
            let _ = write!(out, "<class {}>", c.name);
        }
        Value::Instance(i) => {
            let _ = write!(out, "{} instance", i.class.name);
        }
        Value::Closure(c) => {
            let _ = write!(out, "<fn {}>", c.proto.name);
        }
    }
    room(out.len())
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Num(_) => "number",
        Value::Str(_) => "string",
        Value::List(_) => "list",
        Value::Native(_) => "native",
        Value::Function(_) => "function",
        Value::Class(_) => "class",
        Value::Instance(_) => "instance",
        Value::Closure(_) => "function",
    }
}

fn binary(op: BinOp, left: &Value, right: &Value) -> Native {
    match (op, left, right) {
        (BinOp::Eq, _, _) => Ok(Value::Bool(equals(left, right))),
        (BinOp::Ne, _, _) => Ok(Value::Bool(!equals(left, right))),
        (BinOp::Add, Value::Str(a), Value::Str(b)) => {
            room(a.len() + b.len())?;
            Ok(Value::Str(format!("{a}{b}").into()))
        }
        (_, Value::Num(a), Value::Num(b)) => numeric(op, *a, *b),
        (BinOp::Add, _, _) => Err("Operands must be two numbers or two strings.".into()),
        _ => Err("Operands must be numbers.".into()),
    }
}

fn numeric(op: BinOp, a: f64, b: f64) -> Native {
    Ok(match op {
        BinOp::Add => Value::Num(a + b),
        BinOp::Sub => Value::Num(a - b),
        BinOp::Mul => Value::Num(a * b),
        BinOp::Div | BinOp::Rem if b == 0.0 => return Err("Division by zero.".into()),
        BinOp::Div => Value::Num(a / b),
        BinOp::Rem => Value::Num(a % b),
        BinOp::Lt => Value::Bool(a < b),
        BinOp::Le => Value::Bool(a <= b),
        BinOp::Gt => Value::Bool(a > b),
        BinOp::Ge => Value::Bool(a >= b),
        BinOp::Eq => Value::Bool(a == b),
        BinOp::Ne => Value::Bool(a != b),
    })
}

fn negate(v: &Value) -> Native {
    match v {
        Value::Num(n) => Ok(Value::Num(-n)),
        _ => Err("Operand must be a number.".into()),
    }
}

fn list_arg<'a>(v: &'a Value, who: &str) -> Result<&'a List, String> {
    match v {
        Value::List(list) => Ok(list),
        _ => Err(format!("{who}() expects a list as its first argument.")),
    }
}

fn index_arg(v: &Value, len: usize) -> Result<usize, String> {
    match v {
        Value::Num(n) if *n >= 0.0 && n.fract() == 0.0 && (*n as usize) < len => Ok(*n as usize),
        _ => Err("Index out of range.".into()),
    }
}

fn native_print(out: &mut String, args: &[Value]) -> Native {
    let text = text_of(&args[0])?;
    if out.len() + text.len() >= MAX_OUTPUT {
        return Err("Output limit exceeded.".into());
    }
    out.push_str(&text);
    out.push('\n');
    Ok(Value::Nil)
}

fn native_len(_: &mut String, args: &[Value]) -> Native {
    match &args[0] {
        Value::Str(s) => Ok(Value::Num(s.chars().count() as f64)),
        Value::List(list) => Ok(Value::Num(list.items.borrow().len() as f64)),
        _ => Err("len() expects a string or a list.".into()),
    }
}

fn native_str(_: &mut String, args: &[Value]) -> Native {
    Ok(Value::Str(text_of(&args[0])?.into()))
}

fn native_num(_: &mut String, args: &[Value]) -> Native {
    Ok(match &args[0] {
        Value::Num(n) => Value::Num(*n),
        Value::Str(s) => {
            let number = s.trim().parse::<f64>().ok().filter(|n| n.is_finite());
            number.map_or(Value::Nil, Value::Num)
        }
        _ => Value::Nil,
    })
}

fn native_type(_: &mut String, args: &[Value]) -> Native {
    Ok(Value::Str(type_name(&args[0]).into()))
}

fn native_assert(_: &mut String, args: &[Value]) -> Native {
    if truthy(&args[0]) {
        return Ok(Value::Nil);
    }
    Err(match args.get(1) {
        Some(message) => format!("Assertion failed: {}", text_of(message)?),
        None => "Assertion failed.".into(),
    })
}

fn native_list(_: &mut String, _: &[Value]) -> Native {
    Ok(Value::List(Rc::new(List {
        items: RefCell::new(Vec::new()),
        _alive: Alive::new()?,
    })))
}

fn native_push(_: &mut String, args: &[Value]) -> Native {
    let list = list_arg(&args[0], "push")?;
    let mut items = list.items.borrow_mut();
    if items.len() >= MAX_LIST {
        return Err("List too long.".into());
    }
    items.push(args[1].clone());
    Ok(Value::Nil)
}

fn native_pop(_: &mut String, args: &[Value]) -> Native {
    let last = list_arg(&args[0], "pop")?.items.borrow_mut().pop();
    Ok(last.unwrap_or(Value::Nil))
}

fn native_get(_: &mut String, args: &[Value]) -> Native {
    let items = list_arg(&args[0], "get")?.items.borrow();
    Ok(items[index_arg(&args[1], items.len())?].clone())
}

fn native_set(_: &mut String, args: &[Value]) -> Native {
    let list = list_arg(&args[0], "set")?;
    let mut items = list.items.borrow_mut();
    let index = index_arg(&args[1], items.len())?;
    // The old value must be freed after the borrow ends: it may be the list itself.
    let old = std::mem::replace(&mut items[index], args[2].clone());
    drop(items);
    drop(old);
    Ok(Value::Nil)
}

macro_rules! natives {
    ($($name:literal, $min:literal, $max:literal, $run:ident;)*) => {
        [$(NativeFn { name: $name, min: $min, max: $max, run: $run }),*]
    };
}

static NATIVES: [NativeFn; 11] = natives! {
    "print", 1, 1, native_print;
    "len", 1, 1, native_len;
    "str", 1, 1, native_str;
    "num", 1, 1, native_num;
    "type", 1, 1, native_type;
    "assert", 1, 2, native_assert;
    "list", 0, 0, native_list;
    "push", 2, 2, native_push;
    "pop", 1, 1, native_pop;
    "get", 2, 2, native_get;
    "set", 3, 3, native_set;
};

fn native_globals() -> HashMap<Rc<str>, Value> {
    NATIVES
        .iter()
        .map(|n| (n.name.into(), Value::Native(n)))
        .collect()
}

fn call_native(n: &NativeFn, out: &mut String, args: &[Value]) -> Native {
    let argc = args.len();
    if argc < n.min || argc > n.max {
        return Err(if n.min == n.max {
            format!("Expected {} arguments but got {argc}.", n.min)
        } else {
            format!("Expected {} to {} arguments but got {argc}.", n.min, n.max)
        });
    }
    (n.run)(out, args)
}

fn fuel_message(fuel: u64) -> String {
    format!("Fuel exhausted: step limit ({fuel}) exceeded.")
}

// ---------------------------------------------------------------- running the tree

/// The variables of one scope, and the scope around it.
struct Env {
    vars: RefCell<Vec<(Rc<str>, Value)>>,
    parent: Option<Rc<Env>>,
}

impl Env {
    fn new(parent: Option<Rc<Env>>) -> Rc<Env> {
        Rc::new(Env {
            vars: RefCell::new(Vec::new()),
            parent,
        })
    }

    fn define(&self, name: Rc<str>, value: Value) {
        self.vars.borrow_mut().push((name, value));
    }

    fn out(self: &Rc<Env>, distance: usize) -> &Rc<Env> {
        let mut env = self;
        for _ in 0..distance {
            env = env
                .parent
                .as_ref()
                .expect("the resolver counted the scopes");
        }
        env
    }

    fn get(&self, name: &str) -> Value {
        let vars = self.vars.borrow();
        let found = vars.iter().find(|(n, _)| &**n == name);
        found.expect("the resolver saw the declaration").1.clone()
    }

    fn set(&self, name: &str, value: Value) {
        let mut vars = self.vars.borrow_mut();
        let slot = vars.iter_mut().find(|(n, _)| &**n == name);
        let slot = &mut slot.expect("the resolver saw the declaration").1;
        let old = std::mem::replace(slot, value);
        drop(vars);
        drop(old);
    }
}

/// How a statement ended.
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

struct Frame {
    name: Rc<str>,
    call_line: u32,
}

struct Failure {
    message: String,
    trace: Vec<String>,
}

type Run<T> = Result<T, Failure>;

struct Interp {
    globals: HashMap<Rc<str>, Value>,
    out: String,
    fuel: u64,
    steps: u64,
    line: u32,
    frames: Vec<Frame>,
}

impl Interp {
    /// An error with its trace, the innermost call first.
    fn failure(&self, mut line: u32, message: impl Into<String>) -> Failure {
        let mut trace = Vec::new();
        for frame in self.frames.iter().rev() {
            trace.push(format!("[line {line}] in {}()", frame.name));
            line = frame.call_line;
        }
        trace.push(format!("[line {line}] in script"));
        Failure {
            message: message.into(),
            trace,
        }
    }

    fn tick(&mut self) -> Run<()> {
        self.steps += 1;
        if self.steps > self.fuel {
            return Err(self.failure(self.line, fuel_message(self.fuel)));
        }
        Ok(())
    }

    fn block(&mut self, list: &[Stmt], env: &Rc<Env>) -> Run<Flow> {
        for s in list {
            match self.execute(s, env)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn declare(&mut self, n: &Name, value: Value, env: &Rc<Env>) {
        if n.depth.get().is_some() {
            env.define(n.name.clone(), value);
        } else {
            self.globals.insert(n.name.clone(), value);
        }
    }

    fn execute(&mut self, s: &Stmt, env: &Rc<Env>) -> Run<Flow> {
        self.tick()?;
        match s {
            Stmt::Expr(e) => {
                self.evaluate(e, env)?;
            }
            Stmt::Var(n, init) => {
                let value = match init {
                    Some(e) => self.evaluate(e, env)?,
                    None => Value::Nil,
                };
                self.declare(n, value, env);
            }
            Stmt::Block(list) => return self.block(list, &Env::new(Some(env.clone()))),
            Stmt::If(c, then, other) => {
                if truthy(&self.evaluate(c, env)?) {
                    return self.execute(then, env);
                } else if let Some(other) = other {
                    return self.execute(other, env);
                }
            }
            Stmt::While(c, body, step) => {
                while truthy(&self.evaluate(c, env)?) {
                    match self.execute(body, env)? {
                        Flow::Break => break,
                        Flow::Normal | Flow::Continue => {}
                        returned @ Flow::Return(_) => return Ok(returned),
                    }
                    if let Some(step) = step {
                        self.evaluate(step, env)?;
                    }
                }
            }
            Stmt::Break(_) => return Ok(Flow::Break),
            Stmt::Continue(_) => return Ok(Flow::Continue),
            Stmt::Return(_, value) => {
                let value = match value {
                    Some(e) => self.evaluate(e, env)?,
                    None => Value::Nil,
                };
                return Ok(Flow::Return(value));
            }
            Stmt::Fun(decl) => {
                let line = decl.name.span.line;
                let alive = Alive::new().map_err(|m| self.failure(line, m))?;
                let function = Function {
                    decl: decl.clone(),
                    closure: env.clone(),
                    is_init: false,
                    _alive: alive,
                };
                self.declare(&decl.name, Value::Function(Rc::new(function)), env);
            }
            Stmt::Class(c) => self.class(c, env)?,
        }
        Ok(Flow::Normal)
    }

    fn class(&mut self, c: &ClassDecl, env: &Rc<Env>) -> Run<()> {
        let line = c.name.span.line;
        let superclass = match &c.superclass {
            None => None,
            Some(n) => match self.lookup(n, env)? {
                Value::Class(parent) => Some(parent),
                _ => return Err(self.failure(n.span.line, "Superclass must be a class.")),
            },
        };
        // Methods close over a scope holding `super`, if there is one. Binding adds a scope holding `this`.
        let closure = match &superclass {
            Some(parent) => {
                let scope = Env::new(Some(env.clone()));
                scope.define("super".into(), Value::Class(parent.clone()));
                scope
            }
            None => env.clone(),
        };
        let mut methods = HashMap::new();
        for decl in &c.methods {
            let method = Function {
                decl: decl.clone(),
                closure: closure.clone(),
                is_init: decl.kind == FnKind::Initializer,
                _alive: Alive::new().map_err(|m| self.failure(line, m))?,
            };
            methods.insert(decl.name.name.clone(), Rc::new(method));
        }
        let class = Class {
            name: c.name.name.clone(),
            superclass,
            methods,
            _alive: Alive::new().map_err(|m| self.failure(line, m))?,
        };
        self.declare(&c.name, Value::Class(Rc::new(class)), env);
        Ok(())
    }

    /// A copy of `method` with one more scope, holding `this`.
    fn bind(&mut self, method: &Function, this: Value, line: u32) -> Run<Rc<Function>> {
        let scope = Env::new(Some(method.closure.clone()));
        scope.define("this".into(), this);
        Ok(Rc::new(Function {
            decl: method.decl.clone(),
            closure: scope,
            is_init: method.is_init,
            _alive: Alive::new().map_err(|m| self.failure(line, m))?,
        }))
    }

    fn lookup(&mut self, n: &Name, env: &Rc<Env>) -> Run<Value> {
        self.line = n.span.line;
        match n.depth.get() {
            Some(distance) => Ok(env.out(distance).get(&n.name)),
            None => match self.globals.get(&n.name) {
                Some(value) => Ok(value.clone()),
                None => Err(self.failure(n.span.line, format!("Undefined variable '{}'.", n.name))),
            },
        }
    }

    fn evaluate(&mut self, e: &Expr, env: &Rc<Env>) -> Run<Value> {
        self.tick()?;
        match e {
            Expr::Nil => Ok(Value::Nil),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Num(n) => Ok(Value::Num(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Var(n) => self.lookup(n, env),
            Expr::Assign(n, value) => {
                let value = self.evaluate(value, env)?;
                self.line = n.span.line;
                match n.depth.get() {
                    Some(distance) => env.out(distance).set(&n.name, value.clone()),
                    None => match self.globals.get_mut(&n.name) {
                        Some(slot) => *slot = value.clone(),
                        None => {
                            let message = format!("Undefined variable '{}'.", n.name);
                            return Err(self.failure(n.span.line, message));
                        }
                    },
                }
                Ok(value)
            }
            Expr::Unary(op, operand, span) => {
                let value = self.evaluate(operand, env)?;
                self.line = span.line;
                match op {
                    UnOp::Neg => negate(&value).map_err(|m| self.failure(span.line, m)),
                    UnOp::Not => Ok(Value::Bool(!truthy(&value))),
                }
            }
            Expr::Binary(left, op, right, span) => {
                let left = self.evaluate(left, env)?;
                let right = self.evaluate(right, env)?;
                self.line = span.line;
                binary(*op, &left, &right).map_err(|m| self.failure(span.line, m))
            }
            Expr::Logical(left, is_and, right) => {
                let left = self.evaluate(left, env)?;
                if truthy(&left) == *is_and {
                    self.evaluate(right, env)
                } else {
                    Ok(left)
                }
            }
            Expr::Call(callee, args, span) => {
                let callee = self.evaluate(callee, env)?;
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.evaluate(a, env)?);
                }
                self.line = span.line;
                self.call(callee, values, span.line)
            }
            Expr::This(n) => self.lookup(n, env),
            Expr::Get(object, name, span) => {
                let object = self.evaluate(object, env)?;
                self.line = span.line;
                let Value::Instance(instance) = object else {
                    return Err(self.failure(span.line, "Only instances have properties."));
                };
                let field = instance.fields.borrow().get(name).cloned();
                if let Some(value) = field {
                    return Ok(value);
                }
                let Some(method) = instance.class.find_method(name) else {
                    return Err(self.failure(span.line, format!("Undefined property '{name}'.")));
                };
                let bound = self.bind(&method, Value::Instance(instance), span.line)?;
                Ok(Value::Function(bound))
            }
            Expr::Set(object, name, value, span) => {
                let object = self.evaluate(object, env)?;
                self.line = span.line;
                let Value::Instance(instance) = object else {
                    return Err(self.failure(span.line, "Only instances have fields."));
                };
                let value = self.evaluate(value, env)?;
                let old = instance
                    .fields
                    .borrow_mut()
                    .insert(name.clone(), value.clone());
                drop(old);
                Ok(value)
            }
            Expr::Super(n, name, span) => {
                self.line = span.line;
                let distance = n.depth.get().expect("the resolver found `super`");
                let Value::Class(parent) = env.out(distance).get("super") else {
                    unreachable!("`super` always holds a class");
                };
                // The scope holding `this` is the one just inside the scope holding `super`.
                let this = env.out(distance - 1).get("this");
                let Some(method) = parent.find_method(name) else {
                    return Err(self.failure(span.line, format!("Undefined property '{name}'.")));
                };
                Ok(Value::Function(self.bind(&method, this, span.line)?))
            }
        }
    }

    fn call(&mut self, callee: Value, args: Vec<Value>, line: u32) -> Run<Value> {
        match callee {
            Value::Native(n) => {
                call_native(n, &mut self.out, &args).map_err(|m| self.failure(line, m))
            }
            Value::Function(f) => self.call_function(&f, args, line),
            Value::Class(class) => {
                let init = class.find_method("init");
                let instance = Value::Instance(Rc::new(Instance {
                    class,
                    fields: RefCell::new(HashMap::new()),
                    _alive: Alive::new().map_err(|m| self.failure(line, m))?,
                }));
                match init {
                    Some(init) => {
                        let bound = self.bind(&init, instance.clone(), line)?;
                        self.call_function(&bound, args, line)?;
                    }
                    None if args.is_empty() => {}
                    None => {
                        let message = format!("Expected 0 arguments but got {}.", args.len());
                        return Err(self.failure(line, message));
                    }
                }
                Ok(instance)
            }
            _ => Err(self.failure(line, "Can only call functions and classes.")),
        }
    }

    fn call_function(&mut self, f: &Function, args: Vec<Value>, line: u32) -> Run<Value> {
        let wanted = f.decl.params.len();
        if args.len() != wanted {
            let message = format!("Expected {wanted} arguments but got {}.", args.len());
            return Err(self.failure(line, message));
        }
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(self.failure(line, "Stack overflow."));
        }
        let env = Env::new(Some(f.closure.clone()));
        for (param, value) in f.decl.params.iter().zip(args) {
            env.define(param.name.clone(), value);
        }
        self.frames.push(Frame {
            name: f.decl.name.name.clone(),
            call_line: line,
        });
        // On an error the frame stays: nothing runs after it.
        let flow = self.block(&f.decl.body, &env)?;
        self.frames.pop();
        if f.is_init {
            return Ok(f.closure.get("this")); // `init` gives back the object, also after a bare `return;`
        }
        Ok(match flow {
            Flow::Return(value) => value,
            _ => Value::Nil,
        })
    }
}

/// Runs a checked tree. Returns what was printed and the error, if any.
fn interpret(program: &[Stmt], fuel: u64) -> (String, Option<String>) {
    LIVE.set(0); // values that an earlier run left in cycles do not count against this one
    let mut interp = Interp {
        globals: native_globals(),
        out: String::new(),
        fuel,
        steps: 0,
        line: 1,
        frames: Vec::new(),
    };
    let root = Env::new(None);
    for s in program {
        if let Err(failure) = interp.execute(s, &root) {
            let error = format!("{}\n{}", failure.message, failure.trace.join("\n"));
            return (interp.out, Some(error));
        }
    }
    (interp.out, None)
}

/// Script calls that may be active at once.
pub const MAX_CALL_DEPTH: usize = 200;

/// Running uses a lot of Rust stack: walking a tree, and freeing a long chain of values, both recurse. So programs run
/// on a thread with room for that.
pub fn on_big_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let thread = std::thread::Builder::new().stack_size(256 << 20);
        let handle = thread.spawn_scoped(scope, work).expect("spawn a thread");
        match handle.join() {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}

/// Runs a program with the tree-walking interpreter, spending at most `fuel` steps.
pub fn run(src: &str, fuel: u64) -> Outcome {
    on_big_stack(|| run_here(src, fuel))
}

fn run_here(src: &str, fuel: u64) -> Outcome {
    match front_end(src) {
        Err(compile_errors) => Outcome {
            compile_errors,
            ..Outcome::default()
        },
        Ok((program, warnings)) => {
            let (out, error) = interpret(&program, fuel);
            Outcome {
                out,
                error,
                compile_errors: Vec::new(),
                warnings,
            }
        }
    }
}

// ---------------------------------------------------------------- bytecode

/// One instruction of a machine that works on a stack of values. Jump targets are indices into `code`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Push `consts[i]`.
    Const(usize),
    Nil,
    True,
    False,
    Pop,
    /// Push a copy of the value in stack slot `base + i`.
    GetLocal(usize),
    /// Copy the top of the stack into slot `base + i`. The top stays.
    SetLocal(usize),
    /// `consts[i]` is the name, a string.
    GetGlobal(usize),
    /// Pop the top and make it the global named by `consts[i]`.
    DefineGlobal(usize),
    /// Copy the top into an existing global. The top stays.
    SetGlobal(usize),
    /// Pop the right operand, then the left, and push the result.
    Binary(BinOp),
    Not,
    Neg,
    Jump(usize),
    /// Jump if the top of the stack counts as false. The top stays either way.
    JumpIfFalse(usize),
    /// Call the value that lies under the top `n` values, with those as arguments. All of them are replaced by the result.
    Call(usize),
    /// Push a function made from `protos[i]`.
    Closure(usize),
    /// Pop the result and leave the running function. In the script itself this ends the program.
    Return,
}

/// Compiled code for one function, or for the script.
pub struct Proto {
    pub name: Rc<str>,
    /// How many parameters it takes.
    pub arity: usize,
    pub code: Vec<Op>,
    /// The source line of each instruction: as long as `code`, and never 0.
    pub lines: Vec<u32>,
    pub consts: Vec<Value>,
    /// The functions written directly inside this one.
    pub protos: Vec<Rc<Proto>>,
}

/// The program uses something this back end does not have yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported(pub &'static str);

/// A function value of the machine: its code, and from step 9 the variables it captured.
pub struct Closure {
    proto: Rc<Proto>,
    _alive: Alive,
}

// ---------------------------------------------------------------- the compiler

struct Slot {
    name: Rc<str>,
    depth: usize,
}

struct Loop {
    /// Locals deeper than this die on `break` and `continue`.
    depth: usize,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

/// The function being compiled.
struct Building {
    name: Rc<str>,
    arity: usize,
    code: Vec<Op>,
    lines: Vec<u32>,
    consts: Vec<Value>,
    protos: Vec<Rc<Proto>>,
    /// What each stack slot of this function will hold, in order.
    locals: Vec<Slot>,
    depth: usize,
    loops: Vec<Loop>,
}

impl Building {
    fn new(name: Rc<str>, depth: usize) -> Building {
        // Slot 0 holds the function being run, so the first real local is slot 1.
        let callee = Slot {
            name: "".into(),
            depth: 0,
        };
        Building {
            name,
            arity: 0,
            code: Vec::new(),
            lines: Vec::new(),
            consts: Vec::new(),
            protos: Vec::new(),
            locals: vec![callee],
            depth,
            loops: Vec::new(),
        }
    }

    fn finish(self) -> Rc<Proto> {
        Rc::new(Proto {
            name: self.name,
            arity: self.arity,
            code: self.code,
            lines: self.lines,
            consts: self.consts,
            protos: self.protos,
        })
    }
}

type Compiled = Result<(), Unsupported>;

struct Compiler {
    /// The script first, the function being compiled last.
    functions: Vec<Building>,
    /// The line of the latest instruction that had one of its own.
    line: u32,
}

impl Compiler {
    fn f(&mut self) -> &mut Building {
        self.functions
            .last_mut()
            .expect("a function is being built")
    }

    fn emit(&mut self, op: Op) -> usize {
        let line = self.line;
        let f = self.f();
        f.code.push(op);
        f.lines.push(line);
        f.code.len() - 1
    }

    fn emit_at(&mut self, op: Op, span: Span) -> usize {
        self.line = span.line;
        self.emit(op)
    }

    /// Points the jump at `at` to the next instruction.
    fn land(&mut self, at: usize) {
        let f = self.f();
        let target = f.code.len();
        match &mut f.code[at] {
            Op::Jump(t) | Op::JumpIfFalse(t) => *t = target,
            _ => unreachable!("only jumps are patched"),
        }
    }

    fn constant(&mut self, value: Value) -> usize {
        let f = self.f();
        f.consts.push(value);
        f.consts.len() - 1
    }

    fn name_constant(&mut self, name: &str) -> usize {
        self.constant(Value::Str(name.into()))
    }

    fn add_local(&mut self, name: Rc<str>) {
        let f = self.f();
        let depth = f.depth;
        f.locals.push(Slot { name, depth });
    }

    /// Emits what removes every local deeper than `depth` from the stack, without forgetting them.
    fn drop_locals(&mut self, depth: usize) {
        let ops: Vec<Op> = self
            .f()
            .locals
            .iter()
            .rev()
            .take_while(|l| l.depth > depth)
            .map(|_| Op::Pop)
            .collect();
        for op in ops {
            self.emit(op);
        }
    }

    fn end_scope(&mut self) {
        let depth = self.f().depth - 1;
        self.f().depth = depth;
        self.drop_locals(depth);
        self.f().locals.retain(|l| l.depth <= depth);
    }

    fn local_slot(&self, function: usize, name: &str) -> Option<usize> {
        let locals = &self.functions[function].locals;
        locals.iter().rposition(|l| &*l.name == name)
    }

    /// Emits the read (or the write) of a variable.
    fn variable(&mut self, n: &Name, write: bool) -> Compiled {
        let function = self.functions.len() - 1;
        let op = if let Some(slot) = self.local_slot(function, &n.name) {
            if write {
                Op::SetLocal(slot)
            } else {
                Op::GetLocal(slot)
            }
        } else {
            if (0..function).any(|outer| self.local_slot(outer, &n.name).is_some()) {
                return Err(Unsupported("captures"));
            }
            let name = self.name_constant(&n.name);
            if write {
                Op::SetGlobal(name)
            } else {
                Op::GetGlobal(name)
            }
        };
        self.emit_at(op, n.span);
        Ok(())
    }

    fn statements(&mut self, list: &[Stmt]) -> Compiled {
        list.iter().try_for_each(|s| self.statement(s))
    }

    fn statement(&mut self, s: &Stmt) -> Compiled {
        match s {
            Stmt::Expr(e) => {
                self.expr(e)?;
                self.emit(Op::Pop);
            }
            Stmt::Var(n, init) => {
                match init {
                    Some(e) => self.expr(e)?,
                    None => {
                        self.emit_at(Op::Nil, n.span);
                    }
                }
                if self.f().depth == 0 {
                    let name = self.name_constant(&n.name);
                    self.emit_at(Op::DefineGlobal(name), n.span);
                } else {
                    // The value is already where the new local lives.
                    self.add_local(n.name.clone());
                }
            }
            Stmt::Block(list) => {
                self.f().depth += 1;
                self.statements(list)?;
                self.end_scope();
            }
            Stmt::If(c, then, other) => {
                self.expr(c)?;
                let to_else = self.emit(Op::JumpIfFalse(0));
                self.emit(Op::Pop);
                self.statement(then)?;
                let to_end = self.emit(Op::Jump(0));
                self.land(to_else);
                self.emit(Op::Pop);
                if let Some(other) = other {
                    self.statement(other)?;
                }
                self.land(to_end);
            }
            Stmt::While(c, body, step) => self.while_loop(c, body, step.as_ref())?,
            Stmt::Break(span) | Stmt::Continue(span) => {
                self.line = span.line;
                let depth = self
                    .f()
                    .loops
                    .last()
                    .expect("checked by the resolver")
                    .depth;
                self.drop_locals(depth);
                let jump = self.emit(Op::Jump(0));
                let inner = self.f().loops.last_mut().expect("checked by the resolver");
                if matches!(s, Stmt::Break(_)) {
                    inner.breaks.push(jump);
                } else {
                    inner.continues.push(jump);
                }
            }
            Stmt::Return(span, value) => {
                match value {
                    Some(e) => self.expr(e)?,
                    None => {
                        self.emit_at(Op::Nil, *span);
                    }
                }
                self.emit_at(Op::Return, *span);
            }
            Stmt::Fun(decl) => {
                let local = self.f().depth > 0;
                if local {
                    self.add_local(decl.name.name.clone()); // first, so that the body can name itself
                }
                self.function(decl)?;
                if !local {
                    let name = self.name_constant(&decl.name.name);
                    self.emit(Op::DefineGlobal(name));
                }
            }
            Stmt::Class(_) => return Err(Unsupported("classes")),
        }
        Ok(())
    }

    fn function(&mut self, decl: &FunDecl) -> Compiled {
        self.line = decl.name.span.line;
        let mut building = Building::new(decl.name.name.clone(), 1);
        building.arity = decl.params.len();
        self.functions.push(building);
        for p in &decl.params {
            self.add_local(p.name.clone());
        }
        self.statements(&decl.body)?;
        self.emit(Op::Nil);
        self.emit(Op::Return);
        let proto = self.functions.pop().expect("pushed above").finish();
        let protos = &mut self.f().protos;
        protos.push(proto);
        let index = protos.len() - 1;
        self.line = decl.name.span.line;
        self.emit(Op::Closure(index));
        Ok(())
    }

    fn while_loop(&mut self, c: &Expr, body: &Stmt, step: Option<&Expr>) -> Compiled {
        let start = self.f().code.len();
        self.expr(c)?;
        let exit = self.emit(Op::JumpIfFalse(0));
        self.emit(Op::Pop);
        let depth = self.f().depth;
        self.f().loops.push(Loop {
            depth,
            breaks: Vec::new(),
            continues: Vec::new(),
        });
        self.statement(body)?;
        let finished = self.f().loops.pop().expect("pushed above");
        finished.continues.into_iter().for_each(|j| self.land(j));
        if let Some(step) = step {
            self.expr(step)?;
            self.emit(Op::Pop);
        }
        self.emit(Op::Jump(start));
        self.land(exit);
        self.emit(Op::Pop);
        // `break` lands here: on that path the condition has been popped already.
        finished.breaks.into_iter().for_each(|j| self.land(j));
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> Compiled {
        match e {
            Expr::Nil => {
                self.emit(Op::Nil);
            }
            Expr::Bool(b) => {
                self.emit(if *b { Op::True } else { Op::False });
            }
            Expr::Num(n) => {
                let i = self.constant(Value::Num(*n));
                self.emit(Op::Const(i));
            }
            Expr::Str(s) => {
                let i = self.constant(Value::Str(s.clone()));
                self.emit(Op::Const(i));
            }
            Expr::Var(n) => self.variable(n, false)?,
            Expr::Assign(n, value) => {
                self.expr(value)?;
                self.variable(n, true)?;
            }
            Expr::Unary(op, operand, span) => {
                self.expr(operand)?;
                let op = if *op == UnOp::Neg { Op::Neg } else { Op::Not };
                self.emit_at(op, *span);
            }
            Expr::Binary(left, op, right, span) => {
                self.expr(left)?;
                self.expr(right)?;
                self.emit_at(Op::Binary(*op), *span);
            }
            Expr::Logical(left, true, right) => {
                self.expr(left)?;
                let end = self.emit(Op::JumpIfFalse(0)); // false: the left value is the answer
                self.emit(Op::Pop);
                self.expr(right)?;
                self.land(end);
            }
            Expr::Logical(left, false, right) => {
                self.expr(left)?;
                let to_right = self.emit(Op::JumpIfFalse(0));
                let end = self.emit(Op::Jump(0)); // true: the left value is the answer
                self.land(to_right);
                self.emit(Op::Pop);
                self.expr(right)?;
                self.land(end);
            }
            Expr::Call(callee, args, span) => {
                self.expr(callee)?;
                for a in args {
                    self.expr(a)?;
                }
                self.emit_at(Op::Call(args.len()), *span);
            }
            Expr::Get(..) | Expr::Set(..) | Expr::This(_) | Expr::Super(..) => {
                return Err(Unsupported("classes"));
            }
        }
        Ok(())
    }
}

/// Turns a checked tree into the script's code.
pub fn compile(program: &[Stmt]) -> Result<Rc<Proto>, Unsupported> {
    let mut compiler = Compiler {
        functions: vec![Building::new("script".into(), 0)],
        line: 1,
    };
    compiler.statements(program)?;
    compiler.emit(Op::Nil);
    compiler.emit(Op::Return);
    Ok(compiler
        .functions
        .pop()
        .expect("the script is left")
        .finish())
}

// ---------------------------------------------------------------- the machine

struct CallFrame {
    proto: Rc<Proto>,
    ip: usize,
    /// Where the function's slot 0 is on the stack.
    base: usize,
}

struct Machine {
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    globals: HashMap<Rc<str>, Value>,
    out: String,
}

fn global_name(proto: &Proto, i: usize) -> Rc<str> {
    match &proto.consts[i] {
        Value::Str(name) => name.clone(),
        _ => unreachable!("the compiler stores names as strings"),
    }
}

impl Machine {
    fn pop(&mut self) -> Value {
        self.stack
            .pop()
            .expect("the compiler keeps the stack balanced")
    }

    fn top(&self) -> Value {
        self.stack
            .last()
            .expect("the compiler keeps the stack balanced")
            .clone()
    }

    /// The message and the trace. Each frame's `ip` is one past the instruction it is running.
    fn failure(&self, message: String) -> String {
        let mut lines = vec![message];
        for (i, frame) in self.frames.iter().enumerate().rev() {
            let line = frame.proto.lines[frame.ip - 1];
            lines.push(if i == 0 {
                format!("[line {line}] in script")
            } else {
                format!("[line {line}] in {}()", frame.proto.name)
            });
        }
        lines.join("\n")
    }

    /// Runs the top frame, and the frames it calls, until the script returns. `Err` holds the message.
    fn steps(&mut self, fuel: u64) -> Result<(), String> {
        let mut spent = 0u64;
        loop {
            let frame = self.frames.last_mut().expect("a frame is running");
            let (proto, ip, base) = (frame.proto.clone(), frame.ip, frame.base);
            frame.ip += 1;
            spent += 1;
            if spent > fuel {
                return Err(fuel_message(fuel));
            }
            match proto.code[ip] {
                Op::Const(i) => self.stack.push(proto.consts[i].clone()),
                Op::Nil => self.stack.push(Value::Nil),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => drop(self.pop()),
                Op::GetLocal(slot) => self.stack.push(self.stack[base + slot].clone()),
                Op::SetLocal(slot) => self.stack[base + slot] = self.top(),
                Op::GetGlobal(i) => {
                    let name = global_name(&proto, i);
                    match self.globals.get(&name) {
                        Some(value) => self.stack.push(value.clone()),
                        None => return Err(format!("Undefined variable '{name}'.")),
                    }
                }
                Op::DefineGlobal(i) => {
                    let value = self.pop();
                    self.globals.insert(global_name(&proto, i), value);
                }
                Op::SetGlobal(i) => {
                    let name = global_name(&proto, i);
                    let value = self.top();
                    match self.globals.get_mut(&name) {
                        Some(slot) => *slot = value,
                        None => return Err(format!("Undefined variable '{name}'.")),
                    }
                }
                Op::Binary(op) => {
                    let (right, left) = (self.pop(), self.pop());
                    self.stack.push(binary(op, &left, &right)?);
                }
                Op::Not => {
                    let value = self.pop();
                    self.stack.push(Value::Bool(!truthy(&value)));
                }
                Op::Neg => {
                    let value = self.pop();
                    self.stack.push(negate(&value)?);
                }
                Op::Jump(target) => self.jump(target),
                Op::JumpIfFalse(target) => {
                    if !truthy(&self.top()) {
                        self.jump(target);
                    }
                }
                Op::Call(argc) => {
                    let callee_at = self.stack.len() - 1 - argc;
                    match self.stack[callee_at].clone() {
                        Value::Native(n) => {
                            let args = self.stack.split_off(callee_at + 1);
                            self.pop();
                            let value = call_native(n, &mut self.out, &args)?;
                            self.stack.push(value);
                        }
                        Value::Closure(closure) => {
                            let wanted = closure.proto.arity;
                            if argc != wanted {
                                return Err(format!("Expected {wanted} arguments but got {argc}."));
                            }
                            // The script is a frame too, but not a call.
                            if self.frames.len() >= MAX_CALL_DEPTH {
                                return Err("Stack overflow.".into());
                            }
                            self.frames.push(CallFrame {
                                proto: closure.proto.clone(),
                                ip: 0,
                                base: callee_at,
                            });
                        }
                        _ => return Err("Can only call functions and classes.".into()),
                    }
                }
                Op::Closure(i) => {
                    let closure = Closure {
                        proto: proto.protos[i].clone(),
                        _alive: Alive::new()?,
                    };
                    self.stack.push(Value::Closure(Rc::new(closure)));
                }
                Op::Return => {
                    if self.frames.len() == 1 {
                        return Ok(());
                    }
                    let result = self.pop();
                    self.stack.truncate(base);
                    self.frames.pop();
                    self.stack.push(result);
                }
            }
        }
    }

    fn jump(&mut self, target: usize) {
        self.frames.last_mut().expect("a frame is running").ip = target;
    }
}

/// Runs compiled code. The machine sees only the code: never the tree. Returns what was printed and the error, if any.
pub fn execute(script: &Rc<Proto>, fuel: u64) -> (String, Option<String>) {
    LIVE.set(0);
    let mut machine = Machine {
        stack: vec![Value::Nil], // the script's slot 0
        frames: vec![CallFrame {
            proto: script.clone(),
            ip: 0,
            base: 0,
        }],
        globals: native_globals(),
        out: String::new(),
    };
    let result = machine.steps(fuel).map_err(|m| machine.failure(m));
    (machine.out, result.err())
}

/// Checks, compiles and runs a program on the machine. `Err` says what the compiler does not handle yet.
pub fn run_vm(src: &str, fuel: u64) -> Result<Outcome, Unsupported> {
    // As for the tree-walker: the front end and the freeing of values recurse.
    let work = || run_vm_here(src, fuel).map_err(|u| u.0);
    on_big_stack(work).map_err(Unsupported)
}

fn run_vm_here(src: &str, fuel: u64) -> Result<Outcome, Unsupported> {
    let (program, warnings) = match front_end(src) {
        Ok(checked) => checked,
        Err(compile_errors) => {
            return Ok(Outcome {
                compile_errors,
                ..Outcome::default()
            })
        }
    };
    let script = compile(&program)?;
    let (out, error) = execute(&script, fuel);
    Ok(Outcome {
        out,
        error,
        compile_errors: Vec::new(),
        warnings,
    })
}
