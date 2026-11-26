//! RECIPE 821 (compilers): abstract interpretation: interval, sign, parity and reduced-product domains, widening/narrowing, difference bounds.
//! LEVEL: advanced
//! Use in: static analysers that must prove "this array access / division / addition can never go wrong" for ALL inputs without running the program.   Run: cargo run --release --example abstract_interpretation_intervals
//! COMMON USES: Astree (flight-control software proofs of absence of run-time errors), Frama-C's Eva plugin, the Clang Static Analyzer's range
//! constraints, LLVM's ConstantRange / CorrelatedValuePropagation (drops bounds checks and overflow flags), rustc's MIR const-prop and range
//! lints, the JIT range analyses in V8 and HotSpot that remove bounds checks, NASA's IKOS and the Crab library (open-source abstract-interpretation frameworks).
//! Course: Cousot and Cousot, "Abstract interpretation: a unified lattice model for static analysis of programs" (POPL 1977; cited by title only);
//! Moller and Schwartzbach "Static Program Analysis", the material on widening and narrowing.
#![allow(dead_code)]
//!
//! THE IDEA. A program's concrete behaviours are sets of states (undecidable to compute). Pick an ABSTRACT DOMAIN of finitely describable
//! sets (here: intervals [lo,hi], sign sets, parity) related to concrete sets by a GALOIS CONNECTION: alpha (best abstraction of a set)
//! and gamma (the set an abstract value stands for) with  S subset gamma(alpha(S))  and  alpha(gamma(a)) <= a.  Each abstract operation
//! must over-approximate the concrete one: op(x,y) in gamma(op#(a,b)) whenever x in gamma(a), y in gamma(b). Then every fact the analysis derives
//! is TRUE of all executions ("sound"), but it may also say "maybe" about things that are fine ("incomplete"): that is the precision/cost trade-off.
//! We test local soundness (random finite sets, every concrete pair) and global soundness (2000 random programs, concrete runs, every observed value).
//!
//! LOOPS NEED WIDENING. The loop invariant is a least fixpoint over an infinite-height lattice (the interval chain [0,0] < [0,1] < [0,2] ...
//! never stabilises). WIDENING (nabla) jumps an unstable bound to the next THRESHOLD (a constant from the program) or to infinity
//! (here: the i32 limits), which forces convergence in a few steps. NARROWING then re-applies the loop body a fixed number of times
//! (we intersect with F(inv)) to win back precision lost by the jump, e.g. `i < 100` turns [0, MAX] into [0, 100].
//!
//! THE MACHINE MODEL. 32-bit signed integers; `+ - *` that leave the range WRAP (the concrete interpreter does so and records an overflow
//! event); division by zero records an event and yields 0; out-of-bounds table/out accesses record an event and the program continues
//! (so that one run exercises many sites). The analysis raises an ALARM for a check it cannot prove safe; the soundness test asserts that
//! every concrete event happens only at a site that raised an alarm, i.e. "no alarm" really means "proved safe".
//!
//! DOMAINS. Interval (with thresholds), Sign ({neg,zero,pos} powerset), Parity ({even,odd}), and their REDUCED PRODUCT Interval x Parity (each
//! component tightens the other: an even-parity interval [1,6] becomes [2,6]; a singleton interval fixes the parity). Parity alone can prove
//! `100 / (2*k+1)` safe where intervals cannot (k in [-5,5] gives [-9,11] which contains 0). Intervals prove indices where parity cannot.
//! RELATIONAL DOMAINS: intervals lose x - y facts (`z = y - x` where y = x gives [-10,10]). A difference-bound matrix (the "zones" domain, the
//! simplest octagon-style relational domain) stores x_i - x_j <= c, closed by Floyd-Warshall; we verify the closure against brute force.
//!
//! Read: Cousot and Cousot (POPL 1977, title only); Moller and Schwartzbach, Static Program Analysis https://cs.au.dk/~amoeller/spa/ ;
//! Mine, "The Octagon Abstract Domain" (title only; zones/DBM are the subset used here); Cornell CS 6120 lessons https://www.cs.cornell.edu/courses/cs6120/2020fa/ ;
//! https://en.wikipedia.org/wiki/Abstract_interpretation and https://en.wikipedia.org/wiki/Widening_(computer_science)
//! TRY NEXT:
//!   - [S] In `Itv::widen` drop the threshold list (pass an empty slice) and print `An::iters` for the demo loops: how many extra narrowing passes does `Config::narrow` need to recover [0,100]?
//!   - [S] Make `Sign::binop` for `Op::Sub` claim `{pos}` for pos-pos; the local-soundness test `check_ops` should fail: read the counterexample it prints.
//!   - [M] Predict, then measure: if you add a `Mod` operator, which domain (interval, sign, parity) benefits most for proving `x % 2 == 0` guards, and does the reduced product help?
//!   - [M] Port: add a `Bool`-like `assert(cond)` statement and report which asserts each domain proves (use `assume` on the negated condition and check for bottom).
//!   - [A] Project: combine this with `dataflow_analysis_framework` (use its worklist solver over a CFG instead of the structured AST recursion) and with `loop_optimizations` (delete bounds checks the analysis proves safe, then re-run the equivalence test).
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

const I32MIN: i64 = i32::MIN as i64;
const I32MAX: i64 = i32::MAX as i64;
const NV: usize = 4; // v0,v1 are inputs; v2,v3 start at 0
const TABLE_LEN: i64 = 8; // read-only table, every entry in [0,9]
const OUT_LEN: i64 = 8; // write-only output buffer

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

// ---------------------------------------------------------------- the language

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}
impl Cmp {
    fn negate(self) -> Cmp {
        match self {
            Cmp::Lt => Cmp::Ge,
            Cmp::Le => Cmp::Gt,
            Cmp::Gt => Cmp::Le,
            Cmp::Ge => Cmp::Lt,
            Cmp::Eq => Cmp::Ne,
            Cmp::Ne => Cmp::Eq,
        }
    }
    fn holds(self, a: i64, b: i64) -> bool {
        match self {
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
        }
    }
}
#[derive(Clone, Debug)]
enum Expr {
    Num(i64),
    Var(usize),
    Bin(usize, Op, Box<Expr>, Box<Expr>), // (site id, ...)
}
#[derive(Clone, Debug)]
struct Cond {
    op: Cmp,
    l: Expr,
    r: Expr,
}
#[derive(Clone, Debug)]
enum Stmt {
    Assign(usize, usize, Expr),
    Load(usize, usize, Expr), // var = TABLE[idx]
    Store(usize, Expr, Expr), // OUT[idx] = val
    If(Cond, Vec<Stmt>, Vec<Stmt>),
    While(Cond, Vec<Stmt>),
}
#[derive(Clone, Debug)]
struct Prog {
    stmts: Vec<Stmt>,
    ranges: [(i64, i64); 2],
}
/// The kind of run-time error a check guards against.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Chk {
    Ovf,
    Div,
    Oob,
}

/// Builder: hands out fresh site ids so every check and every observed value has a stable name.
#[derive(Default)]
struct B {
    next: usize,
}
impl B {
    fn site(&mut self) -> usize {
        self.next += 1;
        self.next - 1
    }
    fn bin(&mut self, op: Op, l: Expr, r: Expr) -> Expr {
        Expr::Bin(self.site(), op, Box::new(l), Box::new(r))
    }
    fn assign(&mut self, v: usize, e: Expr) -> Stmt {
        Stmt::Assign(self.site(), v, e)
    }
    fn load(&mut self, v: usize, idx: Expr) -> Stmt {
        Stmt::Load(self.site(), v, idx)
    }
    fn store(&mut self, idx: Expr, val: Expr) -> Stmt {
        Stmt::Store(self.site(), idx, val)
    }
}
fn num(k: i64) -> Expr {
    Expr::Num(k)
}
fn var(v: usize) -> Expr {
    Expr::Var(v)
}
fn cond(op: Cmp, l: Expr, r: Expr) -> Cond {
    Cond { op, l, r }
}

