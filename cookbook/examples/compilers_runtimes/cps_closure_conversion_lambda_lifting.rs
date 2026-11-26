//! RECIPE 814 (compilers): ANF, CPS (Plotkin and one-pass), defunctionalisation, closure conversion, lambda lifting and trampolines.
//! LEVEL: advanced
//! Use in: the middle end of any compiler for a language with first-class functions, and any place you must turn recursion or callbacks into data.   Run: cargo run --release --example cps_closure_conversion_lambda_lifting
//! COMMON USES: CPS and ANF as compiler intermediate forms (SML/NJ is built around CPS; GHC's Core-to-STG pipeline works on an ANF-like form),
//! closure conversion in every native compiler with lambdas (rustc turns a closure into an anonymous struct holding its captures),
//! lambda lifting in lazy-language compilers, defunctionalisation to make continuations serialisable (workflow engines, explicit-stack interpreters).
//! Course: Reynolds, "Definitional interpreters for higher-order programming languages"; Danvy and Filinski, "Representing control".
#![allow(dead_code)]
//!
//! THE PIPELINE. One small strict language (ints, n-ary lambdas, `let`, `letrec`, `if`: non-zero is true) and one transformation per
//! section, each with its own machine, so that the meaning can be re-checked after every step.
//!  0. UNIQUIFY: give every binder a different name. Later passes MOVE code under other binders; without this step they capture variables
//!     (demonstrated below: the one-pass CPS transform returns a wrong number on a shadowing program).
//!  1. A-NORMAL FORM: every argument of a call, operator and `if` is atomic; every intermediate result has a name. Evaluation order is
//!     now written in the program.
//!  2. CPS: no function returns. Each takes an extra argument `k`, "what to do with the result", and every call is a TAIL CALL. So the machine
//!     for CPS is a loop without a stack: the stack has become the chain of continuation closures.
//!     PLOTKIN's transform is three lines per case but litters the output with "administrative" redexes `(fn (k) ...) k2`.
//!     DANVY-FILINSKI's one-pass transform represents the continuation at COMPILE time as a Rust closure, which is applied during the
//!     translation instead of being emitted. Result: exactly the redexes of the source program, none added (asserted).
//!     (A source redex may be hidden behind a `let` in function position; see `source_redexes`.)
//!  3. CLOSURE CONVERSION: every lambda becomes closed top-level code plus a FLAT closure record of the free variables it captures.
//!  4. LAMBDA LIFTING (Johnsson): the other way to remove free variables: add them as extra PARAMETERS. A `letrec` function that is only
//!     called needs no allocation at all (`Direct` call); used as a value it becomes a partial application. If f calls g, f must also receive
//!     g's extra parameters (transitively), which closure conversion avoids by passing g's record.
//!  5. DEFUNCTIONALISATION (Reynolds): replace "a function value" by a constructor of a data type, one per lambda, and "call it" by one
//!     `apply` that dispatches on the constructor. Applied AFTER CPS, the continuation constructors are exactly the stack frames of an
//!     abstract machine. Because they are plain data, a running computation can be written to text, read back and resumed (done below).
//!  6. TAIL CALLS: a call in tail position needs no stack frame. The TRAMPOLINE returns such a call as a value to a driver loop.
//! LIMITS. Machines use `Rc` environments; freeing a very long continuation chain recurses inside `Drop`, so `main` runs on a large stack.
//! The "depth" numbers count the evaluator's own recursion, which is what a native stack would hold.
//!
//! Read: Reynolds (1972, title only); Plotkin, "Call-by-name, call-by-value and the lambda-calculus" (1975, title only); Danvy and Filinski (1992,
//! title only); Flanagan et al., "The essence of compiling with continuations" (1993, title only); Johnsson, "Lambda lifting" (1985, title only);
//! Appel, "Compiling with Continuations" (book, title only); https://en.wikipedia.org/wiki/Continuation-passing_style ;
//! https://en.wikipedia.org/wiki/A-normal_form ; https://en.wikipedia.org/wiki/Defunctionalization ; https://en.wikipedia.org/wiki/Lambda_lifting
//! TRY NEXT:
//!   - [S] In `all_stages`, give `lift_program` the source instead of the uniquified program and find a random program on which it fails.
//!   - [S] Change `cps_tail` so a tail call wraps the continuation as `(fn (x) (k x))`; watch the continuation chain grow in the loop demo.
//!   - [M] Add an eta-reduction pass over CPS output and measure the size change.
//!   - [M] Make closure conversion share one environment record between sibling lambdas ("linked closures") and discuss what it keeps alive.
//!   - [A] Project: emit C from the closure-converted form (one C function per code block) and compile it.
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

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

// ---------------------------------------------------------------- the language

