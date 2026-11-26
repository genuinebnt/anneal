//! RECIPE 872 (vectors): filtered ANN (pre / post / in-algorithm filtering with a selectivity planner) and hybrid BM25 + vector search with RRF.
//! LEVEL: advanced
//! Use in: "nearest products under $50 in category X", multi-tenant search, RAG with metadata, and any query that mixes keywords and embeddings.   Run: cargo run --release --example filtered_ann_hybrid_search
#![allow(dead_code)]
//!
//! PART 1, FILTERED ANN. Real queries carry a predicate: `cat = 7 AND price BETWEEN 20 AND 80`. Three ways to combine it with an index:
//!   PRE-filter   resolve the predicate to an id list with attribute indexes (posting lists, sorted array for ranges), then brute-force
//!                the survivors. Exact, cost = #matches. Wins when the filter is very selective (few matches).
//!   POST-filter  run the ANN search for k/selectivity results and drop non-matches. Cheap when most rows match; collapses
//!                when the filter is selective (you must over-fetch ~k/s candidates, and may still return fewer than k).
//!   IN-FILTER    search the graph but keep only MATCHING nodes in the result list, while still EXPANDING through non-matching
//!                nodes (they are the bridges; refusing to cross them disconnects the graph). Works in the middle; at very low
//!                selectivity it wanders the whole graph and a correlated filter (matches all in one cluster) strands the search
//!                far away unless it STARTS near the matches: we keep a per-label medoid as extra entry point, the "start
//!                nodes" idea of Filtered-DiskANN (that paper also adds label-aware edges at build time so the graph restricted to a label stays
//!                connected; Qdrant adds payload-aware extra links, Weaviate/Lucene/Elasticsearch fall back to exact search under a selectivity cutoff).
//! PLANNER: gets the selectivity from the attribute indexes (exact counts; `AttrIndex::estimate` shows the cheap independence estimate for AND and why it is risky near the crossover), looks up
//! a cost table CALIBRATED on training queries (average distance computations and feasibility per log2-selectivity bucket for each
//! plan), and picks the cheapest plan that is feasible (recall >= 0.8 on at least 90% of calibration queries); pre-filter is always feasible.
//! If a graph plan returns fewer than min(k, matches) rows the engine falls back to pre-filter and pays for both (cost is counted).
//!
//! PART 2, HYBRID SEARCH. A small BM25 inverted index (term-at-a-time, tf saturation k1=1.2, length norm b=0.75) and an exact dense
//! retriever run on the same synthetic corpus; each query has a lexical part that misses 40% of the time (the entity word is dropped) and a
//! vector part that is always moderately noisy (relevant docs land at mid ranks), so each retriever alone fails on different queries
//! and RRF profits from AGREEMENT: a relevant doc is in both lists, a distractor usually in one. Fusion by
//! RECIPROCAL RANK FUSION score(d) = sum over lists 1/(c + rank_list(d)), c=60 (ranks only; no score calibration needed) and by
//! WEIGHTED min-max normalised scores (needs calibration; a document missing from a list counts as 0). nDCG@10 and recall@50 are
//! computed against the synthetic relevance set (all docs mentioning the query entity).
//!
//! WHAT IS NOT GUARANTEED: against a HINDSIGHT oracle (any plan that happened to work for that one query, reliable or not) the worst
//! ratio is far above 3x (the run prints it). The planner deliberately skips a graph plan that is cheap on average but fails recall on
//! more than 10% of the calibration queries in its bucket, because nobody knows in advance which queries will be the failures.
//!
//! VERIFIED: every plan returns only rows satisfying the filter (300 queries x 3 plans); in-filter at ef = 800 matches filtered
//! brute force with recall >= 0.97; on 300 held-out random queries the planner's effective cost is within 3x of the cheapest feasible
//! plan AMONG THOSE IT WAS ALLOWED TO PICK (pre-filter, plus graph plans that calibration found reliable in that bucket); BM25 equals a naive per-document oracle; RRF properties (missing docs, ties by id, symmetry, idempotence, single list
//! keeps order); fusion beats both single retrievers on nDCG@10.
//! COMMON USES: Qdrant filterable HNSW and payload indexes, Weaviate ACORN/filtered search, Elasticsearch/Lucene kNN with filter and the
//! "approximate then exact fallback" cutoff, pgvector (post-filter surprise: HNSW + WHERE returns fewer than k rows), Vespa and
//! OpenSearch hybrid queries (RRF), Pinecone/Milvus metadata filtering.
//! Read: Filtered-DiskANN: Graph Algorithms for Approximate Nearest Neighbor Search with Filters, Gollapudi et al., WWW 2023 (cited by title; the ACM URL returned 403) ;
//! Reciprocal Rank Fusion outperforms Condorcet and individual Rank Learning Methods, Cormack, Clarke, Buettcher, SIGIR 2009 https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf ;
//! https://arxiv.org/abs/1603.09320 (HNSW) ; Robertson and Zaragoza, "The Probabilistic Relevance Framework: BM25 and Beyond" (cited by title; URL not checked).
//!
//! TRY NEXT:
//!   - [S] Predict, then measure: change `RRF_C` from 60 to 1 and to 1000. Which fusion metric moves, and does the order of `fuse_rrf` outputs change for documents ranked 1 and 2 in only one list?
//!   - [S] In `search_in`, stop expanding non-matching nodes (push only matching nodes into `cand`). At which selectivity bucket does recall collapse, and does the correlated `cluster` filter break first?
//!   - [M] Replace the calibrated cost table in `Planner` by the analytic estimate `alpha * ef / s`; find the query class where its choice is more than 3x worse and explain which assumption failed.
//!   - [M] Real-world port: write `parse_filter("cat = 7 and price < 50")` producing the `Filter` enum, then make `Planner::choose` log (selectivity, plan) per query like an EXPLAIN output.
//!   - [A] PROJECT: put this planner in front of the quantised codes from `scalar_binary_quantization_rescoring` (filter first, rescore after) and swap the in-filter graph for `hnsw_lite`; compare the crossover selectivities.
use std::cell::Cell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};

