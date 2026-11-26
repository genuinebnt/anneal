//! RECIPE 875 (vectors): evaluating ANN indexes honestly: recall definitions, Pareto curves, tail latency, dataset difficulty (contrast, LID, hubness), out-of-distribution queries.
//! LEVEL: intermediate
//! Use in: choosing or tuning a vector index, reading vendor benchmarks, regression-testing recall in CI.   Run: cargo run --release --example ann_evaluation_harness
#![allow(dead_code)]
//!
//! THE QUESTION a benchmark must answer is not "how fast is index X" but "at the recall I need, on data like mine, what does a query cost,
//! what does the index cost to build and to hold, and how bad is the slowest 1%". A speed number without a recall number means nothing,
//! because every ANN index can be made arbitrarily fast by returning worse answers.
//!
//! THREE RECALL DEFINITIONS (all in this file, all checked on hand-computed cases):
//!   * ID RECALL: |returned ids intersect true ids| / k. Simple, but WRONG under ties: with duplicate or equidistant vectors there are many
//!     correct answer sets and an index is punished for picking a different one.
//!   * DISTANCE-THRESHOLD RECALL (what ann-benchmarks uses): count returned items whose distance is <= the true k-th distance (+ epsilon).
//!     Tie-safe. Requires the returned DISTANCES to be recomputed or trusted: an index that reports approximate distances (PQ, SQ) must be
//!     scored with exact distances of the ids it returned, which is what `evaluate` does.
//!   * APPROXIMATION RATIO: mean over ranks i of dist(returned_i) / dist(true_i) with TRUE (not squared) distances. 1.0 is perfect. It says
//!     how much worse the misses are: recall 0.5 with ratio 1.01 may be fine for a recommender, recall 0.5 with ratio 3 is not. It hides
//!     short results, so the count of missing results is reported next to it.
//!
//! METHODOLOGY (the ann-benchmarks method, reproduced in miniature): sweep each index's QUERY-TIME knob (nprobe, ef) on a fixed build,
//! plot (recall, queries per second) and keep only the PARETO FRONTIER: points for which no other point has both higher-or-equal recall and
//! lower-or-equal cost, with one strictly better. Compare indexes by their frontiers, never by one hand-picked setting each. Report build
//! time and memory beside the curve. Here the frontier is computed on DISTANCE COMPUTATIONS PER QUERY, which is deterministic and
//! machine-independent; QPS and p50/p95/p99 latency are PRINTED ONLY. Distance counts are a proxy, not speed: they ignore cache misses
//! (graph traversal jumps through memory, a scan streams), so two indexes with equal counts can differ severalfold in wall-clock time.
//! TAIL LATENCY: report percentiles, not means. A query fanned out to 100 shards waits for the slowest, so with independent shards the chance
//! that at least one is above its own p99 is 1 - 0.99^100 = 63%.
//!
//! DATASET DIFFICULTY. Results do not transfer between datasets, and these numbers say why:
//!   * DISTANCE CONCENTRATION: in high dimension, for i.i.d. coordinates, (d_max - d_min)/d_min shrinks: the "nearest" neighbour is hardly
//!     nearer than a random point (Beyer et al., "When Is Nearest Neighbor Meaningful?").
//!   * RELATIVE CONTRAST (He, Kumar, Chang, "On the Difficulty of Nearest Neighbor Search"): mean distance / nearest distance. Near 1 = hard.
//!   * LOCAL INTRINSIC DIMENSIONALITY: how fast the number of points grows with the radius around a query. Maximum-likelihood estimate from
//!     the k nearest distances r_1 <= ... <= r_k:  LID = -1 / mean_i ln(r_i / r_k). It measures the manifold, not the ambient dimension: 8-dim
//!     data embedded in 64 dims scores like 8-dim data. The estimator is noisy and tends to read LOW when the true dimension is high.
//!   * HUBNESS (Radovanovic et al., "Hubs in Space"): N_k(x) = how many points list x among their k nearest. In high intrinsic dimension
//!     the distribution of N_k gets a long right tail: a few "hubs" appear in very many neighbour lists and some points in none.
//!   * OUT-OF-DISTRIBUTION QUERIES (text queries against image embeddings is the classic case): the index was built (k-means centroids, graph
//!     edges) for the data distribution, so recall measured on in-distribution queries need not transfer. It is NOT automatically lower: a mild
//!     shift can leave recall unchanged or raise it (the run prints one). Tune on queries drawn like PRODUCTION queries and measure.
//!
//! VERIFIED: recall, ratio and percentile functions on hand-computed cases; metamorphic properties on random inputs (replacing a wrong
//! result by a missing true one never lowers recall, result order does not matter, an exact answer scores 1); ties: with duplicated
//! vectors id recall is below 1 while distance recall is exactly 1; the Pareto function against an O(n^2) oracle and its invariants;
//! brute force has recall exactly 1.0; IVF-lite recall is non-decreasing in nprobe per query and 1.0 at nprobe = nlist; the HNSW-lite
//! reaches recall >= 0.95 on the low-intrinsic-dimension set at fewer than 25% of the brute-force distance computations; relative contrast
//! falls and hubness skew rises with dimension on uniform data; LID recovers the dimension of a 2-dim and 8-dim manifold embedded in 64
//! dims; the uniform set has higher LID than the manifold set and more than 0.2 lower IVF recall at nprobe = 4; queries drawn around UNSEEN
//! cluster centres lose more than 0.1 IVF recall at nprobe 1 and 2.
//! LIMITS: n is a few thousand, so every number is about the METHOD, not about which index wins at scale. The last two claims are
//! observations on these seeded datasets, not theorems.
//!
//! COMMON USES: ann-benchmarks and VectorDBBench reports, recall regression tests in Lucene/Qdrant/Weaviate CI, choosing `ef_search` /
//! `nprobe` for a service-level objective, shadow evaluation of a new index against exact search on sampled production queries.
//! Read: ann-benchmarks https://ann-benchmarks.com and its paper, Aumueller, Bernhardsson, Faithfull https://arxiv.org/abs/1807.05614 ;
//! HNSW, Malkov and Yashunin https://arxiv.org/abs/1603.09320 ; cited by title only: Beyer et al., "When Is Nearest Neighbor Meaningful?" (ICDT 1999);
//! He, Kumar and Chang, "On the Difficulty of Nearest Neighbor Search" (ICML 2012); Radovanovic, Nanopoulos and Ivanovic, "Hubs in Space"
//! (JMLR 2010); Amsaleg et al., "Estimating Local Intrinsic Dimensionality" (KDD 2015); Dean and Barroso, "The Tail at Scale" (CACM 2013).
//!
//! TRY NEXT:
//!   - [S] Predict, then measure: change k from 10 to 1 and to 100. Which index's frontier moves most?
//!   - [S] Add a "recall 0.99" column to the "cost to reach a target recall" table. How much more does the last 4% cost than the first 95%?
//!   - [M] Add a third index to the harness by implementing `AnnIndex` for a random-hyperplane LSH, without changing `sweep` or `pareto`.
//!   - [M] Replace `ratio` with the tie-aware nDCG of the returned list and find a pair of result lists that the two metrics rank differently.
//!   - [A] PROJECT: point this harness at the indexes in `scalar_binary_quantization_rescoring` and `spann_disk_ivf_lite`; add a second cost
//!     axis (bytes read per query) and compute a three-way Pareto frontier.
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashSet};
use std::time::Instant;

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
#[derive(Clone)]
struct Mat {
    n: usize,
    dim: usize,
    v: Vec<f32>,
}
impl Mat {
    fn row(&self, i: usize) -> &[f32] {
        &self.v[i * self.dim..(i + 1) * self.dim]
    }
    /// First `n` rows and the rest, so that data and queries come from ONE generator call (same distribution).
    fn split(self, n: usize) -> (Mat, Mat) {
        let dim = self.dim;
        let (a, b) = self.v.split_at(n * dim);
        (
            Mat {
                n,
                dim,
                v: a.to_vec(),
            },
            Mat {
                n: self.n - n,
                dim,
                v: b.to_vec(),
            },
        )
    }
}
fn clusters(n: usize, dim: usize, k: usize, centre_scale: f32, noise: f32, rng: &mut Rng) -> Mat {
    let centres: Vec<f32> = (0..k * dim).map(|_| rng.gauss() * centre_scale).collect();
    let mut v = Vec::with_capacity(n * dim);
    for _ in 0..n {
        let c = rng.below(k);
        v.extend((0..dim).map(|d| centres[c * dim + d] + rng.gauss() * noise));
    }
    Mat { n, dim, v }
}
fn uniform(n: usize, dim: usize, rng: &mut Rng) -> Mat {
    Mat {
        n,
        dim,
        v: (0..n * dim)
            .map(|_| rng.unit() as f32 * 2.0 - 1.0)
            .collect(),
    }
}
/// `latent`-dimensional uniform cube embedded in `dim` dimensions by a random linear map with ORTHONORMAL columns (an isometry),
/// so the ambient dimension is `dim` but the geometry is exactly that of the cube.
fn manifold(n: usize, dim: usize, latent: usize, rng: &mut Rng) -> Mat {
    let mut basis: Vec<Vec<f32>> = Vec::with_capacity(latent);
    while basis.len() < latent {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gauss()).collect();
        for b in &basis {
            let p: f32 = v.iter().zip(b).map(|(x, y)| x * y).sum();
            v.iter_mut().zip(b).for_each(|(x, y)| *x -= p * y);
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-3 {
            v.iter_mut().for_each(|x| *x /= norm);
            basis.push(v);
        }
    }
    let mut out = Vec::with_capacity(n * dim);
    for _ in 0..n {
        let z: Vec<f32> = (0..latent).map(|_| rng.unit() as f32 * 2.0 - 1.0).collect();
        out.extend((0..dim).map(|d| (0..latent).map(|j| basis[j][d] * z[j]).sum::<f32>()));
    }
    Mat { n, dim, v: out }
}
/// Squared Euclidean distance.
fn l2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
#[derive(PartialEq, Clone, Copy, Debug)]
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
/// (squared distance, id), nearest first, ties by id.
type Hits = Vec<(f32, u32)>;
fn sort_hits(v: &mut Hits) {
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
}
fn exact_topk(q: &[f32], data: &Mat, k: usize) -> Hits {
    let mut v: Hits = (0..data.n)
        .map(|i| (l2(q, data.row(i)), i as u32))
        .collect();
    sort_hits(&mut v);
    v.truncate(k);
    v
}

