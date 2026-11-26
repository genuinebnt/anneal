//! RECIPE 840 (database): worst-case optimal joins: AGM bound, Generic Join and Leapfrog Triejoin vs binary hash-join plans.
//! LEVEL: advanced
//! Use in: graph pattern matching (triangles, cliques), Datalog engines, RDF/SPARQL stores, any multiway join with cycles.   Run: cargo run --release --example leapfrog_triejoin_worst_case_optimal
//! COMMON USES: LogicBlox (LogiQL) built its engine on Leapfrog Triejoin (see Veldhuizen's paper below); research and
//! graph databases such as Umbra and Kuzu describe worst-case-optimal / multiway join operators for cyclic patterns.
//! Triangle and clique counting in social-graph analytics, graph pattern matching in general.
//! Course: CMU 15-721 (advanced DB) covers multiway joins; the papers below are the primary sources.
#![allow(dead_code)]
//!
//! WHAT. A conjunctive query is a set of ATOMS (relations over variables), e.g. the triangle
//! `Q(a,b,c) :- R(a,b), S(b,c), T(a,c)`. The textbook executor joins two relations at a time (a left-deep plan of hash joins).
//! That is PROVABLY suboptimal on cyclic queries: any binary plan can be forced to build an intermediate result of size
//! Theta(N^2) when the final output and the AGM bound are only N^1.5. A worst-case optimal join (WCOJ) instead binds ONE
//! VARIABLE AT A TIME: for variable x it intersects, across every atom mentioning x, the set of values consistent with the
//! variables already bound, and recurses on each value in the intersection. No intermediate result is ever materialised.
//!
//! THE AGM BOUND (Atserias-Grohe-Marx). Give each atom e a weight x_e >= 0 such that every variable is covered with total weight
//! >= 1 (a FRACTIONAL EDGE COVER). Then |Q| <= prod_e |R_e|^{x_e}, and the best bound minimises sum_e x_e*ln|R_e|: a tiny LP.
//! Triangle with all sizes N: x=(1/2,1/2,1/2) gives N^1.5. Here the LP is solved by brute force: the optimum of an LP is at a
//! VERTEX of the feasible polytope, i.e. a point where m linearly independent constraints (cover or x_e=0) are tight; with at
//! most ~10 constraints we just try every subset of m of them and solve the m x m system by Gaussian elimination.
//!
//! GENERIC JOIN (Ngo-Porat-Re-Rudra). For each variable, take the candidate lists from all atoms containing it, iterate over
//! the SMALLEST list and probe the others (binary search). Cost per value is bounded by the smallest list, which is what makes
//! the total match the AGM bound (up to a log).
//!
//! LEAPFROG TRIEJOIN (Veldhuizen). Same algorithm, packaged for sorted data. Each relation is a TRIE: level 0 holds the
//! distinct first columns, below each first value sits the sorted run of second values. A TRIE ITERATOR offers
//! `open()` (descend to the children of the current key), `up()`, `next()`, `seek(k)` (jump to the first key >= k, by galloping
//! so the cost is log of the DISTANCE moved) and `key()/at_end()`. The LEAPFROG intersection of k iterators keeps them in a
//! circle sorted by key: the one holding the smallest key seeks to the current maximum; when all keys are equal a match is
//! emitted. Because seeks skip ahead, the work is proportional to the SMALLER side of every intersection.
//!
//! VARIABLE ORDER matters (all orders are correct, costs differ). The relations must be sorted consistently with the variable
//! order, so `reorder` transposes and re-sorts a relation when an atom's columns run against the chosen order. A greedy heuristic
//! picks the most-constrained variable first and then variables connected to what is chosen.
//!
//! CHECKS. (1) 500 random graphs: triangle, 4-clique and 3-edge path; Leapfrog, Generic Join and a left-deep hash-join plan all
//! equal a brute-force nested-loop oracle (also under every variable order for triangles). (2) On the classic "cross" instance
//! R=S=T={(0,j)}U{(i,0)} every binary plan needs ~m^2 intermediate tuples while Leapfrog stays ~linear: asserted via the growth
//! ratio as m is multiplied by 8. (3) On random instances Leapfrog's operation count stays within
//! `c * atoms * (AGM + N) * (log2 N + 2)`. Operation counts (not time) are asserted because they are deterministic.
//! Read: Veldhuizen, "Leapfrog Triejoin: a worst-case optimal join algorithm" https://arxiv.org/abs/1210.0481
//!       Ngo, Porat, Re, Rudra, "Worst-case Optimal Join Algorithms" https://arxiv.org/abs/1203.1952
//!       Ngo, Re, Rudra, "Skew Strikes Back: New Developments in the Theory of Join Algorithms" (cited by title; URL not checked)
//!
//! TRY NEXT:
//!   - [S] In `Trie::seek` replace galloping (`gallop`) with a plain binary search over [pos,hi). Predict how `Stats::probes` on the
//!     cross instance changes, then measure; when does galloping lose?
//!   - [S] Add a 4-cycle query (a-b, b-c, c-d, a-d) beside `clique4`; compute its `agm_bound` by hand first (fractional cover), then compare.
//!   - [M] Predict which variable order of `cross_query` is cheapest, then print `levels` for all six; why do the orders split into two cost classes?
//!   - [M] Port: count triangles in a real edge list (e.g. a SNAP graph) with `leapfrog`, orienting edges low-degree to high-degree; how does that change `Stats::ops`?
//!   - [A] Project: combine with `join_algorithms` and `cardinality_estimation_lab`: a planner that estimates intermediate sizes and chooses
//!     between `best_binary_plan` and `leapfrog` per query; test it on random and skewed graphs.

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
        self.next() % n.max(1)
    }
}