// ---------- shared mini-harness ----------
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    fn gauss(&mut self) -> f32 {
        let (u, v) = (self.unit(), self.unit());
        ((-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()) as f32
    }
}
fn l2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
#[derive(PartialEq, Clone, Copy)]
struct F(f32);
impl Eq for F {}
impl PartialOrd for F {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for F {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}
fn recall(approx: &[usize], exact: &[usize]) -> f64 {
    if exact.is_empty() {
        return 1.0;
    }
    approx.iter().filter(|a| exact.contains(a)).count() as f64 / exact.len() as f64
}

// ---------- the collection: vectors + attributes ----------
const DIM: usize = 32;
const NCAT: usize = 50;
const NCLUSTER: usize = 20;
struct Row {
    cat: usize, // independent of the vector, zipf-skewed (a few huge categories, many tiny)
    cluster: usize, // CORRELATED with the vector (which blob it came from)
    price: u32, // 0..1000 independent
}
struct Collection {
    vecs: Vec<f32>,
    rows: Vec<Row>,
    centres: Vec<f32>,
}
impl Collection {
    fn vec(&self, i: usize) -> &[f32] {
        &self.vecs[i * DIM..(i + 1) * DIM]
    }
    fn n(&self) -> usize {
        self.rows.len()
    }
    fn make(n: usize, rng: &mut Rng) -> Self {
        let centres: Vec<f32> = (0..NCLUSTER * DIM).map(|_| rng.gauss() * 2.0).collect();
        let (mut vecs, mut rows) = (Vec::with_capacity(n * DIM), Vec::with_capacity(n));
        for _ in 0..n {
            let cluster = rng.below(NCLUSTER);
            vecs.extend((0..DIM).map(|d| centres[cluster * DIM + d] + rng.gauss() * 0.9));
            let cat = ((NCAT as f64).powf(rng.unit()) - 1.0) as usize % NCAT; // log-uniform: many ids near 0
            rows.push(Row {
                cat,
                cluster,
                price: rng.below(1000) as u32,
            });
        }
        Collection {
            vecs,
            rows,
            centres,
        }
    }
    fn query_vec(&self, rng: &mut Rng) -> Vec<f32> {
        let c = rng.below(NCLUSTER);
        (0..DIM)
            .map(|d| self.centres[c * DIM + d] + rng.gauss() * 0.9)
            .collect()
    }
}

// ---------- predicates and the attribute indexes ----------
#[derive(Clone, Debug)]
enum Filter {
    CatEq(usize),
    PriceRange(u32, u32), // inclusive
    ClusterIn(Vec<usize>),
    And(Box<Filter>, Box<Filter>),
}
impl Filter {
    fn matches(&self, r: &Row) -> bool {
        match self {
            Filter::CatEq(c) => r.cat == *c,
            Filter::PriceRange(lo, hi) => (*lo..=*hi).contains(&r.price),
            Filter::ClusterIn(s) => s.contains(&r.cluster),
            Filter::And(a, b) => a.matches(r) && b.matches(r),
        }
    }
}
struct AttrIndex {
    by_cat: Vec<Vec<usize>>,
    by_cluster: Vec<Vec<usize>>,
    by_price: Vec<(u32, usize)>, // sorted by (price, id): range = two binary searches
}
impl AttrIndex {
    fn build(c: &Collection) -> Self {
        let mut by_cat = vec![vec![]; NCAT];
        let mut by_cluster = vec![vec![]; NCLUSTER];
        for (i, r) in c.rows.iter().enumerate() {
            by_cat[r.cat].push(i);
            by_cluster[r.cluster].push(i);
        }
        let mut by_price: Vec<(u32, usize)> = c
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.price, i))
            .collect();
        by_price.sort();
        AttrIndex {
            by_cat,
            by_cluster,
            by_price,
        }
    }
    fn range(&self, lo: u32, hi: u32) -> &[(u32, usize)] {
        let a = self.by_price.partition_point(|p| p.0 < lo);
        let b = self.by_price.partition_point(|p| p.0 <= hi);
        &self.by_price[a..b.max(a)]
    }
    /// Estimated number of matches (exact for single predicates, independence assumption for AND).
    fn estimate(&self, f: &Filter, n: usize) -> f64 {
        match f {
            Filter::CatEq(c) => self.by_cat[*c].len() as f64,
            Filter::PriceRange(lo, hi) => self.range(*lo, *hi).len() as f64,
            Filter::ClusterIn(s) => {
                s.iter().map(|c| self.by_cluster[*c].len()).sum::<usize>() as f64
            }
            Filter::And(a, b) => self.estimate(a, n) * self.estimate(b, n) / n as f64,
        }
    }
    /// Resolve to ids: materialise the smallest side, test the other side per row.
    fn resolve(&self, c: &Collection, f: &Filter) -> Vec<usize> {
        match f {
            Filter::CatEq(x) => self.by_cat[*x].clone(),
            Filter::PriceRange(lo, hi) => self.range(*lo, *hi).iter().map(|p| p.1).collect(),
            Filter::ClusterIn(s) => s
                .iter()
                .flat_map(|x| self.by_cluster[*x].iter().copied())
                .collect(),
            Filter::And(a, b) => {
                let (small, other) = if self.estimate(a, c.n()) <= self.estimate(b, c.n()) {
                    (a, b)
                } else {
                    (b, a)
                };
                self.resolve(c, small)
                    .into_iter()
                    .filter(|&i| other.matches(&c.rows[i]))
                    .collect()
            }
        }
    }
}
fn random_filter(rng: &mut Rng) -> Filter {
    let price = |rng: &mut Rng| {
        let width = (1000.0 * 10f64.powf(-3.0 * rng.unit())).max(1.0) as u32; // widths from 1 to 1000: selectivity 0.1%..100%
        let lo = rng.below(1000 - width as usize + 1) as u32;
        Filter::PriceRange(lo, lo + width - 1)
    };
    match rng.below(5) {
        0 => Filter::CatEq(((NCAT as f64).powf(rng.unit()) - 1.0) as usize % NCAT),
        1 => price(rng),
        2 => Filter::ClusterIn((0..1 + rng.below(4)).map(|_| rng.below(NCLUSTER)).collect()),
        3 => Filter::And(Box::new(Filter::CatEq(rng.below(8))), Box::new(price(rng))),
        _ => Filter::And(
            Box::new(Filter::ClusterIn(vec![
                rng.below(NCLUSTER),
                rng.below(NCLUSTER),
            ])),
            Box::new(price(rng)),
        ),
    }
}

