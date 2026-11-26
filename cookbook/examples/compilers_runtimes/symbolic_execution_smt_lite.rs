//! RECIPE 824 (compilers): symbolic and concolic execution with a DPLL(T)-lite solver (boolean core + bounded linear-integer theory), state merging, bug finding and bounded proofs.
//! LEVEL: advanced
//! Use in: test generation, bug finding and bounded verification of small programs; the engine room of KLEE/SAGE-style tools.   Run: cargo run --release --example symbolic_execution_smt_lite
#![allow(dead_code)]
//!
//! WHAT: a tiny imperative language (integer variables, linear arithmetic, if, while, assume, assert) executed three ways.
//!   1. CONCRETE interpreter: the oracle. Every claim below is replayed or compared against it.
//!   2. SYMBOLIC engine: program inputs are unknowns. Each variable holds a LINEAR EXPRESSION over the unknowns, and a PATH
//!      CONDITION (a boolean formula) records which branches were taken. At a branch both sides are checked for feasibility
//!      with the solver and the state FORKS when both are possible. At `assert c` the solver is asked whether
//!      (path condition AND NOT c) is satisfiable; a model is a COUNTEREXAMPLE input. If it is unsatisfiable on every path the
//!      assertion is PROVED (within the loop bound). Loops are unrolled up to a bound; paths that would exceed it are counted
//!      as `bound hits` and are the reason a bounded "no bug found" is not an unbounded proof.
//!   3. STATE MERGING: instead of forking at an `if`, the two end states are joined into one: every variable that differs
//!      becomes a fresh unknown `v` DEFINED by `guard_i => v = value_i` (an `ite`). One path survives, but the solver
//!      formulas get harder. This is the classic trade-off (veritesting, KLEE `--use-merge`): fewer paths, harder queries.
//!      Two details keep the formulas from exploding. (a) `guard_i` is only what state i added AFTER the states forked; the
//!      shared prefix of the path conditions is kept once. Copying whole path conditions grew them about 4x per merge
//!      (13, 67, 283, ... 294907 nodes after 8 merges) until DPLL, which recurses once per decision, overflowed the stack.
//!      (b) The definitions live OUTSIDE the path condition, so an outer merge never copies an inner merge's definitions into
//!      its guards; a query pulls in just those of the variables it mentions, transitively (cone-of-influence slicing).
//!   4. CONCOLIC testing (DART / SAGE): run the program on a concrete input while tracking the symbolic path condition on the
//!      side, then NEGATE each branch constraint in turn (keeping the prefix), solve, and run the new input. GENERATIONAL
//!      search gives each child the index after the decision it negated and only negates later ones, so (with an exact solver
//!      and shadow, as here) every feasible path gets exactly one concrete run. It only negates branches of paths that
//!      really ran. In real tools, where the shadow is imprecise, a wrong guess just costs a wasted (divergent) run.
//!
//! THE SOLVER (`solve_f`), DPLL(T) in miniature:
//!   * formulas are in negation normal form over ATOMS `sum(c_i * x_i) + k <= 0` (equality = two atoms, disequality = a
//!     disjunction), all unknowns are BOUNDED integers;
//!   * Plaisted-Greenbaum/Tseitin encoding turns the formula into CNF over boolean variables (one per distinct atom, one per
//!     and/or node);
//!   * the BOOLEAN CORE is DPLL: unit propagation, then branch on a literal of the first unsatisfied clause;
//!   * the THEORY SOLVER decides whether a conjunction of atoms (an atom set to false means its integer negation
//!     `-sum - k + 1 <= 0`) has an integer solution: interval (bounds) propagation to a fixpoint plus bisection of domains. It is
//!     complete because the domains are finite; that finiteness is the "bit-blast fallback" in spirit (every solution is a point
//!     in a box). Fourier-Motzkin would decide the rational relaxation of the same conjunction but is incomplete for integers;
//!   * on a theory conflict the solver MINIMISES the conflicting literal set (deletion) and LEARNS its negation as a clause
//!     (a theory lemma), the same step as in Z3/CVC5 minus watched literals and 1UIP (see sat_solver_cdcl for those).
//!
//! VERIFICATION: (a) the solver agrees with brute force over a small box on 3000 random constraint sets and every model
//! satisfies the formula; (b) a suite of 12 buggy and 6 safe programs: the set of assertions each engine can break equals the
//! set found by exhaustive concrete execution, every counterexample is REPLAYED concretely and trips exactly that assertion, and
//! the safe programs report nothing; (c) the sets of explored paths of the symbolic engine and the concolic driver are equal;
//! (d) the same checks on 400 random programs with nested ifs, input-bounded loops, assumes and several asserts; (e) the
//! concolic driver never runs a path twice or leaves its predicted prefix; (f) merged path conditions stay linear in size.
//!
//! COMMON USES: KLEE (symbolic execution of C/LLVM), Microsoft SAGE and DART (concolic fuzzing), angr and other binary
//! analysers, Java PathFinder / SPF, Rust verifiers such as Kani and Prusti (bounded model checking and deductive proofs use
//! the same path-condition ideas), property-based test generators, and SMT solvers (Z3, CVC5, Yices) built as CDCL(T).
//! Read: Godefroid, Klarlund, Sen "DART: Directed Automated Random Testing" (PLDI 2005, title only, not re-read here);
//! Cadar et al. "KLEE: Unassisted and Automatic Generation of High-Coverage Tests" (OSDI 2008, title only);
//! King "Symbolic Execution and Program Testing" (CACM 1976, title only); Nieuwenhuis, Oliveras, Tinelli "Solving SAT and SAT
//! Modulo Theories: From an abstract DPLL procedure to DPLL(T)" (J. ACM 2006, title only);
//! https://klee-se.org/ ; https://en.wikipedia.org/wiki/Symbolic_execution ; https://en.wikipedia.org/wiki/Concolic_testing
//! TRY NEXT:
//!   - [S] In `dpll`, try the NEGATIVE phase of the picked literal first (`[-l, l]`), run the 3000-case test and compare the
//!     printed theory calls; predict the direction first.
//!   - [S] In `demo_loop_bound`, widen the input range of `loop_sum_wide` to (0, 20): predict which of its assertions then
//!     fail and explain why `bound hits` cannot be zero at bound 12, so that run proves nothing.
//!   - [M] Add multiplication of two unknowns by concretising one operand to a model value on first use (as KLEE does) and
//!     show with a program that this loses completeness while staying sound for the counterexamples it reports.
//!   - [M] Add array reads `a[i]` with a symbolic index: encode as an `ite` chain over the finite array and prove an
//!     out-of-bounds access impossible for a guarded program.
//!   - [A] PROJECT: combine with `sat_solver_cdcl`: replace the DPLL core of `solve_f` by its CDCL solver (watched literals,
//!     1UIP, backjumping) and keep the bounds theory as a theory propagator that returns explanations; compare `theory_calls`.
use std::collections::{BTreeMap, BTreeSet, HashMap};

// ---------------------------------------------------------------- rng

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
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- linear expressions and formulas

/// sum(coef * var) + k, with zero coefficients removed so equal expressions compare equal.
#[derive(Clone, PartialEq, Eq, Debug, Default, PartialOrd, Ord)]
struct Lin {
    t: BTreeMap<usize, i64>,
    k: i64,
}

