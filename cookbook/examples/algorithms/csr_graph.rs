//! RECIPE 133 (graphs): CSR (compressed sparse row) graph: counting-sort build, reverse graph, BFS and PageRank vs Vec<Vec<u32>>.
//! LEVEL: intermediate
//! Use in: big read-mostly graphs, sparse matrices, graph analytics; anywhere `Vec<Vec<_>>` costs too much memory or cache misses.   Run: cargo run --release --example csr_graph
#![allow(dead_code)]
//!
//! WHAT: two flat arrays describe the whole graph. `offsets[u]..offsets[u+1]` is the slice of `targets` holding u's
//! out-neighbours, so `neighbors(u)` is just `&targets[offsets[u]..offsets[u+1]]`. Optional edge weights live in a
//! parallel array indexed the same way as `targets`.
//! HOW (build): a counting sort keyed by source node. Pass 1 counts out-degrees, a prefix sum turns counts into start
//! offsets, pass 2 drops every edge into its slot. O(V+E), no per-node allocation, and the sort is stable so neighbour
//! order equals input order (which is why the two representations below give bit-identical PageRank).
//! WHY: `Vec<Vec<u32>>` pays 24 bytes of header per node, one heap allocation per node, spare capacity, and a pointer
//! chase on every neighbour list. CSR is one contiguous scan: fewer cache misses, 4 bytes per edge, trivially
//! serialisable / mmap-able. The price: it is static (inserting an edge means rebuilding), so use it after loading.
//! CSC is just CSR of the reversed graph: it gives in-neighbours, needed for "pull" algorithms and for backward search.
//! COMMON USES: graph databases and analytics engines keep an immutable CSR/CSC adjacency for scans and joins;
//! sparse-matrix libraries store matrices as CSR/CSC and compute SpMV (y = A x) as a loop over rows, which is exactly
//! "for each node, sum over neighbours" (PageRank is repeated SpMV); GNN / graph frameworks (e.g. PyTorch Geometric,
//! DGL) keep message-passing adjacency in compressed sparse formats so aggregation over neighbours is a segmented scan.
//! Read: https://en.wikipedia.org/wiki/Sparse_matrix#Compressed_sparse_row_(CSR,_CRS_or_Yale_format)
//! and https://en.wikipedia.org/wiki/PageRank
use std::time::Instant;

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
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Minimal read-only graph interface so the algorithms run unchanged on both representations.
trait Graph {
    fn n(&self) -> usize;
    fn neighbors(&self, u: usize) -> &[u32];
}

struct Csr {
    offsets: Vec<u32>,
    targets: Vec<u32>,
    weights: Option<Vec<f32>>, // parallel to `targets`
}

impl Graph for Csr {
    fn n(&self) -> usize {
        self.offsets.len() - 1
    }
    fn neighbors(&self, u: usize) -> &[u32] {
        &self.targets[self.offsets[u] as usize..self.offsets[u + 1] as usize]
    }
}
impl Graph for Vec<Vec<u32>> {
    fn n(&self) -> usize {
        self.len()
    }
    fn neighbors(&self, u: usize) -> &[u32] {
        &self[u]
    }
}

impl Csr {
    fn from_edges(n: usize, edges: &[(u32, u32)]) -> Csr {
        Self::build(n, edges, None)
    }

    fn from_weighted(n: usize, edges: &[(u32, u32, f32)]) -> Csr {
        let plain: Vec<(u32, u32)> = edges.iter().map(|&(u, v, _)| (u, v)).collect();
        let w: Vec<f32> = edges.iter().map(|e| e.2).collect();
        Self::build(n, &plain, Some(&w))
    }