// ---------- proximity graph (single layer, HNSW-style insert with the neighbour-selection heuristic) ----------
struct Graph {
    nbrs: Vec<Vec<usize>>,
    entry: usize,
    calls: Cell<u64>,
    cat_medoid: Vec<usize>,
    cluster_medoid: Vec<usize>,
}
impl Graph {
    fn d(&self, c: &Collection, q: &[f32], i: usize) -> f32 {
        self.calls.set(self.calls.get() + 1);
        l2(q, c.vec(i))
    }
    fn build(c: &Collection, m: usize, efc: usize) -> Self {
        let n = c.n();
        let mut mean = vec![0.0f32; DIM];
        for i in 0..n {
            for d in 0..DIM {
                mean[d] += c.vec(i)[d] / n as f32;
            }
        }
        let entry = (0..n)
            .min_by(|&a, &b| l2(&mean, c.vec(a)).total_cmp(&l2(&mean, c.vec(b))))
            .unwrap();
        let mut g = Graph {
            nbrs: vec![vec![]; n],
            entry,
            calls: Cell::new(0),
            cat_medoid: vec![],
            cluster_medoid: vec![],
        };
        let mut order: Vec<usize> = (0..n).collect();
        let mut rng = Rng(0x5eed);
        for i in (1..n).rev() {
            order.swap(i, rng.below(i + 1));
        }
        let mut inserted = vec![entry];
        for &id in order.iter().filter(|&&i| i != entry) {
            let found = g.search_in(c, c.vec(id), &|_| true, efc, &[entry]);
            let chosen = g.select(c, &found, m);
            for &nb in &chosen {
                g.nbrs[id].push(nb);
                g.nbrs[nb].push(id);
                if g.nbrs[nb].len() > 2 * m {
                    let mut cand: Vec<(f32, usize)> = g.nbrs[nb]
                        .iter()
                        .map(|&x| (l2(c.vec(nb), c.vec(x)), x))
                        .collect();
                    cand.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                    g.nbrs[nb] = g.select(c, &cand, 2 * m);
                }
            }
            inserted.push(id);
        }
        // medoid per label = member closest to the label's centroid: extra start nodes for correlated filters
        let medoid = |members: &[usize]| -> usize {
            let mut cen = vec![0.0f32; DIM];
            for &i in members {
                for d in 0..DIM {
                    cen[d] += c.vec(i)[d] / members.len() as f32;
                }
            }
            *members
                .iter()
                .min_by(|&&a, &&b| l2(&cen, c.vec(a)).total_cmp(&l2(&cen, c.vec(b))))
                .unwrap_or(&entry)
        };
        let ix = AttrIndex::build(c);
        g.cat_medoid = ix.by_cat.iter().map(|m| medoid(m)).collect();
        g.cluster_medoid = ix.by_cluster.iter().map(|m| medoid(m)).collect();
        g
    }
    fn select(&self, c: &Collection, cands: &[(f32, usize)], m: usize) -> Vec<usize> {
        let mut kept: Vec<(f32, usize)> = vec![];
        let mut dropped = vec![];
        for &(dq, e) in cands {
            if kept.len() >= m {
                break;
            }
            if kept.iter().any(|&(_, r)| l2(c.vec(e), c.vec(r)) < dq) {
                dropped.push(e)
            } else {
                kept.push((dq, e))
            }
        }
        let mut out: Vec<usize> = kept.into_iter().map(|k| k.1).collect();
        out.extend(dropped.into_iter().take(m - out.len()));
        out
    }
    /// Best-first search. `ok(i)` says whether node i may appear in the RESULT; every node may still be traversed.
    /// Returns up to `ef` (dist, id) nearest-first.
    fn search_in(
        &self,
        c: &Collection,
        q: &[f32],
        ok: &dyn Fn(usize) -> bool,
        ef: usize,
        entries: &[usize],
    ) -> Vec<(f32, usize)> {
        let mut seen = vec![false; c.n()];
        let mut cand: BinaryHeap<Reverse<(F, usize)>> = BinaryHeap::new();
        let mut res: BinaryHeap<(F, usize)> = BinaryHeap::new();
        for &e in entries {
            if !seen[e] {
                seen[e] = true;
                let d = F(self.d(c, q, e));
                cand.push(Reverse((d, e)));
                if ok(e) {
                    res.push((d, e));
                }
            }
        }
        while let Some(Reverse((d, u))) = cand.pop() {
            if res.len() >= ef && d > res.peek().unwrap().0 {
                break;
            }
            for &v in &self.nbrs[u] {
                if std::mem::replace(&mut seen[v], true) {
                    continue;
                }
                let dv = F(self.d(c, q, v));
                let worst_ok = res.len() < ef || dv < res.peek().unwrap().0;
                if worst_ok {
                    cand.push(Reverse((dv, v))); // bridges: non-matching nodes are expanded too, but only while competitive
                    if ok(v) {
                        res.push((dv, v));
                        if res.len() > ef {
                            res.pop();
                        }
                    }
                }
            }
        }
        let mut out: Vec<(f32, usize)> = res.into_iter().map(|(d, i)| (d.0, i)).collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        out
    }
    fn entries_for(&self, f: &Filter) -> Vec<usize> {
        let mut e = vec![self.entry];
        match f {
            Filter::CatEq(c) => e.push(self.cat_medoid[*c]),
            Filter::ClusterIn(s) => e.extend(s.iter().map(|c| self.cluster_medoid[*c])),
            Filter::And(a, b) => {
                e.extend(self.entries_for(a));
                e.extend(self.entries_for(b));
            }
            Filter::PriceRange(..) => {}
        }
        e.sort();
        e.dedup();
        e
    }
}

