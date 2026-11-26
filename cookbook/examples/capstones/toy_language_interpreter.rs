//! CAPSTONE 16 (compilers): a Lox-style scripting language: scanner, Pratt/recursive-descent parser, resolver, tree-walk interpreter AND a bytecode VM, differentially tested.
//! LEVEL: capstone
//! Use in: embedded scripting, config/rule languages, learning how interpreters and compilers are layered.   Run: cargo run --release --example toy_language_interpreter
#![allow(dead_code)]
//!
//! WHAT: a small dynamically typed language in the style of Crafting Interpreters' "Lox" (jlox + clox): numbers, strings, booleans, nil,
//!   lists (via natives), variables, blocks, `if`/`while`/`for`/`break`/`continue`, first-class functions with closures, classes with
//!   methods, `init`, `this`, single inheritance and `super`. Natives: print, len, str, num, type, assert, list, push, get, set, pop.
//! WHY: every interpreter is the same pipeline; writing both a tree-walker and a bytecode VM for ONE AST lets us test each against the other.
//! PIPELINE (each stage is a plain function, tested alone and together):
//!   scan      source -> tokens with spans (strings with escapes, `//` and NESTED `/* */` comments); lexical errors are diagnostics, scanning continues
//!   parse     recursive descent for statements, Pratt (binding powers) for binary operators; PANIC-MODE recovery: on an error we record it,
//!             skip to the next statement boundary (`;` or a statement keyword) and go on, so one run reports many syntax errors with line:col + caret
//!   resolve   a static pass over the AST: computes for every variable use how many scopes up it lives (so closures capture the RIGHT variable,
//!             not whatever shadows it later), and reports use-before-define in an own initialiser, duplicate locals, `return` outside a function,
//!             `this`/`super` outside a class, `break` outside a loop; unused locals become WARNINGS
//!   run       back end A: tree-walking interpreter over environments (Rc<Env> chains), call stack of names+lines for traces, call-depth limit
//!             (Stack overflow.), step FUEL limit (so a runaway script is an error, not a hang)
//!             back end B: compile the same AST to bytecode (a Vec<Op> per function) and run it on a stack VM with clox-style UPVALUES
//!             (open upvalues point into the stack and are closed, i.e. copied to the heap, when their variable goes out of scope);
//!             the VM covers everything except classes (compile reports "unsupported" and the differential test skips those programs)
//! `for` is desugared to `while` (+ an increment slot so `continue` still runs it). Division by zero is a runtime error (not inf) so both
//!   back ends agree. Rc cycles (closure <-> environment) are never collected: acceptable for short-lived scripts, a real VM would use a GC.
//! VERIFICATION: a hand-written suite with exact expected output (fib, closures, higher-order functions, sieve, sorts, JSON printer, classes,
//!   mutual recursion, the loop-closure capture rule) and exact error messages + stack traces; then 300 seeded random programs run on BOTH back
//!   ends must produce identical output and identical errors. Timing of tree-walk vs bytecode on fib(27) is printed (not asserted).
//! COMMON USES: game/engine scripting, configuration DSLs, rule engines, formula languages, teaching compilers; the same stages appear in
//!   CPython/Lua/V8 (bytecode + upvalues), TypeScript/rustc (scanner -> parser -> resolver/semantic analysis -> back end) and linters (resolver warnings).
//! Read: Crafting Interpreters https://craftinginterpreters.com/ (parts II and III), Matklad on Pratt parsing
//!   https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html , the Lua 5 VM paper https://www.lua.org/doc/jucs05.pdf
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;
use std::time::Instant;

const MAX_CALL_DEPTH: usize = 400;
const DEFAULT_FUEL: u64 = 20_000_000_000;

// =====================================================================================================================
// Spans and diagnostics
// =====================================================================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Span {
    start: usize,
    end: usize,
    line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum At {
    Nowhere,
    Token,
    End,
}

#[derive(Clone, Debug)]
struct Diag {
    span: Span,
    at: At,
    msg: String,
    warn: bool,
}

