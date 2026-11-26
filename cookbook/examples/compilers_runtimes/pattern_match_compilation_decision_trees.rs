//! RECIPE 815 (compilers): compiling pattern matching: decision trees, backtracking automata, sharing, exhaustiveness and redundancy.
//! LEVEL: intermediate
//! Use in: any compiler or interpreter with `match` on algebraic data, and rule/dispatch tables that must be complete and free of dead rules.   Run: cargo run --release --example pattern_match_compilation_decision_trees
//! COMMON USES: match compilation in ML-family compilers; OCaml's compiler (backtracking automata) and its exhaustiveness/unused-case warnings;
//! rustc, which checks exhaustiveness and usefulness on patterns (crate rustc_pattern_analysis) and lowers `match` to MIR branches.
//! Course: Maranget, "Compiling Pattern Matching to Good Decision Trees" and "Warnings for pattern matching" (the algorithms used here).
#![allow(dead_code)]
//!
//! THE PROBLEM. `match v { p1 if g1 => e1, p2 => e2, ... }` means "the FIRST arm whose pattern matches and whose guard is true". Running that
//! definition literally (`naive_match` below, our oracle) re-inspects the same parts of `v` once per arm. A compiler wants to inspect each part
//! as few times as possible.
//!
//! THE MATRIX. Rows are arms, columns are the places still to inspect (OCCURRENCES, i.e. paths into the scrutinee). Two operations drive
//! everything:
//!   specialise S(c, M): "the first column is known to be constructor c": keep rows that start with c (replace the cell by c's argument
//!     patterns) or with a wildcard (replace by arity(c) wildcards); drop the rest.
//!   default D(M): "it is none of the constructors in the column": keep only wildcard rows and drop the column.
//! DECISION TREE: pick a column in which the first row needs a test, switch on it, specialise per constructor, default for the others, recurse.
//! No place is tested twice on any path (asserted below), but rows can be copied into several branches: size can grow exponentially.
//! WHICH COLUMN matters: heuristics below are first-column, necessity (the column that most rows actually need) and small branching factor.
//! BACKTRACKING AUTOMATON: take the longest prefix of rows that all need a test in the chosen column, switch on it, and if that fails fall
//! through to the compiled remaining rows. Rows are never copied between groups (size stays small), but a place may be tested again.
//! SHARING: hash-consing identical subtrees turns the tree into a DAG, which removes much of the duplication (the DAG is what you would emit
//! as basic blocks with jumps).
//! USEFULNESS. U(P, q) asks: is there a value matched by q and by no row of P? It uses the same two operations. It gives BOTH checks:
//! the match is exhaustive iff U(all rows, _) is false (otherwise it returns a counterexample pattern), and arm i is redundant iff
//! U(rows above i, p_i) is false. A guarded arm covers nothing (the guard may be false), which is exactly what rustc does.
//! RANGES. Integer columns have a huge "constructor set". Split the domain at every range endpoint into disjoint segments; inside one segment
//! every range pattern in the column behaves the same, so one representative case per segment suffices.
//! OR-PATTERNS are expanded into several rows with the same arm, leftmost alternative most significant. That order matters: with a guard, a
//! failing guard must try the NEXT alternative (Rust does this). Pre-expansion is simple but can be exponential (`(a|b, a|b, ...)`).
//! HOW rustc RELATES: rustc checks usefulness on its own pattern representation, then builds MIR in which `match` becomes `SwitchInt`
//! terminators on discriminants and integers plus comparisons for ranges; see the dev guide chapters linked below. We do not claim rustc uses
//! the exact tree construction in this file.
//!
//! Read: Maranget, decision trees http://moscova.inria.fr/~maranget/papers/ml05e-maranget.pdf and warnings
//! http://moscova.inria.fr/~maranget/papers/warn/index.html ; rustc dev guide https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html
//! and https://rustc-dev-guide.rust-lang.org/mir/construction.html ; https://doc.rust-lang.org/nightly/nightly-rustc/rustc_pattern_analysis/index.html
//! TRY NEXT:
//!   - [S] Make guarded rows count as covering in `unguarded_rows`; predict which of the brute-force checks fails first.
//!   - [M] Add the "arity" tie-break (prefer constructors with fewer arguments) as a fourth heuristic and compare node counts.
//!   - [M] Expand or-patterns lazily during specialisation instead of up front; what must you do to keep the guard order?
//!   - [A] Project: emit the DAG as Rust source (`match` + labelled blocks) and compare its instruction count with rustc's for the same match.
use std::collections::{BTreeSet, HashMap, HashSet};
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
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- types, values, patterns