// ---------- the three plans; each returns (ids, distance computations) ----------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Plan {
    Pre,
    Post,
    In,
}
struct Engine<'a> {
    c: &'a Collection,
    ix: AttrIndex,
    g: Graph,
}
const K: usize = 10;
impl<'a> Engine<'a> {
    fn pre(&self, q: &[f32], f: &Filter, k: usize) -> (Vec<usize>, u64) {
        let ids = self.ix.resolve(self.c, f);
        let mut v: Vec<(f32, usize)> = ids.iter().map(|&i| (l2(q, self.c.vec(i)), i)).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        (
            v.into_iter().take(k).map(|p| p.1).collect(),
            ids.len() as u64,
        )
    }
    fn post(&self, q: &[f32], f: &Filter, k: usize, sel: f64) -> (Vec<usize>, u64) {
        let ef = ((2.0 * k as f64 / sel.max(1e-9)).ceil() as usize).clamp(k, 400); // over-fetch ~2k/s, capped
        self.g.calls.set(0);
        let found = self.g.search_in(self.c, q, &|_| true, ef, &[self.g.entry]);
        let ids = found
            .into_iter()
            .map(|p| p.1)
            .filter(|&i| f.matches(&self.c.rows[i]))
            .take(k)
            .collect();
        (ids, self.g.calls.get())
    }
    fn infilter(&self, q: &[f32], f: &Filter, k: usize, ef: usize) -> (Vec<usize>, u64) {
        self.g.calls.set(0);
        let found = self.g.search_in(
            self.c,
            q,
            &|i| f.matches(&self.c.rows[i]),
            ef.max(k),
            &self.g.entries_for(f),
        );
        (
            found.into_iter().take(k).map(|p| p.1).collect(),
            self.g.calls.get(),
        )
    }
    fn run(&self, plan: Plan, q: &[f32], f: &Filter, sel: f64) -> (Vec<usize>, u64) {
        match plan {
            Plan::Pre => self.pre(q, f, K),
            Plan::Post => self.post(q, f, K, sel),
            Plan::In => self.infilter(q, f, K, 60),
        }
    }
}