    /// Counting sort by source node.
    fn build(n: usize, edges: &[(u32, u32)], w: Option<&[f32]>) -> Csr {
        let mut offsets = vec![0u32; n + 1];
        for &(u, _) in edges {
            offsets[u as usize + 1] += 1; // shifted by one so the prefix sum yields start offsets
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut cursor = offsets.clone(); // next free slot per source node
        let mut targets = vec![0u32; edges.len()];
        let mut weights = w.map(|_| vec![0f32; edges.len()]);
        for (i, &(u, v)) in edges.iter().enumerate() {
            let slot = cursor[u as usize] as usize;
            cursor[u as usize] += 1;
            targets[slot] = v;
            if let (Some(out), Some(src)) = (weights.as_mut(), w) {
                out[slot] = src[i];
            }
        }
        Csr {
            offsets,
            targets,
            weights,
        }
    }

    fn weights_of(&self, u: usize) -> &[f32] {
        let w = self.weights.as_ref().expect("graph has no weights");
        &w[self.offsets[u] as usize..self.offsets[u + 1] as usize]
    }

    /// CSC = CSR of the transposed graph (in-neighbours). Same counting sort, endpoints swapped.
    fn reverse(&self) -> Csr {
        let mut flipped = Vec::with_capacity(self.targets.len());
        for u in 0..self.n() {
            for &v in self.neighbors(u) {
                flipped.push((v, u as u32));
            }
        }
        // weights follow the edges in the same iteration order, so they stay aligned
        Self::build(self.n(), &flipped, self.weights.as_deref())
    }

    fn bytes(&self) -> usize {
        (self.offsets.len() + self.targets.len()) * 4
            + self.weights.as_ref().map_or(0, |w| w.len() * 4)
    }

    /// Sparse matrix-vector product y = A x where A[u][v] = weight(u -> v).
    fn spmv(&self, x: &[f64]) -> Vec<f64> {
        (0..self.n())
            .map(|u| {
                self.neighbors(u)
                    .iter()
                    .zip(self.weights_of(u))
                    .map(|(&v, &w)| w as f64 * x[v as usize])
                    .sum()
            })
            .collect()
    }
}

/// Estimate for the Vec<Vec>: headers + capacity + ~16 B malloc bookkeeping per non-empty list (an estimate, not measured).
fn vecvec_bytes(g: &[Vec<u32>]) -> usize {
    g.len() * 24
        + g.iter()
            .map(|v| v.capacity() * 4 + if v.capacity() > 0 { 16 } else { 0 })
            .sum::<usize>()
}

fn bfs<G: Graph + ?Sized>(g: &G, src: usize) -> Vec<u32> {
    let mut dist = vec![u32::MAX; g.n()];
    let mut queue = vec![src as u32]; // Vec + head index beats VecDeque here: no wrap-around logic
    dist[src] = 0;
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head] as usize;
        head += 1;
        for &v in g.neighbors(u) {
            if dist[v as usize] == u32::MAX {
                dist[v as usize] = dist[u] + 1;
                queue.push(v);
            }
        }
    }
    dist
}

/// Push-style PageRank (power iteration). Dangling nodes (no out-edges) spread their mass uniformly,
/// so total rank stays exactly 1. Stops when the L1 change drops below `tol`.
fn pagerank<G: Graph + ?Sized>(
    g: &G,
    damping: f64,
    tol: f64,
    max_iter: usize,
) -> (Vec<f64>, usize) {
    let n = g.n();
    let mut rank = vec![1.0 / n as f64; n];
    for iter in 1..=max_iter {
        let mut next = vec![0.0; n];
        let mut dangling = 0.0;
        for u in 0..n {
            let nb = g.neighbors(u);
            if nb.is_empty() {
                dangling += rank[u];
                continue;
            }
            let share = rank[u] / nb.len() as f64;
            for &v in nb {
                next[v as usize] += share;
            }
        }
        let base = (1.0 - damping) / n as f64 + damping * dangling / n as f64;
        let mut diff = 0.0;
        for v in 0..n {
            next[v] = base + damping * next[v];
            diff += (next[v] - rank[v]).abs();
        }
        rank = next;
        if diff < tol {
            return (rank, iter);
        }
    }
    (rank, max_iter)
}

/// Pull-style PageRank on the reverse graph (CSC): each node gathers from its in-neighbours. Writes are
/// sequential (good for parallelism: no two threads write one slot), which is why GPUs/GNNs like CSC for aggregation.
fn pagerank_pull(fwd: &Csr, rev: &Csr, damping: f64, tol: f64, max_iter: usize) -> Vec<f64> {
    let n = fwd.n();
    let outdeg: Vec<usize> = (0..n).map(|u| fwd.neighbors(u).len()).collect();
    let mut rank = vec![1.0 / n as f64; n];
    for _ in 0..max_iter {
        let dangling: f64 = (0..n).filter(|&u| outdeg[u] == 0).map(|u| rank[u]).sum();
        let base = (1.0 - damping) / n as f64 + damping * dangling / n as f64;
        let next: Vec<f64> = (0..n)
            .map(|v| {
                base + damping
                    * rev
                        .neighbors(v)
                        .iter()
                        .map(|&u| rank[u as usize] / outdeg[u as usize] as f64)
                        .sum::<f64>()
            })
            .collect();
        let diff: f64 = next.iter().zip(&rank).map(|(a, b)| (a - b).abs()).sum();
        rank = next;
        if diff < tol {
            break;
        }
    }
    rank
}