type Name = Rc<str>;
#[derive(Clone, Copy, PartialEq, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    Lt,
}
impl Op {
    fn apply(self, a: i64, b: i64) -> i64 {
        match self {
            Op::Add => a.wrapping_add(b),
            Op::Sub => a.wrapping_sub(b),
            Op::Mul => a.wrapping_mul(b),
            Op::Lt => i64::from(a < b),
        }
    }
    fn text(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Lt => "<",
        }
    }
}
#[derive(Clone, PartialEq, Debug)]
enum E {
    Int(i64),
    Var(Name),
    Prim(Op, Box<E>, Box<E>),
    If(Box<E>, Box<E>, Box<E>),
    Lam(Vec<Name>, Rc<E>),
    App(Box<E>, Vec<E>),
    Let(Name, Box<E>, Box<E>),
    /// (name, params, function body, rest)
    LetRec(Name, Vec<Name>, Rc<E>, Box<E>),
    /// only produced by CPS: stop with this value
    Halt(Box<E>),
}
fn name(s: &str) -> Name {
    s.into()
}
fn var(n: &Name) -> E {
    E::Var(n.clone())
}
fn app(f: E, args: Vec<E>) -> E {
    E::App(Box::new(f), args)
}
fn lam(ps: Vec<Name>, body: E) -> E {
    E::Lam(ps, Rc::new(body))
}
fn let_(x: Name, d: E, body: E) -> E {
    E::Let(x, Box::new(d), Box::new(body))
}
fn prim(op: Op, a: E, b: E) -> E {
    E::Prim(op, Box::new(a), Box::new(b))
}
fn if_(c: E, t: E, e: E) -> E {
    E::If(Box::new(c), Box::new(t), Box::new(e))
}
fn kids(e: &E) -> Vec<&E> {
    match e {
        E::Int(_) | E::Var(_) => vec![],
        E::Prim(_, a, b) => vec![a, b],
        E::If(a, b, c) => vec![a, b, c],
        E::Lam(_, b) => vec![b],
        E::App(f, a) => std::iter::once(&**f).chain(a).collect(),
        E::Let(_, d, b) => vec![d, b],
        E::LetRec(_, _, fb, rest) => vec![fb, rest],
        E::Halt(a) => vec![a],
    }
}
fn size(e: &E) -> usize {
    1 + kids(e).iter().map(|k| size(k)).sum::<usize>()
}
fn show(e: &E) -> String {
    let list = |v: &[Name]| v.join(" ");
    match e {
        E::Int(k) => k.to_string(),
        E::Var(x) => x.to_string(),
        E::Prim(o, a, b) => format!("({} {} {})", o.text(), show(a), show(b)),
        E::If(c, t, f) => format!("(if {} {} {})", show(c), show(t), show(f)),
        E::Lam(ps, b) => format!("(fn ({}) {})", list(ps), show(b)),
        E::App(f, a) => format!(
            "({})",
            std::iter::once(show(f))
                .chain(a.iter().map(show))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        E::Let(x, d, b) => format!("(let {x} {} {})", show(d), show(b)),
        E::LetRec(f, ps, fb, rest) => {
            format!("(letrec {f} ({}) {} {})", list(ps), show(fb), show(rest))
        }
        E::Halt(a) => format!("(halt {})", show(a)),
    }
}

// ---------------------------------------------------------------- parser (s-expressions)

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
fn from_sx(s: &Sx) -> E {
    let atom = |s: &Sx| match s {
        Sx::Atom(a) => name(a),
        Sx::List(_) => panic!("name expected"),
    };
    let names = |s: &Sx| match s {
        Sx::List(v) => v.iter().map(atom).collect::<Vec<_>>(),
        Sx::Atom(_) => panic!("parameter list expected"),
    };
    match s {
        Sx::Atom(a) => a.parse().map(E::Int).unwrap_or_else(|_| E::Var(name(a))),
        Sx::List(v) => {
            let head = match &v[0] {
                Sx::Atom(a) => a.as_str(),
                Sx::List(_) => "",
            };
            let at = |i: usize| from_sx(&v[i]);
            match head {
                "+" => prim(Op::Add, at(1), at(2)),
                "-" => prim(Op::Sub, at(1), at(2)),
                "*" => prim(Op::Mul, at(1), at(2)),
                "<" => prim(Op::Lt, at(1), at(2)),
                "if" => if_(at(1), at(2), at(3)),
                "fn" => lam(names(&v[1]), at(2)),
                "let" => let_(atom(&v[1]), at(2), at(3)),
                "letrec" => E::LetRec(atom(&v[1]), names(&v[2]), Rc::new(at(3)), Box::new(at(4))),
                "halt" => E::Halt(Box::new(at(1))),
                _ => app(at(0), v[1..].iter().map(from_sx).collect()),
            }
        }
    }
}
fn parse(src: &str) -> E {
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

// ---------------------------------------------------------------- free variables and uniquify

fn free_vars(e: &E) -> BTreeSet<Name> {
    let mut out = BTreeSet::new();
    match e {
        E::Var(x) => {
            out.insert(x.clone());
        }
        E::Lam(ps, b) => {
            out = free_vars(b);
            ps.iter().for_each(|p| {
                out.remove(p);
            });
        }
        E::Let(x, d, b) => {
            out = free_vars(b);
            out.remove(x);
            out.extend(free_vars(d));
        }
        E::LetRec(f, ps, fb, rest) => {
            out = free_vars(fb);
            ps.iter().for_each(|p| {
                out.remove(p);
            });
            out.extend(free_vars(rest));
            out.remove(f);
        }
        other => kids(other).iter().for_each(|k| out.extend(free_vars(k))),
    }
    out
}
#[derive(Default)]
struct Fresh(Cell<usize>);
impl Fresh {
    /// '.' and '%' never occur in source names, so generated names cannot collide with them.
    fn name(&self, base: &str) -> Name {
        self.0.set(self.0.get() + 1);
        format!("{base}.{}", self.0.get()).into()
    }
}
/// Rename every binder to a new name; free variables keep theirs.
fn uniquify(e: &E, env: &mut Vec<(Name, Name)>, fresh: &Fresh) -> E {
    let base = |n: &Name| n.split('.').next().unwrap_or("v").to_string();
    let go = |e: &E, env: &mut Vec<(Name, Name)>| uniquify(e, env, fresh);
    let bind = |ns: &[Name], env: &mut Vec<(Name, Name)>| -> Vec<Name> {
        ns.iter()
            .map(|n| {
                let new = fresh.name(&base(n));
                env.push((n.clone(), new.clone()));
                new
            })
            .collect()
    };
    match e {
        E::Int(k) => E::Int(*k),
        E::Var(x) => E::Var(
            env.iter()
                .rev()
                .find(|(old, _)| old == x)
                .map_or_else(|| x.clone(), |p| p.1.clone()),
        ),
        E::Prim(o, a, b) => prim(*o, go(a, env), go(b, env)),
        E::If(c, t, f) => if_(go(c, env), go(t, env), go(f, env)),
        E::App(f, a) => app(go(f, env), a.iter().map(|x| go(x, env)).collect()),
        E::Halt(a) => E::Halt(Box::new(go(a, env))),
        E::Lam(ps, b) => {
            let mark = env.len();
            let ps2 = bind(ps, env);
            let b2 = go(b, env);
            env.truncate(mark);
            lam(ps2, b2)
        }
        E::Let(x, d, b) => {
            let d2 = go(d, env);
            let mark = env.len();
            let x2 = bind(std::slice::from_ref(x), env).remove(0);
            let b2 = go(b, env);
            env.truncate(mark);
            let_(x2, d2, b2)
        }
        E::LetRec(f, ps, fb, rest) => {
            let mark = env.len();
            let f2 = bind(std::slice::from_ref(f), env).remove(0);
            let inner = env.len();
            let ps2 = bind(ps, env);
            let fb2 = go(fb, env);
            env.truncate(inner);
            let rest2 = go(rest, env);
            env.truncate(mark);
            E::LetRec(f2, ps2, Rc::new(fb2), Box::new(rest2))
        }
    }
}
fn unique(e: &E, fresh: &Fresh) -> E {
    uniquify(e, &mut vec![], fresh)
}
fn all_names(e: &E, used: &mut BTreeSet<Name>, bound: &mut Vec<Name>) {
    match e {
        E::Var(x) => {
            used.insert(x.clone());
        }
        E::Lam(ps, _) => bound.extend(ps.iter().cloned()),
        E::Let(x, ..) => bound.push(x.clone()),
        E::LetRec(f, ps, ..) => {
            bound.push(f.clone());
            bound.extend(ps.iter().cloned());
        }
        _ => {}
    }
    kids(e).iter().for_each(|k| all_names(k, used, bound));
}
/// Oracle for `free_vars` by a different route: once all binders are distinct (and distinct from free names), free = used minus bound.
fn free_vars_oracle(e: &E) -> BTreeSet<Name> {
    let u = unique(e, &Fresh::default());
    let (mut used, mut bound) = (BTreeSet::new(), vec![]);
    all_names(&u, &mut used, &mut bound);
    assert_eq!(
        bound.len(),
        bound.iter().collect::<BTreeSet<_>>().len(),
        "uniquify made all binders distinct"
    );
    used.into_iter().filter(|x| !bound.contains(x)).collect()
}

// ---------------------------------------------------------------- 0. the reference interpreter and the trampoline

#[derive(Clone)]
enum Val {
    Int(i64),
    Clos(Rc<Closure>),
}
struct Closure {
    /// set for `letrec`: the closure binds its own name when entered
    rec: Option<Name>,
    params: Vec<Name>,
    body: Rc<E>,
    env: Env,
}
/// A persistent linked list: closures share tails instead of copying environments.
type Env = Option<Rc<Binding>>;
struct Binding {
    name: Name,
    value: Val,
    next: Env,
}
fn extend(env: &Env, n: Name, v: Val) -> Env {
    Some(Rc::new(Binding {
        name: n,
        value: v,
        next: env.clone(),
    }))
}
fn find(env: &Env, n: &str) -> Val {
    let mut cur = env;
    while let Some(node) = cur {
        if &*node.name == n {
            return node.value.clone();
        }
        cur = &node.next;
    }
    panic!("unbound variable {n}")
}
fn int(v: &Val) -> i64 {
    match v {
        Val::Int(k) => *k,
        Val::Clos(_) => panic!("integer expected"),
    }
}
fn enter(f: &Val, args: Vec<Val>) -> (Rc<E>, Env) {
    let Val::Clos(c) = f else {
        panic!("calling a number")
    };
    assert_eq!(c.params.len(), args.len(), "arity mismatch");
    let mut env = c.env.clone();
    if let Some(me) = &c.rec {
        env = extend(&env, me.clone(), f.clone());
    }
    for (p, a) in c.params.iter().zip(args) {
        env = extend(&env, p.clone(), a);
    }
    (c.body.clone(), env)
}
#[derive(Default)]
struct Meter {
    depth: usize,
    max_depth: usize,
    calls: usize,
}
impl Meter {
    fn down<T>(&mut self, f: impl FnOnce(&mut Meter) -> T) -> T {
        self.depth += 1;
        self.max_depth = self.max_depth.max(self.depth);
        let r = f(self);
        self.depth -= 1;
        r
    }
}
fn closure(rec: Option<&Name>, params: &[Name], body: &Rc<E>, env: &Env) -> Val {
    Val::Clos(Rc::new(Closure {
        rec: rec.cloned(),
        params: params.to_vec(),
        body: body.clone(),
        env: env.clone(),
    }))
}
/// The textbook big-step interpreter: the definition of the language. Every call uses the host stack.
fn eval(e: &E, env: &Env, m: &mut Meter) -> Val {
    m.down(|m| match e {
        E::Int(k) => Val::Int(*k),
        E::Var(x) => find(env, x),
        E::Prim(o, a, b) => {
            let (x, y) = (int(&eval(a, env, m)), int(&eval(b, env, m)));
            Val::Int(o.apply(x, y))
        }
        E::If(c, t, f) => {
            let b = int(&eval(c, env, m)) != 0;
            eval(if b { t } else { f }, env, m)
        }
        E::Lam(ps, b) => closure(None, ps, b, env),
        E::App(f, a) => {
            let fv = eval(f, env, m);
            let args = a.iter().map(|x| eval(x, env, m)).collect();
            let (body, env2) = enter(&fv, args);
            m.calls += 1;
            eval(&body, &env2, m)
        }
        E::Let(x, d, b) => {
            let v = eval(d, env, m);
            eval(b, &extend(env, x.clone(), v), m)
        }
        E::LetRec(f, ps, fb, rest) => eval(
            rest,
            &extend(env, f.clone(), closure(Some(f), ps, fb, env)),
            m,
        ),
        E::Halt(a) => eval(a, env, m),
    })
}
/// What a non-recursive step of the trampolined interpreter produces.
enum Bounce {
    Done(Val),
    TailCall(Val, Vec<Val>),
}
/// Evaluate in TAIL position: a call is not performed, it is handed back to the driver.
fn eval_tail(e: &E, env: &Env, m: &mut Meter) -> Bounce {
    match e {
        E::App(f, a) => {
            let fv = eval_tr(f, env, m);
            Bounce::TailCall(fv, a.iter().map(|x| eval_tr(x, env, m)).collect())
        }
        E::If(c, t, f) => {
            let b = int(&eval_tr(c, env, m)) != 0;
            eval_tail(if b { t } else { f }, env, m)
        }
        E::Let(x, d, b) => {
            let v = eval_tr(d, env, m);
            eval_tail(b, &extend(env, x.clone(), v), m)
        }
        E::LetRec(f, ps, fb, rest) => eval_tail(
            rest,
            &extend(env, f.clone(), closure(Some(f), ps, fb, env)),
            m,
        ),
        other => Bounce::Done(eval_tr(other, env, m)),
    }
}
/// Evaluate to a value: the trampoline loop. Only NON-tail subexpressions recurse.
fn eval_tr(e: &E, env: &Env, m: &mut Meter) -> Val {
    m.down(|m| match e {
        E::Int(k) => Val::Int(*k),
        E::Var(x) => find(env, x),
        E::Prim(o, a, b) => {
            let (x, y) = (int(&eval_tr(a, env, m)), int(&eval_tr(b, env, m)));
            Val::Int(o.apply(x, y))
        }
        E::Lam(ps, b) => closure(None, ps, b, env),
        E::Halt(a) => eval_tr(a, env, m),
        other => {
            let mut step = eval_tail(other, env, m);
            loop {
                match step {
                    Bounce::Done(v) => return v,
                    Bounce::TailCall(f, args) => {
                        m.calls += 1;
                        let (body, env2) = enter(&f, args);
                        step = eval_tail(&body, &env2, m);
                    }
                }
            }
        }
    })
}
/// (tail calls, non-tail calls), a purely syntactic property.
fn count_calls(e: &E, tail: bool, acc: &mut (usize, usize)) {
    match e {
        E::App(f, a) => {
            if tail {
                acc.0 += 1
            } else {
                acc.1 += 1
            }
            count_calls(f, false, acc);
            a.iter().for_each(|x| count_calls(x, false, acc));
        }
        E::If(c, t, f) => {
            count_calls(c, false, acc);
            count_calls(t, tail, acc);
            count_calls(f, tail, acc);
        }
        E::Let(_, d, b) => {
            count_calls(d, false, acc);
            count_calls(b, tail, acc);
        }
        E::LetRec(_, _, fb, rest) => {
            count_calls(fb, true, acc);
            count_calls(rest, tail, acc);
        }
        E::Lam(_, b) => count_calls(b, true, acc),
        other => kids(other).iter().for_each(|k| count_calls(k, false, acc)),
    }
}

// ---------------------------------------------------------------- 1. A-normal form

type Cont<'a> = Box<dyn FnOnce(E) -> E + 'a>;
fn is_atom(e: &E) -> bool {
    matches!(e, E::Int(_) | E::Var(_) | E::Lam(..))
}
fn anf(e: &E, fresh: &Fresh) -> E {
    normalise(e, fresh, Box::new(|x| x))
}
/// Normalise `e` and pass its result (atomic, or one operation on atoms) to `k`.
fn normalise<'a>(e: &'a E, fresh: &'a Fresh, k: Cont<'a>) -> E {
    match e {
        E::Int(_) | E::Var(_) => k(e.clone()),
        E::Lam(ps, b) => k(lam(ps.clone(), anf(b, fresh))),
        E::Prim(o, a, b) => atomise(
            a,
            fresh,
            Box::new(move |a| atomise(b, fresh, Box::new(move |b| k(prim(*o, a, b))))),
        ),
        E::If(c, t, f) => atomise(
            c,
            fresh,
            Box::new(move |c| k(if_(c, anf(t, fresh), anf(f, fresh)))),
        ),
        E::App(f, args) => atomise(
            f,
            fresh,
            Box::new(move |f| atomise_all(args, vec![], fresh, Box::new(move |a| k(app(f, a))))),
        ),
        E::Let(x, d, b) => normalise(
            d,
            fresh,
            Box::new(move |d| let_(x.clone(), d, normalise(b, fresh, k))),
        ),
        E::LetRec(f, ps, fb, rest) => E::LetRec(
            f.clone(),
            ps.clone(),
            Rc::new(anf(fb, fresh)),
            Box::new(normalise(rest, fresh, k)),
        ),
        E::Halt(a) => atomise(a, fresh, Box::new(move |a| k(E::Halt(Box::new(a))))),
    }
}
/// Like `normalise`, but name the result if it is not atomic.
fn atomise<'a>(e: &'a E, fresh: &'a Fresh, k: Cont<'a>) -> E {
    normalise(
        e,
        fresh,
        Box::new(move |r| {
            if is_atom(&r) {
                k(r)
            } else {
                let t = fresh.name("t");
                let_(t.clone(), r, k(var(&t)))
            }
        }),
    )
}
fn atomise_all<'a>(
    rest: &'a [E],
    mut done: Vec<E>,
    fresh: &'a Fresh,
    k: Box<dyn FnOnce(Vec<E>) -> E + 'a>,
) -> E {
    match rest.split_first() {
        None => k(done),
        Some((first, rest)) => atomise(
            first,
            fresh,
            Box::new(move |a| {
                done.push(a);
                atomise_all(rest, done, fresh, k)
            }),
        ),
    }
}
fn atom_ok(e: &E) -> bool {
    match e {
        E::Lam(_, b) => is_anf(b),
        other => is_atom(other),
    }
}
/// An atom, or one operation whose operands are atoms.
fn op_ok(e: &E) -> bool {
    match e {
        E::Prim(_, a, b) => atom_ok(a) && atom_ok(b),
        E::App(f, a) => atom_ok(f) && a.iter().all(atom_ok),
        E::If(c, t, f) => atom_ok(c) && is_anf(t) && is_anf(f),
        other => atom_ok(other),
    }
}
fn is_anf(e: &E) -> bool {
    match e {
        E::Let(_, d, b) => op_ok(d) && is_anf(b),
        E::LetRec(_, _, fb, rest) => is_anf(fb) && is_anf(rest),
        other => op_ok(other),
    }
}