fn line_col(src: &str, pos: usize) -> (usize, usize) {
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

impl Diag {
    fn error(span: Span, at: At, msg: &str) -> Diag {
        Diag {
            span,
            at,
            msg: msg.to_string(),
            warn: false,
        }
    }
    /// One-line form used by the tests, e.g. `[line 2:7] Error at ';': Expect expression.`
    fn short(&self, src: &str) -> String {
        let (line, col) = line_col(src, self.span.start);
        let kind = if self.warn { "Warning" } else { "Error" };
        let at = match self.at {
            At::Nowhere => String::new(),
            At::End => " at end".to_string(),
            At::Token => format!(" at '{}'", &src[self.span.start..self.span.end]),
        };
        format!("[line {line}:{col}] {kind}{at}: {}", self.msg)
    }
    /// Compiler-style rendering: the summary, the source line and a caret line under the span.
    fn render(&self, src: &str) -> String {
        let (line, col) = line_col(src, self.span.start);
        let text = src.lines().nth(line - 1).unwrap_or("");
        let width = src[self.span.start..self.span.end.min(src.len())]
            .chars()
            .count()
            .clamp(1, 40);
        format!(
            "{}\n{line:>4} | {text}\n     | {}{}",
            self.short(src),
            " ".repeat(col - 1),
            "^".repeat(width)
        )
    }
}

// =====================================================================================================================
// Scanner
// =====================================================================================================================
#[derive(Clone, Debug, PartialEq)]
enum T {
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

#[derive(Clone, Debug)]
struct Token {
    t: T,
    span: Span,
}

fn keyword(s: &str) -> Option<T> {
    Some(match s {
        "and" => T::And,
        "break" => T::Break,
        "class" => T::Class,
        "continue" => T::Continue,
        "else" => T::Else,
        "false" => T::False,
        "for" => T::For,
        "fun" => T::Fun,
        "if" => T::If,
        "nil" => T::Nil,
        "or" => T::Or,
        "return" => T::Return,
        "super" => T::Super,
        "this" => T::This,
        "true" => T::True,
        "var" => T::Var,
        "while" => T::While,
        _ => return None,
    })
}

/// Scan the whole input; lexical errors are collected and scanning continues so later errors are still found.
fn scan(src: &str) -> (Vec<Token>, Vec<Diag>) {
    let b = src.as_bytes();
    let (mut toks, mut diags) = (Vec::new(), Vec::new());
    let (mut i, mut line) = (0usize, 1u32);
    while i < b.len() {
        let (start, tok_line) = (i, line);
        let c = b[i];
        i += 1;
        let two = |i: &mut usize, next: u8, yes: T, no: T| {
            if b.get(*i) == Some(&next) {
                *i += 1;
                yes
            } else {
                no
            }
        };
        let t = match c {
            b'\n' => {
                line += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' => continue,
            b'(' => T::LParen,
            b')' => T::RParen,
            b'{' => T::LBrace,
            b'}' => T::RBrace,
            b',' => T::Comma,
            b'.' => T::Dot,
            b'-' => T::Minus,
            b'+' => T::Plus,
            b';' => T::Semi,
            b'*' => T::Star,
            b'%' => T::Percent,
            b'!' => two(&mut i, b'=', T::BangEq, T::Bang),
            b'=' => two(&mut i, b'=', T::EqEq, T::Eq),
            b'<' => two(&mut i, b'=', T::Le, T::Lt),
            b'>' => two(&mut i, b'=', T::Ge, T::Gt),
            b'/' => match b.get(i) {
                Some(b'/') => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                Some(b'*') => {
                    // Nested block comments: count `/*` and `*/` so commenting out code that has comments works.
                    i += 1;
                    let mut depth = 1;
                    while i < b.len() && depth > 0 {
                        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                            depth += 1;
                            i += 2;
                        } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                            depth -= 1;
                            i += 2;
                        } else {
                            if b[i] == b'\n' {
                                line += 1;
                            }
                            i += 1;
                        }
                    }
                    if depth > 0 {
                        let span = Span {
                            start,
                            end: start + 2,
                            line: tok_line,
                        };
                        diags.push(Diag::error(
                            span,
                            At::Nowhere,
                            "Unterminated block comment.",
                        ));
                    }
                    continue;
                }
                _ => T::Slash,
            },
            b'"' => {
                let (mut buf, mut closed) = (Vec::new(), false);
                while i < b.len() {
                    let ch = b[i];
                    i += 1;
                    match ch {
                        b'"' => {
                            closed = true;
                            break;
                        }
                        b'\\' => {
                            let esc = match b.get(i) {
                                Some(b'n') => Some(b'\n'),
                                Some(b't') => Some(b'\t'),
                                Some(b'\\') => Some(b'\\'),
                                Some(b'"') => Some(b'"'),
                                _ => None,
                            };
                            match esc {
                                Some(e) => buf.push(e),
                                None => {
                                    let span = Span {
                                        start: i - 1,
                                        end: (i + 1).min(b.len()),
                                        line,
                                    };
                                    diags.push(Diag::error(
                                        span,
                                        At::Nowhere,
                                        "Invalid escape sequence.",
                                    ));
                                }
                            }
                            i += 1;
                        }
                        b'\n' => {
                            line += 1;
                            buf.push(ch);
                        }
                        _ => buf.push(ch),
                    }
                }
                if !closed {
                    let span = Span {
                        start,
                        end: b.len().min(start + 1),
                        line: tok_line,
                    };
                    diags.push(Diag::error(span, At::Nowhere, "Unterminated string."));
                    continue;
                }
                T::Str(Rc::from(String::from_utf8_lossy(&buf).as_ref()))
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
                T::Num(src[start..i].parse().unwrap_or(0.0))
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &src[start..i];
                keyword(word).unwrap_or_else(|| T::Ident(Rc::from(word)))
            }
            _ => {
                // Not ASCII-significant: decode the whole char so the span never splits a UTF-8 sequence.
                let ch = src[start..].chars().next().unwrap_or('?');
                i = start + ch.len_utf8();
                let span = Span {
                    start,
                    end: i,
                    line: tok_line,
                };
                diags.push(Diag::error(
                    span,
                    At::Nowhere,
                    &format!("Unexpected character '{ch}'."),
                ));
                continue;
            }
        };
        toks.push(Token {
            t,
            span: Span {
                start,
                end: i,
                line: tok_line,
            },
        });
    }
    toks.push(Token {
        t: T::Eof,
        span: Span {
            start: b.len(),
            end: b.len(),
            line,
        },
    });
    (toks, diags)
}

// =====================================================================================================================
// AST
// =====================================================================================================================
/// A variable reference. `depth` is filled in by the resolver: `Some(n)` = n scopes up, `None` = global.
#[derive(Debug)]
struct Name {
    name: Rc<str>,
    span: Span,
    depth: Cell<Option<usize>>,
}
impl Name {
    fn new(name: Rc<str>, span: Span) -> Name {
        Name {
            name,
            span,
            depth: Cell::new(None),
        }
    }
    fn synthetic(name: &str, span: Span) -> Name {
        Name::new(Rc::from(name), span)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
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
enum UnOp {
    Neg,
    Not,
}

#[derive(Debug)]
enum Expr {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Var(Name),
    Assign(Name, Box<Expr>),
    Unary(UnOp, Box<Expr>, Span),
    Binary(Box<Expr>, BinOp, Box<Expr>, Span),
    Logical(Box<Expr>, bool, Box<Expr>), // bool = is_and
    Call(Box<Expr>, Vec<Expr>, Span),    // span of the closing paren (its line is what traces show)
    Get(Box<Expr>, Rc<str>, Span),
    Set(Box<Expr>, Rc<str>, Box<Expr>, Span),
    This(Name),
    Super(Name, Rc<str>, Span),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FnKind {
    None,
    Function,
    Method,
    Initializer,
}

#[derive(Debug)]
struct FunDecl {
    name: Name,
    params: Vec<Name>,
    body: Vec<Stmt>,
    kind: FnKind,
}
#[derive(Debug)]
struct ClassDecl {
    name: Name,
    superclass: Option<Name>,
    methods: Vec<Rc<FunDecl>>,
}

#[derive(Debug)]
enum Stmt {
    Expr(Expr),
    Var(Name, Option<Expr>),
    Block(Vec<Stmt>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>, Option<Expr>), // optional increment: what `for` desugars into
    Break(Span),
    Continue(Span),
    Return(Span, Option<Expr>),
    Fun(Rc<FunDecl>),
    Class(ClassDecl),
}

// =====================================================================================================================
// Parser: recursive descent + Pratt for binary operators, panic-mode recovery
// =====================================================================================================================
struct Parser {
    toks: Vec<Token>,
    cur: usize,
    diags: Vec<Diag>,
}
/// `Err(())` means "the error is already recorded in `diags`; unwind to the nearest declaration and synchronise".
type PR<X> = Result<X, ()>;

/// Binary operator table: (binding power, kind). Higher binds tighter; all are left associative.
enum Infix {
    Or,
    And,
    Bin(BinOp),
}
fn infix(t: &T) -> Option<(u8, Infix)> {
    Some(match t {
        T::Or => (1, Infix::Or),
        T::And => (2, Infix::And),
        T::EqEq => (3, Infix::Bin(BinOp::Eq)),
        T::BangEq => (3, Infix::Bin(BinOp::Ne)),
        T::Lt => (4, Infix::Bin(BinOp::Lt)),
        T::Le => (4, Infix::Bin(BinOp::Le)),
        T::Gt => (4, Infix::Bin(BinOp::Gt)),
        T::Ge => (4, Infix::Bin(BinOp::Ge)),
        T::Plus => (5, Infix::Bin(BinOp::Add)),
        T::Minus => (5, Infix::Bin(BinOp::Sub)),
        T::Star => (6, Infix::Bin(BinOp::Mul)),
        T::Slash => (6, Infix::Bin(BinOp::Div)),
        T::Percent => (6, Infix::Bin(BinOp::Rem)),
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.toks[self.cur]
    }
    fn prev(&self) -> &Token {
        &self.toks[self.cur.saturating_sub(1)]
    }
    fn at_end(&self) -> bool {
        self.peek().t == T::Eof
    }
    fn advance(&mut self) -> Token {
        let t = self.toks[self.cur].clone();
        if t.t != T::Eof {
            self.cur += 1;
        }
        t
    }
    fn is(&self, t: &T) -> bool {
        &self.peek().t == t
    }
    fn eat(&mut self, t: &T) -> bool {
        let hit = self.is(t) && !self.at_end();
        if hit {
            self.cur += 1;
        }
        hit
    }
    fn diag_at(&mut self, tok: &Token, msg: &str) {
        let at = if tok.t == T::Eof { At::End } else { At::Token };
        self.diags.push(Diag::error(tok.span, at, msg));
    }
    fn fail<X>(&mut self, msg: &str) -> PR<X> {
        let t = self.peek().clone();
        self.diag_at(&t, msg);
        Err(())
    }
    fn consume(&mut self, t: &T, msg: &str) -> PR<Token> {
        if self.is(t) {
            Ok(self.advance())
        } else {
            self.fail(msg)
        }
    }
    fn ident(&mut self, msg: &str) -> PR<Name> {
        if let T::Ident(s) = &self.peek().t {
            let n = Name::new(s.clone(), self.peek().span);
            self.cur += 1;
            Ok(n)
        } else {
            self.fail(msg)
        }
    }

    /// Panic-mode recovery: discard tokens until a likely statement boundary.
    fn sync(&mut self) {
        self.advance();
        while !self.at_end() {
            if self.prev().t == T::Semi {
                return;
            }
            if matches!(
                self.peek().t,
                T::Class | T::Fun | T::Var | T::For | T::If | T::While | T::Return
            ) {
                return;
            }
            self.advance();
        }
    }

    fn program(&mut self) -> Vec<Stmt> {
        let mut out = vec![];
        while !self.at_end() {
            out.extend(self.declaration());
        }
        out
    }
    fn declaration(&mut self) -> Option<Stmt> {
        let r = if self.eat(&T::Class) {
            self.class_decl()
        } else if self.eat(&T::Fun) {
            self.function(false).map(|f| Stmt::Fun(Rc::new(f)))
        } else if self.eat(&T::Var) {
            self.var_decl()
        } else {
            self.statement()
        };
        match r {
            Ok(s) => Some(s),
            Err(()) => {
                self.sync();
                None
            }
        }
    }
    fn var_decl(&mut self) -> PR<Stmt> {
        let name = self.ident("Expect variable name.")?;
        let init = if self.eat(&T::Eq) {
            Some(self.expression()?)
        } else {
            None
        };
        self.consume(&T::Semi, "Expect ';' after variable declaration.")?;
        Ok(Stmt::Var(name, init))
    }
    fn function(&mut self, is_method: bool) -> PR<FunDecl> {
        let what = if is_method { "method" } else { "function" };
        let name = self.ident(&format!("Expect {what} name."))?;
        self.consume(&T::LParen, &format!("Expect '(' after {what} name."))?;
        let mut params = vec![];
        if !self.is(&T::RParen) {
            loop {
                if params.len() >= 255 {
                    let t = self.peek().clone();
                    self.diag_at(&t, "Can't have more than 255 parameters.");
                }
                params.push(self.ident("Expect parameter name.")?);
                if !self.eat(&T::Comma) {
                    break;
                }
            }
        }
        self.consume(&T::RParen, "Expect ')' after parameters.")?;
        self.consume(&T::LBrace, &format!("Expect '{{' before {what} body."))?;
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
    fn class_decl(&mut self) -> PR<Stmt> {
        let name = self.ident("Expect class name.")?;
        let superclass = if self.eat(&T::Lt) {
            Some(self.ident("Expect superclass name.")?)
        } else {
            None
        };
        self.consume(&T::LBrace, "Expect '{' before class body.")?;
        let mut methods = vec![];
        while !self.is(&T::RBrace) && !self.at_end() {
            methods.push(Rc::new(self.function(true)?));
        }
        self.consume(&T::RBrace, "Expect '}' after class body.")?;
        Ok(Stmt::Class(ClassDecl {
            name,
            superclass,
            methods,
        }))
    }
    fn block(&mut self) -> PR<Vec<Stmt>> {
        let mut out = vec![];
        while !self.is(&T::RBrace) && !self.at_end() {
            out.extend(self.declaration());
        }
        self.consume(&T::RBrace, "Expect '}' after block.")?;
        Ok(out)
    }
    fn statement(&mut self) -> PR<Stmt> {
        if self.eat(&T::If) {
            return self.if_stmt();
        }
        if self.eat(&T::While) {
            self.consume(&T::LParen, "Expect '(' after 'while'.")?;
            let cond = self.expression()?;
            self.consume(&T::RParen, "Expect ')' after condition.")?;
            return Ok(Stmt::While(cond, Box::new(self.statement()?), None));
        }
        if self.eat(&T::For) {
            return self.for_stmt();
        }
        if self.eat(&T::LBrace) {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.eat(&T::Return) {
            let kw = self.prev().span;
            let value = if self.is(&T::Semi) {
                None
            } else {
                Some(self.expression()?)
            };
            self.consume(&T::Semi, "Expect ';' after return value.")?;
            return Ok(Stmt::Return(kw, value));
        }
        if self.eat(&T::Break) {
            let kw = self.prev().span;
            self.consume(&T::Semi, "Expect ';' after 'break'.")?;
            return Ok(Stmt::Break(kw));
        }
        if self.eat(&T::Continue) {
            let kw = self.prev().span;
            self.consume(&T::Semi, "Expect ';' after 'continue'.")?;
            return Ok(Stmt::Continue(kw));
        }
        let e = self.expression()?;
        self.consume(&T::Semi, "Expect ';' after expression.")?;
        Ok(Stmt::Expr(e))
    }
    fn if_stmt(&mut self) -> PR<Stmt> {
        self.consume(&T::LParen, "Expect '(' after 'if'.")?;
        let cond = self.expression()?;
        self.consume(&T::RParen, "Expect ')' after if condition.")?;
        let then = Box::new(self.statement()?);
        let other = if self.eat(&T::Else) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };
        Ok(Stmt::If(cond, then, other))
    }
    /// `for (init; cond; incr) body`  ==>  `{ init; while (cond) body  [incr runs after each iteration] }`
    fn for_stmt(&mut self) -> PR<Stmt> {
        self.consume(&T::LParen, "Expect '(' after 'for'.")?;
        let init = if self.eat(&T::Semi) {
            None
        } else if self.eat(&T::Var) {
            Some(self.var_decl()?)
        } else {
            let e = self.expression()?;
            self.consume(&T::Semi, "Expect ';' after loop initializer.")?;
            Some(Stmt::Expr(e))
        };
        let cond = if self.is(&T::Semi) {
            Expr::Bool(true)
        } else {
            self.expression()?
        };
        self.consume(&T::Semi, "Expect ';' after loop condition.")?;
        let incr = if self.is(&T::RParen) {
            None
        } else {
            Some(self.expression()?)
        };
        self.consume(&T::RParen, "Expect ')' after for clauses.")?;
        let body = self.statement()?;
        let looped = Stmt::While(cond, Box::new(body), incr);
        Ok(match init {
            Some(i) => Stmt::Block(vec![i, looped]),
            None => looped,
        })
    }

    // ----- expressions -----
    fn expression(&mut self) -> PR<Expr> {
        let lhs = self.pratt(1)?;
        if self.is(&T::Eq) {
            let eq = self.advance();
            let value = self.expression()?; // assignment is right associative
            return Ok(match lhs {
                Expr::Var(n) => Expr::Assign(n, Box::new(value)),
                Expr::Get(o, name, sp) => Expr::Set(o, name, Box::new(value), sp),
                other => {
                    self.diag_at(&eq, "Invalid assignment target.");
                    other
                }
            });
        }
        Ok(lhs)
    }
    /// Pratt loop: parse a prefix item, then keep folding in operators whose binding power is >= `min`.
    fn pratt(&mut self, min: u8) -> PR<Expr> {
        let mut left = self.unary()?;
        while let Some((bp, kind)) = infix(&self.peek().t) {
            if bp < min {
                break;
            }
            let op_span = self.advance().span;
            let right = self.pratt(bp + 1)?; // +1 => left associative
            left = match kind {
                Infix::Or => Expr::Logical(Box::new(left), false, Box::new(right)),
                Infix::And => Expr::Logical(Box::new(left), true, Box::new(right)),
                Infix::Bin(op) => Expr::Binary(Box::new(left), op, Box::new(right), op_span),
            };
        }
        Ok(left)
    }
    fn unary(&mut self) -> PR<Expr> {
        let op = match self.peek().t {
            T::Minus => UnOp::Neg,
            T::Bang => UnOp::Not,
            _ => return self.call(),
        };
        let span = self.advance().span;
        Ok(Expr::Unary(op, Box::new(self.unary()?), span))
    }
    fn call(&mut self) -> PR<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&T::LParen) {
                let mut args = vec![];
                if !self.is(&T::RParen) {
                    loop {
                        if args.len() >= 255 {
                            let t = self.peek().clone();
                            self.diag_at(&t, "Can't have more than 255 arguments.");
                        }
                        args.push(self.expression()?);
                        if !self.eat(&T::Comma) {
                            break;
                        }
                    }
                }
                let paren = self.consume(&T::RParen, "Expect ')' after arguments.")?;
                e = Expr::Call(Box::new(e), args, paren.span);
            } else if self.eat(&T::Dot) {
                let n = self.ident("Expect property name after '.'.")?;
                e = Expr::Get(Box::new(e), n.name, n.span);
            } else {
                return Ok(e);
            }
        }
    }
    fn primary(&mut self) -> PR<Expr> {
        let tok = self.peek().clone();
        let e = match &tok.t {
            T::False => Expr::Bool(false),
            T::True => Expr::Bool(true),
            T::Nil => Expr::Nil,
            T::Num(n) => Expr::Num(*n),
            T::Str(s) => Expr::Str(s.clone()),
            T::This => Expr::This(Name::synthetic("this", tok.span)),
            T::Ident(s) => Expr::Var(Name::new(s.clone(), tok.span)),
            T::Super => {
                self.cur += 1;
                self.consume(&T::Dot, "Expect '.' after 'super'.")?;
                let m = self.ident("Expect superclass method name.")?;
                return Ok(Expr::Super(
                    Name::synthetic("super", tok.span),
                    m.name,
                    m.span,
                ));
            }
            T::LParen => {
                self.cur += 1;
                let e = self.expression()?;
                self.consume(&T::RParen, "Expect ')' after expression.")?;
                return Ok(e);
            }
            _ => return self.fail("Expect expression."),
        };
        self.cur += 1;
        Ok(e)
    }
}

// =====================================================================================================================
// Resolver: static scoping, semantic errors, unused-local warnings
// =====================================================================================================================
struct VarInfo {
    defined: bool,
    used: bool,
    span: Span,
    warn: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum ClassKind {
    None,
    Class,
    Subclass,
}
struct Resolver {
    scopes: Vec<HashMap<Rc<str>, VarInfo>>,
    func: FnKind,
    class: ClassKind,
    loops: usize,
    diags: Vec<Diag>,
}

impl Resolver {
    fn error(&mut self, span: Span, msg: &str) {
        self.diags.push(Diag::error(span, At::Token, msg));
    }
    fn begin(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn end(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            for (_, v) in scope {
                if v.warn && !v.used {
                    self.diags.push(Diag {
                        span: v.span,
                        at: At::Token,
                        msg: "Local variable is never used.".into(),
                        warn: true,
                    });
                }
            }
        }
    }
    /// Declare (not yet usable): reading the name now means "in its own initialiser".
    fn declare(&mut self, n: &Name, warn: bool) {
        let dup = match self.scopes.last_mut() {
            None => return, // globals are not tracked statically
            Some(s) => s
                .insert(
                    n.name.clone(),
                    VarInfo {
                        defined: false,
                        used: false,
                        span: n.span,
                        warn,
                    },
                )
                .is_some(),
        };
        n.depth.set(Some(0)); // marks "this declaration is a local" for the interpreters
        if dup {
            self.error(n.span, "Already a variable with this name in this scope.");
        }
    }
    fn define(&mut self, n: &Name) {
        if let Some(v) = self.scopes.last_mut().and_then(|s| s.get_mut(&n.name)) {
            v.defined = true;
        }
    }
    /// Distance = how many scopes to walk up at runtime; no hit = global.
    fn local(&mut self, n: &Name, mark_used: bool) {
        let len = self.scopes.len();
        for (i, s) in self.scopes.iter_mut().enumerate().rev() {
            if let Some(v) = s.get_mut(&n.name) {
                v.used |= mark_used;
                n.depth.set(Some(len - 1 - i));
                return;
            }
        }
        n.depth.set(None);
    }

    fn stmts(&mut self, ss: &[Stmt]) {
        for s in ss {
            self.stmt(s);
        }
    }
    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Expr(e) => self.expr(e),
            Stmt::Var(n, init) => {
                self.declare(n, true);
                if let Some(e) = init {
                    self.expr(e);
                }
                self.define(n);
            }
            Stmt::Block(ss) => {
                self.begin();
                self.stmts(ss);
                self.end();
            }
            Stmt::If(c, t, e) => {
                self.expr(c);
                self.stmt(t);
                if let Some(e) = e {
                    self.stmt(e);
                }
            }
            Stmt::While(c, body, incr) => {
                self.expr(c);
                self.loops += 1;
                self.stmt(body);
                if let Some(i) = incr {
                    self.expr(i);
                }
                self.loops -= 1;
            }
            Stmt::Break(sp) | Stmt::Continue(sp) => {
                if self.loops == 0 {
                    let kw = if matches!(s, Stmt::Break(_)) {
                        "break"
                    } else {
                        "continue"
                    };
                    self.error(*sp, &format!("Can't use '{kw}' outside of a loop."));
                }
            }
            Stmt::Return(sp, value) => {
                if self.func == FnKind::None {
                    self.error(*sp, "Can't return from top-level code.");
                }
                if let Some(v) = value {
                    if self.func == FnKind::Initializer {
                        self.error(*sp, "Can't return a value from an initializer.");
                    }
                    self.expr(v);
                }
            }
            Stmt::Fun(d) => {
                self.declare(&d.name, false);
                self.define(&d.name);
                self.function(d);
            }
            Stmt::Class(c) => self.class(c),
        }
    }
    fn function(&mut self, d: &FunDecl) {
        let (saved_fn, saved_loops) = (self.func, self.loops);
        (self.func, self.loops) = (d.kind, 0); // `break` cannot cross a function boundary
        self.begin();
        for p in &d.params {
            self.declare(p, false);
            self.define(p);
        }
        self.stmts(&d.body);
        self.end();
        (self.func, self.loops) = (saved_fn, saved_loops);
    }
    fn class(&mut self, c: &ClassDecl) {
        let saved = self.class;
        self.class = ClassKind::Class;
        self.declare(&c.name, false);
        self.define(&c.name);
        if let Some(sup) = &c.superclass {
            if sup.name == c.name.name {
                self.error(sup.span, "A class can't inherit from itself.");
            }
            self.class = ClassKind::Subclass;
            self.local(sup, true);
            self.begin();
            self.implicit("super");
        }
        self.begin();
        self.implicit("this");
        for m in &c.methods {
            self.function(m);
        }
        self.end();
        if c.superclass.is_some() {
            self.end();
        }
        self.class = saved;
    }
    /// `this` / `super` live in synthetic scopes that mirror the environments the interpreter creates when binding methods.
    fn implicit(&mut self, name: &str) {
        let info = VarInfo {
            defined: true,
            used: true,
            span: Span::default(),
            warn: false,
        };
        if let Some(s) = self.scopes.last_mut() {
            s.insert(Rc::from(name), info);
        }
    }
    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Nil | Expr::Bool(_) | Expr::Num(_) | Expr::Str(_) => {}
            Expr::Var(n) => {
                if self
                    .scopes
                    .last()
                    .and_then(|s| s.get(&n.name))
                    .is_some_and(|v| !v.defined)
                {
                    self.error(n.span, "Can't read local variable in its own initializer.");
                }
                self.local(n, true);
            }
            Expr::Assign(n, v) => {
                self.expr(v);
                self.local(n, false); // writing does not count as using
            }
            Expr::Unary(_, x, _) => self.expr(x),
            Expr::Binary(l, _, r, _) | Expr::Logical(l, _, r) => {
                self.expr(l);
                self.expr(r);
            }
            Expr::Call(c, args, _) => {
                self.expr(c);
                args.iter().for_each(|a| self.expr(a));
            }
            Expr::Get(o, _, _) => self.expr(o),
            Expr::Set(o, _, v, _) => {
                self.expr(v);
                self.expr(o);
            }
            Expr::This(n) => {
                if self.class == ClassKind::None {
                    self.error(n.span, "Can't use 'this' outside of a class.");
                } else {
                    self.local(n, true);
                }
            }
            Expr::Super(n, _, _) => match self.class {
                ClassKind::None => self.error(n.span, "Can't use 'super' outside of a class."),
                ClassKind::Class => {
                    self.error(n.span, "Can't use 'super' in a class with no superclass.")
                }
                ClassKind::Subclass => self.local(n, true),
            },
        }
    }
}

