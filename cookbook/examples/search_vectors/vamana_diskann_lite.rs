//! RECIPE 142 (vectors): a DiskANN / Vamana-style single-layer graph index with GreedySearch, RobustPrune and "node read" accounting.
//! LEVEL: advanced
//! Use in: billion-scale vector search where the graph and full vectors live on SSD and only a small sketch sits in RAM.   Run: cargo run --release --example vamana_diskann_lite
#![allow(dead_code)]
//!
//! WHAT: one flat graph (no layers) with out-degree <= R. BUILD: start from a random graph (so it is connected enough to search),
//! pick the MEDOID (point nearest the dataset mean) as the fixed entry point, then for every point p, in random order: run
//! GreedySearch(medoid, p, L) and keep every node it VISITED, RobustPrune(p, visited) to choose p's out-neighbours, and add reverse
//! edges j -> p, re-pruning j when it overflows R. Do this twice: pass 1 with alpha = 1 (plain "closest and diverse" edges), pass 2
//! with alpha > 1 (keep some long edges).
//! ROBUSTPRUNE(p, V, alpha, R): repeatedly take the closest remaining candidate p*, keep it, and delete every candidate v with
//! alpha * d(p*, v) <= d(p, v), i.e. v is "covered" by p*. With alpha = 1 only strictly closer shortcuts cover; with alpha > 1 a
//! candidate is removed only if p* is MUCH closer to it, so farther, diverse edges survive. Those long edges cut the hop count
//! (the graph's diameter), which is the quantity that matters on disk.
//! PARAMETERS: R = max degree (bigger: fewer hops, more bytes per node); L = candidate list size during build/search (bigger:
//! better recall, more reads); alpha = how aggressively to keep long-range edges (1.0 to 1.5 typical; here 1.2).
//! WHY ONE LAYER FOR SSD: HNSW's upper layers help in RAM, but on disk every hop is a random 4 KB read (~100 us). Vamana's
//! long-range edges let a single layer reach any region in few hops, and each node's vector plus neighbour list is packed in one
//! sector, so cost per query ~ number of expanded nodes ("node reads"). We simulate that by counting them.
//! COMMON USES: DiskANN (Microsoft Research) and its open implementations; Vamana-style graphs in large-scale vector databases
//! that keep compressed vectors in RAM (product quantization, see product_quantization) and full-precision vectors plus graph on NVMe, followed
//! by exact re-ranking of the few nodes read.
//! Read: the DiskANN paper (Subramanya et al., NeurIPS 2019) https://suhasjs.github.io/files/diskann_neurips19.pdf
use std::collections::HashSet;
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

// ---------- Vamana ----------
struct Vamana<'a> {
    data: &'a [Vec<f32>],
    graph: Vec<Vec<usize>>, // out-neighbours per node (arena, usize ids)
    medoid: usize,
    r: usize,
}

/// Result of one GreedySearch: the best `l` candidates (nearest first) and every node whose neighbour list was read.
struct Searched {
    best: Vec<(f32, usize)>,
    visited: Vec<usize>,
}

fn dist(a: &[f32], b: &[f32]) -> f32 {
    l2(a, b).sqrt() // true metric: the alpha test compares distances, so squared L2 would silently square alpha
}

impl<'a> Vamana<'a> {
    fn medoid_of(data: &[Vec<f32>]) -> usize {
        let dim = data[0].len();
        let mean: Vec<f32> = (0..dim)
            .map(|d| data.iter().map(|v| v[d]).sum::<f32>() / data.len() as f32)
            .collect();
        (0..data.len())
            .min_by(|&a, &b| l2(&data[a], &mean).total_cmp(&l2(&data[b], &mean)))
            .unwrap()
    }

    /// Random graph with out-degree R: the starting point that Vamana then sculpts.
    fn random_graph(data: &'a [Vec<f32>], r: usize, rng: &mut Rng) -> Self {
        let n = data.len();
        let graph = (0..n)
            .map(|i| {
                let mut s = HashSet::new();
                while s.len() < r {
                    let j = rng.below(n);
                    if j != i {
                        s.insert(j);
                    }
                }
                let mut v: Vec<usize> = s.into_iter().collect();
                v.sort_unstable(); // HashSet order is random per run; sort to stay deterministic
                v
            })
            .collect();
        Vamana {
            data,
            graph,
            medoid: Self::medoid_of(data),
            r,
        }
    }

    /// GreedySearch(start, q, L): keep a list of the L closest nodes seen; repeatedly expand the closest unexpanded one.
    fn greedy_search(&self, q: &[f32], l: usize, dist_calls: &mut usize) -> Searched {
        let mut seen: HashSet<usize> = HashSet::from([self.medoid]);
        let mut list: Vec<(f32, usize, bool)> =
            vec![(dist(q, &self.data[self.medoid]), self.medoid, false)];
        *dist_calls += 1;
        let mut visited = Vec::new();
        while let Some(pos) = list.iter().position(|e| !e.2) {
            let (_, p, _) = list[pos];
            list[pos].2 = true;
            visited.push(p); // one "disk read": fetch p's vector + neighbour list
            for &nb in &self.graph[p] {
                if !seen.insert(nb) {
                    continue;
                }
                let d = dist(q, &self.data[nb]);
                *dist_calls += 1;
                let at = list.partition_point(|e| e.0 < d);
                if at < l {
                    list.insert(at, (d, nb, false));
                    list.truncate(l);
                }
            }
        }
        Searched {
            best: list.into_iter().map(|e| (e.0, e.1)).collect(),
            visited,
        }
    }

