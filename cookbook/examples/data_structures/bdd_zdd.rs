//! RECIPE 273 (data-structures): reduced ordered BDDs (Bryant) and ZDDs (Minato): canonical boolean functions and families of sets.
//! LEVEL: advanced
//! Use in: equivalence checking, model counting, representing huge set families compactly.   Run: cargo run --release --example bdd_zdd
#![allow(dead_code)]
//!
//! A BDD is a decision DAG: node (var, lo, hi) means "if var then hi else lo". Two rules make it CANONICAL for a fixed
//! variable order: (1) never create a node with lo == hi, (2) never create two identical nodes. Rule 2 is enforced by a
//! UNIQUE TABLE (hash-consing: `mk` looks up (var, lo, hi) before allocating), so two formulas are equivalent
//! exactly when they compile to the SAME node index: equivalence checking is `==` on a u32. All boolean operators come
//! from one memoised `ite(f, g, h)` (if-then-else); the cache makes `apply` polynomial in the sizes of its arguments.
//!
//! The variable ORDER decides everything: x1y1 + x2y2 + ... + xnyn is 2n+2 nodes with x_i next to y_i and about 2^(n+1)
//! with all x before all y; the carry-out of an n-bit adder behaves the same way. Finding the best order is NP-hard.
//!
//! A ZDD (Minato, TAOCP 7.1.4) keeps the same machinery but swaps rule 1 for ZERO-SUPPRESSION: drop a node whose hi
//! child is the empty family. That makes ZDDs ideal for families of SETS over a big universe where most items are
//! absent from most sets: union / intersection / difference / join (product) and counting all work directly on the
//! compressed form. Examples verified below: independent sets of paths and cycles (Fibonacci, Lucas) and N-queens.
//!
//! Verification: 2000 random formulas over <= 8 variables are compiled to BDDs and compared with truth tables for
//! evaluation, model count, restriction and existential quantification, and De Morgan-rewritten twins must hash to the
//! identical node.
//!
//! COMMON USES: symbolic model checking and hardware equivalence checking (the original motivation for BDDs), static
//! analysis with BDD-based Datalog/points-to engines, configuration/feature-model counting (product lines), logic
//! synthesis, fault-tree analysis, combinatorial counting via ZDDs (frontier-based path/matching enumeration).
//! Read: Knuth, The Art of Computer Programming Vol 4A, section 7.1.4 (Binary Decision Diagrams, including ZDDs);
//! Bryant, "Graph-Based Algorithms for Boolean Function Manipulation" (IEEE Trans. Computers, 1986).
//! https://en.wikipedia.org/wiki/Binary_decision_diagram  https://www-cs-faculty.stanford.edu/~knuth/taocp.html

use std::collections::{HashMap, HashSet};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ================================================================== BDD
const FALSE: u32 = 0;
const TRUE: u32 = 1;

struct Bdd {
    nv: u32,
    nodes: Vec<(u32, u32, u32)>, // (var, lo, hi); terminals have var = nv
    unique: HashMap<(u32, u32, u32), u32>,
    ite_cache: HashMap<(u32, u32, u32), u32>,
}

