//! RECIPE 810 (compilers): untyped lambda calculus toolkit: substitution, de Bruijn, reduction strategies, Church/Scott encodings, SKI.
//! LEVEL: intermediate
//! Use in: understanding what every functional-language core does, writing interpreters, optimisers and proof-assistant kernels.   Run: cargo run --release --example lambda_calculus_toolkit
//! COMMON USES: the semantic core of ML/Haskell/Lisp compilers (Core/ANF are typed, strict or lazy cousins), term rewriting engines,
//! proof-assistant kernels (Lean/Coq/Agda all rely on capture-free substitution or de Bruijn terms), template/macro hygiene,
//! combinator-based execution of lazy languages (the Miranda/early Haskell graph-reduction lineage) and, via the S and K
//! combinators, point-free code generation.
//! Course: Pierce, "Types and Programming Languages", ch. 5-7 (untyped lambda calculus, nameless representation).
#![allow(dead_code)]
//!
//! THE IDEA: a term is a variable, `\x. body` or an application `f a`. The only computation rule is BETA: `(\x. b) a -> b[x := a]`.
//! Everything else (numbers, booleans, data, recursion) is an ENCODING. This file builds the machinery around that one rule:
//!
//! 1. NAMED terms with CAPTURE-AVOIDING substitution (rename a binder when it would capture a free variable of the argument)
//!    and ALPHA-equivalence (equal up to consistent renaming of bound variables).
//! 2. DE BRUIJN indices: a variable is the number of binders between it and its binder, so alpha-equivalent terms are
//!    literally `==` and substitution needs `shift` instead of fresh names. The cost is that humans cannot read them.
//!    The two evaluators (named and nameless) must take exactly the same steps: tested on thousands of random terms.
//! 3. REDUCTION STRATEGIES: normal order (leftmost-outermost, reduces under lambdas; finds a normal form whenever one exists),
//!    applicative order (innermost first; may diverge on terms that have a normal form), call-by-name and call-by-value
//!    (WEAK: never reduce under a lambda, stop at a lambda value) and call-by-need (call-by-name plus memoised arguments,
//!    here a Krivine-style environment machine whose thunks are updated in place). CHURCH-ROSSER (confluence): any two
//!    reduction orders that reach a normal form reach the SAME one (up to alpha); tested with a random-redex strategy.
//! 4. ENCODINGS as library terms: Church numerals/booleans/pairs/lists, Scott numerals/lists (data = its own case analysis,
//!    predecessor is O(1) but folding needs recursion), recursion through the Y combinator (normal order) and the Z combinator
//!    (an eta-expanded Y that survives call-by-value).
//! 5. COMBINATORS: BRACKET ABSTRACTION removes variables altogether: `[x]x = I`, `[x]E = K E` if x is not free in E,
//!    `[x](E1 E2) = S [x]E1 [x]E2`. Optimised variants add B (compose) and C (flip), and eta (`[x](E x) = E`), which shrinks output.
//!    Eta is NOT a beta rule, so the eta variant is only checked observationally (Church arithmetic), not by normal-form equality.
//!    The BCKW family (B, C, K, W) and S, K, I are standard; this file uses S K I B C.
//! WHY NON-TERMINATION IS DETECTED BY FUEL: beta-equivalence of terms is undecidable (it can encode the halting problem), so every
//! evaluator here counts steps and size and gives up with `Out::Fuel`; `omega = (\x. x x)(\x. x x)` reduces to itself forever.
//! HOW RUST DIFFERS: a closure `|x| body` is a lambda whose free variables are captured by an ENVIRONMENT (the Krivine machine
//! below is exactly what a closure is), but Rust is call-by-value and its types forbid self-application, so `omega` does not
//! typecheck: `let f = |x| x(x)` fails the occurs check. `Rc<RefCell<..>>` thunks are how `lazy_static`/`OnceCell` implement need.
//! Read: Pierce "Types and Programming Languages" https://www.cis.upenn.edu/~bcpierce/tapl/ ; Barendregt "The Lambda Calculus: its Syntax
//! and Semantics" (cited by title, not fetched); Turner "Another algorithm for bracket abstraction" (JSL 1979, by title);
//! https://en.wikipedia.org/wiki/Lambda_calculus , https://en.wikipedia.org/wiki/Church%E2%80%93Rosser_theorem and
//! Krivine "A call-by-name lambda-calculus machine" (by title).
//! TRY NEXT:
//!   - [S] Break `subst`: delete the `fs.contains(y)` rename branch and rerun; which test fails first, and on what term? Print it.
//!   - [S] Add `eta_reduce` on named terms and check `to_comb(.., BC_ETA)` output is smaller than BC on 1000 random terms.
//!   - [M] Predict, then measure: how often does applicative order beat normal order in step count on `random_closed`? Tally both.
//!   - [M] Add a `Strat::Lazy` (call-by-need on named terms using a shared `Rc<RefCell<..>>` let-heap) and compare `forces` with `Counters`.
//!   - [A] PROJECT: combine with `hindley_milner_inference`: parse these terms with its typechecker, show which library terms (`omega`, `Y`) it rejects and which Church terms need System F.
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

// ------------------------------------------------------------------ rng
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

// ------------------------------------------------------------------ named terms
#[derive(Clone, Debug, PartialEq)]
enum T {
    V(String),
    L(String, Box<T>),
    A(Box<T>, Box<T>),
}
fn var(s: &str) -> T {
    T::V(s.to_string())
}
fn lam(x: &str, b: T) -> T {
    T::L(x.to_string(), Box::new(b))
}
fn app(f: T, a: T) -> T {
    T::A(Box::new(f), Box::new(a))
}
fn size(t: &T) -> usize {
    match t {
        T::V(_) => 1,
        T::L(_, b) => 1 + size(b),
        T::A(f, a) => 1 + size(f) + size(a),
    }
}

/// `\x. b` (extends as far right as possible); application is left-associative. Lambdas as function or argument get parens.
fn show(t: &T) -> String {
    fn go(t: &T, prec: u8) -> String {
        match t {
            T::V(x) => x.clone(),
            T::L(x, b) => {
                let s = format!("\\{x}. {}", go(b, 0));
                if prec > 0 { format!("({s})") } else { s }
            }
            T::A(f, a) => {
                let s = format!("{} {}", go(f, 1), go(a, 2));
                if prec > 1 { format!("({s})") } else { s }
            }
        }
    }
    go(t, 0)
}

