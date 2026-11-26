//! RECIPE 813 (compilers): algebraic effects and handlers, three ways: CPS with delimited continuations, a free monad, and a stack machine.
//! LEVEL: advanced
//! Use in: language runtimes and libraries that need user-defined control flow: exceptions, generators, async, backtracking, dependency injection.   Run: cargo run --release --example algebraic_effects_handlers
//! COMMON USES: OCaml 5 effect handlers (the base of its schedulers; its continuations are one-shot), the Eff and Koka research languages,
//! and, as a restricted relative, Rust's `async`/`.await` (a function that can suspend and be resumed once by an executor).
//! Course: Plotkin and Pretnar, "Handlers of algebraic effects"; Dybvig, Peyton Jones and Sabry, "A monadic framework for delimited continuations".
#![allow(dead_code)]
//!
//! THE IDEA. `(perform op v)` says WHAT is wanted and nothing about how. `(handle body (return x e) (op p k e') ...)` gives the meaning:
//! when `body` performs `op`, the clause runs with the argument `p` and the continuation `k`: the rest of `body` UP TO this handler (a
//! DELIMITED continuation). The clause may ignore `k` (exceptions), call it once (state, generators, async) or many times
//! (nondeterminism). Handlers here are DEEP: calling `k` puts the same handler back around the resumed code. An op the handler does not
//! mention passes through to the next one out.
//! THREE IMPLEMENTATIONS of that one definition, compared on 500 random programs (result, printed output and errors):
//!  (a) CPS + METACONTINUATION. The continuation `k` is a Rust closure that runs "to the end of the current handler"; a separate linked list
//!      holds one frame per handler with the continuation outside it. `perform` cuts that list at the handling frame; resuming pushes the
//!      cut piece onto the caller's list. Everything is trampolined.
//!  (b) FREE MONAD, in direct Rust. Evaluation returns a DATA description: `Pure(v)` or `Op(name, arg, rest)`. `bind` appends to `rest`;
//!      a handler is a fold over this tree. Shortest to write. It recurses on the host stack, and each `perform` is re-wrapped by every
//!      enclosing `bind`, so deeply left-nested code is quadratic.
//!  (c) STACK MACHINE. The continuation is an explicit `Vec<Frame>`. A handler is a marker frame; `perform` splits the vector at the marker and
//!      the cut-off segment IS the continuation. Multi-shot: copy the segment on each resume. ONE-SHOT mode moves it instead and rejects a
//!      second resume. The machine also reports an op nobody handles to the HOST as a suspension that Rust code can keep, copy and resume.
//! WHAT RUST CAN AND CANNOT DO (last section). An `async fn` is compiled to a state machine, and `.await` is a suspension point; the executor
//! is a handler that resumes it. A future is one-shot: it is polled through `Pin<&mut Self>`, resumes by mutating itself, and in general is
//! not `Clone`, so you cannot run "the rest of the function" twice. Nondeterminism by resuming twice is therefore not expressible with
//! `.await`. The yield point is also fixed in the type: only the executor that polls the future "handles" it.
//!
//! Read: Plotkin and Pretnar (2009, title only); Dybvig, Peyton Jones and Sabry (2007, title only); Pretnar, "An introduction to algebraic
//! effects and handlers" (title only); OCaml manual https://ocaml.org/manual/effects.html ; Eff https://www.eff-lang.org/ ; Koka
//! https://koka-lang.github.io/koka/doc/index.html ; https://en.wikipedia.org/wiki/Delimited_continuation
//! TRY NEXT:
//!   - [S] Make the handlers SHALLOW in machine (c) (do not keep the marker in the captured segment); rewrite the state handler to match.
//!   - [S] Run the nondeterminism demo in one-shot mode and explain the error.
//!   - [M] In the free monad, count how often `bind` re-wraps an `Op` for `(+ (+ (+ (perform a 0) 1) 1) 1)`, then fix it with a queue of continuations.
//!   - [M] Represent the machine's stack as a persistent linked list so multi-shot capture copies nothing; measure `frames copied`.
//!   - [A] Project: add `finally` clauses that run exactly once even when a handler never resumes the continuation.
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

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

// ---------------------------------------------------------------- syntax

type Name = Rc<str>;
type T = Rc<E>;
#[derive(Debug, PartialEq, Clone, Copy)]
enum Op {
    Add,
    Sub,
    Mul,
    Lt,
}
#[derive(Debug, PartialEq)]
enum E {
    Int(i64),
    Var(Name),
    Prim(Op, T, T),
    If(T, T, T),
    Fn(Name, T),
    App(T, T),
    Let(Name, T, T),
    /// (function, parameter, function body, rest)
    Rec(Name, Name, T, T),
    Nil,
    Cons(T, T),
    /// (list, if empty, head name, tail name, otherwise)
    Match(T, T, Name, Name, T),
    Append(T, T),
    /// append the value to the output log and return it
    Print(T),
    Perform(Name, T),
    Handle(T, Rc<Handler>),
}
#[derive(Debug, PartialEq)]
struct Handler {
    ret: (Name, T),
    clauses: Vec<Clause>,
}
#[derive(Debug, PartialEq)]
struct Clause {
    op: Name,
    param: Name,
    k: Name,
    body: T,
}
impl Handler {
    fn clause(&self, op: &str) -> Option<&Clause> {
        self.clauses.iter().find(|c| &*c.op == op)
    }
}