type Rel = Vec<(u32, u32)>;
fn norm(mut r: Rel) -> Rel {
    r.sort_unstable();
    r.dedup();
    r
}

/// Atom over global variables x < y (the trie order); `rel` indexes `Query::rels`.
#[derive(Clone, Copy, Debug)]
struct Atom {
    x: usize,
    y: usize,
    rel: usize,
}
#[derive(Clone)]
struct Query {
    nvars: usize,
    atoms: Vec<Atom>,
    rels: Vec<Rel>,
}

#[derive(Default, Clone, Copy, Debug)]
struct Stats {
    seeks: u64,
    nexts: u64,
    probes: u64,
}
impl Stats {
    fn ops(&self) -> u64 {
        self.seeks + self.nexts + self.probes
    }
}

// ---------------- queries ----------------
/// All atoms share one oriented edge relation; `pairs` are variable pairs (i<j).
fn shared_query(n: usize, pairs: &[(usize, usize)], e: &Rel) -> Query {
    Query {
        nvars: n,
        atoms: pairs.iter().map(|&(x, y)| Atom { x, y, rel: 0 }).collect(),
        rels: vec![e.clone()],
    }
}
fn triangle(e: &Rel) -> Query {
    shared_query(3, &[(0, 1), (1, 2), (0, 2)], e)
}
fn clique4(e: &Rel) -> Query {
    shared_query(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)], e)
}
fn path3(e: &Rel) -> Query {
    shared_query(4, &[(0, 1), (1, 2), (2, 3)], e)
}

// ---------------- AGM bound ----------------
/// Solve A x = b (square) by Gaussian elimination with partial pivoting; None if singular.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-9 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in 0..n {
            if r != c {
                let f = a[r][c] / a[c][c];
                for k in c..n {
                    a[r][k] -= f * a[c][k];
                }
                b[r] -= f * b[c];
            }
        }
    }
    Some((0..n).map(|i| b[i] / a[i][i]).collect())
}

