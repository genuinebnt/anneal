//! RECIPE 812 (compilers): a tiny dependent type theory: normalisation by evaluation, bidirectional elaboration with holes, proofs by induction.
//! LEVEL: advanced
//! Use in: proof assistants, dependently typed languages (Agda, Lean, Idris, Coq), refinement/indexed types, and any checker that compares types up to computation.   Run: cargo run --release --example dependent_types_nbe
//! COMMON USES: the kernel/elaborator split of Lean 4, Agda, Coq, Idris 2 and Arend (evaluate into a semantic domain, compare there, read back
//! only to print), type-level computation in Rust-adjacent tools (const generics are the tiny, decidable cousin: `[T; N + 1]`), proof-carrying code,
//! verified compilers such as CompCert and seL4's Isabelle proofs (the same idea, different logic), and checking SMT/proof certificates.
//! Course: Abel, Coquand and others, tutorials on normalisation by evaluation; András Kovács, "elaboration-zoo" (structure of this file).
#![allow(dead_code)]
//!
//! THE THEORY (Martin-Lof style, deliberately small): terms and types are the SAME syntax. `(x : A) -> B` is the dependent function type
//! (a Pi type: the result type may mention the argument, which is how `Vec A n` can say "n elements"). `Type : Type` keeps the rule set
//! tiny but is INCONSISTENT (Girard's paradox): fine for a teaching kernel, never for a real prover, which has a universe hierarchy
//! `Type 0 : Type 1 : ...`. Nat has zero, suc and the eliminator `natrec` (induction principle AND recursion). `Vec A n` is an indexed family with
//! `nil`, `cons`, `vecrec`. `Id A a b` is propositional equality with `refl` and the eliminator `J` (the induction principle of equality:
//! to prove something about every `p : Id A a b` it is enough to prove it for `refl`). All constants are PRIMITIVES with their types written as
//! ordinary source text and checked once; their computation rules live in one function (`reduce_elim`), so the checker has no special cases.
//! WHY NORMALISATION BY EVALUATION: type checking needs "are these two types equal up to computation?" (`Vec A (plus 2 3)` vs `Vec A 5`).
//! Substituting naively and reducing terms is slow and capture-prone. NbE instead EVALUATES a term into a semantic domain of host-language
//! values (closures for lambdas, NEUTRALS for stuck terms such as `natrec P z s n` with a variable `n`), compares values, and only
//! reads back (`quote`) to syntax when printing. Two index systems fall out naturally: terms use de Bruijn INDICES (distance to the binder: good
//! for substitution and alpha-equivalence), values use de Bruijn LEVELS (distance from the root: a variable keeps its name when it is moved under
//! more binders, which is what makes weakening free). `quote` converts levels back to indices: `index = depth - level - 1`.
//! CONVERSION (definitional equality) compares values structurally and uses the ETA rule for functions: `f` equals `\x. f x`, checked by
//! applying both sides to a fresh variable. It is symmetric and transitive because it equates normal forms; the tests check exactly that.
//! BIDIRECTIONAL ELABORATION: `infer` returns a term and its type; `check` takes the expected type, which is what lets `\x. e` omit its
//! annotation. HOLES: `_` makes a METAVARIABLE solved by unification (so `cons Nat _ 1 xs` needs no length argument, and `cong suc _ _ ih`
//! finds both sides from `ih`); `?name` is a TYPED HOLE that fails with the goal type and the context, the "what do I need here?"
//! workflow of Agda/Lean. Our metas are simple: a meta may be solved only by a term whose variables were all in scope where the meta was created
//! (checked with levels) and that does not mention the meta (occurs check); metas are never applied to arguments, so there is no higher-order
//! pattern unification (the elaboration-zoo covers that next).
//! THE EXAMPLES, all checked by the kernel here: plus, cong, sym, trans, transport (from J), vector append with the length index
//! `Vec A (plus m n)`, vmap, associativity and commutativity of plus by induction (proof terms written out), plus 20 deliberately wrong programs.
//! HOW RUST DIFFERS: Rust's const generics evaluate `N + 1` only for concrete literals; `Vec<T, N>` with N a variable and a proof that `N + M == M + N` is
//! impossible without nightly `generic_const_exprs` precisely because the compiler has no normaliser for open terms. Everything here is the
//! opposite design: types are values you can compute with.
//! Read: András Kovács, elaboration-zoo https://github.com/AndrasKovacs/elaboration-zoo ; Abel, "Normalization by Evaluation: Dependent Types
//! and Impredicativity" (habilitation thesis, by title) and the tutorial "Checking Dependent Types with Normalization by Evaluation: A Tutorial"
//! (Coquand-style, cited by title, not fetched); Löh, McBride and Swierstra, "A tutorial implementation of a dependently typed lambda calculus"
//! (Fundamenta Informaticae 2010, by title); https://en.wikipedia.org/wiki/Normalisation_by_evaluation
//! TRY NEXT:
//!   - [S] Add `Bool` with `true`, `false` and `boolrec` to `PRIM_TYPES` and `reduce_elim`; prove `not (not b) = b` by induction on `b`.
//!   - [S] Remove the occurs check in `scope_check`: what happens to the occurs-check case in `test_holes_and_scope`? Predict first.
//!   - [M] Replace `Type : Type` by a hierarchy `Type 0`, `Type 1`: change `Val::Ty`, `check_ty` and `unify`; then try to type `Type : Type` and observe the rejection.
//!   - [M] Predict, then measure: time `nbe_nf` against `nf_naive` on `plus 12 12` unrolled as `natrec`; which is faster and why (sharing in closures vs substitution)?
//!   - [A] PROJECT: pair with `hindley_milner_inference` by elaborating its `forall` types into Pi types over `Type`, so an ML program becomes a term of this kernel.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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

// ------------------------------------------------------------------ primitives
/// (arity, is_eliminator). Constructors and type formers are stuck "canonical" values; eliminators compute.
fn prim_info(p: &str) -> (usize, bool) {
    match p {
        "Nat" | "zero" => (0, false),
        "suc" | "nil" => (1, false),
        "Vec" | "refl" => (2, false),
        "Id" => (3, false),
        "cons" => (4, false),
        "natrec" => (4, true),
        "vecrec" | "J" => (6, true),
        _ => panic!("unknown primitive {p}"),
    }
}
const PRIM_TYPES: &[(&str, &str)] = &[
    ("Nat", "Type"),
    ("zero", "Nat"),
    ("suc", "Nat -> Nat"),
    (
        "natrec",
        "(P : Nat -> Type) -> P zero -> ((k : Nat) -> P k -> P (suc k)) -> (n : Nat) -> P n",
    ),
    ("Vec", "Type -> Nat -> Type"),
    ("nil", "(A : Type) -> Vec A zero"),
    (
        "cons",
        "(A : Type) -> (n : Nat) -> A -> Vec A n -> Vec A (suc n)",
    ),
    (
        "vecrec",
        "(A : Type) -> (P : (n : Nat) -> Vec A n -> Type) -> P zero (nil A) -> ((n : Nat) -> (x : A) -> (xs : Vec A n) -> P n xs -> P (suc n) (cons A n x xs)) -> (n : Nat) -> (v : Vec A n) -> P n v",
    ),
    ("Id", "(A : Type) -> A -> A -> Type"),
    ("refl", "(A : Type) -> (a : A) -> Id A a a"),
    (
        "J",
        "(A : Type) -> (a : A) -> (P : (b : A) -> Id A a b -> Type) -> P a (refl A a) -> (b : A) -> (p : Id A a b) -> P b p",
    ),
];

