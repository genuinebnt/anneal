//! RECIPE 423 (compilers): Hindley-Milner type inference with let-polymorphism, level-based generalisation and source-span errors.
//! LEVEL: advanced
//! Use in: language implementations, DSL type checkers, config/query languages with inferred types, and understanding what rustc's inference does and does not do.   Run: cargo run --release --example hindley_milner_inference
#![allow(dead_code)]
//!
//! THE IDEA: give every unknown a type variable, walk the term emitting equations, solve them by UNIFICATION (structural
//! matching that binds variables) and, at each `let`, GENERALISE the bound expression's type: variables that are not tied
//! to the surrounding context become `forall`-bound, so `let id = \x. x in (id 1, id true)` works while the lambda-bound
//! `(\id. (id 1, id true)) (\x. x)` does not (a lambda parameter has ONE monomorphic type). The result is the PRINCIPAL type:
//! the most general one, from which every other valid type is an instance.
//! HOW (the efficient "levels" variant of Algorithm W, as in OCaml): each type variable carries the let-nesting LEVEL at which
//! it was created. Entering a `let` right-hand side bumps the level; afterwards any variable still at a deeper level than the
//! current one cannot be reachable from the environment, so it is generalised. Unifying a variable with a type lowers the
//! levels of all variables inside that type to the variable's own level (this is what stops `\y. let f = \x. y in f` from
//! generalising y's type). The bindings live in a vector (union-find style), so no substitution is ever composed or applied.
//! OCCURS CHECK: binding `a := a -> b` would create an infinite type; that is exactly why `\x. x x` and the Y combinator are
//! rejected, and why recursion needs `let rec` (typed with the name monomorphic inside its own body) instead.
//! LANGUAGE: ints, bools, lambda, application, let, let rec, if, pairs `(a, b)`, lists `[]` / `::`, a few primitives
//! (fst snd uncons not + - * < ==). `uncons xs nil_case (\h t. cons_case)` is a total list eliminator, so well-typed programs
//! never fail at runtime (no `head []`): the soundness test below needs "well-typed programs do not go wrong" to be literally true.
//! HOW RUST DIFFERS: rustc does not generalise local bindings: `let id = |x| x; id(1); id(true)` fails because a closure is
//! monomorphic (only items, i.e. `fn`s with explicit generics, are polymorphic). Its inference is BIDIRECTIONAL and
//! order-dependent: expected types flow inward (a closure's parameter type is read off the `Fn` bound it is passed to), method
//! calls need the receiver type known at that point ("type annotations needed"), and unification variables carry pending TRAIT
//! OBLIGATIONS (`?T: Add<?U>`) solved as information arrives, with integer-literal fallback to i32. Subtyping only exists for
//! lifetimes. HM, by contrast, is complete and needs no annotations at all, at the price of no overloading or subtyping.
//! COMMON USES: ML, OCaml, Haskell (as the core of a richer system), Elm and PureScript inference; typed configuration and query
//! DSLs; type checkers for spreadsheet-like formula languages; teaching how generics are checked.
//! Read: Damas and Milner "Principal type-schemes for functional programs" (POPL 1982); Pierce "Types and Programming Languages" ch. 22 https://www.cis.upenn.edu/~bcpierce/tapl/ ;
//! https://okmij.org/ftp/ML/generalization.html (levels) and https://en.wikipedia.org/wiki/Hindley%E2%80%93Milner_type_system
use std::collections::HashMap;
use std::rc::Rc;

// ---------------------------------------------------------------- types

#[derive(Clone, Debug, PartialEq)]
enum Ty {
    Var(usize),   // unification variable
    Gen(usize),   // quantified variable inside a Scheme
    Rigid(usize), // opaque named type (only used by the test generator: behaves like a constant)
    Int,
    Bool,
    Fun(Box<Ty>, Box<Ty>),
    Pair(Box<Ty>, Box<Ty>),
    List(Box<Ty>),
}
fn fun(a: Ty, b: Ty) -> Ty {
    Ty::Fun(Box::new(a), Box::new(b))
}
fn pair(a: Ty, b: Ty) -> Ty {
    Ty::Pair(Box::new(a), Box::new(b))
}
fn list(a: Ty) -> Ty {
    Ty::List(Box::new(a))
}

#[derive(Clone, Debug)]
struct Scheme {
    n: usize,
    ty: Ty,
}

fn letter(i: usize) -> String {
    let c = (b'a' + (i % 26) as u8) as char;
    if i < 26 {
        c.to_string()
    } else {
        format!("{c}{}", i / 26)
    }
}

/// Names for unresolved variables, assigned by first appearance so messages read `a`, `b`, ... not `t4711`.
#[derive(Default)]
struct Names(HashMap<usize, usize>);

fn show_ty(t: &Ty, names: &mut Names, prec: u8) -> String {
    match t {
        Ty::Var(v) => {
            let next = names.0.len();
            letter(*names.0.entry(*v).or_insert(next))
        }
        Ty::Gen(i) | Ty::Rigid(i) => letter(*i),
        Ty::Int => "int".into(),
        Ty::Bool => "bool".into(),
        Ty::Pair(a, b) => format!("({}, {})", show_ty(a, names, 0), show_ty(b, names, 0)),
        Ty::List(a) => format!("[{}]", show_ty(a, names, 0)),
        Ty::Fun(a, b) => {
            let s = format!("{} -> {}", show_ty(a, names, 1), show_ty(b, names, 0));
            if prec > 0 { format!("({s})") } else { s }
        }
    }
}
fn show_scheme(s: &Scheme) -> String {
    let body = show_ty(&s.ty, &mut Names::default(), 0);
    if s.n == 0 {
        body
    } else {
        let vars: Vec<String> = (0..s.n).map(letter).collect();
        format!("forall {}. {body}", vars.join(" "))
    }
}

// ---------------------------------------------------------------- syntax, lexer, parser