// ---------- metrics ----------
/// |returned ids intersect true ids| / |true|. Duplicate returned ids count once.
fn id_recall(got: &[u32], truth: &[u32]) -> f64 {
    let t: HashSet<u32> = truth.iter().copied().collect();
    let g: HashSet<u32> = got.iter().copied().collect();
    g.intersection(&t).count() as f64 / t.len() as f64
}
/// ann-benchmarks style: fraction of k filled by DISTINCT returned items no farther than the true k-th neighbour.
/// `got` must carry EXACT distances.
fn distance_recall(got: &Hits, truth: &Hits, eps: f32) -> f64 {
    let kth = truth.last().expect("non-empty truth").0;
    let mut seen = HashSet::new();
    let good = got
        .iter()
        .filter(|(d, id)| *d <= kth * (1.0 + eps) + eps * 1e-6 && seen.insert(*id))
        .count();
    good.min(truth.len()) as f64 / truth.len() as f64
}
/// Mean of dist(got_i)/dist(true_i) over the ranks that were returned (true distances, so sqrt of the squared ones).
/// A zero true distance counts as ratio 1 when matched exactly, and the rank is skipped otherwise (the ratio is undefined).
fn approx_ratio(got: &Hits, truth: &Hits) -> f64 {
    let mut g = got.clone();
    sort_hits(&mut g);
    let (mut sum, mut cnt) = (0.0f64, 0usize);
    for (a, b) in g.iter().zip(truth) {
        let (a, b) = ((a.0 as f64).sqrt(), (b.0 as f64).sqrt());
        if b > 0.0 {
            sum += a / b;
            cnt += 1;
        } else if a == 0.0 {
            sum += 1.0;
            cnt += 1;
        }
    }
    if cnt == 0 { 1.0 } else { sum / cnt as f64 }
}
/// Nearest-rank percentile: the smallest value with at least p% of the sample at or below it.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    assert!(!sorted.is_empty() && p > 0.0 && p <= 100.0);
    // the small subtraction stops 99.0 * 100 / 100 = 99.00000000000001 from rounding up a whole rank
    let rank = (p * sorted.len() as f64 / 100.0 - 1e-9).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[derive(Clone, Debug)]
