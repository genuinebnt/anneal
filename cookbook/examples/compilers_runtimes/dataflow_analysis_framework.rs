//! RECIPE 820 (compilers): a generic monotone dataflow framework (lattice + transfer + worklist) with six classic analyses.
//! LEVEL: intermediate
//! Use in: optimising compilers, linters, IDE "unused variable" checks, bytecode verifiers and any tool that asks "what is true at this program point?".   Run: cargo run --release --example dataflow_analysis_framework
//! COMMON USES: LLVM and GCC passes (liveness for register allocation, available expressions for GVN/CSE, constant propagation, reaching
//! definitions for def-use chains), rustc's MIR borrow checker and `rustc_mir_dataflow` crate (a generic framework with the same
//! shape as this one: an `Analysis` trait with a lattice, a transfer function and a direction), Java/Android bytecode verifiers, security taint analysers.
//! Course: Cornell CS 6120 (Advanced Compilers) dataflow lessons; Moller and Schwartzbach "Static Program Analysis" chapters 5-6.
#![allow(dead_code)]
//!
//! THE FRAMEWORK. A dataflow analysis is a tuple (lattice L, join, bottom, boundary value, transfer function per instruction,
//! direction). If the transfer functions are MONOTONE (a <= b implies f(a) <= f(b)) and L has finite height, the iteration
//! "start every block at bottom, re-apply transfers until nothing changes" reaches the LEAST fixpoint (Kleene / Tarski). The order
//! in which the worklist hands out blocks changes only how fast we get there, never the answer. We run three orders (FIFO, LIFO,
//! priority by reverse postorder) and assert the solutions are identical, then print the number of transfer evaluations.
//! Reverse postorder visits a block after (most of) its predecessors, so forward problems converge in about loop-depth + 2 passes;
//! for BACKWARD problems the mirror order (postorder) is the fast one. rustc's engine does the same (it visits blocks in RPO).
//!
//! THE SIX ANALYSES (forward/backward x may/must is the classic 2x2 table):
//!   reaching definitions   forward  may   (join = union)       "which assignments might have produced this value?"
//!   live variables         backward may   (join = union)       "may this value still be read?"   (dead store elimination, regalloc)
//!   available expressions  forward  must  (join = intersect)   "was `a+b` computed on EVERY path, operands unchanged?" (CSE)
//!   very busy expressions  backward must  (join = intersect)   "is `a+b` computed on EVERY path from here before operands change?" (code hoisting)
//!   constant propagation   forward        lattice Bot < Const(k) < Top per variable (not a bit-vector problem; join of Const 1 and Const 2 is Top)
//!   sign analysis          forward        powerset of {neg, zero, pos}: a tiny abstract interpretation
//! For must-analyses the lattice order is flipped: BOTTOM (the identity of join) is the universal set, which is why unreachable
//! blocks keep "everything" and why the boundary value (empty set at the entry) is what makes the answer non-trivial.
//!
//! HOW WE KNOW IT IS RIGHT (every check below runs on 1500 random CFGs with loops, unreachable blocks and saturating arithmetic):
//!   1. all worklist orders return the same solution, and that solution is a FIXPOINT (in[b] = join of predecessor outs, out[b] = transfer(in[b]));
//!   2. transfer functions are monotone on random pairs a <= join(a, b), and join is commutative and idempotent;
//!   3. SOUNDNESS against a concrete interpreter: on random inputs we record the trace; at every executed program point the
//!      concrete state must be inside the abstract fact (constants equal, signs inside the set, last definition inside the reaching set,
//!      every variable read later is live, available expressions were really computed with the same operand values, very busy
//!      expressions really are evaluated later with the same operand values);
//!   4. FAILURE INJECTION: a deliberately buggy liveness (it forgets that `br x` reads x) is caught by the same checker.
//! Arithmetic is SATURATING so that sign and constant facts stay simple (wrapping would make `pos + pos` possibly negative).
//!
//! Read: Kildall, "A Unified Approach to Global Program Optimization" (POPL 1973; the original lattice formulation, cited by title only);
//! Moller and Schwartzbach, Static Program Analysis https://cs.au.dk/~amoeller/spa/ ;
//! Cornell CS 6120 dataflow lesson https://www.cs.cornell.edu/courses/cs6120/2020fa/lesson/4/ ;
//! rustc dev guide on dataflow https://rustc-dev-guide.rust-lang.org/mir/dataflow.html ;
//! https://en.wikipedia.org/wiki/Data-flow_analysis
//! TRY NEXT:
//!   - [S] Add a `Div` operator to `Op` and make `ConstProp` treat division by `Const(0)` as `Bot`; predict which random programs stop being sound, then run the checker.
//!   - [S] Change `Sign::abs_bin` so Sub of two `pos` values returns `{pos}`; the soundness checker should fail: find the smallest counterexample trace it prints.
//!   - [M] Experiment: with FIFO order, how does the evaluation count of `Liveness` on `ladder(n)` grow with n compared with RPO? Predict the exponent, then fit the printed table.
//!   - [M] Port: write a `DefiniteAssignment` analysis (forward, must) and use it to reject programs that read a variable before assigning it, mimicking rustc's E0381.
//!   - [A] Project: feed `Liveness` into `ssa_register_allocation` (replace its own liveness) and run both on `abstract_interpretation_intervals` programs; or turn `ConstProp` into the interval domain of `abstract_interpretation_intervals` with widening at loop headers.
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

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

// ---------------------------------------------------------------- IR