#[derive(Clone, Debug)]
struct Expr {
    k: K,
    lo: usize,
    hi: usize,
}
#[derive(Clone, Debug)]
enum K {
    Int(i64),
    Bool(bool),
    Nil,
    Var(String),
    Lam(String, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    LetRec(String, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Pair(Box<Expr>, Box<Expr>),
}
fn mk(k: K) -> Expr {
    Expr { k, lo: 0, hi: 0 }
}

#[derive(Debug, Clone)]
struct TypeError {
    msg: String,
    lo: usize,
    hi: usize,
}
type Res<T> = Result<T, TypeError>;
fn err<T>(msg: impl Into<String>, lo: usize, hi: usize) -> Res<T> {
    Err(TypeError {
        msg: msg.into(),
        lo,
        hi,
    })
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Int(i64),
    Id(String),
    Sym(&'static str),
    Eof,
}
const SYMS: [&str; 15] = [
    "::", "==", "[]", "\\", ".", "=", "(", ")", ",", "+", "-", "*", "<", "[", "]",
];
const KEYWORDS: [&str; 8] = ["let", "rec", "in", "if", "then", "else", "true", "false"];

fn lex(src: &str) -> Res<Vec<(Tok, usize, usize)>> {
    let b = src.as_bytes();
    let (mut i, mut out) = (0, vec![]);
    while i < b.len() {
        let ch = b[i] as char;
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() {
            let s = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push((Tok::Int(src[s..i].parse().unwrap_or(i64::MAX)), s, i));
        } else if ch.is_ascii_alphabetic() || ch == '_' {
            let s = i;
            while i < b.len()
                && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'\'')
            {
                i += 1;
            }
            out.push((Tok::Id(src[s..i].to_string()), s, i));
        } else if let Some(s) = SYMS.iter().find(|s| src[i..].starts_with(**s)) {
            out.push((Tok::Sym(s), i, i + s.len()));
            i += s.len();
        } else {
            return err(format!("unexpected character `{ch}`"), i, i + 1);
        }
    }
    out.push((Tok::Eof, src.len(), src.len()));
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize, usize)>,
    pos: usize,
}
impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].0
    }
    fn span(&self) -> (usize, usize) {
        (self.toks[self.pos].1, self.toks[self.pos].2)
    }
    fn prev_hi(&self) -> usize {
        self.toks[self.pos.saturating_sub(1)].2
    }
    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if *x == s)
    }
    fn is_kw(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Id(x) if x == s)
    }
    fn eat(&mut self, s: &str) -> bool {
        let hit = self.is_sym(s) || self.is_kw(s);
        if hit {
            self.pos += 1;
        }
        hit
    }
    fn expect(&mut self, s: &str) -> Res<()> {
        if self.eat(s) {
            Ok(())
        } else {
            let (lo, hi) = self.span();
            err(format!("parse error: expected `{s}`"), lo, hi)
        }
    }
    fn ident(&mut self) -> Res<String> {
        match self.peek().clone() {
            Tok::Id(x) if !KEYWORDS.contains(&x.as_str()) => {
                self.pos += 1;
                Ok(x)
            }
            _ => {
                let (lo, hi) = self.span();
                err("parse error: expected an identifier", lo, hi)
            }
        }
    }
    fn expr(&mut self) -> Res<Expr> {
        let lo = self.span().0;
        if self.eat("\\") {
            let mut params = vec![self.ident()?];
            while !self.is_sym(".") {
                params.push(self.ident()?);
            }
            self.expect(".")?;
            let mut body = self.expr()?;
            for p in params.into_iter().rev() {
                body = Expr {
                    lo,
                    hi: body.hi,
                    k: K::Lam(p, Box::new(body)),
                };
            }
            Ok(body)
        } else if self.eat("let") {
            let rec = self.eat("rec");
            let name = self.ident()?;
            self.expect("=")?;
            let rhs = Box::new(self.expr()?);
            self.expect("in")?;
            let body = self.expr()?;
            let hi = body.hi;
            let (name, body) = (name, Box::new(body));
            Ok(Expr {
                lo,
                hi,
                k: if rec {
                    K::LetRec(name, rhs, body)
                } else {
                    K::Let(name, rhs, body)
                },
            })
        } else if self.eat("if") {
            let c = self.expr()?;
            self.expect("then")?;
            let t = self.expr()?;
            self.expect("else")?;
            let e = self.expr()?;
            Ok(Expr {
                lo,
                hi: e.hi,
                k: K::If(Box::new(c), Box::new(t), Box::new(e)),
            })
        } else {
            self.cmp()
        }
    }
    fn binary(&mut self, op: &'static str, l: Expr, r: Expr, op_lo: usize, op_hi: usize) -> Expr {
        let f = Expr {
            lo: op_lo,
            hi: op_hi,
            k: K::Var(op.into()),
        };
        let inner = Expr {
            lo: l.lo,
            hi: op_hi,
            k: K::App(Box::new(f), Box::new(l.clone())),
        };
        Expr {
            lo: l.lo,
            hi: r.hi,
            k: K::App(Box::new(inner), Box::new(r)),
        }
    }
    fn cmp(&mut self) -> Res<Expr> {
        let l = self.cons()?;
        for op in ["<", "=="] {
            if self.is_sym(op) {
                let (lo, hi) = self.span();
                self.pos += 1;
                let r = self.cons()?;
                return Ok(self.binary(if op == "<" { "<" } else { "==" }, l, r, lo, hi));
            }
        }
        Ok(l)
    }
    fn cons(&mut self) -> Res<Expr> {
        let l = self.add()?;
        if self.is_sym("::") {
            let (lo, hi) = self.span();
            self.pos += 1;
            let r = self.cons()?; // right associative
            return Ok(self.binary("::", l, r, lo, hi));
        }
        Ok(l)
    }
    fn add(&mut self) -> Res<Expr> {
        let mut l = self.mul()?;
        loop {
            let op = if self.is_sym("+") {
                "+"
            } else if self.is_sym("-") {
                "-"
            } else {
                return Ok(l);
            };
            let (lo, hi) = self.span();
            self.pos += 1;
            let r = self.mul()?;
            l = self.binary(op, l, r, lo, hi);
        }
    }
    fn mul(&mut self) -> Res<Expr> {
        let mut l = self.app()?;
        while self.is_sym("*") {
            let (lo, hi) = self.span();
            self.pos += 1;
            let r = self.app()?;
            l = self.binary("*", l, r, lo, hi);
        }
        Ok(l)
    }
    fn starts_atom(&self) -> bool {
        match self.peek() {
            Tok::Int(_) => true,
            Tok::Id(x) => !["let", "rec", "in", "if", "then", "else"].contains(&x.as_str()),
            Tok::Sym(s) => ["(", "[]"].contains(s),
            Tok::Eof => false,
        }
    }
    fn app(&mut self) -> Res<Expr> {
        let mut f = self.atom()?;
        while self.starts_atom() {
            let a = self.atom()?;
            f = Expr {
                lo: f.lo,
                hi: a.hi,
                k: K::App(Box::new(f), Box::new(a)),
            };
        }
        Ok(f)
    }
    fn atom(&mut self) -> Res<Expr> {
        let (lo, hi) = self.span();
        let mk = |k| Expr { k, lo, hi };
        match self.peek().clone() {
            Tok::Int(n) => {
                self.pos += 1;
                Ok(mk(K::Int(n)))
            }
            Tok::Id(x) if x == "true" || x == "false" => {
                self.pos += 1;
                Ok(mk(K::Bool(x == "true")))
            }
            Tok::Id(_) => Ok(mk(K::Var(self.ident()?))),
            Tok::Sym("[]") => {
                self.pos += 1;
                Ok(mk(K::Nil))
            }
            Tok::Sym("(") => {
                self.pos += 1;
                let a = self.expr()?;
                let inner = if self.eat(",") {
                    let b = self.expr()?;
                    self.expect(")")?;
                    Expr {
                        lo,
                        hi: self.prev_hi(),
                        k: K::Pair(Box::new(a), Box::new(b)),
                    }
                } else {
                    self.expect(")")?;
                    a
                };
                Ok(inner)
            }
            _ => err("parse error: expected an expression", lo, hi.max(lo + 1)),
        }
    }
}
fn parse(src: &str) -> Res<Expr> {
    let mut p = Parser {
        toks: lex(src)?,
        pos: 0,
    };
    let e = p.expr()?;
    if *p.peek() != Tok::Eof {
        let (lo, hi) = p.span();
        return err("parse error: unexpected trailing input", lo, hi);
    }
    Ok(e)
}