// ---------- planner with a calibrated cost table ----------
const BUCKETS: usize = 11;
/// A graph plan must look RISK times cheaper than the exact scan to be chosen: its cost varies per query (and per bucket edge),
/// while pre-filter cost is known exactly. Asymmetric risk is why real planners prefer the exact path near the crossover.
const RISK: f64 = 1.25;
fn bucket(sel: f64) -> usize {
    ((-sel.max(1e-6).log2()).floor().max(0.0) as usize).min(BUCKETS - 1)
}
/// Stats are kept per (predicate kind, selectivity bucket): graph cost depends on HOW the matches are spread (a correlated
/// cluster filter is cheap with a medoid start node; an independent price range is not), which selectivity alone cannot see.
const KINDS: usize = 5;
const KIND_NAMES: [&str; KINDS] = [
    "cat=",
    "price in",
    "cluster in",
    "cat&price",
    "cluster&price",
];
fn kind(f: &Filter) -> usize {
    match f {
        Filter::CatEq(_) => 0,
        Filter::PriceRange(..) => 1,
        Filter::ClusterIn(_) => 2,
        Filter::And(a, _) => {
            if matches!(**a, Filter::CatEq(_)) {
                3
            } else {
                4
            }
        }
    }
}
fn slot(f: &Filter, sel: f64) -> usize {
    kind(f) * BUCKETS + bucket(sel)
}
#[derive(Default, Clone)]
struct Stat {
    n: usize,
    cost: f64,
    feasible: usize,
}
struct Planner {
    post: Vec<Stat>,
    inf: Vec<Stat>,
    pre: Vec<Stat>,
}
impl Planner {
    fn calibrate(e: &Engine, rng: &mut Rng, queries: usize) -> Self {
        let mut p = Planner {
            post: vec![Stat::default(); BUCKETS * KINDS],
            inf: vec![Stat::default(); BUCKETS * KINDS],
            pre: vec![Stat::default(); BUCKETS * KINDS],
        };
        for _ in 0..queries {
            let (f, q) = (random_filter(rng), e.c.query_vec(rng));
            let sel = e.ix.resolve(e.c, &f).len() as f64 / e.c.n() as f64; // exact count (real systems: bitmap popcount)
            let (truth, pre_cost) = e.pre(&q, &f, K);
            if truth.is_empty() {
                continue;
            }
            let b = slot(&f, sel);
            for (plan, table) in [(Plan::Post, &mut p.post), (Plan::In, &mut p.inf)] {
                let (got, cost) = e.run(plan, &q, &f, sel);
                let s = &mut table[b];
                s.n += 1;
                s.cost += cost as f64;
                s.feasible += (recall(&got, &truth) >= 0.8) as usize;
            }
            let s = &mut p.pre[b];
            s.n += 1;
            s.cost += pre_cost as f64;
            s.feasible += 1;
        }
        p
    }
    fn avg(s: &Stat) -> Option<f64> {
        (s.n > 0).then(|| s.cost / s.n as f64)
    }
    /// A graph plan is eligible in a bucket when calibration saw it reach recall >= 0.8 on >= 90% of queries there.
    fn eligible(&self, plan: Plan, slot: usize) -> bool {
        let t = match plan {
            Plan::Pre => return true,
            Plan::Post => &self.post,
            Plan::In => &self.inf,
        };
        let s = &t[slot];
        s.n >= 3 && s.feasible as f64 >= 0.9 * s.n as f64
    }
    fn choose(&self, slot: usize, est_matches: f64) -> Plan {
        let mut best = (est_matches, Plan::Pre); // pre-filter cost is known exactly from the index counts
        for (plan, t) in [(Plan::Post, &self.post), (Plan::In, &self.inf)] {
            let s = &t[slot];
            if self.eligible(plan, slot) {
                let c = s.cost / s.n as f64;
                if c * RISK < best.0 {
                    best = (c, plan);
                }
            }
        }
        best.1
    }
}

// ---------- BM25 ----------
struct Bm25 {
    postings: HashMap<u32, Vec<(u32, u32)>>, // term -> [(doc, tf)] sorted by doc
    doc_len: Vec<u32>,
    avg_len: f64,
    n: usize,
}
const K1: f64 = 1.2;
const B: f64 = 0.75;
impl Bm25 {
    fn build(docs: &[Vec<u32>]) -> Self {
        let mut postings: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        for (d, toks) in docs.iter().enumerate() {
            let mut tf: HashMap<u32, u32> = HashMap::new();
            for t in toks {
                *tf.entry(*t).or_default() += 1;
            }
            for (t, c) in tf {
                postings.entry(t).or_default().push((d as u32, c));
            }
        }
        for p in postings.values_mut() {
            p.sort();
        }
        let doc_len: Vec<u32> = docs.iter().map(|d| d.len() as u32).collect();
        Bm25 {
            postings,
            avg_len: doc_len.iter().sum::<u32>() as f64 / docs.len() as f64,
            doc_len,
            n: docs.len(),
        }
    }
    fn idf(&self, df: usize) -> f64 {
        (1.0 + (self.n as f64 - df as f64 + 0.5) / (df as f64 + 0.5)).ln()
    }
    fn term_score(&self, tf: u32, df: usize, len: u32) -> f64 {
        let tf = tf as f64;
        self.idf(df) * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len as f64 / self.avg_len))
    }
    /// Term-at-a-time scoring into an accumulator array; returns (doc, score) for docs with score > 0, best first.
    fn search(&self, query: &[u32], k: usize) -> Vec<(usize, f64)> {
        let mut acc = vec![0.0f64; self.n];
        let mut uniq = query.to_vec();
        uniq.sort();
        uniq.dedup();
        for t in uniq {
            if let Some(p) = self.postings.get(&t) {
                for &(d, tf) in p {
                    acc[d as usize] += self.term_score(tf, p.len(), self.doc_len[d as usize]);
                }
            }
        }
        let mut v: Vec<(usize, f64)> = acc.into_iter().enumerate().filter(|x| x.1 > 0.0).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(k);
        v
    }
}
fn bm25_oracle(docs: &[Vec<u32>], query: &[u32]) -> Vec<f64> {
    // recomputes everything from raw token lists: no postings, no shared helper except the formula's definition
    let n = docs.len() as f64;
    let avg = docs.iter().map(|d| d.len()).sum::<usize>() as f64 / n;
    let mut uniq = query.to_vec();
    uniq.sort();
    uniq.dedup();
    docs.iter()
        .map(|d| {
            uniq.iter()
                .map(|t| {
                    let tf = d.iter().filter(|x| *x == t).count() as f64;
                    let df = docs.iter().filter(|dd| dd.contains(t)).count() as f64;
                    if tf == 0.0 {
                        0.0
                    } else {
                        (1.0 + (n - df + 0.5) / (df + 0.5)).ln() * tf * (K1 + 1.0)
                            / (tf + K1 * (1.0 - B + B * d.len() as f64 / avg))
                    }
                })
                .sum()
        })
        .collect()
}