const NV: usize = 4; // variables a..d
type Var = usize;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
enum Opnd {
    V(Var),
    I(i64),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
enum Op {
    Add,
    Sub,
    Mul,
}
type Expr = (Op, Opnd, Opnd);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Instr {
    Const(Var, i64),
    Copy(Var, Var),
    Bin(Var, Op, Opnd, Opnd),
    Input(Var),
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Term {
    Jump(usize),
    Br(Var, usize, usize), // nonzero -> first target
    Ret(Var),
}
#[derive(Clone, Debug)]
struct Block {
    instrs: Vec<Instr>,
    term: Term,
}
#[derive(Clone, Debug)]
struct Cfg {
    blocks: Vec<Block>,
}
#[derive(Clone, Copy)]
enum Item<'a> {
    I(&'a Instr),
    T(&'a Term),
}

fn apply(op: Op, a: i64, b: i64) -> i64 {
    match op {
        Op::Add => a.saturating_add(b),
        Op::Sub => a.saturating_sub(b),
        Op::Mul => a.saturating_mul(b),
    }
}
fn opnd_vars(o: Opnd) -> Option<Var> {
    if let Opnd::V(v) = o { Some(v) } else { None }
}
fn expr_vars(e: &Expr) -> Vec<Var> {
    [e.1, e.2].iter().filter_map(|o| opnd_vars(*o)).collect()
}
impl Instr {
    fn def(&self) -> Var {
        match *self {
            Instr::Const(d, _) | Instr::Copy(d, _) | Instr::Bin(d, ..) | Instr::Input(d) => d,
        }
    }
    fn uses(&self) -> Vec<Var> {
        match *self {
            Instr::Const(..) | Instr::Input(_) => vec![],
            Instr::Copy(_, s) => vec![s],
            Instr::Bin(_, _, a, b) => [a, b].iter().filter_map(|o| opnd_vars(*o)).collect(),
        }
    }
    fn expr(&self) -> Option<Expr> {
        if let Instr::Bin(_, op, a, b) = *self {
            Some((op, a, b))
        } else {
            None
        }
    }
}
impl Term {
    fn uses(&self) -> Vec<Var> {
        match *self {
            Term::Jump(_) => vec![],
            Term::Br(v, ..) | Term::Ret(v) => vec![v],
        }
    }
    fn succs(&self) -> Vec<usize> {
        match *self {
            Term::Jump(t) => vec![t],
            Term::Br(_, a, b) => vec![a, b],
            Term::Ret(_) => vec![],
        }
    }
}
impl Item<'_> {
    fn def(&self) -> Option<Var> {
        if let Item::I(i) = self {
            Some(i.def())
        } else {
            None
        }
    }
    fn uses(&self) -> Vec<Var> {
        match self {
            Item::I(i) => i.uses(),
            Item::T(t) => t.uses(),
        }
    }
}
impl Cfg {
    fn n(&self) -> usize {
        self.blocks.len()
    }
    /// Items of a block are its instructions followed by the terminator (index `instrs.len()`).
    fn items(&self, b: usize) -> usize {
        self.blocks[b].instrs.len() + 1
    }
    fn item(&self, b: usize, k: usize) -> Item<'_> {
        let blk = &self.blocks[b];
        if k < blk.instrs.len() {
            Item::I(&blk.instrs[k])
        } else {
            Item::T(&blk.term)
        }
    }
    fn succs(&self, b: usize) -> Vec<usize> {
        self.blocks[b].term.succs()
    }
    fn preds_all(&self) -> Vec<Vec<usize>> {
        let mut p = vec![vec![]; self.n()];
        for b in 0..self.n() {
            for s in self.succs(b) {
                if !p[s].contains(&b) {
                    p[s].push(b);
                }
            }
        }
        p
    }
    fn all_exprs(&self) -> Vec<Expr> {
        let mut seen = BTreeSet::new();
        for b in &self.blocks {
            for i in &b.instrs {
                if let Some(e) = i.expr() {
                    seen.insert(e);
                }
            }
        }
        seen.into_iter().collect()
    }
}

// ---------------------------------------------------------------- text parser (builder for hand-written CFGs)

fn var_of(s: &str) -> Var {
    let c = s.chars().next().unwrap();
    assert!(s.len() == 1 && ('a'..='d').contains(&c), "bad variable {s}");
    c as usize - 'a' as usize
}
fn opnd_of(s: &str) -> Opnd {
    s.parse::<i64>()
        .map(Opnd::I)
        .unwrap_or_else(|_| Opnd::V(var_of(s)))
}
fn parse_cfg(src: &str) -> Cfg {
    let lines: Vec<&str> = src
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let names: Vec<&str> = lines
        .iter()
        .map(|l| l.split(':').next().unwrap().trim())
        .collect();
    let target = |s: &str| {
        names
            .iter()
            .position(|n| *n == s)
            .unwrap_or_else(|| panic!("no block {s}"))
    };
    let mut blocks = vec![];
    for l in &lines {
        let body = l.split_once(':').unwrap().1;
        let mut stmts: Vec<&str> = body
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let t: Vec<&str> = stmts
            .pop()
            .expect("block needs a terminator")
            .split_whitespace()
            .collect();
        let term = match t[0] {
            "jump" => Term::Jump(target(t[1])),
            "br" => Term::Br(var_of(t[1]), target(t[2]), target(t[3])),
            "ret" => Term::Ret(var_of(t[1])),
            other => panic!("bad terminator {other}"),
        };
        let instrs = stmts
            .iter()
            .map(|s| {
                let w: Vec<&str> = s.split_whitespace().collect();
                let d = var_of(w[0]);
                match w.len() {
                    3 if w[2] == "input" => Instr::Input(d),
                    3 => match opnd_of(w[2]) {
                        Opnd::I(k) => Instr::Const(d, k),
                        Opnd::V(v) => Instr::Copy(d, v),
                    },
                    5 => {
                        let op = match w[3] {
                            "+" => Op::Add,
                            "-" => Op::Sub,
                            "*" => Op::Mul,
                            o => panic!("bad op {o}"),
                        };
                        Instr::Bin(d, op, opnd_of(w[2]), opnd_of(w[4]))
                    }
                    _ => panic!("bad statement {s}"),
                }
            })
            .collect();
        blocks.push(Block { instrs, term });
    }
    Cfg { blocks }
}

// ---------------------------------------------------------------- concrete interpreter (the oracle)

struct Step {
    b: usize,
    k: usize,
    regs: [i64; NV],
}
struct Trace {
    steps: Vec<Step>,
    done: bool, // reached `ret` (false = fuel ran out)
}
fn val(regs: &[i64; NV], o: Opnd) -> i64 {
    match o {
        Opnd::V(v) => regs[v],
        Opnd::I(k) => k,
    }
}
fn run(cfg: &Cfg, inputs: &[i64], fuel: usize) -> Trace {
    let mut regs = [0i64; NV];
    let (mut b, mut k, mut next_in) = (0usize, 0usize, 0usize);
    let mut steps = vec![];
    while steps.len() < fuel {
        steps.push(Step { b, k, regs });
        match cfg.item(b, k) {
            Item::I(ins) => {
                match *ins {
                    Instr::Const(d, c) => regs[d] = c,
                    Instr::Copy(d, s) => regs[d] = regs[s],
                    Instr::Bin(d, op, x, y) => regs[d] = apply(op, val(&regs, x), val(&regs, y)),
                    Instr::Input(d) => {
                        regs[d] = inputs[next_in % inputs.len()];
                        next_in += 1;
                    }
                }
                k += 1;
            }
            Item::T(t) => match *t {
                Term::Jump(n) => (b, k) = (n, 0),
                Term::Br(v, t1, t2) => (b, k) = (if regs[v] != 0 { t1 } else { t2 }, 0),
                Term::Ret(_) => return Trace { steps, done: true },
            },
        }
    }
    Trace { steps, done: false }
}

// ---------------------------------------------------------------- the framework

trait Analysis {
    type Fact: Clone + PartialEq + std::fmt::Debug;
    const FORWARD: bool;
    fn name(&self) -> &'static str;
    /// Identity of `join` (the "no information yet" value every block starts with).
    fn bottom(&self) -> Self::Fact;
    /// Fact at the flow entry (program entry when forward, every `ret` when backward).
    fn boundary(&self) -> Self::Fact;
    fn join(&self, a: &Self::Fact, b: &Self::Fact) -> Self::Fact;
    /// Transfer across ONE item (instruction or terminator) in the direction of the analysis.
    fn transfer(&self, it: Item<'_>, loc: (usize, usize), f: &Self::Fact) -> Self::Fact;
    fn arbitrary(&self, rng: &mut Rng) -> Self::Fact;
    fn show(&self, f: &Self::Fact) -> String;
}

fn leq<A: Analysis>(a: &A, x: &A::Fact, y: &A::Fact) -> bool {
    a.join(x, y) == *y
}

/// Facts at the `m + 1` program points of block `b` (point k = just before item k), given the fact at the flow entry.
fn points<A: Analysis>(a: &A, cfg: &Cfg, b: usize, entry: &A::Fact) -> Vec<A::Fact> {
    let m = cfg.items(b);
    let mut p = vec![entry.clone(); m + 1];
    if A::FORWARD {
        for k in 0..m {
            p[k + 1] = a.transfer(cfg.item(b, k), (b, k), &p[k]);
        }
    } else {
        for k in (0..m).rev() {
            p[k] = a.transfer(cfg.item(b, k), (b, k), &p[k + 1]);
        }
    }
    p
}
fn block_transfer<A: Analysis>(a: &A, cfg: &Cfg, b: usize, f: &A::Fact) -> A::Fact {
    let p = points(a, cfg, b, f);
    if A::FORWARD {
        p[cfg.items(b)].clone()
    } else {
        p[0].clone()
    }
}

#[derive(Clone, Copy, Debug)]
enum Order {
    Fifo,
    Lifo,
    Rpo,
}
const ORDERS: [Order; 3] = [Order::Fifo, Order::Lifo, Order::Rpo];

struct Flow {
    preds: Vec<Vec<usize>>,
    succs: Vec<Vec<usize>>,
    entry: Vec<bool>,
    prio: Vec<usize>,
}
fn reverse_postorder(cfg: &Cfg) -> Vec<usize> {
    let n = cfg.n();
    let mut seen = vec![false; n];
    let mut post = vec![];
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, cfg.succs(root), 0usize)];
        while let Some((b, ss, i)) = stack.last_mut() {
            if *i < ss.len() {
                let s = ss[*i];
                *i += 1;
                if !seen[s] {
                    seen[s] = true;
                    stack.push((s, cfg.succs(s), 0));
                }
            } else {
                post.push(*b);
                stack.pop();
            }
        }
    }
    post.reverse();
    post
}
fn flow_for<A: Analysis>(cfg: &Cfg) -> Flow {
    let n = cfg.n();
    let preds = cfg.preds_all();
    let succs: Vec<Vec<usize>> = (0..n).map(|b| cfg.succs(b)).collect();
    let rpo = reverse_postorder(cfg);
    let mut prio = vec![0; n];
    for (i, &b) in rpo.iter().enumerate() {
        prio[b] = if A::FORWARD { i } else { n - 1 - i };
    }
    if A::FORWARD {
        let mut entry = vec![false; n];
        entry[0] = true;
        Flow {
            preds,
            succs,
            entry,
            prio,
        }
    } else {
        let entry = (0..n)
            .map(|b| matches!(cfg.blocks[b].term, Term::Ret(_)))
            .collect();
        Flow {
            preds: succs,
            succs: preds,
            entry,
            prio,
        }
    }
}
fn compute_in<A: Analysis>(a: &A, fl: &Flow, outs: &[A::Fact], b: usize) -> A::Fact {
    let mut acc = if fl.entry[b] {
        a.boundary()
    } else {
        a.bottom()
    };
    for &p in &fl.preds[b] {
        acc = a.join(&acc, &outs[p]);
    }
    acc
}