/// Prints an AST back to source, fully parenthesised (used to feed generated terms through the real parser).
fn src(e: &Expr) -> String {
    match &e.k {
        K::Int(n) => n.to_string(),
        K::Bool(b) => b.to_string(),
        K::Nil => "[]".into(),
        K::Var(x) => x.clone(),
        K::Lam(x, b) => format!("(\\{x}. {})", src(b)),
        K::App(f, a) => {
            if let K::App(op, l) = &f.k {
                if let K::Var(o) = &op.k {
                    if ["+", "-", "*", "<", "==", "::"].contains(&o.as_str()) {
                        return format!("({} {o} {})", src(l), src(a));
                    }
                }
            }
            format!("({} {})", src(f), src(a))
        }
        K::Let(x, a, b) => format!("(let {x} = {} in {})", src(a), src(b)),
        K::LetRec(x, a, b) => format!("(let rec {x} = {} in {})", src(a), src(b)),
        K::If(c, t, f) => format!("(if {} then {} else {})", src(c), src(t), src(f)),
        K::Pair(a, b) => format!("({}, {})", src(a), src(b)),
    }
}

// ---------------------------------------------------------------- inference

enum UErr {
    Mismatch(Ty, Ty),
    Occurs(Ty, Ty),
}

struct Infer {
    binding: Vec<Option<Ty>>, // type variable -> its solution
    level: Vec<usize>,        // type variable -> let-nesting level
    cur: usize,
    env: Vec<(String, Scheme)>,
    unifications: usize,
}