// ---------------------------------------------------------------- 2. continuation-passing style

/// Plotkin: [[e]] is a function that takes the continuation.
fn plotkin(e: &E, fresh: &Fresh) -> E {
    let k = fresh.name("k");
    let kv = var(&k);
    // `[[e]] (fn (v) body(v))`
    let then = |e: &E, body: &dyn Fn(E) -> E| {
        let v = fresh.name("v");
        app(plotkin(e, fresh), vec![lam(vec![v.clone()], body(var(&v)))])
    };
    let body = match e {
        E::Int(_) | E::Var(_) => app(kv, vec![e.clone()]),
        E::Lam(ps, b) => {
            let k2 = fresh.name("k");
            let ps2 = [ps.clone(), vec![k2.clone()]].concat();
            app(kv, vec![lam(ps2, app(plotkin(b, fresh), vec![var(&k2)]))])
        }
        E::Prim(o, a, b) => then(a, &|x| {
            then(b, &|y| app(kv.clone(), vec![prim(*o, x.clone(), y)]))
        }),
        E::If(c, t, f) => then(c, &|v| {
            if_(
                v,
                app(plotkin(t, fresh), vec![kv.clone()]),
                app(plotkin(f, fresh), vec![kv.clone()]),
            )
        }),
        E::App(f, args) => {
            fn go(rest: &[E], done: &[E], f: &E, k: &E, fresh: &Fresh) -> E {
                match rest.split_first() {
                    None => app(f.clone(), [done.to_vec(), vec![k.clone()]].concat()),
                    Some((a, rest)) => {
                        let v = fresh.name("a");
                        app(
                            plotkin(a, fresh),
                            vec![lam(
                                vec![v.clone()],
                                go(rest, &[done.to_vec(), vec![var(&v)]].concat(), f, k, fresh),
                            )],
                        )
                    }
                }
            }
            then(f, &|fv| go(args, &[], &fv, &kv, fresh))
        }
        E::Let(x, d, b) => then(d, &|v| {
            let_(x.clone(), v, app(plotkin(b, fresh), vec![kv.clone()]))
        }),
        E::LetRec(f, ps, fb, rest) => {
            let k2 = fresh.name("k");
            let ps2 = [ps.clone(), vec![k2.clone()]].concat();
            E::LetRec(
                f.clone(),
                ps2,
                Rc::new(app(plotkin(fb, fresh), vec![var(&k2)])),
                Box::new(app(plotkin(rest, fresh), vec![kv])),
            )
        }
        E::Halt(_) => unreachable!("halt is not a source construct"),
    };
    lam(vec![k], body)
}
fn plotkin_program(e: &E, fresh: &Fresh) -> E {
    let r = fresh.name("r");
    app(
        plotkin(e, fresh),
        vec![lam(vec![r.clone()], E::Halt(Box::new(var(&r))))],
    )
}
/// Danvy-Filinski, "value context": `kappa` is a compile-time continuation that builds the code which consumes the (trivial) value.
fn cps<'a>(e: &'a E, fresh: &'a Fresh, kappa: Cont<'a>) -> E {
    match e {
        E::Int(_) | E::Var(_) => kappa(e.clone()),
        E::Lam(ps, b) => kappa(cps_lambda(ps, b, fresh)),
        E::Prim(o, a, b) => cps(
            a,
            fresh,
            Box::new(move |a| cps(b, fresh, Box::new(move |b| kappa(prim(*o, a, b))))),
        ),
        E::App(f, args) => cps(
            f,
            fresh,
            Box::new(move |f| {
                cps_all(
                    args,
                    vec![],
                    fresh,
                    Box::new(move |mut a| {
                        // the call needs a real continuation: turn the compile-time one into a lambda
                        let x = fresh.name("x");
                        a.push(lam(vec![x.clone()], kappa(var(&x))));
                        app(f, a)
                    }),
                )
            }),
        ),
        E::If(c, t, f) => cps(
            c,
            fresh,
            Box::new(move |c| {
                // both branches need the continuation: bind it once (a join point) instead of copying its code
                let (j, x) = (fresh.name("j"), fresh.name("x"));
                let join = lam(vec![x.clone()], kappa(var(&x)));
                let_(
                    j.clone(),
                    join,
                    if_(c, cps_tail(t, j.clone(), fresh), cps_tail(f, j, fresh)),
                )
            }),
        ),
        E::Let(x, d, b) => cps(
            d,
            fresh,
            Box::new(move |d| let_(x.clone(), d, cps(b, fresh, kappa))),
        ),
        E::LetRec(f, ps, fb, rest) => cps_letrec(f, ps, fb, cps(rest, fresh, kappa), fresh),
        E::Halt(_) => unreachable!("halt is not a source construct"),
    }
}
/// "Tail context": the continuation is already a run-time variable `k`, so calls pass it on unchanged (no `(fn (x) (k x))`).
fn cps_tail(e: &E, k: Name, fresh: &Fresh) -> E {
    match e {
        E::App(f, args) => cps(
            f,
            fresh,
            Box::new(move |f| {
                cps_all(
                    args,
                    vec![],
                    fresh,
                    Box::new(move |mut a| {
                        a.push(E::Var(k));
                        app(f, a)
                    }),
                )
            }),
        ),
        E::If(c, t, f) => cps(
            c,
            fresh,
            Box::new(move |c| if_(c, cps_tail(t, k.clone(), fresh), cps_tail(f, k, fresh))),
        ),
        E::Let(x, d, b) => cps(
            d,
            fresh,
            Box::new(move |d| let_(x.clone(), d, cps_tail(b, k, fresh))),
        ),
        E::LetRec(f, ps, fb, rest) => cps_letrec(f, ps, fb, cps_tail(rest, k, fresh), fresh),
        other => cps(other, fresh, Box::new(move |v| app(E::Var(k), vec![v]))),
    }
}
fn cps_lambda(ps: &[Name], b: &E, fresh: &Fresh) -> E {
    let k = fresh.name("k");
    lam(
        [ps.to_vec(), vec![k.clone()]].concat(),
        cps_tail(b, k, fresh),
    )
}
fn cps_letrec(f: &Name, ps: &[Name], fb: &E, rest: E, fresh: &Fresh) -> E {
    let E::Lam(ps2, body) = cps_lambda(ps, fb, fresh) else {
        unreachable!()
    };
    E::LetRec(f.clone(), ps2, body, Box::new(rest))
}
fn cps_all<'a>(
    rest: &'a [E],
    mut done: Vec<E>,
    fresh: &'a Fresh,
    k: Box<dyn FnOnce(Vec<E>) -> E + 'a>,
) -> E {
    match rest.split_first() {
        None => k(done),
        Some((first, rest)) => cps(
            first,
            fresh,
            Box::new(move |a| {
                done.push(a);
                cps_all(rest, done, fresh, k)
            }),
        ),
    }
}
fn cps_program(e: &E, fresh: &Fresh) -> E {
    cps(e, fresh, Box::new(|v| E::Halt(Box::new(v))))
}
/// Trivial: evaluating it cannot call anything.
fn trivial(e: &E) -> bool {
    match e {
        E::Int(_) | E::Var(_) => true,
        E::Prim(_, a, b) => trivial(a) && trivial(b),
        E::Lam(_, b) => is_cps(b),
        _ => false,
    }
}
/// The tail-form predicate: all arguments are trivial, and nothing happens after a call.
fn is_cps(e: &E) -> bool {
    match e {
        E::App(f, a) => trivial(f) && a.iter().all(trivial),
        E::If(c, t, f) => trivial(c) && is_cps(t) && is_cps(f),
        E::Let(_, d, b) => trivial(d) && is_cps(b),
        E::LetRec(_, _, fb, rest) => is_cps(fb) && is_cps(rest),
        E::Halt(a) => trivial(a),
        _ => false,
    }
}
fn redexes(e: &E) -> usize {
    usize::from(matches!(e, E::App(f, _) if matches!(**f, E::Lam(..))))
        + kids(e).iter().map(|k| redexes(k)).sum::<usize>()
}
/// Redexes of the SOURCE program, including those hidden behind a binding: in `((let x 1 (fn (y) y)) 2)` the function is a lambda once the
/// transform has hoisted the `let` outwards.
fn source_redexes(e: &E) -> usize {
    fn yields_lambda(f: &E) -> bool {
        match f {
            E::Lam(..) => true,
            E::Let(_, _, body) => yields_lambda(body),
            E::LetRec(_, _, _, rest) => yields_lambda(rest),
            _ => false,
        }
    }
    usize::from(matches!(e, E::App(f, _) if yields_lambda(f)))
        + kids(e).iter().map(|k| source_redexes(k)).sum::<usize>()
}
/// The CPS machine: a LOOP. It never recurses to make a call because there is never anything to come back to.
fn run_cps(program: &E) -> (i64, usize) {
    fn value(e: &E, env: &Env) -> Val {
        match e {
            E::Int(k) => Val::Int(*k),
            E::Var(x) => find(env, x),
            E::Prim(o, a, b) => Val::Int(o.apply(int(&value(a, env)), int(&value(b, env)))),
            E::Lam(ps, b) => closure(None, ps, b, env),
            other => panic!("not trivial: {}", show(other)),
        }
    }
    let mut control = Rc::new(program.clone());
    let mut env: Env = None;
    let mut calls = 0;
    loop {
        let term = control.clone();
        let mut cur: &E = &term;
        // walk down the current body until the next transfer of control
        loop {
            match cur {
                E::Let(x, d, b) => {
                    env = extend(&env, x.clone(), value(d, &env));
                    cur = b;
                }
                E::LetRec(f, ps, fb, rest) => {
                    env = extend(&env, f.clone(), closure(Some(f), ps, fb, &env));
                    cur = rest;
                }
                E::If(c, t, f) => cur = if int(&value(c, &env)) != 0 { t } else { f },
                E::Halt(a) => return (int(&value(a, &env)), calls),
                E::App(f, a) => {
                    let fv = value(f, &env);
                    let args = a.iter().map(|x| value(x, &env)).collect();
                    (control, env) = enter(&fv, args);
                    calls += 1;
                    break;
                }
                other => panic!("not in tail form: {}", show(other)),
            }
        }
    }
}

