//! RECIPE 141 (vectors): a simplified HNSW: layered proximity graphs, greedy descent, best-first search with `ef`.
//! LEVEL: advanced
//! Use in: low-latency approximate nearest-neighbour search with high recall on in-memory vectors.   Run: cargo run --release --example hnsw_lite
#![allow(dead_code)]
//!
//! WHAT: every vector is a node. Each node draws a random top level L = floor(-ln(U) * mL) with mL = 1/ln(M), so level l holds
//! roughly a 1/M^l fraction of nodes (a skip list over a graph). Each level is a proximity graph (neighbour lists, ids = `usize`
//! indices into one arena `Vec`). Search enters at the top, greedily walks to the closest node on each upper level (few, long hops),
//! drops down, and on level 0 runs a best-first search keeping the `ef` best candidates.
//! INSERT: descend greedily to the node's level, then at each level <= L run the same search with `ef_construction`, choose
//! up to M neighbours from the result, link both ways, and PRUNE any neighbour whose list grew past its cap (M, or 2M = M0 at level 0).
//! PARAMETERS: `M` = out-degree (memory and recall up, build slower); `ef_construction` = how hard each insert looks for good
//! neighbours (graph quality); `ef` at query time = candidate list size, the recall/latency dial (must be >= k).
//! NEIGHBOUR SELECTION: "simple" keeps the M closest. The HEURISTIC (paper Algorithm 4) walks candidates nearest-first and keeps
//! candidate e only if it is closer to the new node q than to every neighbour already kept; otherwise e is "covered" by a kept
//! neighbour in that direction. That yields links in DIFFERENT directions (including bridges between clusters) instead of M links
//! into the same blob, which is what keeps clustered data navigable. Discarded candidates top the list up when fewer than M survive.
//! COMMON USES: the default ANN index in Qdrant, pgvector (hnsw index type) and Lucene/Elasticsearch/OpenSearch dense-vector
//! search; hnswlib is the reference implementation. Costs: memory heavy (graph + raw vectors), slow to build, no cheap deletes.
//! Read: https://arxiv.org/abs/1603.09320 (Malkov and Yashunin, HNSW) and https://github.com/nmslib/hnswlib
use std::cell::Cell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashSet};
// ---------- shared mini-harness (copied into every vector recipe so each file stands alone) ----------
/// xorshift64: tiny, seedable, deterministic. Not for crypto; perfect for reproducible experiments.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform in the open interval (0, 1): never 0, so `ln` is always safe.
    fn unit(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32 + 0.5) / (1u64 << 24) as f32
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    /// Gaussian-ish: sum of 4 uniforms, recentred and rescaled to unit variance (Irwin-Hall). Good enough for test data.
    fn gauss(&mut self) -> f32 {
        let s: f32 = (0..4).map(|_| self.unit()).sum();
        (s - 2.0) * 3.0f32.sqrt()
    }
}

/// Clustered data: real embeddings are not uniform noise, they sit in dense regions. ANN structures only
/// help when such structure exists, so we draw `clusters` centres and add noise around a random centre.
fn make_data(n: usize, dim: usize, clusters: usize, rng: &mut Rng) -> Vec<Vec<f32>> {
    let centres: Vec<Vec<f32>> = (0..clusters)
        .map(|_| (0..dim).map(|_| rng.gauss() * 2.0).collect())
        .collect();
    (0..n)
        .map(|_| {
            let c = &centres[rng.below(clusters)];
            c.iter().map(|x| x + rng.gauss() * 1.5).collect()
        })
        .collect()
}

/// Squared L2 (monotone in L2, so the same ranking without the sqrt).
fn l2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
/// Cosine DISTANCE = 1 - cosine similarity. On unit-normalised vectors it ranks like L2.
fn cosine_dist(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    1.0 - dot / (na * nb)
}

/// ORACLE: exact top-k by scanning everything. Every approximate answer is judged against this.
fn exact_topk(q: &[f32], data: &[Vec<f32>], k: usize) -> Vec<usize> {
    let mut all: Vec<(f32, usize)> = data
        .iter()
        .enumerate()
        .map(|(i, v)| (l2(q, v), i))
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    all.into_iter().take(k).map(|(_, i)| i).collect()
}

/// recall@k = |approx ∩ exact| / k
fn recall(approx: &[usize], exact: &[usize]) -> f64 {
    let hits = approx.iter().filter(|a| exact.contains(a)).count();
    hits as f64 / exact.len() as f64
}

// ---------- float ordering for heaps ----------
#[derive(PartialEq, PartialOrd, Clone, Copy)]
struct F(f32);
impl Eq for F {}
impl Ord for F {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}