struct Point {
    label: String,
    recall: f64,
    /// lower is better
    cost: f64,
}
/// Indices of the non-dominated points (maximise recall, minimise cost), cheapest first. Of exact duplicates only the first is kept.
fn pareto(points: &[Point]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..points.len()).collect();
    // cheapest first; among equal cost the best recall first, so one forward sweep with a running maximum is enough
    order.sort_by(|&a, &b| {
        points[a]
            .cost
            .total_cmp(&points[b].cost)
            .then(points[b].recall.total_cmp(&points[a].recall))
            .then(a.cmp(&b))
    });
    let mut best = f64::NEG_INFINITY;
    order.retain(|&i| {
        let keep = points[i].recall > best;
        if keep {
            best = points[i].recall;
        }
        keep
    });
    order
}
fn dominates(a: &Point, b: &Point) -> bool {
    a.recall >= b.recall && a.cost <= b.cost && (a.recall > b.recall || a.cost < b.cost)
}

// ---------- the index interface ----------
/// Everything the harness needs from an index. One query-time knob (`effort`) is enough for every index here; `efforts` lists the sweep.
/// The trait is object safe, so the harness holds `Box<dyn AnnIndex>` and a new index needs no harness change.
trait AnnIndex {
    fn name(&self) -> &'static str;
    fn efforts(&self) -> Vec<usize>;
    /// Returns up to k hits (nearest first) and the number of vector-vector distance computations spent.
    fn search(&self, data: &Mat, q: &[f32], k: usize, effort: usize) -> (Hits, usize);
    /// Bytes held by the index ON TOP OF the raw vectors.
    fn extra_bytes(&self) -> usize;
}

struct Brute;
impl AnnIndex for Brute {
    fn name(&self) -> &'static str {
        "brute"
    }
    fn efforts(&self) -> Vec<usize> {
        vec![0]
    }
    fn search(&self, data: &Mat, q: &[f32], k: usize, _effort: usize) -> (Hits, usize) {
        (exact_topk(q, data, k), data.n)
    }
    fn extra_bytes(&self) -> usize {
        0
    }
}

/// IVF-Flat: k-means cells; probe the `effort` nearest cells and scan them.
struct IvfLite {
    cents: Mat,
    lists: Vec<Vec<u32>>,
}
impl IvfLite {
    fn build(data: &Mat, nlist: usize, iters: usize, rng: &mut Rng) -> Self {
        // distinct starting rows (partial Fisher-Yates)
        let mut ids: Vec<usize> = (0..data.n).collect();
        for i in 0..nlist {
            ids.swap(i, i + rng.below(data.n - i));
        }
        let dim = data.dim;
        let mut cents = Mat {
            n: nlist,
            dim,
            v: ids[..nlist]
                .iter()
                .flat_map(|&i| data.row(i).to_vec())
                .collect(),
        };
        let nearest = |c: &Mat, x: &[f32]| {
            (0..c.n)
                .min_by(|&a, &b| l2(x, c.row(a)).total_cmp(&l2(x, c.row(b))))
                .unwrap()
        };
        for _ in 0..iters {
            let (mut sum, mut cnt) = (vec![0.0f32; nlist * dim], vec![0usize; nlist]);
            for i in 0..data.n {
                let c = nearest(&cents, data.row(i));
                cnt[c] += 1;
                sum[c * dim..(c + 1) * dim]
                    .iter_mut()
                    .zip(data.row(i))
                    .for_each(|(s, x)| *s += x);
            }
            for c in 0..nlist {
                if cnt[c] > 0 {
                    for d in 0..dim {
                        cents.v[c * dim + d] = sum[c * dim + d] / cnt[c] as f32;
                    }
                }
            }
        }
        let mut lists = vec![vec![]; nlist];
        for i in 0..data.n {
            lists[nearest(&cents, data.row(i))].push(i as u32);
        }
        IvfLite { cents, lists }
    }
}
impl AnnIndex for IvfLite {
    fn name(&self) -> &'static str {
        "ivf-lite"
    }
    fn efforts(&self) -> Vec<usize> {
        [1, 2, 4, 8, 16, 32, self.cents.n]
            .into_iter()
            .filter(|&e| e <= self.cents.n)
            .collect()
    }
    fn search(&self, data: &Mat, q: &[f32], k: usize, effort: usize) -> (Hits, usize) {
        let order = exact_topk(q, &self.cents, effort);
        let mut hits = vec![];
        for &(_, c) in &order {
            hits.extend(
                self.lists[c as usize]
                    .iter()
                    .map(|&i| (l2(q, data.row(i as usize)), i)),
            );
        }
        // the centroid ranking is part of the query cost: nlist distance computations, whatever nprobe is
        let cost = self.cents.n + hits.len();
        sort_hits(&mut hits);
        hits.truncate(k);
        (hits, cost)
    }
    fn extra_bytes(&self) -> usize {
        self.cents.v.len() * 4 + self.lists.iter().map(|l| l.len() * 4).sum::<usize>()
    }
}