/// Every check a program contains (what a perfect analysis would have to prove).
fn all_checks(stmts: &[Stmt], out: &mut Vec<(usize, Chk)>) {
    fn expr(e: &Expr, out: &mut Vec<(usize, Chk)>) {
        if let Expr::Bin(s, op, l, r) = e {
            out.push((*s, Chk::Ovf));
            if *op == Op::Div {
                out.push((*s, Chk::Div));
            }
            expr(l, out);
            expr(r, out);
        }
    }
    for s in stmts {
        match s {
            Stmt::Assign(_, _, e) => expr(e, out),
            Stmt::Load(s, _, e) => {
                out.push((*s, Chk::Oob));
                expr(e, out)
            }
            Stmt::Store(s, i, v) => {
                out.push((*s, Chk::Oob));
                expr(i, out);
                expr(v, out)
            }
            Stmt::If(c, t, e) => {
                expr(&c.l, out);
                expr(&c.r, out);
                all_checks(t, out);
                all_checks(e, out)
            }
            Stmt::While(c, b) => {
                expr(&c.l, out);
                expr(&c.r, out);
                all_checks(b, out)
            }
        }
    }
}
fn collect_consts(stmts: &[Stmt], out: &mut HashSet<i64>) {
    fn expr(e: &Expr, out: &mut HashSet<i64>) {
        match e {
            Expr::Num(k) => {
                out.extend([*k - 1, *k, *k + 1]);
            }
            Expr::Var(_) => {}
            Expr::Bin(_, _, l, r) => {
                expr(l, out);
                expr(r, out)
            }
        }
    }
    for s in stmts {
        match s {
            Stmt::Assign(_, _, e) | Stmt::Load(_, _, e) => expr(e, out),
            Stmt::Store(_, i, v) => {
                expr(i, out);
                expr(v, out)
            }
            Stmt::If(c, t, e) => {
                expr(&c.l, out);
                expr(&c.r, out);
                collect_consts(t, out);
                collect_consts(e, out)
            }
            Stmt::While(c, b) => {
                expr(&c.l, out);
                expr(&c.r, out);
                collect_consts(b, out)
            }
        }
    }
}

// ---------------------------------------------------------------- concrete semantics (the oracle)

/// Exact 32-bit semantics: returns (result, overflowed, divided_by_zero).
fn concrete_op(op: Op, x: i64, y: i64) -> (i64, bool, bool) {
    let r = match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Div => {
            if y == 0 {
                return (0, false, true);
            }
            x / y
        }
    };
    if (I32MIN..=I32MAX).contains(&r) {
        (r, false, false)
    } else {
        (r as i32 as i64, true, false)
    }
}
fn table(i: i64) -> i64 {
    (i * 7 + 3) % 10
}

