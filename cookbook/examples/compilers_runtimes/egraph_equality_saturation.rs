//! RECIPE 825 (compilers): e-graphs and equality saturation (egg): hash-consed e-nodes, congruence closure with deferred rebuilding, e-matching, rewrite rules, a backoff scheduler, constant folding as an analysis, and cost-based extraction.
//! LEVEL: advanced
//! Use in: optimisers that must choose between many equivalent forms without committing to a rewrite order (compilers, numeric-accuracy tools, tensor-graph optimisers, SMT solvers).   Run: cargo run --release --example egraph_equality_saturation
#![allow(dead_code)]
//!
//! THE PROBLEM: a classic optimiser applies rewrites destructively, so the ORDER matters (the phase-ordering problem):
//! `(a*2)/2` -> `a` needs a different order than `a*2` -> `a<<1` first. An e-graph keeps ALL equivalent versions at once.
//!   * E-NODE: an operator whose children are e-CLASSES (not terms). `Add(c1, c2)`.
//!   * E-CLASS: a set of e-nodes known to be equal. A whole exponential set of terms is represented in linear space.
//!   * HASH-CONSING (`memo`): canonical e-node -> class, so an identical node is never created twice.
//!   * UNION-FIND over class ids; `merge(a, b)` unions two classes (that is how a rewrite "fires").
//!   * CONGRUENCE: if a = b then f(a) = f(b). After merges the hash-cons table is stale (two nodes became identical after
//!     canonicalising their children). egg's insight is to DEFER the repair: merge freely during a rewrite phase, then call
//!     `rebuild` once, which processes a worklist of changed classes and re-canonicalises only their PARENT nodes.
//!   * E-MATCHING: find every (class, substitution) where a pattern with variables matches some e-node in the class (a
//!     backtracking search through classes, not terms).
//!   * ANALYSIS: per-class data kept up to date through merges. Here: the constant value of a class, so `(+ 1 2)` becomes
//!     class-equal to `3` (constant folding is not a rewrite rule, it is a lattice-valued fact).
//!   * SATURATION LOOP: (1) search all rules, collecting matches, (2) apply all matches, (3) rebuild; repeat until no
//!     change (SATURATED) or a budget (iterations, nodes) runs out. Associativity+commutativity+distributivity never saturate
//!     on non-trivial terms, so a BACKOFF SCHEDULER bans a rule that produces too many matches, for a growing number of
//!     iterations (the same idea as egg's `BackoffScheduler`).
//!   * EXTRACTION: choose the cheapest term in a class. With a monotone cost function (cost of a node >= sum of children),
//!     a bottom-up fixpoint (Bellman-Ford over classes: cost[c] = min over nodes) is the exact dynamic program for TREE cost,
//!     with no ILP. When shared sub-terms are paid for once (DAG cost) optimal extraction is NP-hard, so there is a second,
//!     GREEDY DAG extractor: each class remembers its whole chosen sub-DAG, a candidate node unions its children's choices and
//!     is charged once per distinct class. It is a heuristic: compared with an exhaustive branch-and-bound on small e-graphs.
//!
//! VERIFICATION (everything is checked by running):
//!   1. every rewrite rule is sound: lhs and rhs evaluate equally on random variable values (wrapping i64 semantics);
//!   2. congruence invariants after `rebuild`: random adds and merges are compared with an independent brute-force
//!      congruence closure (naive union-find, repeat to fixpoint), and the hash-cons table is canonical;
//!   3. the tree extractor matches an independent layered dynamic program (cheapest term of depth <= d, for d = 1, 2, ...);
//!      the greedy DAG extractor is never below the exhaustive DAG optimum, and a hand-built e-graph separates the two costs;
//!   4. the best cost reached on a finite (terminating) rule set does not depend on rule order;
//!   5. 2000 random expressions are saturated, extracted, and evaluated on random inputs against the original term.
//!
//! COMMON USES: the `egg` library and `egglog`; Herbie (rewrites floating point expressions for accuracy, built on egg);
//! Cranelift's mid-end (acyclic e-graphs, see the RFC below) for GVN, LICM and algebraic rewrites; tensor graph
//! superoptimisers (TASO/TENSAT-style); the Simplify prover (Detlefs, Nelson, Saxe) and SMT solvers use the same congruence
//! closure to decide equality with uninterpreted functions.
//! Read: Willsey et al. "egg: Fast and Extensible Equality Saturation" (POPL 2021) https://arxiv.org/abs/2004.03082 ;
//! https://egraphs-good.github.io/ ; https://docs.rs/egg/latest/egg/ ; https://herbie.uwplse.org/ ;
//! Cranelift e-graph RFC https://github.com/bytecodealliance/rfcs/blob/main/accepted/cranelift-egraph.md ;
//! Tate et al. "Equality Saturation: A New Approach to Optimization" (title only, not re-read here).
//! TRY NEXT:
//!   - [S] Remove `distrib` from `full_rules()` and run: predict first which demo assertions (Horner, shared factor) still
//!     hold, given that `factor` alone goes in the useful direction, and how the e-graph sizes of `demo_explosion` change.
//!   - [S] In `demo_explosion`, try `match_limit` 10, 100 and 1000: predict the node count after the last iteration for each,
//!     and whether the best cost changes.
//!   - [M] Add a `Div` op with a precondition rule `(/ (* ?a ?b) ?b) => ?a` that only fires when the analysis proves the
//!     constant `?b` is nonzero; update `apply`, `node_data`, `eval` and make the soundness test skip division by zero.
//!   - [M] `extract_dag_greedy` keeps the first child's choice when two children disagree about a shared class. Keep the
//!     cheaper one instead (or try both) and measure how often it now reaches `dag_optimal` in `test_dag_extraction`.
//!   - [A] PROJECT: combine with `ssa_register_allocation`: lower the extracted term to three-address code, count the
//!     registers needed, and make the cost function penalise register pressure instead of operator weight.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    fn small(&mut self) -> i64 {
        // mix of tiny values (to hit identities) and wide values (to exercise wrapping)
        if self.chance(70) {
            self.below(11) as i64 - 5
        } else {
            self.next() as i64
        }
    }
}

// ---------------------------------------------------------------- language

type Id = usize;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
enum Op {
    Num(i64),
    Var(u8),
    Add,
    Mul,
    Sub,
    Shl,
}

const VAR_NAMES: &str = "xyzabcdw";