// ---------- HNSW ----------
struct Node {
    links: Vec<Vec<usize>>, // links[l] = neighbours on level l; links.len() = level + 1
}

struct Hnsw<'a> {
    data: &'a [Vec<f32>],
    nodes: Vec<Node>,
    entry: Option<usize>,
    m: usize,
    ef_construction: usize,
    heuristic: bool,
    ml: f64,
    dist_calls: Cell<usize>,
}

impl<'a> Hnsw<'a> {
    fn new(data: &'a [Vec<f32>], m: usize, ef_construction: usize, heuristic: bool) -> Self {
        Hnsw {
            data,
            nodes: Vec::new(),
            entry: None,
            m,
            ef_construction,
            heuristic,
            ml: 1.0 / (m as f64).ln(),
            dist_calls: Cell::new(0),
        }
    }
    fn dist(&self, a: &[f32], id: usize) -> f32 {
        self.dist_calls.set(self.dist_calls.get() + 1);
        l2(a, &self.data[id])
    }
    fn max_links(&self, level: usize) -> usize {
        if level == 0 { 2 * self.m } else { self.m } // level 0 carries the recall, so it gets double degree
    }
    fn random_level(&self, rng: &mut Rng) -> usize {
        (-(rng.unit() as f64).ln() * self.ml).floor() as usize
    }

    /// Greedy walk on one level: hop to any neighbour that is closer until none is. This is search_layer with ef = 1.
    fn greedy(&self, q: &[f32], mut cur: usize, level: usize) -> usize {
        let mut best = self.dist(q, cur);
        loop {
            let mut moved = false;
            for &nb in &self.nodes[cur].links[level] {
                let d = self.dist(q, nb);
                if d < best {
                    best = d;
                    cur = nb;
                    moved = true;
                }
            }
            if !moved {
                return cur;
            }
        }
    }

    /// Best-first search on one level (paper Algorithm 2). Returns up to `ef` (dist, id), nearest first.
    fn search_layer(
        &self,
        q: &[f32],
        entries: &[usize],
        ef: usize,
        level: usize,
    ) -> Vec<(f32, usize)> {
        let mut visited: HashSet<usize> = entries.iter().copied().collect();
        let mut cand: BinaryHeap<Reverse<(F, usize)>> = BinaryHeap::new(); // closest unexpanded first
        let mut best: BinaryHeap<(F, usize)> = BinaryHeap::new(); // worst of the kept ef on top
        for &e in entries {
            let d = F(self.dist(q, e));
            cand.push(Reverse((d, e)));
            best.push((d, e));
        }
        while let Some(Reverse((d, c))) = cand.pop() {
            if best.len() >= ef && d > best.peek().unwrap().0 {
                break; // the closest open candidate is worse than our worst kept result: nothing left to gain
            }
            for &nb in &self.nodes[c].links[level] {
                if !visited.insert(nb) {
                    continue;
                }
                let dn = F(self.dist(q, nb));
                if best.len() < ef || dn < best.peek().unwrap().0 {
                    cand.push(Reverse((dn, nb)));
                    best.push((dn, nb));
                    if best.len() > ef {
                        best.pop();
                    }
                }
            }
        }
        let mut out: Vec<(f32, usize)> = best.into_iter().map(|(d, i)| (d.0, i)).collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        out
    }

    /// Pick at most `m` neighbours from `cands` (sorted nearest-first, distances measured to the base point).
    fn select(&self, cands: &[(f32, usize)], m: usize) -> Vec<usize> {
        if !self.heuristic {
            return cands.iter().take(m).map(|c| c.1).collect(); // simple: M closest
        }
        let mut kept: Vec<(f32, usize)> = Vec::new();
        let mut discarded: Vec<usize> = Vec::new();
        for &(d_q, e) in cands {
            if kept.len() >= m {
                break;
            }
            // Keep e only if it is closer to the base point than to any already-kept neighbour.
            let covered = kept
                .iter()
                .any(|&(_, r)| l2(&self.data[e], &self.data[r]) < d_q);
            if covered {
                discarded.push(e)
            } else {
                kept.push((d_q, e))
            }
        }
        let mut out: Vec<usize> = kept.into_iter().map(|k| k.1).collect();
        out.extend(discarded.into_iter().take(m - out.len())); // keepPrunedConnections
        out
    }