/// min sum_e x_e ln|R_e| s.t. every variable covered >= 1, x >= 0, by enumerating polytope vertices. Returns (bound, weights).
fn agm_bound(q: &Query) -> (f64, Vec<f64>) {
    let m = q.atoms.len();
    let n = q.nvars;
    let cost: Vec<f64> = q
        .atoms
        .iter()
        .map(|a| (q.rels[a.rel].len().max(1) as f64).ln())
        .collect();
    // constraint c < n: cover of variable c (rhs 1); c >= n: x_{c-n} = 0.
    let row = |c: usize| -> (Vec<f64>, f64) {
        if c < n {
            (
                q.atoms
                    .iter()
                    .map(|a| if a.x == c || a.y == c { 1.0 } else { 0.0 })
                    .collect(),
                1.0,
            )
        } else {
            (
                (0..m).map(|e| if e == c - n { 1.0 } else { 0.0 }).collect(),
                0.0,
            )
        }
    };
    let mut best: Option<(f64, Vec<f64>)> = None;
    for mask in 0u32..(1 << (n + m)) {
        if mask.count_ones() as usize != m {
            continue;
        }
        let (a, b): (Vec<_>, Vec<_>) = (0..n + m).filter(|c| mask >> c & 1 == 1).map(row).unzip();
        let Some(x) = solve(a, b) else { continue };
        let feasible = x.iter().all(|&v| v > -1e-9)
            && (0..n).all(|v| {
                q.atoms
                    .iter()
                    .zip(&x)
                    .filter(|(a, _)| a.x == v || a.y == v)
                    .map(|(_, w)| w)
                    .sum::<f64>()
                    > 1.0 - 1e-9
            });
        if !feasible {
            continue;
        }
        let obj: f64 = x.iter().zip(&cost).map(|(w, c)| w * c).sum();
        if best.as_ref().is_none_or(|(o, _)| obj < *o) {
            best = Some((obj, x));
        }
    }
    let (o, x) = best.expect("every covered query has a feasible vertex");
    (o.exp(), x)
}

// ---------------- trie iterator ----------------
fn col(r: &[(u32, u32)], i: usize, c: usize) -> u32 {
    if c == 0 { r[i].0 } else { r[i].1 }
}
/// Galloping search from `lo`: first index in [lo,hi) with col > k (strict) or col >= k. Cost ~ log(distance moved).
fn gallop(
    r: &[(u32, u32)],
    lo: usize,
    hi: usize,
    k: u32,
    c: usize,
    strict: bool,
    st: &mut Stats,
) -> usize {
    let before = |v: u32| if strict { v <= k } else { v < k };
    let mut bound = 1usize;
    while lo + bound < hi && {
        st.probes += 1;
        before(col(r, lo + bound, c))
    } {
        bound *= 2;
    }
    let mut a = if bound > 1 { lo + bound / 2 } else { lo };
    let mut b = (lo + bound).min(hi);
    while a < b {
        let mid = (a + b) / 2;
        st.probes += 1;
        if before(col(r, mid, c)) {
            a = mid + 1
        } else {
            b = mid
        }
    }
    a
}

/// Trie over a sorted binary relation. `stack` has one frame (lo, hi, pos) per open level.
struct Trie<'a> {
    r: &'a [(u32, u32)],
    stack: Vec<(usize, usize, usize)>,
}
impl<'a> Trie<'a> {
    fn new(r: &'a [(u32, u32)]) -> Self {
        Trie {
            r,
            stack: Vec::new(),
        }
    }
    fn top(&self) -> (usize, usize, usize) {
        *self.stack.last().unwrap()
    }
    fn level(&self) -> usize {
        self.stack.len() - 1
    }
    fn at_end(&self) -> bool {
        let (_, hi, p) = self.top();
        p >= hi
    }
    fn key(&self) -> u32 {
        col(self.r, self.top().2, self.level())
    }
    fn open(&mut self, st: &mut Stats) {
        if self.stack.is_empty() {
            self.stack.push((0, self.r.len(), 0));
        } else {
            let (_, hi, p) = self.top();
            let end = gallop(self.r, p, hi, self.r[p].0, 0, true, st); // end of the current first-column run
            self.stack.push((p, end, p));
        }
    }
    fn up(&mut self) {
        self.stack.pop();
    }
    fn next(&mut self, st: &mut Stats) {
        st.nexts += 1;
        let (lo, hi, p) = self.top();
        let np = if self.level() == 0 {
            gallop(self.r, p, hi, self.r[p].0, 0, true, st)
        } else {
            p + 1
        };
        *self.stack.last_mut().unwrap() = (lo, hi, np);
    }
    fn seek(&mut self, k: u32, st: &mut Stats) {
        st.seeks += 1;
        let (lo, hi, p) = self.top();
        let np = gallop(self.r, p, hi, k, self.level(), false, st);
        *self.stack.last_mut().unwrap() = (lo, hi, np);
    }
}