// ---------- fusion ----------
const RRF_C: f64 = 60.0;
/// Reciprocal rank fusion over ranked id lists (best first). Ties are broken by smaller id so the result is deterministic.
fn fuse_rrf(lists: &[Vec<usize>], k: usize) -> Vec<usize> {
    let mut score: HashMap<usize, f64> = HashMap::new();
    for l in lists {
        for (rank, d) in l.iter().enumerate() {
            *score.entry(*d).or_default() += 1.0 / (RRF_C + rank as f64 + 1.0);
        }
    }
    let mut v: Vec<(usize, f64)> = score.into_iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(k).map(|p| p.0).collect()
}
/// Weighted fusion: min-max normalise each list's scores to [0,1], missing doc = 0, score = w*a + (1-w)*b.
fn fuse_weighted(a: &[(usize, f64)], b: &[(usize, f64)], w: f64, k: usize) -> Vec<usize> {
    let norm = |l: &[(usize, f64)]| -> HashMap<usize, f64> {
        let (hi, lo) = (
            l.first().map_or(1.0, |x| x.1),
            l.last().map_or(0.0, |x| x.1),
        );
        l.iter()
            .map(|(d, s)| (*d, if hi > lo { (s - lo) / (hi - lo) } else { 1.0 }))
            .collect()
    };
    let (na, nb) = (norm(a), norm(b));
    let mut ids: Vec<usize> = na.keys().chain(nb.keys()).copied().collect();
    ids.sort();
    ids.dedup();
    let mut v: Vec<(usize, f64)> = ids
        .into_iter()
        .map(|d| {
            (
                d,
                w * na.get(&d).unwrap_or(&0.0) + (1.0 - w) * nb.get(&d).unwrap_or(&0.0),
            )
        })
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(k).map(|p| p.0).collect()
}
fn ndcg_at(ranked: &[usize], relevant: &std::collections::HashSet<usize>, k: usize) -> f64 {
    let dcg: f64 = ranked
        .iter()
        .take(k)
        .enumerate()
        .filter(|(_, d)| relevant.contains(d))
        .map(|(i, _)| 1.0 / (i as f64 + 2.0).log2())
        .sum();
    let ideal: f64 = (0..relevant.len().min(k))
        .map(|i| 1.0 / (i as f64 + 2.0).log2())
        .sum();
    if ideal == 0.0 { 0.0 } else { dcg / ideal }
}

// ---------- synthetic hybrid corpus ----------
struct Corpus {
    docs: Vec<Vec<u32>>,
    vecs: Vec<Vec<f32>>,
    entity_of: Vec<usize>,
}
const TOPICS: usize = 20;
const ENTITIES: usize = 200;
const ENT_BASE: u32 = 10_000;
fn zipf(rng: &mut Rng, n: usize) -> usize {
    ((n as f64 + 1.0).powf(rng.unit()) - 1.0) as usize % n
}
fn topic_word(t: usize, w: usize) -> u32 {
    100 + (t * 30 + w) as u32
}
fn make_corpus(n: usize, rng: &mut Rng) -> (Corpus, Vec<f32>, Vec<f32>) {
    let topic_c: Vec<f32> = (0..TOPICS * DIM).map(|_| rng.gauss() * 2.0).collect();
    let ent_off: Vec<f32> = (0..ENTITIES * DIM).map(|_| rng.gauss() * 0.6).collect();
    let (mut docs, mut vecs, mut entity_of) = (vec![], vec![], vec![]);
    for _ in 0..n {
        let e = rng.below(ENTITIES);
        let t = e % TOPICS;
        let mut toks: Vec<u32> = (0..20).map(|_| zipf(rng, 100) as u32).collect(); // common words
        toks.extend((0..14).map(|_| topic_word(t, zipf(rng, 30))));
        toks.extend([ENT_BASE + e as u32, ENT_BASE + e as u32]);
        docs.push(toks);
        vecs.push(
            (0..DIM)
                .map(|d| topic_c[t * DIM + d] + ent_off[e * DIM + d] + rng.gauss() * 0.3)
                .collect(),
        );
        entity_of.push(e);
    }
    (
        Corpus {
            docs,
            vecs,
            entity_of,
        },
        topic_c,
        ent_off,
    )
}