    fn insert(&mut self, id: usize, rng: &mut Rng) {
        let level = self.random_level(rng);
        self.nodes.push(Node {
            links: vec![Vec::new(); level + 1],
        });
        let Some(entry) = self.entry else {
            self.entry = Some(id);
            return;
        };
        let top = self.nodes[entry].links.len() - 1;
        let q = self.data[id].clone();
        let mut cur = entry;
        for l in (level + 1..=top).rev() {
            cur = self.greedy(&q, cur, l);
        }
        let mut eps = vec![cur];
        for l in (0..=level.min(top)).rev() {
            let found = self.search_layer(&q, &eps, self.ef_construction, l);
            let chosen = self.select(&found, self.m);
            for &nb in &chosen {
                self.nodes[id].links[l].push(nb);
                self.nodes[nb].links[l].push(id);
                if self.nodes[nb].links[l].len() > self.max_links(l) {
                    self.prune(nb, l);
                }
            }
            eps = found.iter().map(|c| c.1).collect();
        }
        if level > top {
            self.entry = Some(id); // new highest layer: becomes the global entry point
        }
    }

    /// Over-full neighbour list: re-rank by distance to the owner and re-select.
    fn prune(&mut self, owner: usize, level: usize) {
        let mut c: Vec<(f32, usize)> = self.nodes[owner].links[level]
            .iter()
            .map(|&n| (l2(&self.data[owner], &self.data[n]), n))
            .collect();
        c.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        self.nodes[owner].links[level] = self.select(&c, self.max_links(level));
    }

    fn search(&self, q: &[f32], k: usize, ef: usize) -> Vec<usize> {
        let mut cur = self.entry.unwrap();
        for l in (1..self.nodes[cur].links.len()).rev() {
            cur = self.greedy(q, cur, l);
        }
        let found = self.search_layer(q, &[cur], ef.max(k), 0);
        found.into_iter().take(k).map(|c| c.1).collect()
    }
}

fn run(
    name: &str,
    h: &Hnsw,
    queries: &[Vec<f32>],
    truth: &[Vec<usize>],
    k: usize,
) -> Vec<(usize, f64, f64)> {
    println!("{name}: levels={} ", h.nodes[h.entry.unwrap()].links.len());
    println!(
        "{:>6} {:>10} {:>14} {:>10}",
        "ef", "recall@10", "dist calls/q", "queries/s"
    );
    let mut rows = Vec::new();
    for ef in [10, 20, 40, 80, 160] {
        h.dist_calls.set(0);
        let t = std::time::Instant::now();
        let rec: f64 = queries
            .iter()
            .zip(truth)
            .map(|(q, tr)| recall(&h.search(q, k, ef), tr))
            .sum::<f64>()
            / queries.len() as f64;
        let calls = h.dist_calls.get() as f64 / queries.len() as f64;
        println!(
            "{ef:>6} {rec:>10.3} {calls:>14.0} {:>10.0}",
            queries.len() as f64 / t.elapsed().as_secs_f64()
        );
        rows.push((ef, rec, calls));
    }
    rows
}

fn main() {
    let (n, dim, nq, k) = (8_000, 32, 200, 10);
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut all = make_data(n + nq, dim, 20, &mut rng);
    let queries = all.split_off(n);
    let data = all;
    assert!(cosine_dist(&data[0], &data[1]) > 0.0);
    let truth: Vec<Vec<usize>> = queries.iter().map(|q| exact_topk(q, &data, k)).collect();

    for heuristic in [false, true] {
        let mut h = Hnsw::new(&data, 16, 100, heuristic);
        let mut brng = Rng(0x2545_F491_4F6C_DD1D);
        for id in 0..n {
            h.insert(id, &mut brng);
        }
        let degree: usize = h.nodes.iter().map(|n| n.links[0].len()).max().unwrap();
        assert!(
            degree <= h.max_links(0),
            "pruning keeps level-0 degree <= 2M"
        );
        let rows = run(
            if heuristic {
                "heuristic selection"
            } else {
                "simple M-closest selection"
            },
            &h,
            &queries,
            &truth,
            k,
        );
        assert!(
            rows.windows(2).all(|w| w[1].1 + 0.01 >= w[0].1),
            "more ef must not meaningfully lower recall"
        );
        assert!(
            rows.windows(2).all(|w| w[1].2 >= w[0].2),
            "more ef costs more distance computations"
        );
        assert!(
            rows.last().unwrap().1 >= 0.95,
            "recall@10 at ef=160 was {}",
            rows.last().unwrap().1
        );
        let brute = n as f64;
        assert!(
            rows.last().unwrap().2 < brute / 2.0,
            "graph search must beat brute force on distance calls"
        );
    }
}