/// A type is an index into `TYPES`: either a finite integer type or an algebraic data type.
type TyId = usize;
enum TyDef {
    /// an integer type with the inclusive domain lo..=hi
    Int(i64, i64),
    /// constructors: (name, argument types). Constructor 0 is never recursive, so every type has a small value.
    Adt(&'static [(&'static str, &'static [TyId])]),
}
const BOOL: TyId = 0;
const RGB: TyId = 1;
const INT: TyId = 2;
const OPT: TyId = 3;
const LIST: TyId = 4;
const PAIR: TyId = 5;
const TREE: TyId = 6;
const BYTE: TyId = 7;
const TYPES: &[TyDef] = &[
    TyDef::Adt(&[("F", &[]), ("T", &[])]),
    TyDef::Adt(&[("Red", &[]), ("Green", &[]), ("Blue", &[])]),
    TyDef::Int(0, 7),
    TyDef::Adt(&[("None", &[]), ("Some", &[RGB])]),
    TyDef::Adt(&[("Nil", &[]), ("Cons", &[BOOL, LIST])]),
    TyDef::Adt(&[("Pair", &[OPT, INT])]),
    TyDef::Adt(&[("Leaf", &[]), ("Node", &[TREE, BOOL, TREE])]),
    TyDef::Int(0, 255),
];
fn ctors(t: TyId) -> &'static [(&'static str, &'static [TyId])] {
    match &TYPES[t] {
        TyDef::Adt(c) => c,
        TyDef::Int(..) => panic!("type {t} is an integer type"),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    C(usize, Vec<Value>),
    I(i64),
}

#[derive(Clone, Debug, PartialEq)]
enum Pat {
    Wild,
    /// binds the matched value to a variable number
    Bind(usize),
    Ctor(usize, Vec<Pat>),
    /// inclusive; a literal is lo == hi
    Range(i64, i64),
    Or(Vec<Pat>),
}

/// A guard is an arbitrary but deterministic predicate of the arm's bindings.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Guard {
    salt: u64,
}
type Bindings = Vec<(usize, Value)>;
impl Guard {
    fn holds(self, b: &Bindings) -> bool {
        fn mix(h: u64, v: &Value) -> u64 {
            match v {
                Value::I(k) => h.wrapping_mul(31).wrapping_add(*k as u64 + 1),
                Value::C(c, a) => a
                    .iter()
                    .fold(h.wrapping_mul(37).wrapping_add(*c as u64 + 1), mix),
            }
        }
        let mut sorted: Vec<&(usize, Value)> = b.iter().collect();
        sorted.sort_by_key(|x| x.0); // the order in which a matcher found the bindings must not matter
        (sorted.iter().fold(self.salt, |h, (_, v)| mix(h, v)) >> 1) % 3 != 0
    }
}
#[derive(Clone, Debug)]
struct Arm {
    pat: Pat,
    guard: Option<Guard>,
}
type Outcome = Option<(usize, Bindings)>;
fn sorted(o: Outcome) -> Outcome {
    o.map(|(a, mut b)| {
        b.sort_by_key(|x| x.0);
        (a, b)
    })
}

// ---------------------------------------------------------------- or-expansion and the naive oracle

/// All or-free versions of `p`, in left-to-right priority order.
fn expand(p: &Pat) -> Vec<Pat> {
    match p {
        Pat::Or(alts) => alts.iter().flat_map(expand).collect(),
        Pat::Ctor(c, args) => {
            let mut out = vec![vec![]];
            for a in args {
                let choices = expand(a);
                out = out
                    .iter()
                    .flat_map(|pre: &Vec<Pat>| {
                        choices
                            .iter()
                            .map(|ch| [pre.clone(), vec![ch.clone()]].concat())
                    })
                    .collect();
            }
            out.into_iter().map(|args| Pat::Ctor(*c, args)).collect()
        }
        other => vec![other.clone()],
    }
}
/// Match an or-free pattern, collecting bindings.
fn match_simple(p: &Pat, v: &Value, b: &mut Bindings) -> bool {
    match (p, v) {
        (Pat::Wild, _) => true,
        (Pat::Bind(x), _) => {
            b.push((*x, v.clone()));
            true
        }
        (Pat::Ctor(c, ps), Value::C(d, vs)) => {
            c == d && ps.iter().zip(vs).all(|(p, v)| match_simple(p, v, b))
        }
        (Pat::Range(lo, hi), Value::I(k)) => lo <= k && k <= hi,
        _ => panic!("ill-typed pattern {p:?} against {v:?}"),
    }
}
/// The definition of `match`: arms in order, alternatives in order, first one that matches and passes its guard.
fn naive_match(arms: &[Arm], v: &Value) -> Outcome {
    for (i, arm) in arms.iter().enumerate() {
        for alt in expand(&arm.pat) {
            let mut b = vec![];
            if match_simple(&alt, v, &mut b) && arm.guard.is_none_or(|g| g.holds(&b)) {
                return Some((i, b));
            }
        }
    }
    None
}

// ---------------------------------------------------------------- the matrix