// parser: s-expressions
enum Sx {
    Atom(String),
    List(Vec<Sx>),
}
fn read_sx(toks: &[String], i: &mut usize) -> Sx {
    let t = &toks[*i];
    *i += 1;
    if t != "(" {
        return Sx::Atom(t.clone());
    }
    let mut items = vec![];
    while toks[*i] != ")" {
        items.push(read_sx(toks, i));
    }
    *i += 1;
    Sx::List(items)
}
fn atom(s: &Sx) -> Name {
    match s {
        Sx::Atom(a) => a.as_str().into(),
        Sx::List(_) => panic!("name expected"),
    }
}
fn from_sx(s: &Sx) -> T {
    Rc::new(match s {
        Sx::Atom(a) if a == "nil" => E::Nil,
        Sx::Atom(a) => a
            .parse()
            .map(E::Int)
            .unwrap_or_else(|_| E::Var(a.as_str().into())),
        Sx::List(v) => {
            let head = match &v[0] {
                Sx::Atom(a) => a.as_str(),
                Sx::List(_) => "",
            };
            let at = |i: usize| from_sx(&v[i]);
            let op = |o| E::Prim(o, at(1), at(2));
            match head {
                "+" => op(Op::Add),
                "-" => op(Op::Sub),
                "*" => op(Op::Mul),
                "<" => op(Op::Lt),
                "if" => E::If(at(1), at(2), at(3)),
                "fn" => E::Fn(atom(&v[1]), at(2)),
                "let" => E::Let(atom(&v[1]), at(2), at(3)),
                "rec" => E::Rec(atom(&v[1]), atom(&v[2]), at(3), at(4)),
                "cons" => E::Cons(at(1), at(2)),
                "match" => E::Match(at(1), at(2), atom(&v[3]), atom(&v[4]), at(5)),
                "++" => E::Append(at(1), at(2)),
                "print" => E::Print(at(1)),
                "perform" => E::Perform(atom(&v[1]), at(2)),
                "handle" => {
                    let (mut ret, mut clauses) = (None, vec![]);
                    for c in &v[2..] {
                        let Sx::List(c) = c else {
                            panic!("clause expected")
                        };
                        if matches!(&c[0], Sx::Atom(a) if a == "return") {
                            ret = Some((atom(&c[1]), from_sx(&c[2])));
                        } else {
                            clauses.push(Clause {
                                op: atom(&c[0]),
                                param: atom(&c[1]),
                                k: atom(&c[2]),
                                body: from_sx(&c[3]),
                            });
                        }
                    }
                    E::Handle(
                        at(1),
                        Rc::new(Handler {
                            ret: ret.expect("a handler needs a return clause"),
                            clauses,
                        }),
                    )
                }
                _ => {
                    assert_eq!(v.len(), 2, "application takes one argument");
                    E::App(at(0), at(1))
                }
            }
        }
    })
}
fn parse(src: &str) -> T {
    let toks: Vec<String> = src
        .replace('(', " ( ")
        .replace(')', " ) ")
        .split_whitespace()
        .map(String::from)
        .collect();
    let mut i = 0;
    let e = from_sx(&read_sx(&toks, &mut i));
    assert_eq!(i, toks.len(), "trailing input");
    e
}
fn show(e: &E) -> String {
    match e {
        E::Int(k) => k.to_string(),
        E::Var(x) => x.to_string(),
        E::Prim(o, a, b) => format!(
            "({} {} {})",
            match o {
                Op::Add => "+",
                Op::Sub => "-",
                Op::Mul => "*",
                Op::Lt => "<",
            },
            show(a),
            show(b)
        ),
        E::If(c, t, f) => format!("(if {} {} {})", show(c), show(t), show(f)),
        E::Fn(x, b) => format!("(fn {x} {})", show(b)),
        E::App(f, a) => format!("({} {})", show(f), show(a)),
        E::Let(x, d, b) => format!("(let {x} {} {})", show(d), show(b)),
        E::Rec(f, x, b, r) => format!("(rec {f} {x} {} {})", show(b), show(r)),
        E::Nil => "nil".into(),
        E::Cons(a, b) => format!("(cons {} {})", show(a), show(b)),
        E::Match(l, n, h, t, c) => format!("(match {} {} {h} {t} {})", show(l), show(n), show(c)),
        E::Append(a, b) => format!("(++ {} {})", show(a), show(b)),
        E::Print(a) => format!("(print {})", show(a)),
        E::Perform(o, a) => format!("(perform {o} {})", show(a)),
        E::Handle(b, h) => {
            let cs: String = h
                .clauses
                .iter()
                .map(|c| format!(" ({} {} {} {})", c.op, c.param, c.k, show(&c.body)))
                .collect();
            format!(
                "(handle {} (return {} {}){cs})",
                show(b),
                h.ret.0,
                show(&h.ret.1)
            )
        }
    }
}

// ---------------------------------------------------------------- values shared by all three; `R` is how each represents a resumption

enum Val<R> {
    Int(i64),
    Nil,
    Cons(Rc<(Val<R>, Val<R>)>),
    Fun(Rc<FunVal<R>>),
    Resume(Rc<R>),
}
impl<R> Clone for Val<R> {
    fn clone(&self) -> Self {
        match self {
            Val::Int(k) => Val::Int(*k),
            Val::Nil => Val::Nil,
            Val::Cons(c) => Val::Cons(c.clone()),
            Val::Fun(f) => Val::Fun(f.clone()),
            Val::Resume(r) => Val::Resume(r.clone()),
        }
    }
}
struct FunVal<R> {
    me: Option<Name>,
    param: Name,
    body: T,
    env: Env<R>,
}
type Env<R> = Option<Rc<Binding<R>>>;
struct Binding<R> {
    name: Name,
    value: Val<R>,
    next: Env<R>,
}
fn bind_var<R>(env: &Env<R>, name: &Name, value: Val<R>) -> Env<R> {
    Some(Rc::new(Binding {
        name: name.clone(),
        value,
        next: env.clone(),
    }))
}
fn find<R>(env: &Env<R>, x: &str) -> Result<Val<R>, String> {
    let mut cur = env;
    while let Some(b) = cur {
        if &*b.name == x {
            return Ok(b.value.clone());
        }
        cur = &b.next;
    }
    Err(format!("unbound variable {x}"))
}
fn closure<R>(me: Option<&Name>, param: &Name, body: &T, env: &Env<R>) -> Val<R> {
    Val::Fun(Rc::new(FunVal {
        me: me.cloned(),
        param: param.clone(),
        body: body.clone(),
        env: env.clone(),
    }))
}
/// The environment in which a function body runs.
fn call_env<R>(f: &Rc<FunVal<R>>, arg: Val<R>) -> Env<R> {
    let mut env = f.env.clone();
    if let Some(me) = &f.me {
        env = bind_var(&env, me, Val::Fun(f.clone()));
    }
    bind_var(&env, &f.param, arg)
}
fn int<R>(v: &Val<R>) -> Result<i64, String> {
    match v {
        Val::Int(k) => Ok(*k),
        _ => Err("type error: integer expected".into()),
    }
}
fn prim<R>(o: Op, a: &Val<R>, b: &Val<R>) -> Result<Val<R>, String> {
    let (a, b) = (int(a)?, int(b)?);
    Ok(Val::Int(match o {
        Op::Add => a.wrapping_add(b),
        Op::Sub => a.wrapping_sub(b),
        Op::Mul => a.wrapping_mul(b),
        Op::Lt => i64::from(a < b),
    }))
}
fn cons<R>(h: Val<R>, t: Val<R>) -> Val<R> {
    Val::Cons(Rc::new((h, t)))
}
fn append<R>(a: &Val<R>, b: Val<R>) -> Result<Val<R>, String> {
    let mut items = vec![];
    let mut cur = a;
    loop {
        match cur {
            Val::Nil => break,
            Val::Cons(c) => {
                items.push(c.0.clone());
                cur = &c.1;
            }
            _ => return Err("type error: list expected".into()),
        }
    }
    Ok(items.into_iter().rev().fold(b, |t, h| cons(h, t)))
}
/// Some(None) = empty list, Some(Some((head, tail))) = non-empty.
type ListView<R> = Option<(Val<R>, Val<R>)>;
fn view<R>(v: &Val<R>) -> Result<ListView<R>, String> {
    match v {
        Val::Nil => Ok(None),
        Val::Cons(c) => Ok(Some((c.0.clone(), c.1.clone()))),
        _ => Err("type error: list expected".into()),
    }
}
fn render<R>(v: &Val<R>) -> String {
    match v {
        Val::Int(k) => k.to_string(),
        Val::Nil | Val::Cons(_) => {
            let mut items = vec![];
            let mut cur = v;
            while let Val::Cons(c) = cur {
                items.push(render(&c.0));
                cur = &c.1;
            }
            let tail = if matches!(cur, Val::Nil) {
                String::new()
            } else {
                format!(" . {}", render(cur))
            };
            format!("[{}{tail}]", items.join(" "))
        }
        Val::Fun(_) => "<fn>".into(),
        Val::Resume(_) => "<continuation>".into(),
    }
}

/// Output and a budget on `perform`, which bounds the exponential blow-up multi-shot handlers can cause. All three implementations
/// evaluate in the same order, so they run out at exactly the same point.
#[derive(Default)]
struct World {
    log: RefCell<Vec<String>>,
    performs: Cell<usize>,
}
const PERFORM_BUDGET: usize = 3000;
impl World {
    fn print<R>(&self, v: &Val<R>) {
        self.log.borrow_mut().push(render(v));
    }
    fn count_perform(&self) -> Result<(), String> {
        self.performs.set(self.performs.get() + 1);
        if self.performs.get() > PERFORM_BUDGET {
            Err("perform budget exhausted".into())
        } else {
            Ok(())
        }
    }
}
fn unhandled(op: &str) -> String {
    format!("unhandled effect {op}")
}