impl Op {
    fn arity(self) -> usize {
        match self {
            Op::Num(_) | Op::Var(_) => 0,
            _ => 2,
        }
    }
}

/// Wrapping semantics everywhere (evaluator, constant folding, rule soundness): rewrites such as `x*2 = x<<1` and
/// distributivity hold in Z/2^64 so they stay valid even on overflow.
fn apply(op: Op, a: &[i64]) -> i64 {
    match op {
        Op::Num(c) => c,
        Op::Var(_) => panic!("apply on var"),
        Op::Add => a[0].wrapping_add(a[1]),
        Op::Mul => a[0].wrapping_mul(a[1]),
        Op::Sub => a[0].wrapping_sub(a[1]),
        Op::Shl => a[0].wrapping_shl(a[1] as u32),
    }
}

/// A concrete (tree) term.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Term {
    op: Op,
    kids: Vec<Term>,
}

impl Term {
    fn leaf(op: Op) -> Term {
        Term { op, kids: vec![] }
    }
    fn node(op: Op, a: Term, b: Term) -> Term {
        Term {
            op,
            kids: vec![a, b],
        }
    }
    fn eval(&self, env: &[i64]) -> i64 {
        match self.op {
            Op::Num(c) => c,
            Op::Var(i) => env[i as usize],
            op => {
                let args: Vec<i64> = self.kids.iter().map(|k| k.eval(env)).collect();
                apply(op, &args)
            }
        }
    }
    fn size(&self) -> usize {
        1 + self.kids.iter().map(Term::size).sum::<usize>()
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.op {
            Op::Num(c) => write!(f, "{c}"),
            Op::Var(i) => write!(f, "{}", VAR_NAMES.as_bytes()[i as usize] as char),
            op => {
                let s = match op {
                    Op::Add => "+",
                    Op::Mul => "*",
                    Op::Sub => "-",
                    _ => "<<",
                };
                write!(f, "({s} {} {})", self.kids[0], self.kids[1])
            }
        }
    }
}

// ---------------------------------------------------------------- s-expression parsing (terms and patterns)

enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

fn parse_sexp(s: &str) -> Sexp {
    let spaced = s.replace('(', " ( ").replace(')', " ) ");
    let toks: Vec<&str> = spaced.split_whitespace().collect();
    let mut pos = 0;
    let r = read_sexp(&toks, &mut pos);
    assert_eq!(pos, toks.len(), "trailing tokens in {s}");
    r
}

fn read_sexp(t: &[&str], pos: &mut usize) -> Sexp {
    let tok = t[*pos];
    *pos += 1;
    if tok == "(" {
        let mut v = vec![];
        while t[*pos] != ")" {
            v.push(read_sexp(t, pos));
        }
        *pos += 1;
        Sexp::List(v)
    } else {
        Sexp::Atom(tok.to_string())
    }
}

fn op_of(name: &str) -> Op {
    match name {
        "+" => Op::Add,
        "*" => Op::Mul,
        "-" => Op::Sub,
        "<<" => Op::Shl,
        _ => {
            if let Ok(c) = name.parse::<i64>() {
                Op::Num(c)
            } else {
                Op::Var(
                    VAR_NAMES
                        .find(name)
                        .unwrap_or_else(|| panic!("unknown atom {name}")) as u8,
                )
            }
        }
    }
}

fn parse_term(s: &str) -> Term {
    fn go(s: &Sexp) -> Term {
        match s {
            Sexp::Atom(a) => Term::leaf(op_of(a)),
            Sexp::List(l) => {
                let Sexp::Atom(h) = &l[0] else {
                    panic!("head must be an operator")
                };
                Term {
                    op: op_of(h),
                    kids: l[1..].iter().map(go).collect(),
                }
            }
        }
    }
    go(&parse_sexp(s))
}

/// A pattern: `?name` is a variable bound by e-matching; everything else is a literal operator.
#[derive(Clone, Debug)]
enum Pat {
    Var(usize),
    Node(Op, Vec<Pat>),
}

fn parse_pat(s: &str, names: &mut Vec<String>) -> Pat {
    fn go(s: &Sexp, names: &mut Vec<String>) -> Pat {
        match s {
            Sexp::Atom(a) if a.starts_with('?') => {
                let i = names.iter().position(|n| n == a).unwrap_or_else(|| {
                    names.push(a.clone());
                    names.len() - 1
                });
                Pat::Var(i)
            }
            Sexp::Atom(a) => Pat::Node(op_of(a), vec![]),
            Sexp::List(l) => {
                let Sexp::Atom(h) = &l[0] else {
                    panic!("head must be an operator")
                };
                Pat::Node(op_of(h), l[1..].iter().map(|k| go(k, names)).collect())
            }
        }
    }
    go(&parse_sexp(s), names)
}

fn eval_pat(p: &Pat, vals: &[i64]) -> i64 {
    match p {
        Pat::Var(i) => vals[*i],
        Pat::Node(op, kids) if kids.is_empty() => match op {
            Op::Num(c) => *c,
            _ => panic!("pattern leaf must be a number"),
        },
        Pat::Node(op, kids) => {
            let args: Vec<i64> = kids.iter().map(|k| eval_pat(k, vals)).collect();
            apply(*op, &args)
        }
    }
}

struct Rewrite {
    name: &'static str,
    lhs: Pat,
    rhs: Pat,
    nvars: usize,
}

fn rule(name: &'static str, lhs: &str, rhs: &str) -> Rewrite {
    let mut names = vec![];
    let l = parse_pat(lhs, &mut names);
    let n = names.len();
    let r = parse_pat(rhs, &mut names);
    assert_eq!(
        n,
        names.len(),
        "rhs of {name} uses a variable the lhs does not bind"
    );
    Rewrite {
        name,
        lhs: l,
        rhs: r,
        nvars: n,
    }
}