struct Worklist {
    order: Order,
    q: VecDeque<usize>,
    set: BTreeSet<(usize, usize)>,
    queued: Vec<bool>,
}
impl Worklist {
    fn push(&mut self, b: usize, prio: &[usize]) {
        if self.queued[b] {
            return;
        }
        self.queued[b] = true;
        match self.order {
            Order::Rpo => {
                self.set.insert((prio[b], b));
            }
            _ => self.q.push_back(b),
        }
    }
    fn pop(&mut self) -> Option<usize> {
        let b = match self.order {
            Order::Fifo => self.q.pop_front(),
            Order::Lifo => self.q.pop_back(),
            Order::Rpo => self.set.pop_first().map(|(_, b)| b),
        }?;
        self.queued[b] = false;
        Some(b)
    }
}

struct Solution<F> {
    ins: Vec<F>,  // fact at the flow entry of each block
    outs: Vec<F>, // fact at the flow exit
    evals: usize, // block transfer evaluations (the cost measure)
}
fn solve<A: Analysis>(a: &A, cfg: &Cfg, order: Order) -> Solution<A::Fact> {
    let n = cfg.n();
    let fl = flow_for::<A>(cfg);
    let mut ins = vec![a.bottom(); n];
    let mut outs = vec![a.bottom(); n];
    let mut wl = Worklist {
        order,
        q: VecDeque::new(),
        set: BTreeSet::new(),
        queued: vec![false; n],
    };
    for b in 0..n {
        wl.push(b, &fl.prio);
    }
    let mut evals = 0;
    while let Some(b) = wl.pop() {
        let inn = compute_in(a, &fl, &outs, b);
        let out = block_transfer(a, cfg, b, &inn);
        evals += 1;
        ins[b] = inn;
        if out != outs[b] {
            outs[b] = out;
            for &s in &fl.succs[b] {
                wl.push(s, &fl.prio);
            }
        }
    }
    Solution { ins, outs, evals }
}
/// A solution is a fixpoint iff one more application of every equation changes nothing.
fn is_fixpoint<A: Analysis>(a: &A, cfg: &Cfg, s: &Solution<A::Fact>) -> bool {
    let fl = flow_for::<A>(cfg);
    (0..cfg.n()).all(|b| {
        s.ins[b] == compute_in(a, &fl, &s.outs, b)
            && s.outs[b] == block_transfer(a, cfg, b, &s.ins[b])
    })
}
fn check_monotone<A: Analysis>(a: &A, cfg: &Cfg, rng: &mut Rng) {
    for b in 0..cfg.n() {
        for k in 0..cfg.items(b) {
            for _ in 0..3 {
                let x = a.arbitrary(rng);
                let y = a.join(&x, &a.arbitrary(rng));
                assert!(leq(a, &x, &y));
                assert_eq!(
                    a.join(&x, &y),
                    a.join(&y, &x),
                    "{}: join not commutative",
                    a.name()
                );
                assert_eq!(a.join(&x, &x), x, "{}: join not idempotent", a.name());
                let (tx, ty) = (
                    a.transfer(cfg.item(b, k), (b, k), &x),
                    a.transfer(cfg.item(b, k), (b, k), &y),
                );
                assert!(
                    leq(a, &tx, &ty),
                    "{}: transfer not monotone at ({b},{k}): {x:?} <= {y:?} but {tx:?} !<= {ty:?}",
                    a.name()
                );
            }
        }
    }
}