impl Bdd {
    fn new(nv: u32) -> Self {
        Bdd {
            nv,
            nodes: vec![(nv, 0, 0), (nv, 1, 1)],
            unique: HashMap::new(),
            ite_cache: HashMap::new(),
        }
    }
    fn top(&self, f: u32) -> u32 {
        self.nodes[f as usize].0
    }
    /// The only way to create a node: enforces both reduction rules.
    fn mk(&mut self, var: u32, lo: u32, hi: u32) -> u32 {
        if lo == hi {
            return lo;
        }
        if let Some(&id) = self.unique.get(&(var, lo, hi)) {
            return id;
        }
        let id = self.nodes.len() as u32;
        self.nodes.push((var, lo, hi));
        self.unique.insert((var, lo, hi), id);
        id
    }
    fn var(&mut self, v: u32) -> u32 {
        self.mk(v, FALSE, TRUE)
    }
    fn cofactors(&self, f: u32, v: u32) -> (u32, u32) {
        let (fv, lo, hi) = self.nodes[f as usize];
        if fv == v { (lo, hi) } else { (f, f) }
    }
    fn ite(&mut self, f: u32, g: u32, h: u32) -> u32 {
        if f == TRUE {
            return g;
        }
        if f == FALSE || g == h {
            return if f == FALSE { h } else { g };
        }
        if g == TRUE && h == FALSE {
            return f;
        }
        if let Some(&r) = self.ite_cache.get(&(f, g, h)) {
            return r;
        }
        let v = self.top(f).min(self.top(g)).min(self.top(h));
        let (f0, f1) = self.cofactors(f, v);
        let (g0, g1) = self.cofactors(g, v);
        let (h0, h1) = self.cofactors(h, v);
        let lo = self.ite(f0, g0, h0);
        let hi = self.ite(f1, g1, h1);
        let r = self.mk(v, lo, hi);
        self.ite_cache.insert((f, g, h), r);
        r
    }
    fn not(&mut self, f: u32) -> u32 {
        self.ite(f, FALSE, TRUE)
    }
    fn and(&mut self, f: u32, g: u32) -> u32 {
        self.ite(f, g, FALSE)
    }
    fn or(&mut self, f: u32, g: u32) -> u32 {
        self.ite(f, TRUE, g)
    }
    fn xor(&mut self, f: u32, g: u32) -> u32 {
        let ng = self.not(g);
        self.ite(f, ng, g)
    }
    fn eval(&self, mut f: u32, assign: u32) -> bool {
        while f > TRUE {
            let (v, lo, hi) = self.nodes[f as usize];
            f = if assign >> v & 1 == 1 { hi } else { lo };
        }
        f == TRUE
    }
    /// Number of satisfying assignments over all `nv` variables.
    fn satcount(&self, f: u32) -> u128 {
        fn rec(b: &Bdd, f: u32, memo: &mut HashMap<u32, u128>) -> u128 {
            if f <= TRUE {
                return f as u128;
            }
            if let Some(&c) = memo.get(&f) {
                return c;
            }
            let (v, lo, hi) = b.nodes[f as usize];
            let c = (rec(b, lo, memo) << (b.top(lo) - v - 1))
                + (rec(b, hi, memo) << (b.top(hi) - v - 1));
            memo.insert(f, c);
            c
        }
        rec(self, f, &mut HashMap::new()) << self.top(f)
    }
    /// All satisfying CUBES (None = variable unconstrained on that path).
    fn cubes(&self, f: u32) -> Vec<Vec<Option<bool>>> {
        fn rec(b: &Bdd, f: u32, cur: &mut Vec<Option<bool>>, out: &mut Vec<Vec<Option<bool>>>) {
            if f == FALSE {
                return;
            }
            if f == TRUE {
                out.push(cur.clone());
                return;
            }
            let (v, lo, hi) = b.nodes[f as usize];
            cur[v as usize] = Some(false);
            rec(b, lo, cur, out);
            cur[v as usize] = Some(true);
            rec(b, hi, cur, out);
            cur[v as usize] = None;
        }
        let mut out = vec![];
        rec(self, f, &mut vec![None; self.nv as usize], &mut out);
        out
    }
    fn restrict(&mut self, f: u32, v: u32, val: bool) -> u32 {
        fn rec(b: &mut Bdd, f: u32, v: u32, val: bool, memo: &mut HashMap<u32, u32>) -> u32 {
            let (fv, lo, hi) = b.nodes[f as usize];
            if fv > v {
                return f;
            }
            if fv == v {
                return if val { hi } else { lo };
            }
            if let Some(&r) = memo.get(&f) {
                return r;
            }
            let (l, h) = (rec(b, lo, v, val, memo), rec(b, hi, v, val, memo));
            let r = b.mk(fv, l, h);
            memo.insert(f, r);
            r
        }
        rec(self, f, v, val, &mut HashMap::new())
    }
    fn exists(&mut self, f: u32, v: u32) -> u32 {
        let (a, b) = (self.restrict(f, v, false), self.restrict(f, v, true));
        self.or(a, b)
    }
    fn size(&self, f: u32) -> usize {
        let mut seen = HashSet::new();
        let mut stack = vec![f];
        while let Some(x) = stack.pop() {
            if seen.insert(x) && x > TRUE {
                stack.push(self.nodes[x as usize].1);
                stack.push(self.nodes[x as usize].2);
            }
        }
        seen.len()
    }
}