fn full_rules() -> Vec<Rewrite> {
    vec![
        rule("add-comm", "(+ ?a ?b)", "(+ ?b ?a)"),
        rule("mul-comm", "(* ?a ?b)", "(* ?b ?a)"),
        rule("add-assoc", "(+ ?a (+ ?b ?c))", "(+ (+ ?a ?b) ?c)"),
        rule("add-assoc-r", "(+ (+ ?a ?b) ?c)", "(+ ?a (+ ?b ?c))"),
        rule("mul-assoc", "(* ?a (* ?b ?c))", "(* (* ?a ?b) ?c)"),
        rule("mul-assoc-r", "(* (* ?a ?b) ?c)", "(* ?a (* ?b ?c))"),
        rule("distrib", "(* ?a (+ ?b ?c))", "(+ (* ?a ?b) (* ?a ?c))"),
        rule("factor", "(+ (* ?a ?b) (* ?a ?c))", "(* ?a (+ ?b ?c))"),
        rule("add-0", "(+ ?a 0)", "?a"),
        rule("mul-1", "(* ?a 1)", "?a"),
        rule("mul-0", "(* ?a 0)", "0"),
        rule("sub-self", "(- ?a ?a)", "0"),
        rule("sub-def", "(- ?a ?b)", "(+ ?a (* ?b -1))"),
        rule("mul2-shl", "(* ?a 2)", "(<< ?a 1)"),
        rule("shl-add", "(<< ?a 1)", "(+ ?a ?a)"),
        rule("add-mul2", "(+ ?a ?a)", "(* ?a 2)"),
    ]
}

/// A rule set that cannot create unboundedly many classes: no associativity, no distributivity, no sub-def.
fn finite_rules() -> Vec<Rewrite> {
    let keep = [
        "add-comm", "mul-comm", "factor", "add-0", "mul-1", "mul-0", "sub-self", "mul2-shl",
        "shl-add", "add-mul2",
    ];
    full_rules()
        .into_iter()
        .filter(|r| keep.contains(&r.name))
        .collect()
}

// ---------------------------------------------------------------- the e-graph

#[derive(Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
struct ENode {
    op: Op,
    kids: Vec<Id>,
}

struct Class {
    nodes: Vec<ENode>,
    /// (parent e-node as it was when recorded, the class that contains it). Used by `repair` to find stale hash-cons entries.
    parents: Vec<(ENode, Id)>,
    /// The constant-folding analysis value.
    data: Option<i64>,
}

#[derive(Default)]
struct EGraph {
    uf: Vec<Id>,
    classes: BTreeMap<Id, Class>, // ordered so that the whole run is deterministic
    memo: HashMap<ENode, Id>,
    pending: Vec<Id>,
    analysis_pending: Vec<Id>,
    unions: usize,
}

type Subst = Vec<Option<Id>>;

impl EGraph {
    fn find(&self, mut i: Id) -> Id {
        while self.uf[i] != i {
            i = self.uf[i];
        }
        i
    }

    fn canon(&self, n: &ENode) -> ENode {
        ENode {
            op: n.op,
            kids: n.kids.iter().map(|&k| self.find(k)).collect(),
        }
    }

    fn node_data(&self, n: &ENode) -> Option<i64> {
        match n.op {
            Op::Num(c) => Some(c),
            Op::Var(_) => None,
            op => {
                let mut args = vec![];
                for &k in &n.kids {
                    args.push(self.classes[&self.find(k)].data?);
                }
                Some(apply(op, &args))
            }
        }
    }

    fn add(&mut self, n: ENode) -> Id {
        let n = self.canon(&n);
        if let Some(&id) = self.memo.get(&n) {
            return self.find(id);
        }
        let id = self.uf.len();
        self.uf.push(id);
        let data = self.node_data(&n);
        for &k in &n.kids {
            self.classes
                .get_mut(&k)
                .unwrap()
                .parents
                .push((n.clone(), id));
        }
        self.classes.insert(
            id,
            Class {
                nodes: vec![n.clone()],
                parents: vec![],
                data,
            },
        );
        self.memo.insert(n, id);
        if data.is_some() {
            self.modify(id);
        }
        id
    }

    /// Analysis hook: a class with a known constant must contain the literal node (so extraction can pick it).
    fn modify(&mut self, id: Id) {
        if let Some(c) = self.classes[&self.find(id)].data {
            let lit = self.add(ENode {
                op: Op::Num(c),
                kids: vec![],
            });
            self.merge(id, lit);
        }
    }

    fn add_term(&mut self, t: &Term) -> Id {
        let kids = t.kids.iter().map(|k| self.add_term(k)).collect();
        self.add(ENode { op: t.op, kids })
    }

    fn weight(&self, i: Id) -> usize {
        self.classes[&i].nodes.len() + self.classes[&i].parents.len()
    }