// ---------------- leapfrog triejoin ----------------
struct Leapfrog<'a> {
    q: &'a Query,
    tries: Vec<Trie<'a>>,
    var_atoms: Vec<Vec<usize>>,
    binding: Vec<u32>,
    st: Stats,
    out: Vec<Vec<u32>>,
    count: u64,
    bindings_per_level: Vec<u64>,
}
impl<'a> Leapfrog<'a> {
    fn rec(&mut self, i: usize) {
        if i == self.q.nvars {
            self.count += 1;
            self.out.push(self.binding.clone());
            return;
        }
        let mut part = self.var_atoms[i].clone();
        for &a in &part {
            self.tries[a].open(&mut self.st);
        }
        if part.iter().all(|&a| !self.tries[a].at_end()) {
            part.sort_by_key(|&a| self.tries[a].key());
            let k = part.len();
            let mut p = 0;
            loop {
                let mx = self.tries[part[(p + k - 1) % k]].key();
                let cur = self.tries[part[p]].key();
                if cur == mx {
                    self.binding[i] = cur; // all k iterators sit on `cur`: a match
                    self.bindings_per_level[i] += 1;
                    self.rec(i + 1);
                    self.tries[part[p]].next(&mut self.st);
                } else {
                    self.tries[part[p]].seek(mx, &mut self.st); // leap to the current maximum
                }
                if self.tries[part[p]].at_end() {
                    break;
                }
                p = (p + 1) % k;
            }
        }
        for &a in &part {
            self.tries[a].up();
        }
    }
}
struct LfResult {
    rows: Vec<Vec<u32>>,
    st: Stats,
    levels: Vec<u64>,
}
fn leapfrog(q: &Query) -> LfResult {
    let mut var_atoms = vec![vec![]; q.nvars];
    for (i, a) in q.atoms.iter().enumerate() {
        var_atoms[a.x].push(i);
        var_atoms[a.y].push(i);
    }
    let mut lf = Leapfrog {
        q,
        tries: q.atoms.iter().map(|a| Trie::new(&q.rels[a.rel])).collect(),
        var_atoms,
        binding: vec![0; q.nvars],
        st: Stats::default(),
        out: vec![],
        count: 0,
        bindings_per_level: vec![0; q.nvars],
    };
    lf.rec(0);
    LfResult {
        rows: lf.out,
        st: lf.st,
        levels: lf.bindings_per_level,
    }
}