#[derive(Clone, Debug)]
enum Expr {
    Var(u32),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Xor(Box<Expr>, Box<Expr>),
}

fn random_expr(rng: &mut Rng, nv: u32, depth: u32) -> Expr {
    if depth == 0 || rng.below(6) == 0 {
        return Expr::Var(rng.below(nv as u64) as u32);
    }
    let sub = |r: &mut Rng| Box::new(random_expr(r, nv, depth - 1));
    match rng.below(4) {
        0 => Expr::Not(sub(rng)),
        1 => Expr::And(sub(rng), sub(rng)),
        2 => Expr::Or(sub(rng), sub(rng)),
        _ => Expr::Xor(sub(rng), sub(rng)),
    }
}

fn eval_expr(e: &Expr, a: u32) -> bool {
    match e {
        Expr::Var(v) => a >> v & 1 == 1,
        Expr::Not(x) => !eval_expr(x, a),
        Expr::And(x, y) => eval_expr(x, a) && eval_expr(y, a),
        Expr::Or(x, y) => eval_expr(x, a) || eval_expr(y, a),
        Expr::Xor(x, y) => eval_expr(x, a) ^ eval_expr(y, a),
    }
}

fn compile(b: &mut Bdd, e: &Expr) -> u32 {
    match e {
        Expr::Var(v) => b.var(*v),
        Expr::Not(x) => {
            let x = compile(b, x);
            b.not(x)
        }
        Expr::And(x, y) => {
            let (x, y) = (compile(b, x), compile(b, y));
            b.and(x, y)
        }
        Expr::Or(x, y) => {
            let (x, y) = (compile(b, x), compile(b, y));
            b.or(x, y)
        }
        Expr::Xor(x, y) => {
            let (x, y) = (compile(b, x), compile(b, y));
            b.xor(x, y)
        }
    }
}

/// Structurally different but logically equal: AND via De Morgan, XOR via (a & !b) | (!a & b).
fn de_morgan(e: &Expr) -> Expr {
    use Expr::*;
    let n = |x: Expr| Not(Box::new(x));
    match e {
        Var(v) => Var(*v),
        Not(x) => n(de_morgan(x)),
        And(x, y) => n(Or(Box::new(n(de_morgan(x))), Box::new(n(de_morgan(y))))),
        Or(x, y) => Or(Box::new(de_morgan(x)), Box::new(de_morgan(y))),
        Xor(x, y) => {
            let (a, b) = (de_morgan(x), de_morgan(y));
            Or(
                Box::new(And(Box::new(a.clone()), Box::new(n(b.clone())))),
                Box::new(And(Box::new(n(a)), Box::new(b))),
            )
        }
    }
}

/// x_i y_i sum-of-products under a given variable order: `pos(is_y, i)` gives the level.
fn sum_of_products(n: u32, pos: impl Fn(bool, u32) -> u32) -> (usize, u128) {
    let mut b = Bdd::new(2 * n);
    let mut f = FALSE;
    for i in 0..n {
        let (x, y) = (b.var(pos(false, i)), b.var(pos(true, i)));
        let t = b.and(x, y);
        f = b.or(f, t);
    }
    (b.size(f), b.satcount(f))
}