struct Conc {
    vars: [i64; NV],
    fuel: usize,
    obs: Vec<(usize, i64)>,
    events: Vec<(usize, Chk)>,
}
impl Conc {
    /// Spend one unit of fuel; false when none is left. This is the ONLY place fuel decreases. An unguarded `fuel -= 1` after a loop body
    /// that had just used the last unit would wrap to usize::MAX in release builds (no overflow checks) and make a divergent loop run forever.
    fn tick(&mut self) -> bool {
        match self.fuel.checked_sub(1) {
            Some(left) => {
                self.fuel = left;
                true
            }
            None => false,
        }
    }
    fn eval(&mut self, e: &Expr) -> i64 {
        match e {
            Expr::Num(k) => *k,
            Expr::Var(v) => self.vars[*v],
            Expr::Bin(s, op, l, r) => {
                let (x, y) = (self.eval(l), self.eval(r));
                let (v, ovf, dz) = concrete_op(*op, x, y);
                if ovf {
                    self.events.push((*s, Chk::Ovf));
                }
                if dz {
                    self.events.push((*s, Chk::Div));
                }
                self.obs.push((*s, v));
                v
            }
        }
    }
    fn test(&mut self, c: &Cond) -> bool {
        let (a, b) = (self.eval(&c.l), self.eval(&c.r));
        c.op.holds(a, b)
    }
    /// Returns false when the fuel ran out (the run is abandoned; everything observed so far is still valid).
    fn exec(&mut self, stmts: &[Stmt]) -> bool {
        for s in stmts {
            if !self.tick() {
                return false;
            }
            match s {
                Stmt::Assign(site, v, e) => {
                    let x = self.eval(e);
                    self.vars[*v] = x;
                    self.obs.push((*site, x));
                }
                Stmt::Load(site, v, e) => {
                    let i = self.eval(e);
                    let x = if (0..TABLE_LEN).contains(&i) {
                        table(i)
                    } else {
                        self.events.push((*site, Chk::Oob));
                        0
                    };
                    self.vars[*v] = x;
                    self.obs.push((*site, x));
                }
                Stmt::Store(site, i, v) => {
                    let (i, _) = (self.eval(i), self.eval(v));
                    if !(0..OUT_LEN).contains(&i) {
                        self.events.push((*site, Chk::Oob));
                    }
                }
                Stmt::If(c, t, e) => {
                    let ok = if self.test(c) {
                        self.exec(t)
                    } else {
                        self.exec(e)
                    };
                    if !ok {
                        return false;
                    }
                }
                Stmt::While(c, b) => {
                    while self.test(c) {
                        if !self.tick() || !self.exec(b) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
}

// ---------------------------------------------------------------- abstract domains

trait Dom: Clone + PartialEq + Debug {
    fn name() -> &'static str;
    fn top() -> Self;
    fn bot() -> Self;
    fn is_bot(&self) -> bool;
    fn join(&self, o: &Self) -> Self;
    fn meet(&self, o: &Self) -> Self;
    fn leq(&self, o: &Self) -> bool {
        self.join(o) == *o
    }
    /// Widening: must over-approximate join and force stabilisation of ascending chains.
    fn widen(&self, o: &Self, _th: &[i64]) -> Self {
        self.join(o)
    }
    /// alpha of a single integer / of an interval of integers.
    fn constant(k: i64) -> Self {
        Self::range(k, k)
    }
    fn range(lo: i64, hi: i64) -> Self;
    /// Tightest [lo,hi] this value implies (None for bottom). Used by guards and by the checks.
    fn bounds(&self) -> Option<(i64, i64)>;
    fn contains(&self, v: i64) -> bool;
    fn excludes_zero(&self) -> bool;
    /// Abstract `op`; the flag says "this operation MAY overflow 32 bits" (the result already accounts for wrapping).
    fn binop(op: Op, a: &Self, b: &Self) -> (Self, bool);
    /// Refine a and b under the assumption `a cmp b` (default: go through bounds, which every domain can produce).
    fn assume(c: Cmp, a: &Self, b: &Self) -> (Self, Self) {
        let (Some((al, ah)), Some((bl, bh))) = (a.bounds(), b.bounds()) else {
            return (Self::bot(), Self::bot());
        };
        let ((a1, a2), (b1, b2)) = match c {
            Cmp::Lt => ((al, ah.min(bh - 1)), (bl.max(al + 1), bh)),
            Cmp::Le => ((al, ah.min(bh)), (bl.max(al), bh)),
            Cmp::Gt => ((al.max(bl + 1), ah), (bl, bh.min(ah - 1))),
            Cmp::Ge => ((al.max(bl), ah), (bl, bh.min(ah))),
            Cmp::Eq => ((al.max(bl), ah.min(bh)), (al.max(bl), ah.min(bh))),
            Cmp::Ne => {
                let cut = |lo: i64, hi: i64, o: (i64, i64)| {
                    if o.0 == o.1 && lo == o.0 {
                        (lo + 1, hi)
                    } else if o.0 == o.1 && hi == o.0 {
                        (lo, hi - 1)
                    } else {
                        (lo, hi)
                    }
                };
                (cut(al, ah, (bl, bh)), cut(bl, bh, (al, ah)))
            }
        };
        let r = |lo: i64, hi: i64, x: &Self| {
            if lo > hi {
                Self::bot()
            } else {
                x.meet(&Self::range(lo, hi))
            }
        };
        (r(a1, a2, a), r(b1, b2, b))
    }
}

// ---- intervals

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Itv {
    Bot,
    R(i64, i64),
}
impl Dom for Itv {
    fn name() -> &'static str {
        "interval"
    }
    fn top() -> Self {
        Itv::R(I32MIN, I32MAX)
    }
    fn bot() -> Self {
        Itv::Bot
    }
    fn is_bot(&self) -> bool {
        *self == Itv::Bot
    }
    fn join(&self, o: &Self) -> Self {
        match (*self, *o) {
            (Itv::Bot, x) | (x, Itv::Bot) => x,
            (Itv::R(a, b), Itv::R(c, d)) => Itv::R(a.min(c), b.max(d)),
        }
    }
    fn meet(&self, o: &Self) -> Self {
        match (*self, *o) {
            (Itv::R(a, b), Itv::R(c, d)) if a.max(c) <= b.min(d) => Itv::R(a.max(c), b.min(d)),
            _ => Itv::Bot,
        }
    }
    fn widen(&self, o: &Self, th: &[i64]) -> Self {
        match (*self, *o) {
            (Itv::Bot, x) | (x, Itv::Bot) => x,
            (Itv::R(a, b), Itv::R(c, d)) => {
                // next threshold below / above the new bound; the i32 limits are always thresholds
                let lo = if c < a {
                    th.iter()
                        .copied()
                        .filter(|t| *t <= c)
                        .max()
                        .unwrap_or(I32MIN)
                        .max(I32MIN)
                } else {
                    a
                };
                let hi = if d > b {
                    th.iter()
                        .copied()
                        .filter(|t| *t >= d)
                        .min()
                        .unwrap_or(I32MAX)
                        .min(I32MAX)
                } else {
                    b
                };
                Itv::R(lo, hi)
            }
        }
    }
    fn range(lo: i64, hi: i64) -> Self {
        if lo > hi { Itv::Bot } else { Itv::R(lo, hi) }
    }
    fn bounds(&self) -> Option<(i64, i64)> {
        if let Itv::R(a, b) = *self {
            Some((a, b))
        } else {
            None
        }
    }
    fn contains(&self, v: i64) -> bool {
        matches!(*self, Itv::R(a, b) if a <= v && v <= b)
    }
    fn excludes_zero(&self) -> bool {
        !self.contains(0)
    }
    fn binop(op: Op, a: &Self, b: &Self) -> (Self, bool) {
        let (Itv::R(al, ah), Itv::R(bl, bh)) = (*a, *b) else {
            return (Itv::Bot, false);
        };
        let (lo, hi) = match op {
            Op::Add => (al + bl, ah + bh),
            Op::Sub => (al - bh, ah - bl),
            Op::Mul => {
                let p = [al * bl, al * bh, ah * bl, ah * bh];
                (*p.iter().min().unwrap(), *p.iter().max().unwrap())
            }
            Op::Div => {
                // split the divisor around zero; division by exactly zero yields 0 in our semantics
                let mut r = if bl <= 0 && 0 <= bh {
                    Itv::R(0, 0)
                } else {
                    Itv::Bot
                };
                for (l, h) in [(bl.max(1), bh), (bl, bh.min(-1))] {
                    if l <= h {
                        let q = [al / l, al / h, ah / l, ah / h];
                        r = r.join(&Itv::R(*q.iter().min().unwrap(), *q.iter().max().unwrap()));
                    }
                }
                let Itv::R(l, h) = r else {
                    return (Itv::Bot, false);
                };
                (l, h)
            }
        };
        if lo < I32MIN || hi > I32MAX {
            (Itv::top(), true)
        } else {
            (Itv::R(lo, hi), false)
        }
    }
}

// ---- signs

const NEG: u8 = 1;
const ZERO: u8 = 2;
const POS: u8 = 4;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Sign(u8);
fn sign_of(k: i64) -> u8 {
    [NEG, ZERO, POS][(k.signum() + 1) as usize]
}
fn signs(s: u8) -> impl Iterator<Item = u8> {
    [NEG, ZERO, POS].into_iter().filter(move |b| s & b != 0)
}
impl Dom for Sign {
    fn name() -> &'static str {
        "sign"
    }
    fn top() -> Self {
        Sign(7)
    }
    fn bot() -> Self {
        Sign(0)
    }
    fn is_bot(&self) -> bool {
        self.0 == 0
    }
    fn join(&self, o: &Self) -> Self {
        Sign(self.0 | o.0)
    }
    fn meet(&self, o: &Self) -> Self {
        Sign(self.0 & o.0)
    }
    fn range(lo: i64, hi: i64) -> Self {
        if lo > hi {
            return Sign(0);
        }
        Sign(
            (if lo < 0 { NEG } else { 0 })
                | (if lo <= 0 && hi >= 0 { ZERO } else { 0 })
                | (if hi > 0 { POS } else { 0 }),
        )
    }
    fn bounds(&self) -> Option<(i64, i64)> {
        if self.0 == 0 {
            return None;
        }
        let lo = if self.0 & NEG != 0 {
            I32MIN
        } else if self.0 & ZERO != 0 {
            0
        } else {
            1
        };
        let hi = if self.0 & POS != 0 {
            I32MAX
        } else if self.0 & ZERO != 0 {
            0
        } else {
            -1
        };
        Some((lo, hi))
    }
    fn contains(&self, v: i64) -> bool {
        self.0 & sign_of(v) != 0
    }
    fn excludes_zero(&self) -> bool {
        self.0 & ZERO == 0
    }
    fn binop(op: Op, a: &Self, b: &Self) -> (Self, bool) {
        if a.0 == 0 || b.0 == 0 {
            return (Sign(0), false);
        }
        let mut r = 0u8;
        for x in signs(a.0) {
            for y in signs(b.0) {
                r |= match op {
                    Op::Add => add1(x, y),
                    Op::Sub => add1(x, neg1(y)),
                    Op::Mul => {
                        if x == ZERO || y == ZERO {
                            ZERO
                        } else if x == y {
                            POS
                        } else {
                            NEG
                        }
                    }
                    Op::Div => {
                        if y == ZERO || x == ZERO {
                            ZERO
                        } else if x == y {
                            ZERO | POS
                        } else {
                            ZERO | NEG
                        }
                    }
                };
            }
        }
        let (a, b) = (a.0, b.0);
        let no_pos = |s: u8| s & POS == 0;
        let no_neg = |s: u8| s & NEG == 0;
        let safe = match op {
            Op::Add => {
                a == ZERO || b == ZERO || (no_pos(a) && no_neg(b)) || (no_neg(a) && no_pos(b))
            }
            Op::Sub => b == ZERO || (no_neg(a) && no_neg(b)),
            Op::Mul => a == ZERO || b == ZERO,
            Op::Div => !(a & NEG != 0 && b & NEG != 0), // only MIN / -1 overflows
        };
        if safe {
            (Sign(r), false)
        } else {
            (Sign(7), true)
        }
    }
}
fn neg1(s: u8) -> u8 {
    match s {
        NEG => POS,
        POS => NEG,
        z => z,
    }
}
fn add1(x: u8, y: u8) -> u8 {
    match (x, y) {
        (ZERO, s) | (s, ZERO) => s,
        (NEG, NEG) => NEG,
        (POS, POS) => POS,
        _ => 7,
    }
}

// ---- parity

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Par(u8); // bit0 even, bit1 odd
impl Dom for Par {
    fn name() -> &'static str {
        "parity"
    }
    fn top() -> Self {
        Par(3)
    }
    fn bot() -> Self {
        Par(0)
    }
    fn is_bot(&self) -> bool {
        self.0 == 0
    }
    fn join(&self, o: &Self) -> Self {
        Par(self.0 | o.0)
    }
    fn meet(&self, o: &Self) -> Self {
        Par(self.0 & o.0)
    }
    fn range(lo: i64, hi: i64) -> Self {
        if lo > hi {
            Par(0)
        } else if lo == hi {
            Par(1 << lo.rem_euclid(2))
        } else {
            Par(3)
        }
    }
    fn bounds(&self) -> Option<(i64, i64)> {
        if self.0 == 0 {
            None
        } else {
            Some((I32MIN, I32MAX))
        }
    }
    fn contains(&self, v: i64) -> bool {
        self.0 >> v.rem_euclid(2) & 1 == 1
    }
    fn excludes_zero(&self) -> bool {
        self.0 == 2
    }
    fn binop(op: Op, a: &Self, b: &Self) -> (Self, bool) {
        if a.0 == 0 || b.0 == 0 {
            return (Par(0), false);
        }
        if op == Op::Div {
            return (Par(3), true);
        }
        let mut r = 0;
        for x in 0..2u8 {
            for y in 0..2u8 {
                if a.0 >> x & 1 == 1 && b.0 >> y & 1 == 1 {
                    let p = if op == Op::Mul { x & y } else { x ^ y };
                    r |= 1 << p;
                }
            }
        }
        (Par(r), true) // wrapping mod 2^32 preserves parity, but we can never prove "no overflow"
    }
}

// ---- reduced product Interval x Parity

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Prod(Itv, Par);
impl Prod {
    /// The reduction: each component tightens the other.
    fn reduce(self) -> Prod {
        let Prod(i, p) = self;
        let (Itv::R(mut lo, mut hi), true) = (i, p.0 != 0) else {
            return Prod(Itv::Bot, Par(0));
        };
        if p.0 != 3 {
            let want = if p.0 == 1 { 0 } else { 1 };
            if lo.rem_euclid(2) != want {
                lo += 1;
            }
            if hi.rem_euclid(2) != want {
                hi -= 1;
            }
        }
        if lo > hi {
            return Prod(Itv::Bot, Par(0));
        }
        let p = if lo == hi {
            p.meet(&Par::range(lo, lo))
        } else {
            p
        };
        if p.is_bot() {
            Prod(Itv::Bot, Par(0))
        } else {
            Prod(Itv::R(lo, hi), p)
        }
    }
}
impl Dom for Prod {
    fn name() -> &'static str {
        "interval x parity"
    }
    fn top() -> Self {
        Prod(Itv::top(), Par::top())
    }
    fn bot() -> Self {
        Prod(Itv::Bot, Par(0))
    }
    fn is_bot(&self) -> bool {
        self.0.is_bot()
    }
    fn join(&self, o: &Self) -> Self {
        Prod(self.0.join(&o.0), self.1.join(&o.1))
    }
    fn meet(&self, o: &Self) -> Self {
        Prod(self.0.meet(&o.0), self.1.meet(&o.1)).reduce()
    }
    fn widen(&self, o: &Self, th: &[i64]) -> Self {
        Prod(self.0.widen(&o.0, th), self.1.join(&o.1)) // no reduction here: it could undo the jump and delay convergence
    }
    fn range(lo: i64, hi: i64) -> Self {
        Prod(Itv::range(lo, hi), Par::range(lo, hi)).reduce()
    }
    fn bounds(&self) -> Option<(i64, i64)> {
        self.0.bounds()
    }
    fn contains(&self, v: i64) -> bool {
        self.0.contains(v) && self.1.contains(v)
    }
    fn excludes_zero(&self) -> bool {
        self.0.excludes_zero() || self.1.excludes_zero()
    }
    fn binop(op: Op, a: &Self, b: &Self) -> (Self, bool) {
        let (ri, oi) = Itv::binop(op, &a.0, &b.0);
        let (rp, op_) = Par::binop(op, &a.1, &b.1);
        (Prod(ri, rp).reduce(), oi && op_) // proved overflow-free if EITHER component proves it
    }
}

// ---------------------------------------------------------------- the abstract interpreter

type Env<D> = Option<Vec<D>>;
#[derive(Clone)]
struct Config {
    thresholds: Vec<i64>,
    narrow: usize,
    widen: bool,
    delay: usize,
}
const MAX_ITERS: usize = 200;

struct An<D: Dom> {
    cfg: Config,
    rec: bool,
    alarms: HashSet<(usize, Chk)>,
    vals: HashMap<usize, D>,
    iters: usize,
    fuse_trips: usize,
}
fn join_env<D: Dom>(a: &Env<D>, b: &Env<D>) -> Env<D> {
    match (a, b) {
        (None, x) | (x, None) => x.clone(),
        (Some(x), Some(y)) => Some(x.iter().zip(y).map(|(p, q)| p.join(q)).collect()),
    }
}
fn widen_env<D: Dom>(a: &Env<D>, b: &Env<D>, th: &[i64]) -> Env<D> {
    match (a, b) {
        (None, x) | (x, None) => x.clone(),
        (Some(x), Some(y)) => Some(x.iter().zip(y).map(|(p, q)| p.widen(q, th)).collect()),
    }
}
fn meet_env<D: Dom>(a: &Env<D>, b: &Env<D>) -> Env<D> {
    let (Some(x), Some(y)) = (a, b) else {
        return None;
    };
    let v: Vec<D> = x.iter().zip(y).map(|(p, q)| p.meet(q)).collect();
    if v.iter().any(D::is_bot) {
        None
    } else {
        Some(v)
    }
}
fn leq_env<D: Dom>(a: &Env<D>, b: &Env<D>) -> bool {
    match (a, b) {
        (None, _) => true,
        (_, None) => false,
        (Some(x), Some(y)) => x.iter().zip(y).all(|(p, q)| p.leq(q)),
    }
}

impl<D: Dom> An<D> {
    fn new(cfg: Config) -> Self {
        An {
            cfg,
            rec: true,
            alarms: HashSet::new(),
            vals: HashMap::new(),
            iters: 0,
            fuse_trips: 0,
        }
    }
    fn alarm(&mut self, s: usize, c: Chk) {
        if self.rec {
            self.alarms.insert((s, c));
        }
    }
    fn record(&mut self, s: usize, v: &D) {
        if self.rec {
            let e = self.vals.entry(s).or_insert_with(D::bot);
            *e = e.join(v);
        }
    }
    fn eval(&mut self, e: &Expr, env: &[D]) -> D {
        match e {
            Expr::Num(k) => D::constant(*k),
            Expr::Var(v) => env[*v].clone(),
            Expr::Bin(s, op, l, r) => {
                let (a, b) = (self.eval(l, env), self.eval(r, env));
                if a.is_bot() || b.is_bot() {
                    return D::bot();
                }
                if *op == Op::Div && !b.excludes_zero() {
                    self.alarm(*s, Chk::Div);
                }
                let (res, ovf) = D::binop(*op, &a, &b);
                if ovf {
                    self.alarm(*s, Chk::Ovf);
                }
                self.record(*s, &res);
                res
            }
        }
    }
    fn index_ok(&mut self, site: usize, idx: &D, len: i64) {
        match idx.bounds() {
            Some((lo, hi)) if lo >= 0 && hi < len => {}
            Some(_) => self.alarm(site, Chk::Oob),
            None => {}
        }
    }
    fn assume(&mut self, c: &Cond, env: &[D], positive: bool) -> Env<D> {
        let op = if positive { c.op } else { c.op.negate() };
        let (a, b) = (self.eval(&c.l, env), self.eval(&c.r, env));
        if a.is_bot() || b.is_bot() {
            return None;
        }
        let (a2, b2) = D::assume(op, &a, &b);
        if a2.is_bot() || b2.is_bot() {
            return None;
        }
        let mut out = env.to_vec();
        for (e, refined) in [(&c.l, a2), (&c.r, b2)] {
            if let Expr::Var(i) = e {
                out[*i] = out[*i].meet(&refined);
                if out[*i].is_bot() {
                    return None;
                }
            }
        }
        Some(out)
    }
    fn exec(&mut self, stmts: &[Stmt], mut env: Env<D>) -> Env<D> {
        for s in stmts {
            let Some(e) = env.clone() else { return None };
            env = match s {
                Stmt::Assign(site, v, ex) => {
                    let x = self.eval(ex, &e);
                    if x.is_bot() {
                        None
                    } else {
                        self.record(*site, &x);
                        let mut n = e;
                        n[*v] = x;
                        Some(n)
                    }
                }
                Stmt::Load(site, v, ex) => {
                    let i = self.eval(ex, &e);
                    if i.is_bot() {
                        None
                    } else {
                        self.index_ok(*site, &i, TABLE_LEN);
                        let x = D::range(0, 9).join(&D::constant(0)); // table entries are in [0,9]; an OOB read yields 0
                        self.record(*site, &x);
                        let mut n = e;
                        n[*v] = x;
                        Some(n)
                    }
                }
                Stmt::Store(site, i, v) => {
                    let (i, x) = (self.eval(i, &e), self.eval(v, &e));
                    if i.is_bot() || x.is_bot() {
                        None
                    } else {
                        self.index_ok(*site, &i, OUT_LEN);
                        Some(e)
                    }
                }
                Stmt::If(c, t, f) => {
                    let (et, ef) = (self.assume(c, &e, true), self.assume(c, &e, false));
                    let (rt, rf) = (self.exec(t, et), self.exec(f, ef));
                    join_env(&rt, &rf)
                }
                Stmt::While(c, body) => self.exec_while(c, body, e),
            };
        }
        env
    }
    fn exec_while(&mut self, c: &Cond, body: &[Stmt], init: Vec<D>) -> Env<D> {
        let saved = self.rec;
        self.rec = false; // alarms are only collected on the final, stable invariant
        let init = Some(init);
        let mut inv = init.clone();
        let mut it = 0;
        loop {
            let entry = self.assume_opt(c, &inv, true);
            let after = self.exec(body, entry);
            let cand = join_env(&init, &after);
            let next = if self.cfg.widen && it >= self.cfg.delay {
                widen_env(&inv, &cand, &self.cfg.thresholds)
            } else {
                join_env(&inv, &cand)
            };
            if leq_env(&next, &inv) {
                break;
            }
            inv = next;
            it += 1;
            self.iters += 1;
            if it > MAX_ITERS {
                // fuse: give up precision, stay sound
                self.fuse_trips += 1;
                inv = Some(vec![D::top(); NV]);
                break;
            }
        }
        for _ in 0..self.cfg.narrow {
            let entry = self.assume_opt(c, &inv, true);
            let after = self.exec(body, entry);
            inv = meet_env(&inv, &join_env(&init, &after));
            self.iters += 1;
        }
        self.rec = saved;
        let entry = self.assume_opt(c, &inv, true);
        if saved {
            let _ = self.exec(body, entry); // one recording pass over the stable invariant
        }
        self.assume_opt(c, &inv, false)
    }
    fn assume_opt(&mut self, c: &Cond, e: &Env<D>, pos: bool) -> Env<D> {
        e.as_ref().and_then(|v| self.assume(c, v, pos))
    }
}

fn analyze<D: Dom>(p: &Prog, cfg: &Config) -> (An<D>, Env<D>) {
    let mut an = An::<D>::new(cfg.clone());
    let init: Vec<D> = (0..NV)
        .map(|i| {
            if i < 2 {
                D::range(p.ranges[i].0, p.ranges[i].1)
            } else {
                D::constant(0)
            }
        })
        .collect();
    let out = an.exec(&p.stmts, Some(init));
    (an, out)
}
fn make_config(p: &Prog, thresholds: bool, narrow: usize, widen: bool) -> Config {
    let mut c = HashSet::new();
    collect_consts(&p.stmts, &mut c);
    c.extend([-1, 0, 1]);
    let mut th: Vec<i64> = if thresholds {
        c.into_iter().collect()
    } else {
        vec![]
    };
    th.sort();
    Config {
        thresholds: th,
        narrow,
        widen,
        delay: 1,
    }
}
/// (proven safe, total) checks.
fn precision(p: &Prog, alarms: &HashSet<(usize, Chk)>) -> (usize, usize) {
    let mut all = vec![];
    all_checks(&p.stmts, &mut all);
    (
        all.iter().filter(|c| !alarms.contains(c)).count(),
        all.len(),
    )
}

// ---------------------------------------------------------------- random programs

struct Gen<'a> {
    rng: &'a mut Rng,
    b: B,
    locked: Vec<usize>,
}
impl Gen<'_> {
    fn expr(&mut self, d: u32) -> Expr {
        let k = if d == 0 {
            self.rng.below(2)
        } else {
            self.rng.below(5)
        };
        match k {
            0 => num(self.rng.range(-3, 12)),
            1 => var(self.rng.below(NV as u64) as usize),
            _ => {
                let op = match self.rng.below(20) {
                    0..=6 => Op::Add,
                    7..=11 => Op::Sub,
                    12..=16 => Op::Mul,
                    _ => Op::Div,
                };
                let (l, r) = (self.expr(d - 1), self.expr(d - 1));
                self.b.bin(op, l, r)
            }
        }
    }
    fn free_var(&mut self) -> usize {
        loop {
            let v = self.rng.below(NV as u64) as usize;
            if !self.locked.contains(&v) {
                return v;
            }
        }
    }
    fn cond(&mut self) -> Cond {
        let op = [Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge, Cmp::Eq, Cmp::Ne][self.rng.below(6) as usize];
        cond(op, self.expr(1), self.expr(1))
    }
    fn block(&mut self, depth: u32) -> Vec<Stmt> {
        let n = 1 + self.rng.below(3);
        let mut out = vec![];
        for _ in 0..n {
            self.stmt(depth, &mut out);
        }
        out
    }
    fn stmt(&mut self, depth: u32, out: &mut Vec<Stmt>) {
        match self.rng.below(100) {
            0..=39 => {
                let (v, e) = (self.free_var(), self.expr(2));
                out.push(self.b.assign(v, e));
            }
            40..=49 => {
                let (v, e) = (self.free_var(), self.expr(1));
                out.push(self.b.load(v, e));
            }
            50..=59 => {
                let (i, v) = (self.expr(1), self.expr(1));
                out.push(self.b.store(i, v));
            }
            60..=74 if depth > 0 => {
                let c = self.cond();
                let (t, e) = (self.block(depth - 1), self.block(depth - 1));
                out.push(Stmt::If(c, t, e));
            }
            75..=99 if depth > 0 => {
                // a counted loop: the body cannot assign the counter. It CAN assign a variable bound (and so diverge); the concrete runs have fuel.
                let c = self.free_var();
                self.locked.push(c);
                let bound = if self.rng.chance(60) {
                    num(self.rng.range(2, 20))
                } else {
                    var(self.rng.below(2) as usize)
                };
                out.push(self.b.assign(c, num(0)));
                let mut body = self.block(depth - 1);
                let inc = self.b.bin(Op::Add, var(c), num(1));
                body.push(self.b.assign(c, inc));
                out.push(Stmt::While(cond(Cmp::Lt, var(c), bound), body));
                self.locked.pop();
            }
            _ => {
                let (v, e) = (self.free_var(), self.expr(1));
                out.push(self.b.assign(v, e));
            }
        }
    }
}
fn random_prog(rng: &mut Rng) -> Prog {
    let r = |rng: &mut Rng| {
        let lo = rng.range(-10, 10);
        (lo, lo + rng.range(0, 12))
    };
    let ranges = [r(rng), r(rng)];
    let mut g = Gen {
        rng,
        b: B::default(),
        locked: vec![],
    };
    let stmts = g.block(2).into_iter().chain(g.block(2)).collect();
    Prog { stmts, ranges }
}