// =====================================================================================================================
// Values, natives, shared arithmetic (both back ends use the SAME helpers so they cannot disagree on semantics)
// =====================================================================================================================
type NR = Result<Value, String>;
struct NativeFn {
    name: &'static str,
    min: usize,
    max: usize,
    f: fn(&mut String, &[Value]) -> NR,
}

#[derive(Clone)]
enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    Native(&'static NativeFn),
    Func(Rc<TwFunc>),
    Class(Rc<Class>),
    Instance(Rc<Instance>),
    Closure(Rc<VmClosure>),
}

struct TwFunc {
    decl: Rc<FunDecl>,
    closure: Rc<Env>,
    is_init: bool,
}
struct Class {
    name: Rc<str>,
    superclass: Option<Rc<Class>>,
    methods: HashMap<Rc<str>, Rc<TwFunc>>,
}
impl Class {
    fn find_method(&self, name: &str) -> Option<Rc<TwFunc>> {
        self.methods
            .get(name)
            .cloned()
            .or_else(|| self.superclass.as_ref().and_then(|s| s.find_method(name)))
    }
}
struct Instance {
    class: Rc<Class>,
    fields: RefCell<HashMap<Rc<str>, Value>>,
}

fn truthy(v: &Value) -> bool {
    !matches!(v, Value::Nil | Value::Bool(false))
}
fn equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => Rc::ptr_eq(x, y),
        (Value::Native(x), Value::Native(y)) => std::ptr::eq(*x, *y),
        (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
        (Value::Class(x), Value::Class(y)) => Rc::ptr_eq(x, y),
        (Value::Instance(x), Value::Instance(y)) => Rc::ptr_eq(x, y),
        (Value::Closure(x), Value::Closure(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}
fn fmt_num(n: f64) -> String {
    if n.is_nan() {
        "nan".into()
    } else if n.is_infinite() {
        if n > 0.0 { "inf".into() } else { "-inf".into() }
    } else if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
fn stringify(v: &Value) -> String {
    stringify_depth(v, 0)
}
fn stringify_depth(v: &Value, depth: usize) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => fmt_num(*n),
        Value::Str(s) => s.to_string(),
        Value::List(_) if depth > 8 => "[...]".into(), // a list that contains itself must not recurse forever
        Value::List(l) => {
            let items: Vec<String> = l
                .borrow()
                .iter()
                .map(|x| stringify_depth(x, depth + 1))
                .collect();
            format!("[{}]", items.join(", "))
        }
        Value::Native(n) => format!("<native {}>", n.name),
        Value::Func(f) => format!("<fn {}>", f.decl.name.name),
        Value::Closure(c) => format!("<fn {}>", c.proto.name),
        Value::Class(c) => format!("<class {}>", c.name),
        Value::Instance(i) => format!("{} instance", i.class.name),
    }
}
fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Nil => "nil",
        Value::Bool(_) => "bool",
        Value::Num(_) => "number",
        Value::Str(_) => "string",
        Value::List(_) => "list",
        Value::Native(_) => "native",
        Value::Func(_) | Value::Closure(_) => "function",
        Value::Class(_) => "class",
        Value::Instance(_) => "instance",
    }
}

fn arith(op: BinOp, l: &Value, r: &Value) -> NR {
    use BinOp::*;
    match (op, l, r) {
        (Eq, _, _) => Ok(Value::Bool(equals(l, r))),
        (Ne, _, _) => Ok(Value::Bool(!equals(l, r))),
        (Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(Rc::from(format!("{a}{b}")))),
        (Add, Value::Num(_), Value::Num(_)) => num_op(op, l, r),
        (Add, _, _) => Err("Operands must be two numbers or two strings.".into()),
        _ => num_op(op, l, r),
    }
}
fn num_op(op: BinOp, l: &Value, r: &Value) -> NR {
    use BinOp::*;
    let (Value::Num(a), Value::Num(b)) = (l, r) else {
        return Err("Operands must be numbers.".into());
    };
    let (a, b) = (*a, *b);
    Ok(match op {
        Add => Value::Num(a + b),
        Sub => Value::Num(a - b),
        Mul => Value::Num(a * b),
        Div | Rem if b == 0.0 => return Err("Division by zero.".into()),
        Div => Value::Num(a / b),
        Rem => Value::Num(a % b),
        Lt => Value::Bool(a < b),
        Le => Value::Bool(a <= b),
        Gt => Value::Bool(a > b),
        Ge => Value::Bool(a >= b),
        Eq | Ne => Value::Bool(false), // handled in `arith`
    })
}
fn negate(v: &Value) -> NR {
    match v {
        Value::Num(n) => Ok(Value::Num(-n)),
        _ => Err("Operand must be a number.".into()),
    }
}

fn as_list(v: &Value, who: &str) -> Result<Rc<RefCell<Vec<Value>>>, String> {
    match v {
        Value::List(l) => Ok(l.clone()),
        _ => Err(format!("{who}() expects a list as its first argument.")),
    }
}
fn as_index(v: &Value, len: usize) -> Result<usize, String> {
    match v {
        Value::Num(n) if *n >= 0.0 && n.fract() == 0.0 && (*n as usize) < len => Ok(*n as usize),
        _ => Err("Index out of range.".into()),
    }
}
fn n_print(out: &mut String, a: &[Value]) -> NR {
    out.push_str(&stringify(&a[0]));
    out.push('\n');
    Ok(Value::Nil)
}
fn n_len(_: &mut String, a: &[Value]) -> NR {
    match &a[0] {
        Value::Str(s) => Ok(Value::Num(s.chars().count() as f64)),
        Value::List(l) => Ok(Value::Num(l.borrow().len() as f64)),
        _ => Err("len() expects a string or a list.".into()),
    }
}
fn n_str(_: &mut String, a: &[Value]) -> NR {
    Ok(Value::Str(Rc::from(stringify(&a[0]))))
}
fn n_num(_: &mut String, a: &[Value]) -> NR {
    Ok(match &a[0] {
        Value::Num(n) => Value::Num(*n),
        Value::Str(s) => s
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map_or(Value::Nil, Value::Num),
        _ => Value::Nil,
    })
}
fn n_type(_: &mut String, a: &[Value]) -> NR {
    Ok(Value::Str(Rc::from(type_name(&a[0]))))
}
fn n_assert(_: &mut String, a: &[Value]) -> NR {
    if truthy(&a[0]) {
        return Ok(Value::Nil);
    }
    Err(match a.get(1) {
        Some(m) => format!("Assertion failed: {}", stringify(m)),
        None => "Assertion failed.".into(),
    })
}
fn n_list(_: &mut String, _: &[Value]) -> NR {
    Ok(Value::List(Rc::new(RefCell::new(Vec::new()))))
}
fn n_push(_: &mut String, a: &[Value]) -> NR {
    as_list(&a[0], "push")?.borrow_mut().push(a[1].clone());
    Ok(Value::Nil)
}
fn n_pop(_: &mut String, a: &[Value]) -> NR {
    Ok(as_list(&a[0], "pop")?
        .borrow_mut()
        .pop()
        .unwrap_or(Value::Nil))
}
fn n_get(_: &mut String, a: &[Value]) -> NR {
    let l = as_list(&a[0], "get")?;
    let l = l.borrow();
    Ok(l[as_index(&a[1], l.len())?].clone())
}
fn n_set(_: &mut String, a: &[Value]) -> NR {
    let l = as_list(&a[0], "set")?;
    let mut l = l.borrow_mut();
    let i = as_index(&a[1], l.len())?;
    l[i] = a[2].clone();
    Ok(Value::Nil)
}
static NATIVES: [NativeFn; 11] = [
    NativeFn {
        name: "print",
        min: 1,
        max: 1,
        f: n_print,
    },
    NativeFn {
        name: "len",
        min: 1,
        max: 1,
        f: n_len,
    },
    NativeFn {
        name: "str",
        min: 1,
        max: 1,
        f: n_str,
    },
    NativeFn {
        name: "num",
        min: 1,
        max: 1,
        f: n_num,
    },
    NativeFn {
        name: "type",
        min: 1,
        max: 1,
        f: n_type,
    },
    NativeFn {
        name: "assert",
        min: 1,
        max: 2,
        f: n_assert,
    },
    NativeFn {
        name: "list",
        min: 0,
        max: 0,
        f: n_list,
    },
    NativeFn {
        name: "push",
        min: 2,
        max: 2,
        f: n_push,
    },
    NativeFn {
        name: "pop",
        min: 1,
        max: 1,
        f: n_pop,
    },
    NativeFn {
        name: "get",
        min: 2,
        max: 2,
        f: n_get,
    },
    NativeFn {
        name: "set",
        min: 3,
        max: 3,
        f: n_set,
    },
];
fn native_globals() -> HashMap<Rc<str>, Value> {
    NATIVES
        .iter()
        .map(|n| (Rc::from(n.name), Value::Native(n)))
        .collect()
}
fn check_native_arity(n: &NativeFn, argc: usize) -> Result<(), String> {
    if argc >= n.min && argc <= n.max {
        Ok(())
    } else if n.min == n.max {
        Err(format!("Expected {} arguments but got {argc}.", n.min))
    } else {
        Err(format!(
            "Expected {} to {} arguments but got {argc}.",
            n.min, n.max
        ))
    }
}