/// A small HNSW: geometric level assignment, greedy descent through the upper layers, best-first search with `ef` at layer 0,
/// neighbour selection by the diversity heuristic (keep a candidate only if it is closer to the new node than to any kept neighbour).
struct HnswLite {
    /// links[node][layer]
    links: Vec<Vec<Vec<u32>>>,
    entry: u32,
    m: usize,
}
impl HnswLite {
    fn build(data: &Mat, m: usize, efc: usize, rng: &mut Rng) -> Self {
        let mut h = HnswLite {
            links: Vec::with_capacity(data.n),
            entry: 0,
            m,
        };
        let level_mult = 1.0 / (m as f64).ln();
        for id in 0..data.n {
            let level = (-rng.unit().ln() * level_mult) as usize;
            h.links.push(vec![vec![]; level + 1]);
            if id == 0 {
                continue;
            }
            let q = data.row(id);
            let top = h.links[h.entry as usize].len() - 1;
            let mut cost = 0;
            let mut ep = h.entry;
            for layer in (level + 1..=top).rev() {
                ep = h.greedy(data, q, ep, layer, &mut cost);
            }
            for layer in (0..=level.min(top)).rev() {
                let found = h.search_layer(data, q, ep, efc, layer, &mut cost);
                ep = found[0].1;
                let cap = if layer == 0 { 2 * m } else { m };
                let chosen = h.select(data, &found, m);
                h.links[id][layer] = chosen.clone();
                for nb in chosen {
                    let nb = nb as usize;
                    h.links[nb][layer].push(id as u32);
                    if h.links[nb][layer].len() > cap {
                        let mut c: Hits = h.links[nb][layer]
                            .iter()
                            .map(|&x| (l2(data.row(nb), data.row(x as usize)), x))
                            .collect();
                        sort_hits(&mut c);
                        h.links[nb][layer] = h.select(data, &c, cap);
                    }
                }
            }
            if level > top {
                h.entry = id as u32;
            }
        }
        h
    }
    fn select(&self, data: &Mat, cands: &Hits, m: usize) -> Vec<u32> {
        let (mut kept, mut dropped): (Vec<u32>, Vec<u32>) = (vec![], vec![]);
        for &(dq, e) in cands {
            if kept.len() >= m {
                break;
            }
            if kept
                .iter()
                .any(|&r| l2(data.row(e as usize), data.row(r as usize)) < dq)
            {
                dropped.push(e)
            } else {
                kept.push(e)
            }
        }
        let room = m - kept.len();
        kept.extend(dropped.into_iter().take(room));
        kept
    }
    fn greedy(&self, data: &Mat, q: &[f32], mut cur: u32, layer: usize, cost: &mut usize) -> u32 {
        let mut best = l2(q, data.row(cur as usize));
        *cost += 1;
        loop {
            let mut moved = false;
            for &n in &self.links[cur as usize][layer] {
                let d = l2(q, data.row(n as usize));
                *cost += 1;
                if d < best {
                    (best, cur, moved) = (d, n, true);
                }
            }
            if !moved {
                return cur;
            }
        }
    }
    fn search_layer(
        &self,
        data: &Mat,
        q: &[f32],
        ep: u32,
        ef: usize,
        layer: usize,
        cost: &mut usize,
    ) -> Hits {
        let mut seen = vec![false; data.n];
        seen[ep as usize] = true;
        let d0 = F(l2(q, data.row(ep as usize)));
        *cost += 1;
        let mut cand = BinaryHeap::from([Reverse((d0, ep))]);
        let mut res = BinaryHeap::from([(d0, ep)]);
        while let Some(Reverse((d, u))) = cand.pop() {
            if res.len() >= ef && d > res.peek().unwrap().0 {
                break;
            }
            for &v in &self.links[u as usize][layer] {
                if std::mem::replace(&mut seen[v as usize], true) {
                    continue;
                }
                let dv = F(l2(q, data.row(v as usize)));
                *cost += 1;
                if res.len() < ef || dv < res.peek().unwrap().0 {
                    cand.push(Reverse((dv, v)));
                    res.push((dv, v));
                    if res.len() > ef {
                        res.pop();
                    }
                }
            }
        }
        let mut out: Hits = res.into_iter().map(|(d, i)| (d.0, i)).collect();
        sort_hits(&mut out);
        out
    }
}
impl AnnIndex for HnswLite {
    fn name(&self) -> &'static str {
        "hnsw-lite"
    }
    fn efforts(&self) -> Vec<usize> {
        vec![10, 20, 40, 80, 160, 320]
    }
    fn search(&self, data: &Mat, q: &[f32], k: usize, effort: usize) -> (Hits, usize) {
        let mut cost = 0;
        let mut ep = self.entry;
        for layer in (1..self.links[self.entry as usize].len()).rev() {
            ep = self.greedy(data, q, ep, layer, &mut cost);
        }
        let mut r = self.search_layer(data, q, ep, effort.max(k), 0, &mut cost);
        r.truncate(k);
        (r, cost)
    }
    fn extra_bytes(&self) -> usize {
        self.links.iter().flatten().map(|l| l.len() * 4).sum()
    }
}