// ------------------------------------------------------------------ syntax of the core: de Bruijn INDICES
#[derive(Clone, Debug, PartialEq)]
enum Tm {
    V(usize),
    Lam(String, Rc<Tm>),
    App(Box<Tm>, Box<Tm>),
    Pi(String, Box<Tm>, Rc<Tm>),
    Ty,
    Prim(&'static str),
    Meta(usize),
    Let(String, Box<Tm>, Rc<Tm>),
}
fn app_tm(f: Tm, a: Tm) -> Tm {
    Tm::App(Box::new(f), Box::new(a))
}
fn lam_tm(n: &str, b: Tm) -> Tm {
    Tm::Lam(n.into(), Rc::new(b))
}
fn prim(p: &str) -> Tm {
    Tm::Prim(
        PRIM_TYPES
            .iter()
            .find(|(n, _)| *n == p)
            .expect("known primitive")
            .0,
    )
}
fn apps(f: Tm, args: Vec<Tm>) -> Tm {
    args.into_iter().fold(f, app_tm)
}
fn nat_lit(n: usize) -> Tm {
    (0..n).fold(prim("zero"), |t, _| app_tm(prim("suc"), t))
}

// ------------------------------------------------------------------ semantic domain: de Bruijn LEVELS
#[derive(Clone, Debug)]
enum Val {
    Lam(String, Rc<Clo>),
    Pi(String, Rc<Val>, Rc<Clo>),
    Ty,
    /// a constructor / type former / partially applied primitive
    Prim(&'static str, Vec<Val>),
    /// stuck term: a head applied to arguments
    Neu(Head, Vec<Val>),
}
#[derive(Clone, Debug)]
enum Head {
    Lvl(usize),
    Meta(usize),
    /// a saturated eliminator whose scrutinee is not a constructor yet
    Stuck(&'static str, Vec<Val>),
}
#[derive(Debug)]
struct Clo {
    env: Vec<Val>,
    body: Rc<Tm>,
}
impl Clo {
    fn apply(&self, v: Val) -> Val {
        let mut e = self.env.clone();
        e.push(v);
        eval(&e, &self.body)
    }
}
fn var(l: usize) -> Val {
    Val::Neu(Head::Lvl(l), vec![])
}

// metavariable store: thread-local so that eval/quote need not thread it through every call
struct MetaInfo {
    sol: Option<Val>,
    lvl: usize,
}
thread_local! {
    static METAS: RefCell<Vec<MetaInfo>> = const { RefCell::new(Vec::new()) };
    static PRIM_TY_CACHE: RefCell<HashMap<&'static str, Val>> = RefCell::new(HashMap::new());
}
fn new_meta(lvl: usize) -> usize {
    METAS.with(|m| {
        let mut m = m.borrow_mut();
        m.push(MetaInfo { sol: None, lvl });
        m.len() - 1
    })
}
fn meta_sol(m: usize) -> Option<Val> {
    METAS.with(|ms| ms.borrow()[m].sol.clone())
}
fn meta_lvl(m: usize) -> usize {
    METAS.with(|ms| ms.borrow()[m].lvl)
}

/// Resolve solved metas at the head of a value.
fn force(v: &Val) -> Val {
    let mut cur = v.clone();
    loop {
        match &cur {
            Val::Neu(Head::Meta(m), sp) => match meta_sol(*m) {
                Some(s) => cur = sp.iter().fold(s, |f, a| app(f, a.clone())),
                None => return cur,
            },
            _ => return cur,
        }
    }
}
fn app(f: Val, a: Val) -> Val {
    match force(&f) {
        Val::Lam(_, c) => c.apply(a),
        Val::Prim(p, mut args) => {
            args.push(a);
            let (arity, elim) = prim_info(p);
            if elim && args.len() == arity {
                reduce_elim(p, args)
            } else {
                Val::Prim(p, args)
            }
        }
        Val::Neu(h, mut sp) => {
            sp.push(a);
            Val::Neu(h, sp)
        }
        other => {
            panic!("applying a non-function (ill-typed term reached the evaluator): {other:?}")
        }
    }
}
/// The computation rules of the whole theory.
fn reduce_elim(p: &'static str, a: Vec<Val>) -> Val {
    let stuck = |a: Vec<Val>| Val::Neu(Head::Stuck(p, a), vec![]);
    match p {
        "natrec" => match force(&a[3]) {
            Val::Prim("zero", _) => a[1].clone(),
            Val::Prim("suc", m) => {
                let rec = reduce_elim(
                    "natrec",
                    vec![a[0].clone(), a[1].clone(), a[2].clone(), m[0].clone()],
                );
                app(app(a[2].clone(), m[0].clone()), rec)
            }
            _ => stuck(a),
        },
        "vecrec" => match force(&a[5]) {
            Val::Prim("nil", _) => a[2].clone(),
            Val::Prim("cons", c) => {
                let rec = reduce_elim(
                    "vecrec",
                    vec![
                        a[0].clone(),
                        a[1].clone(),
                        a[2].clone(),
                        a[3].clone(),
                        c[1].clone(),
                        c[3].clone(),
                    ],
                );
                [c[1].clone(), c[2].clone(), c[3].clone(), rec]
                    .into_iter()
                    .fold(a[3].clone(), app)
            }
            _ => stuck(a),
        },
        "J" => match force(&a[5]) {
            Val::Prim("refl", _) => a[3].clone(),
            _ => stuck(a),
        },
        _ => unreachable!(),
    }
}
fn eval(env: &[Val], t: &Tm) -> Val {
    match t {
        Tm::V(i) => env[env.len() - 1 - i].clone(),
        Tm::Lam(n, b) => Val::Lam(
            n.clone(),
            Rc::new(Clo {
                env: env.to_vec(),
                body: b.clone(),
            }),
        ),
        Tm::App(f, a) => app(eval(env, f), eval(env, a)),
        Tm::Pi(n, a, b) => Val::Pi(
            n.clone(),
            Rc::new(eval(env, a)),
            Rc::new(Clo {
                env: env.to_vec(),
                body: b.clone(),
            }),
        ),
        Tm::Ty => Val::Ty,
        Tm::Prim(p) => Val::Prim(p, vec![]),
        Tm::Meta(m) => Val::Neu(Head::Meta(*m), vec![]),
        Tm::Let(_, d, b) => {
            let mut e = env.to_vec();
            e.push(eval(env, d));
            eval(&e, b)
        }
    }
}
/// Read a value back into a term: levels become indices relative to the current depth `lvl`.
fn quote(lvl: usize, v: &Val) -> Tm {
    match force(v) {
        Val::Lam(n, c) => Tm::Lam(n, Rc::new(quote(lvl + 1, &c.apply(var(lvl))))),
        Val::Pi(n, a, c) => Tm::Pi(
            n,
            Box::new(quote(lvl, &a)),
            Rc::new(quote(lvl + 1, &c.apply(var(lvl)))),
        ),
        Val::Ty => Tm::Ty,
        Val::Prim(p, args) => apps(Tm::Prim(p), args.iter().map(|a| quote(lvl, a)).collect()),
        Val::Neu(h, sp) => {
            let head = match h {
                Head::Lvl(l) => Tm::V(lvl - l - 1),
                Head::Meta(m) => Tm::Meta(m),
                Head::Stuck(p, args) => {
                    apps(Tm::Prim(p), args.iter().map(|a| quote(lvl, a)).collect())
                }
            };
            apps(head, sp.iter().map(|a| quote(lvl, a)).collect())
        }
    }
}
fn nbe_nf(ctx_len: usize, t: &Tm) -> Tm {
    let env: Vec<Val> = (0..ctx_len).map(var).collect();
    quote(ctx_len, &eval(&env, t))
}

// ------------------------------------------------------------------ a naive substitution-based normaliser (the oracle)
fn shift(t: &Tm, c: usize, k: usize) -> Tm {
    match t {
        Tm::V(i) => Tm::V(if *i >= c { i + k } else { *i }),
        Tm::Lam(n, b) => Tm::Lam(n.clone(), Rc::new(shift(b, c + 1, k))),
        Tm::App(f, a) => app_tm(shift(f, c, k), shift(a, c, k)),
        Tm::Pi(n, a, b) => Tm::Pi(
            n.clone(),
            Box::new(shift(a, c, k)),
            Rc::new(shift(b, c + 1, k)),
        ),
        Tm::Let(n, d, b) => Tm::Let(
            n.clone(),
            Box::new(shift(d, c, k)),
            Rc::new(shift(b, c + 1, k)),
        ),
        _ => t.clone(),
    }
}
/// Substitute `s` for index `j`, removing that binder (indices above `j` drop by one).
fn subst(t: &Tm, j: usize, s: &Tm) -> Tm {
    match t {
        Tm::V(i) => match (*i).cmp(&j) {
            std::cmp::Ordering::Equal => shift(s, 0, j),
            std::cmp::Ordering::Greater => Tm::V(i - 1),
            std::cmp::Ordering::Less => t.clone(),
        },
        Tm::Lam(n, b) => Tm::Lam(n.clone(), Rc::new(subst(b, j + 1, s))),
        Tm::App(f, a) => app_tm(subst(f, j, s), subst(a, j, s)),
        Tm::Pi(n, a, b) => Tm::Pi(
            n.clone(),
            Box::new(subst(a, j, s)),
            Rc::new(subst(b, j + 1, s)),
        ),
        Tm::Let(n, d, b) => Tm::Let(
            n.clone(),
            Box::new(subst(d, j, s)),
            Rc::new(subst(b, j + 1, s)),
        ),
        _ => t.clone(),
    }
}
fn spine(t: &Tm) -> (&Tm, Vec<&Tm>) {
    let (mut h, mut args) = (t, vec![]);
    while let Tm::App(f, a) = h {
        args.push(&**a);
        h = f;
    }
    args.reverse();
    (h, args)
}
/// Strong normalisation by repeated substitution. Quadratic and slow; used only as a differential oracle.
fn nf_naive(t: &Tm) -> Tm {
    match t {
        Tm::Lam(n, b) => Tm::Lam(n.clone(), Rc::new(nf_naive(b))),
        Tm::Pi(n, a, b) => Tm::Pi(n.clone(), Box::new(nf_naive(a)), Rc::new(nf_naive(b))),
        Tm::Let(_, d, b) => nf_naive(&subst(b, 0, d)),
        Tm::App(..) => {
            let (h, args) = spine(t);
            let h = nf_naive(h);
            if let Tm::Lam(_, b) = &h {
                let first = subst(b, 0, args[0]);
                return nf_naive(&args[1..].iter().fold(first, |f, a| app_tm(f, (*a).clone())));
            }
            let args: Vec<Tm> = args.iter().map(|a| nf_naive(a)).collect();
            if let Tm::Prim("natrec") = h {
                if args.len() >= 4 {
                    let (n_head, n_args) = spine(&args[3]);
                    let rest =
                        |t: Tm| nf_naive(&args[4..].iter().fold(t, |f, a| app_tm(f, a.clone())));
                    match (n_head, n_args.as_slice()) {
                        (Tm::Prim("zero"), []) => return rest(args[1].clone()),
                        (Tm::Prim("suc"), [m]) => {
                            let r = apps(
                                Tm::Prim("natrec"),
                                vec![
                                    args[0].clone(),
                                    args[1].clone(),
                                    args[2].clone(),
                                    (*m).clone(),
                                ],
                            );
                            return rest(app_tm(app_tm(args[2].clone(), (*m).clone()), r));
                        }
                        _ => {}
                    }
                }
            }
            apps(h, args)
        }
        _ => t.clone(),
    }
}
/// Eta-contract a beta-normal term: `\x. f x` becomes `f` when `f` does not mention `x`. Bottom-up, so nested redexes collapse too.
/// The checker's conversion includes eta, so an oracle for CONVERSION must compare beta-eta normal forms. `nf_naive` alone (like `nbe_nf`,
/// which does not eta-reduce either) tells `\x. f x` and `f` apart although they are definitionally equal.
fn eta_short(t: &Tm) -> Tm {
    match t {
        Tm::Lam(n, b) => {
            let b = eta_short(b);
            match &b {
                // index 0 is unused in `f`, so substituting anything for it only lowers the other indices by one
                Tm::App(f, a) if **a == Tm::V(0) && !uses(f, 0) => subst(f, 0, &Tm::Ty),
                _ => Tm::Lam(n.clone(), Rc::new(b)),
            }
        }
        Tm::App(f, a) => app_tm(eta_short(f), eta_short(a)),
        Tm::Pi(n, a, b) => Tm::Pi(n.clone(), Box::new(eta_short(a)), Rc::new(eta_short(b))),
        Tm::Let(n, d, b) => Tm::Let(n.clone(), Box::new(eta_short(d)), Rc::new(eta_short(b))),
        _ => t.clone(),
    }
}
/// Binder names are cosmetic; compare terms without them.
fn strip(t: &Tm) -> Tm {
    match t {
        Tm::Lam(_, b) => Tm::Lam("_".into(), Rc::new(strip(b))),
        Tm::Pi(_, a, b) => Tm::Pi("_".into(), Box::new(strip(a)), Rc::new(strip(b))),
        Tm::App(f, a) => app_tm(strip(f), strip(a)),
        Tm::Let(_, d, b) => Tm::Let("_".into(), Box::new(strip(d)), Rc::new(strip(b))),
        _ => t.clone(),
    }
}

// ------------------------------------------------------------------ printing
fn uses(t: &Tm, i: usize) -> bool {
    match t {
        Tm::V(j) => *j == i,
        Tm::Lam(_, b) => uses(b, i + 1),
        Tm::App(f, a) => uses(f, i) || uses(a, i),
        Tm::Pi(_, a, b) => uses(a, i) || uses(b, i + 1),
        Tm::Let(_, d, b) => uses(d, i) || uses(b, i + 1),
        _ => false,
    }
}
fn as_numeral(t: &Tm) -> Option<usize> {
    match t {
        Tm::Prim("zero") => Some(0),
        Tm::App(f, a) if matches!(**f, Tm::Prim("suc")) => as_numeral(a).map(|n| n + 1),
        _ => None,
    }
}
fn show_tm(names: &mut Vec<String>, t: &Tm, prec: u8) -> String {
    let paren = |s: String, need: bool| if need { format!("({s})") } else { s };
    if let Some(n) = as_numeral(t) {
        return n.to_string();
    }
    match t {
        Tm::V(i) => names
            .get(names.len().wrapping_sub(1 + i))
            .cloned()
            .unwrap_or_else(|| format!("#{i}")),
        Tm::Lam(n, b) => {
            names.push(n.clone());
            let s = format!("\\{n}. {}", show_tm(names, b, 0));
            names.pop();
            paren(s, prec > 0)
        }
        Tm::Pi(n, a, b) => {
            let dep = uses(b, 0);
            let dom = show_tm(names, a, if dep { 0 } else { 1 });
            names.push(n.clone());
            let cod = show_tm(names, b, 0);
            names.pop();
            paren(
                if dep {
                    format!("({n} : {dom}) -> {cod}")
                } else {
                    format!("{dom} -> {cod}")
                },
                prec > 0,
            )
        }
        Tm::App(..) => {
            let (h, args) = spine(t);
            let mut s = show_tm(names, h, 2);
            for a in args {
                s = format!("{s} {}", show_tm(names, a, 2));
            }
            paren(s, prec > 1)
        }
        Tm::Ty => "Type".into(),
        Tm::Prim(p) => p.to_string(),
        Tm::Meta(m) => format!("?{m}"),
        Tm::Let(n, d, b) => {
            let dd = show_tm(names, d, 0);
            names.push(n.clone());
            let s = format!("let {n} = {dd} in {}", show_tm(names, b, 0));
            names.pop();
            paren(s, prec > 0)
        }
    }
}

// ------------------------------------------------------------------ surface syntax
#[derive(Clone, Debug)]
enum S {
    Var(String),
    Lam(String, Box<S>),
    App(Box<S>, Box<S>),
    Pi(String, Box<S>, Box<S>),
    Ty,
    Num(usize),
    Ann(Box<S>, Box<S>),
    Hole,
    Named(String),
    Let(String, Box<S>, Box<S>, Box<S>),
}
#[derive(Clone, Debug, PartialEq)]
enum Tk {
    Id(String),
    Num(usize),
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
        } else {
            out.push(Tk::S(match c {
                '\\' => "\\",
                '.' => ".",
                ':' => ":",
                '(' => "(",
                ')' => ")",
                '=' => "=",
                '?' => "?",
                _ => return Err(format!("unexpected character `{c}`")),
            }));
            i += 1;
        }
    }
    Ok(out)
}
struct Parser {
    t: Vec<Tk>,
    i: usize,
}
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
    fn kw(&mut self, s: &str) -> bool {
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
    fn name(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tk::Id(x)) if x != "let" && x != "in" => {
                let x = x.clone();
                self.i += 1;
                Some(x)
            }
            _ => None,
        }
    }
    fn term(&mut self) -> R<S> {
        if self.eat("\\") {
            let mut names = vec![];
            while let Some(n) = self.name() {
                names.push(n);
            }
            if names.is_empty() {
                return Err("expected a binder after `\\`".into());
            }
            self.expect(".")?;
            let mut b = self.term()?;
            for n in names.into_iter().rev() {
                b = S::Lam(n, Box::new(b));
            }
            return Ok(b);
        }
        if self.kw("let") {
            let x = self.name().ok_or("expected a name after let")?;
            self.expect(":")?;
            let ty = self.term()?;
            self.expect("=")?;
            let d = self.term()?;
            if !self.kw("in") {
                return Err("expected `in`".into());
            }
            return Ok(S::Let(x, Box::new(ty), Box::new(d), Box::new(self.term()?)));
        }
        // `(x y : A) -> B`, else `A -> B`, else an application
        if matches!(self.peek(), Some(Tk::S("("))) {
            let save = self.i;
            self.i += 1;
            let mut names = vec![];
            while let Some(n) = self.name() {
                names.push(n);
            }
            if !names.is_empty() && self.eat(":") {
                let a = self.term()?;
                if self.eat(")") && self.eat("->") {
                    let mut b = self.term()?;
                    for n in names.into_iter().rev() {
                        b = S::Pi(n, Box::new(a.clone()), Box::new(b));
                    }
                    return Ok(b);
                }
            }
            self.i = save;
        }
        let a = self.app()?;
        Ok(if self.eat("->") {
            S::Pi("_".into(), Box::new(a), Box::new(self.term()?))
        } else {
            a
        })
    }
    fn starts_atom(&self) -> bool {
        match self.peek() {
            Some(Tk::Num(_)) | Some(Tk::S("(")) | Some(Tk::S("?")) => true,
            Some(Tk::Id(x)) => x != "in" && x != "let",
            _ => false,
        }
    }
    fn app(&mut self) -> R<S> {
        let mut f = self.atom()?;
        while self.starts_atom() {
            f = S::App(Box::new(f), Box::new(self.atom()?));
        }
        Ok(f)
    }
    fn atom(&mut self) -> R<S> {
        match self.peek().cloned() {
            Some(Tk::Num(n)) => {
                self.i += 1;
                Ok(S::Num(n))
            }
            Some(Tk::S("?")) => {
                self.i += 1;
                Ok(S::Named(
                    self.name().ok_or("expected a hole name after `?`")?,
                ))
            }
            Some(Tk::S("(")) => {
                self.i += 1;
                let e = self.term()?;
                let e = if self.eat(":") {
                    S::Ann(Box::new(e), Box::new(self.term()?))
                } else {
                    e
                };
                self.expect(")")?;
                Ok(e)
            }
            Some(Tk::Id(x)) if x == "Type" => {
                self.i += 1;
                Ok(S::Ty)
            }
            Some(Tk::Id(x)) if x == "_" => {
                self.i += 1;
                Ok(S::Hole)
            }
            Some(Tk::Id(_)) => Ok(S::Var(self.name().ok_or("name expected")?)),
            t => Err(format!("unexpected {t:?}")),
        }
    }
}
fn parse(s: &str) -> R<S> {
    let mut p = Parser { t: lex(s)?, i: 0 };
    let e = p.term()?;
    if p.i != p.t.len() {
        return Err(format!("trailing input at token {}", p.i));
    }
    Ok(e)
}