/// Everything observable about a run.
#[derive(Debug, PartialEq, Clone)]
struct Obs {
    result: Result<String, String>,
    log: Vec<String>,
    performs: usize,
}

// ---------------------------------------------------------------- (a) CPS with a metacontinuation

mod cps {
    use super::*;
    type V = Val<Resumption>;
    /// "What to do with a value", up to the end of the innermost handler.
    type K = Rc<dyn Fn(V, Meta) -> Bounce>;
    /// One frame per active handler, innermost first.
    type Meta = Option<Rc<HFrame>>;
    pub struct HFrame {
        handler: Rc<Handler>,
        env: Env<Resumption>,
        /// the continuation OUTSIDE this handler
        outer: K,
        next: Meta,
    }
    pub struct Resumption {
        /// innermost segment
        k: K,
        /// handler frames from innermost up to and including the one that caught the op
        frames: Vec<Rc<HFrame>>,
    }
    pub enum Bounce {
        Eval(T, Env<Resumption>, K, Meta),
        Return(K, V, Meta),
        Done(V),
        Fail(String),
    }
    /// The end of any segment: leave the innermost handler through its return clause, or finish.
    fn segment_end() -> K {
        Rc::new(|v, meta: Meta| match meta {
            None => Bounce::Done(v),
            Some(f) => {
                let env = bind_var(&f.env, &f.handler.ret.0, v);
                Bounce::Eval(
                    f.handler.ret.1.clone(),
                    env,
                    f.outer.clone(),
                    f.next.clone(),
                )
            }
        })
    }
    /// Evaluate `e`, then continue with `f(value)`.
    fn then(
        e: &T,
        env: &Env<Resumption>,
        meta: Meta,
        f: impl Fn(V, Meta) -> Bounce + 'static,
    ) -> Bounce {
        Bounce::Eval(e.clone(), env.clone(), Rc::new(f), meta)
    }
    fn or_fail<A>(r: Result<A, String>, f: impl FnOnce(A) -> Bounce) -> Bounce {
        match r {
            Ok(a) => f(a),
            Err(e) => Bounce::Fail(e),
        }
    }
    fn apply(f: V, a: V, k: K, meta: Meta) -> Bounce {
        match f {
            Val::Fun(c) => Bounce::Eval(c.body.clone(), call_env(&c, a), k, meta),
            Val::Resume(r) => {
                // rebuild: the catching handler now returns to the CALLER of the continuation, then the inner handlers go back on top
                let (last, inner) = r.frames.split_last().expect("at least the catching frame");
                let mut m = Some(Rc::new(HFrame {
                    handler: last.handler.clone(),
                    env: last.env.clone(),
                    outer: k,
                    next: meta,
                }));
                for f in inner.iter().rev() {
                    m = Some(Rc::new(HFrame {
                        handler: f.handler.clone(),
                        env: f.env.clone(),
                        outer: f.outer.clone(),
                        next: m,
                    }));
                }
                Bounce::Return(r.k.clone(), a, m)
            }
            _ => Bounce::Fail("type error: calling a non-function".into()),
        }
    }
    fn perform(op: &Name, arg: V, k: K, meta: Meta) -> Bounce {
        let mut frames = vec![];
        let mut cur = meta;
        while let Some(f) = cur {
            frames.push(f.clone());
            if let Some(c) = f.handler.clause(op) {
                let resume = Val::Resume(Rc::new(Resumption { k, frames }));
                let env = bind_var(&bind_var(&f.env, &c.param, arg), &c.k, resume);
                return Bounce::Eval(c.body.clone(), env, f.outer.clone(), f.next.clone());
            }
            cur = f.next.clone();
        }
        Bounce::Fail(unhandled(op))
    }
    fn step(e: &E, env: Env<Resumption>, k: K, meta: Meta, w: &Rc<World>) -> Bounce {
        let ret = |v: V, k: K, meta| Bounce::Return(k, v, meta);
        match e {
            E::Int(n) => ret(Val::Int(*n), k, meta),
            E::Nil => ret(Val::Nil, k, meta),
            E::Var(x) => or_fail(find(&env, x), |v| ret(v, k, meta)),
            E::Fn(x, b) => ret(closure(None, x, b, &env), k, meta),
            E::Rec(f, x, b, rest) => {
                let env = bind_var(&env, f, closure(Some(f), x, b, &env));
                Bounce::Eval(rest.clone(), env, k, meta)
            }
            E::Prim(o, a, b) => {
                let (o, b, env2) = (*o, b.clone(), env.clone());
                then(a, &env, meta, move |va, m| {
                    let k = k.clone();
                    then(&b, &env2, m, move |vb, m| {
                        or_fail(prim(o, &va, &vb), |v| Bounce::Return(k.clone(), v, m))
                    })
                })
            }
            E::Cons(a, b) | E::Append(a, b) => {
                let (is_cons, b, env2) = (matches!(e, E::Cons(..)), b.clone(), env.clone());
                then(a, &env, meta, move |va, m| {
                    let k = k.clone();
                    then(&b, &env2, m, move |vb, m| {
                        let joined = if is_cons {
                            Ok(cons(va.clone(), vb))
                        } else {
                            append(&va, vb)
                        };
                        or_fail(joined, |v| Bounce::Return(k.clone(), v, m))
                    })
                })
            }
            E::If(c, t, f) => {
                let (t, f, env2) = (t.clone(), f.clone(), env.clone());
                then(c, &env, meta, move |v, m| {
                    or_fail(int(&v), |n| {
                        Bounce::Eval(
                            if n != 0 { t.clone() } else { f.clone() },
                            env2.clone(),
                            k.clone(),
                            m,
                        )
                    })
                })
            }
            E::App(f, a) => {
                let (a, env2) = (a.clone(), env.clone());
                then(f, &env, meta, move |fv, m| {
                    let k = k.clone();
                    then(&a, &env2, m, move |av, m| {
                        apply(fv.clone(), av, k.clone(), m)
                    })
                })
            }
            E::Let(x, d, b) => {
                let (x, b, env2) = (x.clone(), b.clone(), env.clone());
                then(d, &env, meta, move |v, m| {
                    Bounce::Eval(b.clone(), bind_var(&env2, &x, v), k.clone(), m)
                })
            }
            E::Match(l, nil, h, t, other) => {
                let (nil, h, t, other, env2) = (
                    nil.clone(),
                    h.clone(),
                    t.clone(),
                    other.clone(),
                    env.clone(),
                );
                then(l, &env, meta, move |v, m| {
                    or_fail(view(&v), |list| match list {
                        None => Bounce::Eval(nil.clone(), env2.clone(), k.clone(), m),
                        Some((hv, tv)) => Bounce::Eval(
                            other.clone(),
                            bind_var(&bind_var(&env2, &h, hv), &t, tv),
                            k.clone(),
                            m,
                        ),
                    })
                })
            }
            E::Print(a) => {
                let w = w.clone();
                then(a, &env, meta, move |v, m| {
                    w.print(&v);
                    Bounce::Return(k.clone(), v, m)
                })
            }
            E::Perform(op, a) => {
                let (op, w) = (op.clone(), w.clone());
                then(a, &env, meta, move |v, m| {
                    or_fail(w.count_perform(), |()| perform(&op, v, k.clone(), m))
                })
            }
            E::Handle(body, h) => {
                let frame = Rc::new(HFrame {
                    handler: h.clone(),
                    env: env.clone(),
                    outer: k,
                    next: meta,
                });
                Bounce::Eval(body.clone(), env, segment_end(), Some(frame))
            }
        }
    }
    pub fn run(program: &T) -> Obs {
        let w = Rc::new(World::default());
        let mut next = Bounce::Eval(program.clone(), None, segment_end(), None);
        let result = loop {
            next = match next {
                Bounce::Eval(e, env, k, meta) => step(&e, env, k, meta, &w),
                Bounce::Return(k, v, meta) => k(v, meta),
                Bounce::Done(v) => break Ok(render(&v)),
                Bounce::Fail(e) => break Err(e),
            };
        };
        Obs {
            result,
            log: w.log.take(),
            performs: w.performs.get(),
        }
    }
}