/// A path from the scrutinee: [] is the whole value, [1, 0] is argument 0 of argument 1.
type Occ = Vec<usize>;
#[derive(Clone)]
struct Row {
    pats: Vec<Pat>,
    binds: Vec<(usize, Occ)>,
    arm: usize,
    guard: Option<Guard>,
}
#[derive(Clone)]
struct Matrix {
    cols: Vec<(Occ, TyId)>,
    rows: Vec<Row>,
}
fn refutable(p: &Pat) -> bool {
    !matches!(p, Pat::Wild | Pat::Bind(_))
}
impl Matrix {
    fn new(ty: TyId, arms: &[Arm]) -> Matrix {
        let rows = arms
            .iter()
            .enumerate()
            .flat_map(|(i, a)| {
                expand(&a.pat).into_iter().map(move |p| Row {
                    pats: vec![p],
                    binds: vec![],
                    arm: i,
                    guard: a.guard,
                })
            })
            .collect();
        Matrix {
            cols: vec![(vec![], ty)],
            rows,
        }
    }
    /// Remove column `c` from a row, appending `new_pats` at the front; a variable in the removed cell becomes a binding of its occurrence.
    fn rewrite_row(&self, row: &Row, c: usize, new_pats: Vec<Pat>) -> Row {
        let mut r = row.clone();
        if let Pat::Bind(x) = r.pats.remove(c) {
            r.binds.push((x, self.cols[c].0.clone()));
        }
        r.pats.splice(0..0, new_pats);
        r
    }
    fn cols_with(&self, c: usize, new_cols: Vec<(Occ, TyId)>) -> Vec<(Occ, TyId)> {
        let mut cols = self.cols.clone();
        cols.remove(c);
        cols.splice(0..0, new_cols);
        cols
    }
    fn specialise_ctor(&self, c: usize, k: usize) -> Matrix {
        let (occ, ty) = &self.cols[c];
        let arg_tys = ctors(*ty)[k].1;
        let new_cols = arg_tys
            .iter()
            .enumerate()
            .map(|(i, t)| ([occ.clone(), vec![i]].concat(), *t))
            .collect();
        let rows = self
            .rows
            .iter()
            .filter_map(|r| match &r.pats[c] {
                Pat::Ctor(d, args) if *d == k => Some(self.rewrite_row(r, c, args.clone())),
                Pat::Wild | Pat::Bind(_) => {
                    Some(self.rewrite_row(r, c, vec![Pat::Wild; arg_tys.len()]))
                }
                _ => None,
            })
            .collect();
        Matrix {
            cols: self.cols_with(c, new_cols),
            rows,
        }
    }
    /// `seg` must be a segment from `segments`: every range in the column either contains it or misses it.
    fn specialise_int(&self, c: usize, seg: (i64, i64)) -> Matrix {
        let rows = self
            .rows
            .iter()
            .filter(|r| match &r.pats[c] {
                Pat::Range(lo, hi) => *lo <= seg.0 && seg.1 <= *hi,
                Pat::Wild | Pat::Bind(_) => true,
                other => panic!("constructor pattern in an integer column: {other:?}"),
            })
            .map(|r| self.rewrite_row(r, c, vec![]))
            .collect();
        Matrix {
            cols: self.cols_with(c, vec![]),
            rows,
        }
    }
    fn default_matrix(&self, c: usize) -> Matrix {
        let rows = self
            .rows
            .iter()
            .filter(|r| !refutable(&r.pats[c]))
            .map(|r| self.rewrite_row(r, c, vec![]))
            .collect();
        Matrix {
            cols: self.cols_with(c, vec![]),
            rows,
        }
    }
    fn head_ctors(&self, c: usize) -> BTreeSet<usize> {
        self.rows
            .iter()
            .filter_map(|r| {
                if let Pat::Ctor(k, _) = &r.pats[c] {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect()
    }
    fn head_ranges(&self, c: usize) -> Vec<(i64, i64)> {
        self.rows
            .iter()
            .filter_map(|r| {
                if let Pat::Range(a, b) = &r.pats[c] {
                    Some((*a, *b))
                } else {
                    None
                }
            })
            .collect()
    }
}
/// Cut [lo, hi] at every endpoint of `ranges`: returns (segment, is it covered by some range?).
fn segments((lo, hi): (i64, i64), ranges: &[(i64, i64)]) -> Vec<((i64, i64), bool)> {
    let mut cuts = BTreeSet::from([lo, hi + 1]);
    for (a, b) in ranges {
        cuts.extend([*a, *b + 1].into_iter().filter(|x| *x > lo && *x <= hi));
    }
    let cuts: Vec<i64> = cuts.into_iter().collect();
    cuts.windows(2)
        .map(|w| {
            (
                (w[0], w[1] - 1),
                ranges.iter().any(|(a, b)| *a <= w[0] && w[1] - 1 <= *b),
            )
        })
        .collect()
}

// ---------------------------------------------------------------- the compiled form

#[derive(Debug, PartialEq, Eq, Hash)]
enum Tree {
    Fail,
    Leaf {
        arm: usize,
        binds: Vec<(usize, Occ)>,
    },
    Guard {
        arm: usize,
        binds: Vec<(usize, Occ)>,
        salt: u64,
        otherwise: Rc<Tree>,
    },
    Switch {
        occ: Occ,
        cases: Vec<(usize, Rc<Tree>)>,
        default: Option<Rc<Tree>>,
    },
    IntSwitch {
        occ: Occ,
        cases: Vec<((i64, i64), Rc<Tree>)>,
        default: Option<Rc<Tree>>,
    },
    /// backtracking: run the first; if it reaches `Fail`, run the second
    Try(Rc<Tree>, Rc<Tree>),
}
fn lookup<'v>(v: &'v Value, occ: &[usize]) -> &'v Value {
    occ.iter().fold(v, |v, i| match v {
        Value::C(_, args) => &args[*i],
        Value::I(_) => panic!("occurrence goes below an integer"),
    })
}
/// Run a tree; `tests` counts switch and guard nodes executed.
fn run(t: &Tree, v: &Value, tests: &mut usize) -> Outcome {
    let bind = |binds: &[(usize, Occ)]| -> Bindings {
        binds
            .iter()
            .map(|(x, o)| (*x, lookup(v, o).clone()))
            .collect()
    };
    match t {
        Tree::Fail => None,
        Tree::Leaf { arm, binds } => Some((*arm, bind(binds))),
        Tree::Guard {
            arm,
            binds,
            salt,
            otherwise,
        } => {
            *tests += 1;
            let b = bind(binds);
            if (Guard { salt: *salt }).holds(&b) {
                Some((*arm, b))
            } else {
                run(otherwise, v, tests)
            }
        }
        Tree::Switch {
            occ,
            cases,
            default,
        } => {
            *tests += 1;
            let Value::C(k, _) = lookup(v, occ) else {
                panic!("switch on a non-constructor")
            };
            match cases.iter().find(|(c, _)| c == k) {
                Some((_, sub)) => run(sub, v, tests),
                None => default.as_ref().and_then(|d| run(d, v, tests)),
            }
        }
        Tree::IntSwitch {
            occ,
            cases,
            default,
        } => {
            *tests += 1;
            let Value::I(k) = lookup(v, occ) else {
                panic!("integer switch on a constructor")
            };
            match cases.iter().find(|((a, b), _)| a <= k && k <= b) {
                Some((_, sub)) => run(sub, v, tests),
                None => default.as_ref().and_then(|d| run(d, v, tests)),
            }
        }
        Tree::Try(a, b) => run(a, v, tests).or_else(|| run(b, v, tests)),
    }
}
fn children(t: &Tree) -> Vec<&Rc<Tree>> {
    match t {
        Tree::Fail | Tree::Leaf { .. } => vec![],
        Tree::Guard { otherwise, .. } => vec![otherwise],
        Tree::Switch { cases, default, .. } => cases.iter().map(|c| &c.1).chain(default).collect(),
        Tree::IntSwitch { cases, default, .. } => {
            cases.iter().map(|c| &c.1).chain(default).collect()
        }
        Tree::Try(a, b) => vec![a, b],
    }
}
fn tree_size(t: &Tree) -> usize {
    1 + children(t).iter().map(|c| tree_size(c)).sum::<usize>()
}
/// Number of distinct subtrees = number of nodes after sharing. `Rc<Tree>` hashes and compares by VALUE, so equal subtrees collapse.
fn dag_size<'t>(t: &'t Tree, seen: &mut HashSet<&'t Tree>) -> usize {
    if seen.insert(t) {
        for c in children(t) {
            dag_size(c, seen);
        }
    }
    seen.len()
}
/// Hash-cons: rebuild the tree so that equal subtrees are the same allocation.
fn share(t: &Rc<Tree>, pool: &mut HashMap<Rc<Tree>, Rc<Tree>>) -> Rc<Tree> {
    if let Some(s) = pool.get(t) {
        return s.clone();
    }
    let mut go = |c: &Rc<Tree>| share(c, pool);
    let rebuilt = Rc::new(match &**t {
        Tree::Fail => Tree::Fail,
        Tree::Leaf { arm, binds } => Tree::Leaf {
            arm: *arm,
            binds: binds.clone(),
        },
        Tree::Guard {
            arm,
            binds,
            salt,
            otherwise,
        } => Tree::Guard {
            arm: *arm,
            binds: binds.clone(),
            salt: *salt,
            otherwise: go(otherwise),
        },
        Tree::Switch {
            occ,
            cases,
            default,
        } => Tree::Switch {
            occ: occ.clone(),
            cases: cases.iter().map(|(k, c)| (*k, go(c))).collect(),
            default: default.as_ref().map(&mut go),
        },
        Tree::IntSwitch {
            occ,
            cases,
            default,
        } => Tree::IntSwitch {
            occ: occ.clone(),
            cases: cases.iter().map(|(k, c)| (*k, go(c))).collect(),
            default: default.as_ref().map(&mut go),
        },
        Tree::Try(a, b) => Tree::Try(go(a), go(b)),
    });
    pool.insert(rebuilt.clone(), rebuilt.clone());
    rebuilt
}
/// On every root-to-leaf path of a decision tree, no occurrence is switched on twice.
fn never_retests(t: &Tree, tested: &mut Vec<Occ>) -> bool {
    match t {
        Tree::Switch { occ, .. } | Tree::IntSwitch { occ, .. } => {
            if tested.contains(occ) {
                return false;
            }
            tested.push(occ.clone());
            let ok = children(t).iter().all(|c| never_retests(c, tested));
            tested.pop();
            ok
        }
        _ => children(t).iter().all(|c| never_retests(c, tested)),
    }
}