// ---------------------------------------------------------------- 3. closure conversion

/// Closure-converted terms. There is no lambda and no free variable: only locals, slots of the own closure record, and code labels.
#[derive(Debug, Clone)]
enum C {
    Int(i64),
    Local(Name),
    /// slot of the current closure record
    Env(usize),
    /// the current closure itself (recursion)
    Itself,
    Prim(Op, Box<C>, Box<C>),
    If(Box<C>, Box<C>, Box<C>),
    MakeClosure(usize, Vec<C>),
    Call(Box<C>, Vec<C>),
    Let(Name, Box<C>, Box<C>),
    Halt(Box<C>),
}
struct Code {
    params: Vec<Name>,
    captured: Vec<Name>,
    body: Rc<C>,
}
struct Closed {
    codes: Vec<Code>,
    main: C,
}
struct Scope {
    locals: Vec<Name>,
    slots: Vec<Name>,
    me: Option<Name>,
}
fn access(x: &Name, s: &Scope) -> C {
    if s.locals.contains(x) {
        C::Local(x.clone())
    } else if s.me.as_ref() == Some(x) {
        C::Itself
    } else {
        C::Env(
            s.slots
                .iter()
                .position(|y| y == x)
                .unwrap_or_else(|| panic!("{x} was not captured")),
        )
    }
}
/// Requires unique binders.
fn close(e: &E, s: &mut Scope, codes: &mut Vec<Code>) -> C {
    match e {
        E::Int(k) => C::Int(*k),
        E::Var(x) => access(x, s),
        E::Prim(o, a, b) => C::Prim(
            *o,
            Box::new(close(a, s, codes)),
            Box::new(close(b, s, codes)),
        ),
        E::If(c, t, f) => C::If(
            Box::new(close(c, s, codes)),
            Box::new(close(t, s, codes)),
            Box::new(close(f, s, codes)),
        ),
        E::App(f, a) => C::Call(
            Box::new(close(f, s, codes)),
            a.iter().map(|x| close(x, s, codes)).collect(),
        ),
        E::Halt(a) => C::Halt(Box::new(close(a, s, codes))),
        E::Let(x, d, b) => {
            let d2 = close(d, s, codes);
            s.locals.push(x.clone());
            C::Let(x.clone(), Box::new(d2), Box::new(close(b, s, codes)))
        }
        E::Lam(ps, b) => make_closure(None, ps, b, free_vars(e), s, codes),
        E::LetRec(f, ps, fb, rest) => {
            let mut fv = free_vars(&E::Lam(ps.clone(), fb.clone()));
            fv.remove(f);
            let made = make_closure(Some(f), ps, fb, fv, s, codes);
            s.locals.push(f.clone());
            C::Let(f.clone(), Box::new(made), Box::new(close(rest, s, codes)))
        }
    }
}
fn make_closure(
    me: Option<&Name>,
    ps: &[Name],
    body: &E,
    fv: BTreeSet<Name>,
    outer: &mut Scope,
    codes: &mut Vec<Code>,
) -> C {
    let slots: Vec<Name> = fv.into_iter().collect();
    let record = slots.iter().map(|x| access(x, outer)).collect();
    let mut inner = Scope {
        locals: ps.to_vec(),
        slots: slots.clone(),
        me: me.cloned(),
    };
    let body = close(body, &mut inner, codes);
    codes.push(Code {
        params: ps.to_vec(),
        captured: slots,
        body: Rc::new(body),
    });
    C::MakeClosure(codes.len() - 1, record)
}
fn close_program(e: &E) -> Closed {
    let mut codes = vec![];
    let main = close(
        e,
        &mut Scope {
            locals: vec![],
            slots: vec![],
            me: None,
        },
        &mut codes,
    );
    Closed { codes, main }
}
fn c_kids(c: &C) -> Vec<&C> {
    match c {
        C::Int(_) | C::Local(_) | C::Env(_) | C::Itself => vec![],
        C::Prim(_, a, b) => vec![a, b],
        C::If(a, b, d) => vec![a, b, d],
        C::MakeClosure(_, a) => a.iter().collect(),
        C::Call(f, a) => std::iter::once(&**f).chain(a).collect(),
        C::Let(_, d, b) => vec![d, b],
        C::Halt(a) => vec![a],
    }
}
fn c_size(c: &C) -> usize {
    1 + c_kids(c).iter().map(|k| c_size(k)).sum::<usize>()
}
/// Every code block mentions only its parameters, its own lets, and slots its record really has.
fn is_closed_code(c: &C, locals: &mut Vec<Name>, slots: usize, codes: &[Code]) -> bool {
    match c {
        C::Local(x) => locals.contains(x),
        C::Env(i) => *i < slots,
        C::Let(x, d, b) => {
            let ok = is_closed_code(d, locals, slots, codes);
            locals.push(x.clone());
            let ok = ok && is_closed_code(b, locals, slots, codes);
            locals.pop();
            ok
        }
        C::MakeClosure(l, a) => {
            codes[*l].captured.len() == a.len()
                && a.iter().all(|x| is_closed_code(x, locals, slots, codes))
        }
        other => c_kids(other)
            .iter()
            .all(|k| is_closed_code(k, locals, slots, codes)),
    }
}
#[derive(Clone)]
enum CVal {
    Int(i64),
    Rec(usize, Rc<[CVal]>),
}
fn c_int(v: CVal) -> i64 {
    match v {
        CVal::Int(k) => k,
        CVal::Rec(..) => panic!("integer expected"),
    }
}
struct Frame {
    locals: Vec<(Name, CVal)>,
    me: Option<CVal>,
}
impl Closed {
    /// A tail call replaces the current frame instead of recursing, so a CPS input runs without growing the stack here too.
    fn eval(&self, entry: &C, mut fr: Frame) -> CVal {
        let mut code: Option<Rc<C>> = None; // the code block being run, once a call has left `entry`
        loop {
            let current = code.clone();
            let mut term: &C = current.as_deref().unwrap_or(entry);
            loop {
                match term {
                    C::Let(x, d, b) => {
                        let v = self.value(d, &fr);
                        fr.locals.push((x.clone(), v));
                        term = b;
                    }
                    C::If(c, t, f) => term = if c_int(self.value(c, &fr)) != 0 { t } else { f },
                    C::Halt(a) => return self.value(a, &fr),
                    C::Call(f, a) => {
                        let fv = self.value(f, &fr);
                        let args: Vec<CVal> = a.iter().map(|x| self.value(x, &fr)).collect();
                        let CVal::Rec(label, _) = &fv else {
                            panic!("calling a number")
                        };
                        let block = &self.codes[*label];
                        assert_eq!(block.params.len(), args.len(), "arity mismatch");
                        code = Some(block.body.clone());
                        fr = Frame {
                            locals: block.params.iter().cloned().zip(args).collect(),
                            me: Some(fv),
                        };
                        break;
                    }
                    other => return self.value(other, &fr),
                }
            }
        }
    }
    /// Non-tail positions.
    fn value(&self, c: &C, fr: &Frame) -> CVal {
        match c {
            C::Int(k) => CVal::Int(*k),
            C::Local(x) => fr
                .locals
                .iter()
                .rev()
                .find(|(n, _)| n == x)
                .expect("local")
                .1
                .clone(),
            C::Env(i) => match &fr.me {
                Some(CVal::Rec(_, slots)) => slots[*i].clone(),
                _ => panic!("no closure record in scope"),
            },
            C::Itself => fr.me.clone().expect("inside a function"),
            C::Prim(o, a, b) => {
                CVal::Int(o.apply(c_int(self.value(a, fr)), c_int(self.value(b, fr))))
            }
            C::MakeClosure(l, a) => CVal::Rec(*l, a.iter().map(|x| self.value(x, fr)).collect()),
            tail => self.eval(
                tail,
                Frame {
                    locals: fr.locals.clone(),
                    me: fr.me.clone(),
                },
            ),
        }
    }
    fn run(&self) -> i64 {
        c_int(self.eval(
            &self.main,
            Frame {
                locals: vec![],
                me: None,
            },
        ))
    }
    fn size(&self) -> usize {
        c_size(&self.main)
            + self
                .codes
                .iter()
                .map(|c| c_size(&c.body) + c.params.len() + c.captured.len())
                .sum::<usize>()
    }
    fn check_closed(&self) -> bool {
        is_closed_code(&self.main, &mut vec![], 0, &self.codes)
            && self.codes.iter().all(|c| {
                is_closed_code(
                    &c.body,
                    &mut c.params.clone(),
                    c.captured.len(),
                    &self.codes,
                )
            })
    }
}