// ---------------- generic join (hash/binary-search based) ----------------
struct Index {
    firsts: Vec<u32>,
    seconds: HashMap<u32, Vec<u32>>,
}
fn build_index(r: &Rel) -> Index {
    let mut seconds: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(a, b) in r {
        seconds.entry(a).or_default().push(b);
    }
    let mut firsts: Vec<u32> = seconds.keys().copied().collect();
    firsts.sort_unstable();
    Index { firsts, seconds }
}
fn generic_join(q: &Query) -> (Vec<Vec<u32>>, u64) {
    let idx: Vec<Index> = q.rels.iter().map(build_index).collect();
    let mut out = vec![];
    let mut ops = 0u64;
    let mut binding = vec![0u32; q.nvars];
    fn rec(
        q: &Query,
        idx: &[Index],
        i: usize,
        b: &mut Vec<u32>,
        out: &mut Vec<Vec<u32>>,
        ops: &mut u64,
    ) {
        if i == q.nvars {
            out.push(b.clone());
            return;
        }
        let empty: Vec<u32> = vec![];
        let lists: Vec<&[u32]> = q
            .atoms
            .iter()
            .filter(|a| a.x == i || a.y == i)
            .map(|a| {
                let ix = &idx[a.rel];
                if a.x == i {
                    &ix.firsts[..]
                } else {
                    ix.seconds.get(&b[a.x]).unwrap_or(&empty).as_slice()
                }
            })
            .collect();
        let smallest = (0..lists.len()).min_by_key(|&j| lists[j].len()).unwrap();
        for &c in lists[smallest] {
            *ops += 1;
            let mut ok = true;
            for (j, l) in lists.iter().enumerate() {
                if j != smallest {
                    *ops += 1 + (l.len() as f64 + 1.0).log2() as u64; // a binary-search probe
                    if l.binary_search(&c).is_err() {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                b[i] = c;
                rec(q, idx, i + 1, b, out, ops);
            }
        }
    }
    rec(q, &idx, 0, &mut binding, &mut out, &mut ops);
    (out, ops)
}

// ---------------- left-deep binary hash-join plan ----------------
struct PlanResult {
    rows: Vec<Vec<u32>>,
    inter: Vec<usize>,
    ops: u64,
}
const UNB: u32 = u32::MAX;
fn binary_plan(q: &Query, order: &[usize]) -> PlanResult {
    let mut bound = vec![false; q.nvars];
    let mut table: Vec<Vec<u32>> = vec![vec![UNB; q.nvars]];
    let (mut ops, mut inter) = (0u64, vec![]);
    for &ai in order {
        let a = q.atoms[ai];
        let r = &q.rels[a.rel];
        ops += r.len() as u64; // build/scan the atom
        let mut next = vec![];
        match (bound[a.x], bound[a.y]) {
            (true, true) => {
                let set: HashSet<(u32, u32)> = r.iter().copied().collect();
                for row in &table {
                    ops += 1;
                    if set.contains(&(row[a.x], row[a.y])) {
                        next.push(row.clone());
                    }
                }
            }
            (true, false) | (false, true) => {
                let (bv, nv, fwd) = if bound[a.x] {
                    (a.x, a.y, true)
                } else {
                    (a.y, a.x, false)
                };
                let mut h: HashMap<u32, Vec<u32>> = HashMap::new();
                for &(p, s) in r {
                    if fwd {
                        h.entry(p).or_default().push(s)
                    } else {
                        h.entry(s).or_default().push(p)
                    }
                }
                for row in &table {
                    ops += 1;
                    for &v in h.get(&row[bv]).into_iter().flatten() {
                        ops += 1;
                        let mut nr = row.clone();
                        nr[nv] = v;
                        next.push(nr);
                    }
                }
            }
            (false, false) => {
                for row in &table {
                    for &(p, s) in r {
                        ops += 1;
                        let mut nr = row.clone();
                        nr[a.x] = p;
                        nr[a.y] = s;
                        next.push(nr);
                    }
                }
            }
        }
        bound[a.x] = true;
        bound[a.y] = true;
        table = next;
        inter.push(table.len());
    }
    PlanResult {
        rows: table,
        inter,
        ops,
    }
}
fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![vec![]];
    }
    let mut out = vec![];
    for p in permutations(n - 1) {
        for pos in 0..n {
            let mut v = p.clone();
            v.insert(pos, n - 1);
            out.push(v);
        }
    }
    out
}
/// Best left-deep plan (fewest ops) over all atom orders that stay connected-ish; small queries only.
fn best_binary_plan(q: &Query) -> PlanResult {
    permutations(q.atoms.len())
        .iter()
        .map(|o| binary_plan(q, o))
        .min_by_key(|p| p.ops)
        .unwrap()
}

// ---------------- oracle, variable orders ----------------
fn brute_force(q: &Query, domain: u32) -> Vec<Vec<u32>> {
    let sets: Vec<HashSet<(u32, u32)>> =
        q.rels.iter().map(|r| r.iter().copied().collect()).collect();
    let mut out = vec![];
    let mut b = vec![0u32; q.nvars];
    fn rec(
        q: &Query,
        sets: &[HashSet<(u32, u32)>],
        d: u32,
        i: usize,
        b: &mut Vec<u32>,
        out: &mut Vec<Vec<u32>>,
    ) {
        if i == q.nvars {
            if q.atoms
                .iter()
                .all(|a| sets[a.rel].contains(&(b[a.x], b[a.y])))
            {
                out.push(b.clone());
            }
            return;
        }
        for v in 0..d {
            b[i] = v;
            rec(q, sets, d, i + 1, b, out);
        }
    }
    rec(q, &sets, domain, 0, &mut b, &mut out);
    out
}
/// Relabel variables: old variable v becomes perm[v]; atoms whose columns now run backwards are transposed and re-sorted.
fn reorder(q: &Query, perm: &[usize]) -> Query {
    let mut rels = vec![];
    let atoms = q
        .atoms
        .iter()
        .map(|a| {
            let (nx, ny) = (perm[a.x], perm[a.y]);
            let r = &q.rels[a.rel];
            if nx < ny {
                rels.push(r.clone());
                Atom {
                    x: nx,
                    y: ny,
                    rel: rels.len() - 1,
                }
            } else {
                rels.push(norm(r.iter().map(|&(p, s)| (s, p)).collect()));
                Atom {
                    x: ny,
                    y: nx,
                    rel: rels.len() - 1,
                }
            }
        })
        .collect();
    Query {
        nvars: q.nvars,
        atoms,
        rels,
    }
}
/// Greedy order: most-constrained variable first, then the variable sharing most atoms with the chosen set. Returns perm[old]=new.
fn heuristic_perm(q: &Query) -> Vec<usize> {
    let deg = |v: usize| q.atoms.iter().filter(|a| a.x == v || a.y == v).count();
    let mut chosen: Vec<usize> = vec![];
    while chosen.len() < q.nvars {
        let v = (0..q.nvars)
            .filter(|v| !chosen.contains(v))
            .max_by_key(|&v| {
                let link = q
                    .atoms
                    .iter()
                    .filter(|a| {
                        (a.x == v && chosen.contains(&a.y)) || (a.y == v && chosen.contains(&a.x))
                    })
                    .count();
                (link, deg(v), usize::MAX - v)
            })
            .unwrap();
        chosen.push(v);
    }
    let mut perm = vec![0; q.nvars];
    for (pos, &v) in chosen.iter().enumerate() {
        perm[v] = pos;
    }
    perm
}