/// Solve with all three orders, check they agree and form a fixpoint; return the RPO solution, the per-point facts
/// and the evaluation counts per order.
fn analyze<A: Analysis>(a: &A, cfg: &Cfg) -> (Vec<Vec<A::Fact>>, [usize; 3]) {
    let sols: Vec<Solution<A::Fact>> = ORDERS.iter().map(|o| solve(a, cfg, *o)).collect();
    for s in &sols[1..] {
        assert!(
            s.ins == sols[0].ins && s.outs == sols[0].outs,
            "{}: worklist orders disagree",
            a.name()
        );
    }
    assert!(
        is_fixpoint(a, cfg, &sols[2]),
        "{}: not a fixpoint",
        a.name()
    );
    let pf = (0..cfg.n())
        .map(|b| points(a, cfg, b, &sols[2].ins[b]))
        .collect();
    (pf, [sols[0].evals, sols[1].evals, sols[2].evals])
}

// ---------------------------------------------------------------- 1. reaching definitions (forward, may)

/// Definition ids: 0..NV are the implicit "initial value" definitions of each variable, then one id per instruction.
struct ReachingDefs {
    id: HashMap<(usize, usize), usize>,
    defs_of: [u64; NV],
    universe: u64,
}
impl ReachingDefs {
    fn new(cfg: &Cfg) -> Self {
        let mut id = HashMap::new();
        let mut defs_of = [0u64; NV];
        for (v, m) in defs_of.iter_mut().enumerate() {
            *m = 1 << v;
        }
        let mut next = NV;
        for (b, blk) in cfg.blocks.iter().enumerate() {
            for (k, i) in blk.instrs.iter().enumerate() {
                id.insert((b, k), next);
                defs_of[i.def()] |= 1 << next;
                next += 1;
            }
        }
        assert!(next <= 64);
        ReachingDefs {
            id,
            defs_of,
            universe: if next == 64 { !0 } else { (1u64 << next) - 1 },
        }
    }
}
impl Analysis for ReachingDefs {
    type Fact = u64;
    const FORWARD: bool = true;
    fn name(&self) -> &'static str {
        "reaching-defs"
    }
    fn bottom(&self) -> u64 {
        0
    }
    fn boundary(&self) -> u64 {
        (1 << NV) - 1
    }
    fn join(&self, a: &u64, b: &u64) -> u64 {
        a | b
    }
    fn transfer(&self, it: Item<'_>, loc: (usize, usize), f: &u64) -> u64 {
        match it {
            Item::I(i) => (f & !self.defs_of[i.def()]) | (1 << self.id[&loc]),
            Item::T(_) => *f,
        }
    }
    fn arbitrary(&self, rng: &mut Rng) -> u64 {
        rng.next() & self.universe
    }
    fn show(&self, f: &u64) -> String {
        let ids: Vec<String> = (0..64)
            .filter(|i| f >> i & 1 == 1)
            .map(|i| format!("d{i}"))
            .collect();
        format!("{{{}}}", ids.join(","))
    }
}

// ---------------------------------------------------------------- 2. live variables (backward, may)

struct Liveness {
    /// Failure injection: a buggy analysis that forgets terminator uses.
    ignore_term_uses: bool,
}
fn mask(vs: &[Var]) -> u64 {
    vs.iter().fold(0, |m, v| m | 1 << v)
}
fn show_vars(f: u64) -> String {
    let v: Vec<String> = (0..NV)
        .filter(|i| f >> i & 1 == 1)
        .map(|i| ((b'a' + i as u8) as char).to_string())
        .collect();
    format!("{{{}}}", v.join(","))
}
impl Analysis for Liveness {
    type Fact = u64;
    const FORWARD: bool = false;
    fn name(&self) -> &'static str {
        "liveness"
    }
    fn bottom(&self) -> u64 {
        0
    }
    fn boundary(&self) -> u64 {
        0
    }
    fn join(&self, a: &u64, b: &u64) -> u64 {
        a | b
    }
    fn transfer(&self, it: Item<'_>, _: (usize, usize), f: &u64) -> u64 {
        match it {
            Item::I(i) => (f & !(1 << i.def())) | mask(&i.uses()),
            Item::T(t) if self.ignore_term_uses => {
                let _ = t;
                *f
            }
            Item::T(t) => f | mask(&t.uses()),
        }
    }
    fn arbitrary(&self, rng: &mut Rng) -> u64 {
        rng.below(1 << NV)
    }
    fn show(&self, f: &u64) -> String {
        show_vars(*f)
    }
}