// ---------------------------------------------------------------- 4. lambda lifting

/// A supercombinator: all its former free variables are leading parameters.
struct Super {
    extra: usize,
    params: Vec<Name>,
    body: E,
}
/// Lifted code reuses `E`, with two conventions: `App(Var("@N"), args)` names supercombinator N (a global), and there is no Lam/LetRec.
struct Lifted {
    supers: Vec<Super>,
    main: E,
}
fn global(n: usize) -> E {
    E::Var(format!("@{n}").into())
}
fn global_index(e: &E) -> Option<usize> {
    match e {
        E::Var(x) => x.strip_prefix('@').and_then(|n| n.parse().ok()),
        _ => None,
    }
}
struct Lifter {
    supers: Vec<Super>,
    /// letrec name -> (supercombinator, its extra parameters)
    known: HashMap<Name, (usize, Vec<Name>)>,
    direct_calls: usize,
}
impl Lifter {
    /// A free occurrence of a known function needs that function's extras, not the function itself (Johnsson's closure of the sets).
    fn extras_for(&self, fv: BTreeSet<Name>) -> Vec<Name> {
        let mut out = BTreeSet::new();
        for x in fv {
            match self.known.get(&x) {
                Some((_, ex)) => out.extend(ex.iter().cloned()),
                None => {
                    out.insert(x);
                }
            }
        }
        out.into_iter().collect()
    }
    fn reference(&self, sc: usize, extras: &[Name]) -> E {
        // a partial application of a global to its extras: represented as `(@partial @N e1 e2 ...)`
        app(
            E::Var("@partial".into()),
            std::iter::once(global(sc))
                .chain(extras.iter().map(var))
                .collect(),
        )
    }
    fn define(&mut self, me: Option<&Name>, ps: &[Name], body: &E, extras: Vec<Name>) -> usize {
        let sc = self.supers.len();
        self.supers.push(Super {
            extra: extras.len(),
            params: [extras.clone(), ps.to_vec()].concat(),
            body: E::Int(0),
        });
        if let Some(f) = me {
            self.known.insert(f.clone(), (sc, extras));
        }
        let lifted = self.lift(body);
        self.supers[sc].body = lifted;
        sc
    }
    /// Requires unique binders: an extra argument is passed BY NAME at each call site, so no binder in between may shadow it.
    fn lift(&mut self, e: &E) -> E {
        match e {
            E::Int(k) => E::Int(*k),
            E::Var(x) => match self.known.get(x) {
                Some((sc, ex)) => self.reference(*sc, ex),
                None => var(x),
            },
            E::Prim(o, a, b) => prim(*o, self.lift(a), self.lift(b)),
            E::If(c, t, f) => if_(self.lift(c), self.lift(t), self.lift(f)),
            E::Let(x, d, b) => {
                let d2 = self.lift(d);
                let_(x.clone(), d2, self.lift(b))
            }
            E::Halt(a) => E::Halt(Box::new(self.lift(a))),
            E::Lam(ps, b) => {
                let extras = self.extras_for(free_vars(e));
                let sc = self.define(None, ps, b, extras.clone());
                self.reference(sc, &extras)
            }
            E::LetRec(f, ps, fb, rest) => {
                let mut fv = free_vars(&E::Lam(ps.clone(), fb.clone()));
                fv.remove(f);
                let extras = self.extras_for(fv);
                self.define(Some(f), ps, fb, extras);
                self.lift(rest) // no code at the definition site
            }
            E::App(f, a) => {
                let args: Vec<E> = a.iter().map(|x| self.lift(x)).collect();
                if let E::Var(g) = &**f {
                    if let Some((sc, ex)) = self.known.get(g) {
                        if self.supers[*sc].params.len() == ex.len() + args.len() {
                            self.direct_calls += 1;
                            return app(global(*sc), ex.iter().map(var).chain(args).collect());
                        }
                    }
                }
                app(self.lift(f), args)
            }
        }
    }
}
fn lift_program(e: &E) -> (Lifted, usize) {
    let mut l = Lifter {
        supers: vec![],
        known: HashMap::new(),
        direct_calls: 0,
    };
    let main = l.lift(e);
    (
        Lifted {
            supers: l.supers,
            main,
        },
        l.direct_calls,
    )
}
#[derive(Clone)]
enum PVal {
    Int(i64),
    Partial(usize, Rc<Vec<PVal>>),
}
fn p_int(v: PVal) -> i64 {
    match v {
        PVal::Int(k) => k,
        PVal::Partial(..) => panic!("integer expected"),
    }
}
impl Lifted {
    fn eval(&self, term: &E, locals: &mut Vec<(Name, PVal)>) -> PVal {
        let mark = locals.len();
        let r = match term {
            E::Int(k) => PVal::Int(*k),
            E::Var(x) => locals
                .iter()
                .rev()
                .find(|(n, _)| n == x)
                .unwrap_or_else(|| panic!("supercombinator has a free variable {x}"))
                .1
                .clone(),
            E::Prim(o, a, b) => {
                PVal::Int(o.apply(p_int(self.eval(a, locals)), p_int(self.eval(b, locals))))
            }
            E::If(c, t, f) => {
                let b = p_int(self.eval(c, locals)) != 0;
                self.eval(if b { t } else { f }, locals)
            }
            E::Let(x, d, b) => {
                let v = self.eval(d, locals);
                locals.push((x.clone(), v));
                self.eval(b, locals)
            }
            E::Halt(a) => self.eval(a, locals),
            E::App(f, a) => {
                if matches!(&**f, E::Var(x) if &**x == "@partial") {
                    let sc = global_index(&a[0]).expect("a global");
                    PVal::Partial(
                        sc,
                        Rc::new(a[1..].iter().map(|x| self.eval(x, locals)).collect()),
                    )
                } else {
                    let (sc, mut args) = match global_index(f) {
                        Some(sc) => (sc, vec![]),
                        None => match self.eval(f, locals) {
                            PVal::Partial(sc, held) => (sc, (*held).clone()),
                            PVal::Int(_) => panic!("calling a number"),
                        },
                    };
                    args.extend(a.iter().map(|x| self.eval(x, locals)));
                    let s = &self.supers[sc];
                    assert_eq!(s.params.len(), args.len(), "saturated call");
                    // a supercombinator sees ONLY its parameters
                    let mut frame: Vec<(Name, PVal)> = s.params.iter().cloned().zip(args).collect();
                    self.eval(&s.body, &mut frame)
                }
            }
            E::Lam(..) | E::LetRec(..) => panic!("lifting left a nested function"),
        };
        locals.truncate(mark);
        r
    }
    fn run(&self) -> i64 {
        p_int(self.eval(&self.main, &mut vec![]))
    }
    fn size(&self) -> usize {
        size(&self.main)
            + self
                .supers
                .iter()
                .map(|s| size(&s.body) + s.params.len())
                .sum::<usize>()
    }
    fn has_no_nested_functions(&self) -> bool {
        fn ok(e: &E) -> bool {
            !matches!(e, E::Lam(..) | E::LetRec(..)) && kids(e).iter().all(|k| ok(k))
        }
        ok(&self.main) && self.supers.iter().all(|s| ok(&s.body))
    }
}