// ------------------------------------------------------------------ parser: `\x y. t`, application, parentheses
struct P<'a> {
    s: &'a [u8],
    i: usize,
}
impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
    fn ident(&mut self) -> Option<String> {
        self.ws();
        let st = self.i;
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_alphanumeric() || b"_'".contains(&self.s[self.i]))
        {
            self.i += 1;
        }
        (st != self.i).then(|| String::from_utf8_lossy(&self.s[st..self.i]).into_owned())
    }
    fn term(&mut self) -> Result<T, String> {
        if self.peek() == Some(b'\\') {
            self.i += 1;
            let mut names = vec![];
            while let Some(n) = self.ident() {
                names.push(n);
            }
            if names.is_empty() || self.peek() != Some(b'.') {
                return Err(format!("expected `x. ` after backslash at byte {}", self.i));
            }
            self.i += 1;
            let mut body = self.term()?;
            for n in names.iter().rev() {
                body = lam(n, body);
            }
            return Ok(body);
        }
        let mut f = self.atom()?;
        while matches!(self.peek(), Some(c) if c == b'(' || c == b'_' || c == b'\'' || c.is_ascii_alphanumeric())
        {
            f = app(f, self.atom()?);
        }
        Ok(f)
    }
    fn atom(&mut self) -> Result<T, String> {
        match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let t = self.term()?;
                if self.peek() != Some(b')') {
                    return Err(format!("expected `)` at byte {}", self.i));
                }
                self.i += 1;
                Ok(t)
            }
            _ => self
                .ident()
                .map(|n| T::V(n))
                .ok_or_else(|| format!("expected a variable at byte {}", self.i)),
        }
    }
}
fn parse(s: &str) -> Result<T, String> {
    let mut p = P {
        s: s.as_bytes(),
        i: 0,
    };
    let t = p.term()?;
    if p.peek().is_some() {
        return Err(format!("trailing input at byte {}", p.i));
    }
    Ok(t)
}
fn pt(s: &str) -> T {
    parse(s).unwrap_or_else(|e| panic!("parse {s:?}: {e}"))
}

// ------------------------------------------------------------------ free variables, substitution, alpha
fn fv(t: &T) -> BTreeSet<String> {
    match t {
        T::V(x) => BTreeSet::from([x.clone()]),
        T::L(x, b) => {
            let mut s = fv(b);
            s.remove(x);
            s
        }
        T::A(f, a) => {
            let mut s = fv(f);
            s.extend(fv(a));
            s
        }
    }
}
fn fresh(base: &str, avoid: &BTreeSet<String>) -> String {
    let mut n = format!("{base}'");
    while avoid.contains(&n) {
        n.push('\'');
    }
    n
}
/// `t[x := s]` without capturing: if descending under binder `y` where `y` is free in `s`, first rename `y`.
/// The rename is only needed when `x` actually occurs in the body; skipping it otherwise keeps terms small.
fn subst(t: &T, x: &str, s: &T) -> T {
    match t {
        T::V(y) => {
            if y == x {
                s.clone()
            } else {
                t.clone()
            }
        }
        T::A(f, a) => app(subst(f, x, s), subst(a, x, s)),
        T::L(y, b) => {
            if y == x {
                return t.clone();
            }
            let fb = fv(b);
            if !fb.contains(x) {
                return t.clone();
            }
            let fs = fv(s);
            if fs.contains(y) {
                let mut avoid = fs;
                avoid.extend(fb);
                avoid.insert(x.to_string());
                let y2 = fresh(y, &avoid);
                let renamed = subst(b, y, &var(&y2));
                T::L(y2, Box::new(subst(&renamed, x, s)))
            } else {
                T::L(y.clone(), Box::new(subst(b, x, s)))
            }
        }
    }
}
fn alpha_eq(a: &T, b: &T) -> bool {
    fn go<'a>(a: &'a T, b: &'a T, ea: &mut Vec<&'a str>, eb: &mut Vec<&'a str>) -> bool {
        match (a, b) {
            (T::V(x), T::V(y)) => {
                let (px, py) = (
                    ea.iter().rposition(|n| n == x),
                    eb.iter().rposition(|n| n == y),
                );
                match (px, py) {
                    (Some(i), Some(j)) => i == j, // bound by the same-depth binder
                    (None, None) => x == y,       // both free: names must match
                    _ => false,
                }
            }
            (T::L(x, p), T::L(y, q)) => {
                ea.push(x);
                eb.push(y);
                let r = go(p, q, ea, eb);
                ea.pop();
                eb.pop();
                r
            }
            (T::A(f, p), T::A(g, q)) => go(f, g, ea, eb) && go(p, q, ea, eb),
            _ => false,
        }
    }
    go(a, b, &mut vec![], &mut vec![])
}
/// Consistently rename every binder to a globally fresh name: an alpha-variant of `t`.
fn alpha_variant(t: &T, env: &mut Vec<(String, String)>, n: &mut usize) -> T {
    match t {
        T::V(x) => match env.iter().rev().find(|(o, _)| o == x) {
            Some((_, nn)) => var(nn),
            None => t.clone(),
        },
        T::L(x, b) => {
            *n += 1;
            let nn = format!("w{n}");
            env.push((x.clone(), nn.clone()));
            let b2 = alpha_variant(b, env, n);
            env.pop();
            lam(&nn, b2)
        }
        T::A(f, a) => app(alpha_variant(f, env, n), alpha_variant(a, env, n)),
    }
}

// ------------------------------------------------------------------ de Bruijn terms
#[derive(Clone, Debug, PartialEq)]
enum D {
    V(usize),
    L(Box<D>),
    A(Box<D>, Box<D>),
}
fn dsize(d: &D) -> usize {
    match d {
        D::V(_) => 1,
        D::L(b) => 1 + dsize(b),
        D::A(f, a) => 1 + dsize(f) + dsize(a),
    }
}
/// Free variables get indices past the binders, in order of first appearance (`free` is the naming context).
fn to_db(t: &T, bound: &mut Vec<String>, free: &mut Vec<String>) -> D {
    match t {
        T::V(x) => match bound.iter().rposition(|n| n == x) {
            Some(p) => D::V(bound.len() - 1 - p),
            None => {
                let p = free.iter().position(|n| n == x).unwrap_or_else(|| {
                    free.push(x.clone());
                    free.len() - 1
                });
                D::V(bound.len() + p)
            }
        },
        T::L(x, b) => {
            bound.push(x.clone());
            let r = D::L(Box::new(to_db(b, bound, free)));
            bound.pop();
            r
        }
        T::A(f, a) => D::A(
            Box::new(to_db(f, bound, free)),
            Box::new(to_db(a, bound, free)),
        ),
    }
}
fn db(t: &T) -> D {
    to_db(t, &mut vec![], &mut vec![])
}
/// Binder names are `x<depth>`, unique along any path, so no capture can happen when reading back.
fn from_db(d: &D, depth: usize, free: &[String]) -> T {
    match d {
        D::V(i) if *i < depth => var(&format!("x{}", depth - 1 - i)),
        D::V(i) => var(&free[i - depth]),
        D::L(b) => lam(&format!("x{depth}"), from_db(b, depth + 1, free)),
        D::A(f, a) => app(from_db(f, depth, free), from_db(a, depth, free)),
    }
}
/// Add `k` to every free index (>= cutoff `c`).
fn shift(d: &D, c: usize, k: isize) -> D {
    match d {
        D::V(i) => D::V(if *i >= c {
            (*i as isize + k) as usize
        } else {
            *i
        }),
        D::L(b) => D::L(Box::new(shift(b, c + 1, k))),
        D::A(f, a) => D::A(Box::new(shift(f, c, k)), Box::new(shift(a, c, k))),
    }
}
fn subst_db(d: &D, j: usize, s: &D) -> D {
    match d {
        D::V(i) => {
            if *i == j {
                s.clone()
            } else {
                d.clone()
            }
        }
        D::L(b) => D::L(Box::new(subst_db(b, j + 1, &shift(s, 0, 1)))),
        D::A(f, a) => D::A(Box::new(subst_db(f, j, s)), Box::new(subst_db(a, j, s))),
    }
}
/// `(\. b) a`: put `a` (shifted up under the binder) at index 0, then shift everything down because the binder is gone.
fn beta_db(b: &D, a: &D) -> D {
    shift(&subst_db(b, 0, &shift(a, 0, 1)), 0, -1)
}