impl Infer {
    fn new() -> Self {
        let (a, b) = (Ty::Gen(0), Ty::Gen(1));
        let ints = |r: Ty| fun(Ty::Int, fun(Ty::Int, r));
        let prims = vec![
            ("fst", 2, fun(pair(a.clone(), b.clone()), a.clone())),
            ("snd", 2, fun(pair(a.clone(), b.clone()), b.clone())),
            (
                "uncons",
                2,
                fun(
                    list(a.clone()),
                    fun(
                        b.clone(),
                        fun(fun(a.clone(), fun(list(a.clone()), b.clone())), b.clone()),
                    ),
                ),
            ),
            ("not", 0, fun(Ty::Bool, Ty::Bool)),
            ("+", 0, ints(Ty::Int)),
            ("-", 0, ints(Ty::Int)),
            ("*", 0, ints(Ty::Int)),
            ("<", 0, ints(Ty::Bool)),
            ("==", 0, ints(Ty::Bool)),
            ("::", 1, fun(a.clone(), fun(list(a.clone()), list(a)))),
        ];
        let env = prims
            .into_iter()
            .map(|(n, k, ty)| (n.to_string(), Scheme { n: k, ty }))
            .collect();
        Infer {
            binding: vec![],
            level: vec![],
            cur: 0,
            env,
            unifications: 0,
        }
    }
    fn fresh(&mut self) -> Ty {
        self.binding.push(None);
        self.level.push(self.cur);
        Ty::Var(self.binding.len() - 1)
    }
    fn prune(&self, t: &Ty) -> Ty {
        let mut t = t.clone();
        while let Ty::Var(v) = t {
            match &self.binding[v] {
                Some(n) => t = n.clone(),
                None => break,
            }
        }
        t
    }
    fn zonk(&self, t: &Ty) -> Ty {
        match self.prune(t) {
            Ty::Fun(a, b) => fun(self.zonk(&a), self.zonk(&b)),
            Ty::Pair(a, b) => pair(self.zonk(&a), self.zonk(&b)),
            Ty::List(a) => list(self.zonk(&a)),
            other => other,
        }
    }
    /// The occurs check and the level adjustment are one traversal: lower every variable inside `t` to `lvl`.
    fn occurs_adjust(&mut self, x: usize, lvl: usize, t: &Ty) -> bool {
        match self.prune(t) {
            Ty::Var(y) if y == x => true,
            Ty::Var(y) => {
                self.level[y] = self.level[y].min(lvl);
                false
            }
            Ty::Fun(a, b) | Ty::Pair(a, b) => {
                self.occurs_adjust(x, lvl, &a) || self.occurs_adjust(x, lvl, &b)
            }
            Ty::List(a) => self.occurs_adjust(x, lvl, &a),
            _ => false,
        }
    }
    fn unify(&mut self, a: &Ty, b: &Ty) -> Result<(), UErr> {
        self.unifications += 1;
        let (a, b) = (self.prune(a), self.prune(b));
        match (&a, &b) {
            (Ty::Var(x), Ty::Var(y)) if x == y => Ok(()),
            (Ty::Var(x), _) | (_, Ty::Var(x)) => {
                let other = if matches!(a, Ty::Var(_)) { &b } else { &a };
                if self.occurs_adjust(*x, self.level[*x], other) {
                    return Err(UErr::Occurs(Ty::Var(*x), self.zonk(other)));
                }
                self.binding[*x] = Some(other.clone());
                Ok(())
            }
            (Ty::Int, Ty::Int) | (Ty::Bool, Ty::Bool) => Ok(()),
            (Ty::Rigid(i), Ty::Rigid(j)) if i == j => Ok(()),
            (Ty::Fun(a1, r1), Ty::Fun(a2, r2)) | (Ty::Pair(a1, r1), Ty::Pair(a2, r2)) => {
                self.unify(a1, a2)?;
                self.unify(r1, r2)
            }
            (Ty::List(x), Ty::List(y)) => self.unify(x, y),
            _ => Err(UErr::Mismatch(self.zonk(&a), self.zonk(&b))),
        }
    }
    /// Unify and turn a failure into a message that names the conflicting types.
    fn expect_eq(
        &mut self,
        expected: &Ty,
        found: &Ty,
        what: &str,
        lo: usize,
        hi: usize,
    ) -> Res<()> {
        let Err(ue) = self.unify(expected, found) else {
            return Ok(());
        };
        let (e, f) = (self.zonk(expected), self.zonk(found));
        let mut n = Names::default();
        let head = format!(
            "{what}: expected `{}`, found `{}`",
            show_ty(&e, &mut n, 0),
            show_ty(&f, &mut n, 0)
        );
        let msg = match ue {
            UErr::Mismatch(x, y)
                if (x.clone(), y.clone()) != (e.clone(), f.clone())
                    && (y.clone(), x.clone()) != (e, f) =>
            {
                format!(
                    "{head} (cannot unify `{}` with `{}`)",
                    show_ty(&x, &mut n, 0),
                    show_ty(&y, &mut n, 0)
                )
            }
            UErr::Mismatch(..) => head,
            UErr::Occurs(v, t) => {
                format!(
                    "{what}: infinite type, `{}` occurs in `{}` (occurs check)",
                    show_ty(&v, &mut n, 0),
                    show_ty(&t, &mut n, 0)
                )
            }
        };
        err(msg, lo, hi)
    }
    /// Variables deeper than the current level are not reachable from the environment: quantify them.
    fn generalise(&self, t: &Ty) -> Scheme {
        fn go(i: &Infer, t: &Ty, map: &mut HashMap<usize, usize>) -> Ty {
            match i.prune(t) {
                Ty::Var(v) if i.level[v] > i.cur => {
                    let n = map.len();
                    Ty::Gen(*map.entry(v).or_insert(n))
                }
                Ty::Fun(a, b) => fun(go(i, &a, map), go(i, &b, map)),
                Ty::Pair(a, b) => pair(go(i, &a, map), go(i, &b, map)),
                Ty::List(a) => list(go(i, &a, map)),
                other => other,
            }
        }
        let mut map = HashMap::new();
        let ty = go(self, t, &mut map);
        Scheme { n: map.len(), ty }
    }
    fn instantiate(&mut self, s: &Scheme) -> Ty {
        let vars: Vec<Ty> = (0..s.n).map(|_| self.fresh()).collect();
        fn go(t: &Ty, vars: &[Ty]) -> Ty {
            match t {
                Ty::Gen(i) => vars[*i].clone(),
                Ty::Fun(a, b) => fun(go(a, vars), go(b, vars)),
                Ty::Pair(a, b) => pair(go(a, vars), go(b, vars)),
                Ty::List(a) => list(go(a, vars)),
                other => other.clone(),
            }
        }
        go(&s.ty, &vars)
    }
    fn with_binding<T>(
        &mut self,
        name: &str,
        s: Scheme,
        f: impl FnOnce(&mut Self) -> Res<T>,
    ) -> Res<T> {
        self.env.push((name.to_string(), s));
        let r = f(self);
        self.env.pop();
        r
    }
    fn infer(&mut self, e: &Expr) -> Res<Ty> {
        match &e.k {
            K::Int(_) => Ok(Ty::Int),
            K::Bool(_) => Ok(Ty::Bool),
            K::Nil => Ok(list(self.fresh())),
            K::Var(x) => match self
                .env
                .iter()
                .rev()
                .find(|(n, _)| n == x)
                .map(|(_, s)| s.clone())
            {
                Some(s) => Ok(self.instantiate(&s)),
                None => err(format!("unbound variable `{x}`"), e.lo, e.hi),
            },
            K::Lam(x, body) => {
                let a = self.fresh();
                let r = self.with_binding(
                    x,
                    Scheme {
                        n: 0,
                        ty: a.clone(),
                    },
                    |s| s.infer(body),
                )?;
                Ok(fun(a, r))
            }
            K::App(f, arg) => {
                let tf = self.infer(f)?;
                let ta = self.infer(arg)?;
                match self.prune(&tf) {
                    // Known function type: blame the ARGUMENT precisely.
                    Ty::Fun(p, r) => {
                        self.expect_eq(&p, &ta, "argument type mismatch", arg.lo, arg.hi)?;
                        Ok(*r)
                    }
                    other => {
                        let r = self.fresh();
                        self.expect_eq(&other, &fun(ta, r.clone()), "in application", e.lo, e.hi)?;
                        Ok(r)
                    }
                }
            }
            K::Let(x, rhs, body) => {
                self.cur += 1;
                let t = self.infer(rhs);
                self.cur -= 1;
                let scheme = self.generalise(&t?);
                self.with_binding(x, scheme, |s| s.infer(body))
            }
            K::LetRec(x, rhs, body) => {
                if !matches!(rhs.k, K::Lam(..)) {
                    return err(
                        "the right-hand side of `let rec` must be a function",
                        rhs.lo,
                        rhs.hi,
                    );
                }
                self.cur += 1;
                let tv = self.fresh(); // monomorphic inside its own definition, polymorphic afterwards
                let t = self.with_binding(
                    x,
                    Scheme {
                        n: 0,
                        ty: tv.clone(),
                    },
                    |s| s.infer(rhs),
                );
                let unified = t.and_then(|t| {
                    self.expect_eq(&tv, &t, "recursive definition", rhs.lo, rhs.hi)
                        .map(|_| t)
                });
                self.cur -= 1;
                let scheme = self.generalise(&unified?);
                self.with_binding(x, scheme, |s| s.infer(body))
            }
            K::If(c, t, f) => {
                let tc = self.infer(c)?;
                self.expect_eq(&Ty::Bool, &tc, "`if` condition", c.lo, c.hi)?;
                let tt = self.infer(t)?;
                let tf = self.infer(f)?;
                self.expect_eq(
                    &tt,
                    &tf,
                    "`else` branch must match the `then` branch",
                    f.lo,
                    f.hi,
                )?;
                Ok(tt)
            }
            K::Pair(a, b) => Ok(pair(self.infer(a)?, self.infer(b)?)),
        }
    }
}