// ---------- the harness ----------
#[derive(Clone)]
struct Measurement {
    index: &'static str,
    effort: usize,
    id_recall: f64,
    dist_recall: f64,
    ratio: f64,
    missing: usize,
    dist_comps: f64,
    /// printed only
    qps: f64,
    /// microseconds; printed only
    p50: f64,
    p95: f64,
    p99: f64,
    per_query_recall: Vec<f64>,
}
fn ground_truth(data: &Mat, queries: &Mat, k: usize) -> Vec<Hits> {
    (0..queries.n)
        .map(|i| exact_topk(queries.row(i), data, k))
        .collect()
}
fn evaluate(
    ix: &dyn AnnIndex,
    data: &Mat,
    queries: &Mat,
    truth: &[Hits],
    k: usize,
    effort: usize,
) -> Measurement {
    let (mut lat, mut results, mut comps) = (Vec::with_capacity(queries.n), vec![], 0usize);
    let start = Instant::now();
    for i in 0..queries.n {
        let t = Instant::now();
        let (hits, c) = ix.search(data, queries.row(i), k, effort);
        lat.push(t.elapsed().as_secs_f64() * 1e6);
        comps += c;
        results.push(hits);
    }
    let total = start.elapsed().as_secs_f64();
    lat.sort_by(f64::total_cmp);
    // scoring happens outside the timed loop, with distances recomputed from the ids: never trust an index's own distances
    let (mut idr, mut ratio, mut missing, mut per) = (0.0, 0.0, 0, vec![]);
    let mut dr_sum = 0.0;
    for (i, hits) in results.iter().enumerate() {
        let exact: Hits = hits
            .iter()
            .map(|&(_, id)| (l2(queries.row(i), data.row(id as usize)), id))
            .collect();
        let ids: Vec<u32> = exact.iter().map(|h| h.1).collect();
        let tids: Vec<u32> = truth[i].iter().map(|h| h.1).collect();
        idr += id_recall(&ids, &tids);
        let dr = distance_recall(&exact, &truth[i], 0.0);
        dr_sum += dr;
        per.push(dr);
        ratio += approx_ratio(&exact, &truth[i]);
        missing += truth[i].len().saturating_sub(exact.len());
    }
    let nq = queries.n as f64;
    Measurement {
        index: ix.name(),
        effort,
        id_recall: idr / nq,
        dist_recall: dr_sum / nq,
        ratio: ratio / nq,
        missing,
        dist_comps: comps as f64 / nq,
        qps: nq / total.max(1e-12),
        p50: percentile(&lat, 50.0),
        p95: percentile(&lat, 95.0),
        p99: percentile(&lat, 99.0),
        per_query_recall: per,
    }
}
fn sweep(
    ix: &dyn AnnIndex,
    data: &Mat,
    queries: &Mat,
    truth: &[Hits],
    k: usize,
) -> Vec<Measurement> {
    ix.efforts()
        .into_iter()
        .map(|e| evaluate(ix, data, queries, truth, k, e))
        .collect()
}
fn print_table(ms: &[Measurement]) {
    let points: Vec<Point> = ms
        .iter()
        .map(|m| Point {
            label: format!("{} {}", m.index, m.effort),
            recall: m.dist_recall,
            cost: m.dist_comps,
        })
        .collect();
    let front: HashSet<usize> = pareto(&points).into_iter().collect();
    println!(
        "  {:<10} {:>6} {:>7} {:>7} {:>6} {:>10} | {:>8} {:>7} {:>7} {:>7}  pareto",
        "index",
        "effort",
        "rec(id)",
        "rec(d)",
        "ratio",
        "dist/query",
        "QPS*",
        "p50us*",
        "p95us*",
        "p99us*"
    );
    for (i, m) in ms.iter().enumerate() {
        println!(
            "  {:<10} {:>6} {:>7.3} {:>7.3} {:>6.3} {:>10.0} | {:>8.0} {:>7.0} {:>7.0} {:>7.0}  {}",
            m.index,
            m.effort,
            m.id_recall,
            m.dist_recall,
            m.ratio,
            m.dist_comps,
            m.qps,
            m.p50,
            m.p95,
            m.p99,
            if front.contains(&i) { "*" } else { "" }
        );
    }
}
/// Cheapest measured configuration of `index` that reaches the target recall.
fn cost_at(ms: &[Measurement], index: &str, target: f64) -> Option<f64> {
    ms.iter()
        .filter(|m| m.index == index && m.dist_recall >= target)
        .map(|m| m.dist_comps)
        .min_by(f64::total_cmp)
}

// ---------- dataset difficulty ----------
/// (relative contrast, (d_max - d_min)/d_min, MLE local intrinsic dimensionality), each averaged over the queries.
fn difficulty(data: &Mat, queries: &Mat, k_lid: usize) -> (f64, f64, f64) {
    let (mut rc, mut spread, mut lid) = (0.0, 0.0, 0.0);
    for i in 0..queries.n {
        let mut d: Vec<f64> = (0..data.n)
            .map(|j| (l2(queries.row(i), data.row(j)) as f64).sqrt())
            .collect();
        d.sort_by(f64::total_cmp);
        let min = d[0].max(1e-12);
        rc += d.iter().sum::<f64>() / d.len() as f64 / min;
        spread += (d[d.len() - 1] - min) / min;
        lid += lid_mle(&d[..k_lid]);
    }
    let n = queries.n as f64;
    (rc / n, spread / n, lid / n)
}
/// Maximum-likelihood LID from sorted neighbour distances r_1..r_k. Zero distances are clamped.
fn lid_mle(r: &[f64]) -> f64 {
    let rk = r[r.len() - 1].max(1e-12);
    let mean: f64 = r.iter().map(|x| (x.max(1e-12) / rk).ln()).sum::<f64>() / r.len() as f64;
    if mean == 0.0 {
        f64::INFINITY
    } else {
        -1.0 / mean
    }
}
/// k-occurrence counts N_k over the whole set (each point's k nearest OTHER points), and the skewness of that distribution.
fn hubness(data: &Mat, k: usize) -> (Vec<usize>, f64) {
    let mut nk = vec![0usize; data.n];
    for i in 0..data.n {
        for (_, j) in exact_topk(data.row(i), data, k + 1)
            .into_iter()
            .filter(|h| h.1 as usize != i)
            .take(k)
        {
            nk[j as usize] += 1;
        }
    }
    let mean = nk.iter().sum::<usize>() as f64 / data.n as f64;
    let m2 = nk.iter().map(|&c| (c as f64 - mean).powi(2)).sum::<f64>() / data.n as f64;
    let m3 = nk.iter().map(|&c| (c as f64 - mean).powi(3)).sum::<f64>() / data.n as f64;
    (nk, m3 / m2.powf(1.5))
}

