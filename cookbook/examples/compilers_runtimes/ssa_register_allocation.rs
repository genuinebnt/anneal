//! RECIPE 422 (compilers): a mini compiler backend: CFG, dominators, SSA, optimisation, liveness, register allocation, out-of-SSA.
//! LEVEL: advanced
//! Use in: compilers, JITs, query-expression engines and anything that turns a high-level program into straight machine-like code.   Run: cargo run --release --example ssa_register_allocation
#![allow(dead_code)]
//!
//! PIPELINE (every stage is checked by RUNNING the program, not by eyeballing): structured AST -> three-address IR over a CFG
//! of basic blocks -> dominators (Cooper-Harvey-Kennedy) + dominance frontiers -> SSA (phis at the iterated dominance
//! frontier, renaming down the dominator tree) -> SSA optimisations (constant folding/propagation, copy propagation, dead code
//! elimination, branch folding) -> out-of-SSA (split critical edges, PARALLEL copies sequentialised) -> liveness (backward
//! dataflow to a fixpoint) -> interference graph -> register allocation two ways (Chaitin-Briggs graph colouring and
//! Poletto-Sarkar linear scan, both with spill-and-retry) -> assembly for a K-register machine, which a tiny emulator runs.
//! WHY SSA: every variable has exactly one definition, so "which definition reaches here?" is just the variable name, and
//! optimisations become simple map lookups. Phi nodes merge values at joins. Dominance makes it sound: every use must be
//! dominated by its definition (we assert it).
//! WHY CHK: the iterative dominator algorithm over reverse postorder with a two-finger `intersect` is shorter than
//! Lengauer-Tarjan and as fast in practice on real CFGs; we cross-check it against the textbook set-intersection definition.
//! THE TWO CLASSIC OUT-OF-SSA BUGS, both reproduced here with a naive lowering and then fixed:
//!   * SWAP problem: `a = phi(a0, b); b = phi(b0, a)` are PARALLEL copies; emitting `a = b; b = a` in order loses a value.
//!     Fix: sequentialise the parallel copy, breaking cycles with a temporary.
//!   * LOST-COPY problem: a phi copy placed at the end of a block that also branches elsewhere clobbers a value the other
//!     path still needs. Fix: split critical edges so the copy lives on its own edge block.
//! ALLOCATION: graph colouring simplifies nodes of degree < K, optimistically pushes the cheapest spill candidate when stuck
//! (Briggs), colours in reverse (biased towards move partners so copies vanish), and rewrites spilled variables with
//! load-before-use / store-after-def temporaries that are themselves unspillable, then retries. Spill cost weights uses by
//! 10^loop-depth (loops found from dominator back edges). Linear scan keeps one [start,end] interval per variable and spills
//! the active interval that ends furthest away. Spilled values live in stack slots (`ld`/`st`).
//! Both allocators work after phi lowering, so the IR is no longer SSA there; liveness and interference do not need SSA.
//! COMMON USES: the backend shape of LLVM/Cranelift/V8/HotSpot-style compilers (SSA IR, dominators, linear-scan or graph
//! colouring allocators, parallel-copy resolution), bytecode JITs, shader compilers, and SQL/expression JIT engines.
//! Read: Cooper, Harvey, Kennedy "A Simple, Fast Dominance Algorithm"; Cytron et al. "Efficiently Computing SSA Form";
//! Poletto and Sarkar "Linear Scan Register Allocation" https://web.cs.ucla.edu/~palsberg/course/cs132/linearscan.pdf ;
//! Boissinot et al. "Revisiting Out-of-SSA Translation" https://inria.hal.science/inria-00349925v1/document ;
//! https://en.wikipedia.org/wiki/Static_single-assignment_form and https://en.wikipedia.org/wiki/Register_allocation
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- IR

type Var = usize;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
enum Opnd {
    Const(i64),
    Var(Var),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Lt,
    Le,
    Eq,
    Ne,
}

impl BinOp {
    fn name(self) -> &'static str {
        match self {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Lt => "lt",
            BinOp::Le => "le",
            BinOp::Eq => "eq",
            BinOp::Ne => "ne",
        }
    }
    fn sym(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
        }
    }
    /// Wrapping arithmetic so random programs never trap and folding agrees with execution.
    fn eval(self, a: i64, b: i64) -> i64 {
        match self {
            BinOp::Add => a.wrapping_add(b),
            BinOp::Sub => a.wrapping_sub(b),
            BinOp::Mul => a.wrapping_mul(b),
            BinOp::Lt => (a < b) as i64,
            BinOp::Le => (a <= b) as i64,
            BinOp::Eq => (a == b) as i64,
            BinOp::Ne => (a != b) as i64,
        }
    }
}

#[derive(Clone, Debug)]
enum Rhs {
    Copy(Opnd),
    Bin(BinOp, Opnd, Opnd),
    Load(usize), // from a stack slot (introduced by spilling)
}

#[derive(Clone, Debug)]
enum Inst {
    Def(Var, Rhs),
    Store(usize, Opnd),
}

#[derive(Clone, Debug)]
struct Phi {
    dst: Var,
    args: Vec<(usize, Opnd)>, // (predecessor block, incoming value)
}

#[derive(Clone, Copy, Debug)]
enum Term {
    Jump(usize),
    Branch(Opnd, usize, usize),
    Ret(Opnd),
}

#[derive(Clone, Debug)]
struct Block {
    phis: Vec<Phi>,
    insts: Vec<Inst>,
    term: Term,
}

#[derive(Clone)]
struct Func {
    blocks: Vec<Block>,
    params: Vec<Var>,
    names: Vec<String>,
    orig: Vec<Var>, // pre-SSA variable each version came from
    nslots: usize,
}

fn var_of(o: Opnd) -> Option<Var> {
    match o {
        Opnd::Var(v) => Some(v),
        Opnd::Const(_) => None,
    }
}

impl Rhs {
    fn operands_mut(&mut self) -> Vec<&mut Opnd> {
        match self {
            Rhs::Copy(a) => vec![a],
            Rhs::Bin(_, a, b) => vec![a, b],
            Rhs::Load(_) => vec![],
        }
    }
}
impl Inst {
    fn operands_mut(&mut self) -> Vec<&mut Opnd> {
        match self {
            Inst::Def(_, r) => r.operands_mut(),
            Inst::Store(_, o) => vec![o],
        }
    }
    fn def(&self) -> Option<Var> {
        match self {
            Inst::Def(d, _) => Some(*d),
            Inst::Store(..) => None,
        }
    }
    fn uses(&self) -> Vec<Var> {
        match self {
            Inst::Def(_, Rhs::Copy(a)) => var_of(*a).into_iter().collect(),
            Inst::Def(_, Rhs::Bin(_, a, b)) => [*a, *b].into_iter().filter_map(var_of).collect(),
            Inst::Def(_, Rhs::Load(_)) => vec![],
            Inst::Store(_, o) => var_of(*o).into_iter().collect(),
        }
    }
}
impl Term {
    fn succs(&self) -> Vec<usize> {
        match *self {
            Term::Jump(t) => vec![t],
            Term::Branch(_, t, e) => vec![t, e],
            Term::Ret(_) => vec![],
        }
    }
    fn operands_mut(&mut self) -> Vec<&mut Opnd> {
        match self {
            Term::Jump(_) => vec![],
            Term::Branch(c, ..) => vec![c],
            Term::Ret(o) => vec![o],
        }
    }
    fn uses(&self) -> Vec<Var> {
        match *self {
            Term::Branch(c, ..) => var_of(c).into_iter().collect(),
            Term::Ret(o) => var_of(o).into_iter().collect(),
            Term::Jump(_) => vec![],
        }
    }
    fn retarget(&mut self, from: usize, to: usize) {
        match self {
            Term::Jump(t) if *t == from => *t = to,
            Term::Branch(_, t, e) => {
                if *t == from {
                    *t = to;
                }
                if *e == from {
                    *e = to;
                }
            }
            _ => {}
        }
    }
}

impl Func {
    fn new_var(&mut self, name: String, orig: Option<Var>) -> Var {
        let v = self.names.len();
        self.names.push(name);
        self.orig.push(orig.unwrap_or(v));
        v
    }
    fn new_block(&mut self) -> usize {
        self.blocks.push(Block {
            phis: vec![],
            insts: vec![],
            term: Term::Ret(Opnd::Const(0)),
        });
        self.blocks.len() - 1
    }
    fn preds(&self) -> Vec<Vec<usize>> {
        let mut p = vec![vec![]; self.blocks.len()];
        for (b, blk) in self.blocks.iter().enumerate() {
            for s in blk.term.succs() {
                if !p[s].contains(&b) {
                    p[s].push(b);
                }
            }
        }
        p
    }
    fn show(&self, o: Opnd) -> String {
        match o {
            Opnd::Const(c) => c.to_string(),
            Opnd::Var(v) => self.names[v].clone(),
        }
    }
    fn count_insts(&self) -> usize {
        self.blocks
            .iter()
            .map(|b| b.insts.len() + b.phis.len())
            .sum()
    }
}

