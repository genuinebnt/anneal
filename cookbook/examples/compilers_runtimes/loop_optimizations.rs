//! RECIPE 823 (compilers): loop optimisations on a CFG IR: natural loops, LICM, strength reduction, unrolling, bounds-check hoisting, plus dependence-driven interchange / fusion / fission.
//! LEVEL: intermediate
//! Use in: optimising compilers (LLVM LICM / IndVars / LoopUnroll / LoopInterchange, GCC, V8 TurboFan, HotSpot C2), query-expression JITs, numeric kernels.   Run: cargo run --release --example loop_optimizations
#![allow(dead_code)]
//!
//! WHY LOOPS: programs spend nearly all their time in loops, so a saving per iteration is multiplied by the trip count.
//! IR: registers hold i64, three arrays of 16 cells, blocks end in `Jmp/Br/Ret`. `Load`/`Store` are UNCHECKED (out of range is
//! an `Ub` error in the interpreter, never legal), the front end emits an explicit `Check(idx, arr)` that traps. This makes bounds
//! checks a visible, optimisable instruction. `CheckRange(lo, hi, arr)` is the hoisted form: "if lo < hi, every index in
//! [lo, hi) is in range". Costs: Mul = 3, Load/Store = 2, everything else (and every terminator) = 1.
//! PART 1, loop structure: Cooper-Harvey-Kennedy dominators (checked against the textbook set intersection on random graphs
//! including irreducible ones), BACK EDGE = edge u->h where h dominates u, NATURAL LOOP = h plus everything that reaches u
//! without passing h. Loops sorted inner first.
//! PART 2, transformations on the CFG (each one is verified on 1500 random programs x 3 inputs by comparing the interpreter
//! before and after; a trap must stay a trap, everything else must be identical):
//!  * LICM. An instruction is hoisted to the preheader when (1) its operands are loop invariant, (2) its destination has ONE def in
//!    the loop and every in-loop use is dominated by that def, and (3) it is SAFE: either its block dominates all loop exits
//!    (so it ran anyway) or it is a pure op whose destination is dead after the loop (speculation is harmless). `Load`/`Check`
//!    can trap so they only get (3a); a Load also needs "no store to that array in the loop". That is why a load in the body of
//!    a top-tested `for` stays put but moves out of a `do-while` (demo). Needs liveness (backward dataflow).
//!  * STRENGTH REDUCTION. A basic induction variable i (`i = i + c`, one def) makes `j = i * k` a recurrence: keep `s = i*k`
//!    initialised in the preheader and bump it by `c*k` right after the increment; replace the Mul by `Mov j, s`. DCE removes the leftovers.
//!  * UNROLL by 4 WITH REMAINDER, for the shape `H: c = i < n; br c, B, X; B: ...; i += step; jmp H`. The new fast loop tests
//!    `i + 3*step < n` and runs four bodies back to back; the ORIGINAL loop stays behind as the remainder loop (no
//!    divisibility assumption). Saves 3 of every 4 loop tests, branches and jumps.
//!  * BOUNDS-CHECK HOISTING. `Check(i + c)` in a counted loop with step 1 that runs every iteration, before the increment, is replaced by one
//!    `CheckRange` in the preheader. Imprecise exceptions: the trap fires BEFORE the loop's earlier side effects, so on a trapping
//!    run memory differs; this is the accepted trade (Java JITs do this too) and why equality ignores memory after a trap.
//! PART 3, affine loop nests (separate tiny IR: 2-deep nests, array subscripts `var + const`): dependence distances by solving
//! the subscript equations, the GCD test on 1-D (checked against brute force: "independent" must never be wrong), then legality:
//!  * INTERCHANGE legal iff every dependence vector stays lexicographically non-negative after swapping its components.
//!  * FUSION is prevented by a dependence from a later iteration of loop A to an earlier one of loop B; FISSION is the same test.
//!  Each "legal" verdict is run against a concrete evaluator; "illegal" verdicts are shown to be non-vacuous by finding a witness
//!  where the transformed nest really computes something else. A tiny LRU cache simulator shows why interchange matters
//!  (stride-N versus stride-1 walks, misses printed).
//! COMMON USES: LLVM passes LICM, IndVarSimplify, LoopStrengthReduce, LoopUnroll, LoopInterchange, LoopFuse/LoopDistribute and
//! the bounds-check elimination of JVM and V8 JITs; polyhedral tools (Polly, Pluto) generalise Part 3.
//! Read: Cooper, Harvey, Kennedy "A Simple, Fast Dominance Algorithm"; Allen and Kennedy "Optimizing Compilers for Modern
//! Architectures" (dependence analysis, interchange, fusion); Muchnick "Advanced Compiler Design and Implementation" (LICM, strength reduction);
//! https://en.wikipedia.org/wiki/Loop_optimization ; https://en.wikipedia.org/wiki/Loop-invariant_code_motion ;
//! https://en.wikipedia.org/wiki/Loop_interchange ; https://en.wikipedia.org/wiki/Strength_reduction
//! TRY NEXT:
//!   - [S] Change the unroll factor `UNROLL` from 4 to 8 and predict the new weighted-cost saving before running; why does a bigger factor stop paying?
//!   - [S] Remove the `dominates all exits` test for `Load` in `licm_loop` and run: which assertion fires first and on what program shape?
//!   - [M] Add `Op::Shl` and teach `strength_reduce` that `x*8` becomes a shift; add a cost of 1 and predict the weighted saving.
//!   - [M] Extend `solve` to subscripts with coefficient 2 (A[2i]) using the GCD test as the filter before the distance solver.
//!   - [A] PROJECT: combine with `ssa_register_allocation`: run LICM first, then allocate registers with K=4 and measure how hoisting raises register pressure (spills) inside the loop.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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
type Reg = usize;
const ALEN: i64 = 16;
const NARR: usize = 3;
const FUEL: u64 = 3_000_000;