// ---------- checks on the measuring instruments themselves ----------
fn test_metrics_by_hand() {
    // truth ids 1,2,3,4 at squared distances 1,4,9,16
    let truth: Hits = vec![(1.0, 1), (4.0, 2), (9.0, 3), (16.0, 4)];
    assert_eq!(id_recall(&[1, 2, 3, 4], &[1, 2, 3, 4]), 1.0);
    assert_eq!(id_recall(&[4, 9, 1, 8], &[1, 2, 3, 4]), 0.5);
    assert_eq!(
        id_recall(&[1, 1, 1, 1], &[1, 2, 3, 4]),
        0.25,
        "duplicates count once"
    );
    assert_eq!(id_recall(&[], &[1, 2, 3, 4]), 0.0);
    // distance recall: id 7 is a tie with the 4th neighbour and counts; id 8 is farther and does not
    let got: Hits = vec![(1.0, 1), (4.0, 2), (16.0, 7), (25.0, 8)];
    assert_eq!(distance_recall(&got, &truth, 0.0), 0.75);
    assert_eq!(id_recall(&[1, 2, 7, 8], &[1, 2, 3, 4]), 0.5);
    assert_eq!(
        distance_recall(&vec![(1.0, 1), (1.0, 1), (1.0, 1)], &truth, 0.0),
        0.25,
        "duplicates count once"
    );
    // ratio on true distances: (1/1 + 2/2 + 4/3 + 5/4) / 4
    assert!((approx_ratio(&got, &truth) - (1.0 + 1.0 + 4.0 / 3.0 + 5.0 / 4.0) / 4.0).abs() < 1e-12);
    assert_eq!(approx_ratio(&truth, &truth), 1.0);
    // a short answer has a perfect ratio: this is why `missing` is reported too
    assert_eq!(approx_ratio(&truth[..1].to_vec(), &truth), 1.0);
    assert_eq!(distance_recall(&truth[..1].to_vec(), &truth, 0.0), 0.25);
    // zero true distance
    assert_eq!(approx_ratio(&vec![(0.0, 5)], &vec![(0.0, 6)]), 1.0);
    // nearest-rank percentiles of 1..=100 and of a small sample
    let v: Vec<f64> = (1..=100).map(|x| x as f64).collect();
    for (p, want) in [
        (50.0, 50.0),
        (95.0, 95.0),
        (99.0, 99.0),
        (100.0, 100.0),
        (0.5, 1.0),
    ] {
        assert_eq!(percentile(&v, p), want, "p{p}");
    }
    let s = [10.0, 20.0, 30.0, 1000.0];
    assert_eq!(
        (
            percentile(&s, 50.0),
            percentile(&s, 75.0),
            percentile(&s, 99.0)
        ),
        (20.0, 30.0, 1000.0)
    );
    // LID of radii that grow like count^(1/m) is close to m (the k-th point itself contributes ln 1 = 0)
    for m in [2.0f64, 5.0] {
        let r: Vec<f64> = (1..=2000).map(|i| (i as f64).powf(1.0 / m)).collect();
        assert!(
            (lid_mle(&r) - m).abs() < 0.02 * m,
            "LID of a perfect {m}-dim growth law: {}",
            lid_mle(&r)
        );
    }
}

fn test_metamorphic(rng: &mut Rng) {
    for _ in 0..2000 {
        let n = 5 + rng.below(60);
        let k = 1 + rng.below(n.min(12));
        // coarse distances on purpose: many ties
        let all: Hits = (0..n as u32).map(|i| (rng.below(8) as f32, i)).collect();
        let mut sorted = all.clone();
        sort_hits(&mut sorted);
        let truth: Hits = sorted[..k].to_vec();
        let tids: Vec<u32> = truth.iter().map(|h| h.1).collect();
        // a random answer of k distinct items
        let mut perm = all.clone();
        for i in 0..k {
            let j = i + rng.below(n - i);
            perm.swap(i, j);
        }
        let mut got: Hits = perm[..k].to_vec();
        let ids = |h: &Hits| h.iter().map(|x| x.1).collect::<Vec<_>>();
        let (r0, d0, a0) = (
            id_recall(&ids(&got), &tids),
            distance_recall(&got, &truth, 0.0),
            approx_ratio(&got, &truth),
        );
        assert!(
            d0 >= r0,
            "every true id is within the k-th distance, so distance recall dominates id recall"
        );
        assert!(
            a0 >= 1.0 - 1e-12,
            "no answer can be closer than the exact one"
        );
        // order does not matter
        let mut shuffled = got.clone();
        shuffled.reverse();
        assert_eq!(
            (
                id_recall(&ids(&shuffled), &tids),
                distance_recall(&shuffled, &truth, 0.0),
                approx_ratio(&shuffled, &truth)
            ),
            (r0, d0, a0)
        );
        // replace wrong answers by missing true ones, one at a time: nothing gets worse, and we end at 1.0
        let (mut prev_r, mut prev_d, mut prev_a) = (r0, d0, a0);
        loop {
            let have: HashSet<u32> = got.iter().map(|h| h.1).collect();
            let Some(&missing) = truth.iter().find(|t| !have.contains(&t.1)) else {
                break;
            };
            let wrong = got
                .iter()
                .position(|g| !tids.contains(&g.1))
                .expect("k items, one true one missing, so one is wrong");
            got[wrong] = missing;
            let (r, d, a) = (
                id_recall(&ids(&got), &tids),
                distance_recall(&got, &truth, 0.0),
                approx_ratio(&got, &truth),
            );
            assert!(
                r > prev_r && d >= prev_d && a <= prev_a + 1e-12,
                "adding an exact result made a metric worse"
            );
            (prev_r, prev_d, prev_a) = (r, d, a);
        }
        assert_eq!((prev_r, prev_d), (1.0, 1.0));
    }
}

fn test_ties(rng: &mut Rng) {
    // every vector stored 4 times: a different but equally good answer set exists for almost every query
    let base = clusters(200, 8, 5, 2.0, 1.0, rng);
    let data = Mat {
        n: 800,
        dim: 8,
        v: base.v.repeat(4),
    };
    let q = clusters(20, 8, 5, 2.0, 1.0, rng);
    let (mut idr, mut dr) = (0.0, 0.0);
    for i in 0..q.n {
        let truth = exact_topk(q.row(i), &data, 10);
        // an equally exact index that breaks ties by LARGEST id
        let mut other: Hits = (0..data.n)
            .map(|j| (l2(q.row(i), data.row(j)), j as u32))
            .collect();
        other.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
        other.truncate(10);
        idr += id_recall(
            &other.iter().map(|h| h.1).collect::<Vec<_>>(),
            &truth.iter().map(|h| h.1).collect::<Vec<_>>(),
        ) / q.n as f64;
        let d = distance_recall(&other, &truth, 0.0);
        assert_eq!(
            d, 1.0,
            "an exact index must score 1.0 whatever its tie-break"
        );
        dr += d / q.n as f64;
    }
    println!(
        "ties (each vector stored 4x): an exact index with another tie-break scores id recall {idr:.3} but distance recall {dr:.3}"
    );
    assert!(idr < 0.9);
}