// ---------------------------------------------------------------- 5. defunctionalisation

/// First-order values: numbers and constructors. No code, no environments, so they can be compared, hashed, printed and parsed.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Data {
    Int(i64),
    Con(usize, Vec<Data>),
}
impl Data {
    fn int(&self) -> i64 {
        match self {
            Data::Int(k) => *k,
            Data::Con(..) => panic!("integer expected"),
        }
    }
    fn write(&self, out: &mut String) {
        match self {
            Data::Int(k) => out.push_str(&format!("{k} ")),
            Data::Con(tag, fields) => {
                out.push_str(&format!("( {tag} "));
                fields.iter().for_each(|f| f.write(out));
                out.push_str(") ");
            }
        }
    }
    fn read(toks: &mut std::str::SplitWhitespace) -> Option<Option<Data>> {
        match toks.next()? {
            ")" => Some(None),
            "(" => {
                let tag = toks.next()?.parse().ok()?;
                let mut fields = vec![];
                while let Some(f) = Data::read(toks)? {
                    fields.push(f);
                }
                Some(Some(Data::Con(tag, fields)))
            }
            n => Some(Some(Data::Int(n.parse().ok()?))),
        }
    }
}
/// One arm of `apply`: `apply(Fk{captured..}, params..) = body`.
/// The defunctionalised program is the closure-converted one with a different reading: the label is a CONSTRUCTOR, and `Call` is `apply`.
struct Defun {
    arms: Closed,
}
enum Step {
    Apply(Data, Vec<Data>),
    Halt(Data),
}
impl Defun {
    /// Requires CPS input, so that every right-hand side is straight-line code ending in `apply` or `halt`.
    fn from_cps(e: &E) -> Defun {
        assert!(is_cps(e));
        Defun {
            arms: close_program(e),
        }
    }
    fn data(&self, c: &C, locals: &[(Name, Data)], me: Option<&Data>) -> Data {
        match c {
            C::Int(k) => Data::Int(*k),
            C::Local(x) => locals
                .iter()
                .rev()
                .find(|(n, _)| n == x)
                .expect("local")
                .1
                .clone(),
            C::Env(i) => match me {
                Some(Data::Con(_, fields)) => fields[*i].clone(),
                _ => panic!("no scrutinee"),
            },
            C::Itself => me.expect("inside apply").clone(),
            C::Prim(o, a, b) => Data::Int(o.apply(
                self.data(a, locals, me).int(),
                self.data(b, locals, me).int(),
            )),
            C::MakeClosure(tag, a) => {
                Data::Con(*tag, a.iter().map(|x| self.data(x, locals, me)).collect())
            }
            C::If(..) | C::Call(..) | C::Let(..) | C::Halt(_) => {
                panic!("not first order straight-line code")
            }
        }
    }
    fn body(&self, mut term: &C, mut locals: Vec<(Name, Data)>, me: Option<&Data>) -> Step {
        loop {
            match term {
                C::Let(x, d, b) => {
                    let v = self.data(d, &locals, me);
                    locals.push((x.clone(), v));
                    term = b;
                }
                C::If(c, t, f) => {
                    term = if self.data(c, &locals, me).int() != 0 {
                        t
                    } else {
                        f
                    }
                }
                C::Halt(a) => return Step::Halt(self.data(a, &locals, me)),
                C::Call(f, a) => {
                    return Step::Apply(
                        self.data(f, &locals, me),
                        a.iter().map(|x| self.data(x, &locals, me)).collect(),
                    );
                }
                other => panic!("not in tail form: {other:?}"),
            }
        }
    }
    /// The one and only `apply` function.
    fn apply(&self, f: &Data, args: Vec<Data>) -> Step {
        let Data::Con(tag, _) = f else {
            panic!("applying a number")
        };
        let arm = &self.arms.codes[*tag];
        assert_eq!(arm.params.len(), args.len());
        self.body(
            &arm.body,
            arm.params.iter().cloned().zip(args).collect(),
            Some(f),
        )
    }
    fn start(&self) -> Step {
        self.body(&self.arms.main, vec![], None)
    }
    /// Drive the machine. With `checkpoint`, after every k-th step the WHOLE machine state is serialised to text and rebuilt from it.
    fn run(&self, checkpoint: Option<usize>) -> (i64, usize, usize) {
        let mut step = self.start();
        let (mut steps, mut largest) = (0, 0);
        loop {
            match step {
                Step::Halt(v) => return (v.int(), steps, largest),
                Step::Apply(f, args) => {
                    steps += 1;
                    let (f, args) = match checkpoint {
                        Some(k) if steps % k == 0 => {
                            let text = save(&f, &args);
                            largest = largest.max(text.len());
                            load(&text).expect("a saved state loads")
                        }
                        _ => (f, args),
                    };
                    step = self.apply(&f, args);
                }
            }
        }
    }
    /// The generated program as readable source.
    fn listing(&self) -> Vec<String> {
        self.arms
            .codes
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "F{i} {{ {} }}  applied to ({})",
                    c.captured.join(", "),
                    c.params.join(", ")
                )
            })
            .collect()
    }
}
fn save(f: &Data, args: &[Data]) -> String {
    let mut s = String::new();
    f.write(&mut s);
    args.iter().for_each(|a| a.write(&mut s));
    s
}
fn load(text: &str) -> Option<(Data, Vec<Data>)> {
    let mut toks = text.split_whitespace();
    let mut items = vec![];
    while let Some(d) = Data::read(&mut toks) {
        items.push(d?);
    }
    let mut it = items.into_iter();
    Some((it.next()?, it.collect()))
}