/// Carry-out of an n-bit ripple adder under a given order (carry = majority).
fn adder_carry(n: u32, pos: impl Fn(bool, u32) -> u32) -> usize {
    let mut b = Bdd::new(2 * n);
    let mut c = FALSE;
    for i in 0..n {
        let (x, y) = (b.var(pos(false, i)), b.var(pos(true, i)));
        let (xy, xc, yc) = (b.and(x, y), b.and(x, c), b.and(y, c));
        let t = b.or(xy, xc);
        c = b.or(t, yc);
    }
    b.size(c)
}

// ================================================================== ZDD
struct Zdd {
    nodes: Vec<(u32, u32, u32)>,
    unique: HashMap<(u32, u32, u32), u32>,
    cache: HashMap<(u8, u32, u32), u32>,
}
const EMPTY: u32 = 0; // the empty family {}
const UNIT: u32 = 1; // the family {{}} containing only the empty set
const TERM: u32 = u32::MAX;

impl Zdd {
    fn new() -> Self {
        Zdd {
            nodes: vec![(TERM, 0, 0), (TERM, 1, 1)],
            unique: HashMap::new(),
            cache: HashMap::new(),
        }
    }
    /// Zero-suppression: a node whose hi child is the empty family is replaced by its lo child.
    fn mk(&mut self, v: u32, lo: u32, hi: u32) -> u32 {
        if hi == EMPTY {
            return lo;
        }
        if let Some(&id) = self.unique.get(&(v, lo, hi)) {
            return id;
        }
        let id = self.nodes.len() as u32;
        self.nodes.push((v, lo, hi));
        self.unique.insert((v, lo, hi), id);
        id
    }
    fn single(&mut self, v: u32) -> u32 {
        self.mk(v, EMPTY, UNIT)
    }
    fn power_set(&mut self, n: u32) -> u32 {
        let mut f = UNIT;
        for v in (0..n).rev() {
            f = self.mk(v, f, f);
        }
        f
    }
    fn var_of(&self, f: u32) -> u32 {
        self.nodes[f as usize].0
    }
    fn memo(&mut self, op: u8, p: u32, q: u32, compute: impl FnOnce(&mut Self) -> u32) -> u32 {
        if let Some(&r) = self.cache.get(&(op, p, q)) {
            return r;
        }
        let r = compute(self);
        self.cache.insert((op, p, q), r);
        r
    }
    fn union(&mut self, p: u32, q: u32) -> u32 {
        if p == EMPTY || p == q {
            return q;
        }
        if q == EMPTY {
            return p;
        }
        let (p, q) = (p.min(q), p.max(q)); // commutative: normalise cache key
        self.memo(0, p, q, |z| {
            let ((vp, pl, ph), (vq, ql, qh)) = (z.nodes[p as usize], z.nodes[q as usize]);
            if vp < vq {
                let lo = z.union(pl, q);
                z.mk(vp, lo, ph)
            } else if vp > vq {
                let lo = z.union(p, ql);
                z.mk(vq, lo, qh)
            } else {
                let (lo, hi) = (z.union(pl, ql), z.union(ph, qh));
                z.mk(vp, lo, hi)
            }
        })
    }
    fn intersect(&mut self, p: u32, q: u32) -> u32 {
        if p == EMPTY || q == EMPTY {
            return EMPTY;
        }
        if p == q {
            return p;
        }
        let (p, q) = (p.min(q), p.max(q));
        self.memo(1, p, q, |z| {
            let ((vp, pl, ph), (vq, ql, qh)) = (z.nodes[p as usize], z.nodes[q as usize]);
            if vp < vq {
                z.intersect(pl, q)
            } else if vp > vq {
                z.intersect(p, ql)
            } else {
                let (lo, hi) = (z.intersect(pl, ql), z.intersect(ph, qh));
                z.mk(vp, lo, hi)
            }
        })
    }
    fn diff(&mut self, p: u32, q: u32) -> u32 {
        if p == EMPTY || p == q {
            return EMPTY;
        }
        if q == EMPTY {
            return p;
        }
        self.memo(2, p, q, |z| {
            let ((vp, pl, ph), (vq, ql, qh)) = (z.nodes[p as usize], z.nodes[q as usize]);
            if vp < vq {
                let lo = z.diff(pl, q);
                z.mk(vp, lo, ph)
            } else if vp > vq {
                z.diff(p, ql)
            } else {
                let (lo, hi) = (z.diff(pl, ql), z.diff(ph, qh));
                z.mk(vp, lo, hi)
            }
        })
    }
    /// Join: { a | b : a in p, b in q }.
    fn product(&mut self, p: u32, q: u32) -> u32 {
        if p == EMPTY || q == EMPTY {
            return EMPTY;
        }
        if p == UNIT {
            return q;
        }
        if q == UNIT {
            return p;
        }
        let (p, q) = (p.min(q), p.max(q));
        self.memo(3, p, q, |z| {
            let ((vp, pl, ph), (vq, ql, qh)) = (z.nodes[p as usize], z.nodes[q as usize]);
            if vp < vq {
                let (lo, hi) = (z.product(pl, q), z.product(ph, q));
                z.mk(vp, lo, hi)
            } else if vp > vq {
                let (lo, hi) = (z.product(p, ql), z.product(p, qh));
                z.mk(vq, lo, hi)
            } else {
                let lo = z.product(pl, ql);
                let (a, b, c) = (z.product(ph, qh), z.product(ph, ql), z.product(pl, qh));
                let ab = z.union(a, b);
                let hi = z.union(ab, c);
                z.mk(vp, lo, hi)
            }
        })
    }
    fn count(&self, f: u32) -> u128 {
        fn rec(z: &Zdd, f: u32, memo: &mut HashMap<u32, u128>) -> u128 {
            if f <= UNIT {
                return f as u128;
            }
            if let Some(&c) = memo.get(&f) {
                return c;
            }
            let c = rec(z, z.nodes[f as usize].1, memo) + rec(z, z.nodes[f as usize].2, memo);
            memo.insert(f, c);
            c
        }
        rec(self, f, &mut HashMap::new())
    }
    fn sets(&self, f: u32) -> Vec<Vec<u32>> {
        match f {
            EMPTY => vec![],
            UNIT => vec![vec![]],
            _ => {
                let (v, lo, hi) = self.nodes[f as usize];
                let mut out = self.sets(lo);
                for mut s in self.sets(hi) {
                    s.insert(0, v);
                    out.push(s);
                }
                out
            }
        }
    }
    fn size(&self, f: u32) -> usize {
        let mut seen = HashSet::new();
        let mut stack = vec![f];
        while let Some(x) = stack.pop() {
            if seen.insert(x) && x > UNIT {
                stack.push(self.nodes[x as usize].1);
                stack.push(self.nodes[x as usize].2);
            }
        }
        seen.len()
    }
}