// ---------------------------------------------------------------- verification

const FUEL: usize = 4000;
fn run_concrete(p: &Prog, rng: &mut Rng) -> Conc {
    let pick = |rng: &mut Rng, (lo, hi): (i64, i64)| match rng.below(6) {
        0 => lo,
        1 => hi,
        _ => rng.range(lo, hi),
    };
    let vars = [pick(rng, p.ranges[0]), pick(rng, p.ranges[1]), 0, 0];
    let mut c = Conc {
        vars,
        fuel: FUEL,
        obs: vec![],
        events: vec![],
    };
    c.exec(&p.stmts);
    assert!(c.fuel <= FUEL, "fuel only ever decreases");
    c
}
/// Every observed value is inside the inferred abstraction and every concrete error happened at an alarmed site.
fn check_sound<D: Dom>(_p: &Prog, an: &An<D>, runs: &[Conc]) {
    for r in runs {
        for (s, v) in &r.obs {
            let a = an.vals.get(s).unwrap_or_else(|| {
                panic!(
                    "{}: site {s} executed but analysis thinks it is unreachable",
                    D::name()
                )
            });
            assert!(
                a.contains(*v),
                "{}: site {s} observed {v} outside {a:?}",
                D::name()
            );
        }
        for ev in &r.events {
            assert!(
                an.alarms.contains(ev),
                "{}: concrete error {ev:?} at a site the analysis proved safe",
                D::name()
            );
        }
    }
}