impl Lin {
    fn konst(k: i64) -> Lin {
        Lin {
            t: BTreeMap::new(),
            k,
        }
    }
    fn var(i: usize) -> Lin {
        Lin {
            t: BTreeMap::from([(i, 1)]),
            k: 0,
        }
    }
    fn add(&self, o: &Lin, sign: i64) -> Lin {
        let mut r = self.clone();
        for (&v, &c) in &o.t {
            let nv = r.t.get(&v).copied().unwrap_or(0) + sign * c;
            if nv == 0 {
                r.t.remove(&v);
            } else {
                r.t.insert(v, nv);
            }
        }
        r.k += sign * o.k;
        r
    }
    fn scale(&self, c: i64) -> Lin {
        if c == 0 {
            return Lin::konst(0);
        }
        Lin {
            t: self.t.iter().map(|(&v, &x)| (v, x * c)).collect(),
            k: self.k * c,
        }
    }
    fn eval(&self, p: &[i64]) -> i64 {
        self.t.iter().map(|(&v, &c)| c * p[v]).sum::<i64>() + self.k
    }
    fn range(&self, b: &[(i64, i64)]) -> (i64, i64) {
        let (mut lo, mut hi) = (self.k, self.k);
        for (&v, &c) in &self.t {
            let (a, z) = (c * b[v].0, c * b[v].1);
            lo += a.min(z);
            hi += a.max(z);
        }
        (lo, hi)
    }
}

/// Negation normal form over atoms `lin <= 0`.
#[derive(Clone, Debug, PartialEq)]
enum F {
    True,
    False,
    Le(Lin),
    And(Vec<F>),
    Or(Vec<F>),
}

fn le(l: Lin) -> F {
    if l.t.is_empty() {
        if l.k <= 0 { F::True } else { F::False }
    } else {
        F::Le(l)
    }
}
fn eq0(l: &Lin) -> F {
    f_and(vec![le(l.clone()), le(l.scale(-1))])
}
fn f_and(v: Vec<F>) -> F {
    let mut out = vec![];
    for f in v {
        match f {
            F::True => {}
            F::False => return F::False,
            F::And(inner) => out.extend(inner),
            o => out.push(o),
        }
    }
    match out.len() {
        0 => F::True,
        1 => out.pop().unwrap(),
        _ => F::And(out),
    }
}
fn f_or(v: Vec<F>) -> F {
    let mut out = vec![];
    for f in v {
        match f {
            F::False => {}
            F::True => return F::True,
            F::Or(inner) => out.extend(inner),
            o => out.push(o),
        }
    }
    match out.len() {
        0 => F::False,
        1 => out.pop().unwrap(),
        _ => F::Or(out),
    }
}
/// Integer negation of `l <= 0` is `l >= 1`, i.e. `-l + 1 <= 0`; this is why atoms never need a separate `<` or `>`.
fn neg(f: &F) -> F {
    match f {
        F::True => F::False,
        F::False => F::True,
        F::Le(l) => le(l.scale(-1).add(&Lin::konst(1), 1)),
        F::And(v) => f_or(v.iter().map(neg).collect()),
        F::Or(v) => f_and(v.iter().map(neg).collect()),
    }
}
fn eval_f(f: &F, p: &[i64]) -> bool {
    match f {
        F::True => true,
        F::False => false,
        F::Le(l) => l.eval(p) <= 0,
        F::And(v) => v.iter().all(|x| eval_f(x, p)),
        F::Or(v) => v.iter().any(|x| eval_f(x, p)),
    }
}

fn f_size(f: &F) -> usize {
    match f {
        F::And(v) | F::Or(v) => 1 + v.iter().map(f_size).sum::<usize>(),
        _ => 1,
    }
}

fn f_vars(f: &F, out: &mut BTreeSet<usize>) {
    match f {
        F::Le(l) => out.extend(l.t.keys()),
        F::And(v) | F::Or(v) => v.iter().for_each(|x| f_vars(x, out)),
        F::True | F::False => {}
    }
}

// ---------------------------------------------------------------- theory solver: bounded linear integer arithmetic

type Con = (Vec<(usize, i64)>, i64); // sum(c*x) + k <= 0

fn negate_con(c: &Con) -> Con {
    (c.0.iter().map(|&(v, a)| (v, -a)).collect(), -c.1 + 1)
}
fn fdiv(p: i64, q: i64) -> i64 {
    let d = p / q;
    if p % q != 0 && ((p < 0) != (q < 0)) {
        d - 1
    } else {
        d
    }
}
fn cdiv(p: i64, q: i64) -> i64 {
    -fdiv(-p, q)
}

/// Bounds propagation to a fixpoint. For `sum a_i x_i + k <= 0`, a_i * x_i <= -(k + sum of the minima of the others).
fn propagate(cons: &[Con], lo: &mut [i64], hi: &mut [i64]) -> bool {
    loop {
        let mut changed = false;
        for (t, k) in cons {
            let contrib = |v: usize, a: i64, lo: &[i64], hi: &[i64]| {
                if a > 0 { a * lo[v] } else { a * hi[v] }
            };
            let mn: i64 = *k + t.iter().map(|&(v, a)| contrib(v, a, lo, hi)).sum::<i64>();
            if mn > 0 {
                return false;
            }
            for &(v, a) in t {
                let r = -(mn - contrib(v, a, lo, hi)); // a * x_v <= r
                if a > 0 {
                    let nh = fdiv(r, a);
                    if nh < hi[v] {
                        hi[v] = nh;
                        changed = true;
                    }
                } else {
                    let nl = cdiv(r, a);
                    if nl > lo[v] {
                        lo[v] = nl;
                        changed = true;
                    }
                }
                if lo[v] > hi[v] {
                    return false;
                }
            }
        }
        if !changed {
            return true;
        }
    }
}

/// Complete over a finite box: propagate, then bisect the narrowest undecided domain.
fn theory_search(cons: &[Con], lo: &mut Vec<i64>, hi: &mut Vec<i64>) -> Option<Vec<i64>> {
    if !propagate(cons, lo, hi) {
        return None;
    }
    let pick = (0..lo.len())
        .filter(|&v| lo[v] < hi[v])
        .min_by_key(|&v| hi[v] - lo[v]);
    let Some(v) = pick else {
        return Some(lo.clone());
    }; // every domain is a point and propagation verified all constraints
    let mid = lo[v] + (hi[v] - lo[v]) / 2;
    let (mut l2, mut h2) = (lo.clone(), hi.clone());
    h2[v] = mid;
    if let Some(m) = theory_search(cons, &mut l2, &mut h2) {
        return Some(m);
    }
    lo[v] = mid + 1;
    theory_search(cons, lo, hi)
}

// ---------------------------------------------------------------- boolean core: Tseitin + DPLL(T)

#[derive(Default, Clone, Debug)]
struct Stats {
    queries: usize,
    theory_calls: usize,
    learnt: usize,
}

struct Cnf {
    clauses: Vec<Vec<i32>>,
    atoms: Vec<Option<Con>>, // index = boolean variable (1-based, slot 0 unused)
    ids: HashMap<Con, i32>,
}

impl Cnf {
    fn new() -> Cnf {
        Cnf {
            clauses: vec![],
            atoms: vec![None],
            ids: HashMap::new(),
        }
    }
    fn fresh(&mut self, a: Option<Con>) -> i32 {
        self.atoms.push(a);
        (self.atoms.len() - 1) as i32
    }
    /// Only the "g implies children" direction is emitted: sound for NNF formulas (the encoding is monotone in g).
    fn enc(&mut self, f: &F) -> i32 {
        match f {
            F::Le(l) => {
                let con: Con = (l.t.iter().map(|(&v, &c)| (v, c)).collect(), l.k);
                if let Some(&id) = self.ids.get(&con) {
                    return id;
                }
                let id = self.fresh(Some(con.clone()));
                self.ids.insert(con, id);
                id
            }
            F::And(v) => {
                let lits: Vec<i32> = v.iter().map(|c| self.enc(c)).collect();
                let g = self.fresh(None);
                for l in lits {
                    self.clauses.push(vec![-g, l]);
                }
                g
            }
            F::Or(v) => {
                let lits: Vec<i32> = v.iter().map(|c| self.enc(c)).collect();
                let g = self.fresh(None);
                let mut cl = vec![-g];
                cl.extend(lits);
                self.clauses.push(cl);
                g
            }
            F::True | F::False => unreachable!("smart constructors remove constants"),
        }
    }
}