// ---------------------------------------------------------------- 3 + 4. available and very busy expressions (must)

struct ExprInfo {
    exprs: Vec<Expr>,
    idx: HashMap<Expr, usize>,
    mentions: [u64; NV],
    universe: u64,
}
impl ExprInfo {
    fn new(cfg: &Cfg) -> Self {
        let exprs = cfg.all_exprs();
        assert!(exprs.len() < 64);
        let mut mentions = [0u64; NV];
        for (i, e) in exprs.iter().enumerate() {
            for v in expr_vars(e) {
                mentions[v] |= 1 << i;
            }
        }
        let idx = exprs.iter().enumerate().map(|(i, e)| (*e, i)).collect();
        let universe = (1u64 << exprs.len()) - 1;
        ExprInfo {
            exprs,
            idx,
            mentions,
            universe,
        }
    }
    fn show(&self, f: u64) -> String {
        let v: Vec<String> = (0..self.exprs.len())
            .filter(|i| f >> i & 1 == 1)
            .map(|i| {
                let (op, a, b) = self.exprs[i];
                let o = |x: Opnd| match x {
                    Opnd::V(v) => ((b'a' + v as u8) as char).to_string(),
                    Opnd::I(k) => k.to_string(),
                };
                let s = match op {
                    Op::Add => "+",
                    Op::Sub => "-",
                    Op::Mul => "*",
                };
                format!("{}{}{}", o(a), s, o(b))
            })
            .collect();
        format!("{{{}}}", v.join(","))
    }
    /// gen/kill of one instruction. The expression is evaluated BEFORE the destination is overwritten.
    fn gen_kill(&self, i: &Instr) -> (u64, u64) {
        let gen_ = i.expr().map_or(0, |e| 1 << self.idx[&e]);
        (gen_, self.mentions[i.def()])
    }
}
struct AvailExprs(ExprInfo);
impl Analysis for AvailExprs {
    type Fact = u64;
    const FORWARD: bool = true;
    fn name(&self) -> &'static str {
        "available-exprs"
    }
    fn bottom(&self) -> u64 {
        self.0.universe
    }
    fn boundary(&self) -> u64 {
        0
    }
    fn join(&self, a: &u64, b: &u64) -> u64 {
        a & b
    }
    fn transfer(&self, it: Item<'_>, _: (usize, usize), f: &u64) -> u64 {
        match it {
            Item::I(i) => {
                let (g, k) = self.0.gen_kill(i);
                (f | g) & !k // evaluate first, then the assignment kills (including `x = x + 1` itself)
            }
            Item::T(_) => *f,
        }
    }
    fn arbitrary(&self, rng: &mut Rng) -> u64 {
        rng.next() & self.0.universe
    }
    fn show(&self, f: &u64) -> String {
        self.0.show(*f)
    }
}
struct VeryBusy(ExprInfo);
impl Analysis for VeryBusy {
    type Fact = u64;
    const FORWARD: bool = false;
    fn name(&self) -> &'static str {
        "very-busy-exprs"
    }
    fn bottom(&self) -> u64 {
        self.0.universe
    }
    fn boundary(&self) -> u64 {
        0
    }
    fn join(&self, a: &u64, b: &u64) -> u64 {
        a & b
    }
    fn transfer(&self, it: Item<'_>, _: (usize, usize), f: &u64) -> u64 {
        match it {
            Item::I(i) => {
                let (g, k) = self.0.gen_kill(i);
                (f & !k) | g // backwards: the assignment happens after the evaluation, so gen wins over its own kill
            }
            Item::T(_) => *f,
        }
    }
    fn arbitrary(&self, rng: &mut Rng) -> u64 {
        rng.next() & self.0.universe
    }
    fn show(&self, f: &u64) -> String {
        self.0.show(*f)
    }
}

// ---------------------------------------------------------------- 5. constant propagation

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cp {
    Bot,
    C(i64),
    Top,
}
fn cp_join(a: Cp, b: Cp) -> Cp {
    match (a, b) {
        (Cp::Bot, x) | (x, Cp::Bot) => x,
        (Cp::C(x), Cp::C(y)) if x == y => Cp::C(x),
        _ => Cp::Top,
    }
}
fn cp_bin(op: Op, a: Cp, b: Cp) -> Cp {
    match (a, b) {
        (Cp::Bot, _) | (_, Cp::Bot) => Cp::Bot, // strict: no value yet
        (Cp::C(x), Cp::C(y)) => Cp::C(apply(op, x, y)),
        (Cp::C(0), _) | (_, Cp::C(0)) if op == Op::Mul => Cp::C(0), // 0 * unknown is still 0
        _ => Cp::Top,
    }
}
struct ConstProp;
type CpEnv = [Cp; NV];
fn cp_opnd(f: &CpEnv, o: Opnd) -> Cp {
    match o {
        Opnd::V(v) => f[v],
        Opnd::I(k) => Cp::C(k),
    }
}
impl Analysis for ConstProp {
    type Fact = CpEnv;
    const FORWARD: bool = true;
    fn name(&self) -> &'static str {
        "const-prop"
    }
    fn bottom(&self) -> CpEnv {
        [Cp::Bot; NV]
    }
    fn boundary(&self) -> CpEnv {
        [Cp::C(0); NV] // the interpreter zero-initialises variables
    }
    fn join(&self, a: &CpEnv, b: &CpEnv) -> CpEnv {
        std::array::from_fn(|i| cp_join(a[i], b[i]))
    }
    fn transfer(&self, it: Item<'_>, _: (usize, usize), f: &CpEnv) -> CpEnv {
        let mut g = *f;
        if let Item::I(i) = it {
            g[i.def()] = match *i {
                Instr::Const(_, k) => Cp::C(k),
                Instr::Copy(_, s) => f[s],
                Instr::Input(_) => Cp::Top,
                Instr::Bin(_, op, a, b) => cp_bin(op, cp_opnd(f, a), cp_opnd(f, b)),
            };
        }
        g
    }
    fn arbitrary(&self, rng: &mut Rng) -> CpEnv {
        std::array::from_fn(|_| match rng.below(4) {
            0 => Cp::Bot,
            1 => Cp::Top,
            _ => Cp::C(rng.range(-2, 2)),
        })
    }
    fn show(&self, f: &CpEnv) -> String {
        let parts: Vec<String> = f
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let n = (b'a' + i as u8) as char;
                match c {
                    Cp::Bot => format!("{n}=_"),
                    Cp::C(k) => format!("{n}={k}"),
                    Cp::Top => format!("{n}=T"),
                }
            })
            .collect();
        parts.join(" ")
    }
}