// ------------------------------------------------------------------ strategies and the reducer
#[derive(Clone, Copy, PartialEq, Debug)]
enum Strat {
    Normal,
    Applicative,
    ByName,
    ByValue,
}
#[derive(Debug, PartialEq, Clone)]
enum Out<X> {
    Done(X, usize),
    Fuel(usize),
}
fn is_lam(t: &T) -> bool {
    matches!(t, T::L(..))
}
fn ap2(f: Option<T>, a: &T) -> Option<T> {
    f.map(|f| app(f, a.clone()))
}
fn step(t: &T, s: Strat) -> Option<T> {
    let deep = matches!(s, Strat::Normal | Strat::Applicative); // weak strategies never look under a lambda
    match t {
        T::V(_) => None,
        T::L(x, b) => {
            if deep {
                step(b, s).map(|b2| lam(x, b2))
            } else {
                None
            }
        }
        T::A(f, a) => match s {
            Strat::Normal => match &**f {
                T::L(x, b) => Some(subst(b, x, a)),
                _ => ap2(step(f, s), a).or_else(|| step(a, s).map(|a2| app((**f).clone(), a2))),
            },
            Strat::Applicative => ap2(step(f, s), a)
                .or_else(|| step(a, s).map(|a2| app((**f).clone(), a2)))
                .or_else(|| match &**f {
                    T::L(x, b) => Some(subst(b, x, a)),
                    _ => None,
                }),
            Strat::ByName => match &**f {
                T::L(x, b) => Some(subst(b, x, a)),
                _ => ap2(step(f, s), a),
            },
            Strat::ByValue => {
                if !is_lam(f) {
                    ap2(step(f, s), a)
                } else if !is_lam(a) {
                    step(a, s).map(|a2| app((**f).clone(), a2))
                } else if let T::L(x, b) = &**f {
                    Some(subst(b, x, a))
                } else {
                    None
                }
            }
        },
    }
}
fn run(t: &T, s: Strat, fuel: usize, max_size: usize) -> Out<T> {
    let (mut cur, mut n) = (t.clone(), 0);
    loop {
        match step(&cur, s) {
            None => return Out::Done(cur, n),
            Some(nx) => {
                n += 1;
                if n > fuel || size(&nx) > max_size {
                    return Out::Fuel(n);
                }
                cur = nx;
            }
        }
    }
}
/// The same two deep strategies on de Bruijn terms; must mirror `step` exactly.
fn step_db(d: &D, applicative: bool) -> Option<D> {
    match d {
        D::V(_) => None,
        D::L(b) => step_db(b, applicative).map(|b2| D::L(Box::new(b2))),
        D::A(f, a) => {
            let in_f = |f2: D| D::A(Box::new(f2), a.clone());
            let in_a = |a2: D| D::A(f.clone(), Box::new(a2));
            let contract = || {
                if let D::L(b) = &**f {
                    Some(beta_db(b, a))
                } else {
                    None
                }
            };
            if applicative {
                step_db(f, true)
                    .map(in_f)
                    .or_else(|| step_db(a, true).map(in_a))
                    .or_else(contract)
            } else {
                contract()
                    .or_else(|| step_db(f, false).map(in_f))
                    .or_else(|| step_db(a, false).map(in_a))
            }
        }
    }
}
fn run_db(d: &D, applicative: bool, fuel: usize, max_size: usize) -> Out<D> {
    let (mut cur, mut n) = (d.clone(), 0);
    loop {
        match step_db(&cur, applicative) {
            None => return Out::Done(cur, n),
            Some(nx) => {
                n += 1;
                if n > fuel || dsize(&nx) > max_size {
                    return Out::Fuel(n);
                }
                cur = nx;
            }
        }
    }
}

/// Number of beta-redexes in `t`, and contraction of the n-th one in preorder: lets a test pick a RANDOM redex each step.
fn count_redexes(t: &T) -> usize {
    match t {
        T::V(_) => 0,
        T::L(_, b) => count_redexes(b),
        T::A(f, a) => usize::from(is_lam(f)) + count_redexes(f) + count_redexes(a),
    }
}
fn contract_nth(t: &T, n: &mut usize) -> Option<T> {
    match t {
        T::V(_) => None,
        T::L(x, b) => contract_nth(b, n).map(|b2| lam(x, b2)),
        T::A(f, a) => {
            if let T::L(x, b) = &**f {
                if *n == 0 {
                    return Some(subst(b, x, a));
                }
                *n -= 1;
            }
            if let Some(f2) = contract_nth(f, n) {
                return Some(app(f2, (**a).clone()));
            }
            contract_nth(a, n).map(|a2| app((**f).clone(), a2))
        }
    }
}
fn run_random(t: &T, rng: &mut Rng, fuel: usize, max_size: usize) -> Out<T> {
    let (mut cur, mut n) = (t.clone(), 0);
    loop {
        let c = count_redexes(&cur);
        if c == 0 {
            return Out::Done(cur, n);
        }
        let mut k = rng.below(c);
        cur = contract_nth(&cur, &mut k).expect("counted redex exists");
        n += 1;
        if n > fuel || size(&cur) > max_size {
            return Out::Fuel(n);
        }
    }
}