/// Family of all independent sets of a graph: start from the power set and remove, per edge, every superset of that edge.
fn independent_sets(z: &mut Zdd, n: u32, edges: &[(u32, u32)]) -> u32 {
    let all = z.power_set(n);
    let mut f = all;
    for &(u, v) in edges {
        let (a, b) = (z.single(u), z.single(v));
        let uv = z.product(a, b);
        let supersets = z.product(uv, all);
        f = z.diff(f, supersets);
    }
    f
}

fn queens_zdd(n: u32) -> (u128, usize) {
    let mut z = Zdd::new();
    let v = |r: u32, c: u32| r * n + c;
    let mut f = UNIT;
    for r in 0..n {
        let mut row = EMPTY;
        for c in 0..n {
            let s = z.single(v(r, c));
            row = z.union(row, s);
        }
        f = z.product(f, row); // pick exactly one square in this row
        let all = z.power_set(n * n);
        for r2 in 0..r {
            for c2 in 0..n {
                for c in 0..n {
                    if c2 == c || r.abs_diff(r2) == c.abs_diff(c2) {
                        let (a, b) = (z.single(v(r2, c2)), z.single(v(r, c)));
                        let pair = z.product(a, b);
                        let bad = z.product(pair, all);
                        f = z.diff(f, bad);
                    }
                }
            }
        }
    }
    (z.count(f), z.size(f))
}