fn test_pareto(rng: &mut Rng) {
    for _ in 0..3000 {
        let n = rng.below(25);
        let pts: Vec<Point> = (0..n)
            .map(|i| Point {
                label: i.to_string(),
                recall: rng.below(6) as f64 / 5.0,
                cost: rng.below(6) as f64,
            })
            .collect();
        let front = pareto(&pts);
        // oracle: non-dominated, keeping the first of identical points
        let want: HashSet<usize> = (0..n)
            .filter(|&i| !(0..n).any(|j| dominates(&pts[j], &pts[i])))
            .filter(|&i| {
                !(0..i).any(|j| pts[j].recall == pts[i].recall && pts[j].cost == pts[i].cost)
            })
            .collect();
        assert_eq!(
            front.iter().copied().collect::<HashSet<_>>(),
            want,
            "{pts:?}"
        );
        for w in front.windows(2) {
            assert!(
                pts[w[0]].cost < pts[w[1]].cost && pts[w[0]].recall < pts[w[1]].recall,
                "frontier must rise strictly in both"
            );
        }
    }
}

// ---------- experiments ----------
const K: usize = 10;
struct Report {
    name: &'static str,
    lid: f64,
    ms: Vec<Measurement>,
}
fn run_dataset(name: &'static str, all: Mat, n: usize, rng: &mut Rng) -> Report {
    let (data, queries) = all.split(n);
    let truth = ground_truth(&data, &queries, K);
    let (rc, spread, lid) = difficulty(&data, &queries, 100);
    println!(
        "\n=== {name}: n={} dim={} queries={} k={K} ===",
        data.n, data.dim, queries.n
    );
    println!(
        "  difficulty: relative contrast {rc:.2}, (dmax-dmin)/dmin {spread:.2}, LID(k=100) {lid:.1}"
    );
    let mut build = vec![];
    let mut indexes: Vec<Box<dyn AnnIndex>> = vec![Box::new(Brute)];
    let t = Instant::now();
    indexes.push(Box::new(IvfLite::build(&data, 64, 8, rng)));
    build.push(("ivf-lite", t.elapsed().as_secs_f64()));
    let t = Instant::now();
    indexes.push(Box::new(HnswLite::build(&data, 12, 80, rng)));
    build.push(("hnsw-lite", t.elapsed().as_secs_f64()));
    for ((nm, secs), ix) in build.iter().zip(indexes.iter().skip(1)) {
        println!(
            "  build {nm:<10} {secs:>6.2}s*   index memory {:>7.1} KiB on top of {:.1} KiB of vectors",
            ix.extra_bytes() as f64 / 1024.0,
            data.v.len() as f64 * 4.0 / 1024.0
        );
    }
    let ms: Vec<Measurement> = indexes
        .iter()
        .flat_map(|ix| sweep(ix.as_ref(), &data, &queries, &truth, K))
        .collect();
    print_table(&ms);

    let brute = &ms[0];
    assert_eq!(
        (
            brute.index,
            brute.id_recall,
            brute.dist_recall,
            brute.ratio,
            brute.missing
        ),
        ("brute", 1.0, 1.0, 1.0, 0),
        "brute force must be exact"
    );
    assert_eq!(brute.dist_comps, n as f64);
    for m in &ms {
        assert!(m.dist_recall + 1e-12 >= m.id_recall && m.ratio >= 1.0 - 1e-9);
    }
    let ivf: Vec<&Measurement> = ms.iter().filter(|m| m.index == "ivf-lite").collect();
    for w in ivf.windows(2) {
        assert!(w[0].effort < w[1].effort);
        // more cells = a superset of candidates, so no query can lose a true neighbour
        assert!(
            w[0].per_query_recall
                .iter()
                .zip(&w[1].per_query_recall)
                .all(|(a, b)| *b >= *a - 1e-12),
            "IVF recall fell when nprobe rose"
        );
        assert!(w[1].dist_comps > w[0].dist_comps);
    }
    assert_eq!(
        ivf.last().unwrap().dist_recall,
        1.0,
        "probing every cell is exact"
    );
    assert!(
        ivf.last().unwrap().dist_comps > n as f64,
        "and costs MORE than brute force: the centroid scan is overhead"
    );
    Report { name, lid, ms }
}

fn curse_of_dimensionality(rng: &mut Rng) {
    println!(
        "\n=== uniform data, growing dimension (n=2000): distances concentrate and hubs appear ==="
    );
    println!(
        "  {:>5} {:>10} {:>16} {:>8} {:>10} {:>9} {:>9}",
        "dim", "rel.contr", "(dmax-dmin)/dmin", "LID", "N_10 skew", "max N_10", "N_10 = 0"
    );
    let mut rows = vec![];
    for dim in [2usize, 8, 32, 128] {
        let (data, q) = uniform(2100, dim, rng).split(2000);
        let (rc, spread, lid) = difficulty(&data, &q, 100);
        let (nk, skew) = hubness(&data, 10);
        assert_eq!(
            nk.iter().sum::<usize>(),
            10 * data.n,
            "every point names exactly 10 neighbours"
        );
        println!(
            "  {dim:>5} {rc:>10.2} {spread:>16.2} {lid:>8.1} {skew:>10.2} {:>9} {:>9}",
            nk.iter().max().unwrap(),
            nk.iter().filter(|&&c| c == 0).count()
        );
        rows.push((rc, spread, skew));
    }
    for w in rows.windows(2) {
        assert!(
            w[1].0 < w[0].0,
            "relative contrast should fall with dimension"
        );
        assert!(
            w[1].1 < w[0].1,
            "distance spread should fall with dimension"
        );
    }
    assert!(
        rows[3].2 > rows[0].2 + 1.0,
        "hubness skew should be far higher at 128 dims than at 2"
    );
}