// ------------------------------------------------------------------ call-by-name vs call-by-need: a Krivine machine on de Bruijn terms
type Env = Option<Rc<Node>>;
struct Node {
    head: Rc<Thunk>,
    tail: Env,
}
enum St {
    Delayed(D, Env),
    Done(D, Env),
}
struct Thunk(RefCell<St>);
#[derive(Default)]
struct Counters {
    betas: usize,
    forces: usize, // how many times a delayed argument was actually evaluated
    fuel: usize,
}
fn lookup(env: &Env, i: usize) -> Rc<Thunk> {
    let mut cur = env;
    for _ in 0..i {
        cur = &cur.as_ref().expect("closed term").tail;
    }
    cur.as_ref().expect("closed term").head.clone()
}
/// Evaluate to weak head normal form: a lambda closure. With `need`, a forced thunk is overwritten by its value.
fn eval(mut t: D, mut env: Env, need: bool, c: &mut Counters) -> Option<(D, Env)> {
    let mut stack: Vec<Rc<Thunk>> = vec![];
    loop {
        match t {
            D::A(f, a) => {
                stack.push(Rc::new(Thunk(RefCell::new(St::Delayed(*a, env.clone())))));
                t = *f;
            }
            D::L(b) => match stack.pop() {
                None => return Some((D::L(b), env)),
                Some(arg) => {
                    c.betas += 1;
                    if c.betas > c.fuel {
                        return None;
                    }
                    env = Some(Rc::new(Node {
                        head: arg,
                        tail: env,
                    }));
                    t = *b;
                }
            },
            D::V(i) => {
                let th = lookup(&env, i);
                let cached = match &*th.0.borrow() {
                    St::Done(t2, e2) => Some((t2.clone(), e2.clone())),
                    St::Delayed(..) => None,
                };
                let (t2, e2) = match cached {
                    Some(v) => v,
                    None => {
                        c.forces += 1;
                        let (dt, de) = match &*th.0.borrow() {
                            St::Delayed(t2, e2) => (t2.clone(), e2.clone()),
                            St::Done(..) => unreachable!(),
                        };
                        let v = eval(dt, de, need, c)?;
                        if need {
                            *th.0.borrow_mut() = St::Done(v.0.clone(), v.1.clone());
                        }
                        v
                    }
                };
                t = t2;
                env = e2;
            }
        }
    }
}
/// Substitute closures back into a term (no evaluation): beta-equal to the original, used to compare machines to the reducer.
fn readback(t: &D, env: &Env, depth: usize) -> D {
    match t {
        D::V(i) if *i < depth => D::V(*i),
        D::V(i) => {
            let th = lookup(env, i - depth);
            let (t2, e2) = match &*th.0.borrow() {
                St::Delayed(t2, e2) | St::Done(t2, e2) => (t2.clone(), e2.clone()),
            };
            shift(&readback(&t2, &e2, 0), 0, depth as isize)
        }
        D::L(b) => D::L(Box::new(readback(b, env, depth + 1))),
        D::A(f, a) => D::A(
            Box::new(readback(f, env, depth)),
            Box::new(readback(a, env, depth)),
        ),
    }
}
/// WHNF of a closed term by name or by need; returns (readback term, counters).
fn whnf_machine(t: &T, need: bool, fuel: usize) -> Option<(D, Counters)> {
    let mut c = Counters {
        fuel,
        ..Default::default()
    };
    let d = db(t);
    let (t2, e2) = eval(d, None, need, &mut c)?;
    Some((readback(&t2, &e2, 0), c))
}