    /// RobustPrune(p, V, alpha, R).
    fn robust_prune(&mut self, p: usize, mut cand: Vec<usize>, alpha: f32) {
        cand.extend(self.graph[p].iter().copied());
        cand.retain(|&c| c != p);
        cand.sort_unstable();
        cand.dedup();
        let mut cand: Vec<(f32, usize)> = cand
            .into_iter()
            .map(|c| (dist(&self.data[p], &self.data[c]), c))
            .collect();
        cand.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut out = Vec::with_capacity(self.r);
        while !cand.is_empty() && out.len() < self.r {
            let (_, star) = cand.remove(0); // closest remaining candidate
            out.push(star);
            // Drop candidates that `star` covers: it is (alpha times) closer to them than p is.
            cand.retain(|&(d_pv, v)| alpha * dist(&self.data[star], &self.data[v]) > d_pv);
        }
        self.graph[p] = out;
    }

    /// One full pass over the dataset in random order.
    fn pass(&mut self, alpha: f32, l: usize, rng: &mut Rng) {
        let n = self.data.len();
        let mut order: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            order.swap(i, rng.below(i + 1)); // Fisher-Yates
        }
        let mut sink = 0;
        for p in order {
            let visited = self.greedy_search(&self.data[p], l, &mut sink).visited;
            self.robust_prune(p, visited, alpha);
            for j in self.graph[p].clone() {
                if self.graph[j].contains(&p) {
                    continue;
                }
                if self.graph[j].len() < self.r {
                    self.graph[j].push(p); // room left: plain reverse edge
                } else {
                    self.robust_prune(j, vec![p], alpha); // overflow: re-prune j's list including p
                }
            }
        }
    }

    fn build(
        data: &'a [Vec<f32>],
        r: usize,
        l: usize,
        alpha: f32,
        passes: &[f32],
        rng: &mut Rng,
    ) -> Self {
        let mut v = Self::random_graph(data, r, rng);
        for &a in passes {
            v.pass(a.min(alpha), l, rng);
        }
        v
    }

    /// Beam search = GreedySearch with list size L; returns (ids, node reads, distance computations).
    fn search(&self, q: &[f32], k: usize, l: usize) -> (Vec<usize>, usize, usize) {
        let mut calls = 0;
        let s = self.greedy_search(q, l.max(k), &mut calls);
        (
            s.best.into_iter().take(k).map(|e| e.1).collect(),
            s.visited.len(),
            calls,
        )
    }
}

fn evaluate(
    v: &Vamana,
    queries: &[Vec<f32>],
    truth: &[Vec<usize>],
    k: usize,
    ls: &[usize],
) -> Vec<(usize, f64, f64)> {
    println!(
        "{:>6} {:>10} {:>12} {:>12}",
        "L", "recall@10", "node reads/q", "dist calls/q"
    );
    let mut rows = Vec::new();
    for &l in ls {
        let (mut rec, mut reads, mut calls) = (0.0, 0usize, 0usize);
        for (q, tr) in queries.iter().zip(truth) {
            let (ids, r, c) = v.search(q, k, l);
            rec += recall(&ids, tr);
            reads += r;
            calls += c;
        }
        let nq = queries.len() as f64;
        println!(
            "{l:>6} {:>10.3} {:>12.1} {:>12.0}",
            rec / nq,
            reads as f64 / nq,
            calls as f64 / nq
        );
        rows.push((l, rec / nq, reads as f64 / nq));
    }
    rows
}

fn main() {
    let (n, dim, nq, k, r, l_build, alpha) = (8_000, 32, 200, 10, 32, 64, 1.2f32);
    let mut rng = Rng(0xA076_1D64_78BD_642F);
    let mut all = make_data(n + nq, dim, 20, &mut rng);
    let queries = all.split_off(n);
    let data = all;
    assert!(cosine_dist(&data[0], &data[1]) > 0.0);
    let truth: Vec<Vec<usize>> = queries.iter().map(|q| exact_topk(q, &data, k)).collect();

    let t = std::time::Instant::now();
    let v = Vamana::build(&data, r, l_build, alpha, &[1.0, alpha], &mut rng);
    println!(
        "built R={r} L={l_build} alpha={alpha} in {:.1}s, medoid={}",
        t.elapsed().as_secs_f64(),
        v.medoid
    );
    assert!(
        v.graph.iter().all(|g| g.len() <= r),
        "degree is bounded by R"
    );
    assert!(v.graph.iter().all(|g| !g.is_empty()), "no isolated nodes");

    let rows = evaluate(&v, &queries, &truth, k, &[10, 20, 40, 80, 160]);
    assert!(
        rows.windows(2).all(|w| w[1].1 + 0.01 >= w[0].1),
        "more L must not meaningfully lower recall"
    );
    assert!(
        rows.windows(2).all(|w| w[1].2 >= w[0].2),
        "more L costs more node reads"
    );
    assert!(
        rows.last().unwrap().1 >= 0.95,
        "recall@10 at L=160 was {}",
        rows.last().unwrap().1
    );
    let reads = rows.last().unwrap().2;
    assert!(
        reads < n as f64 / 20.0,
        "disk-style cost: {reads} node reads vs {n} for a full scan"
    );

    // Ablation: a random graph that was never sculpted (0 passes) is far worse at the same effort.
    let raw = Vamana::build(&data, r, l_build, alpha, &[], &mut Rng(7));
    println!("ablation: random graph, no passes");
    let raw_rows = evaluate(&raw, &queries, &truth, k, &[40]);
    let tuned_40 = rows.iter().find(|x| x.0 == 40).unwrap().1;
    assert!(
        tuned_40 > raw_rows[0].1 + 0.2,
        "Vamana passes must improve recall: {tuned_40} vs {}",
        raw_rows[0].1
    );
}