impl fmt::Display for Func {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        let params: Vec<_> = self.params.iter().map(|&p| self.names[p].clone()).collect();
        writeln!(w, "fn({})", params.join(", "))?;
        for (b, blk) in self.blocks.iter().enumerate() {
            writeln!(w, "  b{b}:")?;
            for ph in &blk.phis {
                let args: Vec<_> = ph
                    .args
                    .iter()
                    .map(|&(p, o)| format!("b{p}:{}", self.show(o)))
                    .collect();
                writeln!(w, "    {} = phi({})", self.names[ph.dst], args.join(", "))?;
            }
            for i in &blk.insts {
                match i {
                    Inst::Def(d, Rhs::Copy(a)) => {
                        writeln!(w, "    {} = {}", self.names[*d], self.show(*a))?
                    }
                    Inst::Def(d, Rhs::Bin(op, a, c)) => writeln!(
                        w,
                        "    {} = {} {} {}",
                        self.names[*d],
                        self.show(*a),
                        op.sym(),
                        self.show(*c)
                    )?,
                    Inst::Def(d, Rhs::Load(s)) => {
                        writeln!(w, "    {} = load slot{s}", self.names[*d])?
                    }
                    Inst::Store(s, o) => writeln!(w, "    store slot{s}, {}", self.show(*o))?,
                }
            }
            match blk.term {
                Term::Jump(t) => writeln!(w, "    jump b{t}")?,
                Term::Branch(c, t, e) => writeln!(w, "    br {}, b{t}, b{e}", self.show(c))?,
                Term::Ret(o) => writeln!(w, "    ret {}", self.show(o))?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- source language and lowering to a CFG

#[derive(Clone, Debug)]
enum Expr {
    Const(i64),
    Var(usize),
    Bin(BinOp, Box<Expr>, Box<Expr>),
}
#[derive(Clone, Debug)]
enum Stmt {
    Assign(usize, Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
}
#[derive(Clone, Debug)]
struct Prog {
    nparams: usize,
    nvars: usize,
    body: Vec<Stmt>,
    ret: Expr,
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Bin(op, Box::new(a), Box::new(b))
}
fn v(i: usize) -> Expr {
    Expr::Var(i)
}
fn c(x: i64) -> Expr {
    Expr::Const(x)
}

/// The oracle: a direct tree-walking evaluator of the source language.
fn eval_expr(e: &Expr, env: &[i64]) -> i64 {
    match e {
        Expr::Const(x) => *x,
        Expr::Var(i) => env[*i],
        Expr::Bin(op, a, b) => op.eval(eval_expr(a, env), eval_expr(b, env)),
    }
}
fn exec(ss: &[Stmt], env: &mut Vec<i64>) {
    for s in ss {
        match s {
            Stmt::Assign(d, e) => env[*d] = eval_expr(e, env),
            Stmt::If(cond, t, e) => exec(if eval_expr(cond, env) != 0 { t } else { e }, env),
            Stmt::While(cond, body) => {
                while eval_expr(cond, env) != 0 {
                    exec(body, env);
                }
            }
        }
    }
}
fn eval_prog(p: &Prog, args: &[i64]) -> i64 {
    let mut env = vec![0; p.nvars];
    env[..p.nparams].copy_from_slice(args);
    exec(&p.body, &mut env);
    eval_expr(&p.ret, &env)
}

struct Lowerer {
    f: Func,
    cur: usize,
}
impl Lowerer {
    fn emit(&mut self, i: Inst) {
        self.f.blocks[self.cur].insts.push(i);
    }
    fn temp(&mut self) -> Var {
        let n = self.f.names.len();
        self.f.new_var(format!("t{n}"), None)
    }
    fn expr(&mut self, e: &Expr) -> Opnd {
        match e {
            Expr::Const(x) => Opnd::Const(*x),
            Expr::Var(i) => Opnd::Var(*i),
            Expr::Bin(op, a, b) => {
                let (x, y) = (self.expr(a), self.expr(b));
                let t = self.temp();
                self.emit(Inst::Def(t, Rhs::Bin(*op, x, y)));
                Opnd::Var(t)
            }
        }
    }
    fn expr_into(&mut self, dst: Var, e: &Expr) {
        if let Expr::Bin(op, a, b) = e {
            let (x, y) = (self.expr(a), self.expr(b));
            self.emit(Inst::Def(dst, Rhs::Bin(*op, x, y)));
        } else {
            let o = self.expr(e);
            self.emit(Inst::Def(dst, Rhs::Copy(o)));
        }
    }
    fn set_term(&mut self, t: Term) {
        self.f.blocks[self.cur].term = t;
    }
    fn stmts(&mut self, ss: &[Stmt]) {
        for s in ss {
            match s {
                Stmt::Assign(d, e) => self.expr_into(*d, e),
                Stmt::If(cond, t, e) => {
                    let k = self.expr(cond);
                    let (bt, be, bj) = (self.f.new_block(), self.f.new_block(), self.f.new_block());
                    self.set_term(Term::Branch(k, bt, be));
                    self.cur = bt;
                    self.stmts(t);
                    self.set_term(Term::Jump(bj));
                    self.cur = be;
                    self.stmts(e);
                    self.set_term(Term::Jump(bj));
                    self.cur = bj;
                }
                Stmt::While(cond, body) => {
                    let (h, b, x) = (self.f.new_block(), self.f.new_block(), self.f.new_block());
                    self.set_term(Term::Jump(h));
                    self.cur = h;
                    let k = self.expr(cond);
                    self.set_term(Term::Branch(k, b, x));
                    self.cur = b;
                    self.stmts(body);
                    self.set_term(Term::Jump(h));
                    self.cur = x;
                }
            }
        }
    }
}

fn lower_prog(p: &Prog) -> Func {
    let names = (0..p.nvars)
        .map(|i| {
            if i < p.nparams {
                format!("p{i}")
            } else {
                format!("v{i}")
            }
        })
        .collect();
    let f = Func {
        blocks: vec![],
        params: (0..p.nparams).collect(),
        names,
        orig: (0..p.nvars).collect(),
        nslots: 0,
    };
    let mut l = Lowerer { f, cur: 0 };
    l.cur = l.f.new_block();
    l.stmts(&p.body);
    let r = l.expr(&p.ret);
    l.set_term(Term::Ret(r));
    l.f
}

// ---------------------------------------------------------------- interpreters (the verification backbone)

fn val(vars: &[i64], o: Opnd) -> i64 {
    match o {
        Opnd::Const(c) => c,
        Opnd::Var(v) => vars[v],
    }
}

/// Runs the IR in any of its forms (pre-SSA, SSA with phis, lowered, spilled). Phis are evaluated in parallel on entry.
fn interp(f: &Func, args: &[i64]) -> i64 {
    let mut vars = vec![0i64; f.names.len()];
    let mut slots = vec![0i64; f.nslots];
    for (p, a) in f.params.iter().zip(args) {
        vars[*p] = *a;
    }
    let (mut b, mut prev, mut steps) = (0usize, usize::MAX, 0u64);
    loop {
        let blk = &f.blocks[b];
        let incoming: Vec<(Var, i64)> = blk
            .phis
            .iter()
            .map(|ph| {
                let (_, o) = ph
                    .args
                    .iter()
                    .find(|(q, _)| *q == prev)
                    .expect("phi lacks an argument for the predecessor");
                (ph.dst, val(&vars, *o))
            })
            .collect();
        for (d, x) in incoming {
            vars[d] = x;
        }
        for i in &blk.insts {
            match i {
                Inst::Def(d, Rhs::Copy(a)) => vars[*d] = val(&vars, *a),
                Inst::Def(d, Rhs::Bin(op, a, c)) => {
                    vars[*d] = op.eval(val(&vars, *a), val(&vars, *c))
                }
                Inst::Def(d, Rhs::Load(s)) => vars[*d] = slots[*s],
                Inst::Store(s, o) => slots[*s] = val(&vars, *o),
            }
        }
        steps += blk.insts.len() as u64 + 1;
        assert!(steps < 5_000_000, "interpreter ran away");
        prev = b;
        match blk.term {
            Term::Jump(t) => b = t,
            Term::Branch(k, t, e) => b = if val(&vars, k) != 0 { t } else { e },
            Term::Ret(o) => return val(&vars, o),
        }
    }
}

// ---------------------------------------------------------------- CFG: reverse postorder, dominators, dominance frontiers

fn rpo_order(f: &Func) -> Vec<usize> {
    let mut seen = vec![false; f.blocks.len()];
    let mut post = vec![];
    let mut stack = vec![(0usize, 0usize)];
    seen[0] = true;
    while let Some(&(b, i)) = stack.last() {
        let succs = f.blocks[b].term.succs();
        if i < succs.len() {
            stack.last_mut().unwrap().1 += 1;
            let s = succs[i];
            if !seen[s] {
                seen[s] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(b);
            stack.pop();
        }
    }
    post.reverse();
    post
}

struct DomInfo {
    idom: Vec<usize>, // usize::MAX = unreachable; idom[0] = 0
    rpo: Vec<usize>,
    children: Vec<Vec<usize>>,
    frontier: Vec<BTreeSet<usize>>,
    rounds: usize,
}

impl DomInfo {
    fn reachable(&self, b: usize) -> bool {
        self.idom[b] != usize::MAX
    }
    fn dominates(&self, a: usize, mut b: usize) -> bool {
        loop {
            if a == b {
                return true;
            }
            if b == 0 || self.idom[b] == usize::MAX {
                return false;
            }
            b = self.idom[b];
        }
    }
}

/// Cooper-Harvey-Kennedy: iterate idom[b] = intersect(processed preds) in reverse postorder until nothing changes.
fn compute_dom(f: &Func) -> DomInfo {
    let n = f.blocks.len();
    let rpo = rpo_order(f);
    let mut num = vec![usize::MAX; n];
    for (i, &b) in rpo.iter().enumerate() {
        num[b] = i;
    }
    let preds = f.preds();
    let mut idom = vec![usize::MAX; n];
    idom[0] = 0;
    let (mut changed, mut rounds) = (true, 0);
    while changed {
        changed = false;
        rounds += 1;
        for &b in rpo.iter().skip(1) {
            let mut new = usize::MAX;
            for &p in &preds[b] {
                if idom[p] == usize::MAX {
                    continue; // not processed yet (or unreachable)
                }
                new = if new == usize::MAX {
                    p
                } else {
                    intersect(&idom, &num, p, new)
                };
            }
            if idom[b] != new {
                idom[b] = new;
                changed = true;
            }
        }
    }
    let mut children = vec![vec![]; n];
    for &b in rpo.iter().skip(1) {
        children[idom[b]].push(b);
    }
    // Frontier: walk up from each predecessor of a join until we reach the join's idom.
    let mut frontier = vec![BTreeSet::new(); n];
    for &b in &rpo {
        let ps: Vec<usize> = preds[b]
            .iter()
            .copied()
            .filter(|&p| idom[p] != usize::MAX)
            .collect();
        if ps.len() >= 2 {
            for p in ps {
                let mut runner = p;
                while runner != idom[b] {
                    frontier[runner].insert(b);
                    runner = idom[runner];
                }
            }
        }
    }
    DomInfo {
        idom,
        rpo,
        children,
        frontier,
        rounds,
    }
}

/// Two fingers walk up the (partial) dominator tree; entry has the smallest RPO number so deeper nodes have larger ones.
fn intersect(idom: &[usize], num: &[usize], mut a: usize, mut b: usize) -> usize {
    while a != b {
        while num[a] > num[b] {
            a = idom[a];
        }
        while num[b] > num[a] {
            b = idom[b];
        }
    }
    a
}

/// Textbook oracle: Dom(b) = {b} union intersection of Dom(p) over predecessors, from the "everything" start state.
fn naive_dom_sets(f: &Func) -> Vec<BTreeSet<usize>> {
    let n = f.blocks.len();
    let all: BTreeSet<usize> = (0..n).collect();
    let mut dom = vec![all; n];
    dom[0] = BTreeSet::from([0]);
    let preds = f.preds();
    let mut changed = true;
    while changed {
        changed = false;
        for b in 1..n {
            let mut it = preds[b].iter().map(|&p| dom[p].clone());
            let mut new = it.next().unwrap_or_default();
            for s in it {
                new = new.intersection(&s).copied().collect();
            }
            new.insert(b);
            if new != dom[b] {
                dom[b] = new;
                changed = true;
            }
        }
    }
    dom
}

fn check_dominators(f: &Func, d: &DomInfo) {
    let sets = naive_dom_sets(f);
    let preds = f.preds();
    for b in 0..f.blocks.len() {
        assert!(d.reachable(b), "generated CFGs are fully reachable");
        let chain: BTreeSet<usize> = (0..f.blocks.len()).filter(|&a| d.dominates(a, b)).collect();
        assert_eq!(chain, sets[b], "dominator set of b{b}");
    }
    // DF(x) = { y : x dominates a predecessor of y, but does not strictly dominate y }
    for x in 0..f.blocks.len() {
        let want: BTreeSet<usize> = (0..f.blocks.len())
            .filter(|&y| {
                preds[y].iter().any(|&p| d.dominates(x, p)) && !(x != y && d.dominates(x, y))
            })
            .collect();
        assert_eq!(d.frontier[x], want, "dominance frontier of b{x}");
    }
}

/// Natural-loop nesting depth per block, from back edges t->h where h dominates t.
fn loop_depths(f: &Func, d: &DomInfo) -> Vec<u32> {
    let preds = f.preds();
    let mut bodies: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (t, blk) in f.blocks.iter().enumerate() {
        for h in blk.term.succs() {
            if d.dominates(h, t) {
                let body = bodies.entry(h).or_default();
                body.insert(h);
                let mut work = vec![t];
                while let Some(x) = work.pop() {
                    if body.insert(x) {
                        work.extend(preds[x].iter().copied());
                    }
                }
            }
        }
    }
    let mut depth = vec![0; f.blocks.len()];
    for body in bodies.values() {
        for &b in body {
            depth[b] += 1;
        }
    }
    depth
}

// ---------------------------------------------------------------- SSA construction

fn fresh_version(f: &mut Func, o: Var, counter: &mut [usize]) -> Var {
    counter[o] += 1;
    let name = format!("{}.{}", f.names[o], counter[o]);
    f.new_var(name, Some(o))
}

fn rename(
    f: &mut Func,
    b: usize,
    dom: &DomInfo,
    stacks: &mut Vec<Vec<Var>>,
    counter: &mut Vec<usize>,
) {
    let mut pushed: Vec<Var> = vec![];
    for i in 0..f.blocks[b].phis.len() {
        let o = f.orig[f.blocks[b].phis[i].dst];
        let nv = fresh_version(f, o, counter);
        f.blocks[b].phis[i].dst = nv;
        stacks[o].push(nv);
        pushed.push(o);
    }
    if b == 0 {
        for i in 0..f.params.len() {
            let o = f.params[i];
            let nv = fresh_version(f, o, counter);
            f.params[i] = nv;
            stacks[o].push(nv);
            pushed.push(o);
        }
    }
    // An unwritten variable reads as 0: the interpreter zero-initialises, and such phi inputs are dead anyway.
    let top = |stacks: &Vec<Vec<Var>>, v: Var| {
        stacks[v]
            .last()
            .map(|&n| Opnd::Var(n))
            .unwrap_or(Opnd::Const(0))
    };
    let mut insts = std::mem::take(&mut f.blocks[b].insts);
    for inst in insts.iter_mut() {
        for op in inst.operands_mut() {
            if let Opnd::Var(x) = *op {
                *op = top(stacks, x);
            }
        }
        if let Inst::Def(d, _) = inst {
            let o = *d;
            let nv = fresh_version(f, o, counter);
            *d = nv;
            stacks[o].push(nv);
            pushed.push(o);
        }
    }
    f.blocks[b].insts = insts;
    for op in f.blocks[b].term.operands_mut() {
        if let Opnd::Var(x) = *op {
            *op = top(stacks, x);
        }
    }
    let mut succs = f.blocks[b].term.succs();
    succs.dedup();
    for s in succs {
        for ph in f.blocks[s].phis.iter_mut() {
            let o = f.orig[ph.dst];
            ph.args.push((b, top(stacks, o)));
        }
    }
    for &ch in &dom.children[b] {
        rename(f, ch, dom, stacks, counter);
    }
    for o in pushed {
        stacks[o].pop();
    }
}

/// Returns the number of phis inserted. Minimal SSA: a phi for every iterated-frontier block, dead ones removed later.
fn to_ssa(f: &mut Func, dom: &DomInfo) -> usize {
    let n_orig = f.names.len();
    let mut defsites: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_orig];
    for &p in &f.params {
        defsites[p].insert(0);
    }
    for (b, blk) in f.blocks.iter().enumerate() {
        for i in &blk.insts {
            if let Some(d) = i.def() {
                defsites[d].insert(b);
            }
        }
    }
    let mut count = 0;
    let mut placed: HashSet<(usize, Var)> = HashSet::new();
    for x in 0..n_orig {
        let mut work: Vec<usize> = defsites[x].iter().copied().collect();
        while let Some(b) = work.pop() {
            for &d in &dom.frontier[b] {
                if placed.insert((d, x)) {
                    f.blocks[d].phis.push(Phi {
                        dst: x,
                        args: vec![],
                    });
                    count += 1;
                    if !defsites[x].contains(&d) {
                        work.push(d); // a phi is itself a definition: iterate the frontier
                    }
                }
            }
        }
    }
    let mut stacks = vec![vec![]; n_orig];
    let mut counter = vec![0; n_orig];
    rename(f, 0, dom, &mut stacks, &mut counter);
    count
}

/// Asserts single assignment, phi shape, and that every use is dominated by its definition. Returns the number of defs.
fn check_ssa(f: &Func) -> usize {
    let dom = compute_dom(f);
    let preds = f.preds();
    let mut site: HashMap<Var, (usize, i64)> = HashMap::new();
    let mut define = |v: Var, b: usize, pos: i64| {
        assert!(
            site.insert(v, (b, pos)).is_none(),
            "{} defined twice: not SSA",
            f.names[v]
        );
    };
    for &p in &f.params {
        define(p, 0, -1);
    }
    for (b, blk) in f.blocks.iter().enumerate() {
        for ph in &blk.phis {
            define(ph.dst, b, -1);
        }
        for (i, inst) in blk.insts.iter().enumerate() {
            if let Some(d) = inst.def() {
                define(d, b, i as i64);
            }
        }
    }
    for (b, blk) in f.blocks.iter().enumerate() {
        let ok = |x: Var, pos: i64| {
            let (db, dp) = *site
                .get(&x)
                .unwrap_or_else(|| panic!("use of undefined {}", f.names[x]));
            if db == b {
                dp < pos
            } else {
                dom.dominates(db, b)
            }
        };
        for ph in &blk.phis {
            let keys: BTreeSet<usize> = ph.args.iter().map(|a| a.0).collect();
            assert_eq!(
                keys,
                preds[b].iter().copied().collect(),
                "phi args must match predecessors of b{b}"
            );
            for &(p, o) in &ph.args {
                if let Some(x) = var_of(o) {
                    let (db, _) = site[&x];
                    assert!(
                        dom.dominates(db, p),
                        "phi arg {} not dominated at end of b{p}",
                        f.names[x]
                    );
                }
            }
        }
        for (i, inst) in blk.insts.iter().enumerate() {
            for u in inst.uses() {
                assert!(
                    ok(u, i as i64),
                    "use of {} in b{b} not dominated by its def",
                    f.names[u]
                );
            }
        }
        for u in blk.term.uses() {
            assert!(
                ok(u, blk.insts.len() as i64),
                "terminator use of {} not dominated",
                f.names[u]
            );
        }
    }
    site.len()
}

// ---------------------------------------------------------------- SSA optimisations

#[derive(Default, Debug, Clone)]
struct OptStats {
    const_props: usize,
    copy_props: usize,
    folds: usize,
    dead_removed: usize,
    branches_folded: usize,
    blocks_removed: usize,
}

fn simplify_rhs(rhs: &Rhs) -> Option<Opnd> {
    use Opnd::{Const, Var};
    match *rhs {
        Rhs::Bin(op, Const(a), Const(b)) => Some(Const(op.eval(a, b))),
        Rhs::Bin(BinOp::Add, x, Const(0)) | Rhs::Bin(BinOp::Add, Const(0), x) => Some(x),
        Rhs::Bin(BinOp::Sub, x, Const(0)) => Some(x),
        Rhs::Bin(BinOp::Mul, x, Const(1)) | Rhs::Bin(BinOp::Mul, Const(1), x) => Some(x),
        Rhs::Bin(BinOp::Mul, _, Const(0)) | Rhs::Bin(BinOp::Mul, Const(0), _) => Some(Const(0)),
        Rhs::Bin(op, Var(a), Var(b)) if a == b => match op {
            BinOp::Sub | BinOp::Lt | BinOp::Ne => Some(Const(0)),
            BinOp::Eq | BinOp::Le => Some(Const(1)),
            _ => None,
        },
        _ => None,
    }
}

/// Folding plus constant AND copy propagation. In SSA both are the same move: replace uses of `d` by what `d` is a copy of.
fn propagate_and_fold(f: &mut Func, st: &mut OptStats) -> bool {
    let mut changed = false;
    for blk in &mut f.blocks {
        for inst in &mut blk.insts {
            if let Inst::Def(_, rhs) = inst {
                if let Some(o) = simplify_rhs(rhs) {
                    *rhs = Rhs::Copy(o);
                    st.folds += 1;
                    changed = true;
                }
            }
        }
    }
    let mut repl: HashMap<Var, Opnd> = HashMap::new();
    for blk in &f.blocks {
        for inst in &blk.insts {
            if let Inst::Def(d, Rhs::Copy(o)) = inst {
                repl.insert(*d, *o);
            }
        }
        for ph in &blk.phis {
            // A phi whose inputs are all one value (ignoring itself) is just a copy of that value.
            let distinct: BTreeSet<Opnd> = ph
                .args
                .iter()
                .map(|a| a.1)
                .filter(|o| *o != Opnd::Var(ph.dst))
                .collect();
            if distinct.len() == 1 {
                repl.insert(ph.dst, *distinct.iter().next().unwrap());
            }
        }
    }
    if repl.is_empty() {
        return changed;
    }
    let resolve = |mut o: Opnd| {
        for _ in 0..1000 {
            match o {
                Opnd::Var(x) if repl.contains_key(&x) => o = repl[&x],
                _ => break,
            }
        }
        o
    };
    for blk in &mut f.blocks {
        let mut fix = |op: &mut Opnd| {
            if let Opnd::Var(_) = *op {
                let new = resolve(*op);
                if new != *op {
                    match new {
                        Opnd::Const(_) => st.const_props += 1,
                        Opnd::Var(_) => st.copy_props += 1,
                    }
                    *op = new;
                    changed = true;
                }
            }
        };
        for ph in blk.phis.iter_mut() {
            ph.args.iter_mut().for_each(|a| fix(&mut a.1));
        }
        for inst in blk.insts.iter_mut() {
            inst.operands_mut().into_iter().for_each(&mut fix);
        }
        blk.term.operands_mut().into_iter().for_each(&mut fix);
    }
    changed
}

fn remove_unreachable(f: &mut Func, st: &mut OptStats) -> bool {
    let order = rpo_order(f);
    if order.len() == f.blocks.len() {
        return false;
    }
    let mut map = vec![usize::MAX; f.blocks.len()];
    let mut keep: Vec<usize> = order.clone();
    keep.sort();
    for (new, &old) in keep.iter().enumerate() {
        map[old] = new;
    }
    let mut blocks = vec![];
    for &old in &keep {
        let mut blk = f.blocks[old].clone();
        for ph in blk.phis.iter_mut() {
            ph.args.retain(|a| map[a.0] != usize::MAX);
            ph.args.iter_mut().for_each(|a| a.0 = map[a.0]);
        }
        match &mut blk.term {
            Term::Jump(t) => *t = map[*t],
            Term::Branch(_, t, e) => {
                *t = map[*t];
                *e = map[*e];
            }
            Term::Ret(_) => {}
        }
        blocks.push(blk);
    }
    st.blocks_removed += f.blocks.len() - blocks.len();
    f.blocks = blocks;
    true
}

fn simplify_cfg(f: &mut Func, st: &mut OptStats) -> bool {
    let mut changed = false;
    for b in 0..f.blocks.len() {
        match f.blocks[b].term {
            Term::Branch(Opnd::Const(k), t, e) => {
                let (taken, dead) = if k != 0 { (t, e) } else { (e, t) };
                f.blocks[b].term = Term::Jump(taken);
                if dead != taken {
                    f.blocks[dead]
                        .phis
                        .iter_mut()
                        .for_each(|ph| ph.args.retain(|a| a.0 != b));
                }
                st.branches_folded += 1;
                changed = true;
            }
            Term::Branch(_, t, e) if t == e => {
                f.blocks[b].term = Term::Jump(t);
                changed = true;
            }
            _ => {}
        }
    }
    changed | remove_unreachable(f, st)
}

/// Mark-and-sweep from the observable roots (terminators, stores); also kills dead phi cycles that refcounting would keep.
fn dce(f: &mut Func, st: &mut OptStats) -> bool {
    enum Loc {
        Inst(usize, usize),
        Phi(usize, usize),
    }
    let mut loc: HashMap<Var, Loc> = HashMap::new();
    let mut live: HashSet<Var> = HashSet::new();
    let mut work: Vec<Var> = vec![];
    for (b, blk) in f.blocks.iter().enumerate() {
        for (i, ph) in blk.phis.iter().enumerate() {
            loc.insert(ph.dst, Loc::Phi(b, i));
        }
        for (i, inst) in blk.insts.iter().enumerate() {
            match inst.def() {
                Some(d) => {
                    loc.insert(d, Loc::Inst(b, i));
                }
                None => work.extend(inst.uses()),
            }
        }
        work.extend(blk.term.uses());
    }
    while let Some(x) = work.pop() {
        if !live.insert(x) {
            continue;
        }
        match loc.get(&x) {
            Some(&Loc::Inst(b, i)) => work.extend(f.blocks[b].insts[i].uses()),
            Some(&Loc::Phi(b, i)) => {
                work.extend(f.blocks[b].phis[i].args.iter().filter_map(|a| var_of(a.1)))
            }
            None => {} // parameter
        }
    }
    let before = f.count_insts();
    for blk in &mut f.blocks {
        blk.phis.retain(|p| live.contains(&p.dst));
        blk.insts
            .retain(|i| i.def().is_none_or(|d| live.contains(&d)));
    }
    let removed = before - f.count_insts();
    st.dead_removed += removed;
    removed > 0
}

fn optimise(f: &mut Func) -> OptStats {
    let mut st = OptStats::default();
    loop {
        let mut c = propagate_and_fold(f, &mut st);
        c |= simplify_cfg(f, &mut st);
        c |= dce(f, &mut st);
        if !c {
            return st;
        }
    }
}

// ---------------------------------------------------------------- out of SSA: critical edges and parallel copies

/// Turns a PARALLEL copy (all right-hand sides read before any write) into an ordered list. A copy can go once nobody
/// still needs its destination; if only cycles remain (swap!), save one destination in a fresh temporary to break it.
fn sequentialise(
    copies: Vec<(Var, Opnd)>,
    fresh: &mut impl FnMut() -> Var,
) -> (Vec<(Var, Opnd)>, usize) {
    let mut pending: Vec<(Var, Opnd)> = copies
        .into_iter()
        .filter(|&(d, s)| s != Opnd::Var(d))
        .collect();
    let (mut out, mut cycles) = (vec![], 0);
    while !pending.is_empty() {
        let free = pending
            .iter()
            .position(|&(d, _)| !pending.iter().any(|&(_, s)| s == Opnd::Var(d)));
        if let Some(i) = free {
            out.push(pending.remove(i));
        } else {
            let d = pending[0].0;
            let t = fresh();
            out.push((t, Opnd::Var(d)));
            for cp in pending.iter_mut().filter(|cp| cp.1 == Opnd::Var(d)) {
                cp.1 = Opnd::Var(t);
            }
            cycles += 1;
        }
    }
    (out, cycles)
}

#[derive(Clone, Copy)]
struct LowerOpts {
    split_critical: bool,
    parallel: bool,
}
const CORRECT: LowerOpts = LowerOpts {
    split_critical: true,
    parallel: true,
};

#[derive(Default, Debug)]
struct LowerStats {
    edges_split: usize,
    cycles_broken: usize,
    copies: usize,
}

fn lower_phis(f: &mut Func, o: LowerOpts) -> LowerStats {
    let mut st = LowerStats::default();
    let preds = f.preds();
    let nblocks = f.blocks.len();
    for b in 0..nblocks {
        if f.blocks[b].phis.is_empty() {
            continue;
        }
        for &p in &preds[b] {
            let mut edge = p;
            if o.split_critical && f.blocks[p].term.succs().len() > 1 {
                // Critical edge: p has several successors AND b has several predecessors. Give it its own block.
                let n = f.new_block();
                f.blocks[n].term = Term::Jump(b);
                f.blocks[p].term.retarget(b, n);
                for ph in f.blocks[b].phis.iter_mut() {
                    ph.args
                        .iter_mut()
                        .filter(|a| a.0 == p)
                        .for_each(|a| a.0 = n);
                }
                st.edges_split += 1;
                edge = n;
            }
            let copies: Vec<(Var, Opnd)> = f.blocks[b]
                .phis
                .iter()
                .map(|ph| (ph.dst, ph.args.iter().find(|a| a.0 == edge).unwrap().1))
                .collect();
            let seq = if o.parallel {
                let (s, cyc) = sequentialise(copies, &mut || {
                    let n = f.names.len();
                    f.new_var(format!("pc{n}"), None)
                });
                st.cycles_broken += cyc;
                s
            } else {
                copies
                    .into_iter()
                    .filter(|&(d, s)| s != Opnd::Var(d))
                    .collect() // naive: phi order, one by one
            };
            st.copies += seq.len();
            for (d, s) in seq {
                f.blocks[edge].insts.push(Inst::Def(d, Rhs::Copy(s)));
            }
        }
        f.blocks[b].phis.clear();
    }
    st
}

// ---------------------------------------------------------------- liveness and interference

#[derive(Clone, PartialEq)]
struct BitSet(Vec<u64>);
impl BitSet {
    fn new(n: usize) -> Self {
        BitSet(vec![0; n.div_ceil(64)])
    }
    fn insert(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    fn remove(&mut self, i: usize) {
        self.0[i / 64] &= !(1 << (i % 64));
    }
    fn contains(&self, i: usize) -> bool {
        self.0[i / 64] >> (i % 64) & 1 == 1
    }
    fn union(&mut self, o: &BitSet) {
        self.0.iter_mut().zip(&o.0).for_each(|(a, b)| *a |= b);
    }
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.0.len() * 64).filter(|&i| self.contains(i))
    }
}

struct Liveness {
    live_in: Vec<BitSet>,
    live_out: Vec<BitSet>,
    rounds: usize,
}

/// Backward dataflow: out[b] = union of in[succ]; in[b] = use[b] | (out[b] - def[b]); iterate to a fixpoint.
fn liveness(f: &Func) -> Liveness {
    let (n, nv) = (f.blocks.len(), f.names.len());
    let (mut gen_, mut kill) = (vec![BitSet::new(nv); n], vec![BitSet::new(nv); n]);
    for (b, blk) in f.blocks.iter().enumerate() {
        assert!(blk.phis.is_empty(), "liveness runs after phi lowering");
        for inst in &blk.insts {
            for u in inst.uses() {
                if !kill[b].contains(u) {
                    gen_[b].insert(u);
                }
            }
            if let Some(d) = inst.def() {
                kill[b].insert(d);
            }
        }
        for u in blk.term.uses() {
            if !kill[b].contains(u) {
                gen_[b].insert(u);
            }
        }
    }
    let (mut live_in, mut live_out) = (vec![BitSet::new(nv); n], vec![BitSet::new(nv); n]);
    let mut order = rpo_order(f);
    order.reverse(); // postorder converges fastest for a backward problem
    let (mut changed, mut rounds) = (true, 0);
    while changed {
        changed = false;
        rounds += 1;
        for &b in &order {
            let mut out = BitSet::new(nv);
            for s in f.blocks[b].term.succs() {
                out.union(&live_in[s]);
            }
            let mut inn = gen_[b].clone();
            out.iter()
                .filter(|&x| !kill[b].contains(x))
                .for_each(|x| inn.insert(x));
            live_out[b] = out;
            if inn != live_in[b] {
                live_in[b] = inn;
                changed = true;
            }
        }
    }
    let params: HashSet<Var> = f.params.iter().copied().collect();
    assert!(
        live_in[0].iter().all(|x| params.contains(&x)),
        "a variable is used before any definition"
    );
    Liveness {
        live_in,
        live_out,
        rounds,
    }
}

struct Graph {
    adj: Vec<BTreeSet<usize>>,
    present: Vec<bool>,
    moves: Vec<(Var, Var)>,
}

impl Graph {
    fn edge(&mut self, a: usize, b: usize) {
        if a != b {
            self.adj[a].insert(b);
            self.adj[b].insert(a);
        }
    }
    fn edges(&self) -> usize {
        self.adj.iter().map(|s| s.len()).sum::<usize>() / 2
    }
}

/// At each definition d, d interferes with everything live just after it. For `d = s` the value is the same, so s and d may
/// share a register: skipping that edge is what lets the allocator coalesce copies for free.
fn build_graph(f: &Func, lv: &Liveness) -> Graph {
    let nv = f.names.len();
    let mut g = Graph {
        adj: vec![BTreeSet::new(); nv],
        present: vec![false; nv],
        moves: vec![],
    };
    for (b, blk) in f.blocks.iter().enumerate() {
        let mut live = lv.live_out[b].clone();
        for u in blk.term.uses() {
            live.insert(u);
            g.present[u] = true;
        }
        for inst in blk.insts.iter().rev() {
            if let Some(d) = inst.def() {
                g.present[d] = true;
                let src = if let Inst::Def(_, Rhs::Copy(Opnd::Var(s))) = inst {
                    Some(*s)
                } else {
                    None
                };
                if let Some(s) = src {
                    g.moves.push((d, s));
                }
                for l in live.iter() {
                    if Some(l) != src {
                        g.edge(d, l);
                    }
                }
                live.remove(d);
            }
            for u in inst.uses() {
                live.insert(u);
                g.present[u] = true;
            }
        }
    }
    let entry: Vec<usize> = lv.live_in[0].iter().collect(); // arguments arrive simultaneously
    for (i, &a) in entry.iter().enumerate() {
        g.present[a] = true;
        for &b in &entry[i + 1..] {
            g.edge(a, b);
        }
    }
    g
}

// ---------------------------------------------------------------- register allocation

/// Weight of each variable's defs and uses by 10^loop depth: spilling something used in a hot loop is expensive.
fn spill_costs(f: &Func) -> Vec<f64> {
    let depth = loop_depths(f, &compute_dom(f));
    let mut cost = vec![0.0; f.names.len()];
    for (b, blk) in f.blocks.iter().enumerate() {
        let w = 10f64.powi(depth[b] as i32);
        for inst in &blk.insts {
            inst.def()
                .into_iter()
                .chain(inst.uses())
                .for_each(|x| cost[x] += w);
        }
        blk.term.uses().into_iter().for_each(|x| cost[x] += w);
    }
    cost
}

type Alloc = Result<Vec<usize>, Vec<Var>>; // register per variable, or the variables that must be spilled

fn colour_graph(g: &Graph, k: usize, cost: &[f64], pinned: &HashSet<Var>) -> Alloc {
    let nodes: Vec<usize> = (0..g.adj.len()).filter(|&x| g.present[x]).collect();
    let mut degree: Vec<usize> = g.adj.iter().map(|a| a.len()).collect();
    let mut removed = vec![false; g.adj.len()];
    let mut stack = vec![];
    // Simplify: remove low-degree nodes (they are always colourable); when stuck push the cheapest spill candidate
    // optimistically (Briggs): it may still find a free colour at select time because neighbours can share colours.
    while stack.len() < nodes.len() {
        let easy = nodes
            .iter()
            .copied()
            .find(|&x| !removed[x] && degree[x] < k);
        let pick = easy.unwrap_or_else(|| {
            let metric = |x: &usize| {
                if pinned.contains(x) {
                    f64::INFINITY
                } else {
                    cost[*x] / degree[*x].max(1) as f64
                }
            };
            nodes
                .iter()
                .copied()
                .filter(|&x| !removed[x])
                .min_by(|a, b| metric(a).partial_cmp(&metric(b)).unwrap())
                .unwrap()
        });
        removed[pick] = true;
        stack.push(pick);
        for &nb in &g.adj[pick] {
            degree[nb] = degree[nb].saturating_sub(1);
        }
    }
    let mut colour = vec![usize::MAX; g.adj.len()];
    let mut spills = vec![];
    while let Some(x) = stack.pop() {
        let used: HashSet<usize> = g.adj[x]
            .iter()
            .map(|&n| colour[n])
            .filter(|&c| c != usize::MAX)
            .collect();
        // Bias: prefer the colour of a move partner so the copy becomes `mov r, r` and is deleted.
        let partner = g.moves.iter().filter_map(|&(d, s)| {
            if d == x {
                Some(s)
            } else if s == x {
                Some(d)
            } else {
                None
            }
        });
        let biased = partner
            .map(|p| colour[p])
            .find(|&c| c != usize::MAX && !used.contains(&c));
        match biased.or_else(|| (0..k).find(|c| !used.contains(c))) {
            Some(c) => colour[x] = c,
            None => spills.push(x),
        }
    }
    if spills.is_empty() {
        Ok(colour)
    } else {
        Err(spills)
    }
}

/// One interval per variable, positions: use of instruction i at 2i, its definition at 2i+1 (so `d = a + b` lets d reuse
/// a's register), block boundaries extended for variables live across them.
fn linear_scan(f: &Func, lv: &Liveness, k: usize, pinned: &HashSet<Var>) -> Alloc {
    let nv = f.names.len();
    let (mut start, mut end) = (vec![usize::MAX; nv], vec![0usize; nv]);
    let mut touch = |x: Var, p: usize| {
        start[x] = start[x].min(p);
        end[x] = end[x].max(p);
    };
    let mut idx = 0;
    for &b in &rpo_order(f) {
        let blk = &f.blocks[b];
        let first = 2 * idx;
        for inst in &blk.insts {
            inst.uses().into_iter().for_each(|u| touch(u, 2 * idx));
            if let Some(d) = inst.def() {
                touch(d, 2 * idx + 1);
            }
            idx += 1;
        }
        blk.term.uses().into_iter().for_each(|u| touch(u, 2 * idx));
        let last = 2 * idx + 1;
        idx += 1;
        lv.live_in[b].iter().for_each(|x| touch(x, first));
        lv.live_out[b].iter().for_each(|x| touch(x, last));
    }
    let mut order: Vec<Var> = (0..nv).filter(|&x| start[x] != usize::MAX).collect();
    order.sort_by_key(|&x| (start[x], x));
    let mut reg = vec![usize::MAX; nv];
    let mut free: BTreeSet<usize> = (0..k).collect();
    let mut active: Vec<Var> = vec![];
    let mut spills = vec![];
    for x in order {
        active.retain(|&a| {
            let keep = end[a] >= start[x];
            if !keep {
                free.insert(reg[a]);
            }
            keep
        });
        if let Some(&r) = free.iter().next() {
            free.remove(&r);
            reg[x] = r;
            active.push(x);
            continue;
        }
        // All registers busy: spill whichever of (active + current) ends last; that frees the most future room.
        let victim = active
            .iter()
            .copied()
            .chain([x])
            .filter(|a| !pinned.contains(a))
            .max_by_key(|&a| (end[a], a))
            .expect("only unspillable values are live");
        if victim == x {
            spills.push(x);
        } else {
            reg[x] = std::mem::replace(&mut reg[victim], usize::MAX);
            active.retain(|&a| a != victim);
            active.push(x);
            spills.push(victim);
        }
    }
    if spills.is_empty() {
        Ok(reg)
    } else {
        Err(spills)
    }
}

/// Rewrites each spilled variable: a load into a fresh short-lived temp before every use, a store after every definition.
/// The temps are "pinned" (never spilled again) which guarantees the retry loop terminates.
fn spill_rewrite(f: &mut Func, spills: &[Var], pinned: &mut HashSet<Var>) {
    let slot_of: HashMap<Var, usize> = spills
        .iter()
        .enumerate()
        .map(|(i, &x)| (x, f.nslots + i))
        .collect();
    f.nslots += spills.len();
    let mut tmp = |f: &mut Func, x: Var| {
        let n = f.names.len();
        let t = f.new_var(format!("{}.sp{n}", f.names[x]), Some(f.orig[x]));
        pinned.insert(t);
        t
    };
    let mut entry_stores = vec![];
    for i in 0..f.params.len() {
        if let Some(&s) = slot_of.get(&f.params[i]) {
            let t = tmp(f, f.params[i]);
            f.params[i] = t;
            entry_stores.push(Inst::Store(s, Opnd::Var(t)));
        }
    }
    for b in 0..f.blocks.len() {
        let old = std::mem::take(&mut f.blocks[b].insts);
        let mut out: Vec<Inst> = if b == 0 {
            std::mem::take(&mut entry_stores)
        } else {
            vec![]
        };
        for mut inst in old {
            let mut loaded: HashMap<Var, Var> = HashMap::new();
            for op in inst.operands_mut() {
                if let Opnd::Var(x) = *op {
                    if let Some(&s) = slot_of.get(&x) {
                        let t = *loaded.entry(x).or_insert_with(|| {
                            let t = tmp(f, x);
                            out.push(Inst::Def(t, Rhs::Load(s)));
                            t
                        });
                        *op = Opnd::Var(t);
                    }
                }
            }
            let spilled_def = inst.def().and_then(|d| slot_of.get(&d).map(|&s| (d, s)));
            if let (Some((d, s)), Inst::Def(dst, _)) = (spilled_def, &mut inst) {
                let t = tmp(f, d);
                *dst = t;
                out.push(inst);
                out.push(Inst::Store(s, Opnd::Var(t)));
            } else {
                out.push(inst);
            }
        }
        let mut term = f.blocks[b].term;
        for op in term.operands_mut() {
            if let Opnd::Var(x) = *op {
                if let Some(&s) = slot_of.get(&x) {
                    let t = tmp(f, x);
                    out.push(Inst::Def(t, Rhs::Load(s)));
                    *op = Opnd::Var(t);
                }
            }
        }
        f.blocks[b].term = term;
        f.blocks[b].insts = out;
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Strategy {
    GraphColouring,
    LinearScan,
}

struct Allocated {
    func: Func,
    reg: Vec<usize>,
    spilled: usize, // original variables that went to stack slots
    rounds: usize,
}

fn allocate(f: &Func, k: usize, strat: Strategy) -> Allocated {
    let mut f = f.clone();
    let (mut pinned, mut spilled) = (HashSet::new(), 0);
    for rounds in 1..=30 {
        let lv = liveness(&f);
        let res = match strat {
            Strategy::GraphColouring => {
                colour_graph(&build_graph(&f, &lv), k, &spill_costs(&f), &pinned)
            }
            Strategy::LinearScan => linear_scan(&f, &lv, k, &pinned),
        };
        match res {
            Ok(reg) => {
                return Allocated {
                    func: f,
                    reg,
                    spilled,
                    rounds,
                };
            }
            Err(sp) => {
                assert!(
                    sp.iter().all(|x| !pinned.contains(x)),
                    "tried to spill a spill temp: K too small"
                );
                spilled += sp.len();
                spill_rewrite(&mut f, &sp, &mut pinned);
            }
        }
    }
    panic!("allocation did not converge");
}

/// The invariant the whole allocator exists for: interfering variables never share a register.
fn check_allocation(a: &Allocated, k: usize) -> usize {
    let lv = liveness(&a.func);
    let g = build_graph(&a.func, &lv);
    for x in (0..g.adj.len()).filter(|&x| g.present[x]) {
        assert!(a.reg[x] < k, "{} has no register", a.func.names[x]);
        for &y in &g.adj[x] {
            assert_ne!(
                a.reg[x], a.reg[y],
                "{} and {} interfere but share r{}",
                a.func.names[x], a.func.names[y], a.reg[x]
            );
        }
    }
    g.edges()
}

// ---------------------------------------------------------------- machine code

#[derive(Clone, Copy, Debug)]
enum MOp {
    R(usize),
    I(i64),
}
#[derive(Clone, Debug)]
enum MInst {
    Mov(usize, MOp),
    Bin(BinOp, usize, MOp, MOp),
    Ld(usize, usize),
    St(usize, MOp),
    Jmp(usize),
    Bnz(MOp, usize),
    Ret(MOp),
}
struct MCode {
    insts: Vec<MInst>,
    label_pc: Vec<usize>, // block id -> pc
    layout: Vec<usize>,
    param_regs: Vec<Option<usize>>,
    k: usize,
    nslots: usize,
    elided_moves: usize,
}

fn emit_machine(a: &Allocated, k: usize) -> MCode {
    let f = &a.func;
    let layout = rpo_order(f);
    let mop = |o: Opnd| match o {
        Opnd::Const(c) => MOp::I(c),
        Opnd::Var(x) => MOp::R(a.reg[x]),
    };
    let (mut insts, mut label_pc, mut elided) = (vec![], vec![0; f.blocks.len()], 0);
    for (i, &b) in layout.iter().enumerate() {
        label_pc[b] = insts.len();
        for inst in &f.blocks[b].insts {
            match inst {
                Inst::Def(d, Rhs::Copy(Opnd::Var(s))) if a.reg[*d] == a.reg[*s] => elided += 1, // coalesced
                Inst::Def(d, Rhs::Copy(o)) => insts.push(MInst::Mov(a.reg[*d], mop(*o))),
                Inst::Def(d, Rhs::Bin(op, x, y)) => {
                    insts.push(MInst::Bin(*op, a.reg[*d], mop(*x), mop(*y)))
                }
                Inst::Def(d, Rhs::Load(s)) => insts.push(MInst::Ld(a.reg[*d], *s)),
                Inst::Store(s, o) => insts.push(MInst::St(*s, mop(*o))),
            }
        }
        let next = layout.get(i + 1).copied();
        match f.blocks[b].term {
            Term::Jump(t) => {
                if next != Some(t) {
                    insts.push(MInst::Jmp(t));
                }
            }
            Term::Branch(cnd, t, e) => {
                insts.push(MInst::Bnz(mop(cnd), t));
                if next != Some(e) {
                    insts.push(MInst::Jmp(e));
                }
            }
            Term::Ret(o) => insts.push(MInst::Ret(mop(o))),
        }
    }
    let param_regs = f
        .params
        .iter()
        .map(|&p| Some(a.reg[p]).filter(|&r| r != usize::MAX))
        .collect();
    MCode {
        insts,
        label_pc,
        layout,
        param_regs,
        k,
        nslots: f.nslots,
        elided_moves: elided,
    }
}

fn run_machine(m: &MCode, args: &[i64]) -> i64 {
    let mut regs = vec![0i64; m.k];
    let mut slots = vec![0i64; m.nslots];
    for (r, a) in m.param_regs.iter().zip(args) {
        if let Some(r) = r {
            regs[*r] = *a;
        }
    }
    let rd = |regs: &[i64], o: MOp| match o {
        MOp::R(r) => regs[r],
        MOp::I(c) => c,
    };
    let (mut pc, mut steps) = (0, 0u64);
    loop {
        steps += 1;
        assert!(steps < 20_000_000, "machine ran away");
        match &m.insts[pc] {
            MInst::Mov(d, o) => regs[*d] = rd(&regs, *o),
            MInst::Bin(op, d, a, b) => regs[*d] = op.eval(rd(&regs, *a), rd(&regs, *b)),
            MInst::Ld(d, s) => regs[*d] = slots[*s],
            MInst::St(s, o) => slots[*s] = rd(&regs, *o),
            MInst::Jmp(t) => {
                pc = m.label_pc[*t];
                continue;
            }
            MInst::Bnz(o, t) => {
                if rd(&regs, *o) != 0 {
                    pc = m.label_pc[*t];
                    continue;
                }
            }
            MInst::Ret(o) => return rd(&regs, *o),
        }
        pc += 1;
    }
}

impl fmt::Display for MOp {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MOp::R(r) => write!(w, "r{r}"),
            MOp::I(c) => write!(w, "#{c}"),
        }
    }
}
impl fmt::Display for MCode {
    fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
        let labels: HashMap<usize, usize> =
            self.layout.iter().map(|&b| (self.label_pc[b], b)).collect();
        for (pc, i) in self.insts.iter().enumerate() {
            if let Some(b) = labels.get(&pc) {
                writeln!(w, ".L{b}:")?;
            }
            match i {
                MInst::Mov(d, o) => writeln!(w, "    mov r{d}, {o}")?,
                MInst::Bin(op, d, a, b) => writeln!(w, "    {} r{d}, {a}, {b}", op.name())?,
                MInst::Ld(d, s) => writeln!(w, "    ld  r{d}, [sp+{s}]")?,
                MInst::St(s, o) => writeln!(w, "    st  [sp+{s}], {o}")?,
                MInst::Jmp(t) => writeln!(w, "    jmp .L{t}")?,
                MInst::Bnz(o, t) => writeln!(w, "    bnz {o}, .L{t}")?,
                MInst::Ret(o) => writeln!(w, "    ret {o}")?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- random structured programs

struct Gen<'a> {
    rng: &'a mut Rng,
    nreg: usize,
    ncounters: usize,
}
impl Gen<'_> {
    fn expr(&mut self, depth: u32) -> Expr {
        if depth == 0 || self.rng.chance(25) {
            return if self.rng.chance(60) {
                v(self.rng.below(self.nreg as u64) as usize)
            } else {
                c(self.rng.range(-3, 8))
            };
        }
        let op = [
            BinOp::Add,
            BinOp::Add,
            BinOp::Sub,
            BinOp::Mul,
            BinOp::Lt,
            BinOp::Le,
            BinOp::Eq,
            BinOp::Ne,
        ][self.rng.below(8) as usize];
        bin(op, self.expr(depth - 1), self.expr(depth - 1))
    }
    fn stmts(&mut self, n: u64, depth: u32) -> Vec<Stmt> {
        let mut out = vec![];
        for _ in 0..n {
            let roll = self.rng.below(100);
            if roll < 60 || depth >= 2 {
                out.push(Stmt::Assign(
                    self.rng.below(self.nreg as u64) as usize,
                    self.expr(3),
                ));
            } else if roll < 82 {
                let cond = if self.rng.chance(10) {
                    c(self.rng.range(0, 1))
                } else {
                    self.expr(2)
                };
                let (a, b) = (self.rng.range(1, 3) as u64, self.rng.range(0, 2) as u64);
                out.push(Stmt::If(
                    cond,
                    self.stmts(a, depth + 1),
                    self.stmts(b, depth + 1),
                ));
            } else {
                // Bounded loop with a private counter nobody else assigns, so every program terminates.
                let ctr = self.nreg + self.ncounters;
                self.ncounters += 1;
                let (bound, n_body) = (self.rng.range(1, 4), self.rng.range(1, 3) as u64);
                let mut body = self.stmts(n_body, depth + 1);
                body.push(Stmt::Assign(ctr, bin(BinOp::Add, v(ctr), c(1))));
                out.push(Stmt::Assign(ctr, c(0)));
                out.push(Stmt::While(bin(BinOp::Lt, v(ctr), c(bound)), body));
            }
        }
        out
    }
}

fn gen_prog(rng: &mut Rng) -> Prog {
    let nparams = 2;
    let nreg = rng.range(4, 8) as usize;
    let mut g = Gen {
        rng,
        nreg,
        ncounters: 0,
    };
    let mut body: Vec<Stmt> = (nparams..nreg).map(|i| Stmt::Assign(i, c(0))).collect();
    for (i, s) in body.iter_mut().enumerate() {
        if let Stmt::Assign(_, e) = s {
            *e = c(i as i64 + 1);
        }
    }
    let n = g.rng.range(3, 7) as u64;
    body.extend(g.stmts(n, 0));
    // Observe every variable so none is trivially dead and register pressure stays high.
    let mut ret = v(0);
    for i in 1..nreg {
        ret = bin(BinOp::Add, ret, bin(BinOp::Mul, v(i), c(i as i64 + 1)));
    }
    Prog {
        nparams,
        nvars: nreg + g.ncounters,
        body,
        ret,
    }
}

// ---------------------------------------------------------------- the whole-pipeline differential check

#[derive(Default)]
struct Report {
    phis: usize,
    ssa_defs: usize,
    opt: OptStats,
    lower: LowerStats,
    insts_before: usize,
    insts_after: usize,
    spills: [[usize; 2]; 4],    // [K-3][strategy]
    code_size: [[usize; 2]; 4], // static instruction count
    mem_ops: [[usize; 2]; 4],   // ld + st
    elided: [[usize; 2]; 4],
}

fn compile_and_check(f0: &Func, inputs: &[Vec<i64>], expected: &dyn Fn(&[i64]) -> i64) -> Report {
    let mut r = Report::default();
    let want: Vec<i64> = inputs.iter().map(|a| expected(a)).collect();
    let agree = |f: &Func, what: &str| {
        for (a, w) in inputs.iter().zip(&want) {
            assert_eq!(interp(f, a), *w, "{what} disagrees on {a:?}");
        }
    };
    agree(f0, "pre-SSA IR");
    let dom = compute_dom(f0);
    check_dominators(f0, &dom);
    let mut ssa = f0.clone();
    r.phis = to_ssa(&mut ssa, &dom);
    r.ssa_defs = check_ssa(&ssa);
    agree(&ssa, "SSA");
    let mut opt = ssa.clone();
    r.insts_before = opt.count_insts();
    r.opt = optimise(&mut opt);
    r.insts_after = opt.count_insts();
    check_ssa(&opt);
    agree(&opt, "optimised SSA");
    let mut low = opt.clone();
    r.lower = lower_phis(&mut low, CORRECT);
    assert!(low.blocks.iter().all(|b| b.phis.is_empty()));
    agree(&low, "phi-lowered IR");
    for k in 3..=6 {
        for (si, strat) in [Strategy::GraphColouring, Strategy::LinearScan]
            .into_iter()
            .enumerate()
        {
            let a = allocate(&low, k, strat);
            check_allocation(&a, k);
            agree(&a.func, "spilled IR");
            let m = emit_machine(&a, k);
            for (args, w) in inputs.iter().zip(&want) {
                assert_eq!(
                    run_machine(&m, args),
                    *w,
                    "machine code ({strat:?}, K={k}) disagrees on {args:?}"
                );
            }
            r.spills[k - 3][si] += a.spilled;
            r.code_size[k - 3][si] += m.insts.len();
            r.mem_ops[k - 3][si] += m
                .insts
                .iter()
                .filter(|i| matches!(i, MInst::Ld(..) | MInst::St(..)))
                .count();
            r.elided[k - 3][si] += m.elided_moves;
        }
    }
    r
}

// ---------------------------------------------------------------- demos and tests

fn demo() {
    // s = 0; i = 0; t = 2 * 3; unused = k * 7; while i < n { s = s + i * t; i = i + 1 }; return s + k * 1
    let (n, k, s, i, t, u) = (0, 1, 2, 3, 4, 5);
    let p = Prog {
        nparams: 2,
        nvars: 6,
        body: vec![
            Stmt::Assign(s, c(0)),
            Stmt::Assign(i, c(0)),
            Stmt::Assign(t, bin(BinOp::Mul, c(2), c(3))),
            Stmt::Assign(u, bin(BinOp::Mul, v(k), c(7))),
            Stmt::While(
                bin(BinOp::Lt, v(i), v(n)),
                vec![
                    Stmt::Assign(s, bin(BinOp::Add, v(s), bin(BinOp::Mul, v(i), v(t)))),
                    Stmt::Assign(i, bin(BinOp::Add, v(i), c(1))),
                ],
            ),
        ],
        ret: bin(BinOp::Add, v(s), bin(BinOp::Mul, v(k), c(1))),
    };
    let f0 = lower_prog(&p);
    println!("--- IR before SSA ({} insts) ---\n{f0}", f0.count_insts());
    let dom = compute_dom(&f0);
    println!(
        "idom = {:?}, dominance frontiers = {:?}, CHK rounds = {}",
        dom.idom, dom.frontier, dom.rounds
    );
    let mut ssa = f0.clone();
    let phis = to_ssa(&mut ssa, &dom);
    check_ssa(&ssa);
    println!(
        "--- SSA ({} insts, {phis} phis inserted) ---\n{ssa}",
        ssa.count_insts()
    );
    let st = optimise(&mut ssa);
    check_ssa(&ssa);
    println!(
        "--- optimised SSA ({} insts): {st:?} ---\n{ssa}",
        ssa.count_insts()
    );
    lower_phis(&mut ssa, CORRECT);
    let lv = liveness(&ssa);
    let g = build_graph(&ssa, &lv);
    println!(
        "liveness fixpoint in {} rounds; interference graph: {} vars, {} edges",
        lv.rounds,
        g.present.iter().filter(|&&x| x).count(),
        g.edges()
    );
    for kk in [6, 3] {
        let a = allocate(&ssa, kk, Strategy::GraphColouring);
        check_allocation(&a, kk);
        let m = emit_machine(&a, kk);
        println!(
            "--- machine code, K={kk}, graph colouring: {} insts, {} spilled, {} moves coalesced ---\n{m}",
            m.insts.len(),
            a.spilled,
            m.elided_moves
        );
        for arg in [[5, 3], [0, 9], [8, -2]] {
            assert_eq!(run_machine(&m, &arg), eval_prog(&p, &arg));
        }
    }
}

fn test_sequentialiser() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let mut cycles = 0;
    for _ in 0..2000 {
        let n = rng.range(1, 6) as usize;
        let mut dsts: Vec<usize> = (0..6).collect();
        for i in (1..dsts.len()).rev() {
            dsts.swap(i, rng.below(i as u64 + 1) as usize);
        }
        let copies: Vec<(Var, Opnd)> = dsts[..n]
            .iter()
            .map(|&d| {
                (
                    d,
                    if rng.chance(20) {
                        Opnd::Const(rng.range(-9, 9))
                    } else {
                        Opnd::Var(rng.below(6) as usize)
                    },
                )
            })
            .collect();
        let env: Vec<i64> = (0..8).map(|i| 100 + i * 7 + rng.range(0, 3)).collect();
        let mut parallel = env.clone();
        for &(d, s) in &copies {
            parallel[d] = val(&env, s);
        }
        let mut next = 6;
        let (seq, cyc) = sequentialise(copies, &mut || {
            next += 1;
            next - 1
        });
        cycles += cyc;
        let mut serial = env.clone();
        for (d, s) in seq {
            serial[d] = val(&serial, s);
        }
        assert_eq!(
            &serial[..6],
            &parallel[..6],
            "sequentialised copies differ from the parallel semantics"
        );
    }
    println!(
        "parallel-copy sequentialiser: 2000 random copy sets match parallel semantics ({cycles} cycles broken with a temp)"
    );
}

/// The swap problem: after copy propagation the loop header holds `a = phi(a0, b)`, `b = phi(b0, a)`.
fn swap_program() -> Prog {
    let (a, b, n, i, t) = (0, 1, 2, 3, 4);
    Prog {
        nparams: 3,
        nvars: 5,
        body: vec![
            Stmt::Assign(i, c(0)),
            Stmt::While(
                bin(BinOp::Lt, v(i), v(n)),
                vec![
                    Stmt::Assign(t, v(a)),
                    Stmt::Assign(a, v(b)),
                    Stmt::Assign(b, v(t)),
                    Stmt::Assign(i, bin(BinOp::Add, v(i), c(1))),
                ],
            ),
        ],
        ret: bin(BinOp::Add, v(a), bin(BinOp::Mul, v(b), c(10))),
    }
}

/// The lost-copy problem needs a do-while loop (the latch block both loops back and exits), built by hand:
/// b0: i=0; b1: t=i; i=i+1; c=i<n; br c b1 b2; b2: ret t.  Returns max(n-1, 0).
fn lost_copy_func() -> Func {
    let (n, i, t, cc) = (0, 1, 2, 3);
    let o = Opnd::Var;
    Func {
        blocks: vec![
            Block {
                phis: vec![],
                insts: vec![Inst::Def(i, Rhs::Copy(Opnd::Const(0)))],
                term: Term::Jump(1),
            },
            Block {
                phis: vec![],
                insts: vec![
                    Inst::Def(t, Rhs::Copy(o(i))),
                    Inst::Def(i, Rhs::Bin(BinOp::Add, o(i), Opnd::Const(1))),
                    Inst::Def(cc, Rhs::Bin(BinOp::Lt, o(i), o(n))),
                ],
                term: Term::Branch(o(cc), 1, 2),
            },
            Block {
                phis: vec![],
                insts: vec![],
                term: Term::Ret(o(t)),
            },
        ],
        params: vec![n],
        names: ["n", "i", "t", "c"].iter().map(|s| s.to_string()).collect(),
        orig: (0..4).collect(),
        nslots: 0,
    }
}

fn test_out_of_ssa_pitfalls() {
    // Swap problem.
    let p = swap_program();
    let f0 = lower_prog(&p);
    let (mut opt, dom) = (f0.clone(), compute_dom(&f0));
    to_ssa(&mut opt, &dom);
    optimise(&mut opt);
    let inputs: Vec<Vec<i64>> = (0..6).map(|n| vec![3, 8, n]).collect();
    let (mut good, mut naive) = (opt.clone(), opt.clone());
    let gs = lower_phis(&mut good, CORRECT);
    lower_phis(
        &mut naive,
        LowerOpts {
            split_critical: true,
            parallel: false,
        },
    );
    let wrong = inputs
        .iter()
        .filter(|a| interp(&naive, a) != eval_prog(&p, a))
        .count();
    inputs
        .iter()
        .for_each(|a| assert_eq!(interp(&good, a), eval_prog(&p, a)));
    assert!(gs.cycles_broken >= 1, "the swap cycle needs a temporary");
    assert!(
        wrong > 0,
        "naive in-order phi copies should fail the swap test"
    );
    println!(
        "swap problem: parallel copy OK ({} cycle broken); naive in-order copies wrong on {wrong}/6 inputs",
        gs.cycles_broken
    );
    compile_and_check(&f0, &inputs, &|a| eval_prog(&p, a));

    // Lost copy.
    let f0 = lost_copy_func();
    let expected = |a: &[i64]| (a[0] - 1).max(0);
    let inputs: Vec<Vec<i64>> = (-1..7).map(|n| vec![n]).collect();
    let mut opt = f0.clone();
    to_ssa(&mut opt, &compute_dom(&f0));
    optimise(&mut opt);
    let (mut good, mut naive) = (opt.clone(), opt.clone());
    let gs = lower_phis(&mut good, CORRECT);
    lower_phis(
        &mut naive,
        LowerOpts {
            split_critical: false,
            parallel: true,
        },
    );
    let wrong = inputs
        .iter()
        .filter(|a| interp(&naive, a) != expected(a))
        .count();
    inputs
        .iter()
        .for_each(|a| assert_eq!(interp(&good, a), expected(a)));
    assert!(gs.edges_split >= 1 && wrong > 0);
    println!(
        "lost-copy problem: edge splitting OK ({} edge split); copies at block end wrong on {wrong}/{} inputs",
        gs.edges_split,
        inputs.len()
    );
    compile_and_check(&f0, &inputs, &expected);
}

fn main() {
    demo();
    test_sequentialiser();
    test_out_of_ssa_pitfalls();

    let mut rng = Rng(0xC0FFEE_1234_5678);
    let mut total = Report::default();
    let programs = 300;
    let (mut blocks, mut max_rounds) = (0, 0);
    for _ in 0..programs {
        let p = gen_prog(&mut rng);
        let f0 = lower_prog(&p);
        blocks += f0.blocks.len();
        let inputs: Vec<Vec<i64>> = (0..5)
            .map(|_| vec![rng.range(-5, 20), rng.range(-5, 20)])
            .collect();
        let r = compile_and_check(&f0, &inputs, &|a| eval_prog(&p, a));
        total.phis += r.phis;
        total.ssa_defs += r.ssa_defs;
        total.insts_before += r.insts_before;
        total.insts_after += r.insts_after;
        total.lower.edges_split += r.lower.edges_split;
        total.lower.cycles_broken += r.lower.cycles_broken;
        total.lower.copies += r.lower.copies;
        total.opt.const_props += r.opt.const_props;
        total.opt.copy_props += r.opt.copy_props;
        total.opt.folds += r.opt.folds;
        total.opt.dead_removed += r.opt.dead_removed;
        total.opt.branches_folded += r.opt.branches_folded;
        total.opt.blocks_removed += r.opt.blocks_removed;
        for k in 0..4 {
            for s in 0..2 {
                total.spills[k][s] += r.spills[k][s];
                total.code_size[k][s] += r.code_size[k][s];
                total.mem_ops[k][s] += r.mem_ops[k][s];
                total.elided[k][s] += r.elided[k][s];
            }
        }
        max_rounds = max_rounds.max(compute_dom(&f0).rounds);
    }
    println!(
        "\n{programs} random structured programs x 5 inputs: AST == IR == SSA == optimised == phi-lowered == machine code (K=3..6, both allocators)"
    );
    println!(
        "  avg {} blocks/program, CHK converged in <= {max_rounds} rounds, {} phis inserted, {} SSA defs",
        blocks / programs,
        total.phis,
        total.ssa_defs
    );
    println!(
        "  optimiser: {} -> {} insts; {:?}",
        total.insts_before, total.insts_after, total.opt
    );
    println!(
        "  out of SSA: {} critical edges split, {} copies, {} swap cycles broken",
        total.lower.edges_split, total.lower.copies, total.lower.cycles_broken
    );
    println!(
        "  K | spilled vars (colour / scan) | static insts (colour / scan) | ld+st (colour / scan) | moves coalesced (colour / scan)"
    );
    for k in 0..4 {
        println!(
            "  {} | {:>6} / {:<6}              | {:>6} / {:<6}              | {:>5} / {:<5}        | {:>5} / {:<5}",
            k + 3,
            total.spills[k][0],
            total.spills[k][1],
            total.code_size[k][0],
            total.code_size[k][1],
            total.mem_ops[k][0],
            total.mem_ops[k][1],
            total.elided[k][0],
            total.elided[k][1]
        );
    }
    for s in 0..2 {
        for k in 0..3 {
            assert!(
                total.spills[k][s] >= total.spills[k + 1][s],
                "spills must not grow with more registers (strategy {s}, K={})",
                k + 3
            );
        }
        assert!(
            total.spills[0][s] > total.spills[3][s],
            "K=3 must spill strictly more than K=6"
        );
    }
    assert!(
        total.opt.const_props > 0
            && total.opt.dead_removed > 0
            && total.opt.branches_folded > 0
            && total.lower.copies > 0
    );
    println!("all assertions passed");
}