/// Infers the principal type scheme of a closed program given as source text.
fn principal(source: &str) -> Res<Scheme> {
    let e = parse(source)?;
    let mut inf = Infer::new();
    inf.cur = 1; // like a top-level `let`: whatever stays free is generalised
    let t = inf.infer(&e)?;
    inf.cur = 0;
    Ok(inf.generalise(&t))
}

fn render_error(source: &str, e: &TypeError) -> String {
    let hi = e.hi.max(e.lo + 1);
    format!(
        "error: {}\n   | {source}\n   | {}{}",
        e.msg,
        " ".repeat(e.lo),
        "^".repeat(hi - e.lo)
    )
}

/// One-way matching: is `specific` an instance of `general` (whose Gen variables may be bound consistently)?
fn instance_of(general: &Ty, specific: &Ty, bind: &mut Vec<Option<Ty>>) -> bool {
    match (general, specific) {
        (Ty::Gen(i), t) => {
            if bind.len() <= *i {
                bind.resize(*i + 1, None);
            }
            match &bind[*i] {
                Some(b) => b == t,
                None => {
                    bind[*i] = Some(t.clone());
                    true
                }
            }
        }
        (Ty::Fun(a, b), Ty::Fun(c, d)) | (Ty::Pair(a, b), Ty::Pair(c, d)) => {
            instance_of(a, c, bind) && instance_of(b, d, bind)
        }
        (Ty::List(a), Ty::List(b)) => instance_of(a, b, bind),
        (x, y) => x == y,
    }
}

// ---------------------------------------------------------------- evaluator (to check soundness: no stuck states)

#[derive(Clone)]
enum Val {
    Int(i64),
    Bool(bool),
    Pair(Rc<Val>, Rc<Val>),
    Nil,
    Cons(Rc<Val>, Rc<Val>),
    Clo(Rc<Closure>),
    Prim(&'static str, Vec<Val>),
}
struct Closure {
    param: String,
    body: Expr,
    env: Vec<(String, Val)>,
    rec_name: Option<String>,
}

const PRIMS: [(&str, usize); 10] = [
    ("fst", 1),
    ("snd", 1),
    ("not", 1),
    ("uncons", 3),
    ("+", 2),
    ("-", 2),
    ("*", 2),
    ("<", 2),
    ("==", 2),
    ("::", 2),
];

impl std::fmt::Display for Val {
    fn fmt(&self, w: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Val::Int(n) => write!(w, "{n}"),
            Val::Bool(b) => write!(w, "{b}"),
            Val::Pair(a, b) => write!(w, "({a}, {b})"),
            Val::Nil | Val::Cons(..) => {
                let (mut items, mut cur) = (vec![], self.clone());
                while let Val::Cons(h, t) = cur {
                    items.push(h.to_string());
                    cur = (*t).clone();
                }
                write!(w, "[{}]", items.join(", "))
            }
            Val::Clo(_) | Val::Prim(..) => write!(w, "<fun>"),
        }
    }
}

type Stuck<T> = Result<T, String>;

fn eval(e: &Expr, env: &Vec<(String, Val)>, fuel: &mut u64) -> Stuck<Val> {
    if *fuel == 0 {
        return Err("out of fuel".into());
    }
    *fuel -= 1;
    Ok(match &e.k {
        K::Int(n) => Val::Int(*n),
        K::Bool(b) => Val::Bool(*b),
        K::Nil => Val::Nil,
        K::Var(x) => match env.iter().rev().find(|(n, _)| n == x) {
            Some((_, v)) => v.clone(),
            None => match PRIMS.iter().find(|(n, _)| n == x) {
                Some((n, _)) => Val::Prim(n, vec![]),
                None => return Err(format!("STUCK: unbound `{x}`")),
            },
        },
        K::Lam(x, b) => Val::Clo(Rc::new(Closure {
            param: x.clone(),
            body: (**b).clone(),
            env: env.clone(),
            rec_name: None,
        })),
        K::App(f, a) => {
            let (fv, av) = (eval(f, env, fuel)?, eval(a, env, fuel)?);
            apply(fv, av, fuel)?
        }
        K::Let(x, a, b) => {
            let v = eval(a, env, fuel)?;
            let mut env2 = env.clone();
            env2.push((x.clone(), v));
            eval(b, &env2, fuel)?
        }
        K::LetRec(x, a, b) => {
            let K::Lam(p, body) = &a.k else {
                return Err("STUCK: let rec of a non-function".into());
            };
            let clo = Val::Clo(Rc::new(Closure {
                param: p.clone(),
                body: (**body).clone(),
                env: env.clone(),
                rec_name: Some(x.clone()),
            }));
            let mut env2 = env.clone();
            env2.push((x.clone(), clo));
            eval(b, &env2, fuel)?
        }
        K::If(c, t, f) => match eval(c, env, fuel)? {
            Val::Bool(true) => eval(t, env, fuel)?,
            Val::Bool(false) => eval(f, env, fuel)?,
            _ => return Err("STUCK: `if` on a non-boolean".into()),
        },
        K::Pair(a, b) => Val::Pair(Rc::new(eval(a, env, fuel)?), Rc::new(eval(b, env, fuel)?)),
    })
}