/// Local soundness of every abstract operation on random finite sets (this is the Galois-connection obligation).
fn check_ops<D: Dom>(rng: &mut Rng) {
    let alpha = |s: &[i64]| s.iter().fold(D::bot(), |a, k| a.join(&D::constant(*k)));
    for _ in 0..3000 {
        let mk = |rng: &mut Rng| -> Vec<i64> {
            let big = rng.chance(15);
            (0..1 + rng.below(4))
                .map(|_| {
                    if big {
                        rng.range(I32MAX - 20, I32MAX) * [1, -1][rng.below(2) as usize]
                    } else {
                        rng.range(-12, 12)
                    }
                })
                .collect()
        };
        let (sa, sb) = (mk(rng), mk(rng));
        let (a, b) = (alpha(&sa), alpha(&sb));
        assert!(
            sa.iter().all(|k| a.contains(*k)) && sb.iter().all(|k| b.contains(*k)),
            "{}: S not inside gamma(alpha(S))",
            D::name()
        );
        for op in [Op::Add, Op::Sub, Op::Mul, Op::Div] {
            let (r, ovf) = D::binop(op, &a, &b);
            for x in &sa {
                for y in &sb {
                    let (v, o, _dz) = concrete_op(op, *x, *y);
                    assert!(
                        r.contains(v),
                        "{}: {op:?}({x},{y}) = {v} not in {r:?} (from {a:?}, {b:?})",
                        D::name()
                    );
                    assert!(
                        !o || ovf,
                        "{}: {op:?}({x},{y}) overflowed but flag is false",
                        D::name()
                    );
                }
            }
        }
        // guards: assume must keep every pair that satisfies the comparison
        for c in [Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge, Cmp::Eq, Cmp::Ne] {
            let (a2, b2) = D::assume(c, &a, &b);
            for x in &sa {
                for y in &sb {
                    if c.holds(*x, *y) {
                        assert!(
                            a2.contains(*x) && b2.contains(*y),
                            "{}: assume {c:?} dropped ({x},{y})",
                            D::name()
                        );
                    }
                }
            }
        }
        // widening is an upper bound of join
        assert!(a.join(&b).leq(&a.widen(&b, &[0, 10])));
    }
}

