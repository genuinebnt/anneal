//! RECIPE 841 (database): incremental view maintenance with z-sets and DBSP: I, D, z^-1, bilinear join, distinct, aggregates, recursion.
//! LEVEL: advanced
//! Use in: materialised views, streaming SQL, incremental Datalog, any "recompute only what changed" engine.   Run: cargo run --release --example incremental_view_maintenance_dbsp
//! COMMON USES: Feldera (the DBSP implementation), Materialize and Differential Dataflow (arrangements + deltas), RisingWave and
//! Flink incremental aggregation, database materialised views (Postgres extensions such as pg_ivm, DuckDB/ClickHouse incremental
//! views), build systems and spreadsheets re-evaluating only dirty cells.
//! Course: CMU 15-721 (streaming / materialised views); the DBSP paper below.
#![allow(dead_code)]
//!
//! WHAT. A DATABASE CHANGE is a Z-SET: a map row -> integer weight, where +1 is an insert, -1 a delete, and weights may be larger
//! (duplicates) or cancel to zero (the row disappears). A table is the SUM of all the change batches seen so far; a view is a
//! function of tables. DBSP treats a view definition as a circuit over STREAMS of z-sets and derives its incremental version
//! mechanically:
//!   * I (integrate) is the running sum  I(s)[t] = sum_{u<=t} s[u];  D (differentiate) is  D(s)[t] = s[t] - s[t-1].  D and I are
//!     inverses. Both are built from the one-step DELAY z^-1 (output the previous input): I is the feedback loop
//!     `out = in + z^-1(out)`.
//!   * The incremental version of a query Q is  Q^Δ = D ∘ Q ∘ I : integrate the deltas into full tables, run Q, differentiate
//!     the result. The magic is that for most operators the composition simplifies:
//!       - LINEAR operators (map, filter, project, union, negation, sum) satisfy Q(a+b)=Q(a)+Q(b), so Q^Δ = Q: just apply
//!         the operator to the delta. Cost is O(|delta|), not O(|table|).
//!       - A BILINEAR operator (join) satisfies  Δ(A ⋈ B) = ΔA ⋈ B + A ⋈ ΔB + ΔA ⋈ ΔB  where A and B are the OLD integrated tables.
//!         The three terms are computed with an index on each side's accumulated state.
//!       - DISTINCT is non-linear (weight>0 -> 1). Keep the integrated weights; for each changed row compare "was positive" with "is
//!         positive" and emit +1/-1 only on a flip. This is why z-sets carry multiplicities: a delete of one of two duplicates must
//!         NOT remove the row (the COUNTING PITFALL, demonstrated below against a naive set-based delta).
//!       - GROUP-BY COUNT/SUM are linear in the weights (count = sum w, sum = sum w*v) so per-group state is updated by deltas; the
//!         output delta is "retract the old aggregate row, assert the new one". MIN/MAX are NOT incremental under deletes without
//!         keeping the whole multiset (a deleted minimum needs the runner-up).
//!       - TOP-K keeps the full ordered group set, not just K rows: after deleting a top-K member the (K+1)th must be promoted
//!         (second pitfall demonstrated).
//!       - RECURSION (transitive closure) uses a NESTED time dimension: inside one outer step the delta of P = E + P ⋈ E is
//!         computed by SEMI-NAIVE iteration, ΔP = ΔE + P_old ⋈ ΔE + ΔP ⋈ E_new, i.e. D_0 = ΔE + P_old ⋈ ΔE and D_{i+1} = D_i ⋈ E_new,
//!         ΔP = sum D_i, the inner iteration counter being the inner timestamp. On a DAG the path COUNTS are finite so deletes work
//!         by plain subtraction; a cyclic graph never converges (third pitfall: the iteration cap fires). Real DBSP puts DISTINCT
//!         inside the loop so cyclic graphs converge; path counts also grow fast, so i64 here is only safe on tiny DAGs/forests.
//!
//! THE VIEWS (all maintained together): J = R(a,b) ⋈ S(b,c) filtered by (a+c)%3 != 0 and projected to (a,c);
//!   V1 = DISTINCT(J), V2 = per-a (COUNT, SUM c), V3 = TOP-3 groups of V2 by sum, V4 = reachability over an edge table E (DAG).
//!
//! CHECKS. (1) Stream algebra laws: D(I(s))=s, I(D(s))=s, feedback integrator == prefix sums, Q^Δ = Q for map/filter,
//! bilinear join rule and incremental distinct equal D∘Q∘I on random streams. (2) 500 random update sequences (inserts, deletes,
//! duplicates, 30 steps each): after EVERY step the incrementally maintained views equal both a from-scratch z-set evaluation and
//! an independent nested-loop / DFS oracle, and no maintained weight is ever negative. (3) The three pitfalls fail as described.
//! (4) Work (rows touched, deterministic) for incremental vs recompute is printed per batch size and asserted to be >20x
//! smaller for small deltas on a 3000-row workload.
//! Read: Budiu, McSherry, Ryzhyk, Tannen, "DBSP: Automatic Incremental View Maintenance for Rich Query Languages" https://arxiv.org/abs/2203.16684
//!       McSherry et al., "Differential Dataflow" CIDR 2013 (cited by title; the URL guessed earlier returned 404, so no link)
//!       Gupta & Mumick, "Maintenance of Materialized Views: Problems, Techniques, and Applications" (cited by title)
//!
//! TRY NEXT:
//!   - [S] Add `MIN` to `IncAgg` by keeping a `BTreeMap<c, weight>` per group; verify against `oracle` and explain why a plain scalar fails on delete.
//!   - [S] Predict, then measure with the batch-size table: at what batch size does `IncJoin::step` stop beating `full_eval`? Why is it far above 1?
//!   - [M] Put DISTINCT inside the transitive-closure loop (apply it to each `D_i` against the integrated P) so cyclic graphs converge;
//!     make the third pitfall pass and check against the DFS oracle with back edges allowed.
//!   - [M] Add a LEFT OUTER JOIN view (rows of R without a match in S, padded) built only from `IncJoin`, `IncDistinct` and negation.
//!   - [A] Project: combine with `mvcc_snapshot_isolation` (each commit's write set becomes a z-set delta) and `columnar_encodings`
//!     (store the integrated state columnar); serve readers a consistent view while a writer applies batches.
//!   - [A] Project: feed `datalog_engine_semi_naive`'s delta relations through this `IncTc` pattern to get an incremental Datalog with deletions.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::mem;

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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------- z-sets ----------------
#[derive(Clone, Debug, PartialEq, Default)]
struct ZSet<T: Ord>(BTreeMap<T, i64>);
impl<T: Ord + Clone> ZSet<T> {
    fn new() -> Self {
        ZSet(BTreeMap::new())
    }
    /// Add weight `w` to row `t`; rows whose weight reaches zero are dropped so equality is semantic.
    fn add(&mut self, t: T, w: i64) {
        if w == 0 {
            return;
        }
        let n = {
            let e = self.0.entry(t.clone()).or_insert(0);
            *e += w;
            *e
        };
        if n == 0 {
            self.0.remove(&t);
        }
    }
    fn plus(&mut self, o: &Self) {
        for (t, &w) in &o.0 {
            self.add(t.clone(), w);
        }
    }
    fn neg(&self) -> Self {
        ZSet(self.0.iter().map(|(t, &w)| (t.clone(), -w)).collect())
    }
    fn len(&self) -> usize {
        self.0.len()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn all_positive(&self) -> bool {
        self.0.values().all(|&w| w > 0)
    }
    /// Linear: weights of colliding outputs add (so projection keeps multiplicities).
    fn map<U: Ord + Clone>(&self, f: impl Fn(&T) -> U) -> ZSet<U> {
        let mut o = ZSet::new();
        for (t, &w) in &self.0 {
            o.add(f(t), w);
        }
        o
    }
    fn filter(&self, f: impl Fn(&T) -> bool) -> Self {
        ZSet(
            self.0
                .iter()
                .filter(|(t, _)| f(t))
                .map(|(t, &w)| (t.clone(), w))
                .collect(),
        )
    }
    /// Non-linear: every row with positive weight becomes weight 1.
    fn distinct(&self) -> Self {
        ZSet(
            self.0
                .iter()
                .filter(|(_, w)| **w > 0)
                .map(|(t, _)| (t.clone(), 1))
                .collect(),
        )
    }
    fn from_pairs(v: impl IntoIterator<Item = (T, i64)>) -> Self {
        let mut z = ZSet::new();
        for (t, w) in v {
            z.add(t, w);
        }
        z
    }
}

// ---------------- streams: I, D, z^-1 ----------------
fn integrate<T: Ord + Clone>(s: &[ZSet<T>]) -> Vec<ZSet<T>> {
    let mut acc = ZSet::new();
    s.iter()
        .map(|d| {
            acc.plus(d);
            acc.clone()
        })
        .collect()
}
fn differentiate<T: Ord + Clone>(s: &[ZSet<T>]) -> Vec<ZSet<T>> {
    let mut prev = ZSet::new();
    s.iter()
        .map(|x| {
            let mut d = x.clone();
            d.plus(&prev.neg());
            prev = x.clone();
            d
        })
        .collect()
}
/// z^-1: emits the previous input (the empty set at t=0).
struct Delay<T: Ord>(ZSet<T>);
impl<T: Ord + Clone> Delay<T> {
    fn step(&mut self, x: ZSet<T>) -> ZSet<T> {
        mem::replace(&mut self.0, x)
    }
}
/// I built from a feedback loop: out = in + z^-1(out).
struct Integrator<T: Ord>(Delay<T>);
impl<T: Ord + Clone> Integrator<T> {
    fn step(&mut self, x: &ZSet<T>) -> ZSet<T> {
        let mut out = x.clone();
        out.plus(&self.0.0); // peek at z^-1(out) without consuming
        self.0.step(out.clone());
        out
    }
}

// ---------------- joins ----------------
type Idx = BTreeMap<i64, BTreeMap<i64, i64>>; // join key -> other column -> weight
fn idx_apply(ix: &mut Idx, key: i64, val: i64, w: i64) {
    let inner = ix.entry(key).or_default();
    let n = inner.entry(val).or_insert(0);
    *n += w;
    if *n == 0 {
        inner.remove(&val);
    }
    if inner.is_empty() {
        ix.remove(&key);
    }
}
/// From-scratch equi-join R(a,b) ⋈ S(b,c) -> (a,b,c). `work` counts probes plus emitted rows.
fn join_full(r: &ZSet<(i64, i64)>, s: &ZSet<(i64, i64)>, work: &mut u64) -> ZSet<(i64, i64, i64)> {
    let mut ix: Idx = BTreeMap::new();
    for (&(b, c), &w) in &s.0 {
        idx_apply(&mut ix, b, c, w);
        *work += 1;
    }
    let mut out = ZSet::new();
    for (&(a, b), &wr) in &r.0 {
        *work += 1;
        for (&c, &ws) in ix.get(&b).into_iter().flatten() {
            *work += 1;
            out.add((a, b, c), wr * ws);
        }
    }
    out
}
/// Incremental join: state = indexes of the integrated inputs; step implements the bilinear delta rule.
struct IncJoin {
    r: Idx, // key b -> a
    s: Idx, // key b -> c
}
impl IncJoin {
    fn new() -> Self {
        IncJoin {
            r: BTreeMap::new(),
            s: BTreeMap::new(),
        }
    }
    fn step(
        &mut self,
        dr: &ZSet<(i64, i64)>,
        ds: &ZSet<(i64, i64)>,
        work: &mut u64,
    ) -> ZSet<(i64, i64, i64)> {
        let mut out = ZSet::new();
        // dR ⋈ S_old
        for (&(a, b), &w) in &dr.0 {
            *work += 1;
            for (&c, &ws) in self.s.get(&b).into_iter().flatten() {
                *work += 1;
                out.add((a, b, c), w * ws);
            }
        }
        // R_old ⋈ dS
        for (&(b, c), &w) in &ds.0 {
            *work += 1;
            for (&a, &wr) in self.r.get(&b).into_iter().flatten() {
                *work += 1;
                out.add((a, b, c), wr * w);
            }
        }
        // dR ⋈ dS
        let mut dsi: Idx = BTreeMap::new();
        for (&(b, c), &w) in &ds.0 {
            idx_apply(&mut dsi, b, c, w);
        }
        for (&(a, b), &w) in &dr.0 {
            for (&c, &ws) in dsi.get(&b).into_iter().flatten() {
                *work += 1;
                out.add((a, b, c), w * ws);
            }
        }
        for (&(a, b), &w) in &dr.0 {
            idx_apply(&mut self.r, b, a, w);
        }
        for (&(b, c), &w) in &ds.0 {
            idx_apply(&mut self.s, b, c, w);
        }
        out
    }
}

// ---------------- distinct ----------------
struct IncDistinct<T: Ord> {
    state: ZSet<T>,
}
impl<T: Ord + Clone> IncDistinct<T> {
    fn new() -> Self {
        IncDistinct { state: ZSet::new() }
    }
    /// D(distinct(I(delta))): emit only when a row's integrated weight crosses zero.
    fn step(&mut self, d: &ZSet<T>, work: &mut u64) -> ZSet<T> {
        let mut out = ZSet::new();
        for (t, &w) in &d.0 {
            *work += 1;
            let old = self.state.0.get(t).copied().unwrap_or(0);
            out.add(t.clone(), (old + w > 0) as i64 - (old > 0) as i64);
            self.state.add(t.clone(), w);
        }
        out
    }
}

// ---------------- group-by count/sum ----------------
type Group = (i64, i64, i64); // (a, count, sum)
struct IncAgg {
    state: BTreeMap<i64, (i64, i64)>,
}
impl IncAgg {
    fn step(&mut self, d: &ZSet<(i64, i64)>, work: &mut u64) -> ZSet<Group> {
        let mut touched: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
        for (&(a, c), &w) in &d.0 {
            *work += 1;
            let e = touched.entry(a).or_insert((0, 0));
            e.0 += w;
            e.1 += w * c;
        }
        let mut out = ZSet::new();
        for (a, (dc, ds)) in touched {
            let old = self.state.get(&a).copied().unwrap_or((0, 0));
            let new = (old.0 + dc, old.1 + ds);
            if old.0 > 0 {
                out.add((a, old.0, old.1), -1); // retract the old aggregate row
            }
            if new.0 > 0 {
                out.add((a, new.0, new.1), 1); // assert the new one
                self.state.insert(a, new);
            } else {
                assert_eq!(new.1, 0, "an empty group must have sum 0");
                self.state.remove(&a);
            }
        }
        out
    }
}
fn agg_full(ac: &ZSet<(i64, i64)>, work: &mut u64) -> ZSet<Group> {
    let mut g: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for (&(a, c), &w) in &ac.0 {
        *work += 1;
        let e = g.entry(a).or_insert((0, 0));
        e.0 += w;
        e.1 += w * c;
    }
    ZSet::from_pairs(
        g.into_iter()
            .filter(|(_, (n, _))| *n > 0)
            .map(|(a, (n, s))| ((a, n, s), 1)),
    )
}

// ---------------- top-k over the group view ----------------
const K: usize = 3;
struct IncTopK {
    all: BTreeSet<(Reverse<i64>, i64, i64)>, // (sum desc, a, count): ALL groups, not just K
    top: Vec<Group>,
}
impl IncTopK {
    fn step(&mut self, d: &ZSet<Group>, work: &mut u64) -> ZSet<Group> {
        for (&(a, n, s), &w) in &d.0 {
            *work += 1;
            if w < 0 {
                assert!(
                    self.all.remove(&(Reverse(s), a, n)),
                    "retracting an unknown group"
                );
            }
        }
        for (&(a, n, s), &w) in &d.0 {
            if w > 0 {
                self.all.insert((Reverse(s), a, n));
            }
        }
        let new_top: Vec<Group> = self
            .all
            .iter()
            .take(K)
            .map(|&(Reverse(s), a, n)| (a, n, s))
            .collect();
        let mut out = ZSet::new();
        for g in &self.top {
            out.add(*g, -1);
        }
        for g in &new_top {
            out.add(*g, 1);
        }
        self.top = new_top;
        out
    }
}
fn topk_full(groups: &ZSet<Group>) -> ZSet<Group> {
    let mut v: Vec<Group> = groups.0.keys().copied().collect();
    v.sort_by_key(|&(a, _, s)| (Reverse(s), a));
    ZSet::from_pairs(v.into_iter().take(K).map(|g| (g, 1)))
}

// ---------------- transitive closure (semi-naive, nested time) ----------------
struct IncTc {
    e: BTreeMap<i64, BTreeMap<i64, i64>>, // src -> dst -> weight (E_new)
    p_dst: BTreeMap<i64, BTreeMap<i64, i64>>, // dst -> src -> path count (P_old)
}
impl IncTc {
    fn new() -> Self {
        IncTc {
            e: BTreeMap::new(),
            p_dst: BTreeMap::new(),
        }
    }
    /// Returns ΔP (path-count delta). Err if the inner iteration does not terminate within `cap` rounds (cycle).
    fn step(
        &mut self,
        de: &ZSet<(i64, i64)>,
        work: &mut u64,
        cap: usize,
    ) -> Result<ZSet<(i64, i64)>, String> {
        for (&(u, v), &w) in &de.0 {
            idx_apply(&mut self.e, u, v, w);
        }
        // D_0 = ΔE + P_old ⋈ ΔE
        let mut d = de.clone();
        for (&(y, z), &w) in &de.0 {
            for (&x, &wp) in self.p_dst.get(&y).into_iter().flatten() {
                *work += 1;
                d.add((x, z), wp * w);
            }
        }
        let mut total = d.clone();
        let mut round = 0;
        while !d.is_empty() {
            round += 1;
            if round > cap {
                return Err(format!("no fixpoint after {cap} inner iterations (cycle?)"));
            }
            let mut nd = ZSet::new();
            for (&(x, y), &w) in &d.0 {
                for (&z, &we) in self.e.get(&y).into_iter().flatten() {
                    *work += 1;
                    nd.add((x, z), w * we);
                }
            }
            total.plus(&nd);
            d = nd; // D_{i+1} = D_i ⋈ E_new
        }
        for (&(x, y), &w) in &total.0 {
            idx_apply(&mut self.p_dst, y, x, w);
        }
        Ok(total)
    }
}
/// From-scratch path counts on a DAG by iterating D_{i+1} = D_i ⋈ E.
fn tc_full(e: &ZSet<(i64, i64)>, work: &mut u64) -> ZSet<(i64, i64)> {
    let mut adj: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    for (&(u, v), &w) in &e.0 {
        adj.entry(u).or_default().push((v, w));
    }
    let (mut p, mut d) = (e.clone(), e.clone());
    for _ in 0..1000 {
        if d.is_empty() {
            return p;
        }
        let mut nd = ZSet::new();
        for (&(x, y), &w) in &d.0 {
            for &(z, we) in adj.get(&y).into_iter().flatten() {
                *work += 1;
                nd.add((x, z), w * we);
            }
        }
        p.plus(&nd);
        d = nd;
    }
    panic!("tc_full did not converge: cyclic input");
}

// ---------------- the whole engine ----------------
#[derive(Clone, Debug, PartialEq, Default)]
struct Views {
    v1: ZSet<(i64, i64)>,
    v2: ZSet<Group>,
    v3: ZSet<Group>,
    v4: ZSet<(i64, i64)>,
}
impl Views {
    fn apply(&mut self, d: &Views) {
        self.v1.plus(&d.v1);
        self.v2.plus(&d.v2);
        self.v3.plus(&d.v3);
        self.v4.plus(&d.v4);
    }
    fn well_formed(&self) -> bool {
        self.v1.all_positive()
            && self.v2.all_positive()
            && self.v3.all_positive()
            && self.v4.all_positive()
    }
}
fn keep(&(a, _, c): &(i64, i64, i64)) -> bool {
    (a + c) % 3 != 0
}
struct Engine {
    join: IncJoin,
    d1: IncDistinct<(i64, i64)>,
    agg: IncAgg,
    topk: IncTopK,
    tc: IncTc,
    d4: IncDistinct<(i64, i64)>,
    work: u64,
}
impl Engine {
    fn new() -> Self {
        Engine {
            join: IncJoin::new(),
            d1: IncDistinct::new(),
            agg: IncAgg {
                state: BTreeMap::new(),
            },
            topk: IncTopK {
                all: BTreeSet::new(),
                top: vec![],
            },
            tc: IncTc::new(),
            d4: IncDistinct::new(),
            work: 0,
        }
    }
    fn step(
        &mut self,
        dr: &ZSet<(i64, i64)>,
        ds: &ZSet<(i64, i64)>,
        de: &ZSet<(i64, i64)>,
    ) -> Result<Views, String> {
        let w = &mut self.work;
        let dj = self.join.step(dr, ds, w).filter(keep); // linear: filter the delta
        let ac = dj.map(|&(a, _, c)| (a, c)); // linear: project the delta
        let v1 = self.d1.step(&ac, w);
        let v2 = self.agg.step(&ac, w);
        let v3 = self.topk.step(&v2, w);
        let dp = self.tc.step(de, w, 64)?;
        let v4 = self.d4.step(&dp, w);
        Ok(Views { v1, v2, v3, v4 })
    }
}
/// Recompute everything from the integrated base tables with the same z-set operators.
fn full_eval(
    r: &ZSet<(i64, i64)>,
    s: &ZSet<(i64, i64)>,
    e: &ZSet<(i64, i64)>,
    work: &mut u64,
) -> Views {
    let j = join_full(r, s, work).filter(keep);
    let ac = j.map(|&(a, _, c)| (a, c));
    *work += ac.len() as u64;
    let v2 = agg_full(&ac, work);
    let v3 = topk_full(&v2);
    let v4 = tc_full(e, work).distinct();
    *work += v4.len() as u64;
    Views {
        v1: ac.distinct(),
        v2,
        v3,
        v4,
    }
}

// ---------------- update workloads and the independent oracle ----------------
#[derive(Default, Clone)]
struct Model {
    r: BTreeMap<(i64, i64), i64>,
    s: BTreeMap<(i64, i64), i64>,
    e: BTreeSet<(i64, i64)>,
}
struct Dom {
    a: u64,
    b: u64,
    c: u64,
    nodes: u64,
    forest: bool,
}
fn bump(m: &mut BTreeMap<(i64, i64), i64>, k: (i64, i64), w: i64) {
    let n = m.entry(k).or_insert(0);
    *n += w;
    if *n == 0 {
        m.remove(&k);
    }
}
/// One batch of `ops` random modifications applied to the model; returns the matching deltas.
fn gen_batch(rng: &mut Rng, m: &mut Model, dom: &Dom, ops: usize) -> [ZSet<(i64, i64)>; 3] {
    let mut d = [ZSet::new(), ZSet::new(), ZSet::new()];
    for _ in 0..ops {
        match rng.below(3) {
            0 | 1 => {
                let which = rng.below(2) as usize;
                let (tbl, d1, d2) = if which == 0 {
                    (dom.a, dom.b, 0)
                } else {
                    (dom.b, dom.c, 1)
                };
                let model = if which == 0 { &mut m.r } else { &mut m.s };
                if rng.chance(45) && !model.is_empty() {
                    let k = *model
                        .keys()
                        .nth(rng.below(model.len() as u64) as usize)
                        .unwrap();
                    bump(model, k, -1);
                    d[which].add(k, -1);
                } else {
                    let k = (rng.below(tbl) as i64, rng.below(d1) as i64);
                    bump(model, k, 1);
                    d[which].add(k, 1);
                }
                let _ = d2;
            }
            _ => {
                if dom.forest {
                    let v = 1 + rng.below(dom.nodes - 1) as i64;
                    if let Some(&edge) = m.e.iter().find(|&&(_, t)| t == v) {
                        m.e.remove(&edge);
                        d[2].add(edge, -1);
                    } else {
                        let edge = (rng.below(v as u64) as i64, v);
                        m.e.insert(edge);
                        d[2].add(edge, 1);
                    }
                } else {
                    let (x, y) = (rng.below(dom.nodes) as i64, rng.below(dom.nodes) as i64);
                    if x == y {
                        continue;
                    }
                    let edge = (x.min(y), x.max(y));
                    if m.e.remove(&edge) {
                        d[2].add(edge, -1);
                    } else {
                        m.e.insert(edge);
                        d[2].add(edge, 1);
                    }
                }
            }
        }
    }
    d
}
fn zset_of(m: &BTreeMap<(i64, i64), i64>) -> ZSet<(i64, i64)> {
    ZSet::from_pairs(m.iter().map(|(&k, &w)| (k, w)))
}
fn eset_of(e: &BTreeSet<(i64, i64)>) -> ZSet<(i64, i64)> {
    ZSet::from_pairs(e.iter().map(|&k| (k, 1)))
}
/// Plain nested loops and DFS: shares no code with the z-set operators.
fn oracle(m: &Model) -> Views {
    let mut ac: BTreeMap<(i64, i64), i64> = BTreeMap::new();
    for (&(a, b), &wr) in &m.r {
        for (&(b2, c), &ws) in &m.s {
            if b == b2 && (a + c) % 3 != 0 {
                *ac.entry((a, c)).or_insert(0) += wr * ws;
            }
        }
    }
    let v1 = ZSet::from_pairs(ac.iter().filter(|(_, w)| **w > 0).map(|(&k, _)| (k, 1)));
    let mut g: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
    for (&(a, c), &w) in &ac {
        let e = g.entry(a).or_insert((0, 0));
        e.0 += w;
        e.1 += w * c;
    }
    let mut rows: Vec<Group> = g
        .iter()
        .filter(|(_, v)| v.0 > 0)
        .map(|(&a, &(n, s))| (a, n, s))
        .collect();
    let v2 = ZSet::from_pairs(rows.iter().map(|&r| (r, 1)));
    rows.sort_by(|x, y| y.2.cmp(&x.2).then(x.0.cmp(&y.0)));
    let v3 = ZSet::from_pairs(rows.into_iter().take(K).map(|r| (r, 1)));
    let mut v4 = ZSet::new();
    let srcs: BTreeSet<i64> = m.e.iter().map(|&(u, _)| u).collect();
    for src in srcs {
        let (mut seen, mut stack) = (BTreeSet::new(), vec![src]);
        while let Some(u) = stack.pop() {
            for &(x, y) in m.e.range((u, i64::MIN)..=(u, i64::MAX)) {
                debug_assert_eq!(x, u);
                if seen.insert(y) {
                    stack.push(y);
                }
            }
        }
        for t in seen {
            v4.add((src, t), 1);
        }
    }
    Views { v1, v2, v3, v4 }
}

// ---------------- checks ----------------
fn random_stream(rng: &mut Rng, len: usize, dom: u64) -> Vec<ZSet<(i64, i64)>> {
    (0..len)
        .map(|_| {
            ZSet::from_pairs((0..rng.below(5)).map(|_| {
                (
                    (rng.below(dom) as i64, rng.below(dom) as i64),
                    rng.below(5) as i64 - 2,
                )
            }))
        })
        .collect()
}
fn check_stream_laws() {
    let mut rng = Rng(841);
    for _ in 0..200 {
        let s = random_stream(&mut rng, 8, 4);
        assert_eq!(differentiate(&integrate(&s)), s, "D(I(s)) = s");
        assert_eq!(integrate(&differentiate(&s)), s, "I(D(s)) = s");
        let mut it = Integrator(Delay(ZSet::new()));
        let fb: Vec<_> = s.iter().map(|x| it.step(x)).collect();
        assert_eq!(
            fb,
            integrate(&s),
            "feedback loop through z^-1 is the integral"
        );
        // linear operators: Q^Δ = Q
        let f = |t: &(i64, i64)| (t.0 % 2, t.1);
        let g = |t: &(i64, i64)| (t.0 + t.1) % 2 == 0;
        let inc: Vec<_> = s
            .iter()
            .map(|x| x.map(f).filter(|t| t.0 >= 0 && g(&(t.0, t.1))))
            .collect();
        let via_i: Vec<_> = differentiate(
            &integrate(&s)
                .iter()
                .map(|x| x.map(f).filter(|t| t.0 >= 0 && g(&(t.0, t.1))))
                .collect::<Vec<_>>(),
        );
        assert_eq!(inc, via_i, "linear ops commute with I/D");
        // distinct
        let mut d = IncDistinct::new();
        let mut w = 0;
        let got: Vec<_> = s.iter().map(|x| d.step(x, &mut w)).collect();
        let want = differentiate(
            &integrate(&s)
                .iter()
                .map(|x| x.distinct())
                .collect::<Vec<_>>(),
        );
        assert_eq!(got, want, "incremental distinct = D(distinct(I(s)))");
        // bilinear join rule against D(I(a) ⋈ I(b))
        let sb = random_stream(&mut rng, 8, 4);
        let mut j = IncJoin::new();
        let got: Vec<_> = s
            .iter()
            .zip(&sb)
            .map(|(a, b)| j.step(a, b, &mut w))
            .collect();
        let (ia, ib) = (integrate(&s), integrate(&sb));
        let want = differentiate(
            &ia.iter()
                .zip(&ib)
                .map(|(a, b)| join_full(a, b, &mut w))
                .collect::<Vec<_>>(),
        );
        assert_eq!(got, want, "ΔA⋈B + A⋈ΔB + ΔA⋈ΔB = D(I(A)⋈I(B))");
    }
    println!(
        "stream laws: D∘I = I∘D = id, z^-1 feedback = integral, linear ops pass through, bilinear join rule, incremental distinct"
    );
}

fn check_random_sequences() {
    let dom = Dom {
        a: 6,
        b: 5,
        c: 6,
        nodes: 9,
        forest: false,
    };
    let (mut steps, mut inc_work, mut full_work) = (0u64, 0u64, 0u64);
    let mut rng = Rng(0xD85B_0841);
    for seq in 0..500 {
        let mut model = Model::default();
        let mut eng = Engine::new();
        let mut views = Views::default();
        for step in 0..30 {
            let ops = 1 + rng.below(4) as usize;
            let [dr, ds, de] = gen_batch(&mut rng, &mut model, &dom, ops);
            let dv = eng.step(&dr, &ds, &de).expect("DAG input must converge");
            views.apply(&dv);
            assert!(
                views.well_formed(),
                "negative or zero weight in a maintained view (seq {seq} step {step})"
            );
            let mut w = 0;
            let scratch = full_eval(
                &zset_of(&model.r),
                &zset_of(&model.s),
                &eset_of(&model.e),
                &mut w,
            );
            assert_eq!(
                views, scratch,
                "incremental != from-scratch z-set evaluation (seq {seq} step {step})"
            );
            assert_eq!(
                views,
                oracle(&model),
                "incremental != nested-loop oracle (seq {seq} step {step})"
            );
            full_work += w;
            steps += 1;
        }
        inc_work += eng.work;
    }
    println!(
        "500 random update sequences x 30 steps = {steps} checkpoints: incremental == scratch == oracle at every step"
    );
    println!(
        "  rows touched on these tiny tables: incremental {inc_work}, recompute {full_work} (tables are too small to show a big gap)"
    );
}

fn check_pitfalls() {
    // 1. Counting pitfall: the row is inserted twice, deleted once. It must remain.
    let (ins, del) = (
        ZSet::from_pairs([((1, 1), 1)]),
        ZSet::from_pairs([((1, 1), -1)]),
    );
    let mut good = IncDistinct::new();
    let mut w = 0;
    let mut view = ZSet::new();
    for d in [&ins, &ins, &del] {
        view.plus(&good.step(d, &mut w));
    }
    assert_eq!(
        view.0.get(&(1, 1)),
        Some(&1),
        "z-set distinct keeps the row after one of two copies is deleted"
    );
    // naive: treat every delete as "remove from the set"
    let mut naive: BTreeSet<(i64, i64)> = BTreeSet::new();
    for (row, w) in [((1, 1), 1), ((1, 1), 1), ((1, 1), -1)] {
        if w > 0 {
            naive.insert(row)
        } else {
            naive.remove(&row)
        };
    }
    assert!(naive.is_empty(), "the naive set-delta wrongly lost the row");
    println!(
        "pitfall 1 (counting): naive set-based delete loses a row that still has one copy; z-set weights keep it"
    );

    // 2. Top-K pitfall: keep only K rows and a delete cannot be repaired.
    let mut full = IncTopK {
        all: BTreeSet::new(),
        top: vec![],
    };
    let mut naive_top: Vec<Group> = vec![];
    let groups = [(1, 1, 10), (2, 1, 9), (3, 1, 8), (4, 1, 7)];
    let mut view = ZSet::new();
    for g in groups {
        view.plus(&full.step(&ZSet::from_pairs([(g, 1)]), &mut w));
        naive_top.push(g);
        naive_top.sort_by_key(|&(a, _, s)| (Reverse(s), a));
        naive_top.truncate(K);
    }
    view.plus(&full.step(&ZSet::from_pairs([((1, 1, 10), -1)]), &mut w)); // delete the leader
    naive_top.retain(|&g| g != (1, 1, 10));
    let truth: BTreeSet<Group> = [(2, 1, 9), (3, 1, 8), (4, 1, 7)].into();
    assert_eq!(
        view.0.keys().copied().collect::<BTreeSet<_>>(),
        truth,
        "full-state top-k promotes the (K+1)th row"
    );
    assert_eq!(
        naive_top.len(),
        K - 1,
        "K-row state cannot refill after a delete"
    );
    println!(
        "pitfall 2 (top-k): after deleting the leader the K-row naive state has {} rows, the full-state view has {K}",
        naive_top.len()
    );

    // 3. Recursion pitfall: a cycle never reaches a fixpoint.
    let mut tc = IncTc::new();
    tc.step(&ZSet::from_pairs([((0, 1), 1), ((1, 2), 1)]), &mut w, 64)
        .unwrap();
    let r = tc.step(&ZSet::from_pairs([((2, 0), 1)]), &mut w, 64);
    assert!(r.is_err(), "a back edge makes path counts infinite");
    println!("pitfall 3 (recursion): {}", r.unwrap_err());
}

/// Run `steps` batches of `batch` changes on a warmed-up 3000-row workload; returns (incremental work, recompute work).
fn run_workload(batch: usize, steps: usize, seed: u64) -> (u64, u64) {
    let dom = Dom {
        a: 200,
        b: 100,
        c: 200,
        nodes: 300,
        forest: true,
    };
    let mut rng = Rng(seed);
    let mut model = Model::default();
    let mut eng = Engine::new();
    let mut views = Views::default();
    let [dr, ds, de] = gen_batch(
        &mut rng,
        &mut model,
        &Dom {
            a: 200,
            b: 100,
            c: 200,
            nodes: 300,
            forest: true,
        },
        0,
    );
    let _ = (dr, ds, de);
    // warm-up: bulk load as one huge delta
    let (mut r0, mut s0, mut e0) = (ZSet::new(), ZSet::new(), ZSet::new());
    for _ in 0..3000 {
        let k = (rng.below(dom.a) as i64, rng.below(dom.b) as i64);
        bump(&mut model.r, k, 1);
        r0.add(k, 1);
        let k = (rng.below(dom.b) as i64, rng.below(dom.c) as i64);
        bump(&mut model.s, k, 1);
        s0.add(k, 1);
    }
    for v in 1..300i64 {
        let edge = (rng.below(v as u64) as i64, v);
        model.e.insert(edge);
        e0.add(edge, 1);
    }
    views.apply(&eng.step(&r0, &s0, &e0).unwrap());
    eng.work = 0;
    let mut full_work = 0;
    for _ in 0..steps {
        let [dr, ds, de] = gen_batch(&mut rng, &mut model, &dom, batch);
        views.apply(&eng.step(&dr, &ds, &de).unwrap());
        let mut w = 0;
        let scratch = full_eval(
            &zset_of(&model.r),
            &zset_of(&model.s),
            &eset_of(&model.e),
            &mut w,
        );
        assert_eq!(
            views, scratch,
            "incremental diverged from recompute (batch {batch})"
        );
        full_work += w;
    }
    assert_eq!(views, oracle(&model), "final state must match the oracle");
    (eng.work, full_work)
}

fn main() {
    check_stream_laws();
    check_random_sequences();
    check_pitfalls();
    println!(
        "\nwork (rows touched) on 3000-row R and S, 300-node forest E, 20 steps per row; ratio = recompute / incremental:"
    );
    println!(
        "{:>12} {:>14} {:>14} {:>9}",
        "changes/step", "incremental", "recompute", "ratio"
    );
    let mut small_ratio = 0.0;
    for batch in [1usize, 5, 25, 125, 600] {
        let (inc, full) = run_workload(batch, 20, 77 + batch as u64);
        let ratio = full as f64 / inc.max(1) as f64;
        println!("{batch:>12} {inc:>14} {full:>14} {ratio:>8.1}x");
        if batch == 5 {
            small_ratio = ratio;
        }
    }
    assert!(
        small_ratio > 20.0,
        "small deltas should be >20x cheaper than recomputation, got {small_ratio:.1}x"
    );
    println!("all checks passed");
}