fn part_filtered() {
    println!("=== PART 1: filtered ANN ===");
    let mut rng = Rng(0x872_872);
    let c = Collection::make(30_000, &mut rng);
    let t0 = std::time::Instant::now();
    let g = Graph::build(&c, 16, 80);
    let e = Engine {
        ix: AttrIndex::build(&c),
        g,
        c: &c,
    };
    println!(
        "graph built in {:.1}s (avg degree {:.1})",
        t0.elapsed().as_secs_f64(),
        e.g.nbrs.iter().map(|x| x.len()).sum::<usize>() as f64 / c.n() as f64
    );

    // (b) in-filter at high ef must match filtered brute force
    let mut rec = vec![];
    for _ in 0..150 {
        let (f, q) = (random_filter(&mut rng), c.query_vec(&mut rng));
        let (truth, _) = e.pre(&q, &f, K);
        if truth.len() < K {
            continue;
        }
        let (got, _) = e.infilter(&q, &f, K, 800);
        assert!(got.iter().all(|&i| f.matches(&c.rows[i])));
        rec.push(recall(&got, &truth));
    }
    let avg = rec.iter().sum::<f64>() / rec.len() as f64;
    println!(
        "in-filter at ef=800 vs filtered brute force: mean recall@10 {avg:.4} over {} filters",
        rec.len()
    );
    assert!(avg >= 0.97);

    // calibrate planner, then test on held-out queries
    let planner = Planner::calibrate(&e, &mut rng, 1500);
    println!(
        "\ncalibrated cost table (avg distance computations; feasible = recall>=0.8 on fraction):"
    );
    println!(
        "{:<14} {:>9} {:>5} {:>8} {:>12} {:>12} {:>8}",
        "predicate", "selectivity", "n", "pre", "post", "in-filter", "chosen"
    );
    for kd in 0..KINDS {
        for b in 0..BUCKETS {
            let i = kd * BUCKETS + b;
            let s = &planner.pre[i];
            if s.n < 8 {
                continue;
            }
            let f = |t: &Vec<Stat>| {
                format!(
                    "{:.0} ({:.0}%)",
                    t[i].cost / t[i].n.max(1) as f64,
                    100.0 * t[i].feasible as f64 / t[i].n.max(1) as f64
                )
            };
            let est = Planner::avg(s).unwrap();
            println!(
                "{:<14} {:>9} {:>5} {:>8.0} {:>12} {:>12} {:>8?}",
                KIND_NAMES[kd],
                format!("~2^-{b}"),
                s.n,
                est,
                f(&planner.post),
                f(&planner.inf),
                planner.choose(i, est)
            );
        }
    }
    let mut worst_hind = 0.0f64;
    let (mut worst_ratio, mut total_chosen, mut total_pre, mut recall_sum, mut cnt) =
        (0.0f64, 0.0, 0.0, 0.0, 0);
    let mut chosen_count: std::collections::BTreeMap<String, usize> = Default::default();
    for _ in 0..300 {
        let (f, q) = (random_filter(&mut rng), c.query_vec(&mut rng));
        let matches = e.ix.resolve(&c, &f).len() as f64;
        let sel = matches / c.n() as f64;
        let (truth, pre_cost) = e.pre(&q, &f, K);
        let mut costs = vec![];
        for plan in [Plan::Pre, Plan::Post, Plan::In] {
            let (got, cost) = e.run(plan, &q, &f, sel);
            assert!(
                got.iter().all(|&i| f.matches(&c.rows[i])),
                "{plan:?} returned a row that violates {f:?}"
            );
            assert!(got.len() <= K);
            let feasible = recall(&got, &truth) >= 0.8;
            costs.push((plan, cost, feasible, got));
        }
        if truth.is_empty() {
            continue;
        }
        // reference = cheapest plan the planner was ALLOWED to pick (eligible in this bucket) AND that worked for this query
        let best = costs
            .iter()
            .filter(|c| c.2 && planner.eligible(c.0, slot(&f, sel)))
            .map(|c| c.1)
            .min()
            .unwrap()
            .max(1) as f64;
        let hindsight = costs
            .iter()
            .filter(|c| c.2)
            .map(|c| c.1)
            .min()
            .unwrap()
            .max(1) as f64;
        let pick = planner.choose(slot(&f, sel), matches);
        *chosen_count.entry(format!("{pick:?}")).or_default() += 1;
        let (_, mut cost, feasible, got) = costs.iter().find(|c| c.0 == pick).cloned().unwrap();
        if got.len() < K.min(truth.len()) && pick != Plan::Pre {
            cost += pre_cost; // engine notices the short result and falls back to exact pre-filtering
        }
        let _ = feasible;
        worst_ratio = worst_ratio.max(cost as f64 / best);
        worst_hind = worst_hind.max(cost as f64 / hindsight);
        total_chosen += cost as f64;
        total_pre += pre_cost as f64;
        recall_sum += recall(&got, &truth);
        cnt += 1;
    }
    println!(
        "\nplanner on 300 held-out queries: picks {chosen_count:?}; worst cost ratio vs best eligible plan = {worst_ratio:.2}x (vs hindsight oracle {worst_hind:.1}x, printed only); mean cost {:.0} vs always-pre {:.0}; mean recall of chosen {:.3}",
        total_chosen / cnt as f64,
        total_pre / cnt as f64,
        recall_sum / cnt as f64
    );
    assert!(
        worst_ratio <= 3.0,
        "planner picked a plan {worst_ratio:.2}x worse than the best feasible one"
    );
    assert!(
        total_chosen < total_pre,
        "planning should beat always-pre-filter on average"
    );
    assert!(recall_sum / cnt as f64 >= 0.9);
}

fn rrf_properties(rng: &mut Rng) {
    // missing docs contribute 0: a doc in both lists beats a doc only in one at the same rank
    let f = fuse_rrf(&[vec![1, 2], vec![3, 1]], 3);
    assert_eq!(f[0], 1);
    // ties broken by id: ranks (1 in A) vs (1 in B) give equal scores
    assert_eq!(fuse_rrf(&[vec![9], vec![4]], 2), vec![4, 9]);
    // exact value check
    let s = 1.0 / (RRF_C + 1.0) + 1.0 / (RRF_C + 2.0);
    let lists = [vec![7usize, 5], vec![5, 7]];
    assert_eq!(fuse_rrf(&lists, 1), vec![5]); // 5: rank2+rank1 == 7: rank1+rank2 (tie) -> smaller id
    assert!((s - (1.0 / 61.0 + 1.0 / 62.0)).abs() < 1e-15);
    for _ in 0..200 {
        let mk = |rng: &mut Rng| {
            let mut ids: Vec<usize> = (0..30).collect();
            for i in (1..ids.len()).rev() {
                ids.swap(i, rng.below(i + 1));
            }
            ids.truncate(5 + rng.below(20));
            ids
        };
        let (a, b) = (mk(rng), mk(rng));
        assert_eq!(
            fuse_rrf(&[a.clone(), b.clone()], 50),
            fuse_rrf(&[b.clone(), a.clone()], 50),
            "RRF must be symmetric"
        );
        assert_eq!(
            fuse_rrf(&[a.clone()], 50),
            a,
            "a single list keeps its order"
        );
        assert_eq!(
            fuse_rrf(&[a.clone(), a.clone()], 50),
            a,
            "duplicating a list changes no ranking"
        );
        let f = fuse_rrf(&[a.clone(), b.clone()], 100);
        let mut u: Vec<usize> = a.iter().chain(&b).copied().collect();
        u.sort();
        u.dedup();
        let mut fs = f.clone();
        fs.sort();
        assert_eq!(fs, u, "fusion returns exactly the union of the input lists");
    }
}