/// A runtime error with its stack trace, innermost frame first.
struct RtErr {
    msg: String,
    trace: Vec<String>,
}
impl RtErr {
    fn text(&self) -> String {
        format!("{}\n{}", self.msg, self.trace.join("\n"))
    }
}
fn fuel_msg(fuel: u64) -> String {
    format!("Fuel exhausted: step limit ({fuel}) exceeded.")
}

// =====================================================================================================================
// Back end A: tree-walking interpreter
// =====================================================================================================================
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
    fn define(&self, name: Rc<str>, v: Value) {
        self.vars.borrow_mut().push((name, v));
    }
    /// The resolver guarantees `d` scopes exist; falling back to `self` is unreachable but keeps this panic-free.
    fn ancestor<'a>(self: &'a Rc<Env>, d: usize) -> &'a Rc<Env> {
        let mut e = self;
        for _ in 0..d {
            e = e.parent.as_ref().unwrap_or(e);
        }
        e
    }
    fn get(&self, name: &str) -> Option<Value> {
        self.vars
            .borrow()
            .iter()
            .rev()
            .find(|(k, _)| &**k == name)
            .map(|(_, v)| v.clone())
    }
    fn set(&self, name: &str, v: Value) -> bool {
        match self
            .vars
            .borrow_mut()
            .iter_mut()
            .rev()
            .find(|(k, _)| &**k == name)
        {
            Some(slot) => {
                slot.1 = v;
                true
            }
            None => false,
        }
    }
}

enum Flow {
    Next,
    Break,
    Continue,
    Return(Value),
}
struct Frame {
    name: Rc<str>,
    call_line: u32,
}
type Ev<X> = Result<X, RtErr>;

struct Interp {
    globals: HashMap<Rc<str>, Value>,
    out: String,
    fuel: u64,
    steps: u64,
    line: u32,
    frames: Vec<Frame>,
}

impl Interp {
    fn new(fuel: u64) -> Interp {
        Interp {
            globals: native_globals(),
            out: String::new(),
            fuel,
            steps: 0,
            line: 1,
            frames: vec![Frame {
                name: Rc::from("script"),
                call_line: 0,
            }],
        }
    }
    /// Build an error whose trace lists `[line N] in f()` from the innermost call outwards.
    fn rt(&self, line: u32, msg: impl Into<String>) -> RtErr {
        let n = self.frames.len();
        let trace = (0..n)
            .rev()
            .map(|i| {
                let l = if i == n - 1 {
                    line
                } else {
                    self.frames[i + 1].call_line
                };
                if i == 0 {
                    format!("[line {l}] in script")
                } else {
                    format!("[line {l}] in {}()", self.frames[i].name)
                }
            })
            .collect();
        RtErr {
            msg: msg.into(),
            trace,
        }
    }
    fn tick(&mut self) -> Ev<()> {
        self.steps += 1;
        if self.steps > self.fuel {
            return Err(self.rt(self.line, fuel_msg(self.fuel)));
        }
        Ok(())
    }
    fn run(&mut self, prog: &[Stmt]) -> Ev<()> {
        let root = Env::new(None);
        for s in prog {
            self.exec(s, &root)?;
        }
        Ok(())
    }

    fn exec_block(&mut self, ss: &[Stmt], env: &Rc<Env>) -> Ev<Flow> {
        for s in ss {
            match self.exec(s, env)? {
                Flow::Next => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
    }
    fn declare(&mut self, n: &Name, v: Value, env: &Rc<Env>) {
        if n.depth.get().is_some() {
            env.define(n.name.clone(), v);
        } else {
            self.globals.insert(n.name.clone(), v);
        }
    }
    fn exec(&mut self, s: &Stmt, env: &Rc<Env>) -> Ev<Flow> {
        self.tick()?;
        match s {
            Stmt::Expr(e) => {
                self.eval(e, env)?;
            }
            Stmt::Var(n, init) => {
                let v = match init {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Nil,
                };
                self.declare(n, v, env);
            }
            Stmt::Block(ss) => return self.exec_block(ss, &Env::new(Some(env.clone()))),
            Stmt::If(c, t, e) => {
                if truthy(&self.eval(c, env)?) {
                    return self.exec(t, env);
                } else if let Some(e) = e {
                    return self.exec(e, env);
                }
            }
            Stmt::While(c, body, incr) => {
                while truthy(&self.eval(c, env)?) {
                    match self.exec(body, env)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Next | Flow::Continue => {}
                    }
                    if let Some(i) = incr {
                        self.eval(i, env)?; // `continue` lands here, so the increment is never skipped
                    }
                }
            }
            Stmt::Break(_) => return Ok(Flow::Break),
            Stmt::Continue(_) => return Ok(Flow::Continue),
            Stmt::Return(_, v) => {
                let v = match v {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Nil,
                };
                return Ok(Flow::Return(v));
            }
            Stmt::Fun(d) => {
                let f = Value::Func(Rc::new(TwFunc {
                    decl: d.clone(),
                    closure: env.clone(),
                    is_init: false,
                }));
                self.declare(&d.name, f, env);
            }
            Stmt::Class(c) => self.exec_class(c, env)?,
        }
        Ok(Flow::Next)
    }
    fn exec_class(&mut self, c: &ClassDecl, env: &Rc<Env>) -> Ev<()> {
        let superclass = match &c.superclass {
            None => None,
            Some(n) => match self.lookup(n, env)? {
                Value::Class(k) => Some(k),
                _ => return Err(self.rt(n.span.line, "Superclass must be a class.")),
            },
        };
        // Methods close over an environment holding `super` (if any); binding later adds `this` on top.
        let method_env = match &superclass {
            Some(k) => {
                let e = Env::new(Some(env.clone()));
                e.define(Rc::from("super"), Value::Class(k.clone()));
                e
            }
            None => env.clone(),
        };
        let methods = c
            .methods
            .iter()
            .map(|m| {
                (
                    m.name.name.clone(),
                    Rc::new(TwFunc {
                        decl: m.clone(),
                        closure: method_env.clone(),
                        is_init: m.kind == FnKind::Initializer,
                    }),
                )
            })
            .collect();
        let class = Value::Class(Rc::new(Class {
            name: c.name.name.clone(),
            superclass,
            methods,
        }));
        self.declare(&c.name, class, env);
        Ok(())
    }

    fn lookup(&mut self, n: &Name, env: &Rc<Env>) -> Ev<Value> {
        self.line = n.span.line;
        match n.depth.get() {
            Some(d) => Ok(env.ancestor(d).get(&n.name).unwrap_or(Value::Nil)),
            None => match self.globals.get(&n.name) {
                Some(v) => Ok(v.clone()),
                None => Err(self.rt(n.span.line, format!("Undefined variable '{}'.", n.name))),
            },
        }
    }
    fn eval(&mut self, e: &Expr, env: &Rc<Env>) -> Ev<Value> {
        self.tick()?;
        match e {
            Expr::Nil => Ok(Value::Nil),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Num(n) => Ok(Value::Num(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Var(n) => self.lookup(n, env),
            Expr::This(n) => self.lookup(n, env),
            Expr::Assign(n, v) => {
                let value = self.eval(v, env)?;
                self.line = n.span.line;
                match n.depth.get() {
                    Some(d) => {
                        env.ancestor(d).set(&n.name, value.clone());
                    }
                    None if self.globals.contains_key(&n.name) => {
                        self.globals.insert(n.name.clone(), value.clone());
                    }
                    None => {
                        return Err(
                            self.rt(n.span.line, format!("Undefined variable '{}'.", n.name))
                        );
                    }
                }
                Ok(value)
            }
            Expr::Unary(op, x, sp) => {
                let v = self.eval(x, env)?;
                self.line = sp.line;
                match op {
                    UnOp::Neg => negate(&v).map_err(|m| self.rt(sp.line, m)),
                    UnOp::Not => Ok(Value::Bool(!truthy(&v))),
                }
            }
            Expr::Binary(l, op, r, sp) => {
                let a = self.eval(l, env)?;
                let b = self.eval(r, env)?;
                self.line = sp.line;
                arith(*op, &a, &b).map_err(|m| self.rt(sp.line, m))
            }
            Expr::Logical(l, is_and, r) => {
                let a = self.eval(l, env)?;
                if truthy(&a) == *is_and {
                    self.eval(r, env)
                } else {
                    Ok(a)
                } // short-circuit
            }
            Expr::Call(c, args, sp) => {
                let callee = self.eval(c, env)?;
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval(a, env)?);
                }
                self.line = sp.line;
                self.call(callee, vals, sp.line)
            }
            Expr::Get(o, name, sp) => {
                let obj = self.eval(o, env)?;
                self.line = sp.line;
                let Value::Instance(inst) = obj else {
                    return Err(self.rt(sp.line, "Only instances have properties."));
                };
                if let Some(v) = inst.fields.borrow().get(name) {
                    return Ok(v.clone());
                }
                match inst.class.find_method(name) {
                    Some(m) => Ok(Value::Func(bind(&m, Value::Instance(inst.clone())))),
                    None => Err(self.rt(sp.line, format!("Undefined property '{name}'."))),
                }
            }
            Expr::Set(o, name, v, sp) => {
                let obj = self.eval(o, env)?;
                self.line = sp.line;
                let Value::Instance(inst) = obj else {
                    return Err(self.rt(sp.line, "Only instances have fields."));
                };
                let value = self.eval(v, env)?;
                inst.fields.borrow_mut().insert(name.clone(), value.clone());
                Ok(value)
            }
            Expr::Super(n, method, sp) => {
                self.line = sp.line;
                let d = n.depth.get().unwrap_or(0);
                let sup = env.ancestor(d).get("super");
                let this = if d > 0 {
                    env.ancestor(d - 1).get("this")
                } else {
                    None
                };
                let (Some(Value::Class(sup)), Some(this)) = (sup, this) else {
                    return Err(self.rt(sp.line, "Invalid use of 'super'."));
                };
                match sup.find_method(method) {
                    Some(m) => Ok(Value::Func(bind(&m, this))),
                    None => Err(self.rt(sp.line, format!("Undefined property '{method}'."))),
                }
            }
        }
    }

    fn call(&mut self, callee: Value, args: Vec<Value>, line: u32) -> Ev<Value> {
        match callee {
            Value::Func(f) => self.call_function(&f, args, line),
            Value::Native(n) => {
                check_native_arity(n, args.len()).map_err(|m| self.rt(line, m))?;
                (n.f)(&mut self.out, &args).map_err(|m| self.rt(line, m))
            }
            Value::Class(k) => {
                let inst = Value::Instance(Rc::new(Instance {
                    class: k.clone(),
                    fields: RefCell::new(HashMap::new()),
                }));
                match k.find_method("init") {
                    Some(init) => {
                        self.call_function(&bind(&init, inst.clone()), args, line)?;
                    }
                    None if args.is_empty() => {}
                    None => {
                        return Err(self.rt(
                            line,
                            format!("Expected 0 arguments but got {}.", args.len()),
                        ));
                    }
                }
                Ok(inst)
            }
            _ => Err(self.rt(line, "Can only call functions and classes.")),
        }
    }
    fn call_function(&mut self, f: &Rc<TwFunc>, args: Vec<Value>, line: u32) -> Ev<Value> {
        if args.len() != f.decl.params.len() {
            return Err(self.rt(
                line,
                format!(
                    "Expected {} arguments but got {}.",
                    f.decl.params.len(),
                    args.len()
                ),
            ));
        }
        if self.frames.len() > MAX_CALL_DEPTH {
            return Err(self.rt(line, "Stack overflow."));
        }
        let env = Env::new(Some(f.closure.clone()));
        for (p, v) in f.decl.params.iter().zip(args) {
            env.define(p.name.clone(), v);
        }
        self.frames.push(Frame {
            name: f.decl.name.name.clone(),
            call_line: line,
        });
        let flow = self.exec_block(&f.decl.body, &env)?;
        self.frames.pop();
        if f.is_init {
            return Ok(f.closure.get("this").unwrap_or(Value::Nil)); // `init` always returns the instance
        }
        Ok(match flow {
            Flow::Return(v) => v,
            _ => Value::Nil,
        })
    }
}

/// Method binding = a copy of the function whose closure has one extra scope holding `this`.
fn bind(m: &Rc<TwFunc>, this: Value) -> Rc<TwFunc> {
    let env = Env::new(Some(m.closure.clone()));
    env.define(Rc::from("this"), this);
    Rc::new(TwFunc {
        decl: m.decl.clone(),
        closure: env,
        is_init: m.is_init,
    })
}

// =====================================================================================================================
// Back end B: bytecode compiler + stack VM with upvalue closures (no classes)
// =====================================================================================================================
#[derive(Clone, Copy, Debug)]
enum Op {
    Const(usize),
    Nil,
    True,
    False,
    Pop,
    GetLocal(usize),
    SetLocal(usize),
    GetGlobal(usize),
    DefGlobal(usize),
    SetGlobal(usize),
    GetUpvalue(usize),
    SetUpvalue(usize),
    Bin(BinOp),
    Not,
    Neg,
    Jump(usize),
    JumpIfFalse(usize), // peeks the condition; the compiler emits the Pop on each path
    Loop(usize),        // absolute backwards jump
    Call(usize),
    Closure(usize),
    CloseUpvalue,
    Return,
}

/// A compiled function: code, per-instruction source lines (for traces), constants, nested prototypes, upvalue descriptors.
struct Proto {
    name: Rc<str>,
    arity: usize,
    code: Vec<Op>,
    lines: Vec<u32>,
    consts: Vec<Value>,
    protos: Vec<Rc<Proto>>,
    upvals: Vec<(bool, usize)>, // (captures a local of the enclosing function?, slot or upvalue index)
}
struct VmClosure {
    proto: Rc<Proto>,
    upvalues: Vec<Rc<RefCell<Upv>>>,
}
/// Open = still lives in the VM stack at that slot; Closed = the value was moved to the heap when its scope ended.
enum Upv {
    Open(usize),
    Closed(Value),
}

struct Local {
    name: Rc<str>,
    depth: usize,
    captured: bool,
}
struct LoopCtx {
    depth: usize, // scope depth at loop entry: locals deeper than this die on break/continue
    breaks: Vec<usize>,
    continues: Vec<usize>,
}
struct FnState {
    name: Rc<str>,
    arity: usize,
    code: Vec<Op>,
    lines: Vec<u32>,
    consts: Vec<Value>,
    protos: Vec<Rc<Proto>>,
    upvals: Vec<(bool, usize)>,
    locals: Vec<Local>,
    depth: usize,
    loops: Vec<LoopCtx>,
}
impl FnState {
    fn new(name: Rc<str>, depth: usize) -> FnState {
        // slot 0 holds the callee itself, as in clox, so locals start at 1
        let slot0 = Local {
            name: Rc::from(""),
            depth: 0,
            captured: false,
        };
        FnState {
            name,
            arity: 0,
            code: vec![],
            lines: vec![],
            consts: vec![],
            protos: vec![],
            upvals: vec![],
            locals: vec![slot0],
            depth,
            loops: vec![],
        }
    }
}
struct Compiler {
    fns: Vec<FnState>,
}
type CR = Result<(), String>;

impl Compiler {
    fn f(&mut self) -> &mut FnState {
        self.fns
            .last_mut()
            .expect("compiler always has an active function")
    }
    fn emit(&mut self, op: Op, line: u32) -> usize {
        let f = self.f();
        f.code.push(op);
        f.lines.push(line);
        f.code.len() - 1
    }
    fn here(&mut self) -> usize {
        self.f().code.len()
    }
    fn patch(&mut self, at: usize) {
        let target = self.here();
        match &mut self.f().code[at] {
            Op::Jump(t) | Op::JumpIfFalse(t) => *t = target,
            _ => {}
        }
    }
    fn constant(&mut self, v: Value) -> usize {
        let f = self.f();
        f.consts.push(v);
        f.consts.len() - 1
    }
    fn name_const(&mut self, n: &str) -> usize {
        self.constant(Value::Str(Rc::from(n)))
    }
    fn end_scope(&mut self, line: u32) {
        self.f().depth -= 1;
        loop {
            let f = self.f();
            let d = f.depth;
            match f.locals.last() {
                Some(l) if l.depth > d => {
                    let op = if l.captured {
                        Op::CloseUpvalue
                    } else {
                        Op::Pop
                    };
                    f.locals.pop();
                    self.emit(op, line);
                }
                _ => break,
            }
        }
    }
    /// Pops for locals that die when jumping out of loop bodies (without forgetting them: the code after the jump still uses them).
    fn emit_exits(&mut self, depth: usize, line: u32) {
        let ops: Vec<Op> = self
            .f()
            .locals
            .iter()
            .rev()
            .take_while(|l| l.depth > depth)
            .map(|l| {
                if l.captured {
                    Op::CloseUpvalue
                } else {
                    Op::Pop
                }
            })
            .collect();
        for op in ops {
            self.emit(op, line);
        }
    }
    fn add_local(&mut self, name: Rc<str>) {
        let depth = self.f().depth;
        self.f().locals.push(Local {
            name,
            depth,
            captured: false,
        });
    }
    fn resolve_local(&self, fi: usize, name: &str) -> Option<usize> {
        self.fns[fi].locals.iter().rposition(|l| &*l.name == name)
    }
    fn resolve_upvalue(&mut self, fi: usize, name: &str) -> Option<usize> {
        if fi == 0 {
            return None;
        }
        if let Some(slot) = self.resolve_local(fi - 1, name) {
            self.fns[fi - 1].locals[slot].captured = true;
            return Some(self.add_upvalue(fi, true, slot));
        }
        let up = self.resolve_upvalue(fi - 1, name)?;
        Some(self.add_upvalue(fi, false, up))
    }
    fn add_upvalue(&mut self, fi: usize, is_local: bool, index: usize) -> usize {
        let ups = &mut self.fns[fi].upvals;
        match ups.iter().position(|u| *u == (is_local, index)) {
            Some(i) => i,
            None => {
                ups.push((is_local, index));
                ups.len() - 1
            }
        }
    }
    fn load_var(&mut self, name: &str, line: u32) {
        let fi = self.fns.len() - 1;
        let op = if let Some(s) = self.resolve_local(fi, name) {
            Op::GetLocal(s)
        } else if let Some(u) = self.resolve_upvalue(fi, name) {
            Op::GetUpvalue(u)
        } else {
            Op::GetGlobal(self.name_const(name))
        };
        self.emit(op, line);
    }
    fn store_var(&mut self, name: &str, line: u32) {
        let fi = self.fns.len() - 1;
        let op = if let Some(s) = self.resolve_local(fi, name) {
            Op::SetLocal(s)
        } else if let Some(u) = self.resolve_upvalue(fi, name) {
            Op::SetUpvalue(u)
        } else {
            Op::SetGlobal(self.name_const(name))
        };
        self.emit(op, line);
    }