// ---------------------------------------------------------------- column heuristics and the two compilers

#[derive(Clone, Copy, PartialEq, Debug)]
enum Heuristic {
    FirstColumn,
    Necessity,
    SmallBranching,
    NecessityThenBranching,
}
const HEURISTICS: [Heuristic; 4] = [
    Heuristic::FirstColumn,
    Heuristic::Necessity,
    Heuristic::SmallBranching,
    Heuristic::NecessityThenBranching,
];

/// Only columns where the FIRST row needs a test are candidates (any other choice could test something the winning row never needs).
fn choose(m: &Matrix, h: Heuristic) -> usize {
    let candidates = (0..m.cols.len()).filter(|&c| refutable(&m.rows[0].pats[c]));
    let necessity = |c: usize| m.rows.iter().filter(|r| refutable(&r.pats[c])).count() as isize;
    let branching = |c: usize| match &TYPES[m.cols[c].1] {
        TyDef::Int(..) => segments((i64::MIN / 2, i64::MAX / 2), &m.head_ranges(c)).len() as isize,
        TyDef::Adt(all) => {
            let used = m.head_ctors(c).len();
            (used + usize::from(used < all.len())) as isize
        }
    };
    let key = |c: usize| -> (isize, isize) {
        match h {
            Heuristic::FirstColumn => (0, 0),
            Heuristic::Necessity => (-necessity(c), 0),
            Heuristic::SmallBranching => (branching(c), 0),
            Heuristic::NecessityThenBranching => (-necessity(c), branching(c)),
        }
    };
    candidates
        .min_by_key(|&c| key(c))
        .expect("a refutable first row has a candidate column")
}
fn fail() -> Rc<Tree> {
    Rc::new(Tree::Fail)
}
/// Build a switch on column `c`. `sub` compiles a specialised matrix; the default is compiled only if some value can reach it.
fn switch(m: &Matrix, c: usize, sub: &mut dyn FnMut(&Matrix) -> Rc<Tree>) -> Rc<Tree> {
    let (occ, ty) = m.cols[c].clone();
    match &TYPES[ty] {
        TyDef::Adt(all) => {
            let heads = m.head_ctors(c);
            let cases = heads
                .iter()
                .map(|&k| (k, sub(&m.specialise_ctor(c, k))))
                .collect();
            let default = (heads.len() < all.len()).then(|| sub(&m.default_matrix(c)));
            Rc::new(Tree::Switch {
                occ,
                cases,
                default,
            })
        }
        TyDef::Int(lo, hi) => {
            let segs = segments((*lo, *hi), &m.head_ranges(c));
            let cases = segs
                .iter()
                .filter(|s| s.1)
                .map(|&(seg, _)| (seg, sub(&m.specialise_int(c, seg))))
                .collect();
            let default = segs.iter().any(|s| !s.1).then(|| sub(&m.default_matrix(c)));
            Rc::new(Tree::IntSwitch {
                occ,
                cases,
                default,
            })
        }
    }
}
/// The first row needs no more tests: bind its variables. If it has a guard, the rest of the matrix is the fallback.
fn finish(m: &Matrix, rest: &mut dyn FnMut(&Matrix) -> Rc<Tree>) -> Rc<Tree> {
    let r = &m.rows[0];
    let mut binds = r.binds.clone();
    binds.extend(r.pats.iter().zip(&m.cols).filter_map(|(p, (o, _))| {
        if let Pat::Bind(x) = p {
            Some((*x, o.clone()))
        } else {
            None
        }
    }));
    match r.guard {
        None => Rc::new(Tree::Leaf { arm: r.arm, binds }),
        Some(g) => {
            let below = Matrix {
                cols: m.cols.clone(),
                rows: m.rows[1..].to_vec(),
            };
            Rc::new(Tree::Guard {
                arm: r.arm,
                binds,
                salt: g.salt,
                otherwise: rest(&below),
            })
        }
    }
}
fn compile_tree(m: &Matrix, h: Heuristic) -> Rc<Tree> {
    let mut rec = |m: &Matrix| compile_tree(m, h);
    if m.rows.is_empty() {
        fail()
    } else if !m.rows[0].pats.iter().any(refutable) {
        finish(m, &mut rec)
    } else {
        switch(m, choose(m, h), &mut rec)
    }
}
/// The "mixture rule": group the leading rows that test the same column; everything below the group is compiled once, as the fallback.
fn compile_backtracking(m: &Matrix) -> Rc<Tree> {
    let mut rec = |m: &Matrix| compile_backtracking(m);
    if m.rows.is_empty() {
        return fail();
    }
    if !m.rows[0].pats.iter().any(refutable) {
        return finish(m, &mut rec);
    }
    let c = choose(m, Heuristic::FirstColumn);
    let group = m.rows.iter().take_while(|r| refutable(&r.pats[c])).count();
    let head = Matrix {
        cols: m.cols.clone(),
        rows: m.rows[..group].to_vec(),
    };
    let first = switch(&head, c, &mut rec);
    if group == m.rows.len() {
        first
    } else {
        Rc::new(Tree::Try(
            first,
            compile_backtracking(&Matrix {
                cols: m.cols.clone(),
                rows: m.rows[group..].to_vec(),
            }),
        ))
    }
}