// ---------------------------------------------------------------- (b) free monad

mod free {
    use super::*;
    type V = Val<Resumption>;
    type Rest = Rc<dyn Fn(V) -> Comp>;
    pub struct Resumption(Rest);
    /// A computation as data: finished, or asking for an operation with the rest of itself as a function.
    pub enum Comp {
        Pure(V),
        Op(Name, V, Rest),
        Fail(String),
    }
    use Comp::{Fail, Pure};
    fn bind(c: Comp, f: Rc<dyn Fn(V) -> Comp>) -> Comp {
        match c {
            Pure(v) => f(v),
            Fail(e) => Fail(e),
            Comp::Op(op, arg, rest) => {
                Comp::Op(op, arg, Rc::new(move |x| bind(rest(x), f.clone())))
            }
        }
    }
    fn and_then(c: Comp, f: impl Fn(V) -> Comp + 'static) -> Comp {
        bind(c, Rc::new(f))
    }
    fn lift(r: Result<V, String>) -> Comp {
        r.map_or_else(Fail, Pure)
    }
    /// A handler is a fold over the computation tree.
    fn handle(c: Comp, h: Rc<Handler>, env: Env<Resumption>, w: Rc<World>) -> Comp {
        match c {
            Pure(v) => eval(&h.ret.1, &bind_var(&env, &h.ret.0, v), &w),
            Fail(e) => Fail(e),
            Comp::Op(op, arg, rest) => {
                // deep: the resumed computation is handled by the same handler again
                let again: Rest = {
                    let (h, env, w) = (h.clone(), env.clone(), w.clone());
                    Rc::new(move |x| handle(rest(x), h.clone(), env.clone(), w.clone()))
                };
                match h.clause(&op) {
                    Some(cl) => {
                        let resume = Val::Resume(Rc::new(Resumption(again)));
                        eval(
                            &cl.body,
                            &bind_var(&bind_var(&env, &cl.param, arg), &cl.k, resume),
                            &w,
                        )
                    }
                    None => Comp::Op(op, arg, again),
                }
            }
        }
    }
    fn apply(f: V, a: V, w: &Rc<World>) -> Comp {
        match f {
            Val::Fun(c) => eval(&c.body, &call_env(&c, a), w),
            Val::Resume(r) => (r.0)(a),
            _ => Fail("type error: calling a non-function".into()),
        }
    }
    /// Evaluate `a`, then `b`, then combine.
    fn two(
        a: &T,
        b: &T,
        env: &Env<Resumption>,
        w: &Rc<World>,
        f: impl Fn(&V, V, &Rc<World>) -> Comp + 'static,
    ) -> Comp {
        let (b, env2, w2, f) = (b.clone(), env.clone(), w.clone(), Rc::new(f));
        and_then(eval(a, env, w), move |va| {
            let (f, w3) = (f.clone(), w2.clone());
            and_then(eval(&b, &env2, &w2), move |vb| f(&va, vb, &w3))
        })
    }
    fn eval(e: &E, env: &Env<Resumption>, w: &Rc<World>) -> Comp {
        let (env2, w2) = (env.clone(), w.clone());
        match e {
            E::Int(n) => Pure(Val::Int(*n)),
            E::Nil => Pure(Val::Nil),
            E::Var(x) => lift(find(env, x)),
            E::Fn(x, b) => Pure(closure(None, x, b, env)),
            E::Rec(f, x, b, rest) => eval(rest, &bind_var(env, f, closure(Some(f), x, b, env)), w),
            E::Prim(o, a, b) => {
                let o = *o;
                two(a, b, env, w, move |x, y, _| lift(prim(o, x, &y)))
            }
            E::Cons(a, b) => two(a, b, env, w, |x, y, _| Pure(cons(x.clone(), y))),
            E::Append(a, b) => two(a, b, env, w, |x, y, _| lift(append(x, y))),
            E::App(f, a) => two(f, a, env, w, |f, a, w| apply(f.clone(), a, w)),
            E::If(c, t, f) => {
                let (t, f) = (t.clone(), f.clone());
                and_then(eval(c, env, w), move |v| match int(&v) {
                    Ok(n) => eval(if n != 0 { &t } else { &f }, &env2, &w2),
                    Err(e) => Fail(e),
                })
            }
            E::Let(x, d, b) => {
                let (x, b) = (x.clone(), b.clone());
                and_then(eval(d, env, w), move |v| {
                    eval(&b, &bind_var(&env2, &x, v), &w2)
                })
            }
            E::Match(l, nil, h, t, other) => {
                let (nil, h, t, other) = (nil.clone(), h.clone(), t.clone(), other.clone());
                and_then(eval(l, env, w), move |v| match view(&v) {
                    Ok(None) => eval(&nil, &env2, &w2),
                    Ok(Some((hv, tv))) => {
                        eval(&other, &bind_var(&bind_var(&env2, &h, hv), &t, tv), &w2)
                    }
                    Err(e) => Fail(e),
                })
            }
            E::Print(a) => and_then(eval(a, env, w), move |v| {
                w2.print(&v);
                Pure(v)
            }),
            E::Perform(op, a) => {
                let op = op.clone();
                and_then(eval(a, env, w), move |v| match w2.count_perform() {
                    Ok(()) => Comp::Op(op.clone(), v, Rc::new(Pure)),
                    Err(e) => Fail(e),
                })
            }
            E::Handle(body, h) => handle(eval(body, env, w), h.clone(), env2, w2),
        }
    }
    pub fn run(program: &T) -> Obs {
        let w = Rc::new(World::default());
        let result = match eval(program, &None, &w) {
            Pure(v) => Ok(render(&v)),
            Fail(e) => Err(e),
            Comp::Op(op, ..) => Err(unhandled(&op)),
        };
        Obs {
            result,
            log: w.log.take(),
            performs: w.performs.get(),
        }
    }
}

// ---------------------------------------------------------------- (c) the stack machine