// ---------------------------------------------------------------- random typed programs

#[derive(Clone, PartialEq)]
enum Ty {
    Int,
    Fun(Vec<Ty>, Box<Ty>),
}
const POOL: [&str; 6] = ["x", "y", "z", "f", "g", "k"];
struct Gen<'r> {
    rng: &'r mut Rng,
}
impl Gen<'_> {
    fn ty(&mut self, d: usize) -> Ty {
        if d == 0 || self.rng.below(3) > 0 {
            return Ty::Int;
        }
        let n = 1 + self.rng.below(2);
        Ty::Fun(
            (0..n).map(|_| self.ty(d - 1)).collect(),
            Box::new(self.ty(d - 1)),
        )
    }
    /// A random name from a tiny pool, so that shadowing is common.
    fn pick_name(&mut self, taken: &[Name]) -> Name {
        loop {
            let n = name(POOL[self.rng.below(POOL.len())]);
            if !taken.contains(&n) {
                return n;
            }
        }
    }
    fn params(&mut self, tys: &[Ty], ctx: &mut Vec<(Name, Ty)>) -> Vec<Name> {
        let mut ps: Vec<Name> = vec![];
        for t in tys {
            let p = self.pick_name(&ps);
            ctx.push((p.clone(), t.clone()));
            ps.push(p);
        }
        ps
    }
    fn variable(&mut self, ty: &Ty, ctx: &[(Name, Ty)]) -> Option<E> {
        // only the innermost binding of each name is visible
        let visible: Vec<&Name> = ctx
            .iter()
            .enumerate()
            .filter(|(i, (n, t))| t == ty && !ctx[i + 1..].iter().any(|(m, _)| m == n))
            .map(|(_, p)| &p.0)
            .collect();
        (!visible.is_empty()).then(|| var(visible[self.rng.below(visible.len())]))
    }
    fn term(&mut self, ty: &Ty, ctx: &mut Vec<(Name, Ty)>, d: usize) -> E {
        let roll = self.rng.below(if d == 0 { 3 } else { 12 });
        let mark = ctx.len();
        let e = match roll {
            0..=1 => match self.variable(ty, ctx) {
                Some(v) => v,
                None => self.canonical(ty, ctx, d),
            },
            2 => self.canonical(ty, ctx, d),
            3 | 4 if *ty == Ty::Int => {
                let op = [Op::Add, Op::Sub, Op::Mul, Op::Lt][self.rng.below(4)];
                prim(op, self.term(ty, ctx, d - 1), self.term(ty, ctx, d - 1))
            }
            5 => if_(
                self.term(&Ty::Int, ctx, d - 1),
                self.term(ty, ctx, d - 1),
                self.term(ty, ctx, d - 1),
            ),
            6 | 7 => {
                let t = self.ty(2);
                let def = self.term(&t, ctx, d - 1);
                let x = self.pick_name(&[]);
                ctx.push((x.clone(), t));
                let_(x, def, self.term(ty, ctx, d - 1))
            }
            8 | 9 => {
                let n = 1 + self.rng.below(2);
                let tys: Vec<Ty> = (0..n).map(|_| self.ty(1)).collect();
                let f = self.term(&Ty::Fun(tys.clone(), Box::new(ty.clone())), ctx, d - 1);
                app(f, tys.iter().map(|t| self.term(t, ctx, d - 1)).collect())
            }
            10 => self.recursion(ty, ctx, d),
            _ => self.canonical(ty, ctx, d),
        };
        ctx.truncate(mark);
        e
    }
    fn canonical(&mut self, ty: &Ty, ctx: &mut Vec<(Name, Ty)>, d: usize) -> E {
        match ty {
            Ty::Int => E::Int(self.rng.below(10) as i64),
            Ty::Fun(a, r) => {
                let mark = ctx.len();
                let ps = self.params(a, ctx);
                let body = self.term(r, ctx, d.saturating_sub(1));
                ctx.truncate(mark);
                lam(ps, body)
            }
        }
    }
    /// A recursive function that terminates for EVERY argument: it recurses only when 1 <= n <= 5, on n - 1.
    fn recursion(&mut self, ty: &Ty, ctx: &mut Vec<(Name, Ty)>, d: usize) -> E {
        let mark = ctx.len();
        let (f, n, r) = (self.pick_name(&[]), name("n"), name("r"));
        ctx.push((n.clone(), Ty::Int));
        let base = self.term(&Ty::Int, ctx, d - 1);
        // `n` and `r` are not in the name pool, so the guard and the recursive call cannot be shadowed
        let recur = app(var(&f), vec![prim(Op::Sub, var(&n), E::Int(1))]);
        ctx.push((r.clone(), Ty::Int));
        let step = self.term(&Ty::Int, ctx, d - 1);
        ctx.truncate(mark);
        let inner = let_(r, recur, step);
        let body = if_(
            prim(Op::Lt, var(&n), E::Int(1)),
            base.clone(),
            if_(prim(Op::Lt, E::Int(5), var(&n)), base, inner),
        );
        ctx.push((f.clone(), Ty::Fun(vec![Ty::Int], Box::new(Ty::Int))));
        let rest = self.term(ty, ctx, d - 1);
        E::LetRec(f, vec![n], Rc::new(body), Box::new(rest))
    }
}
/// `base` may not mention `f`: it is generated before `f` is in scope. But an OUTER `f` of the pool could be referenced by it,
/// and inside the body that name now means the new function. So reject bodies where that changed the type or meaning.
fn body_is_safe(e: &E) -> bool {
    match e {
        E::LetRec(f, _, body, rest) => {
            let E::If(_, base, other) = &**body else {
                return true;
            };
            let E::If(_, _, inner) = &**other else {
                return true;
            };
            let E::Let(_, _, step) = &**inner else {
                return true;
            };
            !free_vars(base).contains(f)
                && !free_vars(step).contains(f)
                && body_is_safe(base)
                && body_is_safe(step)
                && body_is_safe(rest)
        }
        other => kids(other).iter().all(|k| body_is_safe(k)),
    }
}
fn random_program(rng: &mut Rng) -> E {
    loop {
        let mut g = Gen { rng: &mut *rng };
        let e = g.term(&Ty::Int, &mut vec![], 5);
        if body_is_safe(&e) && size(&e) >= 8 {
            return e;
        }
    }
}

// ---------------------------------------------------------------- verification and demos

const STAGES: [&str; 8] = [
    "source",
    "A-normal form",
    "CPS (Plotkin)",
    "CPS (one-pass)",
    "closure conv. (direct)",
    "closure conv. (of CPS)",
    "lambda lifted",
    "defunctionalised CPS",
];