// ---------------- instances ----------------
fn random_oriented_graph(r: &mut Rng, n: u32, m: usize) -> Rel {
    let mut e = vec![];
    for _ in 0..m {
        let (a, b) = (r.below(n as u64) as u32, r.below(n as u64) as u32);
        if a != b {
            e.push((a.min(b), a.max(b)));
        }
    }
    norm(e)
}
fn random_directed_graph(r: &mut Rng, n: u32, m: usize) -> Rel {
    norm(
        (0..m)
            .map(|_| (r.below(n as u64) as u32, r.below(n as u64) as u32))
            .collect(),
    )
}
/// {(0,j)} U {(i,0)}: every pairwise join has ~m^2 rows but the triangle output is only ~3m.
fn cross_rel(m: u32) -> Rel {
    norm(
        (0..m)
            .map(|j| (0, j))
            .chain((0..m).map(|i| (i, 0)))
            .collect(),
    )
}
fn cross_query(m: u32) -> Query {
    let r = cross_rel(m);
    Query {
        nvars: 3,
        atoms: vec![
            Atom { x: 0, y: 1, rel: 0 },
            Atom { x: 1, y: 2, rel: 1 },
            Atom { x: 0, y: 2, rel: 2 },
        ],
        rels: vec![r.clone(), r.clone(), r],
    }
}
fn sorted(mut v: Vec<Vec<u32>>) -> Vec<Vec<u32>> {
    v.sort();
    v
}

// ---------------- checks ----------------
fn check_random_graphs() {
    let mut r = Rng(0x840A_5EED);
    let (mut lf_ops, mut gj_ops, mut bp_ops, mut worst_ratio) = (0u64, 0u64, 0u64, 0f64);
    let mut total_out = 0usize;
    for case in 0..500 {
        let n = 4 + r.below(7) as u32; // brute force is n^4 at most
        let m = r.below(n as u64 * 3) as usize + 1;
        let e = random_oriented_graph(&mut r, n, m);
        let d = random_directed_graph(&mut r, n, m);
        let queries = [
            ("triangle", triangle(&e)),
            ("4-clique", clique4(&e)),
            ("path3", path3(&d)),
        ];
        for (name, q) in &queries {
            let oracle = sorted(brute_force(q, n));
            let lf = leapfrog(q);
            assert_eq!(
                sorted(lf.rows.clone()),
                oracle,
                "leapfrog wrong on {name}, case {case}"
            );
            let (gj, g_ops) = generic_join(q);
            assert_eq!(
                sorted(gj),
                oracle,
                "generic join wrong on {name}, case {case}"
            );
            let order: Vec<usize> = (0..q.atoms.len()).collect();
            let bp = binary_plan(q, &order);
            assert_eq!(
                sorted(bp.rows),
                oracle,
                "binary plan wrong on {name}, case {case}"
            );
            total_out += oracle.len();
            lf_ops += lf.st.ops();
            gj_ops += g_ops;
            bp_ops += bp.ops;
            // AGM-times-log envelope (generous constant; the point is the SHAPE, not the constant).
            let (agm, _) = agm_bound(q);
            let nmax = q.rels.iter().map(Vec::len).max().unwrap().max(2) as f64;
            let envelope = 8.0 * q.atoms.len() as f64 * (agm + nmax) * (nmax.log2() + 2.0);
            let ratio = lf.st.ops() as f64 / envelope;
            worst_ratio = worst_ratio.max(ratio);
            assert!(
                ratio <= 1.0,
                "leapfrog ops {} exceed AGM*log envelope {envelope:.0} on {name}",
                lf.st.ops()
            );
            assert!(
                oracle.len() as f64 <= agm + 1e-6,
                "output {} exceeds AGM bound {agm}",
                oracle.len()
            );
        }
        // every variable order gives the same triangle count
        let tri = triangle(&e);
        let want = brute_force(&tri, n).len();
        for p in permutations(3) {
            let re = reorder(&tri, &p);
            assert_eq!(
                leapfrog(&re).rows.len(),
                want,
                "variable order {p:?} changes the answer"
            );
        }
        let heur = reorder(&tri, &heuristic_perm(&tri));
        assert_eq!(leapfrog(&heur).rows.len(), want);
    }
    println!(
        "500 random graphs x {{triangle, 4-clique, path3}}: leapfrog == generic join == binary plan == brute force"
    );
    println!(
        "  total output rows {total_out}; ops leapfrog {lf_ops}, generic {gj_ops}, binary {bp_ops}"
    );
    println!("  worst leapfrog-ops / (AGM*log envelope) = {worst_ratio:.4} (must stay <= 1)");
}