mod machine {
    use super::*;
    pub type V = Val<Resumption>;
    type En = Env<Resumption>;
    /// "What to do with the value being returned." All data, all cheap to clone.
    #[derive(Clone)]
    enum Frame {
        /// evaluate the second operand next
        Second(Two, T, En),
        /// both operands are known after this value
        Combine(Two, V),
        If(T, T, En),
        Let(Name, T, En),
        Match(T, Name, Name, T, En),
        Print,
        Perform(Name),
        /// the delimiter
        Handle(Rc<Handler>, En),
    }
    #[derive(Clone, Copy)]
    enum Two {
        Prim(Op),
        Cons,
        Append,
        Apply,
    }
    /// A captured stack segment, outermost frame (the handler marker) first.
    pub struct Resumption {
        /// `None` once a one-shot continuation has been used
        segment: RefCell<Option<Vec<Frame>>>,
    }
    enum Control {
        Eval(T, En),
        Return(V),
    }
    #[derive(Default, Clone, Copy)]
    pub struct Stats {
        pub captures: usize,
        pub frames_copied: usize,
    }
    pub struct Machine {
        control: Control,
        stack: Vec<Frame>,
        one_shot: bool,
        pub stats: Stats,
        pub world: Rc<World>,
    }
    pub enum Stop {
        Done(V),
        /// no handler in the program wants this op: the host decides, and may call `resume` on (a copy of) the machine
        Suspended(Name, V),
    }
    impl Machine {
        pub fn new(program: &T, one_shot: bool) -> Machine {
            Machine {
                control: Control::Eval(program.clone(), None),
                stack: vec![],
                one_shot,
                stats: Stats::default(),
                world: Rc::default(),
            }
        }
        /// An independent copy of a suspended computation: the host-level form of multi-shot. It copies the stack but shares values, which is
        /// safe in multi-shot mode (captured segments are never mutated).
        pub fn fork(&self) -> Machine {
            let control = match &self.control {
                Control::Eval(e, env) => Control::Eval(e.clone(), env.clone()),
                Control::Return(v) => Control::Return(v.clone()),
            };
            let world = World {
                log: RefCell::new(self.world.log.borrow().clone()),
                performs: self.world.performs.clone(),
            };
            Machine {
                control,
                stack: self.stack.clone(),
                one_shot: self.one_shot,
                stats: self.stats,
                world: Rc::new(world),
            }
        }
        pub fn resume(&mut self, value: V) {
            self.control = Control::Return(value);
        }
        pub fn run(&mut self) -> Result<Stop, String> {
            loop {
                let control = std::mem::replace(&mut self.control, Control::Return(Val::Nil));
                match control {
                    Control::Eval(e, env) => self.eval(&e, env)?,
                    Control::Return(v) => match self.stack.pop() {
                        None => return Ok(Stop::Done(v)),
                        Some(Frame::Perform(op)) => {
                            self.world.count_perform()?;
                            if let Some(stop) = self.perform(op, v)? {
                                return Ok(stop);
                            }
                        }
                        Some(frame) => self.ret(frame, v)?,
                    },
                }
            }
        }
        fn go(&mut self, e: &T, env: &En) {
            self.control = Control::Eval(e.clone(), env.clone());
        }
        fn value(&mut self, v: V) {
            self.control = Control::Return(v);
        }
        fn two(&mut self, op: Two, a: &T, b: &T, env: En) {
            self.stack.push(Frame::Second(op, b.clone(), env.clone()));
            self.go(a, &env);
        }
        fn eval(&mut self, e: &E, env: En) -> Result<(), String> {
            match e {
                E::Int(n) => self.value(Val::Int(*n)),
                E::Nil => self.value(Val::Nil),
                E::Var(x) => self.value(find(&env, x)?),
                E::Fn(x, b) => self.value(closure(None, x, b, &env)),
                E::Rec(f, x, b, rest) => {
                    let env = bind_var(&env, f, closure(Some(f), x, b, &env));
                    self.go(rest, &env);
                }
                E::Prim(o, a, b) => self.two(Two::Prim(*o), a, b, env),
                E::Cons(a, b) => self.two(Two::Cons, a, b, env),
                E::Append(a, b) => self.two(Two::Append, a, b, env),
                E::App(f, a) => self.two(Two::Apply, f, a, env),
                E::If(c, t, f) => {
                    self.stack
                        .push(Frame::If(t.clone(), f.clone(), env.clone()));
                    self.go(c, &env);
                }
                E::Let(x, d, b) => {
                    self.stack
                        .push(Frame::Let(x.clone(), b.clone(), env.clone()));
                    self.go(d, &env);
                }
                E::Match(l, nil, h, t, other) => {
                    self.stack.push(Frame::Match(
                        nil.clone(),
                        h.clone(),
                        t.clone(),
                        other.clone(),
                        env.clone(),
                    ));
                    self.go(l, &env);
                }
                E::Print(a) => {
                    self.stack.push(Frame::Print);
                    self.go(a, &env);
                }
                E::Perform(op, a) => {
                    self.stack.push(Frame::Perform(op.clone()));
                    self.go(a, &env);
                }
                E::Handle(body, h) => {
                    self.stack.push(Frame::Handle(h.clone(), env.clone()));
                    self.go(body, &env);
                }
            }
            Ok(())
        }
        fn ret(&mut self, frame: Frame, v: V) -> Result<(), String> {
            match frame {
                Frame::Second(op, b, env) => {
                    self.stack.push(Frame::Combine(op, v));
                    self.go(&b, &env);
                }
                Frame::Combine(op, first) => match op {
                    Two::Prim(o) => self.value(prim(o, &first, &v)?),
                    Two::Cons => self.value(cons(first, v)),
                    Two::Append => self.value(append(&first, v)?),
                    Two::Apply => self.apply(first, v)?,
                },
                Frame::If(t, f, env) => self.go(if int(&v)? != 0 { &t } else { &f }, &env),
                Frame::Let(x, b, env) => self.go(&b, &bind_var(&env, &x, v)),
                Frame::Match(nil, h, t, other, env) => match view(&v)? {
                    None => self.go(&nil, &env),
                    Some((hv, tv)) => self.go(&other, &bind_var(&bind_var(&env, &h, hv), &t, tv)),
                },
                Frame::Print => {
                    self.world.print(&v);
                    self.value(v);
                }
                // the body finished normally: run the return clause outside the handler
                Frame::Handle(h, env) => self.go(&h.ret.1, &bind_var(&env, &h.ret.0, v)),
                Frame::Perform(_) => unreachable!("handled by `run`"),
            }
            Ok(())
        }
        fn apply(&mut self, f: V, a: V) -> Result<(), String> {
            match f {
                Val::Fun(c) => self.go(&c.body, &call_env(&c, a)),
                Val::Resume(r) => {
                    if self.one_shot {
                        let segment = r
                            .segment
                            .borrow_mut()
                            .take()
                            .ok_or("continuation resumed twice")?;
                        self.stack.extend(segment); // moved, not copied
                    } else {
                        let segment = r.segment.borrow();
                        let segment = segment
                            .as_ref()
                            .expect("multi-shot segments are never taken");
                        self.stats.frames_copied += segment.len();
                        self.stack.extend_from_slice(segment);
                    }
                    self.value(a);
                }
                _ => return Err("type error: calling a non-function".into()),
            }
            Ok(())
        }
        fn perform(&mut self, op: Name, arg: V) -> Result<Option<Stop>, String> {
            let marker = self
                .stack
                .iter()
                .rposition(|f| matches!(f, Frame::Handle(h, _) if h.clause(&op).is_some()));
            let Some(at) = marker else {
                return Ok(Some(Stop::Suspended(op, arg)));
            };
            let segment = self.stack.split_off(at);
            let Frame::Handle(h, env) = &segment[0] else {
                unreachable!("position of a handler")
            };
            let (h, env) = (h.clone(), env.clone());
            let clause = h.clause(&op).expect("checked above");
            self.stats.captures += 1;
            let resume = Val::Resume(Rc::new(Resumption {
                segment: RefCell::new(Some(segment)),
            }));
            self.go(
                &clause.body,
                &bind_var(&bind_var(&env, &clause.param, arg), &clause.k, resume),
            );
            Ok(None)
        }
    }
    pub fn run_with(program: &T, one_shot: bool) -> (Obs, Stats) {
        let mut m = Machine::new(program, one_shot);
        let result = match m.run() {
            Ok(Stop::Done(v)) => Ok(render(&v)),
            Ok(Stop::Suspended(op, _)) => Err(unhandled(&op)),
            Err(e) => Err(e),
        };
        (
            Obs {
                result,
                log: m.world.log.borrow().clone(),
                performs: m.world.performs.get(),
            },
            m.stats,
        )
    }
}