// ---------------------------------------------------------------- relational sketch: difference bound matrices

const INF: i64 = i64::MAX / 4;
/// m[i][j] = c means x_i - x_j <= c; variable 0 is the constant zero, so bounds are x_i - 0 <= c.
#[derive(Clone, PartialEq, Debug)]
struct Dbm {
    m: Vec<Vec<i64>>,
}
impl Dbm {
    fn new(n: usize) -> Dbm {
        let mut m = vec![vec![INF; n]; n];
        for (i, row) in m.iter_mut().enumerate() {
            row[i] = 0;
        }
        Dbm { m }
    }
    fn add(&mut self, i: usize, j: usize, c: i64) {
        self.m[i][j] = self.m[i][j].min(c);
    }
    /// Floyd-Warshall: shortest paths = tightest implied constraints. Returns false if a negative cycle (empty set) appears.
    fn close(&mut self) -> bool {
        let n = self.m.len();
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    if self.m[i][k] < INF && self.m[k][j] < INF {
                        self.m[i][j] = self.m[i][j].min(self.m[i][k] + self.m[k][j]);
                    }
                }
            }
        }
        (0..n).all(|i| self.m[i][i] >= 0)
    }
    fn forget(&mut self, x: usize) {
        self.close(); // keep what other variables imply THROUGH x before dropping it
        for j in 0..self.m.len() {
            if j != x {
                self.m[x][j] = INF;
                self.m[j][x] = INF;
            }
        }
    }
    fn assign_const(&mut self, x: usize, k: i64) {
        self.forget(x);
        self.add(x, 0, k);
        self.add(0, x, -k);
    }
    /// x = y + c
    fn assign_var(&mut self, x: usize, y: usize, c: i64) {
        if x == y {
            for j in 0..self.m.len() {
                if j != x {
                    if self.m[x][j] < INF {
                        self.m[x][j] += c;
                    }
                    if self.m[j][x] < INF {
                        self.m[j][x] -= c;
                    }
                }
            }
        } else {
            self.forget(x);
            self.add(x, y, c);
            self.add(y, x, -c);
        }
        self.close();
    }
    fn join(&self, o: &Dbm) -> Dbm {
        let (mut a, mut b) = (self.clone(), o.clone());
        a.close();
        b.close();
        Dbm {
            m: a.m
                .iter()
                .zip(&b.m)
                .map(|(r, s)| r.iter().zip(s).map(|(x, y)| *x.max(y)).collect())
                .collect(),
        }
    }
    /// Standard DBM widening: keep a constraint only if the new iterate did not weaken it. Never close the result (that can break termination).
    fn widen(&self, o: &Dbm) -> Dbm {
        let mut b = o.clone();
        b.close();
        Dbm {
            m: self
                .m
                .iter()
                .zip(&b.m)
                .map(|(r, s)| {
                    r.iter()
                        .zip(s)
                        .map(|(x, y)| if y <= x { *x } else { INF })
                        .collect()
                })
                .collect(),
        }
    }
    fn bounds(&self, x: usize) -> (i64, i64) {
        (
            if self.m[0][x] >= INF {
                I32MIN
            } else {
                -self.m[0][x]
            },
            if self.m[x][0] >= INF {
                I32MAX
            } else {
                self.m[x][0]
            },
        )
    }
}
fn dbm_demo() {
    // x in [0,10]; y = x; z = y - x. Intervals: z in [-10,10]. Zones: z - 0 <= 0 and 0 - z <= 0 (z is exactly 0).
    let (x, y) = (1, 2);
    let mut d = Dbm::new(3);
    d.add(x, 0, 10);
    d.add(0, x, 0);
    d.assign_var(y, x, 0);
    // intervals forget that y and x move together; the zone keeps both y - x <= 0 and x - y <= 0:
    let (ylo, yhi) = d.bounds(y);
    let itv = Itv::binop(Op::Sub, &Itv::R(ylo, yhi), &Itv::R(0, 10)).0;
    let rel = d.m[y][x].max(d.m[x][y]);
    println!(
        "  y = x with x in [0,10]: interval y - x = {itv:?}; zones know y - x <= {} and x - y <= {} (exactly 0)",
        d.m[y][x], d.m[x][y]
    );
    assert_eq!(itv, Itv::R(-10, 10));
    assert_eq!(rel, 0);

    // while (i < n) i = i + 1  with n in [0,100]; i starts at 0. Variables: 0 zero, 1 = i, 2 = n.
    let (i, n) = (1, 2);
    let mut init = Dbm::new(3);
    init.add(n, 0, 100);
    init.add(0, n, 0);
    init.assign_const(i, 0);
    assert!(init.close()); // the first widening operand must be closed; later ones are widening results and must NOT be
    let mut inv = init.clone();
    let mut iters = 0;
    loop {
        let mut body = inv.clone();
        body.add(i, n, -1); // guard i < n  (i - n <= -1)
        if !body.close() {
            break;
        }
        body.assign_var(i, i, 1);
        let next = inv.widen(&init.join(&body));
        iters += 1;
        if next == inv {
            break;
        }
        inv = next;
    }
    let mut exit = inv.clone();
    exit.add(n, i, 0); // exit guard i >= n  (n - i <= 0)
    assert!(exit.close());
    let (ilo, ihi) = exit.bounds(i);
    println!(
        "  loop `while (i < n) i = i + 1`, n in [0,100]: zones converge in {iters} widening steps; at exit i - n <= {} and n - i <= {} (i == n), so i in [{ilo},{ihi}]",
        exit.m[i][n], exit.m[n][i]
    );
    assert_eq!((exit.m[i][n], exit.m[n][i]), (0, 0));
    assert_eq!((ilo, ihi), (0, 100));
}