fn small_demo() {
    let edges = [
        (0, 1, 2.0),
        (0, 2, 3.0),
        (2, 1, 1.0),
        (3, 0, 4.0),
        (0, 3, 5.0),
    ];
    let g = Csr::from_weighted(4, &edges);
    assert_eq!(g.offsets, vec![0, 3, 3, 4, 5]);
    assert_eq!(g.neighbors(0), &[1, 2, 3]); // stable: input order kept
    assert_eq!(g.weights_of(0), &[2.0, 3.0, 5.0]);
    assert!(g.neighbors(1).is_empty());
    let rev = g.reverse();
    assert_eq!(rev.neighbors(1), &[0, 2]);
    assert_eq!(rev.weights_of(1), &[2.0, 1.0]);
    // SpMV against a dense matrix
    let x = [1.0, 10.0, 100.0, 1000.0];
    let mut dense = [[0.0f64; 4]; 4];
    for &(u, v, w) in &edges {
        dense[u as usize][v as usize] = w as f64;
    }
    let y = g.spmv(&x);
    for u in 0..4 {
        let expect: f64 = (0..4).map(|v| dense[u][v] * x[v]).sum();
        assert!((y[u] - expect).abs() < 1e-12);
    }
    println!("small CSR: offsets={:?} targets={:?}", g.offsets, g.targets);
}

fn main() {
    small_demo();

    let (n, m) = (100_000usize, 1_000_000usize);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let edges: Vec<(u32, u32)> = (0..m)
        .map(|_| {
            let u = rng.below(n as u64) as u32;
            let v = ((rng.unit().powi(2)) * n as f64) as u32; // squared => skewed targets, a few hubs
            (u, v.min(n as u32 - 1))
        })
        .collect();

    let t = Instant::now();
    let csr = Csr::from_edges(n, &edges);
    let t_csr = t.elapsed();
    let t = Instant::now();
    let mut vv: Vec<Vec<u32>> = vec![Vec::new(); n];
    for &(u, v) in &edges {
        vv[u as usize].push(v);
    }
    let t_vv = t.elapsed();
    println!("build: CSR {t_csr:?} vs Vec<Vec> {t_vv:?}");
    println!(
        "memory: CSR {} KiB vs Vec<Vec> ~{} KiB (estimate)",
        csr.bytes() / 1024,
        vecvec_bytes(&vv) / 1024
    );
    assert!(csr.bytes() < vecvec_bytes(&vv));
    assert!((0..n).all(|u| csr.neighbors(u) == vv[u].as_slice()));

    // BFS from several sources: same distances, compare speed
    let sources = [0usize, 17, 4242, 99_999, 31_337, 5, 777, 12_345];
    let t = Instant::now();
    let d_csr: Vec<Vec<u32>> = sources.iter().map(|&s| bfs(&csr, s)).collect();
    let bfs_csr = t.elapsed();
    let t = Instant::now();
    let d_vv: Vec<Vec<u32>> = sources.iter().map(|&s| bfs(&vv, s)).collect();
    let bfs_vv = t.elapsed();
    assert_eq!(d_csr, d_vv);
    let reached = d_csr[0].iter().filter(|&&d| d != u32::MAX).count();
    println!(
        "BFS x{}: CSR {bfs_csr:?} vs Vec<Vec> {bfs_vv:?} (reached {reached} of {n} from node 0)",
        sources.len()
    );

    // CSC: every edge u->v must appear as v's in-neighbour u; degree sums must match
    let rev = csr.reverse();
    assert_eq!(rev.targets.len(), m);
    let mut fwd_sorted = edges.clone();
    fwd_sorted.sort();
    let mut rev_edges: Vec<(u32, u32)> = (0..n)
        .flat_map(|v| rev.neighbors(v).iter().map(move |&u| (u, v as u32)))
        .collect();
    rev_edges.sort();
    assert_eq!(fwd_sorted, rev_edges);

    // PageRank on both representations, plus the pull variant over CSC
    let t = Instant::now();
    let (pr_csr, it1) = pagerank(&csr, 0.85, 1e-12, 200);
    let pr_csr_t = t.elapsed();
    let t = Instant::now();
    let (pr_vv, it2) = pagerank(&vv, 0.85, 1e-12, 200);
    let pr_vv_t = t.elapsed();
    let pr_pull = pagerank_pull(&csr, &rev, 0.85, 1e-12, 200);
    let max_diff = |a: &[f64], b: &[f64]| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max)
    };
    assert_eq!(it1, it2);
    assert!(max_diff(&pr_csr, &pr_vv) < 1e-9);
    assert!(max_diff(&pr_csr, &pr_pull) < 1e-9);
    let sum: f64 = pr_csr.iter().sum();
    assert!((sum - 1.0).abs() < 1e-9, "sum = {sum}");
    let (top, best) = pr_csr
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    println!(
        "PageRank: {it1} iterations, CSR {pr_csr_t:?} vs Vec<Vec> {pr_vv_t:?}; sum={sum:.12}; top node {top} rank {best:.3e}"
    );
    println!("OK");
}