/// Run all three and insist that they observe the same thing.
fn run_all(program: &T) -> Obs {
    let c = machine::run_with(program, false).0;
    let a = cps::run(program);
    assert_eq!(a, c, "CPS and stack machine differ on {}", show(program));
    let b = free::run(program);
    assert_eq!(
        b,
        c,
        "free monad and stack machine differ on {}",
        show(program)
    );
    c
}

// ---------------------------------------------------------------- handlers written in the language

/// State by parameter passing: the handler turns the computation into a function of the current state. Returns [result . final-state].
fn with_state(body: &str, init: i64) -> String {
    format!(
        "((handle {body} (return x (fn s (cons x s))) (get u k (fn s ((k s) s))) (put v k (fn s ((k 0) v)))) {init})"
    )
}
/// All outcomes of all choices, depth first.
fn all_choices(body: &str) -> String {
    format!("(handle {body} (return x (cons x nil)) (choose u k (++ (k 0) (k 1))) (fail u k nil))")
}
/// `(pick n)` chooses a number in 1..=n out of binary choices; `(guard c)` fails unless c.
const PICK: &str = "(rec pick n (if (< n 2) (if (< n 1) (perform fail 0) 1) (if (perform choose 0) n (pick (- n 1))))";

fn demo_effects() {
    println!("== one `perform`, many meanings ==");
    let show_run = |label: &str, src: &str, want: &str| {
        let o = run_all(&parse(src));
        let got = o.result.clone().unwrap_or_else(|e| format!("error: {e}"));
        println!(
            "  {label:<52} -> {got}{}",
            if o.log.is_empty() {
                String::new()
            } else {
                format!("   printed {}", o.log.join(" "))
            }
        );
        assert_eq!(got, want, "{src}");
    };
    // exceptions: never resume
    let body = "(+ 1 (+ (print 2) (perform raise 40)))";
    show_run(
        "exception: the clause drops k",
        &format!("(handle {body} (return x x) (raise m k (+ m 100)))"),
        "140",
    );
    show_run(
        "same body, but resume with a default instead",
        &format!("(handle {body} (return x x) (raise m k (k 7)))"),
        "10",
    );
    show_run("no handler at all", body, "error: unhandled effect raise");
    show_run(
        "state: counter",
        &with_state(
            "(let a (perform get 0) (let _ (perform put (+ a 5)) (* (perform get 0) 2)))",
            1,
        ),
        "[12 . 6]",
    );
    // generators
    let produce = "(rec go n (if (< 4 n) 0 (let _ (perform yield (* n n)) (go (+ n 1)))) (go 1))";
    show_run(
        "generator: collect every yield",
        &format!("(handle {produce} (return x nil) (yield v k (cons v (k 0))))"),
        "[1 4 9 16]",
    );
    // pull-style: the handler hands the continuation to the consumer, who decides when (and whether) to go on
    let pull = format!(
        "(let g (handle {produce} (return x nil) (yield v k (cons v k))) (match g 0 first more (match (more 0) 0 second rest (+ (* first 100) second))))"
    );
    show_run("generator: pull two values, abandon the rest", &pull, "104");
    // nondeterminism: every resume explores one branch; the handler appends the answers
    let triples = format!(
        "{PICK} (let a (pick 8) (let b (pick 8) (let c (pick 10)
           (if (< a b)
             (let diff (- (+ (* a a) (* b b)) (* c c))
               (if (< diff 0) (perform fail 0) (if (< 0 diff) (perform fail 0) (cons a (cons b (cons c nil))))))
             (perform fail 0))))))"
    );
    let mut brute = vec![];
    for a in 1..=8 {
        for b in a + 1..=8 {
            brute.extend(
                (1..=10)
                    .filter(|c| a * a + b * b == c * c)
                    .map(|c| format!("[{a} {b} {c}]")),
            );
        }
    }
    show_run(
        "nondeterminism: Pythagorean triples (a<=8, c<=10)",
        &all_choices(&triples),
        &format!("[{}]", brute.join(" ")),
    );
}

fn demo_scheduler() -> Vec<String> {
    println!("== async/await-style cooperative scheduler as a handler ==");
    // A queue entry is a function of a dummy that yields "a function of the rest of the queue".
    // `task` wraps a thread in the scheduling handler; `run` starts the next entry.
    // `count` takes the list [id left] because functions have one parameter
    let src = "
    (rec run q (match q 0 next others ((next 0) others))
    (rec task thread
        (handle (thread 0)
            (return x (fn q (run q)))
            (yield u k (fn q (run (++ q (cons (fn d (k 0)) nil)))))
            (spawn child k (fn q ((k 0) (++ q (cons (fn d (task child)) nil))))))
    (rec count spec
        (match spec 0 id more (match more 0 left unused
            (if (< 0 left)
                (let _ (print (+ (* id 10) left)) (let _ (perform yield 0) (count (cons id (cons (- left 1) nil)))))
                0)))
    (let worker (fn id (fn n (fn d (count (cons id (cons n nil))))))
    (let main (fn d
        (let _ (perform spawn ((worker 1) 3))
        (let _ (perform spawn ((worker 2) 2))
        (print 0))))
    (run (cons (fn d (task main)) nil)))))))";
    let o = run_all(&parse(src));
    println!("  result {:?}, trace {}", o.result, o.log.join(" "));
    assert_eq!(o.result, Ok("0".into()));
    assert_eq!(
        o.log,
        ["0", "13", "22", "12", "21", "11"],
        "round-robin interleaving of two workers"
    );
    o.log
}

fn demo_composition_order() {
    println!("== the order of handlers is part of the meaning ==");
    let body = "(let _ (if (perform choose 0) (perform put (+ (perform get 0) 1)) (perform put (+ (perform get 0) 10))) (perform get 0))";
    let global = run_all(&parse(&with_state(&all_choices(body), 0)))
        .result
        .expect("runs");
    let local = run_all(&parse(&all_choices(&with_state(body, 0))))
        .result
        .expect("runs");
    println!("  state outside choice (one store, survives backtracking) : {global}");
    println!("  choice outside state (each branch gets its own store)   : {local}");
    assert_eq!(global, "[[10 11] . 11]");
    assert_eq!(local, "[[10 . 10] [1 . 1]]");
}

fn demo_one_shot() {
    println!("== one-shot vs multi-shot continuations (machine c) ==");
    let once = with_state(
        "(let a (perform get 0) (let _ (perform put (+ a 1)) (perform get 0)))",
        41,
    );
    let twice = all_choices("(+ (if (perform choose 0) 1 2) 10)");
    for (label, src) in [
        ("state (each k used once)", &once),
        ("choice (k used twice)", &twice),
    ] {
        let p = parse(src);
        let (multi, ms) = machine::run_with(&p, false);
        let (single, ss) = machine::run_with(&p, true);
        println!(
            "  {label:<26} multi-shot {:<8} ({} captures, {} frames copied)   one-shot {}",
            multi.result.clone().unwrap(),
            ms.captures,
            ms.frames_copied,
            single.result.clone().unwrap_or_else(|e| e)
        );
        assert_eq!(ss.frames_copied, 0, "one-shot resume moves the segment");
        if label.starts_with("state") {
            assert_eq!(single, multi);
        } else {
            assert_eq!(single.result, Err("continuation resumed twice".into()));
        }
        assert!(ms.frames_copied > 0);
    }
}