/// Closure is exact for integer difference constraints: compare with brute force over a small box.
fn dbm_closure_property(rng: &mut Rng) {
    let (mut sat_cases, mut unsat_cases) = (0, 0);
    for _ in 0..600 {
        let mut d = Dbm::new(4);
        for v in 1..4 {
            d.add(v, 0, 4);
            d.add(0, v, 4); // box [-4,4]
        }
        let mut cons = vec![];
        for _ in 0..rng.range(1, 7) {
            let (i, j) = (rng.below(4) as usize, rng.below(4) as usize);
            if i != j {
                let c = rng.range(-4, 4);
                d.add(i, j, c);
                cons.push((i, j, c));
            }
        }
        let feasible = d.clone().close();
        let mut best = vec![vec![i64::MIN; 4]; 4];
        let mut any = false;
        for a in -4..=4i64 {
            for b in -4..=4i64 {
                for c in -4..=4i64 {
                    let x = [0, a, b, c];
                    if d.m.iter().enumerate().all(|(i, r)| {
                        r.iter()
                            .enumerate()
                            .all(|(j, c)| *c >= INF || x[i] - x[j] <= *c)
                    }) {
                        any = true;
                        for i in 0..4 {
                            for j in 0..4 {
                                best[i][j] = best[i][j].max(x[i] - x[j]);
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(
            feasible, any,
            "closure feasibility disagrees with brute force for {cons:?}"
        );
        if feasible {
            let mut cl = d.clone();
            cl.close();
            assert_eq!(
                cl.m, best,
                "closed DBM must equal the exact implied bounds for {cons:?}"
            );
            sat_cases += 1;
        } else {
            unsat_cases += 1;
        }
    }
    println!(
        "  closure == brute force on {sat_cases} satisfiable and {unsat_cases} unsatisfiable random constraint sets"
    );
}

// ---------------------------------------------------------------- demos

fn demo_programs() -> Vec<(&'static str, Prog)> {
    let mut out = vec![];
    // 1. sum the table: i < 8 is safe, i <= 8 is not
    for (name, op) in [
        ("sum table[0..8) with i < 8", Cmp::Lt),
        ("sum table[0..=8] with i <= 8", Cmp::Le),
    ] {
        let mut b = B::default();
        let init_i = b.assign(2, num(0));
        let init_s = b.assign(3, num(0));
        let ld = b.load(1, var(2));
        let add = b.bin(Op::Add, var(3), var(1));
        let acc = b.assign(3, add);
        let inc = b.bin(Op::Add, var(2), num(1));
        let step = b.assign(2, inc);
        out.push((
            name,
            Prog {
                stmts: vec![
                    init_i,
                    init_s,
                    Stmt::While(cond(op, var(2), num(8)), vec![ld, acc, step]),
                ],
                ranges: [(0, 0), (0, 0)],
            },
        ));
    }
    // 2. divide by an odd number: only parity proves it
    let mut b = B::default();
    let m = b.bin(Op::Mul, var(0), num(2));
    let d = b.bin(Op::Add, m, num(1));
    let s1 = b.assign(2, d);
    let q = b.bin(Op::Div, num(100), var(2));
    let s2 = b.assign(3, q);
    out.push((
        "100 / (2*k + 1), k in [-5,5]",
        Prog {
            stmts: vec![s1, s2],
            ranges: [(-5, 5), (0, 0)],
        },
    ));
    // 3. product of inputs: depends on the input range
    for (name, hi) in [
        ("x * y with x,y in [0,1000]", 1000),
        ("x * y with x,y in [0,100000]", 100000),
    ] {
        let mut b = B::default();
        let p = b.bin(Op::Mul, var(0), var(1));
        let s = b.assign(2, p);
        out.push((
            name,
            Prog {
                stmts: vec![s],
                ranges: [(0, hi), (0, hi)],
            },
        ));
    }
    // 4. out[i] = i for i in 0..8 (store index proof needs the loop invariant)
    let mut b = B::default();
    let init = b.assign(2, num(0));
    let st = b.store(var(2), var(2));
    let inc = b.bin(Op::Add, var(2), num(1));
    let step = b.assign(2, inc);
    out.push((
        "out[i] = i, i in 0..8",
        Prog {
            stmts: vec![
                init,
                Stmt::While(cond(Cmp::Lt, var(2), num(8)), vec![st, step]),
            ],
            ranges: [(0, 0), (0, 0)],
        },
    ));
    out
}
fn loop_demo() {
    // i = 0; while (i < 100) i = i + 1
    let mut b = B::default();
    let init = b.assign(2, num(0));
    let inc = b.bin(Op::Add, var(2), num(1));
    let step = b.assign(2, inc);
    let p = Prog {
        stmts: vec![
            init,
            Stmt::While(cond(Cmp::Lt, var(2), num(100)), vec![step]),
        ],
        ranges: [(0, 0), (0, 0)],
    };
    println!("  i = 0; while (i < 100) i = i + 1     (value of i at loop exit)");
    for (label, th, narrow, widen) in [
        ("no widening (join only)", false, 0, false),
        ("widening to infinity    ", false, 0, true),
        ("widening + narrowing x2 ", false, 2, true),
        ("widening with thresholds", true, 0, true),
    ] {
        let (an, out) = analyze::<Itv>(&p, &make_config(&p, th, narrow, widen));
        let exit = out.map(|e| e[2]);
        println!(
            "    {label}: exit i = {exit:?}, fixpoint iterations {}, gave up (fuse) {}",
            an.iters, an.fuse_trips
        );
        if label.starts_with("no widening") {
            assert!(
                an.iters >= 100,
                "without widening the chain [0,0] < [0,1] < ... takes one step per value"
            );
        } else {
            assert_eq!(an.fuse_trips, 0);
        }
        if narrow > 0 || th {
            assert_eq!(exit, Some(Itv::R(100, 100)));
        }
    }
}
/// A diverging loop must stop for EVERY fuel budget, including those where the body consumes exactly the last unit.
fn fuel_regression() {
    let mut b = B::default();
    let body = vec![b.assign(2, num(1))];
    let diverge = [Stmt::While(cond(Cmp::Lt, num(0), num(1)), body)];
    for fuel in 0..20 {
        let mut c = Conc {
            vars: [0; NV],
            fuel,
            obs: vec![],
            events: vec![],
        };
        assert!(!c.exec(&diverge), "a diverging loop cannot finish");
        assert_eq!(c.fuel, 0);
    }
}
fn galois_demo() {
    println!(
        "  alpha({{1,2,5}}) = {:?}, alpha({{-3,4}}) = {:?} (sign mask), alpha({{2,4,10}}) = {:?} (parity mask)",
        [1, 2, 5]
            .iter()
            .fold(Itv::Bot, |a, k| a.join(&Itv::constant(*k))),
        [-3, 4]
            .iter()
            .fold(Sign::bot(), |a, k| a.join(&Sign::constant(*k))),
        [2, 4, 10]
            .iter()
            .fold(Par::bot(), |a, k| a.join(&Par::constant(*k)))
    );
    // gamma(alpha(S)) is bigger than S: that is the precision loss; alpha(gamma(a)) = a for these domains (a Galois INSERTION)
    let a = Itv::R(1, 5);
    let g: Vec<i64> = (-10..=10).filter(|v| a.contains(*v)).collect();
    let back = g.iter().fold(Itv::Bot, |x, k| x.join(&Itv::constant(*k)));
    assert_eq!(back, a);
    let r = Prod::range(1, 6).meet(&Prod(Itv::top(), Par(1)));
    println!(
        "  reduced product: [1,6] meet even = {r:?}  (interval tightened to [2,6]); singleton [4,4] with parity top gets parity {:?}",
        Prod(Itv::R(4, 4), Par::top()).reduce().1
    );
    assert_eq!(r.0, Itv::R(2, 6));
    assert_eq!(Prod(Itv::R(4, 4), Par::top()).reduce().1, Par(1));
}

fn main() {
    let mut rng = Rng(0xAB57_2AC7_0002_0821);
    println!("== Galois connection and the reduced product ==");
    galois_demo();
    check_ops::<Itv>(&mut rng);
    check_ops::<Sign>(&mut rng);
    check_ops::<Par>(&mut rng);
    check_ops::<Prod>(&mut rng);
    println!(
        "  local soundness (op, guards, widening bound) held for 4 domains x 3000 random set pairs"
    );

    fuel_regression();

    println!("\n== widening and narrowing on a counting loop ==");
    loop_demo();

    println!(
        "\n== hand-written programs: how many checks does each domain prove? (proven/total) =="
    );
    println!(
        "  {:<34} {:>10} {:>10} {:>10} {:>18}",
        "program", "interval", "sign", "parity", "interval x parity"
    );
    for (name, p) in demo_programs() {
        let cfg = make_config(&p, true, 2, true);
        let cell = |alarms: &HashSet<(usize, Chk)>| {
            let (s, t) = precision(&p, alarms);
            format!("{s}/{t}")
        };
        let (a, _) = analyze::<Itv>(&p, &cfg);
        let (s, _) = analyze::<Sign>(&p, &cfg);
        let (q, _) = analyze::<Par>(&p, &cfg);
        let (r, _) = analyze::<Prod>(&p, &cfg);
        println!(
            "  {:<34} {:>10} {:>10} {:>10} {:>18}",
            name,
            cell(&a.alarms),
            cell(&s.alarms),
            cell(&q.alarms),
            cell(&r.alarms)
        );
        // spot checks of the narrative
        let has = |an: &HashSet<(usize, Chk)>, c: Chk| an.iter().any(|(_, k)| *k == c);
        match name {
            "sum table[0..8) with i < 8" => {
                assert!(!has(&a.alarms, Chk::Oob), "intervals prove i in [0,7]")
            }
            "sum table[0..=8] with i <= 8" => {
                assert!(has(&a.alarms, Chk::Oob), "off-by-one must alarm")
            }
            "100 / (2*k + 1), k in [-5,5]" => {
                assert!(has(&a.alarms, Chk::Div), "interval [-9,11] contains 0");
                assert!(
                    !has(&q.alarms, Chk::Div) && !has(&r.alarms, Chk::Div),
                    "parity: 2k+1 is odd, never 0"
                );
            }
            "x * y with x,y in [0,1000]" => assert!(!has(&a.alarms, Chk::Ovf)),
            "x * y with x,y in [0,100000]" => assert!(has(&a.alarms, Chk::Ovf)),
            _ => {}
        }
    }

    println!("\n== relational sketch: difference bound matrices (zones) ==");
    dbm_demo();
    dbm_closure_property(&mut rng);

    println!(
        "\n== 2000 random programs: soundness against concrete runs, termination, precision =="
    );
    let configs: [(&str, bool, usize, bool); 2] = [
        ("thresholds+narrowing", true, 2, true),
        ("plain widening", false, 0, true),
    ];
    let mut safe = vec![[0usize; 2]; 6];
    let mut total = 0usize;
    let (mut obs_total, mut event_total, mut fuse_no_widen) = (0, 0, 0);
    let mut by_kind = [[0usize; 3]; 4]; // (domain) x (Ovf, Div, Oob) proven
    let mut kind_total = [0usize; 3];
    for _ in 0..2000 {
        let p = random_prog(&mut rng);
        let runs: Vec<Conc> = (0..4).map(|_| run_concrete(&p, &mut rng)).collect();
        obs_total += runs.iter().map(|r| r.obs.len()).sum::<usize>();
        event_total += runs.iter().map(|r| r.events.len()).sum::<usize>();
        let mut checks = vec![];
        all_checks(&p.stmts, &mut checks);
        total += checks.len();
        for (_, c) in &checks {
            kind_total[*c as usize] += 1;
        }
        for (ci, (_, th, nar, wid)) in configs.iter().enumerate() {
            let cfg = make_config(&p, *th, *nar, *wid);
            let (an, _) = analyze::<Itv>(&p, &cfg);
            assert_eq!(
                an.fuse_trips, 0,
                "widening must terminate without needing the fuse"
            );
            check_sound(&p, &an, &runs);
            safe[ci][0] += precision(&p, &an.alarms).0;
            if ci == 0 {
                for (s, c) in &checks {
                    if !an.alarms.contains(&(*s, *c)) {
                        by_kind[0][*c as usize] += 1;
                    }
                }
            }
        }
        let cfg = make_config(&p, true, 2, true);
        let (an, _) = analyze::<Sign>(&p, &cfg);
        assert_eq!(an.fuse_trips, 0);
        check_sound(&p, &an, &runs);
        safe[2][0] += precision(&p, &an.alarms).0;
        for (s, c) in &checks {
            if !an.alarms.contains(&(*s, *c)) {
                by_kind[1][*c as usize] += 1;
            }
        }
        let (an, _) = analyze::<Par>(&p, &cfg);
        assert_eq!(an.fuse_trips, 0);
        check_sound(&p, &an, &runs);
        safe[3][0] += precision(&p, &an.alarms).0;
        for (s, c) in &checks {
            if !an.alarms.contains(&(*s, *c)) {
                by_kind[2][*c as usize] += 1;
            }
        }
        let (an, _) = analyze::<Prod>(&p, &cfg);
        assert_eq!(an.fuse_trips, 0);
        check_sound(&p, &an, &runs);
        safe[4][0] += precision(&p, &an.alarms).0;
        for (s, c) in &checks {
            if !an.alarms.contains(&(*s, *c)) {
                by_kind[3][*c as usize] += 1;
            }
        }
        // without widening, loops with large bounds do not converge: count how often the fuse trips (soundly)
        let (an, _) = analyze::<Itv>(&p, &make_config(&p, false, 0, false));
        fuse_no_widen += an.fuse_trips;
        check_sound(&p, &an, &runs);
    }
    println!(
        "  {obs_total} observed values were all inside the inferred abstractions; {event_total} concrete run-time errors all happened at alarmed sites"
    );
    println!(
        "  widening terminated on all 2000 programs (fuse never tripped); without widening the fuse tripped {fuse_no_widen} times (and stayed sound)"
    );
    println!(
        "\n  checks proven safe out of {total} (ovf {}, div {}, oob {}):",
        kind_total[0], kind_total[1], kind_total[2]
    );
    println!(
        "  {:<24} {:>8} {:>8} {:>8} {:>8}",
        "domain", "total", "overflow", "div0", "bounds"
    );
    let names = [
        "interval (thr+narrow)",
        "sign",
        "parity",
        "interval x parity",
    ];
    println!(
        "  {:<24} {:>8} {:>8} {:>8} {:>8}",
        "interval (plain widen)", safe[1][0], "-", "-", "-"
    );
    for (i, n) in names.iter().enumerate() {
        println!(
            "  {:<24} {:>8} {:>8} {:>8} {:>8}",
            n,
            safe[[0, 2, 3, 4][i]][0],
            by_kind[i][0],
            by_kind[i][1],
            by_kind[i][2]
        );
    }
    // precision ordering that must hold by construction
    assert!(
        safe[0][0] >= safe[1][0],
        "narrowing and thresholds cannot lose checks"
    );
    assert!(
        safe[4][0] >= safe[0][0],
        "the reduced product is at least as precise as intervals"
    );
    assert!(safe[4][0] >= safe[3][0]);
    println!("\nall assertions passed");
}