struct Dpll<'a> {
    cnf: Cnf,
    bounds: &'a [(i64, i64)],
    stats: &'a mut Stats,
}

impl Dpll<'_> {
    fn theory(&mut self, lits: &[i32]) -> Option<Vec<i64>> {
        self.stats.theory_calls += 1;
        let cons: Vec<Con> = lits
            .iter()
            .map(|&l| {
                let c = self.cnf.atoms[l.unsigned_abs() as usize].clone().unwrap();
                if l > 0 { c } else { negate_con(&c) }
            })
            .collect();
        let mut lo: Vec<i64> = self.bounds.iter().map(|b| b.0).collect();
        let mut hi: Vec<i64> = self.bounds.iter().map(|b| b.1).collect();
        theory_search(&cons, &mut lo, &mut hi)
    }

    /// Deletion-based minimisation of an infeasible literal set: smaller lemmas prune more of the search.
    fn minimise(&mut self, lits: Vec<i32>) -> Vec<i32> {
        let mut core = lits;
        for i in (0..core.len()).rev() {
            let mut t = core.clone();
            t.remove(i);
            if self.theory(&t).is_none() {
                core = t;
            }
        }
        core
    }

    fn dpll(&mut self, mut a: Vec<i8>) -> Option<Vec<i64>> {
        // unit propagation to a fixpoint (clauses include learnt theory lemmas)
        loop {
            let mut changed = false;
            for ci in 0..self.cnf.clauses.len() {
                let (mut unassigned, mut n_un, mut sat) = (0i32, 0, false);
                for &l in &self.cnf.clauses[ci] {
                    match a[l.unsigned_abs() as usize] {
                        0 => {
                            n_un += 1;
                            unassigned = l;
                        }
                        s if (s > 0) == (l > 0) => {
                            sat = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if sat {
                    continue;
                }
                if n_un == 0 {
                    return None;
                }
                if n_un == 1 {
                    a[unassigned.unsigned_abs() as usize] = if unassigned > 0 { 1 } else { -1 };
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // theory check on the atoms assigned so far (early pruning)
        let lits: Vec<i32> = (1..a.len())
            .filter(|&v| self.cnf.atoms[v].is_some() && a[v] != 0)
            .map(|v| v as i32 * a[v] as i32)
            .collect();
        let Some(model) = self.theory(&lits) else {
            let core = self.minimise(lits);
            self.cnf.clauses.push(core.iter().map(|l| -l).collect());
            self.stats.learnt += 1;
            return None;
        };
        // branch on a literal of the first unsatisfied clause; none left means the formula is satisfied
        let mut pick = None;
        'outer: for cl in &self.cnf.clauses {
            let mut first = None;
            for &l in cl {
                match a[l.unsigned_abs() as usize] {
                    0 => first = first.or(Some(l)),
                    s if (s > 0) == (l > 0) => continue 'outer,
                    _ => {}
                }
            }
            pick = first;
            break;
        }
        let Some(l) = pick else { return Some(model) };
        for lit in [l, -l] {
            let mut b = a.clone();
            b[lit.unsigned_abs() as usize] = if lit > 0 { 1 } else { -1 };
            if let Some(m) = self.dpll(b) {
                return Some(m);
            }
        }
        None
    }
}

/// Is there an integer point inside `bounds` satisfying `f`? Returns a model (one value per unknown).
fn solve_f(f: &F, bounds: &[(i64, i64)], stats: &mut Stats) -> Option<Vec<i64>> {
    stats.queries += 1;
    match f {
        F::True => return Some(bounds.iter().map(|b| b.0).collect()),
        F::False => return None,
        _ => {}
    }
    let mut cnf = Cnf::new();
    let root = cnf.enc(f);
    cnf.clauses.push(vec![root]);
    let n = cnf.atoms.len();
    let mut d = Dpll { cnf, bounds, stats };
    d.dpll(vec![0; n])
}

// ---------------------------------------------------------------- the object language

#[derive(Clone, Debug)]
enum Expr {
    Const(i64),
    Var(usize),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Scale(i64, Box<Expr>), // multiplication by a constant keeps everything linear
}
#[derive(Clone, Copy, Debug)]
enum Rel {
    Lt,
    Le,
    Eq,
    Ne,
    Gt,
    Ge,
}
#[derive(Clone, Debug)]
enum Cond {
    Cmp(Expr, Rel, Expr),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
}
#[derive(Clone, Debug)]
enum Stmt {
    Assign(usize, Expr),
    If(Cond, Vec<Stmt>, Vec<Stmt>),
    While(Cond, Vec<Stmt>),
    Assert(usize, Cond),
    Assume(Cond),
}

struct Program {
    name: &'static str,
    nin: usize,
    nloc: usize,
    bounds: Vec<(i64, i64)>, // input ranges
    body: Vec<Stmt>,
}

fn v(i: usize) -> Expr {
    Expr::Var(i)
}
fn k(c: i64) -> Expr {
    Expr::Const(c)
}
fn add(a: Expr, b: Expr) -> Expr {
    Expr::Add(Box::new(a), Box::new(b))
}
fn sub(a: Expr, b: Expr) -> Expr {
    Expr::Sub(Box::new(a), Box::new(b))
}
fn mul(c: i64, a: Expr) -> Expr {
    Expr::Scale(c, Box::new(a))
}
fn cmp(a: Expr, r: Rel, b: Expr) -> Cond {
    Cond::Cmp(a, r, b)
}
fn lt(a: Expr, b: Expr) -> Cond {
    cmp(a, Rel::Lt, b)
}
fn leq(a: Expr, b: Expr) -> Cond {
    cmp(a, Rel::Le, b)
}
fn eq(a: Expr, b: Expr) -> Cond {
    cmp(a, Rel::Eq, b)
}
fn ne(a: Expr, b: Expr) -> Cond {
    cmp(a, Rel::Ne, b)
}
fn gt(a: Expr, b: Expr) -> Cond {
    cmp(a, Rel::Gt, b)
}
fn ge(a: Expr, b: Expr) -> Cond {
    cmp(a, Rel::Ge, b)
}
fn cand(a: Cond, b: Cond) -> Cond {
    Cond::And(Box::new(a), Box::new(b))
}
fn cnot(a: Cond) -> Cond {
    Cond::Not(Box::new(a))
}

fn eval_expr(e: &Expr, vals: &[i64]) -> i64 {
    match e {
        Expr::Const(c) => *c,
        Expr::Var(i) => vals[*i],
        Expr::Add(a, b) => eval_expr(a, vals) + eval_expr(b, vals),
        Expr::Sub(a, b) => eval_expr(a, vals) - eval_expr(b, vals),
        Expr::Scale(c, a) => c * eval_expr(a, vals),
    }
}
fn eval_cond(c: &Cond, vals: &[i64]) -> bool {
    match c {
        Cond::Cmp(a, r, b) => {
            let (x, y) = (eval_expr(a, vals), eval_expr(b, vals));
            match r {
                Rel::Lt => x < y,
                Rel::Le => x <= y,
                Rel::Eq => x == y,
                Rel::Ne => x != y,
                Rel::Gt => x > y,
                Rel::Ge => x >= y,
            }
        }
        Cond::And(a, b) => eval_cond(a, vals) && eval_cond(b, vals),
        Cond::Or(a, b) => eval_cond(a, vals) || eval_cond(b, vals),
        Cond::Not(a) => !eval_cond(a, vals),
    }
}
fn eval_lin(e: &Expr, vars: &[Lin]) -> Lin {
    match e {
        Expr::Const(c) => Lin::konst(*c),
        Expr::Var(i) => vars[*i].clone(),
        Expr::Add(a, b) => eval_lin(a, vars).add(&eval_lin(b, vars), 1),
        Expr::Sub(a, b) => eval_lin(a, vars).add(&eval_lin(b, vars), -1),
        Expr::Scale(c, a) => eval_lin(a, vars).scale(*c),
    }
}
/// Condition -> formula over the current symbolic values; `pos == false` yields the negation directly (De Morgan, relation flip).
fn to_f(c: &Cond, vars: &[Lin], pos: bool) -> F {
    match c {
        Cond::Cmp(a, r, b) => {
            let d = eval_lin(a, vars).add(&eval_lin(b, vars), -1);
            let r = if pos {
                *r
            } else {
                match r {
                    Rel::Lt => Rel::Ge,
                    Rel::Le => Rel::Gt,
                    Rel::Eq => Rel::Ne,
                    Rel::Ne => Rel::Eq,
                    Rel::Gt => Rel::Le,
                    Rel::Ge => Rel::Lt,
                }
            };
            let one = Lin::konst(1);
            match r {
                Rel::Lt => le(d.add(&one, 1)),
                Rel::Le => le(d),
                Rel::Gt => le(d.scale(-1).add(&one, 1)),
                Rel::Ge => le(d.scale(-1)),
                Rel::Eq => eq0(&d),
                Rel::Ne => f_or(vec![le(d.add(&one, 1)), le(d.scale(-1).add(&one, 1))]),
            }
        }
        Cond::And(a, b) => {
            let (x, y) = (to_f(a, vars, pos), to_f(b, vars, pos));
            if pos {
                f_and(vec![x, y])
            } else {
                f_or(vec![x, y])
            }
        }
        Cond::Or(a, b) => {
            let (x, y) = (to_f(a, vars, pos), to_f(b, vars, pos));
            if pos {
                f_or(vec![x, y])
            } else {
                f_and(vec![x, y])
            }
        }
        Cond::Not(a) => to_f(a, vars, !pos),
    }
}

// ---------------------------------------------------------------- concrete interpreter (the oracle)

#[derive(Debug, PartialEq, Clone, Copy)]
enum Res {
    Pass,
    Fail(usize),
    Pruned, // an assume failed
    OutOfFuel,
}

fn run_concrete(p: &Program, input: &[i64]) -> Res {
    let mut vals = input.to_vec();
    vals.resize(p.nin + p.nloc, 0);
    let mut fuel = 100_000;
    exec_concrete(&p.body, &mut vals, &mut fuel).unwrap_or(Res::Pass)
}
fn exec_concrete(stmts: &[Stmt], vals: &mut Vec<i64>, fuel: &mut i32) -> Option<Res> {
    for s in stmts {
        match s {
            Stmt::Assign(d, e) => vals[*d] = eval_expr(e, vals),
            Stmt::If(c, t, e) => {
                let r = if eval_cond(c, vals) {
                    exec_concrete(t, vals, fuel)
                } else {
                    exec_concrete(e, vals, fuel)
                };
                if r.is_some() {
                    return r;
                }
            }
            Stmt::While(c, body) => {
                while eval_cond(c, vals) {
                    *fuel -= 1;
                    if *fuel < 0 {
                        return Some(Res::OutOfFuel);
                    }
                    if let Some(r) = exec_concrete(body, vals, fuel) {
                        return Some(r);
                    }
                }
            }
            Stmt::Assert(id, c) => {
                if !eval_cond(c, vals) {
                    return Some(Res::Fail(*id));
                }
            }
            Stmt::Assume(c) => {
                if !eval_cond(c, vals) {
                    return Some(Res::Pruned);
                }
            }
        }
    }
    None
}

/// Ground truth: which assertions can fail on SOME input of the box (exhaustive).
fn brute_force_failures(p: &Program) -> BTreeSet<usize> {
    let mut failing = BTreeSet::new();
    let mut input: Vec<i64> = p.bounds.iter().map(|b| b.0).collect();
    loop {
        if let Res::Fail(id) = run_concrete(p, &input) {
            failing.insert(id);
        }
        let mut i = 0;
        loop {
            if i == input.len() {
                return failing;
            }
            if input[i] < p.bounds[i].1 {
                input[i] += 1;
                break;
            }
            input[i] = p.bounds[i].0;
            i += 1;
        }
    }
}

// ---------------------------------------------------------------- the symbolic engine

#[derive(Clone)]
struct State {
    vars: Vec<Lin>,
    pc: Vec<F>,
    dirs: Vec<bool>, // direction of every non-constant decision, to compare path sets with the concolic driver
}

type PathKey = (Vec<bool>, Option<usize>); // (decisions, failing assertion if any)

struct Engine<'a> {
    prog: &'a Program,
    bounds: Vec<(i64, i64)>, // inputs first, then fresh merge variables
    merge: bool,
    loop_bound: usize,
    stats: Stats,
    forks: usize,
    bound_hits: usize,
    paths: BTreeSet<PathKey>,
    bugs: BTreeMap<usize, Vec<i64>>,
    merges: usize,
    max_pc_nodes: usize, // largest merged path condition, in formula nodes
    /// merge variable -> its `guard => v = value` constraints. They DEFINE `v` and restrict nothing else (the guards are
    /// mutually exclusive), so they live outside the path conditions: an outer merge must not copy them into its guards.
    defs: BTreeMap<usize, Vec<F>>,
}

impl<'a> Engine<'a> {
    fn new(prog: &'a Program, merge: bool, loop_bound: usize) -> Self {
        Engine {
            prog,
            bounds: prog.bounds.clone(),
            merge,
            loop_bound,
            stats: Stats::default(),
            forks: 0,
            bound_hits: 0,
            paths: BTreeSet::new(),
            bugs: BTreeMap::new(),
            merges: 0,
            max_pc_nodes: 0,
            defs: BTreeMap::new(),
        }
    }

    fn sat(&mut self, pc: &[F], extra: F) -> Option<Vec<i64>> {
        let mut all = pc.to_vec();
        all.push(extra);
        // cone of influence: add the definitions of every merge variable the query mentions, directly or through another
        let mut mentioned = BTreeSet::new();
        for f in &all {
            f_vars(f, &mut mentioned);
        }
        let mut work: Vec<usize> = mentioned.iter().copied().collect();
        while let Some(v) = work.pop() {
            for d in self.defs.get(&v).into_iter().flatten() {
                let mut more = BTreeSet::new();
                f_vars(d, &mut more);
                work.extend(more.into_iter().filter(|&w| mentioned.insert(w)));
                all.push(d.clone());
            }
        }
        solve_f(&f_and(all), &self.bounds, &mut self.stats)
    }

    fn run(&mut self) {
        let p = self.prog;
        let mut vars: Vec<Lin> = (0..p.nin).map(Lin::var).collect();
        vars.extend((0..p.nloc).map(|_| Lin::konst(0)));
        let ends = self.exec_block(
            &p.body,
            State {
                vars,
                pc: vec![],
                dirs: vec![],
            },
        );
        for s in ends {
            self.paths.insert((s.dirs, None));
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt], st: State) -> Vec<State> {
        let mut cur = vec![st];
        for s in stmts {
            let mut next = vec![];
            for st in cur {
                next.extend(self.exec_stmt(s, st));
            }
            cur = next;
            if cur.is_empty() {
                break;
            }
        }
        cur
    }

    /// Split a state on formula `f`. Returns (state for f, state for not f), each only if feasible.
    fn branch(&mut self, st: State, f: F) -> (Option<State>, Option<State>) {
        match f {
            F::True => return (Some(st), None),
            F::False => return (None, Some(st)),
            _ => {}
        }
        let nf = neg(&f);
        let t_ok = self.sat(&st.pc, f.clone()).is_some();
        let e_ok = self.sat(&st.pc, nf.clone()).is_some();
        if t_ok && e_ok {
            self.forks += 1;
        }
        let mk = |mut s: State, dir: bool, c: F, both: bool| {
            s.dirs.push(dir);
            if both {
                s.pc.push(c); // when only one side is feasible the path condition already implies it
            }
            s
        };
        (
            t_ok.then(|| mk(st.clone(), true, f, t_ok && e_ok)),
            e_ok.then(|| mk(st, false, nf, t_ok && e_ok)),
        )
    }

    fn exec_stmt(&mut self, s: &Stmt, mut st: State) -> Vec<State> {
        match s {
            Stmt::Assign(d, e) => {
                st.vars[*d] = eval_lin(e, &st.vars);
                vec![st]
            }
            Stmt::If(c, t, e) => {
                let f = to_f(c, &st.vars, true);
                let (ts, es) = self.branch(st, f);
                let mut out = vec![];
                if let Some(s) = ts {
                    out.extend(self.exec_block(t, s));
                }
                if let Some(s) = es {
                    out.extend(self.exec_block(e, s));
                }
                self.maybe_merge(out)
            }
            Stmt::While(c, body) => {
                let exits = self.exec_while(c, body, st);
                self.maybe_merge(exits)
            }
            Stmt::Assume(c) => {
                let f = to_f(c, &st.vars, true);
                match f {
                    F::True => vec![st],
                    F::False => vec![],
                    _ => {
                        if self.sat(&st.pc, f.clone()).is_some() {
                            st.dirs.push(true);
                            st.pc.push(f);
                            vec![st]
                        } else {
                            vec![]
                        }
                    }
                }
            }
            Stmt::Assert(id, c) => {
                let f = to_f(c, &st.vars, true);
                let nf = neg(&f);
                let fail = if nf == F::False {
                    None
                } else {
                    self.sat(&st.pc, nf.clone())
                };
                if let Some(model) = &fail {
                    self.bugs
                        .entry(*id)
                        .or_insert_with(|| model[..self.prog.nin].to_vec());
                    let mut d = st.dirs.clone();
                    if f != F::False {
                        d.push(false);
                    }
                    self.paths.insert((d, Some(*id)));
                }
                if f == F::False {
                    return vec![];
                }
                if f == F::True {
                    return vec![st];
                }
                if self.sat(&st.pc, f.clone()).is_none() {
                    return vec![];
                }
                st.dirs.push(true);
                if fail_feasible(&fail) {
                    st.pc.push(f);
                }
                vec![st]
            }
        }
    }

    fn exec_while(&mut self, c: &Cond, body: &[Stmt], st: State) -> Vec<State> {
        let mut cur = vec![st];
        let mut exits = vec![];
        for iter in 0..=self.loop_bound {
            let mut next = vec![];
            for s in cur {
                let f = to_f(c, &s.vars, true);
                let (t, e) = self.branch(s, f);
                exits.extend(e);
                if let Some(t) = t {
                    if iter == self.loop_bound {
                        self.bound_hits += 1; // this path wants more iterations than we unroll
                    } else {
                        next.extend(self.exec_block(body, t));
                    }
                }
            }
            cur = next;
            if cur.is_empty() {
                break;
            }
            if self.merge && cur.len() >= 2 {
                cur = vec![self.merge_states(cur)];
            }
        }
        exits
    }

    fn maybe_merge(&mut self, states: Vec<State>) -> Vec<State> {
        if self.merge && states.len() >= 2 {
            vec![self.merge_states(states)]
        } else {
            states
        }
    }

    /// ite-merge: each differing variable becomes a fresh unknown constrained per incoming path condition.
    fn merge_states(&mut self, states: Vec<State>) -> State {
        self.merges += 1;
        let nv = states[0].vars.len();
        // The conjuncts the states share (everything from before they forked) are kept ONCE. Only what each state added
        // since, its suffix, goes into the disjunction and the guards. Copying whole path conditions instead mentions the old
        // one four times per two-way merge, so the formula grows as 4^merges (and DPLL's recursion depth with it).
        let mut shared = 0;
        while states
            .iter()
            .all(|s| s.pc.get(shared).is_some_and(|f| *f == states[0].pc[shared]))
        {
            shared += 1;
        }
        let guards: Vec<F> = states
            .iter()
            .map(|s| f_and(s.pc[shared..].to_vec()))
            .collect();
        // The states come from different sides of some fork, so each suffix holds a decision that excludes the others.
        assert!(
            guards.iter().all(|g| *g != F::True),
            "a state with nothing of its own cannot be told apart"
        );
        let mut pc = states[0].pc[..shared].to_vec();
        let any = f_or(guards.clone());
        if any != F::True {
            pc.push(any);
        }
        let mut out = State {
            vars: states[0].vars.clone(),
            pc,
            dirs: vec![],
        };
        for j in 0..nv {
            if states.iter().all(|s| s.vars[j] == states[0].vars[j]) {
                continue;
            }
            let (mut lo, mut hi) = (i64::MAX, i64::MIN);
            for s in &states {
                let (a, b) = s.vars[j].range(&self.bounds);
                lo = lo.min(a);
                hi = hi.max(b);
            }
            self.bounds.push((lo, hi));
            let fresh = Lin::var(self.bounds.len() - 1);
            let defs = states
                .iter()
                .zip(&guards)
                .map(|(s, guard)| f_or(vec![neg(guard), eq0(&fresh.add(&s.vars[j], -1))]))
                .collect();
            self.defs.insert(self.bounds.len() - 1, defs);
            out.vars[j] = fresh;
        }
        self.max_pc_nodes = self.max_pc_nodes.max(out.pc.iter().map(f_size).sum());
        out
    }
}

fn fail_feasible(m: &Option<Vec<i64>>) -> bool {
    m.is_some()
}

// ---------------------------------------------------------------- concolic driver (DART / SAGE generational search)

struct CRun {
    outcome: Res,
    dirs: Vec<bool>,
    trace: Vec<F>, // the constraint actually taken at each decision
}

fn concolic_run(p: &Program, input: &[i64], bound: usize) -> CRun {
    let mut vals = input.to_vec();
    vals.resize(p.nin + p.nloc, 0);
    let mut lins: Vec<Lin> = (0..p.nin).map(Lin::var).collect();
    lins.extend((0..p.nloc).map(|_| Lin::konst(0)));
    let mut run = CRun {
        outcome: Res::Pass,
        dirs: vec![],
        trace: vec![],
    };
    if let Some(o) = exec_concolic(&p.body, &mut vals, &mut lins, &mut run, bound) {
        run.outcome = o;
    }
    run
}

fn decide(run: &mut CRun, f_pos: F, taken: bool) {
    if matches!(f_pos, F::True | F::False) {
        return; // constant: not a decision, mirrors the symbolic engine
    }
    run.dirs.push(taken);
    run.trace.push(if taken { f_pos } else { neg(&f_pos) });
}

fn exec_concolic(
    stmts: &[Stmt],
    vals: &mut Vec<i64>,
    lins: &mut Vec<Lin>,
    run: &mut CRun,
    bound: usize,
) -> Option<Res> {
    for s in stmts {
        match s {
            Stmt::Assign(d, e) => {
                let (nv, nl) = (eval_expr(e, vals), eval_lin(e, lins));
                vals[*d] = nv;
                lins[*d] = nl;
            }
            Stmt::If(c, t, e) => {
                let taken = eval_cond(c, vals);
                decide(run, to_f(c, lins, true), taken);
                let r = if taken {
                    exec_concolic(t, vals, lins, run, bound)
                } else {
                    exec_concolic(e, vals, lins, run, bound)
                };
                if r.is_some() {
                    return r;
                }
            }
            Stmt::While(c, body) => {
                let mut iter = 0;
                loop {
                    let taken = eval_cond(c, vals);
                    decide(run, to_f(c, lins, true), taken);
                    if !taken {
                        break;
                    }
                    if iter == bound {
                        return Some(Res::OutOfFuel); // truncated path, same rule as the symbolic bound
                    }
                    iter += 1;
                    if let Some(r) = exec_concolic(body, vals, lins, run, bound) {
                        return Some(r);
                    }
                }
            }
            Stmt::Assert(id, c) => {
                let f = to_f(c, lins, true);
                let ok = eval_cond(c, vals);
                decide(run, f, ok);
                if !ok {
                    return Some(Res::Fail(*id));
                }
            }
            Stmt::Assume(c) => {
                let f = to_f(c, lins, true);
                let ok = eval_cond(c, vals);
                decide(run, f, ok);
                if !ok {
                    return Some(Res::Pruned);
                }
            }
        }
    }
    None
}

struct Dart {
    runs: usize,
    found_at: BTreeMap<usize, usize>, // assertion -> number of the run that first failed it
    paths: BTreeSet<PathKey>,
    bugs: BTreeMap<usize, Vec<i64>>,
}

/// GENERATIONAL SEARCH: an input made by negating decision i carries the bound i + 1 and only negates decisions from there
/// on. Earlier ones were already expanded by an ancestor; negating them again would only re-run covered paths. The solver
/// and the symbolic shadow are exact here, so every child follows the predicted prefix and no path is ever run twice.
fn dart(p: &Program, bound: usize, start: Vec<i64>) -> Dart {
    let mut stats = Stats::default();
    let mut out = Dart {
        runs: 0,
        found_at: BTreeMap::new(),
        paths: BTreeSet::new(),
        bugs: BTreeMap::new(),
    };
    let mut queue = vec![(start, 0usize, None::<Vec<bool>>)]; // input, generation bound, predicted decision prefix
    let mut seen_paths: BTreeSet<Vec<bool>> = BTreeSet::new();
    while let Some((input, first, predicted)) = queue.pop() {
        let r = concolic_run(p, &input, bound);
        out.runs += 1;
        if let Some(prefix) = predicted {
            assert!(
                r.dirs.starts_with(&prefix),
                "{}: divergence, {input:?} left the predicted path",
                p.name
            );
        }
        assert!(
            seen_paths.insert(r.dirs.clone()),
            "{}: path run twice by {input:?}",
            p.name
        );
        match r.outcome {
            Res::Pass => {
                out.paths.insert((r.dirs.clone(), None));
            }
            Res::Fail(id) => {
                out.bugs.entry(id).or_insert_with(|| input.clone());
                out.found_at.entry(id).or_insert(out.runs);
                out.paths.insert((r.dirs.clone(), Some(id)));
            }
            _ => {}
        }
        for i in first..r.trace.len() {
            let mut cs: Vec<F> = r.trace[..i].to_vec();
            cs.push(neg(&r.trace[i]));
            if let Some(m) = solve_f(&f_and(cs), &p.bounds, &mut stats) {
                let mut prefix = r.dirs[..=i].to_vec();
                prefix[i] = !prefix[i];
                queue.push((m, i + 1, Some(prefix)));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- the program suite

fn prog(name: &'static str, nloc: usize, bounds: &[(i64, i64)], body: Vec<Stmt>) -> Program {
    Program {
        name,
        nin: bounds.len(),
        nloc,
        bounds: bounds.to_vec(),
        body,
    }
}
fn asg(d: usize, e: Expr) -> Stmt {
    Stmt::Assign(d, e)
}
fn iff(c: Cond, t: Vec<Stmt>, e: Vec<Stmt>) -> Stmt {
    Stmt::If(c, t, e)
}
fn assert_(id: usize, c: Cond) -> Stmt {
    Stmt::Assert(id, c)
}

/// Inputs x=v(0), y=v(1), z=v(2); locals follow.
fn suite() -> Vec<(Program, bool)> {
    let b3 = [(-8, 8), (-8, 8), (-8, 8)];
    let (x, y, z) = (|| v(0), || v(1), || v(2));
    let buggy = vec![
        prog(
            "off_by_one",
            1,
            &b3,
            vec![
                iff(
                    leq(x(), k(5)),
                    vec![asg(3, add(x(), k(1)))],
                    vec![asg(3, x())],
                ),
                assert_(0, gt(v(3), x())),
            ],
        ),
        prog(
            "magic_value",
            0,
            &b3,
            vec![assert_(0, ne(mul(2, x()), k(14)))],
        ),
        prog(
            "nested_eq",
            0,
            &b3,
            vec![iff(
                gt(x(), k(0)),
                vec![iff(
                    gt(y(), x()),
                    vec![assert_(0, ne(z(), add(x(), y())))],
                    vec![],
                )],
                vec![],
            )],
        ),
        prog(
            "wrong_min",
            1,
            &b3,
            vec![
                iff(lt(x(), y()), vec![asg(3, y())], vec![asg(3, x())]),
                assert_(0, leq(v(3), x())),
                assert_(1, leq(v(3), y())),
            ],
        ),
        prog(
            "loop_sum",
            2,
            &b3,
            vec![
                Stmt::While(
                    lt(v(4), x()),
                    vec![asg(3, add(v(3), v(4))), asg(4, add(v(4), k(1)))],
                ),
                assert_(0, ne(v(3), k(10))),
            ],
        ),
        prog(
            "linear_eq",
            0,
            &b3,
            vec![assert_(0, ne(add(mul(3, x()), mul(2, y())), k(7)))],
        ),
        prog(
            "assume_gap",
            1,
            &b3,
            vec![
                Stmt::Assume(gt(x(), y())),
                asg(3, sub(x(), y())),
                assert_(0, gt(v(3), k(1))),
            ],
        ),
        prog(
            "clamp_bug",
            0,
            &b3,
            vec![
                iff(gt(x(), k(5)), vec![asg(0, k(5))], vec![]),
                iff(lt(x(), k(-5)), vec![asg(0, k(-5))], vec![]),
                assert_(0, lt(x(), k(5))),
            ],
        ),
        prog(
            "two_asserts",
            1,
            &b3,
            vec![
                iff(
                    gt(x(), y()),
                    vec![asg(3, sub(x(), y()))],
                    vec![asg(3, sub(y(), x()))],
                ),
                assert_(0, ge(v(3), k(0))),
                assert_(1, ne(v(3), k(3))),
            ],
        ),
        prog(
            "countdown",
            2,
            &b3,
            vec![
                asg(3, x()),
                Stmt::While(
                    gt(v(3), k(0)),
                    vec![asg(3, sub(v(3), k(2))), asg(4, add(v(4), k(1)))],
                ),
                assert_(0, ne(v(4), k(3))),
            ],
        ),
        prog(
            "wrap_sim",
            1,
            &b3,
            vec![
                iff(
                    gt(add(x(), y()), k(12)),
                    vec![asg(3, sub(add(x(), y()), k(16)))],
                    vec![asg(3, add(x(), y()))],
                ),
                assert_(0, ge(v(3), k(-8))),
            ],
        ),
        prog(
            "deep_sum",
            0,
            &b3,
            vec![iff(
                gt(x(), k(0)),
                vec![iff(
                    gt(y(), k(0)),
                    vec![iff(
                        gt(z(), k(0)),
                        vec![assert_(0, ne(add(add(x(), y()), z()), k(12)))],
                        vec![],
                    )],
                    vec![],
                )],
                vec![],
            )],
        ),
    ];
    let safe = vec![
        prog(
            "abs_nonneg",
            1,
            &b3,
            vec![
                iff(
                    lt(x(), k(0)),
                    vec![asg(3, sub(k(0), x()))],
                    vec![asg(3, x())],
                ),
                assert_(0, ge(v(3), k(0))),
            ],
        ),
        prog(
            "max_ge",
            1,
            &b3,
            vec![
                iff(gt(x(), y()), vec![asg(3, x())], vec![asg(3, y())]),
                assert_(0, ge(v(3), x())),
                assert_(1, ge(v(3), y())),
            ],
        ),
        prog(
            "clamp_ok",
            0,
            &b3,
            vec![
                iff(gt(x(), k(5)), vec![asg(0, k(5))], vec![]),
                assert_(0, leq(x(), k(5))),
            ],
        ),
        prog(
            "swap",
            3,
            &b3,
            vec![
                asg(3, x()),
                asg(4, y()),
                asg(5, x()),
                asg(0, y()),
                asg(1, v(5)),
                assert_(0, cand(eq(x(), v(4)), eq(y(), v(3)))),
            ],
        ),
        prog(
            "loop_const",
            2,
            &b3,
            vec![
                Stmt::While(
                    lt(v(4), k(4)),
                    vec![asg(3, add(v(3), x())), asg(4, add(v(4), k(1)))],
                ),
                assert_(0, eq(v(3), mul(4, x()))),
            ],
        ),
        prog(
            "assume_sum",
            0,
            &b3,
            vec![
                Stmt::Assume(ge(x(), k(0))),
                Stmt::Assume(ge(y(), k(0))),
                assert_(0, ge(add(x(), y()), x())),
            ],
        ),
    ];
    buggy
        .into_iter()
        .map(|p| (p, true))
        .chain(safe.into_iter().map(|p| (p, false)))
        .collect()
}

// ---------------------------------------------------------------- tests and demos

fn gen_atom(rng: &mut Rng, nv: usize) -> F {
    let mut l = Lin::konst(rng.range(-4, 4));
    for _ in 0..(1 + rng.below(3)) {
        l = l.add(
            &Lin::var(rng.below(nv as u64) as usize).scale(rng.range(-2, 2)),
            1,
        );
    }
    if rng.chance(25) { eq0(&l) } else { le(l) }
}
fn gen_f(rng: &mut Rng, nv: usize, depth: u32) -> F {
    if depth == 0 || rng.chance(25) {
        return gen_atom(rng, nv);
    }
    let kids: Vec<F> = (0..(2 + rng.below(2)))
        .map(|_| gen_f(rng, nv, depth - 1))
        .collect();
    let f = if rng.chance(50) {
        f_and(kids)
    } else {
        f_or(kids)
    };
    if rng.chance(20) { neg(&f) } else { f }
}

fn test_solver_vs_brute_force() {
    let mut rng = Rng(0x5a7_5017_e4);
    let bounds = vec![(-3i64, 3i64); 3];
    let mut stats = Stats::default();
    let (mut sat_count, mut unsat_count) = (0, 0);
    for _ in 0..3000 {
        let f = gen_f(&mut rng, 3, 3);
        let mut brute = false;
        'o: for a in -3..=3 {
            for b in -3..=3 {
                for c in -3..=3 {
                    if eval_f(&f, &[a, b, c]) {
                        brute = true;
                        break 'o;
                    }
                }
            }
        }
        let got = solve_f(&f, &bounds, &mut stats);
        assert_eq!(
            got.is_some(),
            brute,
            "solver disagrees with brute force on {f:?}"
        );
        if let Some(m) = got {
            assert!(
                m.iter().zip(&bounds).all(|(x, b)| b.0 <= *x && *x <= b.1),
                "model outside the box"
            );
            assert!(eval_f(&f, &m), "model does not satisfy the formula");
            sat_count += 1;
        } else {
            unsat_count += 1;
        }
    }
    println!(
        "solver: 3000 random constraint sets agree with brute force over a 7^3 box ({sat_count} sat, {unsat_count} unsat); {} theory calls, {} learnt lemmas",
        stats.theory_calls, stats.learnt
    );
    assert!(
        sat_count > 500 && unsat_count > 500,
        "generator should produce both outcomes"
    );
}

// Random programs: inputs v0, v1; data locals v2, v3; v4 and v5 count the loops at nesting depth 0 and 1.
fn gen_linear(rng: &mut Rng) -> Expr {
    // lean on the inputs: a condition over the zero-initialised locals alone is constant and never forks
    let leaf = |rng: &mut Rng| match rng.below(20) {
        0..=9 => v(rng.below(2) as usize),
        10..=15 => v(2 + rng.below(2) as usize),
        _ => k(rng.range(-3, 3)),
    };
    let a = leaf(rng);
    match rng.below(4) {
        0 => add(a, leaf(rng)),
        1 => sub(a, leaf(rng)),
        2 => mul(rng.range(-2, 2), a),
        _ => a,
    }
}
fn gen_cond(rng: &mut Rng) -> Cond {
    let atom = |rng: &mut Rng| {
        let rel = [Rel::Lt, Rel::Le, Rel::Eq, Rel::Ne, Rel::Gt, Rel::Ge][rng.below(6) as usize];
        cmp(gen_linear(rng), rel, gen_linear(rng))
    };
    let a = atom(rng);
    match rng.below(10) {
        0 => cand(a, atom(rng)),
        1 => Cond::Or(Box::new(a), Box::new(atom(rng))),
        2 => cnot(a),
        _ => a,
    }
}
fn gen_block(rng: &mut Rng, depth: usize, next_assert: &mut usize) -> Vec<Stmt> {
    let mut out = vec![];
    for _ in 0..(1 + rng.below(3)) {
        match rng.below(12) {
            0..=2 if depth < 2 => {
                let c = gen_cond(rng);
                let then = gen_block(rng, depth + 1, next_assert);
                out.push(iff(c, then, gen_block(rng, depth + 1, next_assert)));
            }
            3 if depth < 2 => {
                let counter = 4 + depth;
                let limit = if rng.chance(70) {
                    v(rng.below(2) as usize)
                } else {
                    k(rng.range(1, 3))
                };
                let mut body = gen_block(rng, depth + 1, next_assert);
                body.push(asg(counter, add(v(counter), k(1))));
                out.push(asg(counter, k(0)));
                out.push(Stmt::While(lt(v(counter), limit), body));
            }
            4..=5 => {
                out.push(assert_(*next_assert, gen_cond(rng)));
                *next_assert += 1;
            }
            6 => out.push(Stmt::Assume(gen_cond(rng))),
            _ => out.push(asg(2 + rng.below(2) as usize, gen_linear(rng))),
        }
    }
    out
}

fn test_random_programs() {
    let mut rng = Rng(0x0000_d1ff_0824);
    let n = 400;
    let (mut failing, mut merges, mut paths) = (0, 0, 0);
    for case in 0..n {
        let mut asserts = 0;
        let mut body = gen_block(&mut rng, 0, &mut asserts);
        body.extend(gen_block(&mut rng, 0, &mut asserts));
        body.extend(gen_block(&mut rng, 0, &mut asserts));
        body.push(assert_(asserts, gen_cond(&mut rng)));
        let p = prog("random", 4, &[(-3, 3), (-3, 3)], body);
        let truth = brute_force_failures(&p);
        let mut fork = Engine::new(&p, false, 4);
        fork.run();
        let mut merged = Engine::new(&p, true, 4);
        merged.run();
        assert_eq!(
            (fork.bound_hits, merged.bound_hits),
            (0, 0),
            "case {case}: loop limits never exceed 3"
        );
        for (name, bugs) in [("fork", &fork.bugs), ("merge", &merged.bugs)] {
            assert_eq!(
                bugs.keys().copied().collect::<BTreeSet<_>>(),
                truth,
                "case {case}: {name} engine: {:?}",
                p.body
            );
            replay(&p, bugs);
        }
        let d = dart(&p, 4, vec![0, 0]);
        // a seed run that fails an `assume` still reaches the rest through the negated decision
        assert_eq!(
            d.bugs.keys().copied().collect::<BTreeSet<_>>(),
            truth,
            "case {case}: concolic: {:?}",
            p.body
        );
        replay(&p, &d.bugs);
        assert_eq!(
            d.paths, fork.paths,
            "case {case}: path sets differ: {:?}",
            p.body
        );
        failing += usize::from(!truth.is_empty());
        merges += merged.merges;
        paths += fork.paths.len();
    }
    println!(
        "random programs: {n} (ifs, input-bounded loops, assume): fork, merge, concolic = exhaustive execution; {failing} have a failing assert, {paths} paths, {merges} merges"
    );
    assert!(
        failing > n / 5 && failing < n && merges > n,
        "generator too weak: {failing} failing, {merges} merges"
    );
}

fn replay(p: &Program, bugs: &BTreeMap<usize, Vec<i64>>) {
    for (id, input) in bugs {
        assert!(
            input
                .iter()
                .zip(&p.bounds)
                .all(|(x, b)| b.0 <= *x && *x <= b.1),
            "{}: counterexample outside bounds",
            p.name
        );
        assert_eq!(
            run_concrete(p, input),
            Res::Fail(*id),
            "{}: counterexample {input:?} does not fail assert {id} when replayed",
            p.name
        );
    }
}

fn test_suite() {
    println!(
        "suite (loop bound 12): program | can fail | fork: paths/forks/queries | merge: states/queries | concolic runs"
    );
    let mut found_in_buggy = 0;
    for (p, is_buggy) in suite() {
        let truth = brute_force_failures(&p);
        assert_eq!(
            !truth.is_empty(),
            is_buggy,
            "{}: suite labelling is wrong",
            p.name
        );

        let mut fork = Engine::new(&p, false, 12);
        fork.run();
        let mut merged = Engine::new(&p, true, 12);
        merged.run();
        let fork_found: BTreeSet<usize> = fork.bugs.keys().copied().collect();
        let merge_found: BTreeSet<usize> = merged.bugs.keys().copied().collect();
        assert_eq!(
            fork_found, truth,
            "{}: fork-mode symbolic execution",
            p.name
        );
        assert_eq!(
            merge_found, truth,
            "{}: merge-mode symbolic execution",
            p.name
        );
        assert_eq!(
            fork.bound_hits, 0,
            "{}: loop bound too small for a proof",
            p.name
        );
        replay(&p, &fork.bugs);
        replay(&p, &merged.bugs);

        let start: Vec<i64> = p.bounds.iter().map(|b| (b.0 + b.1) / 2).collect();
        let d = dart(&p, 12, start);
        let dart_found: BTreeSet<usize> = d.bugs.keys().copied().collect();
        assert_eq!(dart_found, truth, "{}: concolic driver", p.name);
        replay(&p, &d.bugs);
        assert_eq!(
            d.paths, fork.paths,
            "{}: concolic and symbolic path sets differ",
            p.name
        );

        if is_buggy {
            found_in_buggy += 1;
        }
        println!(
            "  {:<12} {:<8} | {:>3} paths {:>3} forks {:>4} q | {:>2} merges {:>4} q | {:>3} runs",
            p.name,
            format!("{:?}", truth),
            fork.paths.len(),
            fork.forks,
            fork.stats.queries,
            merged.merges,
            merged.stats.queries,
            d.runs
        );
    }
    assert_eq!(found_in_buggy, 12);
    println!(
        "  all 12 buggy programs: every failing assertion found by all three engines and replayed; 6 safe programs: no report, 0 bound hits (proved over the whole input box)"
    );
}

/// Path explosion: k independent branches give 2^k paths; merging collapses them to one state.
fn demo_path_explosion() {
    println!(
        "path explosion (8 inputs in {{-1,0,1}}, count how many are positive, assert count != 5):"
    );
    let n = 8;
    let mut body = vec![];
    for i in 0..n {
        body.push(iff(gt(v(i), k(0)), vec![asg(n, add(v(n), k(1)))], vec![]));
    }
    body.push(assert_(0, ne(v(n), k(5))));
    let p = prog("count_positive", 1, &vec![(-1, 1); n], body);
    let truth = brute_force_failures(&p);
    let mut fork = Engine::new(&p, false, 4);
    fork.run();
    let mut merged = Engine::new(&p, true, 4);
    merged.run();
    println!(
        "  fork:  {} paths, {} forks, {} solver queries, {} theory calls",
        fork.paths.len(),
        fork.forks,
        fork.stats.queries,
        fork.stats.theory_calls
    );
    println!(
        "  merge: {} merges, {} fresh unknowns, {} solver queries, {} theory calls",
        merged.merges,
        merged.bounds.len() - n,
        merged.stats.queries,
        merged.stats.theory_calls
    );
    assert_eq!(truth, BTreeSet::from([0]));
    assert_eq!(fork.bugs.keys().copied().collect::<BTreeSet<_>>(), truth);
    assert_eq!(merged.bugs.keys().copied().collect::<BTreeSet<_>>(), truth);
    assert!(fork.paths.len() > 200, "expected about 2^8 paths");
    println!(
        "  largest merged path condition: {} formula nodes after {} merges",
        merged.max_pc_nodes, merged.merges
    );
    assert!(
        merged.max_pc_nodes <= 40 * n,
        "merged path conditions must grow linearly, got {}",
        merged.max_pc_nodes
    );
    replay(&p, &fork.bugs);
    replay(&p, &merged.bugs);
}

/// Concolic finds a needle that random testing misses.
fn demo_concolic_needle() {
    let p = prog(
        "needle",
        0,
        &[(-1000, 1000), (-1000, 1000)],
        vec![iff(
            eq(v(0), k(777)),
            vec![iff(
                eq(v(1), k(-333)),
                vec![assert_(0, ne(k(0), k(0)))],
                vec![],
            )],
            vec![],
        )],
    );
    let d = dart(&p, 4, vec![0, 0]);
    assert_eq!(d.bugs.get(&0), Some(&vec![777, -333]));
    replay(&p, &d.bugs);
    let mut rng = Rng(99);
    let mut random_hits = 0;
    for _ in 0..50_000 {
        let input = [rng.range(-1000, 1000), rng.range(-1000, 1000)];
        if matches!(run_concrete(&p, &input), Res::Fail(_)) {
            random_hits += 1;
        }
    }
    println!(
        "concolic needle: x==777 && y==-333 in a 2001x2001 box: found on run {} of {} (seed input (0,0)); 50000 random inputs hit it {} times",
        d.found_at[&0], d.runs, random_hits
    );
    assert_eq!(
        (d.found_at[&0], d.runs),
        (3, 3),
        "three feasible paths, one run each"
    );
}

/// A bounded "no bug" is only as good as the loop bound.
fn demo_loop_bound() {
    let p = prog(
        "loop_sum_wide",
        2,
        &[(0, 12)],
        vec![
            Stmt::While(
                lt(v(2), v(0)),
                vec![asg(1, add(v(1), v(2))), asg(2, add(v(2), k(1)))],
            ),
            assert_(0, ne(v(1), k(45))), // 0+1+...+9 = 45, needs x = 10 (10 iterations)
        ],
    );
    let truth = brute_force_failures(&p);
    println!("loop bound: assertion can fail = {truth:?} (needs 10 iterations)");
    for bound in [4usize, 9, 10, 12] {
        let mut e = Engine::new(&p, false, bound);
        e.run();
        let found = e.bugs.contains_key(&0);
        println!(
            "  bound {bound:>2}: bug found {found:<5} bound hits {}",
            e.bound_hits
        );
        assert_eq!(found, bound >= 10);
        assert_eq!(
            e.bound_hits > 0,
            bound < 12,
            "bound hits are the honest signal that the result is not a proof"
        );
    }
}

fn main() {
    test_solver_vs_brute_force();
    test_suite();
    test_random_programs();
    demo_path_explosion();
    demo_concolic_needle();
    demo_loop_bound();
    println!("all symbolic execution checks passed");
}