    fn merge(&mut self, a: Id, b: Id) -> Id {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return a;
        }
        if self.weight(a) < self.weight(b) {
            std::mem::swap(&mut a, &mut b); // union by size keeps find() paths short
        }
        self.unions += 1;
        self.uf[b] = a;
        let cb = self.classes.remove(&b).unwrap();
        let ca = self.classes.get_mut(&a).unwrap();
        let data_changed = match (ca.data, cb.data) {
            (Some(x), Some(y)) => {
                assert_eq!(
                    x, y,
                    "unsound merge: class constants {x} and {y} were unified"
                );
                false
            }
            (None, Some(y)) => {
                ca.data = Some(y);
                true
            }
            (Some(_), None) => true, // b's parents may now be foldable
            (None, None) => false,
        };
        ca.nodes.extend(cb.nodes);
        ca.parents.extend(cb.parents);
        self.pending.push(a);
        if data_changed {
            self.analysis_pending.push(a);
        }
        a
    }

    /// Restore the hash-cons/congruence invariant. Deferred: rules merge freely, then this runs once per iteration.
    fn rebuild(&mut self) {
        loop {
            if let Some(id) = self.pending.pop() {
                let id = self.find(id);
                self.repair(id);
            } else if let Some(id) = self.analysis_pending.pop() {
                self.propagate(self.find(id));
            } else {
                break;
            }
        }
        self.canonicalize_nodes();
    }

    fn repair(&mut self, id: Id) {
        let old = std::mem::take(&mut self.classes.get_mut(&id).unwrap().parents);
        for (pn, _) in &old {
            self.memo.remove(pn);
        }
        for (pn, pc) in &old {
            let pn = self.canon(pn);
            let pc = self.find(*pc);
            if let Some(existing) = self.memo.insert(pn, pc) {
                self.merge(existing, pc); // two nodes became identical: congruence
            }
        }
        let mut fresh: BTreeMap<ENode, Id> = BTreeMap::new();
        for (pn, pc) in old {
            let pn = self.canon(&pn);
            let pc = self.find(pc);
            if let Some(prev) = fresh.insert(pn, pc) {
                self.merge(prev, pc);
            }
        }
        let root = self.find(id);
        let list: Vec<(ENode, Id)> = fresh.into_iter().collect();
        self.classes.get_mut(&root).unwrap().parents.extend(list);
    }

    fn propagate(&mut self, id: Id) {
        let parents = self.classes[&id].parents.clone();
        for (pn, pc) in parents {
            let pc = self.find(pc);
            let Some(c) = self.node_data(&self.canon(&pn)) else {
                continue;
            };
            match self.classes[&pc].data {
                None => {
                    self.classes.get_mut(&pc).unwrap().data = Some(c);
                    self.analysis_pending.push(pc);
                    self.modify(pc);
                }
                Some(old) => assert_eq!(old, c, "analysis disagreement"),
            }
        }
    }

    fn canonicalize_nodes(&mut self) {
        let ids: Vec<Id> = self.classes.keys().copied().collect();
        for id in ids {
            let nodes = std::mem::take(&mut self.classes.get_mut(&id).unwrap().nodes);
            let mut v: Vec<ENode> = nodes.iter().map(|n| self.canon(n)).collect();
            v.sort();
            v.dedup();
            self.classes.get_mut(&id).unwrap().nodes = v;
        }
    }

    fn num_nodes(&self) -> usize {
        self.classes.values().map(|c| c.nodes.len()).sum()
    }

    /// Hash-cons invariant: every canonical node of every class is in `memo` and points at that very class.
    fn check_invariants(&self) {
        for (&id, c) in &self.classes {
            assert_eq!(self.find(id), id);
            for n in &c.nodes {
                assert_eq!(self.canon(n), *n, "node not canonical after rebuild");
                let got = self.memo.get(n).map(|&m| self.find(m));
                assert_eq!(got, Some(id), "memo disagrees about {n:?}");
            }
        }
    }

    // ------------------------------------------------------------ e-matching

    fn ematch(&self, p: &Pat, class: Id, s: &Subst) -> Vec<Subst> {
        match p {
            Pat::Var(v) => match s[*v] {
                Some(bound) if self.find(bound) == class => vec![s.clone()],
                Some(_) => vec![],
                None => {
                    let mut s2 = s.clone();
                    s2[*v] = Some(class);
                    vec![s2]
                }
            },
            Pat::Node(op, kids) => {
                let mut out = vec![];
                for n in &self.classes[&class].nodes {
                    if n.op != *op || n.kids.len() != kids.len() {
                        continue;
                    }
                    let mut cur = vec![s.clone()];
                    for (kp, &kc) in kids.iter().zip(&n.kids) {
                        let kc = self.find(kc);
                        let mut next = vec![];
                        for sub in &cur {
                            next.extend(self.ematch(kp, kc, sub));
                        }
                        cur = next;
                        if cur.is_empty() {
                            break;
                        }
                    }
                    out.extend(cur);
                }
                out
            }
        }
    }

    fn search(&self, r: &Rewrite) -> Vec<(Id, Subst)> {
        let mut out = vec![];
        for &id in self.classes.keys() {
            for s in self.ematch(&r.lhs, id, &vec![None; r.nvars]) {
                out.push((id, s));
            }
        }
        out
    }

    fn instantiate(&mut self, p: &Pat, s: &Subst) -> Id {
        match p {
            Pat::Var(v) => s[*v].unwrap(),
            Pat::Node(op, kids) => {
                let kids = kids.iter().map(|k| self.instantiate(k, s)).collect();
                self.add(ENode { op: *op, kids })
            }
        }
    }
}

// ---------------------------------------------------------------- saturation with a backoff scheduler

#[derive(Debug, PartialEq, Clone, Copy)]
enum Stop {
    Saturated,
    IterLimit,
    NodeLimit,
}

struct Limits {
    iters: usize,
    nodes: usize,
    match_limit: usize, // per rule per iteration, doubled each time the rule is banned
    backoff: bool,
}

struct Report {
    iters: usize,
    stop: Stop,
    history: Vec<(usize, usize)>, // (e-nodes, e-classes) after each iteration
}

fn run(eg: &mut EGraph, rules: &[Rewrite], lim: &Limits) -> Report {
    let mut banned_until = vec![0usize; rules.len()];
    let mut times_banned = vec![0u32; rules.len()];
    let mut history = vec![];
    for iter in 0..lim.iters {
        let (n0, u0) = (eg.num_nodes(), eg.unions);
        let mut all: Vec<(usize, Vec<(Id, Subst)>)> = vec![];
        let mut any_banned = false;
        for (ri, r) in rules.iter().enumerate() {
            if lim.backoff && banned_until[ri] > iter {
                any_banned = true;
                continue;
            }
            let ms = eg.search(r);
            if lim.backoff && ms.len() > (lim.match_limit << times_banned[ri]) {
                banned_until[ri] = iter + (2usize << times_banned[ri]);
                times_banned[ri] += 1;
                any_banned = true;
                continue;
            }
            all.push((ri, ms));
        }
        'apply: for (ri, ms) in all {
            for (id, s) in ms {
                let new = eg.instantiate(&rules[ri].rhs, &s);
                eg.merge(id, new);
                if eg.num_nodes() > lim.nodes * 4 {
                    break 'apply; // hard stop inside one iteration so a bad rule cannot eat all memory
                }
            }
        }
        eg.rebuild();
        history.push((eg.num_nodes(), eg.classes.len()));
        if eg.num_nodes() > lim.nodes {
            return Report {
                iters: iter + 1,
                stop: Stop::NodeLimit,
                history,
            };
        }
        if eg.num_nodes() == n0 && eg.unions == u0 {
            if any_banned {
                banned_until.iter_mut().for_each(|b| *b = 0); // nothing changed only because rules sat out: release them
            } else {
                return Report {
                    iters: iter + 1,
                    stop: Stop::Saturated,
                    history,
                };
            }
        }
    }
    Report {
        iters: lim.iters,
        stop: Stop::IterLimit,
        history,
    }
}

// ---------------------------------------------------------------- extraction

fn cost_size(_: Op) -> u64 {
    1
}

/// "Hardware" weights: multiplication is expensive (think matrix multiply in a tensor setting), shifts are cheap.
fn cost_weighted(op: Op) -> u64 {
    match op {
        Op::Num(_) | Op::Var(_) => 1,
        Op::Add | Op::Sub | Op::Shl => 2,
        Op::Mul => 10,
    }
}