// ---------------------------------------------------------------- the host as outermost handler: a resumable parser

/// Sums comma-separated numbers, asking for one character code at a time; a negative code means end of input.
const SUMMER: &str = "(rec go total (fn cur (let c (perform next 0)
    (if (< c 0) (+ total cur)
      (if (< c 48) ((go (+ total cur)) 0)
        ((go total) (+ (* cur 10) (- c 48))))))) ((go 0) 0))";
/// Feed one chunk; the machine is left suspended at its next request.
fn feed(m: &mut machine::Machine, chunk: &str) {
    for byte in chunk.bytes() {
        m.resume(Val::Int(i64::from(byte)));
        match m.run() {
            Ok(machine::Stop::Suspended(op, _)) => assert_eq!(&*op, "next"),
            _ => panic!("the parser stopped early"),
        }
    }
}
fn finish(m: &mut machine::Machine) -> i64 {
    m.resume(Val::Int(-1));
    match m.run() {
        Ok(machine::Stop::Done(Val::Int(n))) => n,
        _ => panic!("expected a final number"),
    }
}
fn start_parser() -> machine::Machine {
    let mut m = machine::Machine::new(&parse(SUMMER), false);
    assert!(matches!(m.run(), Ok(machine::Stop::Suspended(..))));
    m
}
fn demo_resumable_parser(rng: &mut Rng) {
    println!(
        "== resumable parser: input arrives in chunks, the parse is a suspended machine in between =="
    );
    for case in 0..300 {
        let numbers: Vec<i64> = (0..1 + rng.below(8))
            .map(|_| rng.below(10_000) as i64)
            .collect();
        let text: String = numbers
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut m = start_parser();
        let mut at = 0;
        while at < text.len() {
            let end = (at + 1 + rng.below(6)).min(text.len());
            feed(&mut m, &text[at..end]);
            at = end;
        }
        assert_eq!(
            finish(&mut m),
            numbers.iter().sum::<i64>(),
            "case {case}: {text}"
        );
    }
    // multi-shot at host level: keep the parse of a common prefix and continue it in two different ways
    let mut prefix = start_parser();
    feed(&mut prefix, "100,20");
    let (mut a, mut b) = (prefix.fork(), prefix.fork());
    feed(&mut a, ",3");
    feed(&mut b, "7");
    let (a, b, original) = (finish(&mut a), finish(&mut b), finish(&mut prefix));
    println!(
        "  300 random inputs in random chunks are summed correctly; prefix \"100,20\" + \",3\" = {a}, + \"7\" = {b}, + nothing = {original}"
    );
    assert_eq!((a, b, original), (123, 307, 120));
}

// ---------------------------------------------------------------- the same scheduler with Rust's async

/// Pending exactly once: the Rust spelling of `(perform yield 0)`.
struct Yield(bool);
impl Future for Yield {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if std::mem::replace(&mut self.0, true) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
async fn rust_worker(id: i64, n: i64, log: Rc<RefCell<Vec<String>>>) {
    for left in (1..=n).rev() {
        log.borrow_mut().push((id * 10 + left).to_string());
        Yield(false).await;
    }
}
fn demo_rust_async(in_language: &[String]) {
    println!("== the same schedule with Rust futures: the executor plays the handler ==");
    let log = Rc::new(RefCell::new(vec!["0".to_string()]));
    let mut queue: VecDeque<Pin<Box<dyn Future<Output = ()>>>> = VecDeque::new();
    queue.push_back(Box::pin(rust_worker(1, 3, log.clone())));
    queue.push_back(Box::pin(rust_worker(2, 2, log.clone())));
    let mut cx = Context::from_waker(Waker::noop());
    let mut polls = 0;
    while let Some(mut task) = queue.pop_front() {
        polls += 1;
        if task.as_mut().poll(&mut cx).is_pending() {
            queue.push_back(task); // resume later; there is no way to also keep a copy and resume it twice
        }
    }
    println!("  trace {} after {polls} polls", log.borrow().join(" "));
    assert_eq!(*log.borrow(), in_language);
}

// ---------------------------------------------------------------- random programs

struct Gen<'r> {
    rng: &'r mut Rng,
    fresh: usize,
}
#[derive(Clone, Default)]
struct Scope {
    /// integer variables
    ints: Vec<String>,
    /// operations some enclosing handler provides
    ops: Vec<&'static str>,
    state: bool,
    choice: bool,
}
const OPS: [&str; 3] = ["a", "b", "c"];
const SUM: &str = "(rec sum l (match l 0 h t (+ h (sum t)))";
impl Gen<'_> {
    fn name(&mut self, base: &str) -> String {
        self.fresh += 1;
        format!("{base}{}", self.fresh)
    }
    fn leaf(&mut self, s: &Scope) -> String {
        if !s.ints.is_empty() && self.rng.below(2) == 0 {
            s.ints[self.rng.below(s.ints.len())].clone()
        } else {
            self.rng.below(10).to_string()
        }
    }
    /// An expression of type integer.
    fn int(&mut self, s: &Scope, d: usize) -> String {
        if d == 0 {
            return self.leaf(s);
        }
        let d = d - 1;
        match self.rng.below(17) {
            0 => self.leaf(s),
            1 => format!(
                "({} {} {})",
                ["+", "-", "*", "<"][self.rng.below(4)],
                self.int(s, d),
                self.int(s, d)
            ),
            2 => format!(
                "(if {} {} {})",
                self.int(s, d),
                self.int(s, d),
                self.int(s, d)
            ),
            3 => {
                let (x, def) = (self.name("x"), self.int(s, d));
                let mut inner = s.clone();
                inner.ints.push(x.clone());
                format!("(let {x} {def} {})", self.int(&inner, d))
            }
            4 => {
                let x = self.name("x");
                let mut inner = s.clone();
                inner.ints.push(x.clone());
                format!("((fn {x} {}) {})", self.int(&inner, d), self.int(s, d))
            }
            5 => format!("(print {})", self.int(s, d)),
            6..=8 if !s.ops.is_empty() => format!(
                "(perform {} {})",
                s.ops[self.rng.below(s.ops.len())],
                self.int(s, d)
            ),
            9 if s.state => {
                if self.rng.below(2) == 0 {
                    "(perform get 0)".into()
                } else {
                    format!("(perform put {})", self.int(s, d))
                }
            }
            10 if s.choice => {
                if self.rng.below(5) == 0 {
                    "(perform fail 0)".into()
                } else {
                    "(perform choose 0)".into()
                }
            }
            // rarely, an op that may have no handler: all three must fail alike
            11 if self.rng.below(12) == 0 => {
                format!("(perform {} {})", OPS[self.rng.below(3)], self.int(s, d))
            }
            11..=13 => self.handler(s, d),
            14 => {
                let mut inner = s.clone();
                inner.state = true;
                let (body, init) = (self.int(&inner, d), self.rng.below(10));
                format!(
                    "(match {} 0 r st (+ r (* 7 st)))",
                    with_state(&body, init as i64)
                )
            }
            15 => {
                let mut inner = s.clone();
                inner.choice = true;
                format!("{SUM} (sum {}))", all_choices(&self.int(&inner, d)))
            }
            _ => format!("(+ {} {})", self.int(s, d), self.int(s, d)),
        }
    }
    /// A handler for a random subset of OPS with random clause shapes: drop, resume once, resume twice, resume nested.
    fn handler(&mut self, s: &Scope, d: usize) -> String {
        let mut handled: Vec<&'static str> = OPS
            .iter()
            .copied()
            .filter(|_| self.rng.below(2) == 0)
            .collect();
        if handled.is_empty() {
            handled.push(OPS[self.rng.below(3)]);
        }
        let mut inner = s.clone();
        for op in &handled {
            if !inner.ops.contains(op) {
                inner.ops.push(op);
            }
        }
        let body = self.int(&inner, d);
        let x = self.name("r");
        let mut ret_scope = s.clone();
        ret_scope.ints.push(x.clone());
        let mut out = format!(
            "(handle {body} (return {x} {})",
            self.int(&ret_scope, d.min(2))
        );
        for op in handled {
            let (p, k) = (self.name("p"), self.name("k"));
            let mut cs = s.clone(); // a clause runs OUTSIDE its own handler
            cs.ints.push(p.clone());
            let clause_depth = d.min(2);
            let (e1, e2) = (self.int(&cs, clause_depth), self.int(&cs, clause_depth));
            let clause = match self.rng.below(6) {
                0 => e1,
                1 | 2 => format!("({k} {e1})"),
                3 => format!("(+ ({k} {e1}) {e2})"),
                4 => format!("(+ ({k} {e1}) ({k} {e2}))"),
                _ => format!("({k} ({k} {e1}))"),
            };
            out.push_str(&format!(" ({op} {p} {k} {clause})"));
        }
        out.push(')');
        out
    }
}
fn random_program(rng: &mut Rng) -> T {
    let mut g = Gen { rng, fresh: 0 };
    loop {
        let src = g.handler(&Scope::default(), 6);
        let e = parse(&src);
        assert_eq!(parse(&show(&e)), e, "printer and parser round trip");
        if src.matches("perform").count() >= 2 {
            return e;
        }
    }
}
fn test_agreement(rng: &mut Rng) {
    let (
        mut ok,
        mut unhandled_runs,
        mut budget,
        mut performs,
        mut printed,
        mut multi,
        mut one_shot_same,
    ) = (0, 0, 0, 0, 0, 0, 0);
    for _ in 0..500 {
        let p = random_program(rng);
        let o = run_all(&p);
        performs += o.performs;
        printed += o.log.len();
        match &o.result {
            Ok(_) => ok += 1,
            Err(e) if e.starts_with("unhandled effect") => unhandled_runs += 1,
            Err(e) if e == "perform budget exhausted" => budget += 1,
            Err(e) => panic!(
                "generated programs are well typed, got `{e}` for {}",
                show(&p)
            ),
        }
        // one-shot execution is either identical or stops at the first second resume, having printed a prefix of the same output
        let single = machine::run_with(&p, true).0;
        if single.result == Err("continuation resumed twice".into()) {
            multi += 1;
            assert!(o.log.starts_with(&single.log));
        } else {
            assert_eq!(single, o);
            one_shot_same += 1;
        }
    }
    println!(
        "== 500 random programs: all three implementations agree on result, output and number of performs =="
    );
    println!(
        "  {ok} returned a value, {unhandled_runs} hit an unhandled effect, {budget} hit the perform budget; {performs} performs, {printed} lines printed"
    );
    println!(
        "  one-shot mode: identical on {one_shot_same} programs, rejected a second resume on {multi}"
    );
    assert!(
        ok > 300 && multi > 30 && one_shot_same > 100 && performs > 1000,
        "the sample must exercise both styles"
    );
}