#[derive(Clone, Copy, Debug, PartialEq)]
enum BOp {
    Add,
    Sub,
    Mul,
    And,
    Lt,
    Eq,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ins {
    Const(Reg, i64),
    Bin(Reg, BOp, Reg, Reg),
    Mov(Reg, Reg),
    Load(Reg, usize, Reg),
    Store(usize, Reg, Reg),
    Check(Reg, usize),
    CheckRange(Reg, Reg, usize),
}
#[derive(Clone, Debug, PartialEq)]
enum Term {
    Jmp(usize),
    Br(Reg, usize, usize),
    Ret(Reg),
}
#[derive(Clone, Debug)]
struct Block {
    ins: Vec<Ins>,
    term: Term,
}
#[derive(Clone, Debug)]
struct Func {
    blocks: Vec<Block>,
    nregs: usize,
}
impl Ins {
    fn def(&self) -> Option<Reg> {
        match *self {
            Ins::Const(d, _) | Ins::Bin(d, ..) | Ins::Mov(d, _) | Ins::Load(d, ..) => Some(d),
            _ => None,
        }
    }
    fn uses(&self) -> Vec<Reg> {
        match *self {
            Ins::Const(..) => vec![],
            Ins::Bin(_, _, a, b) => vec![a, b],
            Ins::Mov(_, a) => vec![a],
            Ins::Load(_, _, i) => vec![i],
            Ins::Store(_, i, v) => vec![i, v],
            Ins::Check(i, _) => vec![i],
            Ins::CheckRange(l, h, _) => vec![l, h],
        }
    }
    fn cost(&self) -> u64 {
        match self {
            Ins::Bin(_, BOp::Mul, ..) => 3,
            Ins::Load(..) | Ins::Store(..) => 2,
            _ => 1,
        }
    }
    fn is_pure(&self) -> bool {
        matches!(self, Ins::Const(..) | Ins::Bin(..) | Ins::Mov(..))
    }
}
impl Term {
    fn succs(&self) -> Vec<usize> {
        match *self {
            Term::Jmp(t) => vec![t],
            Term::Br(_, t, e) => vec![t, e],
            Term::Ret(_) => vec![],
        }
    }
    fn uses(&self) -> Vec<Reg> {
        match *self {
            Term::Br(c, ..) => vec![c],
            Term::Ret(r) => vec![r],
            Term::Jmp(_) => vec![],
        }
    }
    fn retarget(&mut self, from: usize, to: usize) {
        match self {
            Term::Jmp(t) if *t == from => *t = to,
            Term::Br(_, t, e) => {
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
    fn fresh(&mut self) -> Reg {
        self.nregs += 1;
        self.nregs - 1
    }
    fn succ_lists(&self) -> Vec<Vec<usize>> {
        self.blocks.iter().map(|b| b.term.succs()).collect()
    }
    fn dump(&self) -> String {
        let mut s = String::new();
        for (i, b) in self.blocks.iter().enumerate() {
            s += &format!("  b{i}:\n");
            for ins in &b.ins {
                s += &format!(
                    "      {}\n",
                    match ins {
                        Ins::Const(d, c) => format!("r{d} = {c}"),
                        Ins::Bin(d, op, a, b) => format!("r{d} = r{a} {op:?} r{b}"),
                        Ins::Mov(d, a) => format!("r{d} = r{a}"),
                        Ins::Load(d, a, i) => format!("r{d} = A{a}[r{i}]"),
                        Ins::Store(a, i, v) => format!("A{a}[r{i}] = r{v}"),
                        Ins::Check(i, a) => format!("check r{i} < len(A{a})"),
                        Ins::CheckRange(l, h, a) => format!("checkrange [r{l}, r{h}) in A{a}"),
                    }
                );
            }
            s += &format!("      {:?}\n", b.term);
        }
        s
    }
}

// ---------------------------------------------------------------- interpreter (the oracle)
#[derive(Debug, PartialEq, Clone)]
enum Fail {
    Trap,
    Ub,
    Fuel,
}
#[derive(Debug, PartialEq, Clone)]
struct Done {
    ret: i64,
    mem: Vec<Vec<i64>>,
}
type Outcome = Result<Done, Fail>;
fn bin(op: BOp, a: i64, b: i64) -> i64 {
    match op {
        BOp::Add => a.wrapping_add(b),
        BOp::Sub => a.wrapping_sub(b),
        BOp::Mul => a.wrapping_mul(b),
        BOp::And => a & b,
        BOp::Lt => (a < b) as i64,
        BOp::Eq => (a == b) as i64,
    }
}
fn interp(f: &Func, init: &[i64], mem0: &[Vec<i64>]) -> (Outcome, u64, u64) {
    let mut regs = vec![0i64; f.nregs];
    regs[..init.len()].copy_from_slice(init);
    let mut mem = mem0.to_vec();
    let (mut n, mut cost, mut b) = (0u64, 0u64, 0usize);
    loop {
        for ins in &f.blocks[b].ins {
            n += 1;
            cost += ins.cost();
            if n > FUEL {
                return (Err(Fail::Fuel), n, cost);
            }
            let inb = |i: i64| (0..ALEN).contains(&i);
            match *ins {
                Ins::Const(d, c) => regs[d] = c,
                Ins::Bin(d, op, a, c) => regs[d] = bin(op, regs[a], regs[c]),
                Ins::Mov(d, a) => regs[d] = regs[a],
                Ins::Load(d, a, i) => {
                    if !inb(regs[i]) {
                        return (Err(Fail::Ub), n, cost);
                    }
                    regs[d] = mem[a][regs[i] as usize];
                }
                Ins::Store(a, i, v) => {
                    if !inb(regs[i]) {
                        return (Err(Fail::Ub), n, cost);
                    }
                    mem[a][regs[i] as usize] = regs[v];
                }
                Ins::Check(i, _) => {
                    if !inb(regs[i]) {
                        return (Err(Fail::Trap), n, cost);
                    }
                }
                Ins::CheckRange(l, h, _) => {
                    if regs[l] < regs[h] && !(inb(regs[l]) && inb(regs[h] - 1)) {
                        return (Err(Fail::Trap), n, cost);
                    }
                }
            }
        }
        n += 1;
        cost += 1;
        match f.blocks[b].term {
            Term::Jmp(t) => b = t,
            Term::Br(c, t, e) => b = if regs[c] != 0 { t } else { e },
            Term::Ret(r) => return (Ok(Done { ret: regs[r], mem }), n, cost),
        }
    }
}
/// Traps compare equal regardless of memory (imprecise exceptions); everything else must match exactly.
fn same(a: &Outcome, b: &Outcome) -> bool {
    match (a, b) {
        (Err(Fail::Trap), Err(Fail::Trap)) => true,
        _ => a == b,
    }
}

// ---------------------------------------------------------------- dominators and loops
struct Cfg {
    preds: Vec<Vec<usize>>,
    rpo: Vec<usize>,
    num: Vec<usize>, // rpo number, MAX if unreachable
    idom: Vec<usize>,
}
fn dom_info(succs: &[Vec<usize>]) -> Cfg {
    let n = succs.len();
    let mut preds = vec![vec![]; n];
    for (b, ss) in succs.iter().enumerate() {
        for &s in ss {
            preds[s].push(b);
        }
    }
    // iterative DFS postorder
    let (mut seen, mut post) = (vec![false; n], vec![]);
    let mut stack = vec![(0usize, 0usize)];
    seen[0] = true;
    while let Some(&(v, i)) = stack.last() {
        if i < succs[v].len() {
            stack.last_mut().unwrap().1 += 1;
            let w = succs[v][i];
            if !seen[w] {
                seen[w] = true;
                stack.push((w, 0));
            }
        } else {
            post.push(v);
            stack.pop();
        }
    }
    let rpo: Vec<usize> = post.into_iter().rev().collect();
    let mut num = vec![usize::MAX; n];
    for (i, &b) in rpo.iter().enumerate() {
        num[b] = i;
    }
    let mut idom = vec![usize::MAX; n];
    idom[0] = 0;
    let mut changed = true;
    while changed {
        changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new = usize::MAX;
            for &p in &preds[b] {
                if idom[p] == usize::MAX {
                    continue;
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
    Cfg {
        preds,
        rpo,
        num,
        idom,
    }
}
/// The two-finger walk up the dominator tree: the finger with the larger rpo number is deeper, so move it up.
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
impl Cfg {
    fn dominates(&self, a: usize, mut b: usize) -> bool {
        if self.idom[b] == usize::MAX {
            return false;
        }
        loop {
            if a == b {
                return true;
            }
            if b == 0 {
                return false;
            }
            b = self.idom[b];
        }
    }
    fn reachable(&self, b: usize) -> bool {
        self.num[b] != usize::MAX
    }
}
/// Textbook definition: Dom(entry) = {entry}; Dom(n) = {n} + intersection of Dom(p) over reachable preds.
fn naive_dom(succs: &[Vec<usize>], c: &Cfg) -> Vec<BTreeSet<usize>> {
    let n = succs.len();
    let all: BTreeSet<usize> = (0..n).filter(|&b| c.reachable(b)).collect();
    let mut dom: Vec<BTreeSet<usize>> = (0..n)
        .map(|b| if b == 0 { [0].into() } else { all.clone() })
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        for b in 1..n {
            if !c.reachable(b) {
                continue;
            }
            let mut it = c.preds[b].iter().filter(|&&p| c.reachable(p));
            let mut s = dom[*it.next().unwrap()].clone();
            for &p in it {
                s = s.intersection(&dom[p]).copied().collect();
            }
            s.insert(b);
            if s != dom[b] {
                dom[b] = s;
                changed = true;
            }
        }
    }
    dom
}
struct Loop {
    header: usize,
    body: BTreeSet<usize>,
    latches: Vec<usize>,
}
fn find_loops(f: &Func, c: &Cfg) -> Vec<Loop> {
    let mut by_header: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (t, b) in f.blocks.iter().enumerate() {
        if !c.reachable(t) {
            continue;
        }
        for s in b.term.succs() {
            if c.dominates(s, t) {
                by_header.entry(s).or_default().push(t);
            }
        }
    }
    let mut loops = vec![];
    for (h, latches) in by_header {
        let mut body: BTreeSet<usize> = [h].into();
        let mut work = latches.clone();
        while let Some(x) = work.pop() {
            if body.insert(x) {
                work.extend(c.preds[x].iter().copied());
            }
        }
        loops.push(Loop {
            header: h,
            body,
            latches,
        });
    }
    loops.sort_by_key(|l| l.body.len()); // inner loops first
    loops
}

// ---------------------------------------------------------------- liveness and helpers
fn liveness(f: &Func) -> (Vec<BTreeSet<Reg>>, Vec<BTreeSet<Reg>>) {
    let n = f.blocks.len();
    let (mut lin, mut lout) = (vec![BTreeSet::new(); n], vec![BTreeSet::new(); n]);
    let mut changed = true;
    while changed {
        changed = false;
        for b in (0..n).rev() {
            let mut out = BTreeSet::new();
            for s in f.blocks[b].term.succs() {
                out.extend(lin[s].iter().copied());
            }
            let mut live = out.clone();
            live.extend(f.blocks[b].term.uses());
            for ins in f.blocks[b].ins.iter().rev() {
                if let Some(d) = ins.def() {
                    live.remove(&d);
                }
                live.extend(ins.uses());
            }
            if live != lin[b] || out != lout[b] {
                lin[b] = live;
                lout[b] = out;
                changed = true;
            }
        }
    }
    (lin, lout)
}
fn loop_defs(f: &Func, body: &BTreeSet<usize>) -> HashMap<Reg, usize> {
    let mut m = HashMap::new();
    for &b in body {
        for ins in &f.blocks[b].ins {
            if let Some(d) = ins.def() {
                *m.entry(d).or_insert(0) += 1;
            }
        }
    }
    m
}
/// Registers with exactly one definition in the whole function and that definition is a constant.
fn const_regs(f: &Func) -> HashMap<Reg, i64> {
    let (mut cnt, mut val) = (HashMap::<Reg, usize>::new(), HashMap::new());
    for b in &f.blocks {
        for ins in &b.ins {
            if let Some(d) = ins.def() {
                *cnt.entry(d).or_insert(0) += 1;
                if let Ins::Const(_, c) = ins {
                    val.insert(d, *c);
                }
            }
        }
    }
    val.retain(|r, _| cnt[r] == 1);
    val
}
/// Returns a block that runs exactly once per loop entry and falls into the header (creating one if needed).
fn ensure_preheader(f: &mut Func, c: &Cfg, lp: &Loop) -> Option<usize> {
    let h = lp.header;
    let outside: Vec<usize> = c.preds[h]
        .iter()
        .copied()
        .filter(|p| !lp.body.contains(p) && c.reachable(*p))
        .collect();
    if h == 0 || outside.is_empty() {
        return None;
    }
    if outside.len() == 1 && f.blocks[outside[0]].term == Term::Jmp(h) {
        return Some(outside[0]);
    }
    let nb = f.blocks.len();
    f.blocks.push(Block {
        ins: vec![],
        term: Term::Jmp(h),
    });
    for p in outside {
        f.blocks[p].term.retarget(h, nb);
    }
    Some(nb)
}
/// Loop driver: run `step` on each loop (inner first) once, recomputing the CFG after every change.
fn for_each_loop(f: &mut Func, mut step: impl FnMut(&mut Func, &Cfg, &Loop) -> usize) -> usize {
    let (mut total, mut done) = (0, HashSet::new());
    loop {
        let cfg = dom_info(&f.succ_lists());
        let loops = find_loops(f, &cfg);
        let Some(lp) = loops.iter().find(|l| !done.contains(&l.header)) else {
            return total;
        };
        done.insert(lp.header);
        total += step(f, &cfg, lp);
    }
}

// ---------------------------------------------------------------- LICM
fn uses_dominated_by(f: &Func, c: &Cfg, lp: &Loop, d: Reg, at: (usize, usize)) -> bool {
    for &b in &lp.body {
        for (j, ins) in f.blocks[b].ins.iter().enumerate() {
            if (b, j) != at
                && ins.uses().contains(&d)
                && !((b == at.0 && j > at.1) || (b != at.0 && c.dominates(at.0, b)))
            {
                return false;
            }
        }
        if f.blocks[b].term.uses().contains(&d) && !(b == at.0 || c.dominates(at.0, b)) {
            return false;
        }
    }
    true
}
fn licm_loop(f: &mut Func, c: &Cfg, lp: &Loop) -> usize {
    let (livein, _) = liveness(f);
    let defs = loop_defs(f, &lp.body);
    let mut stored = BTreeSet::new();
    for &b in &lp.body {
        for ins in &f.blocks[b].ins {
            if let Ins::Store(a, ..) = ins {
                stored.insert(*a);
            }
        }
    }
    let (mut exit_src, mut exit_live) = (vec![], BTreeSet::new());
    for &b in &lp.body {
        for s in f.blocks[b].term.succs() {
            if !lp.body.contains(&s) {
                exit_src.push(b);
                exit_live.extend(livein[s].iter().copied());
            }
        }
    }
    let order: Vec<usize> = c
        .rpo
        .iter()
        .copied()
        .filter(|b| lp.body.contains(b))
        .collect();
    let (mut hoisted, mut hoisted_set, mut hoisted_defs) = (vec![], HashSet::new(), HashSet::new());
    let mut changed = true;
    while changed {
        changed = false;
        for &b in &order {
            for (k, ins) in f.blocks[b].ins.iter().enumerate() {
                if hoisted_set.contains(&(b, k)) {
                    continue;
                }
                let nonspec = matches!(ins, Ins::Load(..) | Ins::Check(..));
                if !ins.is_pure() && !nonspec {
                    continue;
                }
                let invariant = ins.uses().iter().all(|u| match defs.get(u) {
                    None => true,
                    Some(1) => hoisted_defs.contains(u),
                    _ => false,
                });
                if !invariant {
                    continue;
                }
                if let Ins::Load(_, a, _) = ins {
                    if stored.contains(a) {
                        continue;
                    }
                }
                if let Some(d) = ins.def() {
                    if defs[&d] != 1 || !uses_dominated_by(f, c, lp, d, (b, k)) {
                        continue;
                    }
                }
                let dom_exits = exit_src.iter().all(|&e| c.dominates(b, e));
                if nonspec && !dom_exits {
                    continue; // may trap: never speculate
                }
                if !dom_exits && ins.def().is_some_and(|d| exit_live.contains(&d)) {
                    continue; // the zero-trip path would observe the clobbered value
                }
                hoisted.push((b, k));
                hoisted_set.insert((b, k));
                if let Some(d) = ins.def() {
                    hoisted_defs.insert(d);
                }
                changed = true;
            }
        }
    }
    if hoisted.is_empty() {
        return 0;
    }
    let Some(ph) = ensure_preheader(f, c, lp) else {
        return 0;
    };
    let moved: Vec<Ins> = hoisted
        .iter()
        .map(|&(b, k)| f.blocks[b].ins[k].clone())
        .collect();
    let mut rm = hoisted.clone();
    rm.sort_by(|a, b| b.cmp(a));
    for (b, k) in rm {
        f.blocks[b].ins.remove(k);
    }
    let n = moved.len();
    f.blocks[ph].ins.extend(moved);
    n
}
fn licm(f: &mut Func) -> usize {
    for_each_loop(f, |f, c, lp| licm_loop(f, c, lp))
}

// ---------------------------------------------------------------- strength reduction
fn strength_reduce(f: &mut Func) -> usize {
    let consts = const_regs(f);
    for_each_loop(f, |f, c, lp| {
        let defs = loop_defs(f, &lp.body);
        // basic induction variables: reg -> (step, block, position of the increment)
        let mut ivs: HashMap<Reg, (i64, usize, usize)> = HashMap::new();
        for &b in &lp.body {
            for (k, ins) in f.blocks[b].ins.iter().enumerate() {
                if let Ins::Bin(d, BOp::Add, x, y) = *ins {
                    let other = if x == d {
                        y
                    } else if y == d {
                        x
                    } else {
                        continue;
                    };
                    if defs[&d] == 1 && consts.get(&other).is_some_and(|&s| s > 0) {
                        ivs.insert(d, (consts[&other], b, k));
                    }
                }
            }
        }
        let mut found: Vec<(usize, usize, Reg, Reg, i64)> = vec![]; // block, pos, j, iv, k
        for &b in &lp.body {
            for (k, ins) in f.blocks[b].ins.iter().enumerate() {
                if let Ins::Bin(j, BOp::Mul, x, y) = *ins {
                    if ivs.contains_key(&x) && consts.contains_key(&y) {
                        found.push((b, k, j, x, consts[&y]));
                    } else if ivs.contains_key(&y) && consts.contains_key(&x) {
                        found.push((b, k, j, y, consts[&x]));
                    }
                }
            }
        }
        if found.is_empty() {
            return 0;
        }
        let Some(ph) = ensure_preheader(f, c, lp) else {
            return 0;
        };
        let mut shadow: HashMap<(Reg, i64), Reg> = HashMap::new();
        let mut inserts: Vec<(usize, usize, Ins)> = vec![];
        for &(b, k, j, iv, kv) in &found {
            let s = *shadow.entry((iv, kv)).or_insert_with(|| {
                let (kk, s, dc) = (f.fresh(), f.fresh(), f.fresh());
                let (step, ib, ik) = ivs[&iv];
                f.blocks[ph].ins.extend([
                    Ins::Const(kk, kv),
                    Ins::Bin(s, BOp::Mul, iv, kk),
                    Ins::Const(dc, step * kv),
                ]);
                inserts.push((ib, ik, Ins::Bin(s, BOp::Add, s, dc)));
                s
            });
            f.blocks[b].ins[k] = Ins::Mov(j, s);
        }
        inserts.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
        for (b, k, ins) in inserts {
            f.blocks[b].ins.insert(k + 1, ins);
        }
        found.len()
    })
}

// ---------------------------------------------------------------- dead code elimination
fn dce(f: &mut Func) -> usize {
    let mut removed = 0;
    loop {
        let (_, lout) = liveness(f);
        let mut any = false;
        for b in 0..f.blocks.len() {
            let mut live = lout[b].clone();
            live.extend(f.blocks[b].term.uses());
            let mut keep = vec![true; f.blocks[b].ins.len()];
            for (k, ins) in f.blocks[b].ins.iter().enumerate().rev() {
                if let Some(d) = ins.def() {
                    if ins.is_pure() && !live.contains(&d) {
                        keep[k] = false;
                        continue;
                    }
                    live.remove(&d);
                }
                live.extend(ins.uses());
            }
            let mut it = keep.iter();
            let before = f.blocks[b].ins.len();
            f.blocks[b].ins.retain(|_| *it.next().unwrap());
            let gone = before - f.blocks[b].ins.len();
            removed += gone;
            any |= gone > 0;
        }
        if !any {
            return removed;
        }
    }
}

// ---------------------------------------------------------------- unrolling with remainder
const UNROLL: usize = 4;
fn unroll(f: &mut Func) -> usize {
    let consts = const_regs(f);
    for_each_loop(f, |f, c, lp| {
        if lp.body.len() != 2 || lp.header == 0 {
            return 0;
        }
        let h = lp.header;
        let b = *lp.body.iter().find(|&&x| x != h).unwrap();
        let (livein, _) = liveness(f);
        let [Ins::Bin(cmp, BOp::Lt, i, n)] = f.blocks[h].ins[..] else {
            return 0;
        };
        let Term::Br(cc, t, x) = f.blocks[h].term else {
            return 0;
        };
        // the fast loop never writes `cmp`, so neither the body nor the code after the loop may read the header's value
        if cc != cmp
            || t != b
            || lp.body.contains(&x)
            || f.blocks[b].term != Term::Jmp(h)
            || livein[x].contains(&cmp)
            || livein[b].contains(&cmp)
        {
            return 0;
        }
        let defs = loop_defs(f, &lp.body);
        if defs.get(&n).is_some() || defs.get(&i) != Some(&1) || defs.get(&cmp) != Some(&1) {
            return 0;
        }
        let step = f.blocks[b].ins.iter().find_map(|ins| match *ins {
            Ins::Bin(d, BOp::Add, p, q) if d == i && (p == i || q == i) => {
                consts.get(if p == i { &q } else { &p }).copied()
            }
            _ => None,
        });
        let Some(step) = step.filter(|s| *s > 0) else {
            return 0;
        };
        let (kr, t1, c2) = (f.fresh(), f.fresh(), f.fresh());
        let body = f.blocks[b].ins.clone();
        let h2 = f.blocks.len();
        let b2 = h2 + 1;
        f.blocks.push(Block {
            ins: vec![
                Ins::Const(kr, (UNROLL as i64 - 1) * step),
                Ins::Bin(t1, BOp::Add, i, kr),
                Ins::Bin(c2, BOp::Lt, t1, n),
            ],
            term: Term::Br(c2, b2, h),
        });
        f.blocks.push(Block {
            ins: body.repeat(UNROLL),
            term: Term::Jmp(h2),
        });
        for &p in c.preds[h].iter().filter(|p| !lp.body.contains(p)) {
            f.blocks[p].term.retarget(h, h2);
        }
        1
    })
}

// ---------------------------------------------------------------- bounds-check hoisting
fn hoist_checks(f: &mut Func) -> usize {
    let consts = const_regs(f);
    for_each_loop(f, |f, c, lp| {
        let h = lp.header;
        if lp.latches.len() != 1 || h == 0 {
            return 0;
        }
        let latch = lp.latches[0];
        let Some(&Ins::Bin(cmp, BOp::Lt, i, n)) = f.blocks[h].ins.last() else {
            return 0;
        };
        match f.blocks[h].term {
            Term::Br(cc, t, x) if cc == cmp && lp.body.contains(&t) && !lp.body.contains(&x) => {}
            _ => return 0,
        }
        let defs = loop_defs(f, &lp.body);
        if defs.get(&n).is_some() || defs.get(&i) != Some(&1) {
            return 0;
        }
        let inc = f.blocks[latch].ins.iter().position(|ins| match *ins {
            Ins::Bin(d, BOp::Add, p, q) if d == i && (p == i || q == i) => {
                consts.get(if p == i { &q } else { &p }) == Some(&1)
            }
            _ => false,
        });
        let Some(ip) = inc else { return 0 };
        let mut hits: Vec<(usize, usize, usize, i64)> = vec![]; // block, pos, array, offset
        for &b in &lp.body {
            if b == h || !c.dominates(b, latch) {
                continue;
            }
            for (k, ins) in f.blocks[b].ins.iter().enumerate() {
                let Ins::Check(idx, arr) = *ins else { continue };
                if b == latch && k >= ip {
                    continue; // after the increment: i has already moved past n-1
                }
                let off = if idx == i {
                    Some(0)
                } else if defs.get(&idx) == Some(&1) {
                    f.blocks[b].ins[..k].iter().find_map(|d| match *d {
                        Ins::Bin(r, BOp::Add, p, q) if r == idx && (p == i || q == i) => {
                            consts.get(if p == i { &q } else { &p }).copied()
                        }
                        _ => None,
                    })
                } else {
                    None
                };
                if let Some(off) = off {
                    hits.push((b, k, arr, off));
                }
            }
        }
        if hits.is_empty() {
            return 0;
        }
        let Some(ph) = ensure_preheader(f, c, lp) else {
            return 0;
        };
        let wanted: BTreeSet<(usize, i64)> = hits.iter().map(|&(_, _, a, o)| (a, o)).collect();
        hits.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
        for &(b, k, ..) in &hits {
            f.blocks[b].ins.remove(k);
        }
        for (arr, off) in wanted {
            if off == 0 {
                f.blocks[ph].ins.push(Ins::CheckRange(i, n, arr));
            } else {
                let (kk, lo, hi) = (f.fresh(), f.fresh(), f.fresh());
                f.blocks[ph].ins.extend([
                    Ins::Const(kk, off),
                    Ins::Bin(lo, BOp::Add, i, kk),
                    Ins::Bin(hi, BOp::Add, n, kk),
                    Ins::CheckRange(lo, hi, arr),
                ]);
            }
        }
        hits.len()
    })
}

// ---------------------------------------------------------------- front end: random structured programs -> CFG
#[derive(Clone, Debug)]
enum E {
    C(i64),
    V(usize),
    Bin(BOp, Box<E>, Box<E>),
    Ld(usize, Box<E>, bool),
}
#[derive(Clone, Debug)]
enum S {
    Assign(usize, E),
    Store(usize, E, bool, E),
    For {
        iv: usize,
        lo: i64,
        hi: E,
        step: i64,
        body: Vec<S>,
    },
    DoWhile {
        iv: usize,
        n: E,
        body: Vec<S>,
    },
    If(E, Vec<S>, Vec<S>),
}
const NVARS: usize = 10; // v0 = n, v1 = m (inputs), v2..v7 data, v8/v9 loop counters

struct Lower {
    f: Func,
    cur: usize,
}
impl Lower {
    fn block(&mut self) -> usize {
        self.f.blocks.push(Block {
            ins: vec![],
            term: Term::Ret(0),
        });
        self.f.blocks.len() - 1
    }
    fn emit(&mut self, i: Ins) {
        self.f.blocks[self.cur].ins.push(i);
    }
    fn end(&mut self, t: Term) {
        self.f.blocks[self.cur].term = t;
    }
    fn konst(&mut self, c: i64) -> Reg {
        let r = self.f.fresh();
        self.emit(Ins::Const(r, c));
        r
    }
    fn idx(&mut self, e: &E, masked: bool, arr: usize) -> Reg {
        let mut r = self.expr(e);
        if masked {
            let m = self.konst(ALEN - 1);
            let d = self.f.fresh();
            self.emit(Ins::Bin(d, BOp::And, r, m));
            r = d;
        }
        self.emit(Ins::Check(r, arr));
        r
    }
    fn expr(&mut self, e: &E) -> Reg {
        match e {
            E::C(c) => self.konst(*c),
            E::V(v) => *v,
            E::Bin(op, a, b) => {
                let (x, y) = (self.expr(a), self.expr(b));
                let d = self.f.fresh();
                self.emit(Ins::Bin(d, *op, x, y));
                d
            }
            E::Ld(arr, ix, masked) => {
                let r = self.idx(ix, *masked, *arr);
                let d = self.f.fresh();
                self.emit(Ins::Load(d, *arr, r));
                d
            }
        }
    }
    fn stmts(&mut self, ss: &[S]) {
        for s in ss {
            self.stmt(s);
        }
    }
    fn stmt(&mut self, s: &S) {
        match s {
            S::Assign(v, e) => {
                let r = self.expr(e);
                self.emit(Ins::Mov(*v, r));
            }
            S::Store(arr, ix, masked, val) => {
                let r = self.idx(ix, *masked, *arr);
                let v = self.expr(val);
                self.emit(Ins::Store(*arr, r, v));
            }
            S::For {
                iv,
                lo,
                hi,
                step,
                body,
            } => {
                let hr = self.expr(hi);
                let th = self.f.fresh();
                self.emit(Ins::Mov(th, hr)); // the bound is evaluated once
                self.emit(Ins::Const(*iv, *lo));
                let (h, b, x) = (self.block(), self.block(), self.block());
                self.end(Term::Jmp(h));
                self.cur = h;
                let c = self.f.fresh();
                self.emit(Ins::Bin(c, BOp::Lt, *iv, th));
                self.end(Term::Br(c, b, x));
                self.cur = b;
                self.stmts(body);
                let st = self.konst(*step);
                self.emit(Ins::Bin(*iv, BOp::Add, *iv, st));
                self.end(Term::Jmp(h));
                self.cur = x;
            }
            S::DoWhile { iv, n, body } => {
                let nr = self.expr(n);
                let th = self.f.fresh();
                self.emit(Ins::Mov(th, nr));
                self.emit(Ins::Const(*iv, 0));
                let (b, x) = (self.block(), self.block());
                self.end(Term::Jmp(b));
                self.cur = b;
                self.stmts(body);
                let one = self.konst(1);
                self.emit(Ins::Bin(*iv, BOp::Add, *iv, one));
                let c = self.f.fresh();
                self.emit(Ins::Bin(c, BOp::Lt, *iv, th));
                self.end(Term::Br(c, b, x));
                self.cur = x;
            }
            S::If(c, t, e) => {
                let cr = self.expr(c);
                let (tb, eb, j) = (self.block(), self.block(), self.block());
                self.end(Term::Br(cr, tb, eb));
                self.cur = tb;
                self.stmts(t);
                self.end(Term::Jmp(j));
                self.cur = eb;
                self.stmts(e);
                self.end(Term::Jmp(j));
                self.cur = j;
            }
        }
    }
}
fn lower(body: &[S]) -> Func {
    let mut l = Lower {
        f: Func {
            blocks: vec![],
            nregs: NVARS,
        },
        cur: 0,
    };
    l.block();
    l.stmts(body);
    l.end(Term::Ret(2));
    l.f
}

fn gen_expr(r: &mut Rng, ivs: &[usize], depth: usize) -> E {
    if depth == 0 || r.chance(35) {
        return match r.below(10) {
            0..=3 if !ivs.is_empty() => E::V(ivs[r.below(ivs.len() as u64) as usize]),
            4..=7 => E::V(r.below(8) as usize),
            _ => E::C(r.range(0, 6)),
        };
    }
    match r.below(10) {
        0 if !ivs.is_empty() => E::Bin(
            BOp::Mul,
            Box::new(E::V(ivs[0])),
            Box::new(E::C(r.range(2, 8))),
        ),
        0..=6 => {
            let op =
                [BOp::Add, BOp::Sub, BOp::Mul, BOp::And, BOp::Lt, BOp::Eq][r.below(6) as usize];
            E::Bin(
                op,
                Box::new(gen_expr(r, ivs, depth - 1)),
                Box::new(gen_expr(r, ivs, depth - 1)),
            )
        }
        _ => {
            let (ix, m) = gen_idx(r, ivs);
            E::Ld(r.below(NARR as u64) as usize, Box::new(ix), m)
        }
    }
}
/// Mostly counter-based indices (`i + c`, may trap) so there is something for bounds hoisting; otherwise masked (never traps).
fn gen_idx(r: &mut Rng, ivs: &[usize]) -> (E, bool) {
    if !ivs.is_empty() && r.chance(60) {
        let iv = ivs[r.below(ivs.len() as u64) as usize];
        (
            E::Bin(BOp::Add, Box::new(E::V(iv)), Box::new(E::C(r.range(0, 3)))),
            false,
        )
    } else {
        (gen_expr(r, ivs, 1), true)
    }
}
fn gen_body(r: &mut Rng, ivs: &[usize], n: usize, depth: usize) -> Vec<S> {
    let mut out = vec![];
    for _ in 0..n {
        let roll = r.below(100);
        let s = match roll {
            0..=39 => S::Assign(2 + r.below(6) as usize, gen_expr(r, ivs, 2)),
            40..=59 => {
                let (ix, m) = gen_idx(r, ivs);
                S::Store(r.below(NARR as u64) as usize, ix, m, gen_expr(r, ivs, 2))
            }
            60..=77 if depth < 2 => {
                let iv = if depth == 0 { 8 } else { 9 };
                let mut inner = ivs.to_vec();
                inner.push(iv);
                let cnt = 1 + r.below(3) as usize;
                let hi = if r.chance(70) {
                    E::V(0)
                } else {
                    E::C(r.range(2, 9))
                };
                S::For {
                    iv,
                    lo: r.range(0, 1),
                    hi,
                    step: if r.chance(85) { 1 } else { 2 },
                    body: gen_body(r, &inner, cnt, depth + 1),
                }
            }
            78..=84 if depth < 2 => {
                let iv = if depth == 0 { 8 } else { 9 };
                let mut inner = ivs.to_vec();
                inner.push(iv);
                let cnt = 1 + r.below(3) as usize;
                S::DoWhile {
                    iv,
                    n: E::V(r.below(2) as usize),
                    body: gen_body(r, &inner, cnt, depth + 1),
                }
            }
            85..=92 if depth < 2 => {
                let cond = gen_expr(r, ivs, 1);
                let then_len = 1 + r.below(2) as usize;
                let then_body = gen_body(r, ivs, then_len, depth + 1);
                S::If(cond, then_body, gen_body(r, ivs, 1, depth + 1))
            }
            _ => S::Assign(2 + r.below(6) as usize, gen_expr(r, ivs, 1)),
        };
        out.push(s);
    }
    out
}
fn random_inputs(r: &mut Rng) -> (Vec<i64>, Vec<Vec<i64>>) {
    // Counter-based subscripts reach `bound - 1 + 3`: bounds up to 13 stay inside the 16 cells, larger ones can trap.
    // Both matter: without trapping runs "a trap must stay a trap" would be checked on nothing.
    let bound = |r: &mut Rng| {
        if r.chance(15) {
            r.range(14, 18)
        } else {
            r.range(0, 13)
        }
    };
    let regs = vec![bound(r), bound(r), r.range(0, 5), r.range(0, 5), 0, 1, 2, 3];
    let mem = (0..NARR)
        .map(|_| (0..ALEN).map(|_| r.range(-5, 20)).collect())
        .collect();
    (regs, mem)
}

// ---------------------------------------------------------------- affine nests (Part 3)
#[derive(Clone, Copy, PartialEq, Debug)]
struct Sub {
    var: Option<usize>, // loop variable 0 (i) or 1 (j), or a constant subscript
    off: i64,
}
#[derive(Clone, Copy, Debug)]
struct Acc {
    arr: usize,
    sub: [Sub; 2],
}
#[derive(Clone, Debug)]
struct AStmt {
    w: Acc,
    reads: Vec<Acc>,
    k: i64,
}
/// `order[0]` is the outer loop variable. Both loops run 1..=N.
#[derive(Clone, Debug)]
struct Nest {
    order: [usize; 2],
    stmts: Vec<AStmt>,
}
type Mem2 = Vec<Vec<i64>>;
fn cell(a: &Acc, iv: [i64; 2], d: usize) -> usize {
    let g = |s: Sub| (s.off + s.var.map_or(0, |v| iv[v])) as usize;
    g(a.sub[0]) * d + g(a.sub[1])
}
fn eval_nest(n: &Nest, mem: &mut Mem2, size: i64) {
    let d = (size + 2) as usize;
    for a in 1..=size {
        for b in 1..=size {
            let mut iv = [0; 2];
            iv[n.order[0]] = a;
            iv[n.order[1]] = b;
            for s in &n.stmts {
                let v = s
                    .reads
                    .iter()
                    .fold(s.k, |acc, r| acc.wrapping_add(mem[r.arr][cell(r, iv, d)]));
                mem[s.w.arr][cell(&s.w, iv, d)] = v;
            }
        }
    }
}
fn trace(n: &Nest, size: i64) -> Vec<usize> {
    let d = (size + 2) as usize;
    let mut t = vec![];
    for a in 1..=size {
        for b in 1..=size {
            let mut iv = [0; 2];
            iv[n.order[0]] = a;
            iv[n.order[1]] = b;
            for s in &n.stmts {
                for r in s.reads.iter().chain(std::iter::once(&s.w)) {
                    t.push(r.arr * d * d + cell(r, iv, d));
                }
            }
        }
    }
    t
}
/// Fully associative LRU cache with 8-element lines; returns the number of misses.
fn misses(trace: &[usize], lines: usize) -> usize {
    let (mut lru, mut miss) = (Vec::<usize>::new(), 0);
    for &a in trace {
        let l = a / 8;
        if let Some(p) = lru.iter().position(|&x| x == l) {
            lru.remove(p);
        } else {
            miss += 1;
            if lru.len() == lines {
                lru.remove(0);
            }
        }
        lru.push(l);
    }
    miss
}

#[derive(Debug, PartialEq)]
enum Dep {
    None,
    Unknown,
    Dist([Option<i64>; 2]), // beta - alpha per loop variable; None = unconstrained
}
/// Which iteration pairs make `a` at iteration alpha and `b` at iteration beta touch the same cell?
fn solve(a: &Acc, b: &Acc) -> Dep {
    if a.arr != b.arr {
        return Dep::None;
    }
    let mut d: [Option<i64>; 2] = [None, None];
    for k in 0..2 {
        let (sa, sb) = (a.sub[k], b.sub[k]);
        match (sa.var, sb.var) {
            (None, None) => {
                if sa.off != sb.off {
                    return Dep::None;
                }
            }
            (Some(v), Some(w)) if v == w => {
                let dv = sa.off - sb.off; // alpha_v + oa == beta_v + ob
                match d[v] {
                    None => d[v] = Some(dv),
                    Some(x) if x != dv => return Dep::None,
                    _ => {}
                }
            }
            _ => return Dep::Unknown,
        }
    }
    Dep::Dist(d)
}
#[derive(Debug, PartialEq, Clone, Copy)]
enum Sign {
    Zero,
    Pos,
    Neg,
    Any,
}
fn lex_sign(e: [Option<i64>; 2]) -> Sign {
    for x in e {
        match x {
            Some(0) => continue,
            Some(v) if v > 0 => return Sign::Pos,
            Some(_) => return Sign::Neg,
            None => return Sign::Any,
        }
    }
    Sign::Zero
}
fn in_order(d: [Option<i64>; 2], order: [usize; 2]) -> [Option<i64>; 2] {
    [d[order[0]], d[order[1]]]
}
fn accesses(n: &Nest) -> Vec<(Acc, bool)> {
    n.stmts
        .iter()
        .flat_map(|s| {
            s.reads
                .iter()
                .map(|r| (*r, false))
                .chain(std::iter::once((s.w, true)))
        })
        .collect()
}
fn interchange_legal(n: &Nest) -> bool {
    let acc = accesses(n);
    for (x, xw) in &acc {
        for (y, yw) in &acc {
            if !(*xw || *yw) {
                continue;
            }
            match solve(x, y) {
                Dep::None => {}
                Dep::Unknown => return false,
                Dep::Dist(d) => {
                    let e = in_order(d, n.order);
                    match lex_sign(e) {
                        Sign::Pos => {
                            if !matches!(lex_sign([e[1], e[0]]), Sign::Pos | Sign::Zero) {
                                return false;
                            }
                        }
                        Sign::Any => return false,
                        Sign::Zero | Sign::Neg => {} // Neg is the same dependence seen from the other end
                    }
                }
            }
        }
    }
    true
}
/// True if running all of `a` then all of `b` differs from the fused loop (also the fission test).
fn fusion_prevented(a: &Nest, b: &Nest) -> bool {
    for (x, xw) in accesses(a) {
        for (y, yw) in accesses(b) {
            if !(xw || yw) {
                continue;
            }
            match solve(&x, &y) {
                Dep::None => {}
                Dep::Unknown => return true,
                Dep::Dist(d) => {
                    if matches!(lex_sign(in_order(d, a.order)), Sign::Neg | Sign::Any) {
                        return true;
                    }
                }
            }
        }
    }
    false
}
fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}
/// A[a1*x + c1] vs A[a2*y + c2]: integer solutions exist only if gcd(a1, a2) divides c2 - c1.
fn gcd_independent(a1: i64, c1: i64, a2: i64, c2: i64) -> bool {
    let g = gcd(a1, a2);
    if g == 0 { c1 != c2 } else { (c2 - c1) % g != 0 }
}
fn brute_dependent(a1: i64, c1: i64, a2: i64, c2: i64, trip: i64) -> bool {
    (0..trip).any(|x| (0..trip).any(|y| a1 * x + c1 == a2 * y + c2))
}

fn rand_sub(r: &mut Rng, pref: usize, size: i64) -> Sub {
    match r.below(100) {
        0..=59 => Sub {
            var: Some(pref),
            off: r.range(-1, 1),
        },
        60..=84 => Sub {
            var: Some(1 - pref),
            off: r.range(-1, 1),
        },
        _ => Sub {
            var: None,
            off: r.range(1, size),
        },
    }
}
fn rand_acc(r: &mut Rng, size: i64) -> Acc {
    Acc {
        arr: r.below(2) as usize,
        sub: [rand_sub(r, 0, size), rand_sub(r, 1, size)],
    }
}
fn rand_stmt(r: &mut Rng, size: i64) -> AStmt {
    AStmt {
        w: rand_acc(r, size),
        reads: (0..1 + r.below(2)).map(|_| rand_acc(r, size)).collect(),
        k: r.range(0, 9),
    }
}
fn rand_mem(r: &mut Rng, size: i64) -> Mem2 {
    (0..2)
        .map(|_| {
            (0..(size + 2) * (size + 2))
                .map(|_| r.range(-9, 9))
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------- harness
fn demo_licm_shapes() {
    println!("-- LICM safety: an invariant load in a top-tested for versus a do-while");
    let load = |arr| E::Ld(arr, Box::new(E::C(3)), false);
    let acc = S::Assign(2, E::Bin(BOp::Add, Box::new(E::V(2)), Box::new(load(1))));
    let for_loop = lower(&[S::For {
        iv: 8,
        lo: 0,
        hi: E::V(0),
        step: 1,
        body: vec![acc.clone()],
    }]);
    let do_loop = lower(&[S::DoWhile {
        iv: 8,
        n: E::V(0),
        body: vec![acc],
    }]);
    for (name, f) in [("for", for_loop), ("do-while", do_loop)] {
        let mut g = f.clone();
        let n = licm(&mut g);
        let loads_in_loop = g
            .blocks
            .iter()
            .enumerate()
            .filter(|(i, _)| *i > 0)
            .flat_map(|(_, b)| &b.ins)
            .filter(|i| matches!(i, Ins::Load(..)))
            .count();
        println!(
            "   {name:<9} hoisted {n} instructions, loads left in blocks 1..: {loads_in_loop}"
        );
        let (mut rng, mut ok) = (Rng(5), 0);
        for _ in 0..50 {
            let (regs, mem) = random_inputs(&mut rng);
            assert!(same(&interp(&f, &regs, &mem).0, &interp(&g, &regs, &mem).0));
            ok += 1;
        }
        println!("   {ok} inputs agree before/after");
        if name == "for" {
            assert!(
                loads_in_loop >= 1,
                "the body of a top-tested loop may run zero times: the (non-speculable) load must stay"
            );
        } else {
            println!("{}", g.dump());
            assert_eq!(
                loads_in_loop, 0,
                "do-while body always runs: its load dominates the exit and can be hoisted"
            );
        }
    }
}

/// Directed counterexample (random programs rarely build this shape): a check that does not run on every iteration must stay.
fn demo_conditional_check() {
    let guard = E::Bin(BOp::Lt, Box::new(E::V(8)), Box::new(E::C(3)));
    let body = vec![S::If(
        guard,
        vec![S::Store(0, E::V(8), false, E::C(7))],
        vec![],
    )];
    let f0 = lower(&[S::For {
        iv: 8,
        lo: 0,
        hi: E::V(0),
        step: 1,
        body,
    }]);
    let mut f = f0.clone();
    assert_eq!(hoist_checks(&mut f), 0, "a conditional check was hoisted");
    let mut regs = vec![0; 8];
    regs[0] = ALEN + 4; // the loop counts past the array, but only i < 3 is ever used as an index
    let mem: Mem2 = vec![vec![0; ALEN as usize]; NARR];
    let (a, b) = (interp(&f0, &regs, &mem).0, interp(&f, &regs, &mem).0);
    assert!(a.is_ok() && same(&a, &b));
    println!(
        "-- bounds hoist: `for i in 0..20 {{ if i < 3 {{ a[i] = 7 }} }}` keeps its check (hoisting it would trap a valid run)"
    );
}

fn demo_strength() {
    println!(
        "-- strength reduction + unroll + bounds hoist on `for i in 0..n {{ a[i] = i*4 + a[i+1] }}`"
    );
    let body = vec![S::Store(
        0,
        E::V(8),
        false,
        E::Bin(
            BOp::Add,
            Box::new(E::Bin(BOp::Mul, Box::new(E::V(8)), Box::new(E::C(4)))),
            Box::new(E::Ld(
                0,
                Box::new(E::Bin(BOp::Add, Box::new(E::V(8)), Box::new(E::C(1)))),
                false,
            )),
        ),
    )];
    let f0 = lower(&[S::For {
        iv: 8,
        lo: 0,
        hi: E::V(0),
        step: 1,
        body,
    }]);
    let mut f = f0.clone();
    let counts = (
        strength_reduce(&mut f),
        hoist_checks(&mut f),
        unroll(&mut f),
        licm(&mut f),
        dce(&mut f),
    );
    println!("   rewrites (sr, checks, unroll, licm, dce removed) = {counts:?}");
    let mut rng = Rng(9);
    let (mut c0, mut c1, mut w0, mut w1) = (0, 0, 0, 0);
    for n in 0..=11 {
        let mut regs = vec![0; 8];
        regs[0] = n;
        let mem: Mem2 = (0..NARR)
            .map(|a| {
                (0..ALEN)
                    .map(|i| i * (a as i64 + 1) + rng.range(0, 3))
                    .collect()
            })
            .collect();
        let (a, ia, ca) = interp(&f0, &regs, &mem);
        let (b, ib, cb) = interp(&f, &regs, &mem);
        assert!(same(&a, &b), "n={n}: {a:?} vs {b:?}");
        c0 += ia;
        c1 += ib;
        w0 += ca;
        w1 += cb;
    }
    println!("   dynamic instructions over n=0..11: {c0} -> {c1}; weighted cost {w0} -> {w1}");
    assert!(w1 < w0);
    println!("{}", f.dump());
}

fn verify_dominators() {
    let mut r = Rng(0xD0D0);
    let (mut graphs, mut irreducible) = (0, 0);
    for _ in 0..800 {
        let n = 2 + r.below(10) as usize;
        let succs: Vec<Vec<usize>> = (0..n)
            .map(|_| {
                (0..r.below(3))
                    .map(|_| r.below(n as u64) as usize)
                    .collect()
            })
            .collect();
        let c = dom_info(&succs);
        let nd = naive_dom(&succs, &c);
        for b in 0..n {
            if !c.reachable(b) {
                continue;
            }
            for a in 0..n {
                assert_eq!(
                    c.dominates(a, b),
                    nd[b].contains(&a),
                    "dominates({a},{b}) disagrees with the set definition"
                );
            }
        }
        // a back edge u->h must have h dominating u; a retreating edge whose target does NOT dominate means irreducible
        let retreating = (0..n).any(|u| {
            c.reachable(u)
                && succs[u]
                    .iter()
                    .any(|&s| c.num[s] <= c.num[u] && !c.dominates(s, u))
        });
        irreducible += retreating as usize;
        graphs += 1;
    }
    println!(
        "-- dominators: CHK == set-intersection definition on {graphs} random graphs ({irreducible} irreducible ones, loops there are not 'natural')"
    );
}

#[derive(Default)]
struct Totals {
    applied: usize,
    insns: u64,
    cost: u64,
    runs: u64,
}
fn verify_passes() {
    println!("-- 1500 random programs x 3 inputs: every pass preserves behaviour");
    type Pass = fn(&mut Func) -> usize;
    fn pipeline(f: &mut Func) -> usize {
        let a = licm(f) + strength_reduce(f) + hoist_checks(f) + unroll(f);
        a + licm(f) + dce(f)
    }
    fn sr_dce(f: &mut Func) -> usize {
        let n = strength_reduce(f);
        n + dce(f)
    }
    fn unroll_dce(f: &mut Func) -> usize {
        unroll(f) + dce(f)
    }
    let passes: [(&str, Pass); 6] = [
        ("licm", licm),
        ("strength reduce", sr_dce),
        ("bounds hoist", hoist_checks),
        ("unroll x4", unroll_dce),
        ("pipeline", pipeline),
        ("dce only", dce),
    ];
    let mut tot: Vec<Totals> = passes.iter().map(|_| Totals::default()).collect();
    let (mut base, mut loops_seen, mut traps, mut multi_loops) =
        (Totals::default(), 0usize, 0usize, 0usize);
    for seed in 0..1500u64 {
        let mut r = Rng(0xFACE_FEED ^ (seed + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let n = 2 + r.below(3) as usize;
        let prog = gen_body(&mut r, &[], n, 0);
        let f0 = lower(&prog);
        let cfg = dom_info(&f0.succ_lists());
        for b in 0..f0.blocks.len() {
            assert!(cfg.reachable(b), "front end produced an unreachable block");
        }
        let loops = find_loops(&f0, &cfg);
        for l in &loops {
            assert!(
                l.body.iter().all(|&b| cfg.dominates(l.header, b)),
                "header must dominate its natural loop"
            );
        }
        loops_seen += loops.len();
        multi_loops += (loops.len() > 1) as usize;
        let transformed: Vec<(Func, usize)> = passes
            .iter()
            .map(|(_, p)| {
                let mut g = f0.clone();
                let k = p(&mut g);
                (g, k)
            })
            .collect();
        for _ in 0..3 {
            let (regs, mem) = random_inputs(&mut r);
            let (o0, i0, c0) = interp(&f0, &regs, &mem);
            assert!(
                !matches!(o0, Err(Fail::Ub | Fail::Fuel)),
                "generated program must not hit UB or run out of fuel: {o0:?}"
            );
            let trapped = o0.is_err();
            traps += trapped as usize;
            if !trapped {
                base.insns += i0;
                base.cost += c0;
                base.runs += 1;
            }
            for (t, (g, k)) in tot.iter_mut().zip(&transformed) {
                let (o1, i1, c1) = interp(g, &regs, &mem);
                assert!(
                    same(&o0, &o1),
                    "behaviour changed (seed {seed})\n original: {o0:?}\n transformed: {o1:?}\n{}",
                    g.dump()
                );
                if !trapped {
                    t.insns += i1;
                    t.cost += c1;
                    t.runs += 1;
                }
                t.applied += *k;
            }
        }
    }
    println!(
        "   {loops_seen} natural loops found ({multi_loops} programs with several), {traps} trapping runs (traps must stay traps)"
    );
    println!(
        "   {:<16} {:>9} {:>14} {:>14} {:>9}",
        "pass", "rewrites", "dyn insns", "weighted cost", "saved"
    );
    println!(
        "   {:<16} {:>9} {:>14} {:>14}",
        "baseline", "-", base.insns, base.cost
    );
    for ((name, _), t) in passes.iter().zip(&tot) {
        // only non-trapping runs are summed, so the baseline for the saving is the same set
        let pct = 100.0 * (base.cost as f64 - t.cost as f64) / base.cost as f64;
        println!(
            "   {:<16} {:>9} {:>14} {:>14} {:>8.1}%",
            name, t.applied, t.insns, t.cost, pct
        );
        assert!(t.runs == base.runs);
    }
    assert!(
        traps > 100 && base.runs > 2000,
        "need both trapping and non-trapping runs, got {traps} and {}",
        base.runs
    );
    for ((name, _), t) in passes.iter().zip(&tot) {
        if *name != "dce only" {
            assert!(t.applied > 0, "{name} never fired: generator too weak");
        }
        assert!(
            t.cost <= base.cost + base.cost / 50,
            "{name} made programs slower"
        );
    }
    let p = &tot[4];
    assert!(p.cost < base.cost && p.insns < base.insns);
}

fn verify_nests() {
    println!("-- affine nests: dependence tests vs concrete evaluation");
    let size = 5;
    let mut r = Rng(0xAFF1);
    // GCD test soundness against brute force
    let (mut indep, mut maybe_but_none) = (0, 0);
    for _ in 0..4000 {
        let (a1, a2) = (r.range(-3, 4), r.range(-3, 4));
        let (c1, c2) = (r.range(-6, 6), r.range(-6, 6));
        let dep = brute_dependent(a1, c1, a2, c2, 12);
        if gcd_independent(a1, c1, a2, c2) {
            indep += 1;
            assert!(
                !dep,
                "GCD test claimed independence of A[{a1}x+{c1}] / A[{a2}y+{c2}] but a collision exists"
            );
        } else if !dep {
            maybe_but_none += 1; // GCD is conservative: ignores the loop bounds
        }
    }
    println!(
        "   GCD test: {indep} proven independent, never wrong; {maybe_but_none} 'maybe' answers that bounds would have refuted"
    );
    assert!(gcd_independent(2, 0, 2, 1) && !gcd_independent(2, 0, 4, 2));

    // interchange
    let (mut legal, mut illegal, mut witnessed) = (0, 0, 0);
    for _ in 0..1500 {
        let n = Nest {
            order: [0, 1],
            stmts: (0..1 + r.below(2))
                .map(|_| rand_stmt(&mut r, size))
                .collect(),
        };
        let sw = Nest {
            order: [1, 0],
            ..n.clone()
        };
        let (mut m1, mut m2) = (rand_mem(&mut r, size), vec![]);
        m2.clone_from(&m1);
        eval_nest(&n, &mut m1, size);
        eval_nest(&sw, &mut m2, size);
        if interchange_legal(&n) {
            legal += 1;
            assert_eq!(
                m1, m2,
                "interchange declared legal but results differ: {n:?}"
            );
        } else {
            illegal += 1;
            witnessed += (m1 != m2) as usize;
        }
    }
    println!(
        "   interchange: {legal} legal (all verified equal), {illegal} refused ({witnessed} of them really change results)"
    );
    assert!(legal > 100 && witnessed > 20);

    // fusion
    let (mut ok, mut blocked, mut wit) = (0, 0, 0);
    for _ in 0..1500 {
        let a = Nest {
            order: [0, 1],
            stmts: vec![rand_stmt(&mut r, size)],
        };
        let b = Nest {
            order: [0, 1],
            stmts: vec![rand_stmt(&mut r, size)],
        };
        let fused = Nest {
            order: [0, 1],
            stmts: vec![a.stmts[0].clone(), b.stmts[0].clone()],
        };
        let mut m1 = rand_mem(&mut r, size);
        let mut m2 = m1.clone();
        eval_nest(&a, &mut m1, size);
        eval_nest(&b, &mut m1, size);
        eval_nest(&fused, &mut m2, size);
        if !fusion_prevented(&a, &b) {
            ok += 1;
            assert_eq!(m1, m2, "fusion declared legal but results differ");
        } else {
            blocked += 1;
            wit += (m1 != m2) as usize;
        }
    }
    println!(
        "   fusion/fission: {ok} legal (all verified equal), {blocked} prevented ({wit} witnessed); the fission test is the same function"
    );
    assert!(ok > 50 && wit > 20);

    // locality demo: A[j][i] = A[j][i] + 1 walks a column per inner iteration
    let col = |arr| Acc {
        arr,
        sub: [
            Sub {
                var: Some(1),
                off: 0,
            },
            Sub {
                var: Some(0),
                off: 0,
            },
        ],
    };
    let nest = Nest {
        order: [0, 1],
        stmts: vec![AStmt {
            w: col(0),
            reads: vec![col(0)],
            k: 1,
        }],
    };
    let swapped = Nest {
        order: [1, 0],
        ..nest.clone()
    };
    assert!(interchange_legal(&nest));
    let (before, after) = (
        misses(&trace(&nest, 64), 32),
        misses(&trace(&swapped, 64), 32),
    );
    println!(
        "   locality: A[j][i] += 1 on 64x64 (LRU cache of 32 lines x 8 cells): {before} misses -> {after} after interchange"
    );
    assert!(after * 4 < before);
    // loop-carried flow dependence A[i][j] = A[i-1][j+1] has distance (1,-1): interchange would make it (-1,1)
    let acc = |di, dj| Acc {
        arr: 0,
        sub: [
            Sub {
                var: Some(0),
                off: di,
            },
            Sub {
                var: Some(1),
                off: dj,
            },
        ],
    };
    let skew = Nest {
        order: [0, 1],
        stmts: vec![AStmt {
            w: acc(0, 0),
            reads: vec![acc(-1, 1)],
            k: 0,
        }],
    };
    println!(
        "   A[i][j] = A[i-1][j+1] interchange legal? {} (distance (1,-1) would become (-1,1))",
        interchange_legal(&skew)
    );
    assert!(!interchange_legal(&skew));
}

fn main() {
    verify_dominators();
    demo_licm_shapes();
    demo_conditional_check();
    demo_strength();
    verify_passes();
    verify_nests();
    println!("loop_optimizations: all checks passed");
}