/// Bottom-up fixpoint: cost[c] = min over nodes (op cost + sum of child costs). Costs are positive so a cycle through a
/// class can never be cheaper than leaving it, hence the best choice graph is acyclic and the term is finite.
fn best_choices(eg: &EGraph, cost: &dyn Fn(Op) -> u64) -> HashMap<Id, (u64, ENode)> {
    let mut best: HashMap<Id, (u64, ENode)> = HashMap::new();
    loop {
        let mut changed = false;
        for (&id, c) in &eg.classes {
            for n in &c.nodes {
                let mut total = cost(n.op);
                let mut ok = true;
                for &k in &n.kids {
                    match best.get(&eg.find(k)) {
                        Some((kc, _)) => total += kc,
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok && best.get(&id).is_none_or(|(old, _)| total < *old) {
                    best.insert(id, (total, n.clone()));
                    changed = true;
                }
            }
        }
        if !changed {
            return best;
        }
    }
}

fn build_term(eg: &EGraph, best: &HashMap<Id, (u64, ENode)>, id: Id) -> Term {
    let (_, n) = &best[&eg.find(id)];
    Term {
        op: n.op,
        kids: n.kids.iter().map(|&k| build_term(eg, best, k)).collect(),
    }
}

fn extract(eg: &EGraph, root: Id, cost: &dyn Fn(Op) -> u64) -> (u64, Term) {
    let best = best_choices(eg, cost);
    (best[&eg.find(root)].0, build_term(eg, &best, root))
}

/// Number of distinct classes the chosen term touches (its size as a DAG with sharing).
fn dag_size(eg: &EGraph, best: &HashMap<Id, (u64, ENode)>, root: Id) -> usize {
    let mut seen = HashSet::new();
    let mut stack = vec![eg.find(root)];
    while let Some(c) = stack.pop() {
        if seen.insert(c) {
            stack.extend(best[&c].1.kids.iter().map(|&k| eg.find(k)));
        }
    }
    seen.len()
}

/// Independent oracle: table[d][class] = cheapest term of depth <= d, built layer by layer (no fixpoint iteration).
fn min_cost_by_depth(eg: &EGraph, root: Id, cost: &dyn Fn(Op) -> u64) -> u64 {
    let depth = eg.classes.len() + 1;
    let mut table: HashMap<Id, u64> = HashMap::new();
    for _ in 0..depth {
        let mut next = table.clone();
        for (&id, c) in &eg.classes {
            for n in &c.nodes {
                let mut total = cost(n.op);
                let mut ok = true;
                for &k in &n.kids {
                    match table.get(&eg.find(k)) {
                        Some(v) => total += v,
                        None => ok = false,
                    }
                }
                if ok && next.get(&id).is_none_or(|&o| total < o) {
                    next.insert(id, total);
                }
            }
        }
        table = next;
    }
    table[&eg.find(root)]
}

fn term_cost(t: &Term, cost: &dyn Fn(Op) -> u64) -> u64 {
    cost(t.op) + t.kids.iter().map(|k| term_cost(k, cost)).sum::<u64>()
}

fn optimise(
    t: &Term,
    rules: &[Rewrite],
    lim: &Limits,
    cost: &dyn Fn(Op) -> u64,
) -> (Term, Report, EGraph, Id) {
    let mut eg = EGraph::default();
    let root = eg.add_term(t);
    eg.rebuild();
    let rep = run(&mut eg, rules, lim);
    let (_, out) = extract(&eg, root, cost);
    (out, rep, eg, root)
}

// ---------------------------------------------------------------- DAG-cost extraction

/// One chosen node per class: an extraction as a DAG rather than a tree.
type Choice = BTreeMap<Id, ENode>;

/// Cost of the DAG reachable from `root` (each class paid once). `None` if the choices contain a cycle.
fn dag_cost(eg: &EGraph, choice: &Choice, root: Id, cost: &dyn Fn(Op) -> u64) -> Option<u64> {
    fn go(
        eg: &EGraph,
        ch: &Choice,
        c: Id,
        cost: &dyn Fn(Op) -> u64,
        done: &mut HashMap<Id, bool>,
    ) -> Option<u64> {
        match done.get(&c) {
            Some(true) => return Some(0), // already paid for
            Some(false) => return None,   // still on the DFS stack: a cycle
            None => {}
        }
        done.insert(c, false);
        let n = &ch[&c];
        let mut total = cost(n.op);
        for &k in &n.kids {
            total += go(eg, ch, eg.find(k), cost, done)?;
        }
        done.insert(c, true);
        Some(total)
    }
    go(eg, choice, eg.find(root), cost, &mut HashMap::new())
}

fn term_of_choice(eg: &EGraph, choice: &Choice, id: Id) -> Term {
    let n = &choice[&eg.find(id)];
    Term {
        op: n.op,
        kids: n
            .kids
            .iter()
            .map(|&k| term_of_choice(eg, choice, k))
            .collect(),
    }
}

/// Greedy bottom-up DAG extraction. Terminates because a class is only updated when its cost strictly decreases.
fn extract_dag_greedy(eg: &EGraph, root: Id, cost: &dyn Fn(Op) -> u64) -> (u64, Choice) {
    let mut best: HashMap<Id, (u64, Choice)> = HashMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for (&id, c) in &eg.classes {
            for n in &c.nodes {
                if !n.kids.iter().all(|&k| best.contains_key(&eg.find(k))) {
                    continue;
                }
                let mut choice = Choice::new();
                for &k in &n.kids {
                    for (&cl, picked) in &best[&eg.find(k)].1 {
                        choice.entry(cl).or_insert_with(|| picked.clone());
                    }
                }
                if choice.contains_key(&id) {
                    continue; // a child already uses this class: choosing `n` here would close a cycle
                }
                choice.insert(id, n.clone());
                // mixing two children's choices can create a cycle or orphan an entry: `dag_cost` handles both
                let Some(total) = dag_cost(eg, &choice, id, cost) else {
                    continue;
                };
                if best.get(&id).is_none_or(|(old, _)| total < *old) {
                    best.insert(id, (total, choice));
                    changed = true;
                }
            }
        }
    }
    let (c, choice) = best.remove(&eg.find(root)).expect("root has a finite term");
    (c, choice)
}

/// Exhaustive branch and bound: choose one node for every class that becomes needed. Exponential; small e-graphs only.
fn dag_optimal(eg: &EGraph, root: Id, cost: &dyn Fn(Op) -> u64) -> u64 {
    fn go(
        eg: &EGraph,
        root: Id,
        cost: &dyn Fn(Op) -> u64,
        pending: &mut Vec<Id>,
        ch: &mut Choice,
        acc: u64,
        best: &mut u64,
    ) {
        if acc >= *best {
            return; // costs are positive, so this branch can only get worse
        }
        let Some(c) = pending.pop() else {
            if dag_cost(eg, ch, root, cost).is_some() {
                *best = acc;
            }
            return;
        };
        if ch.contains_key(&c) {
            go(eg, root, cost, pending, ch, acc, best);
        } else {
            for n in &eg.classes[&c].nodes {
                let len = pending.len();
                pending.extend(n.kids.iter().map(|&k| eg.find(k)));
                ch.insert(c, n.clone());
                go(eg, root, cost, pending, ch, acc + cost(n.op), best);
                ch.remove(&c);
                pending.truncate(len);
            }
        }
        pending.push(c);
    }
    let mut best = u64::MAX;
    go(
        eg,
        root,
        cost,
        &mut vec![eg.find(root)],
        &mut Choice::new(),
        0,
        &mut best,
    );
    best
}

// ---------------------------------------------------------------- tests

fn test_dag_extraction() {
    let mut rng = Rng(0xda9_5eed);
    let rules = finite_rules();
    let lim = Limits {
        iters: 6,
        nodes: 200,
        match_limit: 100,
        backoff: true,
    };
    let (mut checked, mut greedy_opt, mut tree_opt) = (0, 0, 0);
    while checked < 300 {
        let t = gen_term(&mut rng, 3);
        let (tree_term, _, eg, root) = optimise(&t, &rules, &lim, &cost_weighted);
        // keep the exhaustive search affordable
        let combos = eg
            .classes
            .values()
            .fold(1u64, |p, c| p.saturating_mul(c.nodes.len() as u64));
        if combos > 20_000 {
            continue;
        }
        let optimum = dag_optimal(&eg, root, &cost_weighted);
        let (greedy, choice) = extract_dag_greedy(&eg, root, &cost_weighted);
        assert_eq!(dag_cost(&eg, &choice, root, &cost_weighted), Some(greedy));
        let tree_choice: Choice = best_choices(&eg, &cost_weighted)
            .into_iter()
            .map(|(c, (_, n))| (c, n))
            .collect();
        let tree_as_dag = dag_cost(&eg, &tree_choice, root, &cost_weighted)
            .expect("tree-optimal choices are acyclic");
        assert!(
            greedy >= optimum && tree_as_dag >= optimum,
            "something beat the exhaustive optimum on {t}"
        );
        assert!(
            optimum <= term_cost(&tree_term, &cost_weighted),
            "sharing can only lower the cost"
        );
        let out = term_of_choice(&eg, &choice, root);
        for _ in 0..8 {
            let env: Vec<i64> = (0..3).map(|_| rng.small()).collect();
            assert_eq!(
                t.eval(&env),
                out.eval(&env),
                "DAG extraction changed the meaning: {t} -> {out}"
            );
        }
        greedy_opt += usize::from(greedy == optimum);
        tree_opt += usize::from(tree_as_dag == optimum);
        checked += 1;
    }
    println!(
        "dag extraction: {checked} e-graphs vs exhaustive optimum: greedy-dag optimal on {greedy_opt}, tree-DP choices (costed as a DAG) on {tree_opt}"
    );

    // Hand-built e-graph where the two cost models disagree. The final merge stands for an equality some rule proved;
    // only the costs matter here.
    let mut eg = EGraph::default();
    let shared = eg.add_term(&parse_term("(+ (* (* a b) c) (* (* a b) c))"));
    let flat = eg.add_term(&parse_term("(+ (* x y) (+ (* z d) (* w a)))"));
    eg.merge(shared, flat);
    eg.rebuild();
    let (tree_cost, tree_term) = extract(&eg, shared, &cost_weighted);
    let (dcost, choice) = extract_dag_greedy(&eg, shared, &cost_weighted);
    let dterm = term_of_choice(&eg, &choice, shared);
    println!("  tree DP picks   {tree_term}  (tree cost {tree_cost})");
    println!(
        "  greedy DAG picks {dterm}  (dag cost {dcost}, tree cost {})",
        term_cost(&dterm, &cost_weighted)
    );
    assert_eq!(tree_term, parse_term("(+ (* x y) (+ (* z d) (* w a)))"));
    assert_eq!(dterm, parse_term("(+ (* (* a b) c) (* (* a b) c))"));
    assert_eq!(dcost, dag_optimal(&eg, shared, &cost_weighted));
    assert!(dcost < tree_cost && term_cost(&dterm, &cost_weighted) > tree_cost);
}

fn test_rules_sound() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for r in full_rules() {
        for _ in 0..500 {
            let vals: Vec<i64> = (0..r.nvars).map(|_| rng.small()).collect();
            assert_eq!(
                eval_pat(&r.lhs, &vals),
                eval_pat(&r.rhs, &vals),
                "rule {} is unsound for {vals:?}",
                r.name
            );
        }
    }
    println!(
        "rules: {} rewrite rules each agree with the evaluator on 500 random bindings (wrapping i64)",
        full_rules().len()
    );
}