    fn stmts(&mut self, ss: &[Stmt]) -> CR {
        ss.iter().try_for_each(|s| self.stmt(s))
    }
    /// Bind the value on top of the stack to `n`: a global definition at depth 0, otherwise the value simply stays as a new local slot.
    fn bind_name(&mut self, n: &Name) {
        if self.f().depth == 0 {
            let c = self.name_const(&n.name);
            self.emit(Op::DefGlobal(c), n.span.line);
        } else {
            self.add_local(n.name.clone());
        }
    }
    fn stmt(&mut self, s: &Stmt) -> CR {
        match s {
            Stmt::Expr(e) => {
                self.expr(e)?;
                self.emit(Op::Pop, 0);
            }
            Stmt::Var(n, init) => {
                match init {
                    Some(e) => self.expr(e)?,
                    None => {
                        self.emit(Op::Nil, n.span.line);
                    }
                }
                self.bind_name(n);
            }
            Stmt::Block(ss) => {
                self.f().depth += 1;
                self.stmts(ss)?;
                self.end_scope(0);
            }
            Stmt::If(c, t, e) => {
                self.expr(c)?;
                let to_else = self.emit(Op::JumpIfFalse(0), 0);
                self.emit(Op::Pop, 0);
                self.stmt(t)?;
                let to_end = self.emit(Op::Jump(0), 0);
                self.patch(to_else);
                self.emit(Op::Pop, 0);
                if let Some(e) = e {
                    self.stmt(e)?;
                }
                self.patch(to_end);
            }
            Stmt::While(c, body, incr) => self.while_loop(c, body, incr.as_ref())?,
            Stmt::Break(sp) | Stmt::Continue(sp) => {
                let depth = self.f().loops.last().map(|l| l.depth).unwrap_or(0);
                self.emit_exits(depth, sp.line);
                let j = self.emit(Op::Jump(0), sp.line);
                if let Some(l) = self.f().loops.last_mut() {
                    if matches!(s, Stmt::Break(_)) {
                        l.breaks.push(j)
                    } else {
                        l.continues.push(j)
                    }
                }
            }
            Stmt::Return(sp, v) => {
                match v {
                    Some(e) => self.expr(e)?,
                    None => {
                        self.emit(Op::Nil, sp.line);
                    }
                }
                self.emit(Op::Return, sp.line);
            }
            Stmt::Fun(d) => {
                if self.f().depth > 0 {
                    self.add_local(d.name.name.clone()); // declared first so the body can refer to itself
                }
                let line = d.name.span.line;
                self.function(d)?;
                if self.f().depth == 0 {
                    let c = self.name_const(&d.name.name);
                    self.emit(Op::DefGlobal(c), line);
                }
            }
            Stmt::Class(_) => return Err("the bytecode back end does not support classes".into()),
        }
        Ok(())
    }
    fn while_loop(&mut self, c: &Expr, body: &Stmt, incr: Option<&Expr>) -> CR {
        let start = self.here();
        self.expr(c)?;
        let exit = self.emit(Op::JumpIfFalse(0), 0);
        self.emit(Op::Pop, 0);
        let depth = self.f().depth;
        self.f().loops.push(LoopCtx {
            depth,
            breaks: vec![],
            continues: vec![],
        });
        self.stmt(body)?;
        let ctx = self.f().loops.pop().expect("loop context pushed above");
        for j in ctx.continues {
            self.patch(j); // `continue` jumps here: to the increment (or straight back to the condition)
        }
        if let Some(i) = incr {
            self.expr(i)?;
            self.emit(Op::Pop, 0);
        }
        self.emit(Op::Loop(start), 0);
        self.patch(exit);
        self.emit(Op::Pop, 0);
        for j in ctx.breaks {
            self.patch(j); // `break` jumps past the loop (the condition was already popped on that path)
        }
        Ok(())
    }
    fn function(&mut self, d: &FunDecl) -> CR {
        let mut st = FnState::new(d.name.name.clone(), 1);
        st.arity = d.params.len();
        self.fns.push(st);
        for p in &d.params {
            self.add_local(p.name.clone());
        }
        self.stmts(&d.body)?;
        let line = d.name.span.line;
        self.emit(Op::Nil, line);
        self.emit(Op::Return, line);
        let st = self.fns.pop().expect("function state pushed above");
        let proto = Rc::new(Proto {
            name: st.name,
            arity: st.arity,
            code: st.code,
            lines: st.lines,
            consts: st.consts,
            protos: st.protos,
            upvals: st.upvals,
        });
        let f = self.f();
        f.protos.push(proto);
        let idx = f.protos.len() - 1;
        self.emit(Op::Closure(idx), line);
        Ok(())
    }
    fn expr(&mut self, e: &Expr) -> CR {
        match e {
            Expr::Nil => {
                self.emit(Op::Nil, 0);
            }
            Expr::Bool(b) => {
                self.emit(if *b { Op::True } else { Op::False }, 0);
            }
            Expr::Num(n) => {
                let c = self.constant(Value::Num(*n));
                self.emit(Op::Const(c), 0);
            }
            Expr::Str(s) => {
                let c = self.constant(Value::Str(s.clone()));
                self.emit(Op::Const(c), 0);
            }
            Expr::Var(n) => self.load_var(&n.name, n.span.line),
            Expr::Assign(n, v) => {
                self.expr(v)?;
                self.store_var(&n.name, n.span.line);
            }
            Expr::Unary(op, x, sp) => {
                self.expr(x)?;
                self.emit(if *op == UnOp::Neg { Op::Neg } else { Op::Not }, sp.line);
            }
            Expr::Binary(l, op, r, sp) => {
                self.expr(l)?;
                self.expr(r)?;
                self.emit(Op::Bin(*op), sp.line);
            }
            Expr::Logical(l, is_and, r) => {
                self.expr(l)?;
                if *is_and {
                    let end = self.emit(Op::JumpIfFalse(0), 0); // false: keep the left value as the result
                    self.emit(Op::Pop, 0);
                    self.expr(r)?;
                    self.patch(end);
                } else {
                    let rhs = self.emit(Op::JumpIfFalse(0), 0);
                    let end = self.emit(Op::Jump(0), 0); // truthy: keep the left value
                    self.patch(rhs);
                    self.emit(Op::Pop, 0);
                    self.expr(r)?;
                    self.patch(end);
                }
            }
            Expr::Call(c, args, sp) => {
                self.expr(c)?;
                for a in args {
                    self.expr(a)?;
                }
                self.emit(Op::Call(args.len()), sp.line);
            }
            Expr::Get(..) | Expr::Set(..) | Expr::This(_) | Expr::Super(..) => {
                return Err("the bytecode back end does not support classes".into());
            }
        }
        Ok(())
    }
}

fn compile(prog: &[Stmt]) -> Result<Rc<Proto>, String> {
    let mut c = Compiler {
        fns: vec![FnState::new(Rc::from("script"), 0)],
    };
    c.stmts(prog)?;
    c.emit(Op::Nil, 0);
    c.emit(Op::Return, 0);
    let st = c.fns.pop().expect("script state");
    Ok(Rc::new(Proto {
        name: st.name,
        arity: 0,
        code: st.code,
        lines: st.lines,
        consts: st.consts,
        protos: st.protos,
        upvals: st.upvals,
    }))
}

struct CallFrame {
    closure: Rc<VmClosure>,
    ip: usize,
    base: usize,
}
struct Vm {
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    globals: HashMap<Rc<str>, Value>,
    open: Vec<Rc<RefCell<Upv>>>,
    out: String,
    fuel: u64,
    steps: u64,
}

impl Vm {
    fn new(fuel: u64) -> Vm {
        Vm {
            stack: vec![],
            frames: vec![],
            globals: native_globals(),
            open: vec![],
            out: String::new(),
            fuel,
            steps: 0,
        }
    }
    fn pop(&mut self) -> Value {
        self.stack.pop().unwrap_or(Value::Nil) // the compiler keeps the stack balanced; Nil is only a panic-free fallback
    }
    fn peek(&self) -> Value {
        self.stack.last().cloned().unwrap_or(Value::Nil)
    }
    /// Error with a trace: every frame's saved `ip` points just past its current instruction, so `lines[ip - 1]` is where it is.
    fn err(&mut self, ip: usize, msg: impl Into<String>) -> RtErr {
        if let Some(f) = self.frames.last_mut() {
            f.ip = ip;
        }
        let n = self.frames.len();
        let trace = (0..n)
            .rev()
            .map(|i| {
                let f = &self.frames[i];
                let line = f
                    .closure
                    .proto
                    .lines
                    .get(f.ip.wrapping_sub(1))
                    .copied()
                    .unwrap_or(0);
                if i == 0 {
                    format!("[line {line}] in script")
                } else {
                    format!("[line {line}] in {}()", f.closure.proto.name)
                }
            })
            .collect();
        RtErr {
            msg: msg.into(),
            trace,
        }
    }
    fn capture(&mut self, slot: usize) -> Rc<RefCell<Upv>> {
        for u in &self.open {
            if matches!(&*u.borrow(), Upv::Open(s) if *s == slot) {
                return u.clone();
            }
        }
        let u = Rc::new(RefCell::new(Upv::Open(slot)));
        self.open.push(u.clone());
        u
    }
    /// Close every open upvalue at or above `from`: copy the stack value into the upvalue so it outlives the frame/scope.
    fn close_upvalues(&mut self, from: usize) {
        let stack = &self.stack;
        self.open.retain(|u| {
            let mut b = u.borrow_mut();
            let slot = match &*b {
                Upv::Open(s) => *s,
                Upv::Closed(_) => return false,
            };
            if slot >= from {
                *b = Upv::Closed(stack[slot].clone());
                false
            } else {
                true
            }
        });
    }
    fn run(&mut self, script: Rc<Proto>) -> Ev<()> {
        let closure = Rc::new(VmClosure {
            proto: script,
            upvalues: vec![],
        });
        self.stack.push(Value::Closure(closure.clone()));
        self.frames.push(CallFrame {
            closure,
            ip: 0,
            base: 0,
        });
        self.exec()
    }
    fn exec(&mut self) -> Ev<()> {
        let (mut closure, mut ip, mut base) = match self.frames.last() {
            Some(f) => (f.closure.clone(), f.ip, f.base),
            None => return Ok(()),
        };
        loop {
            self.steps += 1;
            if self.steps > self.fuel {
                return Err(self.err(ip, fuel_msg(self.fuel)));
            }
            let op = closure.proto.code[ip];
            ip += 1;
            match op {
                Op::Const(i) => self.stack.push(closure.proto.consts[i].clone()),
                Op::Nil => self.stack.push(Value::Nil),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => {
                    self.pop();
                }
                Op::GetLocal(s) => self.stack.push(self.stack[base + s].clone()),
                Op::SetLocal(s) => self.stack[base + s] = self.peek(),
                Op::GetGlobal(i) => {
                    let name = const_name(&closure.proto, i);
                    match self.globals.get(&name) {
                        Some(v) => self.stack.push(v.clone()),
                        None => return Err(self.err(ip, format!("Undefined variable '{name}'."))),
                    }
                }
                Op::DefGlobal(i) => {
                    let v = self.pop();
                    self.globals.insert(const_name(&closure.proto, i), v);
                }
                Op::SetGlobal(i) => {
                    let name = const_name(&closure.proto, i);
                    if !self.globals.contains_key(&name) {
                        return Err(self.err(ip, format!("Undefined variable '{name}'.")));
                    }
                    self.globals.insert(name, self.peek());
                }
                Op::GetUpvalue(i) => {
                    let v = match &*closure.upvalues[i].borrow() {
                        Upv::Open(s) => self.stack[*s].clone(),
                        Upv::Closed(v) => v.clone(),
                    };
                    self.stack.push(v);
                }
                Op::SetUpvalue(i) => {
                    let v = self.peek();
                    match &mut *closure.upvalues[i].borrow_mut() {
                        Upv::Open(s) => self.stack[*s] = v,
                        Upv::Closed(c) => *c = v,
                    }
                }
                Op::Bin(b) => {
                    let (r, l) = (self.pop(), self.pop());
                    match arith(b, &l, &r) {
                        Ok(v) => self.stack.push(v),
                        Err(m) => return Err(self.err(ip, m)),
                    }
                }
                Op::Not => {
                    let v = self.pop();
                    self.stack.push(Value::Bool(!truthy(&v)));
                }
                Op::Neg => {
                    let v = self.pop();
                    match negate(&v) {
                        Ok(v) => self.stack.push(v),
                        Err(m) => return Err(self.err(ip, m)),
                    }
                }
                Op::Jump(t) => ip = t,
                Op::JumpIfFalse(t) => {
                    if !truthy(&self.peek()) {
                        ip = t;
                    }
                }
                Op::Loop(t) => ip = t,
                Op::Closure(i) => {
                    let proto = closure.proto.protos[i].clone();
                    let upvalues = proto
                        .upvals
                        .iter()
                        .map(|&(is_local, idx)| {
                            if is_local {
                                self.capture(base + idx)
                            } else {
                                closure.upvalues[idx].clone()
                            }
                        })
                        .collect();
                    self.stack
                        .push(Value::Closure(Rc::new(VmClosure { proto, upvalues })));
                }
                Op::CloseUpvalue => {
                    self.close_upvalues(self.stack.len().saturating_sub(1));
                    self.pop();
                }
                Op::Call(argc) => {
                    let callee = self.stack[self.stack.len() - 1 - argc].clone();
                    match callee {
                        Value::Closure(c) => {
                            if c.proto.arity != argc {
                                return Err(self.err(
                                    ip,
                                    format!("Expected {} arguments but got {argc}.", c.proto.arity),
                                ));
                            }
                            if self.frames.len() > MAX_CALL_DEPTH {
                                return Err(self.err(ip, "Stack overflow."));
                            }
                            if let Some(f) = self.frames.last_mut() {
                                f.ip = ip;
                            }
                            base = self.stack.len() - 1 - argc;
                            self.frames.push(CallFrame {
                                closure: c.clone(),
                                ip: 0,
                                base,
                            });
                            closure = c;
                            ip = 0;
                        }
                        Value::Native(n) => {
                            if let Err(m) = check_native_arity(n, argc) {
                                return Err(self.err(ip, m));
                            }
                            let args = self.stack.split_off(self.stack.len() - argc);
                            self.pop();
                            match (n.f)(&mut self.out, &args) {
                                Ok(v) => self.stack.push(v),
                                Err(m) => return Err(self.err(ip, m)),
                            }
                        }
                        _ => return Err(self.err(ip, "Can only call functions and classes.")),
                    }
                }
                Op::Return => {
                    let result = self.pop();
                    self.close_upvalues(base);
                    self.stack.truncate(base);
                    self.frames.pop();
                    let Some(f) = self.frames.last() else {
                        return Ok(());
                    };
                    (closure, ip, base) = (f.closure.clone(), f.ip, f.base);
                    self.stack.push(result);
                }
            }
        }
    }
}
fn const_name(p: &Proto, i: usize) -> Rc<str> {
    match &p.consts[i] {
        Value::Str(s) => s.clone(),
        _ => Rc::from("?"),
    }
}

// =====================================================================================================================
// Driver: source -> outcome on either back end
// =====================================================================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum Backend {
    Tree,
    Vm,
}
#[derive(Default, Debug)]
struct Outcome {
    out: String,
    error: Option<String>,
    compile_errors: Vec<String>,
    warnings: Vec<String>,
    unsupported: bool,
}