/// The four equations of a single state cell, checked in random surrounding programs.
fn test_state_laws(rng: &mut Rng) {
    let laws: [(&str, &str, &str); 4] = [
        (
            "get; put it back = get",
            "(let a (perform get 0) (let _ (perform put a) REST))",
            "(let a (perform get 0) REST)",
        ),
        (
            "put v; get       = put v; v",
            "(let _ (perform put VAL1) (let a (perform get 0) REST))",
            "(let _ (perform put VAL1) (let a VAL1 REST))",
        ),
        (
            "put v; put w     = put w",
            "(let a 0 (let _ (perform put VAL1) (let _ (perform put VAL2) REST)))",
            "(let a 0 (let _ (perform put VAL2) REST))",
        ),
        (
            "get; get         = get once",
            "(let a (perform get 0) (let b (perform get 0) (+ (* 3 b) REST)))",
            "(let a (perform get 0) (let b a (+ (* 3 b) REST)))",
        ),
    ];
    println!("== state laws, each in 200 random contexts ==");
    for (name, lhs, rhs) in laws {
        let mut compared = 0;
        for _ in 0..200 {
            let mut g = Gen {
                rng: &mut *rng,
                fresh: 0,
            };
            // REST may read `a`, uses state, and can contain its own handlers (including multi-shot ones, which see ONE shared store)
            let scope = Scope {
                ints: vec!["a".into()],
                state: true,
                ..Scope::default()
            };
            let rest = g.int(&scope, 4);
            let (v, w, init) = (
                g.rng.below(50).to_string(),
                g.rng.below(50).to_string(),
                g.rng.below(50) as i64,
            );
            let fill = |t: &str| {
                with_state(
                    &t.replace("VAL1", &v)
                        .replace("VAL2", &w)
                        .replace("REST", &rest),
                    init,
                )
            };
            let (l, r) = (run_all(&parse(&fill(lhs))), run_all(&parse(&fill(rhs))));
            // the two sides perform a different NUMBER of ops, so only results and output are compared
            if [&l, &r]
                .iter()
                .all(|o| o.result != Err("perform budget exhausted".into()))
            {
                assert_eq!(
                    (&l.result, &l.log),
                    (&r.result, &r.log),
                    "law `{name}` fails with REST = {rest}"
                );
                compared += 1;
            }
        }
        assert!(
            compared > 180,
            "too few contexts finished within the budget: {compared}"
        );
        println!("  holds in {compared} contexts: {name}");
    }
    // the laws belong to the HANDLER: one that logs every write breaks `put v; put w = put w`
    let logging = |body: &str| {
        format!(
            "((handle {body} (return x (fn s x)) (get u k (fn s ((k s) s))) (put v k (fn s ((k 0) (print v))))) 0)"
        )
    };
    let two = run_all(&parse(&logging("(let _ (perform put 1) (perform put 2))")));
    let one = run_all(&parse(&logging("(perform put 2)")));
    println!(
        "  but not for a handler that prints each write: {:?} vs {:?}",
        two.log, one.log
    );
    assert_ne!(two.log, one.log);
}

fn main() {
    std::thread::Builder::new()
        .stack_size(512 << 20) // the free monad recurses on the host stack
        .spawn(|| {
            demo_effects();
            let trace = demo_scheduler();
            demo_composition_order();
            demo_one_shot();
            let mut rng = Rng(0x0813_EFFE_C75A_11D5);
            demo_resumable_parser(&mut rng);
            demo_rust_async(&trace);
            test_agreement(&mut rng);
            test_state_laws(&mut rng);
            println!("all algebraic effect checks passed");
        })
        .expect("spawn")
        .join()
        .expect("checks passed");
}