fn apply(f: Val, a: Val, fuel: &mut u64) -> Stuck<Val> {
    match f {
        Val::Clo(c) => {
            let mut env = c.env.clone();
            if let Some(n) = &c.rec_name {
                env.push((n.clone(), Val::Clo(c.clone())));
            }
            env.push((c.param.clone(), a));
            eval(&c.body, &env, fuel)
        }
        Val::Prim(name, mut args) => {
            args.push(a);
            let arity = PRIMS.iter().find(|(n, _)| *n == name).unwrap().1;
            if args.len() < arity {
                return Ok(Val::Prim(name, args));
            }
            let bad = || {
                Err(format!(
                    "STUCK: primitive `{name}` applied to ill-typed arguments"
                ))
            };
            match (name, args.as_slice()) {
                ("fst", [Val::Pair(x, _)]) => Ok((**x).clone()),
                ("snd", [Val::Pair(_, y)]) => Ok((**y).clone()),
                ("not", [Val::Bool(b)]) => Ok(Val::Bool(!b)),
                ("+", [Val::Int(x), Val::Int(y)]) => Ok(Val::Int(x.wrapping_add(*y))),
                ("-", [Val::Int(x), Val::Int(y)]) => Ok(Val::Int(x.wrapping_sub(*y))),
                ("*", [Val::Int(x), Val::Int(y)]) => Ok(Val::Int(x.wrapping_mul(*y))),
                ("<", [Val::Int(x), Val::Int(y)]) => Ok(Val::Bool(x < y)),
                ("==", [Val::Int(x), Val::Int(y)]) => Ok(Val::Bool(x == y)),
                ("::", [h, t]) if matches!(t, Val::Nil | Val::Cons(..)) => {
                    Ok(Val::Cons(Rc::new(h.clone()), Rc::new(t.clone())))
                }
                ("uncons", [Val::Nil, n, _]) => Ok(n.clone()),
                ("uncons", [Val::Cons(h, t), _, c]) => {
                    let partial = apply(c.clone(), (**h).clone(), fuel)?;
                    apply(partial, (**t).clone(), fuel)
                }
                _ => bad(),
            }
        }
        _ => Err("STUCK: applying a non-function".into()),
    }
}

/// Does the runtime value have the shape the (ground) static type promises?
fn conforms(v: &Val, t: &Ty) -> bool {
    match (v, t) {
        (Val::Int(_), Ty::Int) | (Val::Bool(_), Ty::Bool) | (Val::Nil, Ty::List(_)) => true,
        (Val::Pair(a, b), Ty::Pair(x, y)) => conforms(a, x) && conforms(b, y),
        (Val::Cons(h, tl), Ty::List(x)) => conforms(h, x) && conforms(tl, t),
        (Val::Clo(_) | Val::Prim(..), Ty::Fun(..)) => true,
        _ => false,
    }
}

fn run(source: &str) -> Stuck<Val> {
    let e = parse(source).map_err(|e| e.msg)?;
    eval(&e, &vec![], &mut 2_000_000)
}

// ---------------------------------------------------------------- random well-typed term generation (from a type)

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
}

struct TermGen<'a> {
    rng: &'a mut Rng,
    env: Vec<(String, Ty)>,
    fresh: usize,
}

fn op(name: &str, l: Expr, r: Expr) -> Expr {
    let f = mk(K::App(Box::new(mk(K::Var(name.into()))), Box::new(l)));
    mk(K::App(Box::new(f), Box::new(r)))
}
fn call(name: &str, a: Expr) -> Expr {
    mk(K::App(Box::new(mk(K::Var(name.into()))), Box::new(a)))
}