/// Front end shared by both back ends. Returns the AST, or all diagnostics (sorted by position) if any is an error.
fn front_end(src: &str) -> Result<(Vec<Stmt>, Vec<Diag>), Vec<Diag>> {
    let (toks, mut diags) = scan(src);
    let mut p = Parser {
        toks,
        cur: 0,
        diags: vec![],
    };
    let prog = p.program();
    diags.extend(p.diags);
    if diags.is_empty() {
        let mut r = Resolver {
            scopes: vec![],
            func: FnKind::None,
            class: ClassKind::None,
            loops: 0,
            diags: vec![],
        };
        r.stmts(&prog);
        diags.extend(r.diags);
    }
    diags.sort_by_key(|d| d.span.start);
    if diags.iter().any(|d| !d.warn) {
        Err(diags)
    } else {
        Ok((prog, diags))
    }
}

fn run_source(src: &str, backend: Backend, fuel: u64) -> Outcome {
    let (prog, diags) = match front_end(src) {
        Ok(ok) => ok,
        Err(diags) => {
            let shorts = diags
                .iter()
                .filter(|d| !d.warn)
                .map(|d| d.short(src))
                .collect();
            return Outcome {
                compile_errors: shorts,
                ..Default::default()
            };
        }
    };
    let mut o = Outcome {
        warnings: diags.iter().map(|d| d.short(src)).collect(),
        ..Default::default()
    };
    match backend {
        Backend::Tree => {
            let mut it = Interp::new(fuel);
            let r = it.run(&prog);
            o.out = it.out;
            o.error = r.err().map(|e| e.text());
        }
        Backend::Vm => match compile(&prog) {
            Err(_) => o.unsupported = true,
            Ok(proto) => {
                let mut vm = Vm::new(fuel);
                let r = vm.run(proto);
                o.out = vm.out;
                o.error = r.err().map(|e| e.text());
            }
        },
    }
    o
}

// =====================================================================================================================
// Hand-written suite: programs with exact expected output / errors
// =====================================================================================================================
struct Case {
    name: &'static str,
    src: &'static str,
    out: &'static str,
    err: Option<&'static str>,
    vm: bool, // does the bytecode back end support it (no classes)?
}
const fn ok(name: &'static str, src: &'static str, out: &'static str, vm: bool) -> Case {
    Case {
        name,
        src,
        out,
        err: None,
        vm,
    }
}
const fn fails(
    name: &'static str,
    src: &'static str,
    out: &'static str,
    err: &'static str,
    vm: bool,
) -> Case {
    Case {
        name,
        src,
        out,
        err: Some(err),
        vm,
    }
}