// ---------------------------------------------------------------- 6. sign analysis

const NEG: u8 = 1;
const ZERO: u8 = 2;
const POS: u8 = 4;
fn sign_bit(k: i64) -> u8 {
    match k.signum() {
        -1 => NEG,
        0 => ZERO,
        _ => POS,
    }
}
fn sign_neg(s: u8) -> u8 {
    (if s & NEG != 0 { POS } else { 0 }) | (s & ZERO) | (if s & POS != 0 { NEG } else { 0 })
}
fn sign_op1(op: Op, a: u8, b: u8) -> u8 {
    match op {
        Op::Add => match (a, b) {
            (ZERO, x) | (x, ZERO) => x,
            (NEG, NEG) => NEG,
            (POS, POS) => POS,
            _ => NEG | ZERO | POS, // opposite signs: anything
        },
        Op::Sub => sign_op1(Op::Add, a, sign_neg(b)),
        Op::Mul => match (a, b) {
            (ZERO, _) | (_, ZERO) => ZERO,
            (x, y) if x == y => POS,
            _ => NEG,
        },
    }
}
/// Lift to sets by trying every pair of single signs (that is the whole "abstract operation" for a finite domain).
fn sign_bin(op: Op, a: u8, b: u8) -> u8 {
    let mut r = 0;
    for x in [NEG, ZERO, POS] {
        for y in [NEG, ZERO, POS] {
            if a & x != 0 && b & y != 0 {
                r |= sign_op1(op, x, y);
            }
        }
    }
    r
}
struct Sign;
type SignEnv = [u8; NV];
fn sign_opnd(f: &SignEnv, o: Opnd) -> u8 {
    match o {
        Opnd::V(v) => f[v],
        Opnd::I(k) => sign_bit(k),
    }
}
impl Analysis for Sign {
    type Fact = SignEnv;
    const FORWARD: bool = true;
    fn name(&self) -> &'static str {
        "sign"
    }
    fn bottom(&self) -> SignEnv {
        [0; NV]
    }
    fn boundary(&self) -> SignEnv {
        [ZERO; NV]
    }
    fn join(&self, a: &SignEnv, b: &SignEnv) -> SignEnv {
        std::array::from_fn(|i| a[i] | b[i])
    }
    fn transfer(&self, it: Item<'_>, _: (usize, usize), f: &SignEnv) -> SignEnv {
        let mut g = *f;
        if let Item::I(i) = it {
            g[i.def()] = match *i {
                Instr::Const(_, k) => sign_bit(k),
                Instr::Copy(_, s) => f[s],
                Instr::Input(_) => NEG | ZERO | POS,
                Instr::Bin(_, op, a, b) => sign_bin(op, sign_opnd(f, a), sign_opnd(f, b)),
            };
        }
        g
    }
    fn arbitrary(&self, rng: &mut Rng) -> SignEnv {
        std::array::from_fn(|_| rng.below(8) as u8)
    }
    fn show(&self, f: &SignEnv) -> String {
        let parts: Vec<String> = f
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let n = (b'a' + i as u8) as char;
                let mut t = String::new();
                if s & NEG != 0 {
                    t.push('-');
                }
                if s & ZERO != 0 {
                    t.push('0');
                }
                if s & POS != 0 {
                    t.push('+');
                }
                format!("{n}:{}", if t.is_empty() { "_".into() } else { t })
            })
            .collect();
        parts.join(" ")
    }
}

// ---------------------------------------------------------------- soundness checks against the concrete trace