/// Independent congruence closure: naive union-find over the recorded nodes, repeated to a fixpoint.
fn test_congruence() {
    let mut rng = Rng(0xfeed_beef_1234_5678);
    let (mut trials, mut classes_total) = (0, 0);
    for _ in 0..300 {
        let mut eg = EGraph::default();
        let mut recorded: Vec<(Id, Op, Vec<Id>)> = vec![];
        let mut ids: Vec<Id> = vec![];
        let mut merges: Vec<(Id, Id)> = vec![];
        for v in 0..5u8 {
            let id = eg.add(ENode {
                op: Op::Var(v),
                kids: vec![],
            });
            recorded.push((id, Op::Var(v), vec![]));
            ids.push(id);
        }
        for _ in 0..(10 + rng.below(30)) {
            if rng.chance(60) {
                let op = [Op::Add, Op::Mul, Op::Sub][rng.below(3) as usize];
                let kids = vec![
                    ids[rng.below(ids.len() as u64) as usize],
                    ids[rng.below(ids.len() as u64) as usize],
                ];
                let id = eg.add(ENode {
                    op,
                    kids: kids.clone(),
                });
                recorded.push((id, op, kids));
                ids.push(id);
            } else {
                let (a, b) = (
                    ids[rng.below(ids.len() as u64) as usize],
                    ids[rng.below(ids.len() as u64) as usize],
                );
                eg.merge(a, b);
                merges.push((a, b));
            }
            if rng.chance(10) {
                eg.rebuild(); // rebuilds in the middle must not change the final answer
            }
        }
        eg.rebuild();
        eg.check_invariants();
        // oracle
        let n = eg.uf.len();
        let mut uf: Vec<usize> = (0..n).collect();
        fn f(uf: &mut Vec<usize>, mut x: usize) -> usize {
            while uf[x] != x {
                uf[x] = uf[uf[x]];
                x = uf[x];
            }
            x
        }
        for &(a, b) in &merges {
            let (ra, rb) = (f(&mut uf, a), f(&mut uf, b));
            uf[ra] = rb;
        }
        loop {
            let mut changed = false;
            for i in 0..recorded.len() {
                for j in (i + 1)..recorded.len() {
                    let (ni, nj) = (&recorded[i], &recorded[j]);
                    if ni.1 != nj.1 || ni.2.len() != nj.2.len() {
                        continue;
                    }
                    let same_kids =
                        ni.2.iter()
                            .zip(&nj.2)
                            .all(|(&x, &y)| f(&mut uf, x) == f(&mut uf, y));
                    if same_kids {
                        let (ra, rb) = (f(&mut uf, ni.0), f(&mut uf, nj.0));
                        if ra != rb {
                            uf[ra] = rb;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for a in 0..n {
            for b in a..n {
                let oracle = f(&mut uf, a) == f(&mut uf, b);
                assert_eq!(
                    oracle,
                    eg.find(a) == eg.find(b),
                    "partition differs for ids {a} {b}"
                );
            }
        }
        trials += 1;
        classes_total += eg.classes.len();
    }
    println!(
        "congruence: {trials} random add/merge/rebuild sequences match brute-force closure ({classes_total} classes total); hash-cons invariant holds"
    );
}

fn gen_term(rng: &mut Rng, depth: u32) -> Term {
    if depth == 0 || rng.chance(25) {
        return if rng.chance(55) {
            Term::leaf(Op::Var(rng.below(3) as u8))
        } else {
            Term::leaf(Op::Num([0, 1, 2, 3, -1][rng.below(5) as usize]))
        };
    }
    match rng.below(10) {
        0..=3 => Term::node(Op::Add, gen_term(rng, depth - 1), gen_term(rng, depth - 1)),
        4..=6 => Term::node(Op::Mul, gen_term(rng, depth - 1), gen_term(rng, depth - 1)),
        7..=8 => Term::node(Op::Sub, gen_term(rng, depth - 1), gen_term(rng, depth - 1)),
        _ => Term::node(
            Op::Shl,
            gen_term(rng, depth - 1),
            Term::leaf(Op::Num(rng.below(4) as i64)),
        ),
    }
}

fn test_extraction_oracle() {
    let mut rng = Rng(0x0dd_ba11_cafe);
    let rules = finite_rules();
    let lim = Limits {
        iters: 6,
        nodes: 400,
        match_limit: 100,
        backoff: true,
    };
    let mut checked = 0;
    for i in 0..150 {
        let t = gen_term(&mut rng, 2 + (i % 2));
        let (out, _, eg, root) = optimise(&t, &rules, &lim, &cost_weighted);
        let (c, _) = extract(&eg, root, &cost_weighted);
        assert_eq!(
            c,
            min_cost_by_depth(&eg, root, &cost_weighted),
            "extractor not optimal for {t}"
        );
        assert_eq!(c, term_cost(&out, &cost_weighted));
        assert!(
            c <= term_cost(&t, &cost_weighted),
            "extraction must never be worse than the input"
        );
        checked += 1;
    }
    println!(
        "extraction: bottom-up DP equals the depth-indexed enumeration oracle on {checked} e-graphs"
    );
}

fn test_order_independence() {
    let mut rng = Rng(0xabcdef_0123);
    let lim = Limits {
        iters: 12,
        nodes: 1500,
        match_limit: usize::MAX >> 8,
        backoff: false,
    };
    let (mut compared, mut skipped) = (0, 0);
    for _ in 0..150 {
        let depth = 2 + rng.below(2) as u32;
        let t = gen_term(&mut rng, depth);
        let mut costs = vec![];
        for variant in 0..3 {
            let mut rules = finite_rules();
            match variant {
                0 => {}
                1 => rules.reverse(),
                _ => {
                    for k in (1..rules.len()).rev() {
                        let j = rng.below(k as u64 + 1) as usize;
                        rules.swap(k, j);
                    }
                }
            }
            let (_, rep, eg, root) = optimise(&t, &rules, &lim, &cost_weighted);
            if rep.stop == Stop::Saturated {
                costs.push(extract(&eg, root, &cost_weighted).0);
            }
        }
        if costs.len() == 3 {
            assert!(
                costs.iter().all(|&c| c == costs[0]),
                "rule order changed the best cost for {t}: {costs:?}"
            );
            compared += 1;
        } else {
            skipped += 1;
        }
    }
    assert!(compared >= 100, "too few saturated runs: {compared}");
    println!(
        "order independence: {compared} expressions saturated under 3 rule orders with identical best cost ({skipped} skipped: not saturated)"
    );
}

fn test_semantics() {
    let mut rng = Rng(0x5eed_0000_0825);
    let rules = full_rules();
    let lim = Limits {
        iters: 4,
        nodes: 500,
        match_limit: 60,
        backoff: true,
    };
    let (mut improved, mut same, mut stops) = (0, 0, [0usize; 3]);
    let (mut before_total, mut after_total) = (0u64, 0u64);
    for _ in 0..2000 {
        let t = gen_term(&mut rng, 3);
        let (out, rep, _, _) = optimise(&t, &rules, &lim, &cost_weighted);
        for _ in 0..8 {
            let env: Vec<i64> = (0..3).map(|_| rng.small()).collect();
            assert_eq!(
                t.eval(&env),
                out.eval(&env),
                "extracted term differs: {t}  ->  {out}  at {env:?}"
            );
        }
        let (cb, ca) = (
            term_cost(&t, &cost_weighted),
            term_cost(&out, &cost_weighted),
        );
        assert!(ca <= cb);
        before_total += cb;
        after_total += ca;
        if ca < cb {
            improved += 1
        } else {
            same += 1
        }
        stops[match rep.stop {
            Stop::Saturated => 0,
            Stop::IterLimit => 1,
            Stop::NodeLimit => 2,
        }] += 1;
    }
    println!(
        "semantics: 2000 random terms x 8 inputs agree after saturation+extraction; improved {improved}, unchanged {same}; total cost {before_total} -> {after_total}"
    );
    println!(
        "           stop reasons: saturated {}, iteration limit {}, node limit {}",
        stops[0], stops[1], stops[2]
    );
}

// ---------------------------------------------------------------- demos

fn show(
    label: &str,
    src: &str,
    rules: &[Rewrite],
    lim: &Limits,
    cost: &dyn Fn(Op) -> u64,
) -> (Term, Term) {
    let t = parse_term(src);
    let (out, rep, eg, root) = optimise(&t, rules, lim, cost);
    let mut rng = Rng(77);
    for _ in 0..200 {
        let env: Vec<i64> = (0..8).map(|_| rng.small()).collect();
        assert_eq!(
            t.eval(&env),
            out.eval(&env),
            "{label}: extracted term is not equivalent"
        );
    }
    let best = best_choices(&eg, cost);
    println!(
        "  {label}: {} (cost {})  =>  {} (cost {}), {:?} after {} iters, {} e-nodes / {} classes, dag size {}",
        t,
        term_cost(&t, cost),
        out,
        term_cost(&out, cost),
        rep.stop,
        rep.iters,
        eg.num_nodes(),
        eg.classes.len(),
        dag_size(&eg, &best, root)
    );
    (t, out)
}

fn demo_arithmetic() {
    println!("demo 1: arithmetic simplification (size cost)");
    let lim = Limits {
        iters: 12,
        nodes: 3000,
        match_limit: 200,
        backoff: true,
    };
    let (_, o) = show(
        "identities",
        "(+ (* x 1) (* 0 y))",
        &full_rules(),
        &lim,
        &cost_size,
    );
    assert_eq!(o.to_string(), "x");
    let (_, o) = show(
        "const fold",
        "(* (+ 1 1) (- 5 3))",
        &full_rules(),
        &lim,
        &cost_size,
    );
    assert_eq!(o.to_string(), "4"); // folding is an analysis: no rule mentions it
    let (_, o) = show(
        "cancel",
        "(+ (- x x) (* (+ y 0) 1))",
        &full_rules(),
        &lim,
        &cost_size,
    );
    assert_eq!(o.to_string(), "y");
}

fn demo_tensor_like() {
    println!(
        "demo 2: tensor-expression-like factoring (A*B + A*C = A*(B+C), Mul is 'a matmul' and costs 10)"
    );
    let lim = Limits {
        iters: 8,
        nodes: 3000,
        match_limit: 200,
        backoff: true,
    };
    let (t, o) = show(
        "matmul",
        "(+ (* a b) (* a c))",
        &full_rules(),
        &lim,
        &cost_weighted,
    );
    assert!(term_cost(&o, &cost_weighted) < term_cost(&t, &cost_weighted));
    assert_eq!(o.size(), 5);
    let (_, o) = show(
        "shared factor",
        "(+ (+ (* a b) (* a c)) (* a x))",
        &full_rules(),
        &lim,
        &cost_weighted,
    );
    assert_eq!(
        term_cost(&o, &cost_weighted),
        10 + 1 + 2 + 2 + 3,
        "one multiplication should remain"
    );
}

fn demo_horner() {
    println!("demo 3: rediscovering Horner's rule (3 multiplications -> 2) and strength reduction");
    let lim = Limits {
        iters: 10,
        nodes: 6000,
        match_limit: 300,
        backoff: true,
    };
    let poly = "(+ (+ (* (* a x) x) (* b x)) c)";
    let t = parse_term(poly);
    let mut eg = EGraph::default();
    let root = eg.add_term(&t);
    eg.rebuild();
    let rep = run(&mut eg, &full_rules(), &lim);
    let (c, out) = extract(&eg, root, &cost_weighted);
    let mut rng = Rng(0x40_12e5);
    for _ in 0..200 {
        let env: Vec<i64> = (0..8).map(|_| rng.small()).collect();
        assert_eq!(t.eval(&env), out.eval(&env));
    }
    let muls = |t: &Term| {
        fn go(t: &Term) -> usize {
            usize::from(t.op == Op::Mul) + t.kids.iter().map(go).sum::<usize>()
        }
        go(t)
    };
    println!(
        "  {t}  (muls {}, cost {})\n  => {out}  (muls {}, cost {c})  stop {:?}",
        muls(&t),
        term_cost(&t, &cost_weighted),
        muls(&out),
        rep.stop
    );
    assert_eq!(muls(&t), 3);
    assert!(muls(&out) <= 2, "Horner form has two multiplications");
    println!(
        "  e-graph growth (nodes, classes) per iteration: {:?}",
        rep.history
    );
    let _ = show(
        "strength",
        "(* (+ x 0) 2)",
        &full_rules(),
        &lim,
        &cost_weighted,
    );
}

/// Associativity + commutativity over a 7-term sum: the e-graph must eventually hold every subset and every split.
fn demo_explosion() {
    println!(
        "demo 4: AC explosion on a 7-variable sum, e-graph growth (nodes, classes) per iteration"
    );
    let t = parse_term("(+ x (+ y (+ z (+ a (+ b (+ c d))))))");
    let mut finals = vec![];
    for backoff in [false, true] {
        let mut eg = EGraph::default();
        let root = eg.add_term(&t);
        eg.rebuild();
        let rep = run(
            &mut eg,
            &full_rules(),
            &Limits {
                iters: 6,
                nodes: 4000,
                match_limit: 40,
                backoff,
            },
        );
        eg.check_invariants();
        let (_, out) = extract(&eg, root, &cost_weighted);
        let env: Vec<i64> = (1..=8).map(|i| i * 1_000_003).collect();
        assert_eq!(t.eval(&env), out.eval(&env));
        let name = if backoff {
            "backoff scheduler"
        } else {
            "no scheduler     "
        };
        println!(
            "  {name}: {:?}  stop {:?} after {} iters",
            rep.history, rep.stop, rep.iters
        );
        finals.push(eg.num_nodes());
    }
    assert!(
        finals[1] * 2 < finals[0],
        "the scheduler should hold growth down: {finals:?}"
    );
}

fn main() {
    test_rules_sound();
    test_congruence();
    test_extraction_oracle();
    test_dag_extraction();
    test_order_independence();
    demo_arithmetic();
    demo_tensor_like();
    demo_horner();
    demo_explosion();
    test_semantics();
    println!("all e-graph checks passed");
}
