//! RECIPE 811 (compilers): System F, higher-rank bidirectional typing (Dunfield-Krishnaswami), record subtyping and variance.
//! LEVEL: advanced
//! Use in: type checkers for languages with generics, higher-rank types, structural records or row-like subtyping; understanding rustc's `for<'a>` bounds.   Run: cargo run --release --example system_f_bidirectional_typing
//! COMMON USES: the core of GHC's RankNTypes/QuickLook-style checking (annotations only where needed), TypeScript/Flow structural
//! subtyping of object types and function parameter variance, Scala/Kotlin generics checking, language-server "what is the type of
//! this expression" engines, and soundness testing of any checker via progress + preservation.
//! Course: Dunfield and Krishnaswami, "Complete and Easy Bidirectional Typechecking for Higher-Rank Polymorphism" (ICFP 2013).
#![allow(dead_code)]
//!
//! THE IDEA: three layers in one file, each checked against a simpler oracle.
//!  A. CORE: fully annotated System F with records and SUBTYPING. `\x:A. e`, `/\a. e` (type abstraction), `e [A]` (type application).
//!     Checking is pure SYNTHESIS (bottom-up), because every binder says its type. Small-step call-by-value semantics. This part
//!     is simple enough to trust, so it is the oracle: PROGRESS (a closed well-typed term is a value or steps) and PRESERVATION
//!     (a step yields a term whose type is a subtype of the old type) are tested on thousands of terms produced by TYPE-DIRECTED
//!     GENERATION (build a term for a requested type, never guess-and-filter).
//!  B. BIDIRECTIONAL CHECKER (the paper's algorithmic system with an ORDERED CONTEXT): `\x. e` needs no annotation. Two modes:
//!     CHECK `e <= A` (the type flows in: lambdas read their parameter type off `A -> B`, `forall` is introduced by a fresh rigid
//!     variable) and SYNTHESISE `e => A` (the type flows out: variables, applications, annotations). Where the type of a lambda
//!     parameter is unknown the checker makes an EXISTENTIAL variable `^a` ("some monotype, to be decided"), and SUBTYPING
//!     (`forall a. a -> a  <:  Int -> Int` by instantiating `a := ^a` and then solving `^a := Int`) solves them. The context is an
//!     ORDERED list; a solution `^a := t` is legal only if everything `t` mentions is declared BEFORE `^a`. That one rule is what
//!     prevents a rigid variable from escaping its scope. Inference is predicative: `^a` is only ever solved with a MONOtype, so
//!     `\f. (f 1, f true)` stays rejected, while `(\f. f 1) : (forall a. a -> a) -> Int` works because the annotation supplies the
//!     polymorphic parameter type: "annotations only where needed" means exactly at higher-rank lambda parameters.
//!     Extensions beyond the paper: Int/Bool/`if`, explicit type application (arbitrary types allowed as arguments), RECORDS with
//!     width + depth subtyping, and `Top`. Record instantiation of an existential is only supported for monotypes.
//!  C. DIFFERENTIAL TEST: every random core term is ERASED to a surface term (lambda annotations dropped, annotations added only at
//!     synthesis positions: application heads, projections, record literals) and the bidirectional checker must accept it at the
//!     core type. Fully annotated terms must also synthesise a type equivalent to the oracle's.
//! SUBTYPING: width (`{a,b} <: {a}`), depth (`{a: Int} <: {a: Top}`), and functions are CONTRAVARIANT in the argument and COVARIANT in
//! the result: `(S -> T) <: (S' -> T')` iff `S' <: S` and `T <: T'`. The LISKOV test generates a value of a subtype, uses it through
//! the supertype (project every field, call every function) and asserts nothing gets stuck and the result still has the promised type.
//! FREE THEOREMS: by parametricity every closed term of type `forall a. a -> a` must be the identity; we sample random inhabitants
//! produced by the generator and check them on several instantiations. The same is done for `forall a b. a -> b -> a` (const),
//! `forall a. a -> a -> a` (one of the two arguments) and the Church-numeral type.
//! HOW RUST DIFFERS: rustc only has subtyping for LIFETIMES (`&'long T <: &'short T`), with the same variance rules (`fn(T)` is
//! contravariant, `&mut T` and `Cell<T>` are invariant, which is exactly the soundness condition for mutable fields). Higher-rank
//! types appear as `for<'a> Fn(&'a T) -> &'a U` (HRTB): rank-2 over lifetimes, and the checker needs no annotation because closure
//! parameter types are read from the expected `Fn` bound, the same "type flows inward" idea as CHECK mode.
//! Read: Dunfield and Krishnaswami, https://arxiv.org/abs/1306.6032 ; Pierce, "Types and Programming Languages", ch. 23 (System F) and 15
//! (subtyping) https://www.cis.upenn.edu/~bcpierce/tapl/ ; Wadler, "Theorems for free!" (FPCA 1989, cited by title, not fetched);
//! Pierce and Turner, "Local Type Inference" (TOPLAS 2000, by title).
//! TRY NEXT:
//!   - [S] Remove the occurs check in `Dk::sub` for `(Ex, _)`: which rejection-suite line fails, and does the checker loop or return garbage?
//!   - [S] Add a `Unit` type and `()` literal: what do `sub`, `gen_ty` and `core_ty` each need? Re-run the Liskov test.
//!   - [M] Predict, then measure: swap `sub` rule order so `forall`-left is tried before `forall`-right; do any of the 1500 differential terms stop checking?
//!   - [M] Make `If` synthesis return the join (least common supertype) of its branches in `core_ty`; update `Gen` option 4 to use different branch types.
//!   - [A] PROJECT: give `Dk` a source-span error type (see `hindley_milner_inference`) and a hover query "type at byte offset"; return types for every subterm.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};