fn suite() -> Vec<Case> {
    vec![
        ok(
            "fib recursive",
            r##"fun fib(n) {
  if (n < 2) return n;
  return fib(n - 1) + fib(n - 2);
}
for (var i = 0; i < 10; i = i + 1) print(fib(i));
print(fib(20));"##,
            "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n6765\n",
            true,
        ),
        ok(
            "fib iterative",
            r##"fun fib(n) {
  var a = 0; var b = 1;
  for (var i = 0; i < n; i = i + 1) { var t = a + b; a = b; b = t; }
  return a;
}
print(fib(30)); print(fib(1));"##,
            "832040\n1\n",
            true,
        ),
        ok(
            "closure counter",
            r##"fun makeCounter() { var n = 0; fun inc() { n = n + 1; return n; } return inc; }
var a = makeCounter(); var b = makeCounter();
print(a()); print(a()); print(b()); print(a());"##,
            "1\n2\n1\n3\n",
            true,
        ),
        ok(
            "map/filter/reduce",
            r##"fun map(l, f) { var r = list(); for (var i = 0; i < len(l); i = i + 1) push(r, f(get(l, i))); return r; }
fun filter(l, p) { var r = list(); for (var i = 0; i < len(l); i = i + 1) { var x = get(l, i); if (p(x)) push(r, x); } return r; }
fun reduce(l, f, acc) { for (var i = 0; i < len(l); i = i + 1) acc = f(acc, get(l, i)); return acc; }
fun range(n) { var r = list(); for (var i = 1; i <= n; i = i + 1) push(r, i); return r; }
fun sq(x) { return x * x; }
fun even(x) { return x % 2 == 0; }
fun add(a, b) { return a + b; }
print(reduce(map(range(5), sq), add, 0));
print(len(filter(range(10), even)));
print(str(map(range(4), sq)));"##,
            "55\n5\n[1, 4, 9, 16]\n",
            true,
        ),
        ok(
            "sieve",
            r##"var n = 30; var sieve = list();
for (var i = 0; i <= n; i = i + 1) push(sieve, true);
for (var i = 2; i * i <= n; i = i + 1) {
  if (get(sieve, i)) { for (var j = i * i; j <= n; j = j + i) set(sieve, j, false); }
}
var out = "";
for (var i = 2; i <= n; i = i + 1) { if (get(sieve, i)) out = out + str(i) + " "; }
print(out);"##,
            "2 3 5 7 11 13 17 19 23 29 \n",
            true,
        ),
        ok(
            "string building and escapes",
            r##"var s = "";
for (var i = 1; i <= 5; i = i + 1) { s = s + str(i * i); if (i < 5) s = s + ","; }
print(s); print(len(s));
var esc = "tab\there \"quoted\" \\ done";
print(esc); print(len(esc));"##,
            "1,4,9,16,25\n11\ntab\there \"quoted\" \\ done\n24\n",
            true,
        ),
        ok(
            "class hierarchy and super",
            r##"class Animal {
  init(name) { this.name = name; }
  speak() { return this.name + " makes a sound"; }
  describe() { return "Animal:" + this.name; }
}
class Dog < Animal {
  init(name, breed) { super.init(name); this.breed = breed; }
  speak() { return super.speak() + " (woof)"; }
}
var d = Dog("Rex", "lab");
print(d.speak()); print(d.describe()); print(d.breed); print(d); print(Dog);
var m = d.speak; print(m());"##,
            "Rex makes a sound (woof)\nAnimal:Rex\nlab\nDog instance\n<class Dog>\nRex makes a sound (woof)\n",
            false,
        ),
        ok(
            "bubble sort and quicksort",
            r##"fun bubble(l) {
  var n = len(l);
  for (var i = 0; i < n; i = i + 1) {
    for (var j = 0; j + 1 < n - i; j = j + 1) {
      var a = get(l, j); var b = get(l, j + 1);
      if (a > b) { set(l, j, b); set(l, j + 1, a); }
    }
  }
  return l;
}
fun qsort(l) {
  if (len(l) < 2) return l;
  var p = get(l, 0); var lo = list(); var hi = list();
  for (var i = 1; i < len(l); i = i + 1) { var x = get(l, i); if (x < p) push(lo, x); else push(hi, x); }
  var r = qsort(lo); push(r, p);
  var h = qsort(hi);
  for (var i = 0; i < len(h); i = i + 1) push(r, get(h, i));
  return r;
}
var data = list();
push(data, 5); push(data, 2); push(data, 9); push(data, 1); push(data, 7); push(data, 3); push(data, 2);
print(str(qsort(data)));
print(str(bubble(data)));"##,
            "[1, 2, 2, 3, 5, 7, 9]\n[1, 2, 2, 3, 5, 7, 9]\n",
            true,
        ),
        ok(
            "tiny JSON printer",
            r##"fun json(v) {
  var t = type(v);
  if (t == "list") {
    var s = "[";
    for (var i = 0; i < len(v); i = i + 1) { if (i > 0) s = s + ","; s = s + json(get(v, i)); }
    return s + "]";
  }
  if (t == "string") return "\"" + v + "\"";
  return str(v);
}
var inner = list(); push(inner, 2); push(inner, 3.5);
var doc = list(); push(doc, 1); push(doc, "a\nb"); push(doc, inner); push(doc, list()); push(doc, true); push(doc, nil);
print(json(doc));"##,
            "[1,\"a\nb\",[2,3.5],[],true,nil]\n",
            true,
        ),
        ok(
            "mutual recursion",
            r##"fun isEven(n) { if (n == 0) return true; return isOdd(n - 1); }
fun isOdd(n) { if (n == 0) return false; return isEven(n - 1); }
print(isEven(10)); print(isOdd(7)); print(isEven(7));"##,
            "true\ntrue\nfalse\n",
            true,
        ),
        ok(
            "loop closure capture",
            r##"var fs = list(); var gs = list();
for (var i = 0; i < 3; i = i + 1) {
  var j = i;
  fun f() { return j; }
  fun g() { return i; }
  push(fs, f); push(gs, g);
}
for (var k = 0; k < 3; k = k + 1) print(str(get(fs, k)()) + "/" + str(get(gs, k)()));"##,
            "0/3\n1/3\n2/3\n",
            true,
        ),
        ok(
            "break and continue",
            r##"var s = 0;
for (var i = 0; i < 10; i = i + 1) { if (i % 2 == 0) continue; if (i > 7) break; s = s + i; }
print(s);
var w = 0; while (true) { w = w + 1; if (w >= 5) break; } print(w);
var c = 0; var n = 0; while (n < 6) { n = n + 1; if (n == 3) continue; c = c + n; } print(c);"##,
            "16\n5\n18\n",
            true,
        ),
        ok(
            "natives and types",
            r##"print(type(nil)); print(type(true)); print(type(1)); print(type("s")); print(type(print)); print(type(list()));
fun f() {} print(type(f));
print(num("12") + 1); print(num("x")); print(len("héllo")); print(str(1.5) + "!");
assert(1 < 2, "ok"); print("assert ok");"##,
            "nil\nbool\nnumber\nstring\nnative\nlist\nfunction\n13\nnil\n5\n1.5!\nassert ok\n",
            true,
        ),
        ok(
            "short circuit",
            r##"var calls = 0;
fun boom() { calls = calls + 1; return true; }
print(false and boom()); print(true or boom()); print(nil or "default"); print(1 and 2); print(calls);
print(true and boom()); print(calls);"##,
            "false\ntrue\ndefault\n2\n0\ntrue\n1\n",
            true,
        ),
        ok(
            "static scoping (resolver)",
            r##"var a = "global";
{ fun show() { print(a); } show(); var a = "local"; show(); print(a); }"##,
            "global\nglobal\nlocal\n",
            true,
        ),
        ok(
            "bound methods, init, chains",
            r##"class Counter { init() { this.n = 0; } inc() { this.n = this.n + 1; return this; } get() { return this.n; } }
var c = Counter(); c.inc().inc().inc(); print(c.get());
var f = c.inc; f(); print(c.n);
print(c.init() == c);
class A { m() { return "A"; } }
class B < A { m() { return "B>" + super.m(); } }
class C < B { m() { return "C>" + super.m(); } }
print(C().m()); print(type(C)); print(type(C()));"##,
            "3\n4\ntrue\nC>B>A\nclass\ninstance\n",
            false,
        ),
        ok(
            "nested comments",
            "/* a /* nested */ still comment */ print(1); // trailing\n/* multi\nline */ print(2);",
            "1\n2\n",
            true,
        ),
        ok(
            "recursion depth 300",
            "fun sum(n) { if (n == 0) return 0; return n + sum(n - 1); } print(sum(300));",
            "45150\n",
            true,
        ),
        // ---- runtime errors: exact message + stack trace ----
        fails(
            "undefined variable",
            "print(1);\nprint(x);",
            "1\n",
            "Undefined variable 'x'.\n[line 2] in script",
            true,
        ),
        fails(
            "wrong arity",
            "fun f(a, b) {}\nf(1);",
            "",
            "Expected 2 arguments but got 1.\n[line 2] in script",
            true,
        ),
        fails(
            "call a non-function",
            "var x = 3;\nx();",
            "",
            "Can only call functions and classes.\n[line 2] in script",
            true,
        ),
        fails(
            "trace through frames",
            "fun inner() {\n  return missing;\n}\nfun outer() {\n  return inner();\n}\nouter();",
            "",
            "Undefined variable 'missing'.\n[line 2] in inner()\n[line 5] in outer()\n[line 7] in script",
            true,
        ),
        fails(
            "operand types",
            "print(1 + \"a\");",
            "",
            "Operands must be two numbers or two strings.\n[line 1] in script",
            true,
        ),
        fails(
            "negate a string",
            "print(-\"x\");",
            "",
            "Operand must be a number.\n[line 1] in script",
            true,
        ),
        fails(
            "compare number and string",
            "print(1 < \"a\");",
            "",
            "Operands must be numbers.\n[line 1] in script",
            true,
        ),
        fails(
            "division by zero",
            "var z = 0;\nprint(10 / z);",
            "",
            "Division by zero.\n[line 2] in script",
            true,
        ),
        fails(
            "assert failure",
            "assert(1 == 2, \"math\");",
            "",
            "Assertion failed: math\n[line 1] in script",
            true,
        ),
        fails(
            "list index",
            "var l = list();\nprint(get(l, 0));",
            "",
            "Index out of range.\n[line 2] in script",
            true,
        ),
        fails(
            "native arity",
            "len();",
            "",
            "Expected 1 arguments but got 0.\n[line 1] in script",
            true,
        ),
        fails(
            "assign to undefined global",
            "x = 1;",
            "",
            "Undefined variable 'x'.\n[line 1] in script",
            true,
        ),
        fails(
            "property on non-instance",
            "var x = 1;\nprint(x.y);",
            "",
            "Only instances have properties.\n[line 2] in script",
            false,
        ),
        fails(
            "undefined property",
            "class A {}\nprint(A().z);",
            "",
            "Undefined property 'z'.\n[line 2] in script",
            false,
        ),
        fails(
            "superclass not a class",
            "var A = 1;\nclass B < A {}",
            "",
            "Superclass must be a class.\n[line 2] in script",
            false,
        ),
        fails(
            "class constructor arity",
            "class A { init(x) {} }\nA();",
            "",
            "Expected 1 arguments but got 0.\n[line 2] in script",
            false,
        ),
    ]
}

/// (source, expected diagnostics as one-liners). Errors are found statically: nothing runs.
fn compile_error_cases() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "{ var a = a; }",
            vec!["[line 1:11] Error at 'a': Can't read local variable in its own initializer."],
        ),
        (
            "return 1;",
            vec!["[line 1:1] Error at 'return': Can't return from top-level code."],
        ),
        (
            "print(this);",
            vec!["[line 1:7] Error at 'this': Can't use 'this' outside of a class."],
        ),
        (
            "{ var a = 1; var a = 2; }",
            vec!["[line 1:18] Error at 'a': Already a variable with this name in this scope."],
        ),
        (
            "break;",
            vec!["[line 1:1] Error at 'break': Can't use 'break' outside of a loop."],
        ),
        (
            "fun f() { break; }\nwhile (true) { f(); break; }",
            vec!["[line 1:11] Error at 'break': Can't use 'break' outside of a loop."],
        ),
        (
            "1 + 2 = 3;",
            vec!["[line 1:7] Error at '=': Invalid assignment target."],
        ),
        (
            "class A < A {}",
            vec!["[line 1:11] Error at 'A': A class can't inherit from itself."],
        ),
        (
            "super.x();",
            vec!["[line 1:1] Error at 'super': Can't use 'super' outside of a class."],
        ),
        (
            "class A { m() { return super.m(); } }",
            vec!["[line 1:24] Error at 'super': Can't use 'super' in a class with no superclass."],
        ),
        (
            "class A { init() { return 1; } }",
            vec!["[line 1:20] Error at 'return': Can't return a value from an initializer."],
        ),
        (
            "/* open",
            vec!["[line 1:1] Error: Unterminated block comment."],
        ),
        (
            "var $x = 1;",
            vec!["[line 1:5] Error: Unexpected character '$'."],
        ),
        // panic-mode recovery: three independent syntax errors are all reported in one run
        (
            "var = 1;\nprint(;\nfun (){}\nprint(2);",
            vec![
                "[line 1:5] Error at '=': Expect variable name.",
                "[line 2:7] Error at ';': Expect expression.",
                "[line 3:5] Error at '(': Expect function name.",
            ],
        ),
    ]
}

// =====================================================================================================================
// Random programs for differential testing (tree-walk vs bytecode)
// =====================================================================================================================
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

#[derive(Default)]
struct Scope {
    vars: Vec<String>, // assignable numeric variables
    ro: Vec<String>,   // loop counters: readable, never assigned (keeps loops terminating)
    funcs: Vec<(String, usize)>,
}
struct Gen {
    r: Rng,
    scopes: Vec<Scope>,
    n: usize,
    out: String,
}