// ---------------------------------------------------------------- usefulness (exhaustiveness and redundancy)

/// P is a list of or-free rows over the columns `tys`. Returns a witness vector matched by `q` and by no row of P.
/// The witness may contain wildcards: then EVERY instance of it is unmatched.
fn useful(tys: &[TyId], p: &[Vec<Pat>], q: &[Pat]) -> Option<Vec<Pat>> {
    let Some(head) = q.first() else {
        return p.is_empty().then(Vec::new);
    };
    let matrix = Matrix {
        cols: tys.iter().map(|t| (vec![], *t)).collect(),
        rows: p
            .iter()
            .map(|r| Row {
                pats: r.clone(),
                binds: vec![],
                arm: 0,
                guard: None,
            })
            .collect(),
    };
    let rows_of = |m: &Matrix| -> Vec<Vec<Pat>> { m.rows.iter().map(|r| r.pats.clone()).collect() };
    let tys_of = |m: &Matrix| -> Vec<TyId> { m.cols.iter().map(|c| c.1).collect() };
    // recurse under constructor k, then fold the first `arity` witness columns back into one pattern
    let under_ctor = |k: usize, q_args: Vec<Pat>| -> Option<Vec<Pat>> {
        let arity = q_args.len();
        let s = matrix.specialise_ctor(0, k);
        let mut w = useful(
            &tys_of(&s),
            &rows_of(&s),
            &[q_args, q[1..].to_vec()].concat(),
        )?;
        let args = w.drain(..arity).collect();
        w.insert(0, Pat::Ctor(k, args));
        Some(w)
    };
    let under_segments = |lo: i64, hi: i64| -> Option<Vec<Pat>> {
        segments((lo, hi), &matrix.head_ranges(0))
            .into_iter()
            .find_map(|(seg, _)| {
                let s = matrix.specialise_int(0, seg);
                let mut w = useful(&tys_of(&s), &rows_of(&s), &q[1..])?;
                w.insert(0, Pat::Range(seg.0, seg.1));
                Some(w)
            })
    };
    match head {
        Pat::Or(alts) => alts
            .iter()
            .find_map(|a| useful(tys, p, &[vec![a.clone()], q[1..].to_vec()].concat())),
        Pat::Ctor(k, args) => under_ctor(*k, args.clone()),
        Pat::Range(lo, hi) => under_segments(*lo, *hi),
        Pat::Wild | Pat::Bind(_) => match &TYPES[tys[0]] {
            TyDef::Int(lo, hi) => under_segments(*lo, *hi),
            TyDef::Adt(all) => {
                let heads = matrix.head_ctors(0);
                if heads.len() == all.len() {
                    // complete signature: the missing value, if any, is below one of the constructors
                    (0..all.len()).find_map(|k| under_ctor(k, vec![Pat::Wild; all[k].1.len()]))
                } else {
                    let d = matrix.default_matrix(0);
                    let mut w = useful(&tys_of(&d), &rows_of(&d), &q[1..])?;
                    let missing = (0..all.len())
                        .find(|k| !heads.contains(k))
                        .expect("incomplete signature");
                    let first = if heads.is_empty() {
                        Pat::Wild
                    } else {
                        Pat::Ctor(missing, vec![Pat::Wild; all[missing].1.len()])
                    };
                    w.insert(0, first);
                    Some(w)
                }
            }
        },
    }
}
/// Rows that are guaranteed to win when they match: guarded arms cover nothing.
fn unguarded_rows(arms: &[Arm]) -> Vec<Vec<Pat>> {
    arms.iter()
        .filter(|a| a.guard.is_none())
        .flat_map(|a| expand(&a.pat))
        .map(|p| vec![p])
        .collect()
}
/// None if exhaustive, otherwise a pattern describing values that fall through.
fn missing_case(ty: TyId, arms: &[Arm]) -> Option<Pat> {
    useful(&[ty], &unguarded_rows(arms), &[Pat::Wild]).map(|mut w| w.remove(0))
}
/// Indices of arms that can never be selected.
fn redundant_arms(ty: TyId, arms: &[Arm]) -> Vec<usize> {
    (0..arms.len())
        .filter(|&i| {
            useful(
                &[ty],
                &unguarded_rows(&arms[..i]),
                std::slice::from_ref(&arms[i].pat),
            )
            .is_none()
        })
        .collect()
}
fn show_pat(ty: TyId, p: &Pat) -> String {
    match p {
        Pat::Wild => "_".into(),
        Pat::Bind(x) => format!("x{x}"),
        Pat::Range(a, b) if a == b => a.to_string(),
        Pat::Range(a, b) => format!("{a}..={b}"),
        Pat::Or(a) => format!(
            "({})",
            a.iter()
                .map(|p| show_pat(ty, p))
                .collect::<Vec<_>>()
                .join(" | ")
        ),
        Pat::Ctor(k, args) => {
            let (name, tys) = ctors(ty)[*k];
            if args.is_empty() {
                name.into()
            } else {
                format!(
                    "{name}({})",
                    args.iter()
                        .zip(tys)
                        .map(|(p, t)| show_pat(*t, p))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

// ---------------------------------------------------------------- brute-force universe and random inputs

/// Every value of `ty` of constructor depth at most `depth`. If no pattern inspects a path longer than `depth - 1`, this is exact for coverage
/// and reachability: any value can be cut off below that point (replacing subvalues by small ones) without changing which patterns match it.
/// Beware the growth: `universe(TREE, 3)` has 723 trees, `universe(TREE, 4)` has over a million.
fn universe(ty: TyId, depth: usize) -> Vec<Value> {
    match &TYPES[ty] {
        TyDef::Int(lo, hi) => (*lo..=*hi).map(Value::I).collect(),
        TyDef::Adt(cs) => {
            let mut out = vec![];
            for (k, (_, args)) in cs.iter().enumerate() {
                if depth == 0 && !args.is_empty() {
                    continue;
                }
                let mut tuples = vec![vec![]];
                for a in args.iter() {
                    let vs = universe(*a, depth.saturating_sub(1));
                    tuples = tuples
                        .iter()
                        .flat_map(|t: &Vec<Value>| {
                            vs.iter().map(|v| [t.clone(), vec![v.clone()]].concat())
                        })
                        .collect();
                }
                out.extend(tuples.into_iter().map(|t| Value::C(k, t)));
            }
            out
        }
    }
}
const PAT_DEPTH: usize = 2;
fn random_pat(r: &mut Rng, ty: TyId, depth: usize, next_var: &mut usize) -> Pat {
    let roll = r.below(100);
    if roll < 10 {
        *next_var += 1;
        return Pat::Bind(*next_var);
    }
    if roll < 25 {
        return Pat::Wild;
    }
    if roll < 33 {
        // or-patterns whose alternatives bind nothing (real languages require every alternative to bind the same names)
        let strip = |p: Pat| unbind(&p);
        return Pat::Or(
            (0..2)
                .map(|_| strip(random_pat(r, ty, depth, next_var)))
                .collect(),
        );
    }
    match &TYPES[ty] {
        TyDef::Int(lo, hi) => {
            let a = lo + r.below((hi - lo + 1) as usize) as i64;
            let b = if r.chance(50) {
                a
            } else {
                a + r.below((hi - a + 1) as usize) as i64
            };
            Pat::Range(a, b)
        }
        TyDef::Adt(cs) => {
            let allowed: Vec<usize> = (0..cs.len())
                .filter(|&k| depth > 0 || cs[k].1.is_empty())
                .collect();
            if allowed.is_empty() {
                return Pat::Wild;
            }
            let k = allowed[r.below(allowed.len())];
            Pat::Ctor(
                k,
                cs[k]
                    .1
                    .iter()
                    .map(|t| random_pat(r, *t, depth.saturating_sub(1), next_var))
                    .collect(),
            )
        }
    }
}
fn unbind(p: &Pat) -> Pat {
    match p {
        Pat::Bind(_) => Pat::Wild,
        Pat::Ctor(k, a) => Pat::Ctor(*k, a.iter().map(unbind).collect()),
        Pat::Or(a) => Pat::Or(a.iter().map(unbind).collect()),
        other => other.clone(),
    }
}
fn random_arms(r: &mut Rng, ty: TyId) -> Vec<Arm> {
    let n = 1 + r.below(7);
    let mut next_var = 0;
    (0..n)
        .map(|_| Arm {
            pat: random_pat(r, ty, PAT_DEPTH, &mut next_var),
            guard: r.chance(18).then(|| Guard { salt: r.next() }),
        })
        .collect()
}

// ---------------------------------------------------------------- verification

#[derive(Default)]
struct Totals {
    values: usize,
    exhaustive: usize,
    redundant: usize,
    guarded: usize,
    tree_nodes: [usize; 5],
    dag_nodes: [usize; 5],
    tests: [usize; 5],
}
fn random_differential(r: &mut Rng) {
    let scrutinees = [BOOL, RGB, OPT, LIST, PAIR, TREE, INT];
    // random patterns inspect paths of length at most PAT_DEPTH (constructors at depth budget 0 have no arguments)
    let universes: Vec<Vec<Value>> = (0..BYTE).map(|t| universe(t, PAT_DEPTH + 1)).collect();
    let mut t = Totals::default();
    for case in 0..3000 {
        let ty = scrutinees[case % scrutinees.len()];
        let arms = random_arms(r, ty);
        let m = Matrix::new(ty, &arms);
        let mut compiled: Vec<Rc<Tree>> = HEURISTICS.iter().map(|&h| compile_tree(&m, h)).collect();
        for tree in &compiled {
            assert!(
                never_retests(tree, &mut vec![]),
                "decision trees test every occurrence at most once per path"
            );
        }
        compiled.push(compile_backtracking(&m));
        let shared: Vec<Rc<Tree>> = compiled
            .iter()
            .map(|c| share(c, &mut HashMap::new()))
            .collect();
        for (i, c) in compiled.iter().enumerate() {
            t.tree_nodes[i] += tree_size(c);
            let n = dag_size(c, &mut HashSet::new());
            assert!(n <= tree_size(c));
            t.dag_nodes[i] += n;
        }
        let mut selected = HashSet::new();
        let mut falls_through_unguarded = false;
        let cover = unguarded_rows(&arms);
        for v in &universes[ty] {
            let want = sorted(naive_match(&arms, v));
            if let Some((a, _)) = &want {
                selected.insert(*a);
            }
            for (i, c) in compiled.iter().enumerate() {
                assert_eq!(
                    sorted(run(c, v, &mut t.tests[i])),
                    want,
                    "compiler {i} disagrees on {v:?} for {arms:?}"
                );
                assert_eq!(
                    sorted(run(&shared[i], v, &mut 0)),
                    want,
                    "the shared DAG must behave like the tree"
                );
            }
            falls_through_unguarded |= !cover
                .iter()
                .any(|row| match_simple(&row[0], v, &mut vec![]));
            t.values += 1;
        }
        // exhaustiveness against brute force; the witness must describe only unmatched values, and at least one
        let missing = missing_case(ty, &arms);
        assert_eq!(
            missing.is_some(),
            falls_through_unguarded,
            "exhaustiveness of {arms:?}: witness {missing:?}"
        );
        match &missing {
            None => t.exhaustive += 1,
            Some(w) => {
                let instances: Vec<&Value> = universes[ty]
                    .iter()
                    .filter(|v| expand(w).iter().any(|p| match_simple(p, v, &mut vec![])))
                    .collect();
                assert!(!instances.is_empty(), "witness {w:?} has no instance");
                for v in instances {
                    assert!(
                        !cover
                            .iter()
                            .any(|row| match_simple(&row[0], v, &mut vec![])),
                        "witness instance {v:?} is matched: {arms:?}"
                    );
                }
            }
        }
        // redundancy against brute force: arm i is useful iff some value reaches it when only unguarded arms above can stop it
        let redundant = redundant_arms(ty, &arms);
        for i in 0..arms.len() {
            let above = unguarded_rows(&arms[..i]);
            let reachable = universes[ty].iter().any(|v| {
                expand(&arms[i].pat)
                    .iter()
                    .any(|p| match_simple(p, v, &mut vec![]))
                    && !above
                        .iter()
                        .any(|row| match_simple(&row[0], v, &mut vec![]))
            });
            assert_eq!(
                !redundant.contains(&i),
                reachable,
                "redundancy of arm {i} in {arms:?}"
            );
            assert!(
                !(redundant.contains(&i) && selected.contains(&i)),
                "an arm reported redundant was selected"
            );
        }
        t.redundant += redundant.len();
        t.guarded += arms.iter().filter(|a| a.guard.is_some()).count();
    }
    println!(
        "-- 3000 random matches, {} (match, value) pairs: 4 decision trees, their DAGs and the backtracking automaton all equal the naive matcher",
        t.values
    );
    println!(
        "   exhaustiveness (with checked witnesses) and per-arm redundancy equal brute force: {} exhaustive, {} redundant arms, {} guarded arms",
        t.exhaustive, t.redundant, t.guarded
    );
    println!(
        "   {:<24} {:>11} {:>10} {:>16}",
        "compiler", "tree nodes", "DAG nodes", "tests per value"
    );
    for (i, name) in compiler_names().iter().enumerate() {
        println!(
            "   {:<24} {:>11} {:>10} {:>16.3}",
            name,
            t.tree_nodes[i],
            t.dag_nodes[i],
            t.tests[i] as f64 / t.values as f64
        );
    }
}
fn compiler_names() -> [String; 5] {
    let h = |i: usize| format!("tree/{:?}", HEURISTICS[i]);
    [h(0), h(1), h(2), h(3), "backtracking".into()]
}

// ---------------------------------------------------------------- classic examples

/// Compile a hand-written matrix whose columns are given directly (a tuple scrutinee).
fn tuple_matrix(tys: &[TyId], rows: &[Vec<Pat>]) -> Matrix {
    Matrix {
        cols: tys.iter().enumerate().map(|(i, t)| (vec![i], *t)).collect(),
        rows: rows
            .iter()
            .enumerate()
            .map(|(i, r)| Row {
                pats: r.clone(),
                binds: vec![],
                arm: i,
                guard: None,
            })
            .collect(),
    }
}
fn naive_tuple(rows: &[Vec<Pat>], vals: &[Value]) -> Option<usize> {
    rows.iter().position(|r| {
        r.iter()
            .zip(vals)
            .all(|(p, v)| expand(p).iter().any(|q| match_simple(q, v, &mut vec![])))
    })
}
/// Print sizes for every compiler on one matrix and check them against the definition on the given universe.
fn report(title: &str, m: &Matrix, check: &dyn Fn(&Tree)) -> Vec<(usize, usize)> {
    println!("   {title}");
    let mut trees: Vec<Rc<Tree>> = HEURISTICS.iter().map(|&h| compile_tree(m, h)).collect();
    trees.push(compile_backtracking(m));
    trees
        .iter()
        .zip(compiler_names())
        .map(|(t, name)| {
            check(t);
            let sizes = (tree_size(t), dag_size(t, &mut HashSet::new()));
            println!(
                "     {name:<30} tree nodes {:>5}   DAG nodes {:>4}",
                sizes.0, sizes.1
            );
            sizes
        })
        .collect()
}
fn check_tuple<'a>(tys: &'a [TyId], rows: &'a [Vec<Pat>], depth: usize) -> impl Fn(&Tree) + 'a {
    move |tree| {
        let mut all = vec![vec![]];
        for t in tys {
            let vs = universe(*t, depth);
            all = all
                .iter()
                .flat_map(|p: &Vec<Value>| vs.iter().map(|v| [p.clone(), vec![v.clone()]].concat()))
                .collect();
        }
        for vals in all {
            let got = run(tree, &Value::C(0, vals.clone()), &mut 0).map(|o| o.0);
            assert_eq!(got, naive_tuple(rows, &vals), "{vals:?}");
        }
    }
}
fn classic_examples() {
    println!("-- classic matrices");
    let (f, t, w) = (
        || Pat::Ctor(0, vec![]),
        || Pat::Ctor(1, vec![]),
        || Pat::Wild,
    );
    // the column choice changes the tree size. Here the necessity choice happens to be the WORSE one: heuristics are only heuristics.
    let rows = vec![
        vec![w(), f(), t()],
        vec![f(), t(), w()],
        vec![w(), w(), f()],
        vec![w(), w(), t()],
    ];
    let tys = [BOOL; 3];
    report(
        "three booleans (_,F,T) (F,T,_) (_,_,F) (_,_,T)",
        &tuple_matrix(&tys, &rows),
        &check_tuple(&tys, &rows, 1),
    );
    // list zip: match (xs, ys)
    let nil = || Pat::Ctor(0, vec![]);
    let cons = |a: Pat, b: Pat| Pat::Ctor(1, vec![a, b]);
    let rows = vec![
        vec![nil(), w()],
        vec![w(), nil()],
        vec![cons(w(), w()), cons(w(), w())],
    ];
    let tys = [LIST; 2];
    report(
        "zip: (Nil,_) (_,Nil) (Cons,Cons)",
        &tuple_matrix(&tys, &rows),
        &check_tuple(&tys, &rows, 3),
    );
    // disjoint pairs: arm i needs columns 2i and 2i+1. Both outcomes of testing column 2i still need all the later arms, so they are copied.
    let n = 7;
    let mut rows: Vec<Vec<Pat>> = (0..n)
        .map(|i| {
            (0..2 * n)
                .map(|j| if j / 2 == i { t() } else { w() })
                .collect()
        })
        .collect();
    rows.push(vec![w(); 2 * n]);
    let tys = vec![BOOL; 2 * n];
    let sizes = report(
        "7 disjoint pairs (T,T,_..) (_,_,T,T,_..): the tree is exponential, sharing and backtracking stay linear",
        &tuple_matrix(&tys, &rows),
        &check_tuple(&tys, &rows, 1),
    );
    assert!(
        sizes[0].0 >= 1 << n,
        "the unshared first-column tree is exponential: {}",
        sizes[0].0
    );
    assert!(sizes[0].1 <= 4 * n, "the shared tree is linear: {sizes:?}");
    // per arm: Try, two switches, a leaf and the two `Fail` defaults; plus the final default leaf
    assert_eq!(
        sizes[4].0,
        6 * n + 1,
        "the backtracking automaton never copies a row"
    );
}
/// Okasaki's red-black balance: four rotations written as four arms (here: only shapes and colours matter).
fn red_black_balance() {
    // a local type universe does not fit `TYPES`, so use TREE with BOOL as the colour: T = red, F = black
    let node = |l: Pat, red: usize, r: Pat| Pat::Ctor(1, vec![l, Pat::Ctor(red, vec![]), r]);
    let red = |l: Pat, r: Pat| node(l, 1, r);
    let black = |l: Pat, r: Pat| node(l, 0, r);
    let w = || Pat::Wild;
    let arms: Vec<Arm> = [
        black(red(red(w(), w()), w()), w()),
        black(red(w(), red(w(), w())), w()),
        black(w(), red(red(w(), w()), w())),
        black(w(), red(w(), red(w(), w()))),
        w(),
    ]
    .into_iter()
    .map(|pat| Arm { pat, guard: None })
    .collect();
    let values = universe(TREE, 3); // the patterns look at grandchildren: paths of length 2 (and their colour)
    let m = Matrix::new(TREE, &arms);
    let check = |tree: &Tree| {
        for v in &values {
            assert_eq!(
                sorted(run(tree, v, &mut 0)),
                sorted(naive_match(&arms, v)),
                "{v:?}"
            );
        }
    };
    report(
        &format!(
            "red-black balance (4 rotations + default), checked on {} trees",
            values.len()
        ),
        &m,
        &check,
    );
    assert_eq!(missing_case(TREE, &arms), None);
    assert!(redundant_arms(TREE, &arms).is_empty());
    let witness =
        missing_case(TREE, &arms[..4]).expect("without the default arm something is missing");
    println!(
        "     without the default arm, rustc-style error: pattern `{}` not covered",
        show_pat(TREE, &witness)
    );
}
fn diagnostics_demo() {
    println!("-- diagnostics on integer ranges (domain 0..=255) and guards");
    let arm = |pat| Arm { pat, guard: None };
    let mut arms = vec![
        arm(Pat::Range(0, 9)),
        arm(Pat::Range(20, 255)),
        arm(Pat::Range(5, 7)),
    ];
    let w = missing_case(BYTE, &arms).expect("10..=19 is missing");
    println!(
        "   0..=9 | 20..=255 | 5..=7        -> not covered: {}, unreachable arms: {:?}",
        show_pat(BYTE, &w),
        redundant_arms(BYTE, &arms)
    );
    assert_eq!(w, Pat::Range(10, 19));
    assert_eq!(redundant_arms(BYTE, &arms), [2]);
    arms.push(arm(Pat::Range(10, 19)));
    assert_eq!(missing_case(BYTE, &arms), None);
    // a guarded catch-all covers nothing, and does not make later arms redundant
    let arms = vec![
        Arm {
            pat: Pat::Wild,
            guard: Some(Guard { salt: 1 }),
        },
        arm(Pat::Ctor(0, vec![])),
    ];
    let w = missing_case(OPT, &arms).expect("a guard may be false");
    println!(
        "   _ if g | None                  -> not covered: {}, unreachable arms: {:?}",
        show_pat(OPT, &w),
        redundant_arms(OPT, &arms)
    );
    assert_eq!(show_pat(OPT, &w), "Some(_)");
    assert!(redundant_arms(OPT, &arms).is_empty());
    // a guard failing on the first alternative of an or-pattern must try the second
    let both = Pat::Ctor(0, vec![Pat::Or(vec![Pat::Bind(1), Pat::Wild]), Pat::Wild]);
    let v = Value::C(0, vec![Value::C(0, vec![]), Value::I(3)]);
    let rejects_bound = (0u64..)
        .map(|salt| Guard { salt })
        .find(|g| !g.holds(&vec![(1, Value::C(0, vec![]))]) && g.holds(&vec![]))
        .expect("such a guard exists");
    let arms = vec![Arm {
        pat: both,
        guard: Some(rejects_bound),
    }];
    let tree = compile_tree(&Matrix::new(PAIR, &arms), Heuristic::FirstColumn);
    assert_eq!(
        naive_match(&arms, &v),
        Some((0, vec![])),
        "the second alternative is tried after the guard fails on the first"
    );
    assert_eq!(run(&tree, &v, &mut 0), Some((0, vec![])));
    println!(
        "   or-pattern + guard             -> a failing guard moves on to the next alternative"
    );
}

fn main() {
    diagnostics_demo();
    classic_examples();
    red_black_balance();
    random_differential(&mut Rng(0x0815_DEC1_5104_77EE));
    println!("all pattern match compilation checks passed");
}