fn part_hybrid() {
    println!("\n=== PART 2: hybrid BM25 + vector ===");
    let mut rng = Rng(0x8721);
    let (corpus, topic_c, ent_off) = make_corpus(4000, &mut rng);
    let bm = Bm25::build(&corpus.docs);
    // BM25 against the naive oracle
    for _ in 0..40 {
        let q: Vec<u32> = (0..3)
            .map(|_| {
                if rng.below(2) == 0 {
                    zipf(&mut rng, 100) as u32
                } else {
                    ENT_BASE + rng.below(ENTITIES) as u32
                }
            })
            .collect();
        let want = bm25_oracle(&corpus.docs[..300], &q);
        let small = Bm25::build(&corpus.docs[..300]);
        let got = small.search(&q, 300);
        let got_map: HashMap<usize, f64> = got.into_iter().collect();
        for (d, w) in want.iter().enumerate() {
            let g = got_map.get(&d).copied().unwrap_or(0.0);
            assert!(
                (g - w).abs() <= 1e-9 * (1.0 + w.abs()),
                "bm25 mismatch doc {d}: {g} vs {w}"
            );
        }
    }
    rrf_properties(&mut rng);
    println!(
        "BM25 equals the per-document oracle on 40 queries; RRF properties hold on 200 random list pairs"
    );

    let nq = 300;
    let k = 50;
    let (mut s_bm, mut s_dn, mut s_rrf) = ([0.0f64; 2], [0.0f64; 2], [0.0f64; 2]);
    let ws = [0.0, 0.2, 0.35, 0.5, 0.65, 0.8, 1.0];
    let mut s_w = vec![0.0f64; ws.len()];
    for _ in 0..nq {
        let e = rng.below(ENTITIES);
        let t = e % TOPICS;
        let relevant: std::collections::HashSet<usize> = (0..corpus.docs.len())
            .filter(|&d| corpus.entity_of[d] == e)
            .collect();
        // lexical part: entity word dropped 40% of the time (replaced by a common word)
        let ent_tok = if rng.unit() < 0.4 {
            zipf(&mut rng, 100) as u32
        } else {
            ENT_BASE + e as u32
        };
        let qtext = vec![
            ent_tok,
            topic_word(t, zipf(&mut rng, 30)),
            topic_word(t, zipf(&mut rng, 30)),
        ];
        // vector part: noisy 30% of the time
        let sigma = 1.5 + 0.8 * rng.unit() as f32; // always somewhat noisy: relevant docs land at mid ranks
        let qvec: Vec<f32> = (0..DIM)
            .map(|d| topic_c[t * DIM + d] + ent_off[e * DIM + d] + rng.gauss() * sigma)
            .collect();
        let lex = bm.search(&qtext, 100);
        let mut dense: Vec<(usize, f64)> = (0..corpus.docs.len())
            .map(|d| (d, -(l2(&qvec, &corpus.vecs[d]) as f64)))
            .collect();
        dense.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        dense.truncate(100);
        let (lex_ids, dense_ids): (Vec<usize>, Vec<usize>) = (
            lex.iter().map(|p| p.0).collect(),
            dense.iter().map(|p| p.0).collect(),
        );
        let rrf = fuse_rrf(&[lex_ids.clone(), dense_ids.clone()], k);
        let acc = |m: &mut [f64; 2], ids: &[usize]| {
            m[0] += ndcg_at(ids, &relevant, 10) / nq as f64;
            m[1] += recall(
                &ids[..ids.len().min(k)],
                &relevant.iter().copied().collect::<Vec<_>>(),
            ) / nq as f64;
        };
        acc(&mut s_bm, &lex_ids);
        acc(&mut s_dn, &dense_ids);
        acc(&mut s_rrf, &rrf);
        for (i, w) in ws.iter().enumerate() {
            s_w[i] += ndcg_at(&fuse_weighted(&dense, &lex, *w, k), &relevant, 10) / nq as f64;
        }
    }
    println!("{:<28} {:>9} {:>10}", "retriever", "nDCG@10", "recall@50");
    println!("{:<28} {:>9.3} {:>10.3}", "BM25 only", s_bm[0], s_bm[1]);
    println!("{:<28} {:>9.3} {:>10.3}", "dense only", s_dn[0], s_dn[1]);
    println!("{:<28} {:>9.3} {:>10.3}", "RRF (c=60)", s_rrf[0], s_rrf[1]);
    for (w, s) in ws.iter().zip(&s_w) {
        println!("{:<28} {:>9.3}", format!("weighted, w_dense={w}"), s);
    }
    assert!(
        s_rrf[0] > s_bm[0].max(s_dn[0]),
        "fusion should beat both single retrievers on nDCG@10"
    );
    assert!(s_rrf[1] >= s_bm[1].max(s_dn[1]) - 1e-9);
    assert!(
        (s_w[0] - s_bm[0]).abs() < 0.06 && (s_w[ws.len() - 1] - s_dn[0]).abs() < 0.06,
        "w=0 and w=1 reduce to a single retriever (up to the padding of lists)"
    );
    let best_w = s_w.iter().cloned().fold(0.0, f64::max);
    assert!(best_w > s_bm[0].max(s_dn[0]));
}

fn main() {
    part_filtered();
    part_hybrid();
    println!("\nall filtered-ANN and hybrid-search checks passed");
}