/// Run every stage on one closed program; returns the common result and the code size after each stage.
fn all_stages(src: &E, direct_calls: &mut usize) -> (i64, [usize; 8]) {
    let want = int(&eval(src, &None, &mut Meter::default()));
    let fresh = Fresh::default();
    let u = unique(src, &fresh);
    assert_eq!(
        int(&eval(&u, &None, &mut Meter::default())),
        want,
        "uniquify: {}",
        show(src)
    );
    assert_eq!(
        int(&eval_tr(&u, &None, &mut Meter::default())),
        want,
        "trampoline: {}",
        show(src)
    );

    let a = anf(&u, &fresh);
    assert!(is_anf(&a), "not in ANF: {}", show(&a));
    assert_eq!(
        int(&eval(&a, &None, &mut Meter::default())),
        want,
        "ANF: {}",
        show(src)
    );

    let plot = plotkin_program(&u, &fresh);
    let one = cps_program(&u, &fresh);
    for (label, p) in [
        ("Plotkin", &plot),
        ("one-pass", &one),
        ("ANF then one-pass", &cps_program(&a, &fresh)),
    ] {
        // CPS passes require unique binders in the OUTPUT too (ANF input may reuse nothing)
        assert!(is_cps(p), "{label} output is not in tail form: {}", show(p));
        assert_eq!(run_cps(p).0, want, "{label}: {}", show(src));
        let mut calls = (0, 0);
        count_calls(p, true, &mut calls);
        assert_eq!(calls.1, 0, "{label}: every call in CPS is a tail call");
    }
    assert_eq!(
        redexes(&one),
        source_redexes(&u),
        "the one-pass transform adds no administrative redex: {}",
        show(src)
    );
    assert!(redexes(&plot) > redexes(&one));

    let cc = close_program(&u);
    let cc_cps = close_program(&one);
    for (label, c) in [("direct", &cc), ("CPS", &cc_cps)] {
        assert!(
            c.check_closed(),
            "closure conversion ({label}) left a free variable"
        );
        assert_eq!(c.run(), want, "closure conversion ({label}): {}", show(src));
    }

    let (lifted, direct) = lift_program(&u);
    *direct_calls += direct;
    assert!(lifted.has_no_nested_functions());
    assert_eq!(lifted.run(), want, "lambda lifting: {}", show(src));

    let d = Defun::from_cps(&one);
    assert_eq!(d.run(None).0, want, "defunctionalised: {}", show(src));
    assert_eq!(
        d.run(Some(1)).0,
        want,
        "defunctionalised, state saved and reloaded at every step"
    );
    (
        want,
        [
            size(&u),
            size(&a),
            size(&plot),
            size(&one),
            cc.size(),
            cc_cps.size(),
            lifted.size(),
            d.arms.size(),
        ],
    )
}

fn demo_pipeline() {
    println!("== one program through every stage ==");
    let src = parse("(let a 3 (letrec sum (n) (if (< n 1) 0 (+ (* a n) (sum (- n 1)))) (sum 4)))");
    assert_eq!(parse(&show(&src)), src, "printer and parser round trip");
    let fresh = Fresh::default();
    let u = unique(&src, &fresh);
    println!("  source    : {}", show(&src));
    println!("  uniquified: {}", show(&u));
    println!("  ANF       : {}", show(&anf(&u, &fresh)));
    println!("  CPS       : {}", show(&cps_program(&u, &fresh)));
    let (lifted, direct) = lift_program(&u);
    for (i, s) in lifted.supers.iter().enumerate() {
        println!(
            "  lifted    : @{i}({}) [{} extra] = {}",
            s.params.join(" "),
            s.extra,
            show(&s.body)
        );
    }
    println!(
        "  lifted    : main = {}   ({direct} direct calls: no closure is ever allocated)",
        show(&lifted.main)
    );
    let d = Defun::from_cps(&cps_program(&u, &fresh));
    for line in d.listing() {
        println!("  defun     : {line}");
    }
    let mut ignored = 0;
    assert_eq!(all_stages(&src, &mut ignored).0, 30);
}

fn demo_hygiene() {
    println!("== why uniquify comes first ==");
    let src = parse("(let y 1 (+ y (let y 10 y)))");
    let right = int(&eval(&src, &None, &mut Meter::default()));
    let captured = run_cps(&cps_program(&src, &Fresh::default())).0;
    let fixed = run_cps(&cps_program(
        &unique(&src, &Fresh::default()),
        &Fresh::default(),
    ))
    .0;
    println!(
        "  {}: source = {right}, CPS without uniquify = {captured}, CPS after uniquify = {fixed}",
        show(&src)
    );
    assert_eq!(
        (right, captured, fixed),
        (11, 20, 11),
        "the inner `y` captures the outer one when code moves under it"
    );
}

fn demo_tail_calls() {
    println!("== tail calls: evaluator recursion depth for sum(n) ==");
    let looping = |n: i64| {
        parse(&format!(
            "(letrec go (n acc) (if (< n 1) acc (go (- n 1) (+ acc n))) (go {n} 0))"
        ))
    };
    let nontail = |n: i64| {
        parse(&format!(
            "(letrec sum (n) (if (< n 1) 0 (+ n (sum (- n 1)))) (sum {n}))"
        ))
    };
    println!(
        "  {:<22} {:>8} {:>14} {:>14} {:>18}",
        "program", "n", "big-step depth", "trampoline", "CPS machine calls"
    );
    for n in [100i64, 1000, 10_000] {
        for (label, p) in [
            ("accumulator (tail)", looping(n)),
            ("n + sum (not tail)", nontail(n)),
        ] {
            let (mut plain, mut tramp) = (Meter::default(), Meter::default());
            let want = n * (n + 1) / 2;
            assert_eq!(int(&eval(&p, &None, &mut plain)), want);
            assert_eq!(int(&eval_tr(&p, &None, &mut tramp)), want);
            let (v, calls) = run_cps(&cps_program(
                &unique(&p, &Fresh::default()),
                &Fresh::default(),
            ));
            assert_eq!(v, want);
            println!(
                "  {label:<22} {n:>8} {:>14} {:>14} {calls:>18}",
                plain.max_depth, tramp.max_depth
            );
            assert!(
                plain.max_depth > n as usize,
                "the big-step interpreter always grows"
            );
            if label.starts_with("accumulator") {
                assert!(
                    tramp.max_depth <= 4,
                    "tail calls run in constant depth: {}",
                    tramp.max_depth
                );
            } else {
                assert!(
                    tramp.max_depth > n as usize,
                    "a trampoline cannot remove NON-tail recursion; CPS can (it moves it to the heap)"
                );
            }
        }
    }
    let mut calls = (0, 0);
    count_calls(&nontail(1), true, &mut calls);
    assert_eq!(
        calls,
        (1, 1),
        "(sum 1) is a tail call, (sum (- n 1)) under `+` is not"
    );
}

fn demo_checkpoint() {
    println!("== defunctionalised continuations are data ==");
    let p = parse("(letrec fact (n) (if (< n 1) 1 (* n (fact (- n 1)))) (fact 12))");
    let d = Defun::from_cps(&cps_program(
        &unique(&p, &Fresh::default()),
        &Fresh::default(),
    ));
    // stop in the middle, keep ONLY the text, and resume
    let mut step = d.start();
    for _ in 0..7 {
        let Step::Apply(f, a) = step else {
            panic!("halted too early")
        };
        step = d.apply(&f, a);
    }
    let Step::Apply(f, a) = step else {
        panic!("halted too early")
    };
    let text = save(&f, &a);
    println!("  state after 7 calls ({} bytes): {text}", text.len());
    drop((f, a));
    let (f, a) = load(&text).expect("loads");
    let mut step = d.apply(&f, a);
    let result = loop {
        match step {
            Step::Halt(v) => break v.int(),
            Step::Apply(f, a) => step = d.apply(&f, a),
        }
    };
    println!("  resumed from text: 12! = {result}");
    assert_eq!(result, 479_001_600);
    assert!(load("( 1 2").is_none(), "a truncated state is rejected");
}

fn test_free_vars(rng: &mut Rng) {
    let (mut terms, mut open) = (0, 0);
    let mut queue: Vec<E> = (0..300).map(|_| random_program(rng)).collect();
    while let Some(e) = queue.pop() {
        let fv = free_vars(&e);
        assert_eq!(fv, free_vars_oracle(&e), "free variables of {}", show(&e));
        terms += 1;
        open += usize::from(!fv.is_empty());
        queue.extend(kids(&e).into_iter().cloned());
    }
    println!(
        "== free-variable analysis agrees with the oracle on {terms} subterms ({open} of them open) =="
    );
    assert!(open > 1000);
}

fn test_random(rng: &mut Rng) {
    let mut totals = [0usize; 8];
    let (mut direct, mut letrecs) = (0, 0);
    for _ in 0..1500 {
        let p = random_program(rng);
        letrecs += usize::from(show(&p).contains("letrec"));
        let (_, sizes) = all_stages(&p, &mut direct);
        totals.iter_mut().zip(sizes).for_each(|(t, s)| *t += s);
    }
    println!(
        "== 1500 random closed programs ({letrecs} with letrec): every stage gives the source result =="
    );
    println!(
        "  also checked: ANF shape, CPS tail form, closed code, no nested functions after lifting, save/load at every defun step"
    );
    println!("  lambda lifting turned {direct} call sites into direct (allocation-free) calls");
    println!("  {:<26} {:>10} {:>8}", "stage", "avg nodes", "growth");
    for (stage, t) in STAGES.iter().zip(totals) {
        println!(
            "  {stage:<26} {:>10.1} {:>7.2}x",
            t as f64 / 1500.0,
            t as f64 / totals[0] as f64
        );
    }
    assert!(
        letrecs > 100 && direct > 0,
        "the sample must exercise recursion and direct calls"
    );
    assert!(
        totals[2] > totals[3],
        "Plotkin output is bigger than one-pass output"
    );
}

fn main() {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(|| {
            demo_pipeline();
            demo_hygiene();
            demo_tail_calls();
            demo_checkpoint();
            let mut rng = Rng(0x0814_C0DE_9E37_79B9);
            test_free_vars(&mut rng);
            test_random(&mut rng);
            println!("all CPS / closure conversion / lifting checks passed");
        })
        .expect("spawn")
        .join()
        .expect("checks passed");
}