impl Gen {
    fn fresh(&mut self, p: &str) -> String {
        self.n += 1;
        format!("{p}{}", self.n)
    }
    fn readable(&self) -> Vec<String> {
        self.scopes
            .iter()
            .flat_map(|s| s.vars.iter().chain(&s.ro))
            .cloned()
            .collect()
    }
    fn assignable(&self) -> Vec<String> {
        self.scopes
            .iter()
            .flat_map(|s| s.vars.iter())
            .cloned()
            .collect()
    }
    fn funcs(&self) -> Vec<(String, usize)> {
        self.scopes
            .iter()
            .flat_map(|s| s.funcs.iter())
            .cloned()
            .collect()
    }
    fn num(&mut self, d: u32) -> String {
        let vars = self.readable();
        let leaf = |g: &mut Gen| {
            if !vars.is_empty() && g.r.below(3) > 0 {
                vars[g.r.below(vars.len())].clone()
            } else {
                ["0", "1", "2", "3", "7", "2.5", "10"][g.r.below(7)].to_string()
            }
        };
        if d == 0 {
            return leaf(self);
        }
        match self.r.below(10) {
            0..=3 => {
                let (a, b) = (self.num(d - 1), self.num(d - 1));
                format!("({a} {} {b})", ["+", "-", "*"][self.r.below(3)])
            }
            4 => {
                let a = self.num(d - 1);
                // mostly a literal divisor; sometimes a computed one, which can be zero and exercise the error path
                let div = if self.r.below(4) == 0 {
                    self.num(d - 1)
                } else {
                    (1 + self.r.below(4)).to_string()
                };
                format!("({a} {} {div})", ["/", "%"][self.r.below(2)])
            }
            5 => match self.funcs() {
                fs if !fs.is_empty() => {
                    let (name, arity) = fs[self.r.below(fs.len())].clone();
                    let args: Vec<String> = (0..arity).map(|_| self.num(d - 1)).collect();
                    format!("{name}({})", args.join(", "))
                }
                _ => leaf(self),
            },
            6 => {
                let (c, a, b) = (self.cond(d - 1), self.num(d - 1), self.num(d - 1));
                format!("({c} and {a} or {b})") // numbers are truthy, so this is a ternary
            }
            7 => format!("(-{})", self.num(d - 1)),
            _ => leaf(self),
        }
    }
    fn cond(&mut self, d: u32) -> String {
        match if d == 0 { 0 } else { self.r.below(5) } {
            0..=2 => {
                let (a, b) = (self.num(d.saturating_sub(1)), self.num(d.saturating_sub(1)));
                format!(
                    "({a} {} {b})",
                    ["<", "<=", ">", ">=", "==", "!="][self.r.below(6)]
                )
            }
            3 => {
                let (a, b) = (self.cond(d - 1), self.cond(d - 1));
                format!("({a} {} {b})", ["and", "or"][self.r.below(2)])
            }
            _ => format!("(!{})", self.cond(d - 1)),
        }
    }
    fn block(&mut self, n: usize, d: u32, in_loop: bool, ro: &[String], vars: &[String]) {
        self.out.push_str("{\n");
        self.scopes.push(Scope {
            vars: vars.to_vec(),
            ro: ro.to_vec(),
            funcs: vec![],
        });
        for _ in 0..1 + self.r.below(n) {
            self.stmt(d, in_loop);
        }
        self.scopes.pop();
        self.out.push_str("}\n");
    }
    fn stmt(&mut self, d: u32, in_loop: bool) {
        let k = if in_loop && self.r.below(9) == 0 {
            99
        } else if d == 0 {
            self.r.below(7)
        } else {
            self.r.below(14)
        };
        match k {
            0..=2 => {
                let (e, v) = (self.num(2), self.fresh("v"));
                let _ = writeln!(self.out, "var {v} = {e};");
                if let Some(s) = self.scopes.last_mut() {
                    s.vars.push(v);
                }
            }
            3 | 4 if !self.assignable().is_empty() => {
                let vs = self.assignable();
                let v = vs[self.r.below(vs.len())].clone();
                let e = self.num(2);
                let _ = writeln!(self.out, "{v} = {e};");
            }
            3..=6 => match self.r.below(3) {
                0 => {
                    let e = self.num(3);
                    let _ = writeln!(self.out, "print({e});");
                }
                1 => {
                    let c = self.cond(2);
                    let _ = writeln!(self.out, "print({c});");
                }
                _ => {
                    let e = self.num(2);
                    let _ = writeln!(self.out, "print(\"s\" + str({e}));");
                }
            },
            7 => {
                let c = self.cond(2);
                let _ = writeln!(self.out, "if {c} ");
                self.block(2, d - 1, in_loop, &[], &[]);
                if self.r.below(2) == 0 {
                    self.out.push_str("else ");
                    self.block(2, d - 1, in_loop, &[], &[]);
                }
            }
            8 => self.block(3, d - 1, in_loop, &[], &[]),
            9 => {
                let (i, bound) = (self.fresh("i"), 1 + self.r.below(5));
                let _ = writeln!(self.out, "for (var {i} = 0; {i} < {bound}; {i} = {i} + 1)");
                self.block(3, d - 1, true, &[i], &[]);
            }
            10 => {
                let (w, bound) = (self.fresh("w"), 1 + self.r.below(5));
                let _ = writeln!(self.out, "var {w} = 0;");
                let _ = writeln!(self.out, "while ({w} < {bound})");
                // the counter is bumped FIRST so `continue` can never skip it
                self.out.push_str(&format!("{{\n{w} = {w} + 1;\n"));
                self.scopes.push(Scope {
                    vars: vec![],
                    ro: vec![w.clone()],
                    funcs: vec![],
                });
                for _ in 0..1 + self.r.below(3) {
                    self.stmt(d - 1, true);
                }
                self.scopes.pop();
                self.out.push_str("}\n");
            }
            11 => {
                let (name, arity) = (self.fresh("f"), self.r.below(3));
                let params: Vec<String> = (0..arity).map(|_| self.fresh("p")).collect();
                let _ = writeln!(self.out, "fun {name}({}) {{", params.join(", "));
                self.scopes.push(Scope {
                    vars: params,
                    ro: vec![],
                    funcs: vec![],
                });
                for _ in 0..self.r.below(3) {
                    self.stmt(d - 1, false);
                }
                if self.r.below(3) == 0 {
                    let (c, e) = (self.cond(1), self.num(1));
                    let _ = writeln!(self.out, "if {c} return {e};");
                }
                let e = self.num(2);
                let _ = writeln!(self.out, "return {e};\n}}");
                self.scopes.pop();
                if let Some(s) = self.scopes.last_mut() {
                    s.funcs.push((name, arity));
                }
            }
            12 => {
                let (mk, g, step) = (self.fresh("mk"), self.fresh("g"), self.r.below(4) + 1);
                let _ = writeln!(
                    self.out,
                    "fun {mk}() {{ var c = 0; fun inc() {{ c = c + {step}; return c; }} return inc; }}"
                );
                let _ = writeln!(
                    self.out,
                    "var {g} = {mk}(); print({g}()); print({g}()); print({g}());"
                );
            }
            99 => {
                let c = self.cond(1);
                let kw = if self.r.below(2) == 0 {
                    "break"
                } else {
                    "continue"
                };
                let _ = writeln!(self.out, "if {c} {kw};");
            }
            _ => {
                let (l, a, b) = (self.fresh("L"), self.num(1), self.num(1));
                let _ = writeln!(
                    self.out,
                    "var {l} = list(); push({l}, {a}); push({l}, {b}); print(len({l})); print(get({l}, 1)); print(str({l}));"
                );
            }
        }
    }
}

fn gen_program(seed: u64) -> String {
    let mut g = Gen {
        r: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
        scopes: vec![Scope::default()],
        n: 0,
        out: String::new(),
    };
    for _ in 0..4 + g.r.below(6) {
        g.stmt(3, false);
    }
    g.out
}

// =====================================================================================================================
// Checks and demo
// =====================================================================================================================
fn run_suite() -> (usize, usize) {
    let (mut tree_runs, mut vm_runs) = (0, 0);
    for c in suite() {
        let t = run_source(c.src, Backend::Tree, DEFAULT_FUEL);
        assert!(
            t.compile_errors.is_empty(),
            "{}: unexpected compile errors {:?}",
            c.name,
            t.compile_errors
        );
        assert_eq!(t.out, c.out, "{} (tree-walk): output", c.name);
        assert_eq!(t.error.as_deref(), c.err, "{} (tree-walk): error", c.name);
        tree_runs += 1;
        if c.vm {
            let v = run_source(c.src, Backend::Vm, DEFAULT_FUEL);
            assert!(!v.unsupported, "{}: VM should support this program", c.name);
            assert_eq!(v.out, c.out, "{} (bytecode): output", c.name);
            assert_eq!(v.error.as_deref(), c.err, "{} (bytecode): error", c.name);
            vm_runs += 1;
        } else {
            assert!(
                run_source(c.src, Backend::Vm, DEFAULT_FUEL).unsupported,
                "{}: classes must be reported as unsupported by the VM",
                c.name
            );
        }
    }
    (tree_runs, vm_runs)
}

fn check_static_errors() {
    for (src, want) in compile_error_cases() {
        let got = run_source(src, Backend::Tree, DEFAULT_FUEL);
        assert_eq!(got.compile_errors, want, "diagnostics for {src:?}");
        assert!(
            got.out.is_empty() && got.error.is_none(),
            "nothing may run when there are compile errors: {src:?}"
        );
    }
    // unterminated string: the scanner error comes first (a follow-on parse error is allowed)
    let o = run_source("print(\"abc);", Backend::Tree, DEFAULT_FUEL);
    assert_eq!(
        o.compile_errors[0],
        "[line 1:7] Error: Unterminated string."
    );
    // warnings do not stop execution
    let o = run_source(
        "{ var unused = 1; var used = 2; print(used); }",
        Backend::Tree,
        DEFAULT_FUEL,
    );
    assert_eq!(
        o.warnings,
        ["[line 1:7] Warning at 'unused': Local variable is never used."]
    );
    assert_eq!(o.out, "2\n");
    let o = run_source(
        "fun f(a) { var x = 1; return a; }\nprint(f(1));",
        Backend::Vm,
        DEFAULT_FUEL,
    );
    assert_eq!(
        o.warnings,
        ["[line 1:16] Warning at 'x': Local variable is never used."]
    );
    assert_eq!(o.out, "1\n");
}

fn check_limits() {
    for be in [Backend::Tree, Backend::Vm] {
        let o = run_source("fun f() { return f(); }\nf();", be, DEFAULT_FUEL);
        let e = o.error.unwrap_or_default();
        assert!(
            e.starts_with("Stack overflow.\n[line 1] in f()"),
            "{be:?}: {}",
            e.lines().next().unwrap_or("")
        );
        assert_eq!(
            e.lines().count(),
            MAX_CALL_DEPTH + 2,
            "{be:?}: trace has every frame"
        );
        let o = run_source("var n = 0;\nwhile (true) { n = n + 1; }", be, 10_000);
        assert!(
            o.error
                .unwrap_or_default()
                .starts_with("Fuel exhausted: step limit (10000) exceeded."),
            "{be:?}"
        );
        // a program that fits in the budget is untouched
        let o = run_source(
            "var n = 0; while (n < 100) n = n + 1; print(n);",
            be,
            100_000,
        );
        assert_eq!((o.out.as_str(), o.error), ("100\n", None));
    }
}

fn check_differential(n: u64) -> (usize, usize, usize) {
    let (mut with_output, mut with_error, mut lines) = (0, 0, 0);
    for seed in 1..=n {
        let src = gen_program(seed);
        let t = run_source(&src, Backend::Tree, 50_000_000);
        assert!(
            t.compile_errors.is_empty(),
            "generated program {seed} must be valid: {:?}\n{src}",
            t.compile_errors
        );
        let v = run_source(&src, Backend::Vm, 50_000_000);
        assert!(!v.unsupported, "program {seed} uses no classes");
        assert_eq!(
            t.out, v.out,
            "output differs for generated program {seed}:\n{src}"
        );
        assert_eq!(
            t.error, v.error,
            "error differs for generated program {seed}:\n{src}"
        );
        lines += src.lines().count();
        if v.error.is_some() {
            with_error += 1
        } else if !v.out.is_empty() {
            with_output += 1
        }
    }
    (with_output, with_error, lines)
}

fn show_script(title: &str, src: &str, be: Backend) {
    println!("--- {title} ---");
    for (i, l) in src.lines().enumerate() {
        println!("{:>3} | {l}", i + 1);
    }
    let o = run_source(src, be, DEFAULT_FUEL);
    print!(
        "{}",
        o.out
            .lines()
            .map(|l| format!("  => {l}\n"))
            .collect::<String>()
    );
    if let Some(e) = &o.error {
        println!(
            "  runtime error: {}",
            e.replace('\n', "\n                 ")
        );
    }
}

fn timing() {
    let src =
        "fun fib(n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }\nprint(fib(27));";
    let mut times = vec![];
    for be in [Backend::Tree, Backend::Vm] {
        let t0 = Instant::now();
        let o = run_source(src, be, DEFAULT_FUEL);
        assert_eq!((o.out.as_str(), o.error), ("196418\n", None), "{be:?}");
        times.push(t0.elapsed());
    }
    println!(
        "fib(27) = 196418: tree-walk {:.0?}, bytecode VM {:.0?} ({:.1}x)   [printed only, not asserted]",
        times[0],
        times[1],
        times[0].as_secs_f64() / times[1].as_secs_f64().max(1e-9)
    );
}

fn run_all() {
    let t0 = Instant::now();
    let (tree_runs, vm_runs) = run_suite();
    println!(
        "hand-written suite: {tree_runs} programs on the tree-walker, {vm_runs} on the bytecode VM, exact output and exact errors/stack traces"
    );
    check_static_errors();
    println!(
        "static diagnostics: {} cases (resolver rules, panic-mode recovery reports 3 syntax errors at once, warnings)",
        compile_error_cases().len() + 4
    );
    check_limits();
    println!(
        "limits: call depth {MAX_CALL_DEPTH} -> 'Stack overflow.' with a {}-frame trace, fuel limit stops runaway loops (both back ends)",
        MAX_CALL_DEPTH + 1
    );
    let (ok_n, err_n, lines) = check_differential(300);
    println!(
        "differential: 300 generated programs ({lines} lines) give identical output and errors on both back ends ({ok_n} printed output, {err_n} ended in the same runtime error)"
    );

    println!();
    show_script(
        "sample 1: closures and recursion (tree-walk)",
        "fun makeCounter() {\n  var n = 0;\n  fun tick() { n = n + 1; return n; }\n  return tick;\n}\nvar c = makeCounter();\nprint(\"count: \" + str(c()) + \", \" + str(c()) + \", \" + str(c()));\nfun fib(n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }\nprint(\"fib(15) = \" + str(fib(15)));",
        Backend::Tree,
    );
    let class_src = "class Account {\n  init(owner, balance) { this.owner = owner; this.balance = balance; }\n  withdraw(n) {\n    if (n > this.balance) return this.fail(n);\n    this.balance = this.balance - n;\n    return this.balance;\n  }\n}\nvar a = Account(\"ann\", 100);\nprint(a.withdraw(30));\nprint(a.withdraw(500));";
    show_script(
        "sample 2: classes and a runtime error with a stack trace (tree-walk)",
        class_src,
        Backend::Tree,
    );
    let o = run_source(class_src, Backend::Tree, DEFAULT_FUEL);
    assert_eq!(o.out, "70\n");
    assert_eq!(
        o.error.as_deref(),
        Some("Undefined property 'fail'.\n[line 4] in withdraw()\n[line 11] in script")
    );
    let bad = "var greeting = \"hi\";\nprint(greeting + );\nfun 42() {}\nvar ok = 1;";
    println!("--- sample 3: syntax errors with carets (panic-mode recovery reports both) ---");
    if let Err(diags) = front_end(bad) {
        assert_eq!(diags.len(), 2);
        for d in &diags {
            println!("{}", d.render(bad));
        }
    }
    println!();
    timing();
    println!("all checks passed in {:.2?}", t0.elapsed());
}

fn main() {
    // Tree-walking recursion uses several Rust frames per script call, so give the interpreter a roomy stack.
    let h = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(run_all)
        .expect("spawn interpreter thread");
    if h.join().is_err() {
        std::process::exit(1);
    }
}