type R<T> = Result<T, String>;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ------------------------------------------------------------------ types
#[derive(Clone, Debug, PartialEq)]
enum Ty {
    Int,
    Bool,
    Top,
    Var(String),
    Ex(usize),
    Fun(Box<Ty>, Box<Ty>),
    Forall(String, Box<Ty>),
    Rec(BTreeMap<String, Ty>),
}
fn fun(a: Ty, b: Ty) -> Ty {
    Ty::Fun(Box::new(a), Box::new(b))
}
fn forall(a: &str, b: Ty) -> Ty {
    Ty::Forall(a.to_string(), Box::new(b))
}
fn tvar(a: &str) -> Ty {
    Ty::Var(a.to_string())
}
fn rec_ty(fs: Vec<(&str, Ty)>) -> Ty {
    Ty::Rec(fs.into_iter().map(|(l, t)| (l.to_string(), t)).collect())
}
static COUNTER: AtomicUsize = AtomicUsize::new(0);
fn fresh_name(base: &str) -> String {
    format!("{base}${}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn fv_ty(t: &Ty, out: &mut BTreeSet<String>) {
    match t {
        Ty::Var(x) => {
            out.insert(x.clone());
        }
        Ty::Fun(a, b) => {
            fv_ty(a, out);
            fv_ty(b, out);
        }
        Ty::Forall(x, b) => {
            let mut inner = BTreeSet::new();
            fv_ty(b, &mut inner);
            inner.remove(x);
            out.extend(inner);
        }
        Ty::Rec(m) => m.values().for_each(|t| fv_ty(t, out)),
        _ => {}
    }
}
fn ex_ids(t: &Ty, out: &mut Vec<usize>) {
    match t {
        Ty::Ex(n) => out.push(*n),
        Ty::Fun(a, b) => {
            ex_ids(a, out);
            ex_ids(b, out);
        }
        Ty::Forall(_, b) => ex_ids(b, out),
        Ty::Rec(m) => m.values().for_each(|t| ex_ids(t, out)),
        _ => {}
    }
}
/// `t[a := s]`, renaming a binder that would capture a free variable of `s`.
fn subst_ty(t: &Ty, a: &str, s: &Ty) -> Ty {
    match t {
        Ty::Var(x) if x == a => s.clone(),
        Ty::Fun(p, q) => fun(subst_ty(p, a, s), subst_ty(q, a, s)),
        Ty::Rec(m) => Ty::Rec(
            m.iter()
                .map(|(l, t)| (l.clone(), subst_ty(t, a, s)))
                .collect(),
        ),
        Ty::Forall(x, b) => {
            if x == a {
                return t.clone();
            }
            let mut fs = BTreeSet::new();
            fv_ty(s, &mut fs);
            if fs.contains(x) {
                let x2 = fresh_name(x);
                let b2 = subst_ty(b, x, &Ty::Var(x2.clone()));
                Ty::Forall(x2, Box::new(subst_ty(&b2, a, s)))
            } else {
                Ty::Forall(x.clone(), Box::new(subst_ty(b, a, s)))
            }
        }
        _ => t.clone(),
    }
}
/// Alpha-equivalence of types.
fn ty_eq(a: &Ty, b: &Ty) -> bool {
    fn go<'a>(a: &'a Ty, b: &'a Ty, ea: &mut Vec<&'a str>, eb: &mut Vec<&'a str>) -> bool {
        match (a, b) {
            (Ty::Var(x), Ty::Var(y)) => match (
                ea.iter().rposition(|n| n == x),
                eb.iter().rposition(|n| n == y),
            ) {
                (Some(i), Some(j)) => i == j,
                (None, None) => x == y,
                _ => false,
            },
            (Ty::Fun(p, q), Ty::Fun(r, s)) => go(p, r, ea, eb) && go(q, s, ea, eb),
            (Ty::Forall(x, p), Ty::Forall(y, q)) => {
                ea.push(x);
                eb.push(y);
                let r = go(p, q, ea, eb);
                ea.pop();
                eb.pop();
                r
            }
            (Ty::Rec(m), Ty::Rec(n)) => {
                m.len() == n.len()
                    && m.iter()
                        .all(|(l, t)| n.get(l).is_some_and(|u| go(t, u, ea, eb)))
            }
            _ => a == b,
        }
    }
    go(a, b, &mut vec![], &mut vec![])
}
fn show_ty(t: &Ty) -> String {
    fn go(t: &Ty, prec: u8) -> String {
        match t {
            Ty::Int => "Int".into(),
            Ty::Bool => "Bool".into(),
            Ty::Top => "Top".into(),
            Ty::Var(x) => x.split('$').next().unwrap().to_string(),
            Ty::Ex(n) => format!("^{n}"),
            Ty::Fun(a, b) => {
                let s = format!("{} -> {}", go(a, 1), go(b, 0));
                if prec > 0 { format!("({s})") } else { s }
            }
            Ty::Forall(x, b) => {
                let s = format!("forall {}. {}", x.split('$').next().unwrap(), go(b, 0));
                if prec > 0 { format!("({s})") } else { s }
            }
            Ty::Rec(m) => format!(
                "{{{}}}",
                m.iter()
                    .map(|(l, t)| format!("{l}: {}", go(t, 0)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    go(t, 0)
}
/// Existentials renamed `^a ^b ..` by first appearance so output does not depend on a global counter.
fn show_norm(t: &Ty) -> String {
    let mut ids = vec![];
    ex_ids(t, &mut ids);
    let mut seen: Vec<usize> = vec![];
    for i in ids {
        if !seen.contains(&i) {
            seen.push(i);
        }
    }
    let mut t2 = t.clone();
    fn rename(t: &mut Ty, seen: &[usize]) {
        match t {
            Ty::Ex(n) => {
                *t = Ty::Var(format!(
                    "^{}",
                    (b'a' + seen.iter().position(|s| s == n).unwrap() as u8) as char
                ))
            }
            Ty::Fun(a, b) => {
                rename(a, seen);
                rename(b, seen);
            }
            Ty::Forall(_, b) => rename(b, seen),
            Ty::Rec(m) => m.values_mut().for_each(|t| rename(t, seen)),
            _ => {}
        }
    }
    rename(&mut t2, &seen);
    show_ty(&t2)
}
fn mono(t: &Ty) -> bool {
    match t {
        Ty::Forall(..) => false,
        Ty::Fun(a, b) => mono(a) && mono(b),
        Ty::Rec(m) => m.values().all(mono),
        _ => true,
    }
}

// ------------------------------------------------------------------ terms
#[derive(Clone, Debug, PartialEq)]
enum E {
    Int(i64),
    Bool(bool),
    Var(String),
    Lam(String, Option<Ty>, Box<E>),
    App(Box<E>, Box<E>),
    TLam(String, Box<E>),
    TApp(Box<E>, Ty),
    Ann(Box<E>, Ty),
    Rec(Vec<(String, E)>),
    Proj(Box<E>, String),
    If(Box<E>, Box<E>, Box<E>),
    Inc(Box<E>),
}
fn bx(e: E) -> Box<E> {
    Box::new(e)
}
fn evar(x: &str) -> E {
    E::Var(x.to_string())
}
fn elam(x: &str, t: Option<Ty>, b: E) -> E {
    E::Lam(x.to_string(), t, bx(b))
}
fn eapp(f: E, a: E) -> E {
    E::App(bx(f), bx(a))
}
/// `inc` is an ordinary annotated lambda over a primitive node, so it is a value and prints as `inc`.
fn prim_inc() -> E {
    elam("n", Some(Ty::Int), E::Inc(bx(evar("n"))))
}
fn subst_ty_e(e: &E, a: &str, s: &Ty) -> E {
    let st = |t: &Ty| subst_ty(t, a, s);
    let go = |e: &E| bx(subst_ty_e(e, a, s));
    match e {
        E::Lam(x, t, b) => E::Lam(x.clone(), t.as_ref().map(st), go(b)),
        E::App(f, x) => E::App(go(f), go(x)),
        E::TLam(x, b) => {
            if x == a {
                e.clone()
            } else {
                E::TLam(x.clone(), go(b))
            }
        }
        E::TApp(f, t) => E::TApp(go(f), st(t)),
        E::Ann(x, t) => E::Ann(go(x), st(t)),
        E::Rec(fs) => E::Rec(
            fs.iter()
                .map(|(l, x)| (l.clone(), subst_ty_e(x, a, s)))
                .collect(),
        ),
        E::Proj(x, l) => E::Proj(go(x), l.clone()),
        E::If(c, t, f) => E::If(go(c), go(t), go(f)),
        E::Inc(x) => E::Inc(go(x)),
        _ => e.clone(),
    }
}
fn subst_e(e: &E, x: &str, v: &E) -> E {
    let go = |e: &E| bx(subst_e(e, x, v));
    match e {
        E::Var(y) => {
            if y == x {
                v.clone()
            } else {
                e.clone()
            }
        }
        E::Lam(y, t, b) => {
            if y == x {
                e.clone()
            } else {
                E::Lam(y.clone(), t.clone(), go(b))
            }
        }
        E::App(f, a) => E::App(go(f), go(a)),
        E::TLam(a, b) => E::TLam(a.clone(), go(b)),
        E::TApp(f, t) => E::TApp(go(f), t.clone()),
        E::Ann(a, t) => E::Ann(go(a), t.clone()),
        E::Rec(fs) => E::Rec(
            fs.iter()
                .map(|(l, a)| (l.clone(), subst_e(a, x, v)))
                .collect(),
        ),
        E::Proj(a, l) => E::Proj(go(a), l.clone()),
        E::If(c, t, f) => E::If(go(c), go(t), go(f)),
        E::Inc(a) => E::Inc(go(a)),
        _ => e.clone(),
    }
}
fn show_e(e: &E) -> String {
    fn go(e: &E, prec: u8) -> String {
        let paren = |s: String, need: bool| if need { format!("({s})") } else { s };
        match e {
            E::Int(n) => n.to_string(),
            E::Bool(b) => b.to_string(),
            E::Var(x) => x.clone(),
            _ if *e == prim_inc() => "inc".into(),
            E::Lam(x, None, b) => paren(format!("\\{x}. {}", go(b, 0)), prec > 0),
            E::Lam(x, Some(t), b) => paren(format!("\\{x}:{}. {}", show_ty(t), go(b, 0)), prec > 0),
            E::App(f, a) => paren(format!("{} {}", go(f, 1), go(a, 2)), prec > 1),
            E::TLam(a, b) => paren(format!("/\\{a}. {}", go(b, 0)), prec > 0),
            E::TApp(f, t) => format!("{}[{}]", go(f, 2), show_ty(t)),
            E::Ann(x, t) => format!("({} : {})", go(x, 0), show_ty(t)),
            E::Rec(fs) => format!(
                "{{{}}}",
                fs.iter()
                    .map(|(l, x)| format!("{l} = {}", go(x, 0)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            E::Proj(x, l) => format!("{}.{l}", go(x, 2)),
            E::If(c, t, f) => paren(
                format!("if {} then {} else {}", go(c, 0), go(t, 0), go(f, 0)),
                prec > 0,
            ),
            E::Inc(x) => format!("<inc {}>", go(x, 2)),
        }
    }
    go(e, 0)
}

// ------------------------------------------------------------------ lexer and parser
#[derive(Clone, Debug, PartialEq)]
enum Tk {
    Id(String),
    Num(i64),
    S(&'static str),
}
fn lex(s: &str) -> R<Vec<Tk>> {
    let cs: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let st = i;
            while i < cs.len() && cs[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Tk::Num(
                cs[st..i]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .map_err(|_| "bad number")?,
            ));
        } else if c.is_alphabetic() || c == '_' {
            let st = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_' || cs[i] == '\'') {
                i += 1;
            }
            out.push(Tk::Id(cs[st..i].iter().collect()));
        } else if c == '-' && cs.get(i + 1) == Some(&'>') {
            out.push(Tk::S("->"));
            i += 2;
        } else if c == '/' && cs.get(i + 1) == Some(&'\\') {
            out.push(Tk::S("/\\"));
            i += 2;
        } else {
            let s = match c {
                '\\' => "\\",
                '.' => ".",
                ':' => ":",
                ',' => ",",
                '=' => "=",
                '[' => "[",
                ']' => "]",
                '{' => "{",
                '}' => "}",
                '(' => "(",
                ')' => ")",
                _ => return Err(format!("unexpected character `{c}`")),
            };
            out.push(Tk::S(s));
            i += 1;
        }
    }
    Ok(out)
}
struct Parser {
    t: Vec<Tk>,
    i: usize,
}
const KW: [&str; 6] = ["if", "then", "else", "forall", "true", "false"];
impl Parser {
    fn peek(&self) -> Option<&Tk> {
        self.t.get(self.i)
    }
    fn eat(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Some(Tk::S(x)) if *x == s) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Some(Tk::Id(x)) if x == s) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> R<()> {
        if self.eat(s) {
            Ok(())
        } else {
            Err(format!("expected `{s}`, found {:?}", self.peek()))
        }
    }
    fn ident(&mut self) -> R<String> {
        match self.peek().cloned() {
            Some(Tk::Id(x)) if !KW.contains(&x.as_str()) => {
                self.i += 1;
                Ok(x)
            }
            t => Err(format!("expected a name, found {t:?}")),
        }
    }
    fn ty(&mut self) -> R<Ty> {
        if self.eat_kw("forall") {
            let mut names = vec![self.ident()?];
            while !matches!(self.peek(), Some(Tk::S("."))) {
                names.push(self.ident()?);
            }
            self.expect(".")?;
            let mut b = self.ty()?;
            for n in names.iter().rev() {
                b = forall(n, b);
            }
            return Ok(b);
        }
        let a = self.ty_atom()?;
        Ok(if self.eat("->") {
            fun(a, self.ty()?)
        } else {
            a
        })
    }
    fn ty_atom(&mut self) -> R<Ty> {
        if self.eat("(") {
            let t = self.ty()?;
            self.expect(")")?;
            return Ok(t);
        }
        if self.eat("{") {
            let mut m = BTreeMap::new();
            while !self.eat("}") {
                if !m.is_empty() {
                    self.expect(",")?;
                }
                let l = self.ident()?;
                self.expect(":")?;
                m.insert(l, self.ty()?);
            }
            return Ok(Ty::Rec(m));
        }
        Ok(match self.ident()?.as_str() {
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "Top" => Ty::Top,
            x => Ty::Var(x.to_string()),
        })
    }
    fn term(&mut self) -> R<E> {
        if self.eat("\\") {
            let x = self.ident()?;
            let t = if self.eat(":") {
                Some(self.ty()?)
            } else {
                None
            };
            self.expect(".")?;
            return Ok(E::Lam(x, t, bx(self.term()?)));
        }
        if self.eat("/\\") {
            let a = self.ident()?;
            self.expect(".")?;
            return Ok(E::TLam(a, bx(self.term()?)));
        }
        if self.eat_kw("if") {
            let c = self.term()?;
            if !self.eat_kw("then") {
                return Err("expected `then`".into());
            }
            let t = self.term()?;
            if !self.eat_kw("else") {
                return Err("expected `else`".into());
            }
            return Ok(E::If(bx(c), bx(t), bx(self.term()?)));
        }
        let mut f = self.postfix()?;
        while self.starts_atom() {
            f = eapp(f, self.postfix()?);
        }
        Ok(f)
    }
    fn starts_atom(&self) -> bool {
        match self.peek() {
            Some(Tk::Num(_)) | Some(Tk::S("(")) | Some(Tk::S("{")) => true,
            Some(Tk::Id(x)) => !["then", "else", "forall"].contains(&x.as_str()),
            _ => false,
        }
    }
    fn postfix(&mut self) -> R<E> {
        let mut a = self.atom()?;
        loop {
            if self.eat(".") {
                a = E::Proj(bx(a), self.ident()?);
            } else if self.eat("[") {
                let t = self.ty()?;
                self.expect("]")?;
                a = E::TApp(bx(a), t);
            } else {
                return Ok(a);
            }
        }
    }
    fn atom(&mut self) -> R<E> {
        match self.peek().cloned() {
            Some(Tk::Num(n)) => {
                self.i += 1;
                Ok(E::Int(n))
            }
            Some(Tk::Id(x)) if x == "true" || x == "false" => {
                self.i += 1;
                Ok(E::Bool(x == "true"))
            }
            Some(Tk::Id(x)) if x == "inc" => {
                self.i += 1;
                Ok(prim_inc())
            }
            Some(Tk::S("(")) => {
                self.i += 1;
                let e = self.term()?;
                let e = if self.eat(":") {
                    E::Ann(bx(e), self.ty()?)
                } else {
                    e
                };
                self.expect(")")?;
                Ok(e)
            }
            Some(Tk::S("{")) => {
                self.i += 1;
                let mut fs = vec![];
                while !self.eat("}") {
                    if !fs.is_empty() {
                        self.expect(",")?;
                    }
                    let l = self.ident()?;
                    self.expect("=")?;
                    fs.push((l, self.term()?));
                }
                Ok(E::Rec(fs))
            }
            Some(Tk::Id(_)) => Ok(E::Var(self.ident()?)),
            t => Err(format!("unexpected {t:?}")),
        }
    }
}
fn parse(s: &str) -> R<E> {
    let mut p = Parser { t: lex(s)?, i: 0 };
    let e = p.term()?;
    if p.i != p.t.len() {
        return Err(format!("trailing tokens at {}", p.i));
    }
    Ok(e)
}
fn parse_ty(s: &str) -> R<Ty> {
    let mut p = Parser { t: lex(s)?, i: 0 };
    let t = p.ty()?;
    if p.i != p.t.len() {
        return Err("trailing tokens".into());
    }
    Ok(t)
}
fn pe(s: &str) -> E {
    parse(s).unwrap_or_else(|e| panic!("parse {s:?}: {e}"))
}
fn pty(s: &str) -> Ty {
    parse_ty(s).unwrap_or_else(|e| panic!("parse type {s:?}: {e}"))
}

// ------------------------------------------------------------------ A. core: subtyping, synthesis, small-step semantics
fn sub_core(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (_, Ty::Top) | (Ty::Int, Ty::Int) | (Ty::Bool, Ty::Bool) => true,
        (Ty::Var(x), Ty::Var(y)) => x == y,
        // contravariant argument, covariant result
        (Ty::Fun(a1, a2), Ty::Fun(b1, b2)) => sub_core(b1, a1) && sub_core(a2, b2),
        // width: every field of the supertype exists; depth: and is a subtype
        (Ty::Rec(fa), Ty::Rec(fb)) => fb
            .iter()
            .all(|(l, tb)| fa.get(l).is_some_and(|ta| sub_core(ta, tb))),
        (Ty::Forall(x, p), Ty::Forall(y, q)) => {
            let z = Ty::Var(fresh_name("sk"));
            sub_core(&subst_ty(p, x, &z), &subst_ty(q, y, &z))
        }
        _ => false,
    }
}
fn core_ty(env: &[(String, Ty)], e: &E) -> R<Ty> {
    match e {
        E::Int(_) => Ok(Ty::Int),
        E::Bool(_) => Ok(Ty::Bool),
        E::Var(x) => env
            .iter()
            .rev()
            .find(|(n, _)| n == x)
            .map(|(_, t)| t.clone())
            .ok_or(format!("unbound variable `{x}`")),
        E::Lam(x, Some(a), b) => {
            let mut env2 = env.to_vec();
            env2.push((x.clone(), a.clone()));
            Ok(fun(a.clone(), core_ty(&env2, b)?))
        }
        E::Lam(_, None, _) => Err("core System F needs a parameter annotation".into()),
        E::App(f, a) => match core_ty(env, f)? {
            Ty::Fun(p, r) => {
                let ta = core_ty(env, a)?;
                if sub_core(&ta, &p) {
                    Ok(*r)
                } else {
                    Err(format!(
                        "argument {} is not a subtype of {}",
                        show_ty(&ta),
                        show_ty(&p)
                    ))
                }
            }
            t => Err(format!("not a function: {}", show_ty(&t))),
        },
        E::TLam(a, b) => Ok(forall(a, core_ty(env, b)?)),
        E::TApp(f, t) => match core_ty(env, f)? {
            Ty::Forall(x, b) => Ok(subst_ty(&b, &x, t)),
            t => Err(format!("not polymorphic: {}", show_ty(&t))),
        },
        E::Ann(x, t) => {
            let tx = core_ty(env, x)?;
            if sub_core(&tx, t) {
                Ok(t.clone())
            } else {
                Err("annotation mismatch".into())
            }
        }
        E::Rec(fs) => Ok(Ty::Rec(
            fs.iter()
                .map(|(l, x)| Ok((l.clone(), core_ty(env, x)?)))
                .collect::<R<_>>()?,
        )),
        E::Proj(x, l) => match core_ty(env, x)? {
            Ty::Rec(m) => m.get(l).cloned().ok_or(format!("no field `{l}`")),
            t => Err(format!("not a record: {}", show_ty(&t))),
        },
        E::If(c, t, f) => {
            if core_ty(env, c)? != Ty::Bool {
                return Err("condition must be Bool".into());
            }
            let (tt, tf) = (core_ty(env, t)?, core_ty(env, f)?);
            if sub_core(&tf, &tt) {
                Ok(tt)
            } else if sub_core(&tt, &tf) {
                Ok(tf)
            } else {
                Err("branches have unrelated types".into())
            }
        }
        E::Inc(x) => {
            if core_ty(env, x)? == Ty::Int {
                Ok(Ty::Int)
            } else {
                Err("inc needs Int".into())
            }
        }
    }
}
fn is_val(e: &E) -> bool {
    match e {
        E::Int(_) | E::Bool(_) | E::Lam(..) | E::TLam(..) => true,
        E::Rec(fs) => fs.iter().all(|(_, x)| is_val(x)),
        _ => false,
    }
}
/// One call-by-value step (left to right). Does not reduce under `\` or `/\`.
fn step(e: &E) -> Option<E> {
    match e {
        E::App(f, a) => {
            if !is_val(f) {
                return step(f).map(|f2| eapp(f2, (**a).clone()));
            }
            if !is_val(a) {
                return step(a).map(|a2| eapp((**f).clone(), a2));
            }
            match &**f {
                E::Lam(x, _, b) => Some(subst_e(b, x, a)),
                _ => None,
            }
        }
        E::TApp(f, t) => match &**f {
            E::TLam(a, b) => Some(subst_ty_e(b, a, t)),
            _ => step(f).map(|f2| E::TApp(bx(f2), t.clone())),
        },
        E::Ann(x, t) => {
            if is_val(x) {
                Some((**x).clone())
            } else {
                step(x).map(|x2| E::Ann(bx(x2), t.clone()))
            }
        }
        E::Rec(fs) => {
            let i = fs.iter().position(|(_, x)| !is_val(x))?;
            let mut fs2 = fs.clone();
            fs2[i].1 = step(&fs[i].1)?;
            Some(E::Rec(fs2))
        }
        E::Proj(x, l) => match &**x {
            E::Rec(fs) if is_val(x) => fs.iter().find(|(n, _)| n == l).map(|(_, v)| v.clone()),
            _ => step(x).map(|x2| E::Proj(bx(x2), l.clone())),
        },
        E::If(c, t, f) => match &**c {
            E::Bool(b) => Some(if *b { (**t).clone() } else { (**f).clone() }),
            _ => step(c).map(|c2| E::If(bx(c2), t.clone(), f.clone())),
        },
        E::Inc(x) => match &**x {
            E::Int(n) => Some(E::Int(n + 1)),
            _ => step(x).map(|x2| E::Inc(bx(x2))),
        },
        _ => None,
    }
}
fn eval(e: &E, fuel: usize) -> R<E> {
    let mut cur = e.clone();
    for _ in 0..fuel {
        if is_val(&cur) {
            return Ok(cur);
        }
        cur = step(&cur).ok_or_else(|| format!("STUCK: {}", show_e(&cur)))?;
    }
    Err("out of fuel".into())
}

// ------------------------------------------------------------------ B. the bidirectional checker with an ordered context
#[derive(Clone, Debug)]
enum Item {
    Uni(String),
    Ex(usize),
    Solved(usize, Ty),
    Marker(usize),
    Var(String, Ty),
}
struct Dk {
    ctx: Vec<Item>,
    next: usize,
}
impl Dk {
    fn new() -> Self {
        Dk {
            ctx: vec![],
            next: 0,
        }
    }
    /// Open a scope. Positions in the context shift whenever an existential is split in place, so scopes are closed by
    /// FINDING their marker again, never by remembering a length.
    fn enter(&mut self) -> usize {
        let id = self.fresh();
        self.ctx.push(Item::Marker(id));
        id
    }
    fn marker_pos(&self, id: usize) -> usize {
        self.ctx
            .iter()
            .position(|i| matches!(i, Item::Marker(m) if *m == id))
            .expect("scope marker present")
    }
    fn leave(&mut self, id: usize) {
        let p = self.marker_pos(id);
        self.ctx.truncate(p);
    }
    fn fresh(&mut self) -> usize {
        self.next += 1;
        self.next
    }
    fn ex_pos(&self, n: usize) -> Option<usize> {
        self.ctx
            .iter()
            .position(|i| matches!(i, Item::Ex(m) | Item::Solved(m, _) if *m == n))
    }
    /// `[Gamma]A`: replace solved existentials by their solutions.
    fn apply(&self, t: &Ty) -> Ty {
        match t {
            Ty::Ex(n) => match self.ctx.iter().find_map(|i| match i {
                Item::Solved(m, s) if m == n => Some(s),
                _ => None,
            }) {
                Some(s) => self.apply(s),
                None => t.clone(),
            },
            Ty::Fun(a, b) => fun(self.apply(a), self.apply(b)),
            Ty::Forall(x, b) => Ty::Forall(x.clone(), Box::new(self.apply(b))),
            Ty::Rec(m) => Ty::Rec(m.iter().map(|(l, t)| (l.clone(), self.apply(t))).collect()),
            _ => t.clone(),
        }
    }
    fn wf(&self, t: &Ty) -> R<()> {
        let mut fs = BTreeSet::new();
        fv_ty(t, &mut fs);
        for x in fs {
            if !self
                .ctx
                .iter()
                .any(|i| matches!(i, Item::Uni(u) if *u == x))
            {
                return Err(format!("unbound type variable `{x}`"));
            }
        }
        Ok(())
    }
    /// Every existential and rigid variable of `t` must be declared before `^n`: this is what stops scope escape.
    fn wf_before(&self, n: usize, t: &Ty) -> bool {
        let Some(idx) = self.ex_pos(n) else {
            return false;
        };
        let mut ids = vec![];
        ex_ids(t, &mut ids);
        let mut fs = BTreeSet::new();
        fv_ty(t, &mut fs);
        ids.iter().all(|m| self.ex_pos(*m).is_some_and(|p| p < idx))
            && fs.iter().all(|x| {
                self.ctx
                    .iter()
                    .position(|i| matches!(i, Item::Uni(u) if u == x))
                    .is_some_and(|p| p < idx)
            })
    }
    fn solve(&mut self, n: usize, t: &Ty) -> R<()> {
        let idx = self.ex_pos(n).ok_or("unknown existential")?;
        if !matches!(self.ctx[idx], Item::Ex(_)) {
            return Err("existential already solved".into());
        }
        if !mono(t) {
            return Err(format!(
                "cannot instantiate ^{n} with the polymorphic type {} (inference is predicative)",
                show_ty(t)
            ));
        }
        if !self.wf_before(n, t) {
            return Err(format!(
                "cannot solve ^{n} := {}: it mentions a variable that is out of scope at ^{n} (escape)",
                show_norm(t)
            ));
        }
        self.ctx[idx] = Item::Solved(n, t.clone());
        Ok(())
    }
    /// Replace `^n` by `^n2, ^n1, ^n = ^n1 -> ^n2` in place (the new variables come right before it).
    fn split(&mut self, n: usize) -> R<(usize, usize)> {
        let idx = self.ex_pos(n).ok_or("unknown existential")?;
        if !matches!(self.ctx[idx], Item::Ex(_)) {
            return Err("existential already solved".into());
        }
        let (n1, n2) = (self.fresh(), self.fresh());
        self.ctx.splice(
            idx..=idx,
            [
                Item::Ex(n2),
                Item::Ex(n1),
                Item::Solved(n, fun(Ty::Ex(n1), Ty::Ex(n2))),
            ],
        );
        Ok((n1, n2))
    }
    fn occurs(&self, n: usize, t: &Ty) -> bool {
        let mut ids = vec![];
        ex_ids(&self.apply(t), &mut ids);
        ids.contains(&n)
    }
    fn escape_check(&self, pos: usize, t: &Ty) -> R<()> {
        let mut ids = vec![];
        ex_ids(&self.apply(t), &mut ids);
        if ids
            .iter()
            .any(|m| self.ex_pos(*m).is_some_and(|p| p >= pos))
        {
            return Err("the result type mentions an unknown created inside its scope; add a type annotation".into());
        }
        Ok(())
    }
    /// Subtyping `a <: b`, updating the context with existential solutions.
    fn sub(&mut self, a: &Ty, b: &Ty) -> R<()> {
        let (a, b) = (self.apply(a), self.apply(b));
        match (&a, &b) {
            (_, Ty::Top) | (Ty::Int, Ty::Int) | (Ty::Bool, Ty::Bool) => Ok(()),
            (Ty::Var(x), Ty::Var(y)) if x == y => Ok(()),
            (Ty::Ex(x), Ty::Ex(y)) if x == y => Ok(()),
            (_, Ty::Forall(y, body)) => {
                let u = fresh_name(y);
                let pos = self.enter();
                self.ctx.push(Item::Uni(u.clone()));
                self.sub(&a, &subst_ty(body, y, &Ty::Var(u)))?;
                self.leave(pos);
                Ok(())
            }
            (Ty::Forall(x, body), _) => {
                let e = self.fresh();
                let pos = self.enter();
                self.ctx.push(Item::Ex(e));
                self.sub(&subst_ty(body, x, &Ty::Ex(e)), &b)?;
                self.leave(pos);
                Ok(())
            }
            (Ty::Fun(a1, a2), Ty::Fun(b1, b2)) => {
                self.sub(b1, a1)?;
                self.sub(a2, b2)
            }
            (Ty::Rec(fa), Ty::Rec(fb)) => {
                for (l, tb) in fb {
                    match fa.get(l) {
                        None => {
                            return Err(format!(
                                "missing field `{l}`: {} is not a subtype of {}",
                                show_norm(&a),
                                show_norm(&b)
                            ));
                        }
                        Some(ta) => self.sub(ta, tb)?,
                    }
                }
                Ok(())
            }
            (Ty::Ex(x), _) => {
                if self.occurs(*x, &b) {
                    return Err(format!("occurs check: ^{x} occurs in {}", show_norm(&b)));
                }
                self.inst(*x, &b, true)
            }
            (_, Ty::Ex(y)) => {
                if self.occurs(*y, &a) {
                    return Err(format!("occurs check: ^{y} occurs in {}", show_norm(&a)));
                }
                self.inst(*y, &a, false)
            }
            _ => Err(format!(
                "type mismatch: {} is not a subtype of {}",
                show_norm(&a),
                show_norm(&b)
            )),
        }
    }
    /// `^n :=< t` (left = true, meaning ^n <: t) or `t :=< ^n` (left = false, meaning t <: ^n).
    fn inst(&mut self, n: usize, t: &Ty, left: bool) -> R<()> {
        let t = self.apply(t);
        if let Ty::Ex(m) = &t {
            if *m == n {
                return Ok(());
            }
            // reach: solve whichever of the two is declared later in terms of the earlier one
            let (pn, pm) = (
                self.ex_pos(n).ok_or("unknown")?,
                self.ex_pos(*m).ok_or("unknown")?,
            );
            return if pm > pn {
                self.solve(*m, &Ty::Ex(n))
            } else {
                self.solve(n, &t)
            };
        }
        if mono(&t) && self.wf_before(n, &t) {
            return self.solve(n, &t);
        }
        match &t {
            Ty::Fun(a1, a2) => {
                let (n1, n2) = self.split(n)?;
                // the argument flips polarity, the result keeps it
                self.inst(n1, a1, !left)?;
                let a2 = self.apply(a2);
                self.inst(n2, &a2, left)
            }
            Ty::Forall(b, body) if left => {
                let u = fresh_name(b);
                let pos = self.enter();
                self.ctx.push(Item::Uni(u.clone()));
                self.inst(n, &subst_ty(body, b, &Ty::Var(u)), true)?;
                self.leave(pos);
                Ok(())
            }
            Ty::Forall(b, body) => {
                let e = self.fresh();
                let pos = self.enter();
                self.ctx.push(Item::Ex(e));
                self.inst(n, &subst_ty(body, b, &Ty::Ex(e)), false)?;
                self.leave(pos);
                Ok(())
            }
            Ty::Rec(_) => Err(format!(
                "cannot instantiate ^{n} with a record containing polymorphic fields: {}",
                show_norm(&t)
            )),
            _ => Err(format!(
                "cannot instantiate ^{n} with {}: a variable would escape its scope",
                show_norm(&t)
            )),
        }
    }
    fn check(&mut self, e: &E, a: &Ty) -> R<()> {
        let a = self.apply(a);
        if let Ty::Forall(y, body) = &a {
            let u = fresh_name(y);
            let pos = self.enter();
            self.ctx.push(Item::Uni(u.clone()));
            let e2 = match e {
                E::TLam(x, b) => subst_ty_e(b, x, &Ty::Var(u.clone())), // explicit `/\x.` names the rigid variable
                _ => e.clone(),
            };
            self.check(&e2, &subst_ty(body, y, &Ty::Var(u)))?;
            self.leave(pos);
            return Ok(());
        }
        match (e, &a) {
            (E::Lam(x, None, b), Ty::Fun(p, r)) => {
                let pos = self.enter();
                self.ctx.push(Item::Var(x.clone(), (**p).clone()));
                self.check(b, r)?;
                self.leave(pos);
                Ok(())
            }
            (E::Lam(x, None, b), Ty::Ex(n)) => {
                let (n1, n2) = self.split(*n)?;
                let pos = self.enter();
                self.ctx.push(Item::Var(x.clone(), Ty::Ex(n1)));
                self.check(b, &Ty::Ex(n2))?;
                self.leave(pos);
                Ok(())
            }
            (E::If(c, t, f), _) => {
                self.check(c, &Ty::Bool)?;
                self.check(t, &a)?;
                self.check(f, &a)
            }
            (E::Rec(fs), Ty::Rec(ft))
                if fs.len() == ft.len() && fs.iter().all(|(l, _)| ft.contains_key(l)) =>
            {
                for (l, x) in fs {
                    self.check(x, &ft[l])?;
                }
                Ok(())
            }
            _ => {
                let t = self.synth(e)?;
                let t = self.apply(&t);
                self.sub(&t, &a)
            }
        }
    }
    fn synth(&mut self, e: &E) -> R<Ty> {
        match e {
            E::Int(_) => Ok(Ty::Int),
            E::Bool(_) => Ok(Ty::Bool),
            E::Var(x) => self
                .ctx
                .iter()
                .rev()
                .find_map(|i| match i {
                    Item::Var(n, t) if n == x => Some(self.apply(t)),
                    _ => None,
                })
                .ok_or(format!("unbound variable `{x}`")),
            E::Ann(x, t) => {
                self.wf(t)?;
                self.check(x, t)?;
                Ok(t.clone())
            }
            E::Lam(x, None, b) => {
                let (n1, n2) = (self.fresh(), self.fresh());
                self.ctx.push(Item::Ex(n1));
                self.ctx.push(Item::Ex(n2));
                let pos = self.enter();
                self.ctx.push(Item::Var(x.clone(), Ty::Ex(n1)));
                self.check(b, &Ty::Ex(n2))?;
                self.leave(pos);
                Ok(fun(Ty::Ex(n1), Ty::Ex(n2)))
            }
            E::Lam(x, Some(a), b) => {
                self.wf(a)?;
                let pos = self.enter();
                self.ctx.push(Item::Var(x.clone(), a.clone()));
                let tb = self.synth(b)?;
                let tb = self.apply(&tb);
                self.escape_check(self.marker_pos(pos), &tb)?;
                self.leave(pos);
                Ok(fun(a.clone(), tb))
            }
            E::App(f, x) => {
                let tf = self.synth(f)?;
                let tf = self.apply(&tf);
                self.app_synth(&tf, x)
            }
            E::TLam(a, b) => {
                let u = fresh_name(a);
                let pos = self.enter();
                self.ctx.push(Item::Uni(u.clone()));
                let tb = self.synth(&subst_ty_e(b, a, &Ty::Var(u.clone())))?;
                let tb = self.apply(&tb);
                self.escape_check(self.marker_pos(pos), &tb)?;
                self.leave(pos);
                Ok(forall(a, subst_ty(&tb, &u, &Ty::Var(a.clone()))))
            }
            E::TApp(f, t) => {
                self.wf(t)?;
                let tf = self.synth(f)?;
                match self.apply(&tf) {
                    Ty::Forall(x, b) => Ok(subst_ty(&b, &x, t)),
                    other => Err(format!(
                        "cannot apply a type argument to {}",
                        show_norm(&other)
                    )),
                }
            }
            E::Rec(fs) => {
                let mut m = BTreeMap::new();
                for (l, x) in fs {
                    let t = self.synth(x)?;
                    m.insert(l.clone(), self.apply(&t));
                }
                Ok(Ty::Rec(m))
            }
            E::Proj(x, l) => {
                let t = self.synth(x)?;
                match self.apply(&t) {
                    Ty::Rec(m) => m
                        .get(l)
                        .cloned()
                        .ok_or(format!("no field `{l}` in {}", show_norm(&Ty::Rec(m)))),
                    Ty::Ex(_) => Err(format!(
                        "cannot project `.{l}` from a value of unknown type; annotate the parameter"
                    )),
                    other => Err(format!("cannot project `.{l}` from {}", show_norm(&other))),
                }
            }
            E::If(c, t, f) => {
                self.check(c, &Ty::Bool)?;
                let tt = self.synth(t)?;
                let tt = self.apply(&tt);
                self.check(f, &tt)?;
                Ok(tt)
            }
            E::Inc(x) => {
                self.check(x, &Ty::Int)?;
                Ok(Ty::Int)
            }
        }
    }
    /// `A . e =>> C`: what is the result of applying something of type `A` to `e`?
    fn app_synth(&mut self, a: &Ty, e: &E) -> R<Ty> {
        match self.apply(a) {
            Ty::Forall(x, b) => {
                let n = self.fresh();
                self.ctx.push(Item::Ex(n));
                self.app_synth(&subst_ty(&b, &x, &Ty::Ex(n)), e)
            }
            Ty::Ex(n) => {
                let (n1, n2) = self.split(n)?;
                self.check(e, &Ty::Ex(n1))?;
                Ok(Ty::Ex(n2))
            }
            Ty::Fun(p, r) => {
                self.check(e, &p)?;
                Ok(*r)
            }
            other => Err(format!(
                "not a function: this has type {}",
                show_norm(&other)
            )),
        }
    }
}
fn dk_synth(e: &E) -> R<Ty> {
    let mut d = Dk::new();
    let t = d.synth(e)?;
    Ok(d.apply(&t))
}
fn dk_check(e: &E, t: &Ty) -> R<()> {
    let mut d = Dk::new();
    d.wf(t)?;
    d.check(e, t)
}
fn dk_sub(a: &Ty, b: &Ty) -> bool {
    Dk::new().sub(a, b).is_ok()
}

// ------------------------------------------------------------------ type-directed generation
struct Gen<'a> {
    r: &'a mut Rng,
    n: usize,
}
const LABELS: [&str; 4] = ["l", "m", "p", "q"];
impl<'a> Gen<'a> {
    fn name(&mut self, p: &str) -> String {
        self.n += 1;
        format!("{p}{}", self.n)
    }
    fn gen_ty(&mut self, d: usize, tvs: &mut Vec<String>, poly: bool) -> Ty {
        let k = if d == 0 {
            self.r.below(4)
        } else {
            self.r.below(10)
        };
        match k {
            0 => Ty::Int,
            1 => Ty::Bool,
            2 => {
                if tvs.is_empty() {
                    Ty::Int
                } else {
                    tvar(&tvs[self.r.below(tvs.len())].clone())
                }
            }
            3 => {
                if self.r.below(3) == 0 {
                    Ty::Top
                } else {
                    Ty::Int
                }
            }
            4..=6 => fun(self.gen_ty(d - 1, tvs, poly), self.gen_ty(d - 1, tvs, poly)),
            7 | 8 => {
                let mut m = BTreeMap::new();
                for _ in 0..1 + self.r.below(3) {
                    let l = LABELS[self.r.below(4)];
                    let t = self.gen_ty(d - 1, tvs, poly);
                    m.insert(l.to_string(), t);
                }
                Ty::Rec(m)
            }
            _ if poly => {
                let a = self.name("a");
                tvs.push(a.clone());
                let b = self.gen_ty(d - 1, tvs, poly);
                tvs.pop();
                forall(&a, b)
            }
            _ => Ty::Int,
        }
    }
    fn closed_ty(&mut self, d: usize, poly: bool) -> Ty {
        self.gen_ty(d, &mut vec![], poly)
    }
    fn gen_term(&mut self, t: &Ty, env: &[(String, Ty)], d: usize) -> Option<E> {
        for _ in 0..4 {
            let k = self.r.below(9);
            if let Some(e) = self.option(k, t, env, d) {
                return Some(e);
            }
        }
        self.intro(t, env, d)
    }
    fn option(&mut self, k: usize, t: &Ty, env: &[(String, Ty)], d: usize) -> Option<E> {
        match k {
            0 => {
                let c: Vec<&(String, Ty)> = env.iter().filter(|(_, ty)| sub_core(ty, t)).collect();
                (!c.is_empty()).then(|| evar(&c[self.r.below(c.len())].0))
            }
            _ if d == 0 => None,
            1 => {
                // (\x:T. body) arg : an explicit redex whose argument has the exact parameter type
                let pt = self.closed_ty(1, false);
                let x = self.name("x");
                let mut env2 = env.to_vec();
                env2.push((x.clone(), pt.clone()));
                let body = self.gen_term(t, &env2, d - 1)?;
                Some(eapp(
                    elam(&x, Some(pt.clone()), body),
                    self.gen_term(&pt, env, d - 1)?,
                ))
            }
            2 => {
                // polymorphic identity instantiated at the target type
                let a = self.name("a");
                let id = E::TLam(a.clone(), bx(elam("i", Some(tvar(&a)), evar("i"))));
                Some(eapp(
                    E::TApp(bx(id), t.clone()),
                    self.gen_term(t, env, d - 1)?,
                ))
            }
            3 => {
                let (a, b) = (self.name("a"), self.name("b"));
                let k = E::TLam(
                    a.clone(),
                    bx(E::TLam(
                        b.clone(),
                        bx(elam(
                            "u",
                            Some(tvar(&a)),
                            elam("v", Some(tvar(&b)), evar("u")),
                        )),
                    )),
                );
                let other = self.closed_ty(1, false);
                let ke = E::TApp(bx(E::TApp(bx(k), t.clone())), other.clone());
                Some(eapp(
                    eapp(ke, self.gen_term(t, env, d - 1)?),
                    self.gen_term(&other, env, d - 1)?,
                ))
            }
            4 => {
                // branches are annotated with the target so they are comparable even if their exact types differ (extra fields)
                let ann = |e: E| E::Ann(bx(e), t.clone());
                Some(E::If(
                    bx(self.gen_term(&Ty::Bool, env, d - 1)?),
                    bx(ann(self.gen_term(t, env, d - 1)?)),
                    bx(ann(self.gen_term(t, env, d - 1)?)),
                ))
            }
            5 => {
                let l = LABELS[self.r.below(4)];
                let rt = rec_ty(vec![(l, t.clone()), ("z", Ty::Int)]);
                Some(E::Proj(bx(self.gen_term(&rt, env, d - 1)?), l.to_string()))
            }
            6 => {
                let c: Vec<&(String, Ty)> = env
                    .iter()
                    .filter(|(_, ty)| matches!(ty, Ty::Fun(_, r) if sub_core(r, t)))
                    .collect();
                if c.is_empty() {
                    return None;
                }
                let (f, ft) = c[self.r.below(c.len())].clone();
                let Ty::Fun(p, _) = ft else { return None };
                Some(eapp(evar(&f), self.gen_term(&p, env, d - 1)?))
            }
            7 if *t == Ty::Int => Some(eapp(prim_inc(), self.gen_term(&Ty::Int, env, d - 1)?)),
            _ => None,
        }
    }
    fn intro(&mut self, t: &Ty, env: &[(String, Ty)], d: usize) -> Option<E> {
        match t {
            Ty::Int | Ty::Top => Some(E::Int(self.r.below(10) as i64)),
            Ty::Bool => Some(E::Bool(self.r.below(2) == 0)),
            Ty::Fun(a, b) => {
                let x = self.name("x");
                let mut env2 = env.to_vec();
                env2.push((x.clone(), (**a).clone()));
                Some(elam(
                    &x,
                    Some((**a).clone()),
                    self.gen_term(b, &env2, d.saturating_sub(1))?,
                ))
            }
            Ty::Forall(a, b) => Some(E::TLam(
                a.clone(),
                bx(self.gen_term(b, env, d.saturating_sub(1))?),
            )),
            Ty::Rec(m) => {
                let mut fs = vec![];
                for (l, ft) in m {
                    fs.push((l.clone(), self.gen_term(ft, env, d.saturating_sub(1))?));
                }
                if !m.contains_key("z") && self.r.below(3) == 0 {
                    fs.push(("z".into(), E::Int(7))); // width: an extra field the type does not mention
                }
                Some(E::Rec(fs))
            }
            Ty::Var(_) | Ty::Ex(_) => None,
        }
    }
    /// A random type together with an inhabitant. Some random types are EMPTY (`forall a. a` has no closed term, which is
    /// parametricity again), so keep drawing until the generator finds a term.
    fn typed_term(&mut self, ty_depth: usize, poly: bool, d: usize) -> (Ty, E) {
        loop {
            let t = self.closed_ty(ty_depth, poly);
            for _ in 0..8 {
                if let Some(e) = self.gen_term(&t, &[], d) {
                    return (t, e);
                }
            }
        }
    }
    fn closed(&mut self, t: &Ty, d: usize) -> E {
        for _ in 0..50 {
            if let Some(e) = self.gen_term(t, &[], d) {
                return e;
            }
        }
        panic!("generator failed for {}", show_ty(t));
    }
    fn super_of(&mut self, t: &Ty) -> Ty {
        match t {
            Ty::Int | Ty::Bool => {
                if self.r.below(3) == 0 {
                    Ty::Top
                } else {
                    t.clone()
                }
            }
            Ty::Fun(a, b) => {
                if self.r.below(6) == 0 {
                    Ty::Top
                } else {
                    fun(self.sub_of(a), self.super_of(b))
                }
            }
            Ty::Rec(m) => {
                if self.r.below(8) == 0 {
                    return Ty::Top;
                }
                let mut out = BTreeMap::new();
                for (l, ft) in m {
                    if self.r.below(4) != 0 {
                        out.insert(l.clone(), self.super_of(ft));
                    }
                }
                Ty::Rec(out)
            }
            _ => t.clone(),
        }
    }
    fn sub_of(&mut self, t: &Ty) -> Ty {
        match t {
            Ty::Top => self.closed_ty(2, false),
            Ty::Fun(a, b) => fun(self.super_of(a), self.sub_of(b)),
            Ty::Rec(m) => {
                let mut out: BTreeMap<String, Ty> = m
                    .iter()
                    .map(|(l, ft)| (l.clone(), self.sub_of(ft)))
                    .collect();
                for l in ["p", "q", "r"] {
                    if !out.contains_key(l) && self.r.below(3) == 0 {
                        out.insert(l.into(), self.closed_ty(1, false));
                    }
                }
                Ty::Rec(out)
            }
            _ => t.clone(),
        }
    }
}

/// Erase annotations the bidirectional checker does not need; keep them where a term is SYNTHESISED.
fn erase(env: &[(String, Ty)], e: &E) -> E {
    let ann = |x: &E| {
        let t = core_ty(env, x).expect("source term is well typed");
        match x {
            E::Var(_) => erase(env, x),
            _ => E::Ann(bx(erase(env, x)), t),
        }
    };
    match e {
        E::Lam(x, Some(a), b) if *e != prim_inc() => {
            let mut env2 = env.to_vec();
            env2.push((x.clone(), a.clone()));
            E::Lam(x.clone(), None, bx(erase(&env2, b)))
        }
        E::App(f, a) => eapp(ann(f), erase(env, a)),
        E::TLam(a, b) => E::TLam(a.clone(), bx(erase(env, b))),
        E::Ann(x, t) => E::Ann(bx(erase(env, x)), t.clone()),
        E::TApp(f, t) => E::TApp(bx(ann(f)), t.clone()),
        E::Rec(fs) => E::Ann(
            bx(E::Rec(
                fs.iter().map(|(l, x)| (l.clone(), erase(env, x))).collect(),
            )),
            core_ty(env, e).unwrap(),
        ),
        E::Proj(x, l) => E::Proj(bx(ann(x)), l.clone()),
        E::If(c, t, f) => E::If(bx(erase(env, c)), bx(erase(env, t)), bx(erase(env, f))),
        _ => e.clone(),
    }
}

// ------------------------------------------------------------------ tests
fn demo_and_suites() {
    println!("-- accepted programs (synthesis or checking)");
    let accept: &[(&str, &str)] = &[
        (r"\x:Int. x", "Int -> Int"),
        (r"/\a. \x:a. x", "forall a. a -> a"),
        (r"(/\a. \x:a. x)[Int] 3", "Int"),
        (r"\x. x", "^a -> ^a"),
        (r"(\x. x) 1", "Int"),
        (r"inc ((\x. x) 41)", "Int"),
        (r"({a = 1, b = true}).a", "Int"),
        (r"(\r:{a:Int}. r.a) {a = 1, b = true}", "Int"),
        (r"((\f. f 1) : (forall a. a -> a) -> Int) (\x. x)", "Int"),
        (
            r"((\f. if f true then f 1 else 2) : (forall a. a -> a) -> Int) (\x. x)",
            "Int",
        ),
        (r"(\f. f 1) inc", "Int"),
        (r"if true then {a = 1} else {a = 2}", "{a: Int}"),
    ];
    for (src, want) in accept {
        let got = dk_synth(&pe(src)).unwrap_or_else(|e| panic!("should accept `{src}`: {e}"));
        println!("  {src:<68} : {}", show_norm(&got));
        assert_eq!(&show_norm(&got), want, "type of `{src}`");
    }
    // checking mode: the annotation does the work
    for (src, ty) in [
        (r"\x. x", "forall a. a -> a"),
        (r"\x. \y. x", "forall a. forall b. a -> b -> a"),
        (r"\f. \x. f (f x)", "forall a. (a -> a) -> a -> a"),
        (r"\r. r.a", "{a: Int} -> Int"),
        (r"\f. f 1", "(forall a. a -> a) -> Int"),
        (r"{a = \x. x, b = 2}", "{a: forall t. t -> t, b: Int}"),
        (r"\x. x", "Top -> Top"),
        (r"\x. {a = x, b = 1}", "Int -> {a: Int}"),
        (r"inc", "Int -> Top"),
    ] {
        dk_check(&pe(src), &pty(ty))
            .unwrap_or_else(|e| panic!("should check `{src}` against {ty}: {e}"));
    }
    println!("-- rejected programs (with the expected message fragment)");
    let reject: &[(&str, &str)] = &[
        (r"\x. x x", "occurs check"),
        (r"1 2", "not a function"),
        (r"(\x. x) 1 2", "not a function"),
        (r"inc true", "Bool is not a subtype of Int"),
        (r"{a = 1}.b", "no field `b`"),
        (r"(/\a. \x:a. x)[Int] true", "Bool is not a subtype of Int"),
        (r"(\x. x : forall a. a -> Int)", "not a subtype"),
        (
            r"((\x:Int. x) : Int -> Bool)",
            "Int is not a subtype of Bool",
        ),
        (r"y", "unbound variable `y`"),
        (r"(1 : b)", "unbound type variable `b`"),
        (r"if 1 then 2 else 3", "Int is not a subtype of Bool"),
        (r"if true then 1 else false", "Bool is not a subtype of Int"),
        (r"({a = 1} : {a: Int, b: Int})", "missing field `b`"),
        (
            r"\x. x.a",
            "cannot project `.a` from a value of unknown type",
        ),
        (r"((\x:Int. x) : forall a. a -> a)", "not a subtype"),
        (r"(\f. if f true then f 1 else 2) (\x. x)", "not a subtype"),
        (r"((\x:Int. x) : Int -> Int -> Int)", "not a subtype"),
        (r"3 [Int]", "cannot apply a type argument"),
        (r"(\x:{a:Int}. x.b)", "no field `b`"),
        (r"(\f:Top -> Int. f) (\x:Int. x)", "not a subtype"),
    ];
    for (src, frag) in reject {
        let err = match dk_synth(&pe(src)) {
            Ok(t) => panic!("`{src}` must be rejected but got {}", show_norm(&t)),
            Err(e) => e,
        };
        println!("  {src:<48} => {err}");
        assert!(
            err.contains(frag),
            "`{src}`: expected message containing `{frag}`, got `{err}`"
        );
    }
    println!(
        "  ({} accepted, {} rejected with the expected message)",
        accept.len(),
        reject.len()
    );
    // the scope-escape rule: a rigid variable can never leak into an outer existential
    let e = dk_check(&pe(r"\f. \x. f x"), &pty("forall a. (a -> a) -> a -> a"));
    assert!(e.is_ok());
    let leak = Dk::new().sub(&pty("forall a. a -> a"), &pty("Int -> Bool"));
    assert!(leak.is_err());
}

fn test_parse_roundtrip(r: &mut Rng) {
    let mut g = Gen { r, n: 0 };
    for _ in 0..400 {
        let (t, e) = g.typed_term(3, true, 3);
        assert_eq!(
            parse_ty(&show_ty(&t)).unwrap(),
            t,
            "type round trip {}",
            show_ty(&t)
        );
        assert_eq!(
            parse(&show_e(&e)).unwrap(),
            e,
            "term round trip {}",
            show_e(&e)
        );
        let s = erase(&[], &e);
        assert_eq!(
            parse(&show_e(&s)).unwrap(),
            s,
            "erased term round trip {}",
            show_e(&s)
        );
    }
    println!("-- parser and printer round trip: 400 random types, terms, erased terms");
}

fn test_subtyping(r: &mut Rng) {
    // hand-written facts: width, depth, variance
    let yes = [
        ("{a: Int, b: Bool}", "{a: Int}"),
        ("{a: {x: Int, y: Int}}", "{a: {x: Int}}"),
        ("Int", "Top"),
        ("Top -> Int", "Int -> Int"),
        ("Int -> {a: Int, b: Int}", "Int -> {a: Int}"),
        ("{a: Int} -> Int", "{a: Int, b: Int} -> Top"),
        ("forall a. a -> a", "forall b. b -> b"),
        (
            "({a: Int, b: Int} -> Int) -> Int",
            "({a: Int} -> Int) -> Int",
        ),
    ];
    let no = [
        ("{a: Int}", "{a: Int, b: Bool}"),
        ("Int -> Int", "Top -> Int"),
        ("Int -> Int", "Int -> Bool"),
        ("{a: Int}", "{a: Bool}"),
        ("Top", "Int"),
        (
            "({a: Int} -> Int) -> Int",
            "({a: Int, b: Int} -> Int) -> Int",
        ),
        ("forall a. a -> a", "forall a. a -> Int"),
    ];
    for (a, b) in yes {
        assert!(
            sub_core(&pty(a), &pty(b)) && dk_sub(&pty(a), &pty(b)),
            "{a} <: {b}"
        );
    }
    for (a, b) in no {
        assert!(
            !sub_core(&pty(a), &pty(b)) && !dk_sub(&pty(a), &pty(b)),
            "{a} must NOT be <: {b}"
        );
    }
    // the bidirectional checker is strictly more powerful on quantifiers: it can instantiate on the left
    assert!(
        dk_sub(&pty("forall a. a -> a"), &pty("Int -> Int"))
            && !sub_core(&pty("forall a. a -> a"), &pty("Int -> Int"))
    );
    assert!(
        dk_sub(&pty("Int -> Int"), &pty("Int -> Int"))
            && !dk_sub(&pty("Int -> Int"), &pty("forall a. a -> a"))
    );
    // random properties
    let mut g = Gen { r, n: 0 };
    let (mut refl, mut trans, mut chains) = (0, 0, 0);
    for _ in 0..3000 {
        let a = g.closed_ty(3, false);
        assert!(
            sub_core(&a, &a) && dk_sub(&a, &a),
            "reflexivity: {}",
            show_ty(&a)
        );
        refl += 1;
        let up = g.super_of(&a);
        let down = g.sub_of(&a);
        assert!(
            sub_core(&a, &up) && dk_sub(&a, &up),
            "{} <: {}",
            show_ty(&a),
            show_ty(&up)
        );
        assert!(
            sub_core(&down, &a) && dk_sub(&down, &a),
            "{} <: {}",
            show_ty(&down),
            show_ty(&a)
        );
        // transitivity along a chain down <: a <: up, and the two checkers agree on random unrelated pairs
        assert!(sub_core(&down, &up));
        chains += 1;
        let other = g.closed_ty(2, false);
        let (x, y) = (sub_core(&a, &other), dk_sub(&a, &other));
        assert_eq!(
            x,
            y,
            "core and bidirectional subtyping disagree on {} <: {}",
            show_ty(&a),
            show_ty(&other)
        );
        if sub_core(&other, &a) && x {
            trans += 1;
            assert!(sub_core(&other, &other)); // antisymmetry up to equivalence would be checked here
        }
    }
    println!(
        "-- subtyping: {refl} reflexive, {chains} generated chains sub <: t <: super, both checkers agree; mutual-subtype pairs: {trans}"
    );
}

fn test_preservation(r: &mut Rng) {
    let mut g = Gen { r, n: 0 };
    let (mut terms, mut steps, mut shrunk) = (0, 0, 0);
    for i in 0..3000 {
        let (t, e) = g.typed_term(2 + i % 3, true, 4);
        let mut ty = core_ty(&[], &e).unwrap_or_else(|er| {
            panic!("generator produced an ill-typed term {}: {er}", show_e(&e))
        });
        assert!(
            sub_core(&ty, &t),
            "{} should have type <: {}",
            show_e(&e),
            show_ty(&t)
        );
        let mut cur = e.clone();
        let mut n = 0;
        while !is_val(&cur) {
            let next = step(&cur)
                .unwrap_or_else(|| panic!("PROGRESS violated: stuck at {}", show_e(&cur)));
            let t2 = core_ty(&[], &next).unwrap_or_else(|er| {
                panic!(
                    "PRESERVATION violated: {} -> {}: {er}",
                    show_e(&cur),
                    show_e(&next)
                )
            });
            assert!(
                sub_core(&t2, &ty),
                "type must not grow: {} -> {}",
                show_ty(&ty),
                show_ty(&t2)
            );
            shrunk += usize::from(!ty_eq(&t2, &ty));
            ty = t2;
            cur = next;
            n += 1;
            assert!(n < 5000, "no normal form");
        }
        steps += n;
        terms += 1;
    }
    println!(
        "-- progress + preservation: {terms} generated terms, {steps} reduction steps, type got strictly smaller (subsumption) {shrunk} times"
    );
    assert!(steps > 3000);
}

fn test_differential(r: &mut Rng) {
    let mut g = Gen { r, n: 0 };
    let (mut ok, mut annotated) = (0, 0);
    for i in 0..1500 {
        let (_, e) = g.typed_term(2 + i % 3, true, 4);
        let core = core_ty(&[], &e).unwrap();
        let surface = erase(&[], &e);
        if let Err(msg) = dk_check(&surface, &core) {
            panic!(
                "bidirectional checker rejected an erased well-typed term\n  core:    {}\n  surface: {}\n  type:    {}\n  error:   {msg}",
                show_e(&e),
                show_e(&surface),
                show_ty(&core)
            );
        }
        ok += 1;
        // fully annotated terms also synthesise, to something equivalent
        let syn = dk_synth(&e)
            .unwrap_or_else(|m| panic!("synthesis failed on annotated {}: {m}", show_e(&e)));
        assert!(
            sub_core(&syn, &core) && sub_core(&core, &syn) || dk_sub(&syn, &core),
            "{} vs {}",
            show_ty(&syn),
            show_ty(&core)
        );
        annotated += 1;
    }
    println!(
        "-- differential: {ok} erased terms accepted at their core type, {annotated} annotated terms synthesise an equivalent type"
    );
}

/// Apply an expression of type `forall a. a -> a` etc. and evaluate to a value (the closed program is well-typed, so it terminates).
fn run_app(f: &E, targs: &[Ty], args: &[E]) -> E {
    let mut e = f.clone();
    for t in targs {
        e = E::TApp(bx(e), t.clone());
    }
    for a in args {
        e = eapp(e, a.clone());
    }
    eval(&e, 100_000).unwrap_or_else(|m| panic!("{m}"))
}
fn test_free_theorems(r: &mut Rng) {
    let mut g = Gen { r, n: 0 };
    let samples = [
        E::Int(5),
        E::Bool(true),
        pe("{l = 3}"),
        pe(r"\x:Int. inc x"),
    ];
    let sample_tys = [Ty::Int, Ty::Bool, pty("{l: Int}"), pty("Int -> Int")];
    let (mut ids, mut consts, mut pick, mut church) = (0, 0, 0, 0);
    for _ in 0..300 {
        // forall a. a -> a : every inhabitant is the identity
        let e = g.closed(&pty("forall a. a -> a"), 4);
        for (t, v) in sample_tys.iter().zip(&samples) {
            assert_eq!(
                &run_app(&e, &[t.clone()], &[v.clone()]),
                v,
                "{} is not the identity",
                show_e(&e)
            );
        }
        ids += 1;
        // forall a b. a -> b -> a : const
        let e = g.closed(&pty("forall a. forall b. a -> b -> a"), 4);
        let out = run_app(&e, &[Ty::Int, Ty::Bool], &[E::Int(1), E::Bool(false)]);
        assert_eq!(out, E::Int(1), "{} is not const", show_e(&e));
        consts += 1;
        // forall a. a -> a -> a : returns one of its arguments, and for Int it is one of the two given numbers
        let e = g.closed(&pty("forall a. a -> a -> a"), 4);
        let out = run_app(&e, &[Ty::Int], &[E::Int(10), E::Int(20)]);
        assert!(
            out == E::Int(10) || out == E::Int(20),
            "{} invented a value: {}",
            show_e(&e),
            show_e(&out)
        );
        pick += 1;
        // forall a. (a -> a) -> a -> a : naturality. f^n x computed with inc (from 0) and with a function that does inc twice
        let e = g.closed(&pty("forall a. (a -> a) -> a -> a"), 5);
        let once = run_app(&e, &[Ty::Int], &[prim_inc(), E::Int(0)]);
        let twice = run_app(&e, &[Ty::Int], &[pe(r"\y:Int. inc (inc y)"), E::Int(0)]);
        let (E::Int(a), E::Int(b)) = (&once, &twice) else {
            panic!("not numbers")
        };
        assert_eq!(*b, 2 * *a, "{} breaks parametricity", show_e(&e));
        let at_bool = run_app(&e, &[Ty::Bool], &[pe(r"\b:Bool. b"), E::Bool(true)]);
        assert_eq!(at_bool, E::Bool(true));
        church += 1;
    }
    println!(
        "-- free theorems: {ids} identities, {consts} consts, {pick} pickers, {church} Church numerals behave parametrically"
    );
}

fn liskov_use(g: &mut Gen, v: &E, ty: &Ty, depth: usize) -> usize {
    if depth == 0 {
        return 0;
    }
    let mut uses = 0;
    match ty {
        Ty::Rec(m) => {
            for (l, ft) in m {
                let p = E::Proj(bx(v.clone()), l.clone());
                let out = eval(&p, 10_000).unwrap_or_else(|e| {
                    panic!(
                        "projection `{l}` from a value used at {} got stuck: {e}",
                        show_ty(ty)
                    )
                });
                let got = core_ty(&[], &out).unwrap();
                assert!(
                    sub_core(&got, ft),
                    "field `{l}` promised {} but is {}",
                    show_ty(ft),
                    show_ty(&got)
                );
                uses += 1 + liskov_use(g, &out, ft, depth - 1);
            }
        }
        Ty::Fun(a, b) => {
            let arg = g.closed(a, 3);
            let out = eval(&eapp(v.clone(), arg), 100_000)
                .unwrap_or_else(|e| panic!("call through {} got stuck: {e}", show_ty(ty)));
            let got = core_ty(&[], &out).unwrap();
            assert!(
                sub_core(&got, b),
                "result promised {} but is {}",
                show_ty(b),
                show_ty(&got)
            );
            uses += 1 + liskov_use(g, &out, b, depth - 1);
        }
        _ => {}
    }
    uses
}
fn test_liskov(r: &mut Rng) {
    let mut g = Gen { r, n: 0 };
    let (mut cases, mut uses) = (0, 0);
    for i in 0..1500 {
        let sup = g.closed_ty(2 + i % 2, false);
        let sub = g.sub_of(&sup);
        assert!(sub_core(&sub, &sup));
        let v = eval(&g.closed(&sub, 3), 100_000).unwrap();
        // substitutability: the value of the SUBtype is used strictly through the SUPERtype
        uses += liskov_use(&mut g, &v, &sup, 3);
        cases += 1;
    }
    println!(
        "-- Liskov substitutability: {cases} (subtype value, supertype use) pairs, {uses} field reads/calls, none stuck, all results at the promised type"
    );
    // and why contravariance matters: a function that needs a field cannot stand in for one that is offered less
    let f = pe(r"\r:{a:Int, b:Int}. r.b");
    let bad = fun(rec_ty(vec![("a", Ty::Int)]), Ty::Int);
    assert!(!sub_core(&core_ty(&[], &f).unwrap(), &bad));
    let stuck = eval(&eapp(f, pe("{a = 1}")), 100);
    assert!(
        core_ty(&[], &eapp(pe(r"\r:{a:Int, b:Int}. r.b"), pe("{a = 1}"))).is_err()
            && stuck.is_err(),
        "the unsound call is rejected statically and gets stuck dynamically"
    );
}

fn main() {
    std::thread::Builder::new()
        .stack_size(128 << 20)
        .spawn(|| {
            let mut r = Rng(0x9E37_79B9_7F4A_7C15);
            demo_and_suites();
            test_parse_roundtrip(&mut r);
            test_subtyping(&mut r);
            test_preservation(&mut r);
            test_differential(&mut r);
            test_free_theorems(&mut r);
            test_liskov(&mut r);
            println!("all System F / bidirectional checks passed");
        })
        .unwrap()
        .join()
        .unwrap();
}