// ------------------------------------------------------------------ the standard library of encodings
const LIB: &[(&str, &str)] = &[
    ("id", r"\x. x"),
    ("true", r"\t f. t"),
    ("false", r"\t f. f"),
    ("not", r"\b. b false true"),
    ("and", r"\p q. p q p"),
    ("or", r"\p q. p p q"),
    ("zero", r"\f x. x"),
    ("succ", r"\n f x. f (n f x)"),
    ("add", r"\m n f x. m f (n f x)"),
    ("mul", r"\m n f. m (n f)"),
    ("exp", r"\m n. n m"),
    ("iszero", r"\n. n (\u. false) true"),
    ("pair", r"\a b s. s a b"),
    ("fst", r"\p. p (\a b. a)"),
    ("snd", r"\p. p (\a b. b)"),
    (
        "pred",
        r"\n. fst (n (\p. pair (snd p) (succ (snd p))) (pair zero zero))",
    ),
    ("sub", r"\m n. n pred m"),
    ("leq", r"\m n. iszero (sub m n)"),
    ("nil", r"\c n. n"),
    ("cons", r"\h t c n. c h (t c n)"),
    ("sum", r"\l. l add zero"),
    ("Y", r"\f. (\x. f (x x)) (\x. f (x x))"),
    ("Z", r"\f. (\x. f (\v. x x v)) (\x. f (\v. x x v))"),
    ("one", r"succ zero"),
    // factorial: normal order may use Y and a strict `if` because untaken branches are never evaluated.
    ("fact", r"Y (\self n. iszero n one (mul n (self (pred n))))"),
    // factorial for call-by-value: branches are thunks `\u. ..` forced by applying `id`; recursion through Z.
    (
        "fact_cbv",
        r"Z (\self n. iszero n (\u. one) (\u. mul n (self (pred n))) id)",
    ),
    // Scott encodings: data is its own case analysis. s_zero z s = z ; s_succ n z s = s n.
    ("s_zero", r"\z s. z"),
    ("s_succ", r"\n z s. s n"),
    ("s_pred", r"\n. n s_zero id"),
    ("s_add", r"Y (\self m n. m n (\p. s_succ (self p n)))"),
    ("s_nil", r"\n c. n"),
    ("s_cons", r"\h t n c. c h t"),
    ("s_len", r"Y (\self l. l zero (\h t. succ (self t)))"),
    ("omega", r"(\x. x x) (\x. x x)"),
];
/// Expand library names occurring free in `src` (definitions are closed, so substitution order is irrelevant).
fn lib(src: &str) -> T {
    let mut t = pt(src);
    loop {
        let names: Vec<String> = fv(&t)
            .into_iter()
            .filter(|n| LIB.iter().any(|(k, _)| k == n))
            .collect();
        if names.is_empty() {
            return t;
        }
        for n in names {
            let def = LIB.iter().find(|(k, _)| *k == n).unwrap().1;
            t = subst(&t, &n, &pt(def));
        }
    }
}
fn church(n: usize) -> T {
    let mut b = var("x");
    for _ in 0..n {
        b = app(var("f"), b);
    }
    lam("f", lam("x", b))
}
fn nf(t: &T) -> Option<T> {
    match run(t, Strat::Normal, 200_000, 100_000) {
        Out::Done(r, _) => Some(r),
        Out::Fuel(_) => None,
    }
}
fn decode_church(t: &T) -> Option<usize> {
    fn body(d: &D) -> Option<usize> {
        match d {
            D::V(0) => Some(0),
            D::A(f, a) if **f == D::V(1) => body(a).map(|n| n + 1),
            _ => None,
        }
    }
    match db(t) {
        D::L(b) => match *b {
            D::L(b2) => body(&b2),
            _ => None,
        },
        _ => None,
    }
}
fn decode_bool(t: &T) -> Option<bool> {
    match db(t) {
        D::L(b) => match *b {
            D::L(b2) => match *b2 {
                D::V(1) => Some(true),
                D::V(0) => Some(false),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}
fn decode_scott(t: &T) -> Option<usize> {
    fn go(d: &D) -> Option<usize> {
        // \z s. z  |  \z s. s n   (n itself a Scott numeral, closed so its indices are untouched)
        let D::L(b) = d else { return None };
        let D::L(b2) = &**b else { return None };
        match &**b2 {
            D::V(1) => Some(0),
            D::A(f, n) if **f == D::V(0) => go(n).map(|k| k + 1),
            _ => None,
        }
    }
    go(&db(t))
}
fn church_of(src_expr: &str) -> usize {
    decode_church(&nf(&lib(src_expr)).expect("terminates"))
        .unwrap_or_else(|| panic!("not a Church numeral: {src_expr}"))
}

// ------------------------------------------------------------------ combinators (bracket abstraction)
#[derive(Clone, Debug, PartialEq)]
enum Cb {
    S,
    K,
    I,
    B,
    C,
    At(String),
    Ap(Box<Cb>, Box<Cb>),
}
fn ap(a: Cb, b: Cb) -> Cb {
    Cb::Ap(Box::new(a), Box::new(b))
}
fn show_cb(c: &Cb) -> String {
    match c {
        Cb::S => "S".into(),
        Cb::K => "K".into(),
        Cb::I => "I".into(),
        Cb::B => "B".into(),
        Cb::C => "C".into(),
        Cb::At(x) => x.clone(),
        Cb::Ap(f, a) => {
            let rhs = match &**a {
                Cb::Ap(..) => format!("({})", show_cb(a)),
                _ => show_cb(a),
            };
            format!("{} {rhs}", show_cb(f))
        }
    }
}
fn cb_size(c: &Cb) -> usize {
    match c {
        Cb::Ap(f, a) => 1 + cb_size(f) + cb_size(a),
        _ => 1,
    }
}
fn occurs(x: &str, c: &Cb) -> bool {
    match c {
        Cb::At(y) => y == x,
        Cb::Ap(f, a) => occurs(x, f) || occurs(x, a),
        _ => false,
    }
}
#[derive(Clone, Copy)]
struct Opt {
    bc: bool,  // use B and C
    eta: bool, // use [x](E x) = E
}
const NAIVE: Opt = Opt {
    bc: false,
    eta: false,
};
const BC: Opt = Opt {
    bc: true,
    eta: false,
};
const BC_ETA: Opt = Opt {
    bc: true,
    eta: true,
};
/// `[x]c`: a combinator term `r` without `x` such that `r x` reduces to `c`.
fn abstract_var(x: &str, c: &Cb, o: Opt) -> Cb {
    if o.bc && !occurs(x, c) {
        return ap(Cb::K, c.clone());
    }
    match c {
        Cb::At(y) if y == x => Cb::I,
        Cb::Ap(e1, e2) => {
            if o.eta && **e2 == Cb::At(x.to_string()) && !occurs(x, e1) {
                return (**e1).clone();
            }
            if o.bc {
                if !occurs(x, e1) {
                    return ap(ap(Cb::B, (**e1).clone()), abstract_var(x, e2, o));
                }
                if !occurs(x, e2) {
                    return ap(ap(Cb::C, abstract_var(x, e1, o)), (**e2).clone());
                }
            }
            ap(ap(Cb::S, abstract_var(x, e1, o)), abstract_var(x, e2, o))
        }
        _ => ap(Cb::K, c.clone()), // naive: constants and other variables
    }
}
/// Translate inside-out so each lambda sees an already variable-free body (shadowing is then harmless).
fn to_comb(t: &T, o: Opt) -> Cb {
    match t {
        T::V(x) => Cb::At(x.clone()),
        T::A(f, a) => ap(to_comb(f, o), to_comb(a, o)),
        T::L(x, b) => abstract_var(x, &to_comb(b, o), o),
    }
}
fn from_comb(c: &Cb) -> T {
    match c {
        Cb::S => pt(r"\s1 s2 s3. s1 s3 (s2 s3)"),
        Cb::K => pt(r"\s1 s2. s1"),
        Cb::I => pt(r"\s1. s1"),
        Cb::B => pt(r"\s1 s2 s3. s1 (s2 s3)"),
        Cb::C => pt(r"\s1 s2 s3. s1 s3 s2"),
        Cb::At(x) => var(x),
        Cb::Ap(f, a) => app(from_comb(f), from_comb(a)),
    }
}
fn spine(c: &Cb) -> (&Cb, Vec<&Cb>) {
    let (mut h, mut args) = (c, vec![]);
    while let Cb::Ap(f, a) = h {
        args.push(&**a);
        h = f;
    }
    args.reverse();
    (h, args)
}
fn rebuild(h: Cb, args: &[Cb]) -> Cb {
    args.iter().fold(h, |acc, a| ap(acc, a.clone()))
}
/// One leftmost-outermost step, also reducing inside arguments (full normalisation of a lambda-free term).
fn step_cb(c: &Cb) -> Option<Cb> {
    let (h, args) = spine(c);
    let a: Vec<Cb> = args.iter().map(|x| (*x).clone()).collect();
    let r = match (h, a.len()) {
        (Cb::I, n) if n >= 1 => Some(rebuild(a[0].clone(), &a[1..])),
        (Cb::K, n) if n >= 2 => Some(rebuild(a[0].clone(), &a[2..])),
        (Cb::S, n) if n >= 3 => Some(rebuild(
            ap(
                ap(a[0].clone(), a[2].clone()),
                ap(a[1].clone(), a[2].clone()),
            ),
            &a[3..],
        )),
        (Cb::B, n) if n >= 3 => Some(rebuild(
            ap(a[0].clone(), ap(a[1].clone(), a[2].clone())),
            &a[3..],
        )),
        (Cb::C, n) if n >= 3 => Some(rebuild(
            ap(ap(a[0].clone(), a[2].clone()), a[1].clone()),
            &a[3..],
        )),
        _ => None,
    };
    if r.is_some() {
        return r;
    }
    for (i, x) in a.iter().enumerate() {
        if let Some(x2) = step_cb(x) {
            let mut b = a.clone();
            b[i] = x2;
            return Some(rebuild(h.clone(), &b));
        }
    }
    None
}
fn nf_cb(c: &Cb, fuel: usize) -> Option<Cb> {
    let mut cur = c.clone();
    for _ in 0..fuel {
        match step_cb(&cur) {
            None => return Some(cur),
            Some(n) => {
                if cb_size(&n) > 50_000 {
                    return None;
                }
                cur = n;
            }
        }
    }
    None
}
/// Count the f's in `f (f (... x))`.
fn decode_f_x(c: &Cb) -> Option<usize> {
    match c {
        Cb::At(x) if x == "x" => Some(0),
        Cb::Ap(f, a) if **f == Cb::At("f".into()) => decode_f_x(a).map(|n| n + 1),
        _ => None,
    }
}

// ------------------------------------------------------------------ random terms
fn gen_term(r: &mut Rng, d: usize, scope: &mut Vec<&'static str>) -> T {
    const NAMES: [&str; 3] = ["x", "y", "z"]; // few names on purpose: shadowing and capture happen all the time
    if d == 0 {
        return if scope.is_empty() {
            lam("x", var("x"))
        } else {
            var(scope[r.below(scope.len())])
        };
    }
    match r.below(10) {
        0..=2 if !scope.is_empty() => var(scope[r.below(scope.len())]),
        3..=5 => {
            let n = NAMES[r.below(3)];
            scope.push(n);
            let b = gen_term(r, d - 1, scope);
            scope.pop();
            lam(n, b)
        }
        _ => app(gen_term(r, d - 1, scope), gen_term(r, d - 1, scope)),
    }
}
fn random_closed(r: &mut Rng, d: usize) -> T {
    gen_term(r, d, &mut vec![])
}

// ------------------------------------------------------------------ demos and tests
fn demo_strategies() {
    println!("-- strategy table: steps to reach a result (F = gave up)");
    let cases = [
        ("(\\x y. y) omega", r"(\x y. y) omega"),
        ("(\\x. x x) (id id)", r"(\x. x x) (id id)"),
        ("add 2 2", r"add (succ (succ zero)) (succ (succ zero))"),
        ("omega", "omega"),
    ];
    println!(
        "{:<20} {:>8} {:>12} {:>8} {:>8}",
        "term", "normal", "applicative", "by-name", "by-value"
    );
    for (name, src) in cases {
        let t = lib(src);
        let cell = |s| match run(&t, s, 5_000, 5_000) {
            Out::Done(_, n) => n.to_string(),
            Out::Fuel(_) => "F".into(),
        };
        println!(
            "{name:<20} {:>8} {:>12} {:>8} {:>8}",
            cell(Strat::Normal),
            cell(Strat::Applicative),
            cell(Strat::ByName),
            cell(Strat::ByValue)
        );
    }
    // normal order finds the normal form that applicative order never reaches
    let t = lib(r"(\x y. y) omega");
    assert!(matches!(run(&t, Strat::Normal, 100, 1000), Out::Done(..)));
    assert!(matches!(
        run(&t, Strat::Applicative, 100, 1000),
        Out::Fuel(_)
    ));
    assert!(matches!(run(&t, Strat::ByValue, 100, 1000), Out::Fuel(_)));
    assert!(matches!(run(&t, Strat::ByName, 100, 1000), Out::Done(..)));
    // omega: every strategy runs out of fuel, one beta step at a time; it reduces to itself
    let w = lib("omega");
    for s in [
        Strat::Normal,
        Strat::Applicative,
        Strat::ByName,
        Strat::ByValue,
    ] {
        assert!(
            matches!(run(&w, s, 1000, 1000), Out::Fuel(_)),
            "omega must exhaust fuel under {s:?}"
        );
    }
    assert_eq!(
        step(&w, Strat::Normal).unwrap(),
        w,
        "omega reduces to itself"
    );
}

fn demo_substitution() {
    println!("-- capture-avoiding substitution");
    let r = subst(&pt(r"\y. x"), "x", &var("y"));
    println!("(\\y. x)[x := y] = {}", show(&r));
    assert_eq!(show(&r), r"\y'. y");
    assert!(alpha_eq(&pt(r"\a. \b. a b"), &pt(r"\p. \q. p q")));
    assert!(!alpha_eq(&pt(r"\a. \b. a b"), &pt(r"\a. \b. b a")));
    assert!(!alpha_eq(&pt(r"\a. a"), &pt(r"\a. b")));
    assert!(
        alpha_eq(&pt(r"\x. \x. x"), &pt(r"\a. \b. b")),
        "shadowing: the inner x wins"
    );
    // the classic wrong answer would be \y. y
    let naive_wrong = lam("y", var("y"));
    assert!(!alpha_eq(&r, &naive_wrong));
    let d = db(&pt(r"\x. \y. x (y x)"));
    println!("de Bruijn of \\x. \\y. x (y x) = {d:?}");
    assert_eq!(
        d,
        D::L(Box::new(D::L(Box::new(D::A(
            Box::new(D::V(1)),
            Box::new(D::A(Box::new(D::V(0)), Box::new(D::V(1))))
        )))))
    );
}

fn test_parse_print(r: &mut Rng) {
    for _ in 0..500 {
        let t = random_closed(r, 6);
        let s = show(&t);
        assert_eq!(parse(&s).unwrap(), t, "round trip failed for {s}");
    }
    assert_eq!(
        pt(r"\x y. x y z"),
        lam("x", lam("y", app(app(var("x"), var("y")), var("z"))))
    );
    assert!(parse(r"\x x").is_err() && parse("(x").is_err() && parse("x )").is_err());
    println!("parser/printer round trip: 500 random terms OK");
}

fn test_alpha_and_db(r: &mut Rng) {
    let mut open_ok = 0;
    for _ in 0..1000 {
        let t = random_closed(r, 6);
        let mut n = 0;
        let v = alpha_variant(&t, &mut vec![], &mut n);
        assert!(alpha_eq(&t, &v) && db(&t) == db(&v));
        let u = random_closed(r, 6);
        assert_eq!(
            alpha_eq(&t, &u),
            db(&t) == db(&u),
            "alpha_eq must coincide with de Bruijn equality"
        );
        // db round trip, with a free variable in the mix
        let open = app(var("f"), t.clone());
        let mut free = vec![];
        let d = to_db(&open, &mut vec![], &mut free);
        assert!(alpha_eq(&from_db(&d, 0, &free), &open));
        open_ok += 1;
    }
    println!(
        "alpha-equivalence == de Bruijn equality, db round trip with free vars: {open_ok} terms OK"
    );
}

fn test_named_vs_db(r: &mut Rng) {
    let (mut done, mut fuel) = (0, 0);
    for i in 0..2000 {
        let t = random_closed(r, 4 + i % 3);
        for applicative in [false, true] {
            let s = if applicative {
                Strat::Applicative
            } else {
                Strat::Normal
            };
            let a = run(&t, s, 300, 3000);
            let b = run_db(&db(&t), applicative, 300, 3000);
            match (&a, &b) {
                (Out::Done(x, n1), Out::Done(y, n2)) => {
                    assert_eq!(n1, n2, "step counts differ on {}", show(&t));
                    assert_eq!(&db(x), y, "normal forms differ on {}", show(&t));
                    done += 1;
                }
                (Out::Fuel(n1), Out::Fuel(n2)) => {
                    assert_eq!(n1, n2);
                    fuel += 1;
                }
                _ => panic!(
                    "named and de Bruijn evaluators disagree on {}: {a:?} vs {b:?}",
                    show(&t)
                ),
            }
        }
    }
    println!("named vs de Bruijn: 4000 runs identical ({done} normal forms, {fuel} gave up)");
    assert!(
        done > 800,
        "generator should produce plenty of terminating terms"
    );
}

fn test_church_rosser(r: &mut Rng) {
    let (mut compared, mut no_only, mut ao_done, mut rnd_done) = (0, 0, 0, 0);
    for i in 0..2000 {
        let t = random_closed(r, 4 + i % 3);
        let no = run(&t, Strat::Normal, 400, 4000);
        let ao = run(&t, Strat::Applicative, 400, 4000);
        let rn = run_random(&t, r, 400, 4000);
        if let (Out::Done(a, _), Out::Done(b, _)) = (&no, &ao) {
            assert!(
                alpha_eq(a, b),
                "Church-Rosser violated (normal vs applicative) on {}",
                show(&t)
            );
            compared += 1;
        }
        if let (Out::Done(a, _), Out::Done(b, _)) = (&no, &rn) {
            assert!(
                alpha_eq(a, b),
                "Church-Rosser violated (normal vs random order) on {}",
                show(&t)
            );
            compared += 1;
        }
        if matches!(no, Out::Done(..)) && !matches!(ao, Out::Done(..)) {
            no_only += 1;
        }
        // normalisation theorem, one direction we can state without a fuel caveat: nothing terminates that
        // normal order can never finish; so a Done from AO or random with NO out of fuel is only a fuel artefact.
        ao_done += usize::from(matches!(ao, Out::Done(..)));
        rnd_done += usize::from(matches!(rn, Out::Done(..)));
    }
    println!(
        "Church-Rosser: {compared} pairwise normal-form comparisons agree; NO finished where AO gave up on {no_only} terms (AO finished {ao_done}, random {rnd_done})"
    );
    assert!(compared > 1000);
}

fn test_machines(r: &mut Rng) {
    // sharing: `x` is used twice; by-need evaluates the argument once
    let t = lib(r"(\x. and x x) (not (not (not true)))");
    let (_, name) = whnf_machine(&t, false, 10_000).unwrap();
    let (_, need) = whnf_machine(&t, true, 10_000).unwrap();
    println!(
        "-- sharing: by-name betas={} forces={}, by-need betas={} forces={}",
        name.betas, name.forces, need.betas, need.forces
    );
    assert!(need.betas < name.betas && need.forces <= name.forces);
    // readback of both is beta-equal to the original (same normal form)
    let want = db(&nf(&t).unwrap());
    for need in [false, true] {
        let (rb, _) = whnf_machine(&t, need, 10_000).unwrap();
        let rb_t = from_db(&rb, 0, &[]);
        assert_eq!(db(&nf(&rb_t).unwrap()), want);
    }
    // omega never gets a WHNF
    assert!(
        whnf_machine(&lib("omega"), true, 5000).is_none()
            && whnf_machine(&lib("omega"), false, 5000).is_none()
    );
    // random closed terms: machine WHNF readback normalises to the same normal form as the term
    let mut ok = 0;
    for i in 0..1500 {
        let t = random_closed(r, 4 + i % 3);
        let Some(want) = (match run(&t, Strat::Normal, 300, 3000) {
            Out::Done(x, _) => Some(db(&x)),
            _ => None,
        }) else {
            continue;
        };
        for need in [false, true] {
            let Some((rb, c)) = whnf_machine(&t, need, 2000) else {
                panic!("machine ran out of fuel on a normalising term")
            };
            let got = match run(&from_db(&rb, 0, &[]), Strat::Normal, 2000, 20_000) {
                Out::Done(x, _) => db(&x),
                _ => panic!("readback did not normalise: {}", show(&t)),
            };
            assert_eq!(got, want, "machine (need={need}) disagrees on {}", show(&t));
            let _ = c;
        }
        ok += 1;
    }
    println!("Krivine by-name and by-need agree with the reducer on {ok} terminating random terms");
}

fn test_encodings() {
    // Church arithmetic against integers
    for m in 0..=5usize {
        for n in 0..=5usize {
            let cm = format!("{}", show(&church(m)));
            let cn = format!("{}", show(&church(n)));
            let e = |op: &str| church_of(&format!("{op} ({cm}) ({cn})"));
            assert_eq!(e("add"), m + n);
            assert_eq!(e("mul"), m * n);
            if m <= 3 && (1..=3).contains(&n) {
                // exp m n = n m = m^n; n = 0 gives `\x. x`, the eta-reduced numeral 1, which the strict decoder rejects
                assert_eq!(e("exp"), m.pow(n as u32));
            }
            assert_eq!(e("sub"), m.saturating_sub(n));
            let le = nf(&lib(&format!("leq ({cm}) ({cn})"))).unwrap();
            assert_eq!(decode_bool(&le), Some(m <= n));
        }
        assert_eq!(
            church_of(&format!("pred ({})", show(&church(m)))),
            m.saturating_sub(1)
        );
        assert_eq!(church_of(&format!("succ ({})", show(&church(m)))), m + 1);
        assert_eq!(
            decode_bool(&nf(&lib(&format!("iszero ({})", show(&church(m))))).unwrap()),
            Some(m == 0)
        );
    }
    // booleans
    for (a, b) in [(true, true), (true, false), (false, true), (false, false)] {
        let (sa, sb) = (
            if a { "true" } else { "false" },
            if b { "true" } else { "false" },
        );
        assert_eq!(
            decode_bool(&nf(&lib(&format!("and {sa} {sb}"))).unwrap()),
            Some(a && b)
        );
        assert_eq!(
            decode_bool(&nf(&lib(&format!("or {sa} {sb}"))).unwrap()),
            Some(a || b)
        );
        assert_eq!(
            decode_bool(&nf(&lib(&format!("not {sa}"))).unwrap()),
            Some(!a)
        );
    }
    // pairs and lists
    assert_eq!(
        church_of(&format!(
            "fst (pair ({}) ({}))",
            show(&church(7)),
            show(&church(2))
        )),
        7
    );
    assert_eq!(
        church_of(&format!(
            "snd (pair ({}) ({}))",
            show(&church(7)),
            show(&church(2))
        )),
        2
    );
    let list = format!(
        "cons ({}) (cons ({}) (cons ({}) nil))",
        show(&church(1)),
        show(&church(2)),
        show(&church(3))
    );
    assert_eq!(church_of(&format!("sum ({list})")), 6);
    // recursion via Y in normal order
    for (n, want) in [(0, 1), (1, 1), (2, 2), (3, 6)] {
        assert_eq!(
            church_of(&format!("fact ({})", show(&church(n)))),
            want,
            "fact {n}"
        );
    }
    println!("Church numerals, booleans, pairs, lists and Y-factorial agree with integers");
    // Scott numerals and lists
    let s3 = format!("s_succ (s_succ (s_succ s_zero))");
    assert_eq!(decode_scott(&nf(&lib(&s3)).unwrap()), Some(3));
    assert_eq!(
        decode_scott(&nf(&lib(&format!("s_pred ({s3})"))).unwrap()),
        Some(2)
    );
    assert_eq!(decode_scott(&nf(&lib("s_pred s_zero")).unwrap()), Some(0));
    assert_eq!(
        decode_scott(&nf(&lib(&format!("s_add ({s3}) ({s3})"))).unwrap()),
        Some(6)
    );
    assert_eq!(
        church_of("s_len (s_cons id (s_cons id (s_cons id s_nil)))"),
        3
    );
    println!("Scott numerals: O(1) predecessor, add via Y, list length OK");
}

fn test_cbv_and_fix() {
    // Y under call-by-value diverges, Z does not
    let y_loop = lib(r"Y (\self n. n)");
    assert!(
        matches!(run(&y_loop, Strat::ByValue, 500, 5000), Out::Fuel(_)),
        "Y diverges under CBV"
    );
    assert!(
        matches!(run(&y_loop, Strat::ByName, 500, 5000), Out::Done(..)),
        "Y is fine under call-by-name"
    );
    let z_ok = lib(r"Z (\self n. n)");
    assert!(
        matches!(run(&z_ok, Strat::ByValue, 500, 5000), Out::Done(..)),
        "Z survives call-by-value"
    );
    // fact_cbv under CBV reaches a lambda value; normalising that value (normal order) decodes to 6
    let t = lib(&format!("fact_cbv ({})", show(&church(3))));
    match run(&t, Strat::ByValue, 200_000, 100_000) {
        Out::Done(v, n) => {
            assert!(is_lam(&v));
            assert_eq!(decode_church(&nf(&v).unwrap()), Some(6));
            println!("fact_cbv 3 via Z under call-by-value: value in {n} steps, decodes to 6");
        }
        Out::Fuel(n) => panic!("CBV factorial ran out of fuel after {n} steps"),
    }
}

fn test_combinators(r: &mut Rng) {
    let t = lib(r"\f x. f (f x)");
    for (name, o) in [("naive", NAIVE), ("B,C", BC), ("B,C,eta", BC_ETA)] {
        let c = to_comb(&t, o);
        println!("two = {name:<8} size {:>2}: {}", cb_size(&c), show_cb(&c));
    }
    assert_eq!(show_cb(&to_comb(&pt(r"\x. x"), NAIVE)), "I");
    assert_eq!(show_cb(&to_comb(&pt(r"\x y. x"), NAIVE)), "S (K K) I");
    assert_eq!(show_cb(&to_comb(&pt(r"\x y. x"), BC)), "B K I");
    // observational: translate Church arithmetic, apply to atoms f and x, count the f's
    for (name, o) in [("naive", NAIVE), ("B,C", BC), ("B,C,eta", BC_ETA)] {
        for (op, f) in [
            ("add", (|a, b| a + b) as fn(usize, usize) -> usize),
            ("mul", |a, b| a * b),
        ] {
            for (m, n) in [(0, 0), (1, 2), (2, 3), (3, 3), (4, 2)] {
                let src = format!("{op} ({}) ({})", show(&church(m)), show(&church(n)));
                let c = ap(
                    ap(to_comb(&lib(&src), o), Cb::At("f".into())),
                    Cb::At("x".into()),
                );
                let r = nf_cb(&c, 100_000).expect("combinator reduction terminates");
                assert_eq!(decode_f_x(&r), Some(f(m, n)), "{name} {src}");
            }
        }
    }
    let sizes: Vec<usize> = [NAIVE, BC, BC_ETA]
        .iter()
        .map(|o| cb_size(&to_comb(&lib("mul"), *o)))
        .collect();
    println!(
        "size of `mul` as combinators: naive {} / B,C {} / B,C,eta {}",
        sizes[0], sizes[1], sizes[2]
    );
    assert!(sizes[1] < sizes[0] && sizes[2] <= sizes[1]);
    // no-eta variants: lambda -> combinator -> lambda keeps the beta normal form
    let (mut checked, mut skipped) = (0, 0);
    for i in 0..1500 {
        let t = random_closed(r, 3 + i % 3);
        let Out::Done(want, _) = run(&t, Strat::Normal, 500, 4000) else {
            continue;
        };
        for o in [NAIVE, BC] {
            let back = from_comb(&to_comb(&t, o));
            match run(&back, Strat::Normal, 1500, 20_000) {
                Out::Done(got, _) => {
                    assert!(
                        alpha_eq(&got, &want),
                        "SKI round trip changed the normal form of {}: {} vs {}",
                        show(&t),
                        show(&got),
                        show(&want)
                    );
                    checked += 1;
                }
                Out::Fuel(_) => skipped += 1,
            }
        }
    }
    println!(
        "lambda -> SKI/BC -> lambda keeps the normal form on {checked} runs ({skipped} skipped for fuel)"
    );
    assert!(checked > 300);
}

fn main() {
    // deep terms and recursion: run on a generous stack
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(|| {
            let mut r = Rng(0x2545_F491_4F6C_DD1D);
            demo_substitution();
            test_parse_print(&mut r);
            test_alpha_and_db(&mut r);
            demo_strategies();
            test_named_vs_db(&mut r);
            test_church_rosser(&mut r);
            test_machines(&mut r);
            test_encodings();
            test_cbv_and_fix();
            test_combinators(&mut r);
            println!("all lambda calculus checks passed");
        })
        .unwrap()
        .join()
        .unwrap();
}