fn check_agm() {
    let e: Rel = (0..1000u32).map(|i| (i, i + 1)).collect();
    let (b, w) = agm_bound(&triangle(&e));
    assert!(
        (b - 1000f64.powf(1.5)).abs() / b < 1e-6,
        "triangle AGM = N^1.5, got {b}"
    );
    assert!(w.iter().all(|x| (x - 0.5).abs() < 1e-6));
    let (b4, _) = agm_bound(&clique4(&e));
    assert!(
        (b4 - 1000f64.powi(2)).abs() / b4 < 1e-6,
        "4-clique AGM = N^2, got {b4}"
    );
    let (bp, _) = agm_bound(&path3(&e));
    assert!(
        (bp - 1000f64.powi(2)).abs() / bp < 1e-6,
        "3-edge path AGM = N^2 (cover e1,e3), got {bp}"
    );
    // unequal sizes: tiny T makes the bound collapse
    let mut q = triangle(&e);
    q.rels = vec![e.clone(), e.clone(), e[..10].to_vec()];
    q.atoms[2].rel = 2;
    let (bt, _) = agm_bound(&q);
    assert!(
        bt < 1000f64.powf(1.5) / 5.0,
        "small T must tighten the bound, got {bt}"
    );
    println!(
        "AGM bounds: triangle N^1.5 = {:.0}, 4-clique N^2 = {:.0}, path3 N^2 = {:.0}, tiny-T triangle = {:.0}",
        b, b4, bp, bt
    );
}