fn lid_sees_the_manifold(rng: &mut Rng) {
    println!("\n=== LID measures the manifold, not the ambient dimension (n=4000, ambient 64) ===");
    let mut got = vec![];
    for latent in [2usize, 8, 32] {
        let (data, q) = manifold(4100, 64, latent, rng).split(4000);
        let (_, _, lid) = difficulty(&data, &q, 100);
        println!("  latent dimension {latent:>2} -> LID estimate {lid:.1}");
        got.push(lid);
    }
    assert!(
        (got[0] - 2.0).abs() < 0.5,
        "2-dim manifold estimate {}",
        got[0]
    );
    assert!(
        got[1] > 5.0 && got[1] < 9.0,
        "8-dim manifold estimate {}",
        got[1]
    );
    assert!(
        got[2] > got[1] + 5.0 && got[2] < 32.0,
        "the 32-dim estimate should be higher, and biased low"
    );
}

fn out_of_distribution(rng: &mut Rng) {
    // one generator call gives data and in-distribution queries with the same cluster centres
    let (data, inq) = clusters(6200, 24, 30, 2.0, 1.0, rng).split(6000);
    // two kinds of foreign query: a mild shift of the in-distribution queries, and a second call, which draws NEW cluster centres
    let dir: Vec<f32> = (0..24).map(|_| rng.gauss()).collect();
    let norm = dir.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mild = Mat {
        n: inq.n,
        dim: inq.dim,
        v: inq
            .v
            .iter()
            .enumerate()
            .map(|(i, x)| x + 6.0 * dir[i % 24] / norm)
            .collect(),
    };
    let unseen = clusters(200, 24, 30, 2.0, 1.0, rng);
    let ivf = IvfLite::build(&data, 64, 8, rng);
    let sets = [&inq, &mild, &unseen];
    let truths: Vec<Vec<Hits>> = sets.iter().map(|q| ground_truth(&data, q, K)).collect();
    println!("\n=== out-of-distribution queries: same index, same nprobe ===");
    println!(
        "  {:>6} {:>10} {:>16} {:>16}",
        "nprobe", "in-dist", "shifted a little", "unseen clusters"
    );
    for p in [1usize, 2, 4, 8] {
        let r: Vec<f64> = (0..3)
            .map(|i| evaluate(&ivf, &data, sets[i], &truths[i], K, p).dist_recall)
            .collect();
        println!("  {p:>6} {:>10.3} {:>16.3} {:>16.3}", r[0], r[1], r[2]);
        // only at small nprobe: higher up, in-distribution recall sits at the ceiling and the gap closes
        if p <= 2 {
            assert!(
                r[0] - r[2] > 0.1,
                "queries from unseen clusters should lose recall at nprobe {p}: {:.3} vs {:.3}",
                r[0],
                r[2]
            );
        }
    }
    println!(
        "  'foreign' is not automatically 'hard': the mild shift is printed because it can leave recall unchanged or even raise it."
    );
}

fn main() {
    let mut rng = Rng(0x875_875);
    test_metrics_by_hand();
    test_metamorphic(&mut rng);
    test_pareto(&mut rng);
    println!(
        "metric checks passed: hand cases, 2000 metamorphic trials, 3000 Pareto trials against an O(n^2) oracle"
    );
    test_ties(&mut rng);
    println!(
        "\nColumns marked * are wall-clock numbers from this machine and run; nothing is asserted about them."
    );

    let n = 6000;
    let reports = [
        run_dataset(
            "manifold: 8-dim cube in 32 dims",
            manifold(n + 200, 32, 8, &mut rng),
            n,
            &mut rng,
        ),
        run_dataset(
            "clustered: 30 gaussians, 32 dims",
            clusters(n + 200, 32, 30, 1.2, 1.0, &mut rng),
            n,
            &mut rng,
        ),
        run_dataset(
            "uniform: 32 dims",
            uniform(n + 200, 32, &mut rng),
            n,
            &mut rng,
        ),
    ];

    println!(
        "\n=== distance computations per query needed to reach a target recall (brute force = {n}) ==="
    );
    println!(
        "  {:<34} {:>6} {:>14} {:>14}",
        "dataset", "LID", "ivf @0.95", "hnsw @0.95"
    );
    for r in &reports {
        let f = |c: Option<f64>| c.map_or("not reached".into(), |c| format!("{c:.0}"));
        println!(
            "  {:<34} {:>6.1} {:>14} {:>14}",
            r.name,
            r.lid,
            f(cost_at(&r.ms, "ivf-lite", 0.95)),
            f(cost_at(&r.ms, "hnsw-lite", 0.95))
        );
    }
    let h =
        cost_at(&reports[0].ms, "hnsw-lite", 0.95).expect("hnsw must reach 0.95 on the manifold");
    assert!(h < 0.25 * n as f64, "hnsw cost {h} on the manifold");
    // difficulty: at the same effort (nprobe = 4) the set with the higher LID has the lower recall
    let at4 = |r: &Report| {
        r.ms.iter()
            .find(|m| m.index == "ivf-lite" && m.effort == 4)
            .unwrap()
            .dist_recall
    };
    let (m, u) = (&reports[0], &reports[2]);
    println!(
        "  IVF nprobe=4: manifold LID {:.1} recall {:.3} vs uniform LID {:.1} recall {:.3}",
        m.lid,
        at4(m),
        u.lid,
        at4(u)
    );
    assert!(m.lid < u.lid && at4(m) > at4(u) + 0.2);

    curse_of_dimensionality(&mut rng);
    lid_sees_the_manifold(&mut rng);
    out_of_distribution(&mut rng);
    println!("\nall ANN evaluation harness checks passed");
}