// ------------------------------------------------------------------ elaboration
#[derive(Clone)]
struct Cx {
    env: Vec<Val>,
    tys: Vec<Val>,
    names: Vec<String>,
}
impl Cx {
    fn empty() -> Cx {
        Cx {
            env: vec![],
            tys: vec![],
            names: vec![],
        }
    }
    fn lvl(&self) -> usize {
        self.env.len()
    }
    fn bind(&self, name: &str, ty: Val) -> Cx {
        let mut c = self.clone();
        c.env.push(var(self.lvl()));
        c.tys.push(ty);
        c.names.push(name.to_string());
        c
    }
    /// a `let`/definition: the variable's VALUE is known, so the body may compute with it
    fn define(&self, name: &str, ty: Val, val: Val) -> Cx {
        let mut c = self.clone();
        c.env.push(val);
        c.tys.push(ty);
        c.names.push(name.to_string());
        c
    }
    fn show(&self, v: &Val) -> String {
        show_tm(&mut self.names.clone(), &quote(self.lvl(), v), 0)
    }
    fn context(&self) -> String {
        if self.names.is_empty() {
            return "(empty)".into();
        }
        self.names
            .iter()
            .zip(&self.tys)
            .map(|(n, t)| {
                format!(
                    "{n} : {}",
                    show_tm(&mut self.names.clone(), &quote(self.lvl(), t), 0)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}
fn prim_type(p: &'static str) -> Val {
    if let Some(v) = PRIM_TY_CACHE.with(|c| c.borrow().get(p).cloned()) {
        return v;
    }
    let src = PRIM_TYPES.iter().find(|(n, _)| *n == p).unwrap().1;
    let s = parse(src).expect("primitive type parses");
    let tm = check_ty(&Cx::empty(), &s)
        .unwrap_or_else(|e| panic!("primitive type of {p} is ill-formed: {e}"));
    let v = eval(&[], &tm);
    PRIM_TY_CACHE.with(|c| c.borrow_mut().insert(p, v.clone()));
    v
}
fn check_ty(cx: &Cx, s: &S) -> R<Tm> {
    check(cx, s, &Val::Ty)
}
fn check(cx: &Cx, s: &S, ty: &Val) -> R<Tm> {
    let ty = force(ty);
    match (s, &ty) {
        (S::Lam(x, b), Val::Pi(_, dom, clo)) => {
            let body_ty = clo.apply(var(cx.lvl()));
            let b2 = check(&cx.bind(x, (**dom).clone()), b, &body_ty)?;
            Ok(Tm::Lam(x.clone(), Rc::new(b2)))
        }
        (S::Let(x, t, d, b), _) => {
            let t2 = check_ty(cx, t)?;
            let tv = eval(&cx.env, &t2);
            let d2 = check(cx, d, &tv)?;
            let dv = eval(&cx.env, &d2);
            let b2 = check(&cx.define(x, tv, dv), b, &ty)?;
            Ok(Tm::Let(x.clone(), Box::new(d2), Rc::new(b2)))
        }
        (S::Hole, _) => Ok(Tm::Meta(new_meta(cx.lvl()))),
        (S::Named(h), _) => Err(format!(
            "hole ?{h} : {}\n    in context: {}",
            cx.show(&ty),
            cx.context()
        )),
        _ => {
            let (t, got) = infer(cx, s)?;
            unify(cx.lvl(), &got, &ty).map_err(|e| {
                format!(
                    "type mismatch: expected `{}` but the term has type `{}` ({e})",
                    cx.show(&ty),
                    cx.show(&got)
                )
            })?;
            Ok(t)
        }
    }
}
fn infer(cx: &Cx, s: &S) -> R<(Tm, Val)> {
    match s {
        S::Var(x) => {
            if let Some(pos) = cx.names.iter().rposition(|n| n == x) {
                return Ok((Tm::V(cx.lvl() - 1 - pos), cx.tys[pos].clone()));
            }
            match PRIM_TYPES.iter().find(|(n, _)| n == x) {
                Some((p, _)) => Ok((Tm::Prim(p), prim_type(p))),
                None => Err(format!("unbound variable `{x}`")),
            }
        }
        S::Ty => Ok((Tm::Ty, Val::Ty)),
        S::Num(n) => Ok((nat_lit(*n), Val::Prim("Nat", vec![]))),
        S::Pi(x, a, b) => {
            let a2 = check_ty(cx, a)?;
            let av = eval(&cx.env, &a2);
            let b2 = check_ty(&cx.bind(x, av), b)?;
            Ok((Tm::Pi(x.clone(), Box::new(a2), Rc::new(b2)), Val::Ty))
        }
        S::App(f, a) => {
            let (ft, fty) = infer(cx, f)?;
            match force(&fty) {
                Val::Pi(_, dom, clo) => {
                    let at = check(cx, a, &dom)?;
                    let av = eval(&cx.env, &at);
                    Ok((app_tm(ft, at), clo.apply(av)))
                }
                other => Err(format!(
                    "expected a function but this has type `{}`",
                    cx.show(&other)
                )),
            }
        }
        S::Ann(e, t) => {
            let t2 = check_ty(cx, t)?;
            let tv = eval(&cx.env, &t2);
            let e2 = check(cx, e, &tv)?;
            Ok((e2, tv))
        }
        S::Let(x, t, d, b) => {
            let t2 = check_ty(cx, t)?;
            let tv = eval(&cx.env, &t2);
            let d2 = check(cx, d, &tv)?;
            let dv = eval(&cx.env, &d2);
            let (b2, bty) = infer(&cx.define(x, tv, dv), b)?;
            // `bty` may be used outside the `let` as it is: it is a VALUE, and `define` already put the let variable's value into it
            Ok((Tm::Let(x.clone(), Box::new(d2), Rc::new(b2)), bty))
        }
        S::Hole => {
            let a = new_meta(cx.lvl());
            let t = new_meta(cx.lvl());
            Ok((Tm::Meta(t), Val::Neu(Head::Meta(a), vec![])))
        }
        S::Lam(..) => Err(
            "cannot infer the type of a lambda; add an annotation like `(\\x. e : A -> B)`".into(),
        ),
        S::Named(h) => Err(format!("hole ?{h}: its type is unknown here; annotate it")),
    }
}
/// Definitional equality, solving metavariables on the way. Functions are compared by eta.
fn unify(lvl: usize, a: &Val, b: &Val) -> R<()> {
    let (a, b) = (force(a), force(b));
    match (&a, &b) {
        (Val::Neu(Head::Meta(m), sp), other) if sp.is_empty() => solve(lvl, *m, other),
        (other, Val::Neu(Head::Meta(m), sp)) if sp.is_empty() => solve(lvl, *m, other),
        (Val::Lam(_, c1), Val::Lam(_, c2)) => {
            unify(lvl + 1, &c1.apply(var(lvl)), &c2.apply(var(lvl)))
        }
        (Val::Lam(_, c), other) | (other, Val::Lam(_, c)) => {
            unify(lvl + 1, &c.apply(var(lvl)), &app(other.clone(), var(lvl)))
        }
        (Val::Pi(_, a1, c1), Val::Pi(_, a2, c2)) => {
            unify(lvl, a1, a2)?;
            unify(lvl + 1, &c1.apply(var(lvl)), &c2.apply(var(lvl)))
        }
        (Val::Ty, Val::Ty) => Ok(()),
        (Val::Prim(p, xs), Val::Prim(q, ys)) if p == q && xs.len() == ys.len() => {
            xs.iter().zip(ys).try_for_each(|(x, y)| unify(lvl, x, y))
        }
        (Val::Neu(h1, s1), Val::Neu(h2, s2)) if s1.len() == s2.len() => {
            match (h1, h2) {
                (Head::Lvl(x), Head::Lvl(y)) if x == y => {}
                (Head::Stuck(p, xs), Head::Stuck(q, ys)) if p == q => {
                    xs.iter().zip(ys).try_for_each(|(x, y)| unify(lvl, x, y))?
                }
                (Head::Meta(x), Head::Meta(y)) if x == y => {}
                _ => return Err("different heads".into()),
            }
            s1.iter().zip(s2).try_for_each(|(x, y)| unify(lvl, x, y))
        }
        (Val::Neu(Head::Meta(_), _), _) | (_, Val::Neu(Head::Meta(_), _)) => Err(
            "unsupported: a metavariable applied to arguments (needs pattern unification)".into(),
        ),
        _ => Err("the two values are not convertible".into()),
    }
}
fn scope_check(t: &Tm, depth: usize, now: usize, m: usize, m_lvl: usize) -> R<()> {
    match t {
        Tm::V(i) if *i >= depth => {
            let level = now + depth - 1 - i;
            if level < m_lvl {
                Ok(())
            } else {
                Err("scope check failed: the solution mentions a variable that is not in scope where the hole was created".into())
            }
        }
        Tm::Meta(m2) => {
            if *m2 == m {
                Err("occurs check: a hole cannot be solved by a term that contains itself".into())
            } else if meta_lvl(*m2) > m_lvl {
                Err("scope check failed: the solution mentions a hole from a deeper scope".into())
            } else {
                Ok(())
            }
        }
        Tm::Lam(_, b) => scope_check(b, depth + 1, now, m, m_lvl),
        Tm::App(f, a) => {
            scope_check(f, depth, now, m, m_lvl).and_then(|_| scope_check(a, depth, now, m, m_lvl))
        }
        Tm::Pi(_, a, b) => scope_check(a, depth, now, m, m_lvl)
            .and_then(|_| scope_check(b, depth + 1, now, m, m_lvl)),
        Tm::Let(_, d, b) => scope_check(d, depth, now, m, m_lvl)
            .and_then(|_| scope_check(b, depth + 1, now, m, m_lvl)),
        _ => Ok(()),
    }
}
fn solve(lvl: usize, m: usize, v: &Val) -> R<()> {
    if let Val::Neu(Head::Meta(m2), sp) = v {
        if *m2 == m && sp.is_empty() {
            return Ok(());
        }
    }
    scope_check(&quote(lvl, v), 0, lvl, m, meta_lvl(m))?;
    METAS.with(|ms| ms.borrow_mut()[m].sol = Some(v.clone()));
    Ok(())
}
/// Replace solved metas by their solutions; an unsolved meta is an error.
fn zonk(lvl: usize, t: &Tm) -> R<Tm> {
    Ok(match t {
        Tm::Meta(m) => match meta_sol(*m) {
            Some(v) => zonk(lvl, &quote(lvl, &v))?,
            None => {
                return Err("unsolved hole `_`: add an explicit argument or an annotation".into());
            }
        },
        Tm::Lam(n, b) => Tm::Lam(n.clone(), Rc::new(zonk(lvl + 1, b)?)),
        Tm::App(f, a) => app_tm(zonk(lvl, f)?, zonk(lvl, a)?),
        Tm::Pi(n, a, b) => Tm::Pi(
            n.clone(),
            Box::new(zonk(lvl, a)?),
            Rc::new(zonk(lvl + 1, b)?),
        ),
        Tm::Let(n, d, b) => Tm::Let(
            n.clone(),
            Box::new(zonk(lvl, d)?),
            Rc::new(zonk(lvl + 1, b)?),
        ),
        _ => t.clone(),
    })
}

/// A growing global context of definitions. Each definition is checked against its declared type; its VALUE stays visible, so later
/// definitions (and types!) compute through it.
struct Program {
    top: Cx,
}
impl Program {
    fn new() -> Self {
        Program { top: Cx::empty() }
    }
    fn define(&mut self, name: &str, ty_src: &str, tm_src: &str) -> R<()> {
        let ty = zonk(self.top.lvl(), &check_ty(&self.top, &parse(ty_src)?)?)?;
        let tv = eval(&self.top.env, &ty);
        let tm = zonk(self.top.lvl(), &check(&self.top, &parse(tm_src)?, &tv)?)?;
        let val = eval(&self.top.env, &tm);
        self.top = self.top.define(name, tv, val);
        Ok(())
    }
    fn normalise(&self, src: &str) -> R<(String, String)> {
        let (tm, ty) = infer(&self.top, &parse(src)?)?;
        let tm = zonk(self.top.lvl(), &tm)?;
        let nf = quote(self.top.lvl(), &eval(&self.top.env, &tm));
        Ok((
            show_tm(&mut self.top.names.clone(), &nf, 0),
            self.top.show(&ty),
        ))
    }
}

// ------------------------------------------------------------------ the proofs (written as terms)
const DEFS: &[(&str, &str, &str)] = &[
    (
        "plus",
        "Nat -> Nat -> Nat",
        r"\m n. natrec (\_. Nat) n (\k r. suc r) m",
    ),
    (
        "cong",
        "(A B : Type) -> (f : A -> B) -> (x y : A) -> Id A x y -> Id B (f x) (f y)",
        r"\A B f x y p. J A x (\y' _. Id B (f x) (f y')) (refl B (f x)) y p",
    ),
    (
        "sym",
        "(A : Type) -> (x y : A) -> Id A x y -> Id A y x",
        r"\A x y p. J A x (\y' _. Id A y' x) (refl A x) y p",
    ),
    (
        "trans",
        "(A : Type) -> (x y z : A) -> Id A x y -> Id A y z -> Id A x z",
        r"\A x y z p q. J A y (\z' _. Id A x z') p z q",
    ),
    (
        "transport",
        "(A : Type) -> (P : A -> Type) -> (x y : A) -> Id A x y -> P x -> P y",
        r"\A P x y p px. J A x (\y' _. P y') px y p",
    ),
    ("three_plus_four", "Id Nat (plus 3 4) 7", "refl Nat 7"),
    (
        "append",
        "(A : Type) -> (m n : Nat) -> Vec A m -> Vec A n -> Vec A (plus m n)",
        r"\A m n xs ys. vecrec A (\m' _. Vec A (plus m' n)) ys (\k x xs' r. cons A (plus k n) x r) m xs",
    ),
    (
        "vmap",
        "(A B : Type) -> (A -> B) -> (n : Nat) -> Vec A n -> Vec B n",
        r"\A B f n v. vecrec A (\n' _. Vec B n') (nil B) (\k x xs r. cons B k (f x) r) n v",
    ),
    (
        "v3",
        "Vec Nat 3",
        "cons Nat _ 7 (cons Nat _ 8 (cons Nat _ 9 (nil Nat)))",
    ),
    ("v6", "Vec Nat 6", "append Nat 3 3 v3 v3"),
    ("v6_inc", "Vec Nat 6", "vmap Nat Nat suc 6 v6"),
    (
        "plus_zero_r",
        "(n : Nat) -> Id Nat (plus n zero) n",
        r"\n. natrec (\n'. Id Nat (plus n' zero) n') (refl Nat zero) (\k ih. cong Nat Nat suc _ _ ih) n",
    ),
    (
        "plus_suc_r",
        "(a b : Nat) -> Id Nat (plus a (suc b)) (suc (plus a b))",
        r"\a b. natrec (\a'. Id Nat (plus a' (suc b)) (suc (plus a' b))) (refl Nat (suc b)) (\k ih. cong Nat Nat suc _ _ ih) a",
    ),
    (
        "plus_assoc",
        "(a b c : Nat) -> Id Nat (plus (plus a b) c) (plus a (plus b c))",
        r"\a b c. natrec (\a'. Id Nat (plus (plus a' b) c) (plus a' (plus b c))) (refl Nat (plus b c)) (\k ih. cong Nat Nat suc _ _ ih) a",
    ),
    (
        "plus_comm",
        "(a b : Nat) -> Id Nat (plus a b) (plus b a)",
        r"\a b. natrec (\a'. Id Nat (plus a' b) (plus b a')) (sym Nat _ _ (plus_zero_r b)) (\k ih. trans Nat (suc (plus k b)) (suc (plus b k)) (plus b (suc k)) (cong Nat Nat suc _ _ ih) (sym Nat _ _ (plus_suc_r b k))) a",
    ),
    ("comm_3_4", "Id Nat (plus 3 4) (plus 4 3)", "plus_comm 3 4"),
    (
        "vec_len_rewrite",
        "Vec Nat (plus 2 3) -> Vec Nat (plus 3 2)",
        r"\v. transport Nat (\n. Vec Nat n) (plus 2 3) (plus 3 2) (plus_comm 2 3) v",
    ),
    ("type_in_type", "Type", "Type"),
];
/// (program name, source type, source term, expected message fragment); each is checked after the good prelude.
const WRONG: &[(&str, &str, &str)] = &[
    ("Id Nat (plus 1 1) 3", "refl Nat 3", "type mismatch"),
    (
        "(n : Nat) -> Id Nat (plus n zero) n",
        r"\n. refl Nat n",
        "type mismatch",
    ),
    (
        "(a b : Nat) -> Id Nat (plus a b) (plus b a)",
        r"\a b. refl Nat (plus a b)",
        "type mismatch",
    ),
    ("Vec Nat 2", "cons Nat _ 1 (nil Nat)", "type mismatch"),
    ("Vec Nat 1", "cons Nat 1 1 (nil Nat)", "type mismatch"),
    ("Nat", "suc (nil Nat)", "type mismatch"),
    ("Nat", "suc zero zero", "expected a function"),
    ("Nat", "undefined_name", "unbound variable"),
    ("Nat -> Nat", r"\x. y", "unbound variable"),
    ("Nat", "_", "unsolved hole"),
    ("Nat", "?goal", "hole ?goal : Nat"),
    ("Type", "Id Nat zero Nat", "type mismatch"),
    (
        "(x y : Nat) -> Id Nat x y -> Id Nat y x",
        r"\x y p. p",
        "type mismatch",
    ),
    (
        "Nat -> Nat",
        r"natrec (\_. Nat) zero (\k r. r r)",
        "expected a function",
    ),
    (
        "(x y z : Nat) -> Id Nat x y -> Id Nat y z -> Id Nat x y",
        r"\x y z p q. q",
        "type mismatch",
    ),
    ("Nat -> Nat", r"\n. nil Nat", "type mismatch"),
    (
        "(A : Type) -> (n : Nat) -> Vec A n -> Vec A (suc n)",
        r"\A n v. v",
        "type mismatch",
    ),
    (
        "(m n : Nat) -> Vec Nat m -> Vec Nat n -> Vec Nat (plus n m)",
        r"\m n xs ys. append Nat m n xs ys",
        "type mismatch",
    ),
    ("Nat", r"(\x. x)", "cannot infer the type of a lambda"),
    (
        "Nat -> Nat",
        r"\n. natrec (\_. Nat) zero (\k r. n) 3 zero",
        "expected a function",
    ),
];

fn show_program(p: &mut Program) {
    for (name, ty, tm) in DEFS {
        p.define(name, ty, tm)
            .unwrap_or_else(|e| panic!("definition `{name}` should check: {e}"));
    }
    println!(
        "-- {} definitions accepted by the kernel (plus, cong, sym, trans, transport, append, vmap, plus_zero_r, plus_assoc, plus_comm, ...)",
        DEFS.len()
    );
    for src in [
        "plus 2 3",
        "v6_inc",
        "append Nat 1 2 (cons Nat _ 5 (nil Nat)) (cons Nat _ 6 (cons Nat _ 7 (nil Nat)))",
        "plus_assoc 1 2 3",
        "plus_comm 2 1",
    ] {
        let (nf, ty) = p.normalise(src).unwrap();
        println!("   {src}\n      = {nf}\n      : {ty}");
    }
    assert_eq!(p.normalise("plus 2 3").unwrap().0, "5");
    // computation inside types: Vec of `plus 2 3` and Vec of 5 are the same type
    p.define("same_type", "Vec Nat (plus 2 3) -> Vec Nat 5", r"\v. v")
        .unwrap();
    // a proof of an open statement is NOT a proof by computation: plus n zero does not reduce for variable n, and the kernel knows it
    assert!(
        p.define(
            "bad",
            "(n : Nat) -> Id Nat (plus n zero) n",
            r"\n. refl Nat n"
        )
        .is_err()
    );
    // the failed definition leaves the program unchanged
    assert!(p.normalise("bad").is_err());
}

fn test_wrong(p: &Program) {
    println!("-- {} deliberately wrong programs rejected", WRONG.len());
    for (ty, tm, frag) in WRONG {
        let mut q = Program { top: p.top.clone() };
        let err = match q.define("wrong", ty, tm) {
            Ok(()) => panic!("`{tm}` : `{ty}` must be rejected"),
            Err(e) => e,
        };
        assert!(
            err.contains(frag),
            "`{tm}`: expected a message containing `{frag}`, got `{err}`"
        );
        println!(
            "   {tm:<56} -> {}",
            err.lines()
                .next()
                .unwrap()
                .chars()
                .take(100)
                .collect::<String>()
        );
    }
    // a typed hole reports its goal and the context
    let mut q = Program { top: p.top.clone() };
    let e = q
        .define("h", "(n : Nat) -> Id Nat (plus zero n) n", r"\n. ?proof")
        .unwrap_err();
    assert!(
        e.contains("hole ?proof : Id Nat n n") && e.contains("n : Nat"),
        "{e}"
    );
    println!("   typed hole: {}", e.replace('\n', " |"));
}

// ------------------------------------------------------------------ random System T terms: NbE vs naive substitution
#[derive(Clone, Debug)]
enum Sty {
    N,
    F(Box<Sty>, Box<Sty>),
}
fn sty_tm(t: &Sty) -> Tm {
    match t {
        Sty::N => prim("Nat"),
        Sty::F(a, b) => Tm::Pi("_".into(), Box::new(sty_tm(a)), Rc::new(sty_tm(b))),
    }
}
fn same(a: &Sty, b: &Sty) -> bool {
    match (a, b) {
        (Sty::N, Sty::N) => true,
        (Sty::F(a1, a2), Sty::F(b1, b2)) => same(a1, b1) && same(a2, b2),
        _ => false,
    }
}
fn nat_fn() -> Sty {
    Sty::F(Box::new(Sty::N), Box::new(Sty::N))
}
fn gen_sty(r: &mut Rng, d: usize) -> Sty {
    if d == 0 || r.below(3) > 0 {
        Sty::N
    } else {
        Sty::F(Box::new(gen_sty(r, d - 1)), Box::new(gen_sty(r, d - 1)))
    }
}
/// A random well-typed System T term of type `ty` in context `ctx` (oldest first); with optional convertible-but-different rewrites.
fn gen_tm(r: &mut Rng, ty: &Sty, ctx: &[Sty], d: usize, mutate: bool) -> Tm {
    let vars: Vec<usize> = (0..ctx.len()).filter(|&p| same(&ctx[p], ty)).collect();
    let var_tm = |p: usize| Tm::V(ctx.len() - 1 - p);
    let mut t = if d == 0 {
        if !vars.is_empty() && r.below(2) == 0 {
            var_tm(vars[r.below(vars.len())])
        } else {
            match ty {
                Sty::N => nat_lit(r.below(3)),
                Sty::F(a, b) => {
                    let mut c2 = ctx.to_vec();
                    c2.push((**a).clone());
                    lam_tm("x", gen_tm(r, b, &c2, 0, mutate))
                }
            }
        }
    } else {
        match r.below(7) {
            0 if !vars.is_empty() => var_tm(vars[r.below(vars.len())]),
            1 if matches!(ty, Sty::N) => app_tm(prim("suc"), gen_tm(r, ty, ctx, d - 1, mutate)),
            2 => {
                // application of a function-typed term
                let a = gen_sty(r, 1);
                let f = Sty::F(Box::new(a.clone()), Box::new(ty.clone()));
                app_tm(
                    gen_tm(r, &f, ctx, d - 1, mutate),
                    gen_tm(r, &a, ctx, d - 1, mutate),
                )
            }
            3 | 4 => {
                // natrec at type `ty`: motive is constant, step gets the predecessor and the recursive result
                let mut c2 = ctx.to_vec();
                c2.push(Sty::N);
                c2.push(ty.clone());
                let step = lam_tm("k", lam_tm("r", gen_tm(r, ty, &c2, d - 1, mutate)));
                apps(
                    prim("natrec"),
                    vec![
                        lam_tm("_", sty_tm(ty)),
                        gen_tm(r, ty, ctx, d - 1, mutate),
                        step,
                        gen_tm(r, &Sty::N, ctx, d - 1, mutate),
                    ],
                )
            }
            _ => match ty {
                Sty::N => nat_lit(r.below(4)),
                Sty::F(a, b) => {
                    let mut c2 = ctx.to_vec();
                    c2.push((**a).clone());
                    lam_tm("x", gen_tm(r, b, &c2, d - 1, mutate))
                }
            },
        }
    };
    if mutate {
        t = match r.below(6) {
            // beta-expansion, let-expansion, natrec on zero, identity application: all CONVERTIBLE with `t`
            0 => app_tm(lam_tm("w", shift(&t, 0, 1)), nat_lit(r.below(2))),
            1 => Tm::Let("l".into(), Box::new(nat_lit(1)), Rc::new(shift(&t, 0, 1))),
            2 => apps(
                prim("natrec"),
                vec![
                    lam_tm("_", prim("Nat")),
                    t,
                    lam_tm("k", lam_tm("r", Tm::V(0))),
                    nat_lit(0),
                ],
            ),
            3 => app_tm(lam_tm("i", Tm::V(0)), t),
            4 if matches!(ty, Sty::F(..)) => lam_tm("e", app_tm(shift(&t, 0, 1), Tm::V(0))), // eta-expansion
            _ => t,
        };
    }
    t
}
fn test_nbe_vs_naive(r: &mut Rng) {
    let mut sizes = 0;
    for i in 0..1000 {
        let ctx = [Sty::N, nat_fn()];
        let ty = if i % 4 == 0 { nat_fn() } else { Sty::N };
        let t = gen_tm(r, &ty, &ctx, 3 + i % 3, i % 2 == 0);
        let want = strip(&nf_naive(&t));
        let got = strip(&nbe_nf(ctx.len(), &t));
        assert_eq!(
            got,
            want,
            "NbE and substitution disagree on {}",
            show_tm(&mut vec!["n".into(), "f".into()], &t, 0)
        );
        sizes += 1;
    }
    println!(
        "-- NbE vs naive substitution normaliser: {sizes} random System T terms (open, with neutrals) give identical normal forms"
    );
}

fn test_conversion(r: &mut Rng) {
    let ctx = [Sty::N, nat_fn()];
    let env: Vec<Val> = (0..ctx.len()).map(var).collect();
    let (mut eq_pairs, mut eta_only, mut chains) = (0, 0, 0);
    for i in 0..600 {
        let ty = if i % 3 == 0 { nat_fn() } else { Sty::N };
        let base = gen_tm(r, &ty, &ctx, 3, false);
        // a chain a ~ b ~ c of convertible rewrites, plus an independent term d
        let a = gen_tm_from(r, &base, &ty);
        let b = gen_tm_from(r, &a, &ty);
        let c = gen_tm_from(r, &b, &ty);
        let d = gen_tm(r, &ty, &ctx, 3, false);
        let conv = |x: &Tm, y: &Tm| unify(ctx.len(), &eval(&env, x), &eval(&env, y)).is_ok();
        for (x, y) in [(&a, &b), (&b, &c), (&a, &c), (&base, &c)] {
            assert!(
                conv(x, y) && conv(y, x),
                "convertible rewrites must be convertible (symmetric)"
            );
        }
        chains += 1;
        // transitivity and symmetry against the independent term, with the beta-eta normal form as ground truth
        let beta_eta = |x: &Tm| strip(&eta_short(&nf_naive(x)));
        let truth = |x: &Tm, y: &Tm| beta_eta(x) == beta_eta(y);
        for x in [&a, &b, &c] {
            assert_eq!(
                beta_eta(x),
                beta_eta(&base),
                "the oracle agrees that every rewrite preserves the beta-eta normal form"
            );
            eta_only += usize::from(strip(&nf_naive(x)) != strip(&nf_naive(&base)));
        }
        assert_eq!(conv(&a, &d), truth(&a, &d));
        assert_eq!(conv(&d, &a), conv(&a, &d), "symmetry");
        if conv(&a, &d) {
            eq_pairs += 1;
            assert!(conv(&c, &d), "transitivity: a ~ c and a ~ d imply c ~ d");
        }
    }
    assert!(
        eta_only > 0,
        "the sample must contain pairs that only the eta rule identifies, or eta is not being tested"
    );
    println!(
        "-- conversion: {chains} chains a~b~c checked symmetric/transitive; {eta_only} rewrites differ from their base up to eta only;"
    );
    println!(
        "   {eq_pairs} coincidentally equal independent pairs; conversion agrees with beta-eta normal forms on every independent pair"
    );
}
/// Rewrite `t` into a different but convertible term (same mutation family as in `gen_tm`).
fn gen_tm_from(r: &mut Rng, t: &Tm, ty: &Sty) -> Tm {
    match r.below(5) {
        0 => app_tm(lam_tm("w", shift(t, 0, 1)), nat_lit(r.below(3))),
        1 => Tm::Let("l".into(), Box::new(nat_lit(2)), Rc::new(shift(t, 0, 1))),
        2 => apps(
            prim("natrec"),
            vec![
                lam_tm("_", prim("Nat")),
                t.clone(),
                lam_tm("k", lam_tm("r", Tm::V(0))),
                nat_lit(0),
            ],
        ),
        3 if matches!(ty, Sty::F(..)) => lam_tm("e", app_tm(shift(t, 0, 1), Tm::V(0))),
        _ => app_tm(lam_tm("i", Tm::V(0)), t.clone()),
    }
}

fn test_holes_and_scope() {
    let mut q = Program::new();
    q.define("id", "(A : Type) -> A -> A", r"\A x. x").unwrap();
    q.define("ok", "Nat", "id _ 3").unwrap();
    assert_eq!(q.normalise("ok").unwrap().0, "3");
    // metas solved by unification inside a nested application
    q.define("pair_len", "Vec Nat 2", "cons _ _ 1 (cons _ _ 2 (nil _))")
        .unwrap();
    let mut rejected = |ty: &str, tm: &str, why: &str| {
        let err = q.define("wrong_hole", ty, tm).expect_err(tm);
        assert!(err.contains(why), "`{tm}`: expected `{why}`, got `{err}`");
    };
    // nothing constrains the second hole
    rejected("Nat", "id _ _", "unsolved hole");
    // occurs check: `refl Type _ : Id Type h h` forces h = ?X and then ?X = ?X -> Nat
    rejected(
        "((X : Type) -> Id Type X (X -> Nat) -> Nat) -> Nat",
        r"\k. k _ (refl Type _)",
        "occurs check",
    );
    // scope check: ?X is created OUTSIDE `\n`, so it cannot be solved by `Vec Nat n`
    rejected(
        "((X : Type) -> ((n : Nat) -> Id Type X (Vec Nat n)) -> Nat) -> Nat",
        r"\k. k _ (\n. refl Type _)",
        "scope check failed",
    );
    // eta for functions is part of definitional equality
    q.define(
        "eta",
        "Id (Nat -> Nat) suc (\\n. suc n)",
        r"refl (Nat -> Nat) suc",
    )
    .unwrap();
    println!(
        "-- holes: `_` solved by unification (id _ 3, cons _ _ 1 ..); unsolved, occurs-check and scope-escape holes rejected; eta accepted"
    );
}

fn main() {
    std::thread::Builder::new()
        .stack_size(128 << 20)
        .spawn(|| {
            let mut r = Rng(0xD1B5_4A32_D192_ED03);
            let mut p = Program::new();
            show_program(&mut p);
            test_wrong(&p);
            test_holes_and_scope();
            test_nbe_vs_naive(&mut r);
            test_conversion(&mut r);
            println!("all dependent type checks passed");
        })
        .unwrap()
        .join()
        .unwrap();
}