/// last_def[v] (a definition id) must be inside the reaching set at every step.
fn check_rd(rd: &ReachingDefs, pf: &[Vec<u64>], tr: &Trace, cfg: &Cfg) -> bool {
    let mut last: [usize; NV] = std::array::from_fn(|v| v);
    for s in &tr.steps {
        if (0..NV).any(|v| pf[s.b][s.k] >> last[v] & 1 == 0) {
            return false;
        }
        if let Item::I(i) = cfg.item(s.b, s.k) {
            last[i.def()] = rd.id[&(s.b, s.k)];
        }
    }
    true
}
/// Backward scan of the trace: the set of variables read before being redefined must be a subset of the live set.
fn check_live(pf: &[Vec<u64>], tr: &Trace, cfg: &Cfg) -> bool {
    let mut need = 0u64;
    for s in tr.steps.iter().rev() {
        let it = cfg.item(s.b, s.k);
        if let Some(d) = it.def() {
            need &= !(1 << d);
        }
        need |= mask(&it.uses());
        if need & !pf[s.b][s.k] != 0 {
            return false;
        }
    }
    true
}
fn expr_vals(regs: &[i64; NV], e: &Expr) -> (i64, i64) {
    (val(regs, e.1), val(regs, e.2))
}
/// Available: every expression in the set was evaluated earlier on this path and the operands still hold the same VALUES.
fn check_avail(info: &ExprInfo, pf: &[Vec<u64>], tr: &Trace, cfg: &Cfg) -> bool {
    let mut last: Vec<Option<(i64, i64)>> = vec![None; info.exprs.len()];
    for s in &tr.steps {
        let f = pf[s.b][s.k];
        for (i, e) in info.exprs.iter().enumerate() {
            if f >> i & 1 == 1 && last[i] != Some(expr_vals(&s.regs, e)) {
                return false;
            }
        }
        if let Item::I(ins) = cfg.item(s.b, s.k) {
            if let Some(e) = ins.expr() {
                last[info.idx[&e]] = Some(expr_vals(&s.regs, &e));
            }
        }
    }
    true
}
/// Very busy (only meaningful when the run reached `ret`): every expression in the set is evaluated later with the same operand values.
fn check_busy(info: &ExprInfo, pf: &[Vec<u64>], tr: &Trace, cfg: &Cfg) -> bool {
    if !tr.done {
        return true;
    }
    for (t, s) in tr.steps.iter().enumerate() {
        let f = pf[s.b][s.k];
        for (i, e) in info.exprs.iter().enumerate() {
            if f >> i & 1 == 0 {
                continue;
            }
            let want = expr_vals(&s.regs, e);
            let later = tr.steps[t..].iter().any(|u| {
                matches!(cfg.item(u.b, u.k), Item::I(ins) if ins.expr() == Some(*e))
                    && expr_vals(&u.regs, e) == want
            });
            if !later {
                return false;
            }
        }
    }
    true
}
fn check_cp(pf: &[Vec<CpEnv>], tr: &Trace) -> bool {
    tr.steps.iter().all(|s| {
        (0..NV).all(|v| match pf[s.b][s.k][v] {
            Cp::Bot => false, // analysis claims this point is unreachable, yet we are here
            Cp::C(c) => c == s.regs[v],
            Cp::Top => true,
        })
    })
}
fn check_sign(pf: &[Vec<SignEnv>], tr: &Trace) -> bool {
    tr.steps
        .iter()
        .all(|s| (0..NV).all(|v| pf[s.b][s.k][v] & sign_bit(s.regs[v]) != 0))
}

// ---------------------------------------------------------------- random programs

fn random_opnd(rng: &mut Rng) -> Opnd {
    if rng.chance(70) {
        Opnd::V(rng.below(NV as u64) as usize)
    } else {
        Opnd::I(rng.range(-2, 3))
    }
}
fn random_instr(rng: &mut Rng) -> Instr {
    let d = rng.below(NV as u64) as usize;
    match rng.below(20) {
        0..=4 => Instr::Const(d, rng.range(-3, 5)),
        5..=7 => Instr::Copy(d, rng.below(NV as u64) as usize),
        8..=10 => Instr::Input(d),
        _ => {
            let op = [Op::Add, Op::Sub, Op::Mul][rng.below(3) as usize];
            Instr::Bin(d, op, random_opnd(rng), random_opnd(rng))
        }
    }
}
fn random_cfg(rng: &mut Rng) -> Cfg {
    let n = 2 + rng.below(6) as usize;
    let mut blocks = vec![];
    for b in 0..n {
        let instrs = (0..rng.below(4)).map(|_| random_instr(rng)).collect();
        let v = rng.below(NV as u64) as usize;
        let term = if b == n - 1 {
            Term::Ret(v)
        } else {
            match rng.below(10) {
                0..=3 => Term::Jump(b + 1),
                4 => Term::Jump(rng.below(n as u64) as usize),
                5..=7 => Term::Br(v, rng.below(n as u64) as usize, b + 1),
                _ => Term::Br(v, b + 1, rng.below(n as u64) as usize),
            }
        };
        blocks.push(Block { instrs, term });
    }
    Cfg { blocks }
}
/// A long CFG with nested back edges, to make the effect of the worklist order visible.
fn ladder(n: usize) -> Cfg {
    let mut blocks = vec![];
    for b in 0..n {
        let d = b % NV;
        let instrs = vec![
            Instr::Bin(d, Op::Add, Opnd::V((d + 1) % NV), Opnd::I(1)),
            Instr::Copy((d + 2) % NV, d),
        ];
        let term = if b == n - 1 {
            Term::Ret(0)
        } else if b % 4 == 3 {
            Term::Br(d, b + 1, b - 3) // back edge over the last four blocks
        } else {
            Term::Jump(b + 1)
        };
        blocks.push(Block { instrs, term });
    }
    Cfg { blocks }
}

// ---------------------------------------------------------------- demo

fn demo_report<A: Analysis>(a: &A, cfg: &Cfg) {
    let sol = solve(a, cfg, Order::Rpo);
    println!("  {}:", a.name());
    for b in 0..cfg.n() {
        let (before, after) = if A::FORWARD {
            (&sol.ins[b], &sol.outs[b])
        } else {
            (&sol.outs[b], &sol.ins[b])
        };
        println!(
            "    b{b}: start {:<34} end {}",
            a.show(before),
            a.show(after)
        );
    }
}

const DEMO: &str = "
    b0: a = 1; b = input; c = a + b; br b b1 b2
    b1: d = a + b; a = 2; jump b3
    b2: d = a * b; jump b3
    b3: c = a + b; a = d + c; br a b0 b4
    b4: ret c
";

#[derive(Default)]
struct Stats {
    evals: [usize; 3],
    programs: usize,
    points_checked: usize,
}