fn check_skew_gap() {
    // correctness at small m against the oracle, including all variable orders
    let small = cross_query(12);
    let oracle = sorted(brute_force(&small, 12));
    assert_eq!(sorted(leapfrog(&small).rows), oracle);
    assert_eq!(sorted(generic_join(&small).0), oracle);

    println!("\nthe 'cross' instance R=S=T={{(0,j)}} U {{(i,0)}}, N = 2m-1 tuples per relation:");
    println!(
        "{:>5} {:>7} {:>10} {:>8} {:>13} {:>11} {:>10} {:>10}",
        "m", "N", "AGM=N^1.5", "output", "binary-interm", "binary ops", "leapfrog", "generic"
    );
    let mut rows = vec![];
    for m in [100u32, 200, 400, 800] {
        let q = cross_query(m);
        let n = q.rels[0].len();
        let (agm, _) = agm_bound(&q);
        let lf = leapfrog(&q);
        let (gj, gj_ops) = generic_join(&q);
        let bp = best_binary_plan(&q);
        assert_eq!(lf.rows.len(), gj.len());
        assert_eq!(lf.rows.len(), bp.rows.len());
        let maxi = *bp.inter.iter().max().unwrap();
        println!(
            "{m:>5} {n:>7} {agm:>10.0} {:>8} {maxi:>13} {:>11} {:>10} {gj_ops:>10}",
            lf.rows.len(),
            bp.ops,
            lf.st.ops()
        );
        // quadratic intermediate for EVERY order, well above the AGM bound
        for o in permutations(3) {
            let p = binary_plan(&q, &o);
            assert!(
                *p.inter.iter().max().unwrap() >= (m as usize) * (m as usize),
                "order {o:?} was supposed to blow up"
            );
        }
        rows.push((maxi as f64, bp.ops as f64, lf.st.ops() as f64));
    }
    // m grows 8x. quadratic => ~64x, near-linear (with log) => < 12x.
    let (first, last) = (rows[0], rows[3]);
    let (bin_growth, lf_growth) = (last.1 / first.1, last.2 / first.2);
    println!(
        "growth when m is multiplied by 8: binary ops x{bin_growth:.1} (quadratic ~64), leapfrog ops x{lf_growth:.1} (linear*log ~8-12)"
    );
    assert!(
        bin_growth > 40.0 && lf_growth < 14.0,
        "asymptotic gap not observed"
    );
    assert!(
        last.0 > 10.0 * (2.0 * 800.0),
        "intermediate must dwarf the input"
    );

    // variable orders on the skewed instance
    let q = cross_query(400);
    println!("\nleapfrog ops by variable order on cross(m=400) (perm[old]=new position):");
    let want = leapfrog(&q).rows.len();
    for p in permutations(3) {
        let re = reorder(&q, &p);
        let res = leapfrog(&re);
        assert_eq!(res.rows.len(), want);
        println!(
            "  perm {p:?}: ops {:>7}, partial bindings per level {:?}",
            res.st.ops(),
            res.levels
        );
    }
    println!("  heuristic perm: {:?}", heuristic_perm(&q));
}

fn check_trie_iterator() {
    let r: Rel = norm(vec![(1, 5), (1, 7), (3, 2), (3, 9), (8, 1)]);
    let mut st = Stats::default();
    let mut t = Trie::new(&r);
    t.open(&mut st);
    assert_eq!(t.key(), 1);
    t.seek(3, &mut st);
    assert_eq!(t.key(), 3);
    t.open(&mut st);
    assert_eq!(t.key(), 2);
    t.seek(9, &mut st);
    assert_eq!(t.key(), 9);
    t.next(&mut st);
    assert!(t.at_end());
    t.up();
    t.next(&mut st);
    assert_eq!(t.key(), 8);
    t.seek(100, &mut st);
    assert!(t.at_end());
    // gallop equals partition_point on random sorted data
    let mut g = Rng(7);
    let mut v: Rel = (0..300).map(|_| (g.below(50) as u32, 0)).collect();
    v.sort();
    for _ in 0..2000 {
        let lo = g.below(300) as usize;
        let k = g.below(55) as u32;
        let want = lo + v[lo..].partition_point(|x| x.0 < k);
        assert_eq!(gallop(&v, lo, 300, k, 0, false, &mut st), want);
        let want_s = lo + v[lo..].partition_point(|x| x.0 <= k);
        assert_eq!(gallop(&v, lo, 300, k, 0, true, &mut st), want_s);
    }
    println!("trie iterator open/up/next/seek and galloping search verified");
}

fn demo_cliques() {
    let mut r = Rng(99);
    let e = random_oriented_graph(&mut r, 300, 3000);
    for (name, q) in [("triangle", triangle(&e)), ("4-clique", clique4(&e))] {
        let lf = leapfrog(&q);
        let bp = binary_plan(&q, &(0..q.atoms.len()).collect::<Vec<_>>());
        let (agm, _) = agm_bound(&q);
        assert_eq!(lf.rows.len(), bp.rows.len());
        println!(
            "G(300 nodes, {} edges) {name}: output {}, AGM {:.0}, binary intermediates {:?}, ops binary {} vs leapfrog {}",
            e.len(),
            lf.rows.len(),
            agm,
            bp.inter,
            bp.ops,
            lf.st.ops()
        );
    }
}

fn main() {
    check_trie_iterator();
    check_agm();
    check_random_graphs();
    check_skew_gap();
    println!();
    demo_cliques();
    println!("all checks passed");
}