impl TermGen<'_> {
    fn ground_type(&mut self, depth: usize) -> Ty {
        match if depth == 0 {
            self.rng.below(2)
        } else {
            self.rng.below(6)
        } {
            0 => Ty::Int,
            1 => Ty::Bool,
            2 => pair(self.ground_type(depth - 1), self.ground_type(depth - 1)),
            3 => list(self.ground_type(depth - 1)),
            _ => fun(self.ground_type(depth - 1), self.ground_type(depth - 1)),
        }
    }
    /// Direct uses of the environment: a variable of exactly type `t`, or a projection out of a pair variable.
    fn from_env(&mut self, t: &Ty) -> Vec<Expr> {
        let mut out = vec![];
        for (x, xt) in &self.env {
            if xt == t {
                out.push(mk(K::Var(x.clone())));
            }
            if let Ty::Pair(a, b) = xt {
                if **a == *t {
                    out.push(call("fst", mk(K::Var(x.clone()))));
                }
                if **b == *t {
                    out.push(call("snd", mk(K::Var(x.clone()))));
                }
            }
        }
        out
    }
    fn build(&mut self, t: &Ty, d: usize) -> Option<Expr> {
        let mut opts: Vec<u8> = if d == 0 {
            vec![0, 1]
        } else {
            vec![0, 1, 2, 3, 4, 5, 5]
        };
        for i in (1..opts.len()).rev() {
            opts.swap(i, self.rng.below(i + 1));
        }
        opts.into_iter().find_map(|o| self.option(o, t, d))
    }
    fn option(&mut self, o: u8, t: &Ty, d: usize) -> Option<Expr> {
        let d1 = d.saturating_sub(1);
        match (o, t) {
            (0, _) => {
                let c = self.from_env(t);
                if c.is_empty() {
                    None
                } else {
                    Some(c[self.rng.below(c.len())].clone())
                }
            }
            (1, Ty::Int) => Some(mk(K::Int(self.rng.below(10) as i64))),
            (1, Ty::Bool) => Some(mk(K::Bool(self.rng.chance(50)))),
            (1, Ty::Fun(a, b)) => {
                self.fresh += 1;
                let x = format!("x{}", self.fresh);
                self.env.push((x.clone(), (**a).clone()));
                let body = self.build(b, d1);
                self.env.pop();
                Some(mk(K::Lam(x, Box::new(body?))))
            }
            (1, Ty::Pair(a, b)) => Some(mk(K::Pair(
                Box::new(self.build(a, d1)?),
                Box::new(self.build(b, d1)?),
            ))),
            (1, Ty::List(a)) => {
                if d > 0 && self.rng.chance(60) {
                    Some(op("::", self.build(a, d1)?, self.build(t, d1)?))
                } else {
                    Some(mk(K::Nil))
                }
            }
            (2, Ty::Int) => Some(op(
                ["+", "-", "*"][self.rng.below(3)],
                self.build(&Ty::Int, d1)?,
                self.build(&Ty::Int, d1)?,
            )),
            (2, Ty::Bool) => Some(op(
                ["<", "=="][self.rng.below(2)],
                self.build(&Ty::Int, d1)?,
                self.build(&Ty::Int, d1)?,
            )),
            (3, _) => {
                let c = self.build(&Ty::Bool, d1)?;
                Some(mk(K::If(
                    Box::new(c),
                    Box::new(self.build(t, d1)?),
                    Box::new(self.build(t, d1)?),
                )))
            }
            (4, _) => {
                let u = self.ground_type(1);
                self.fresh += 1;
                let y = format!("y{}", self.fresh);
                let rhs = self.build(&u, d1)?;
                self.env.push((y.clone(), u));
                let body = self.build(t, d1);
                self.env.pop();
                Some(mk(K::Let(y, Box::new(rhs), Box::new(body?))))
            }
            (5, _) => {
                // A redex: pick an argument type (often one already in scope), build `f arg` with f : q -> t.
                let mut in_scope: Vec<Ty> = vec![];
                for (_, xt) in &self.env {
                    in_scope.push(xt.clone());
                    if let Ty::Pair(x, y) = xt {
                        in_scope.extend([(**x).clone(), (**y).clone()]);
                    }
                }
                let q = if !in_scope.is_empty() && self.rng.chance(60) {
                    in_scope[self.rng.below(in_scope.len())].clone()
                } else {
                    self.ground_type(1)
                };
                let f = self.build(&fun(q.clone(), t.clone()), d1)?;
                Some(mk(K::App(Box::new(f), Box::new(self.build(&q, d1)?))))
            }
            _ => None,
        }
    }
}