fn main() {
    // ---- 1. a hand-written program through every analysis
    let cfg = parse_cfg(DEMO);
    println!("== demo CFG (b3 -> b0 is a back edge; `a + b` is computed in b0 and b3) ==");
    demo_report(&ReachingDefs::new(&cfg), &cfg);
    demo_report(
        &Liveness {
            ignore_term_uses: false,
        },
        &cfg,
    );
    demo_report(&AvailExprs(ExprInfo::new(&cfg)), &cfg);
    demo_report(&VeryBusy(ExprInfo::new(&cfg)), &cfg);
    demo_report(&ConstProp, &cfg);
    demo_report(&Sign, &cfg);
    // known facts about the demo: `a + b` is NOT available at the start of b3 (b1 redefines a) but IS very busy at the start of b1? (it is evaluated first thing)
    let ae = AvailExprs(ExprInfo::new(&cfg));
    let s = solve(&ae, &cfg, Order::Rpo);
    let ab = ae.0.idx[&(Op::Add, Opnd::V(0), Opnd::V(1))];
    assert_eq!(
        s.ins[3] >> ab & 1,
        0,
        "b1 kills a+b on one path, so it is not available at the join"
    );
    assert_eq!(
        s.ins[1] >> ab & 1,
        1,
        "a+b was computed in b0 and nothing killed it on the way to b1"
    );
    let vb = VeryBusy(ExprInfo::new(&cfg));
    let sv = solve(&vb, &cfg, Order::Rpo);
    assert_eq!(
        sv.outs[1] >> ab & 1,
        1,
        "b1 evaluates a+b first, so it is very busy at its start"
    );

    // ---- 2. the randomized battery
    let mut rng = Rng(0x00DA_7AF1_0D00_0001);
    let names = [
        "reaching-defs",
        "liveness",
        "available-exprs",
        "very-busy-exprs",
        "const-prop",
        "sign",
    ];
    let mut stats: Vec<Stats> = names.iter().map(|_| Stats::default()).collect();
    let (mut buggy_caught, mut unsound_live_checked) = (0, 0);
    let mut total_steps = 0usize;
    const PROGRAMS: usize = 1500;
    for _ in 0..PROGRAMS {
        let cfg = random_cfg(&mut rng);
        let traces: Vec<Trace> = (0..3)
            .map(|_| {
                let inputs: Vec<i64> = (0..6).map(|_| rng.range(-4, 4)).collect();
                run(&cfg, &inputs, 300)
            })
            .collect();
        total_steps += traces.iter().map(|t| t.steps.len()).sum::<usize>();

        let rd = ReachingDefs::new(&cfg);
        let live = Liveness {
            ignore_term_uses: false,
        };
        let ae = AvailExprs(ExprInfo::new(&cfg));
        let vb = VeryBusy(ExprInfo::new(&cfg));
        for a in [&rd as &dyn Monotone, &live, &ae, &vb, &ConstProp, &Sign] {
            a.monotone(&cfg, &mut rng);
        }
        let mut record = |i: usize, evals: [usize; 3], pts: usize| {
            let s = &mut stats[i];
            for k in 0..3 {
                s.evals[k] += evals[k];
            }
            s.programs += 1;
            s.points_checked += pts;
        };
        let (pf, ev) = analyze(&rd, &cfg);
        assert!(
            traces.iter().all(|t| check_rd(&rd, &pf, t, &cfg)),
            "reaching defs unsound"
        );
        record(0, ev, 0);
        let (pf, ev) = analyze(&live, &cfg);
        assert!(
            traces.iter().all(|t| check_live(&pf, t, &cfg)),
            "liveness unsound"
        );
        record(1, ev, 0);
        // failure injection: the buggy liveness must be flagged whenever some branch variable is only read by the branch
        let bug = Liveness {
            ignore_term_uses: true,
        };
        let (bpf, _) = analyze(&bug, &cfg);
        unsound_live_checked += 1;
        if !traces.iter().all(|t| check_live(&bpf, t, &cfg)) {
            buggy_caught += 1;
        }
        let (pf, ev) = analyze(&ae, &cfg);
        assert!(
            traces.iter().all(|t| check_avail(&ae.0, &pf, t, &cfg)),
            "available exprs unsound"
        );
        record(2, ev, 0);
        let (pf, ev) = analyze(&vb, &cfg);
        assert!(
            traces.iter().all(|t| check_busy(&vb.0, &pf, t, &cfg)),
            "very busy exprs unsound"
        );
        record(3, ev, 0);
        let (pf, ev) = analyze(&ConstProp, &cfg);
        assert!(
            traces.iter().all(|t| check_cp(&pf, t)),
            "constant propagation unsound"
        );
        record(4, ev, 0);
        let (pf, ev) = analyze(&Sign, &cfg);
        assert!(
            traces.iter().all(|t| check_sign(&pf, t)),
            "sign analysis unsound"
        );
        record(5, ev, 0);
    }
    assert!(
        buggy_caught > 50,
        "failure injection: the checker should catch the buggy liveness often, caught {buggy_caught}"
    );
    println!(
        "\n== {PROGRAMS} random CFGs, {total_steps} concrete steps checked: all six analyses sound, fixpoints, monotone, order-independent =="
    );
    println!(
        "failure injection: buggy liveness (ignores `br x` reads) caught on {buggy_caught}/{unsound_live_checked} programs"
    );
    println!("\naverage transfer evaluations per program (lower is better):");
    println!(
        "  {:<17} {:>8} {:>8} {:>8}",
        "analysis", "FIFO", "LIFO", "RPO-pq"
    );
    for (i, n) in names.iter().enumerate() {
        let s = &stats[i];
        let avg = |k: usize| s.evals[k] as f64 / s.programs as f64;
        println!(
            "  {:<17} {:>8.2} {:>8.2} {:>8.2}",
            n,
            avg(0),
            avg(1),
            avg(2)
        );
    }

    // ---- 3. the order matters at scale
    println!("\nladder CFGs (nested back edges): evaluations by worklist order");
    println!(
        "  {:>5}  {:<15} {:>8} {:>8} {:>8}",
        "n", "analysis", "FIFO", "LIFO", "RPO-pq"
    );
    for n in [40usize, 160, 640] {
        let cfg = ladder(n);
        let (_, e) = analyze(
            &Liveness {
                ignore_term_uses: false,
            },
            &cfg,
        );
        println!(
            "  {:>5}  {:<15} {:>8} {:>8} {:>8}",
            n, "liveness", e[0], e[1], e[2]
        );
        let (_, e) = analyze(&ConstProp, &cfg);
        println!(
            "  {:>5}  {:<15} {:>8} {:>8} {:>8}",
            n, "const-prop", e[0], e[1], e[2]
        );
        // the mirror-image order is the right one for the opposite direction: a fixed order cannot win both
        assert!(e[2] <= e[0] * 2);
    }
    let _ = HashSet::<u8>::new();
    println!("\nall assertions passed");
}

/// Object-safe wrapper so the monotonicity check can loop over analyses with different `Fact` types.
trait Monotone {
    fn monotone(&self, cfg: &Cfg, rng: &mut Rng);
}
impl<A: Analysis> Monotone for A {
    fn monotone(&self, cfg: &Cfg, rng: &mut Rng) {
        check_monotone(self, cfg, rng);
    }
}
