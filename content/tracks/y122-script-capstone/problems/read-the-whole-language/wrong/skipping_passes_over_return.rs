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
                Class | Fun | Var | For | If | While // Wrong: `return` is skipped
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