fn ground(t: &Ty) -> Ty {
    match t {
        Ty::Rigid(_) => Ty::Int,
        Ty::Fun(a, b) => fun(ground(a), ground(b)),
        Ty::Pair(a, b) => pair(ground(a), ground(b)),
        Ty::List(a) => list(ground(a)),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------- demos and tests

fn main() {
    let (a, b) = (Ty::Rigid(0), Ty::Rigid(1));
    let map = "let rec map = \\f xs. uncons xs [] (\\h t. f h :: map f t) in map";
    let foldr = "let rec foldr = \\f z xs. uncons xs z (\\h t. f h (foldr f z t)) in foldr";
    let length = "let rec length = \\xs. uncons xs 0 (\\h t. 1 + length t) in length";

    let accepted: Vec<(&str, &str)> = vec![
        ("\\x. x", "forall a. a -> a"),
        ("\\x y. x", "forall a b. a -> b -> a"),
        (
            "\\f g x. f (g x)",
            "forall a b c. (a -> b) -> (c -> a) -> c -> b",
        ),
        (
            "\\f x y. f y x",
            "forall a b c. (a -> b -> c) -> b -> a -> c",
        ),
        ("\\f x. f (f x)", "forall a. (a -> a) -> a -> a"),
        ("\\p. (snd p, fst p)", "forall a b. (a, b) -> (b, a)"),
        ("let id = \\x. x in (id 1, id true)", "(int, bool)"),
        (
            "let twice = \\f x. f (f x) in (twice (\\n. n + 1) 0, twice not true)",
            "(int, bool)",
        ),
        ("\\y. let f = \\x. y in f", "forall a b. a -> b -> a"),
        (
            "\\y. let f = \\x. y in (f 1, f true)",
            "forall a. a -> (a, a)",
        ),
        ("1 :: 2 :: []", "[int]"),
        ("[]", "forall a. [a]"),
        (map, "forall a b. (a -> b) -> [a] -> [b]"),
        (foldr, "forall a b. (a -> b -> b) -> b -> [a] -> b"),
        (length, "forall a. [a] -> int"),
        (
            "let rec fact = \\n. if n < 2 then 1 else n * fact (n - 1) in fact",
            "int -> int",
        ),
        ("let rec y = \\f. f (y f) in y", "forall a. (a -> a) -> a"),
    ];
    println!("principal types:");
    for (s, want) in &accepted {
        let got = show_scheme(&principal(s).unwrap_or_else(|e| panic!("{s}: {}", e.msg)));
        println!(
            "  {:<44} : {got}",
            if s.len() > 44 {
                format!("{}...", &s[..41])
            } else {
                s.to_string()
            }
        );
        assert_eq!(&got, want, "principal type of {s}");
    }

    // Rejections, with the exact source fragment each error points at.
    let rejected: Vec<(&str, &str, &str)> = vec![
        ("\\x. x x", "infinite type", "x x"),
        ("\\f. (\\x. f (x x)) (\\x. f (x x))", "infinite type", "x x"), // the Y combinator
        (
            "(\\id. (id 1, id true)) (\\x. x)",
            "expected `int`, found `bool`",
            "true",
        ),
        ("1 + true", "expected `int`, found `bool`", "true"),
        (
            "if 1 then 2 else 3",
            "`if` condition: expected `bool`, found `int`",
            "1",
        ),
        ("if true then 1 else false", "`else` branch", "false"),
        ("foo 1", "unbound variable `foo`", "foo"),
        (
            "1 :: true :: []",
            "expected `[int]`, found `[bool]`",
            "true :: []",
        ),
        ("let rec k = 5 in k", "must be a function", "5"),
        ("fst 3", "expected `(a, b)`", "3"),
    ];
    println!("\nrejected (error, with source span):");
    for (s, needle, frag) in &rejected {
        let e = principal(s).expect_err(s);
        assert!(
            e.msg.contains(needle),
            "{s}: message `{}` lacks `{needle}`",
            e.msg
        );
        assert_eq!(&s[e.lo..e.hi], *frag, "span of the error in {s}");
        println!("{}\n", render_error(s, &e));
    }

    // Programs run to the expected values (and the typed ones never get stuck).
    for (s, want) in [
        (
            "let rec fact = \\n. if n < 2 then 1 else n * fact (n - 1) in fact 5",
            "120",
        ),
        ("let id = \\x. x in (id 1, id true)", "(1, true)"),
        (
            &format!("({map}) (\\x. x * 2) (1 :: 2 :: 3 :: [])"),
            "[2, 4, 6]",
        ),
        (
            &format!("({foldr}) (\\a b. a + b) 0 (1 :: 2 :: 3 :: 4 :: [])"),
            "10",
        ),
        (&format!("({length}) (true :: false :: [])"), "2"),
    ] {
        assert_eq!(run(s).unwrap().to_string(), want);
    }

    // Soundness / principality fuzz: terms built FROM a type, inferred, compared, and executed.
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    let rigid_pool: Vec<Ty> = vec![
        fun(a.clone(), a.clone()),
        fun(a.clone(), fun(b.clone(), a.clone())),
        fun(fun(a.clone(), b.clone()), fun(a.clone(), b.clone())),
        fun(a.clone(), pair(a.clone(), a.clone())),
        fun(list(a.clone()), list(a.clone())),
        fun(pair(fun(a.clone(), b.clone()), a.clone()), b.clone()),
        fun(pair(a.clone(), b.clone()), pair(b.clone(), a.clone())),
        fun(
            fun(a.clone(), fun(b.clone(), Ty::Int)),
            fun(b.clone(), fun(a.clone(), Ty::Int)),
        ),
    ];
    let (mut generic_sizes, mut strictly_more_general, mut evaluated, mut total_unifs) =
        (0usize, 0usize, 0usize, 0usize);
    for i in 0..200 {
        let mut g = TermGen {
            rng: &mut rng,
            env: vec![],
            fresh: 0,
        };
        // (1) polymorphic target type: the principal type must be at least as general as the requested one.
        let t = rigid_pool[i % rigid_pool.len()].clone();
        // Some random choices dead-end (e.g. no way to get an `a`), so retry: the pool types are all inhabited.
        let e = (0..500)
            .find_map(|_| g.build(&t, 4))
            .expect("rigid targets in the pool are inhabited");
        let s = src(&e);
        generic_sizes += s.len();
        let p = principal(&s)
            .unwrap_or_else(|er| panic!("generated well-typed term rejected: {s}\n{}", er.msg));
        assert!(
            instance_of(&p.ty, &t, &mut vec![]),
            "`{}` is not an instance of principal `{}` for {s}",
            show_ty(&t, &mut Names::default(), 0),
            show_scheme(&p)
        );
        strictly_more_general += (show_ty(&p.ty, &mut Names::default(), 0)
            != show_ty(&t, &mut Names::default(), 0)) as usize;
        let v = run(&s).unwrap_or_else(|m| panic!("{m} in {s}"));
        assert!(conforms(&v, &ground(&t)));

        // (2) ground target: apply the term to generated arguments until a non-function remains, then run it.
        let t = g.ground_type(2);
        let Some(e) = g.build(&t, 4) else { continue };
        let (mut text, mut rest) = (src(&e), t.clone());
        while let Ty::Fun(pa, pr) = rest {
            let arg = g.build(&pa, 3).expect("ground types are inhabited");
            text = format!("({text} {})", src(&arg));
            rest = *pr;
        }
        let p = principal(&text)
            .unwrap_or_else(|er| panic!("generated well-typed term rejected: {text}\n{}", er.msg));
        assert!(
            instance_of(&p.ty, &rest, &mut vec![]),
            "principal `{}` vs expected `{}` for {text}",
            show_scheme(&p),
            show_ty(&rest, &mut Names::default(), 0)
        );
        let v = run(&text).unwrap_or_else(|m| panic!("{m} in {text}"));
        assert!(
            conforms(&v, &rest),
            "value {v} does not conform to its type for {text}"
        );
        evaluated += 1;
        let mut inf = Infer::new();
        let _ = inf.infer(&parse(&text).unwrap());
        total_unifs += inf.unifications;
    }
    println!(
        "fuzz: 200 polymorphic-target terms inferred (avg {} chars) with the requested type an instance of the principal type",
        generic_sizes / 200
    );
    println!(
        "      {evaluated} ground-typed programs ({total_unifs} unifications) evaluated without getting stuck, value shape matches type"
    );

    // Ill-typed terms must be rejected: a well-typed term of type T used where an int is required.
    let (mut rejected_n, mut tried) = (0, 0);
    while tried < 200 {
        let mut g = TermGen {
            rng: &mut rng,
            env: vec![],
            fresh: 0,
        };
        let t = g.ground_type(2);
        if t == Ty::Int {
            continue;
        }
        let Some(e) = g.build(&t, 3) else { continue };
        let text = src(&op("+", e, mk(K::Int(1))));
        tried += 1;
        let er = principal(&text).expect_err(&text);
        assert!(
            er.msg.contains("expected `int`")
                || er.msg.contains("infinite type")
                || er.msg.contains("expected"),
            "{}",
            er.msg
        );
        assert!(er.lo < er.hi && er.hi <= text.len());
        rejected_n += 1;
    }
    println!(
        "      {rejected_n}/200 ill-typed terms (`t + 1` with t not an int) rejected with a source span"
    );
    println!(
        "      polymorphic results strictly more general than the requested type: {strictly_more_general}/200"
    );
    println!("all assertions passed");
}