fn main() {
    // ---- BDD basics: canonicity
    let mut b = Bdd::new(3);
    let (x, y, z) = (b.var(0), b.var(1), b.var(2));
    let lhs = {
        let t = b.and(x, y);
        b.or(t, z)
    };
    let rhs = {
        let (a, c) = (b.or(x, z), b.or(y, z));
        b.and(a, c)
    };
    assert_eq!(lhs, rhs, "(x&y)|z == (x|z)&(y|z) is pointer equality");
    let other = b.and(x, z);
    assert_ne!(lhs, other);
    assert_eq!(b.satcount(lhs), 5);
    let cubes = b.cubes(lhs);
    let covered: u128 = cubes
        .iter()
        .map(|c| 1u128 << c.iter().filter(|x| x.is_none()).count())
        .sum();
    assert_eq!(covered, 5, "cubes partition the models");
    println!(
        "[bdd] (x&y)|z: {} nodes, {} models, cubes {:?}",
        b.size(lhs),
        b.satcount(lhs),
        cubes
    );

    // ---- ordering effects
    println!(
        "[ordering] nodes of x1y1+..+xnyn and adder carry-out: interleaved vs separated order"
    );
    println!("    n   sop-interleaved  sop-separated   carry-interleaved  carry-separated");
    let mut prev_sep = 0;
    for n in [2u32, 4, 6, 8, 10] {
        let inter = |is_y: bool, i: u32| 2 * i + is_y as u32;
        let sep = |is_y: bool, i: u32| i + if is_y { n } else { 0 };
        let (si, c1) = sum_of_products(n, inter);
        let (ss, c2) = sum_of_products(n, sep);
        assert_eq!(c1, c2, "same function, same model count");
        assert_eq!(si, 2 * n as usize + 2);
        let (ci, cs) = (adder_carry(n, inter), adder_carry(n, sep));
        println!("   {n:2}   {si:14}  {ss:13}   {ci:16}  {cs:15}");
        assert!(ss > prev_sep);
        prev_sep = ss;
        if n == 10 {
            assert!(ss > 20 * si && cs > 10 * ci, "separated order blows up");
        }
    }

    // ---- equivalence of two adder formulations (generate/propagate vs majority)
    let n = 6;
    let mut b = Bdd::new(2 * n);
    let (mut c1, mut c2) = (FALSE, FALSE);
    for i in 0..n {
        let (x, y) = (b.var(2 * i), b.var(2 * i + 1));
        let (xy, xc, yc) = (b.and(x, y), b.and(x, c1), b.and(y, c1));
        let t = b.or(xy, xc);
        let s1 = {
            let t = b.xor(x, y);
            b.xor(t, c1)
        };
        c1 = b.or(t, yc);
        let (g, p) = (b.and(x, y), b.xor(x, y));
        let pc = b.and(p, c2);
        let s2 = {
            let t = b.xor(x, y);
            b.xor(t, c2)
        };
        c2 = b.or(g, pc);
        assert_eq!(s1, s2, "sum bit {i} identical");
    }
    assert_eq!(c1, c2);
    println!(
        "[equivalence] 6-bit adder: majority-carry and generate/propagate carry-out are the same node {c1}"
    );

    // ---- randomised verification against truth tables
    let mut rng = Rng(0xbdd_0123_4567_89ab);
    let mut total_nodes = 0;
    for _ in 0..2000 {
        let nv = 2 + rng.below(7) as u32; // 2..=8
        let e = random_expr(&mut rng, nv, 5);
        let mut b = Bdd::new(nv);
        let f = compile(&mut b, &e);
        let mut models = 0u128;
        for a in 0..1u32 << nv {
            let truth = eval_expr(&e, a);
            assert_eq!(b.eval(f, a), truth);
            models += truth as u128;
        }
        assert_eq!(b.satcount(f), models);
        let g = compile(&mut b, &de_morgan(&e));
        assert_eq!(f, g, "canonical form: equivalent formulas share one node");
        let v = rng.below(nv as u64) as u32;
        let (r0, r1, ex) = (
            b.restrict(f, v, false),
            b.restrict(f, v, true),
            b.exists(f, v),
        );
        for a in 0..1u32 << nv {
            let (a0, a1) = (a & !(1 << v), a | 1 << v);
            assert_eq!(b.eval(r0, a), eval_expr(&e, a0));
            assert_eq!(b.eval(r1, a), eval_expr(&e, a1));
            assert_eq!(b.eval(ex, a), eval_expr(&e, a0) || eval_expr(&e, a1));
        }
        total_nodes += b.nodes.len();
    }
    println!(
        "[verify] 2000 random formulas agree with truth tables (eval, satcount, restrict, exists, De Morgan twin); {total_nodes} nodes built"
    );

    // ---- ZDD basics
    let mut z = Zdd::new();
    let (a, bb, c) = (z.single(0), z.single(1), z.single(2));
    let f1 = {
        let t = z.union(a, bb);
        z.union(t, UNIT)
    }; // {{}, {0}, {1}}
    let f2 = {
        let t = z.union(bb, c);
        z.union(t, UNIT)
    }; // {{}, {1}, {2}}
    let (u, i, d, p) = (
        z.union(f1, f2),
        z.intersect(f1, f2),
        z.diff(f1, f2),
        z.product(f1, f2),
    );
    assert_eq!((z.count(u), z.count(i), z.count(d)), (4, 2, 1));
    assert_eq!(z.sets(d), vec![vec![0]]);
    assert_eq!(z.sets(p).len() as u128, z.count(p));
    println!(
        "[zdd] union {:?} | product has {} sets",
        z.sets(u),
        z.count(p)
    );

    // ---- independent sets: Fibonacci for paths, Lucas for cycles
    let fib = |n: u32| {
        let (mut a, mut b) = (1u128, 2u128); // F(2)=1, F(3)=2 : path with 0 or 1 vertices
        for _ in 0..n {
            let t = a + b;
            a = b;
            b = t;
        }
        a
    };
    let lucas = |n: u32| {
        let (mut a, mut b) = (2u128, 1u128);
        for _ in 0..n {
            let t = a + b;
            a = b;
            b = t;
        }
        a
    };
    let mut z = Zdd::new();
    for n in 1..=24u32 {
        let path: Vec<(u32, u32)> = (0..n - 1).map(|i| (i, i + 1)).collect();
        let f = independent_sets(&mut z, n, &path);
        assert_eq!(z.count(f), fib(n), "path P{n}");
        if n <= 12 {
            let brute = (0u32..1 << n).filter(|m| m & (m >> 1) == 0).count() as u128;
            assert_eq!(z.count(f), brute);
        }
        if n >= 3 {
            let mut cyc = path.clone();
            cyc.push((n - 1, 0));
            let g = independent_sets(&mut z, n, &cyc);
            assert_eq!(z.count(g), lucas(n), "cycle C{n}");
        }
    }
    let path20: Vec<(u32, u32)> = (0..19).map(|i| (i, i + 1)).collect();
    let f = independent_sets(&mut z, 20, &path20);
    println!(
        "[zdd] independent sets: P20 = {} (Fibonacci), ZDD has only {} nodes; C20 = {} (Lucas)",
        z.count(f),
        z.size(f),
        lucas(20)
    );

    // ---- N-queens by ZDD
    let want = [1u128, 0, 0, 2, 10, 4, 40, 92];
    for n in 1..=8u32 {
        let (count, size) = queens_zdd(n);
        assert_eq!(count, want[n as usize - 1], "queens({n})");
        if n == 8 {
            println!(
                "[zdd] N-queens counts 1..8 verified; the 8-queens family of 92 solutions is a {size}-node ZDD"
            );
        }
    }
    println!("all BDD/ZDD checks passed");
}
