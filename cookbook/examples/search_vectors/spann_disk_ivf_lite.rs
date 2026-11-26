//! RECIPE 874 (vectors): SPANN-style disk-resident ANN: centroids in RAM, posting lists on a simulated SSD, boundary replication, query-aware pruning, I/O accounting vs a DiskANN-style graph.
//! LEVEL: advanced
//! Use in: billion-scale vector search where the vectors do not fit in RAM and the SSD is the bottleneck: count BLOCK READS, not distance computations.   Run: cargo run --release --example spann_disk_ivf_lite
#![allow(dead_code)]
//!
//! THE COST MODEL: on an SSD one random 4 KiB read costs ~80-100 us at queue depth 1, and the next sequential blocks of the same request cost ~5-10 us each.
//! So an index is judged by (a) how many requests ("seeks") a query issues and (b) how many blocks they cover, at a given recall. The simulated
//! `Device` counts both, and we print a modelled latency (80 us per seek + 8 us per block; a model, not a measurement).
//! SPANN (Chen et al., NeurIPS 2021) design, reproduced in miniature:
//!   * HEAD in memory: ~n/100 centroids from BALANCED k-means (capacity cap = 1.25 n/k, because a huge posting list is a latency outlier).
//!     CAVEAT of this miniature: the cap bounds the k-means assignment only. The lists are then rebuilt by nearest-centroid + closure assignment,
//!     which can exceed it (the run prints the largest list). A production system has to split oversized postings (see SPFresh).
//!   * POSTING LISTS on disk: all vectors assigned to a centroid, stored contiguously: ONE seek reads a whole list.
//!   * BOUNDARY REPLICATION ("closure assignment" + RNG pruning): a vector near the boundary of two cells is stored in BOTH lists so a query that
//!     lands on the "wrong" side still finds it. Replica rule for vector x with sorted centroid distances d_1 <= d_2 <= ...: take c_j only if
//!     d_j <= (1+eps) d_1 (it is almost as close as the nearest) AND for every already chosen c_s, d(c_j, c_s) >= d(x, c_j) (RNG rule: if c_j is
//!     closer to a chosen centroid than to x, it lies "behind" it and adds no new direction). At most `R_MAX` replicas. Costs disk space, saves I/O per query.
//!   * QUERY-AWARE DYNAMIC PRUNING: probe centroid i only while d_i(q) <= (1+eps_q) d_1(q) (up to `nprobe`): easy queries (one clear cell) read
//!     one list, ambiguous queries read several. It is a HEURISTIC and its value depends on the data (see below).
//!   * PAGE-ALIGNED LAYOUT: each list starts on a 4 KiB boundary, so a list of L bytes costs ceil(L/4096) blocks instead of up to one more when packed.
//! DISKANN-STYLE BASELINE: graph (R=24 links) + in-RAM SQ8 copy of vectors for navigation; each node's record [degree, links, full vector] lives in its
//! own slot (17 per block); expanding a node = one block read; the answer is re-ranked with the full vectors read along the way.
//!
//! VERIFIED: (1) with nothing pruned and every list probed, recall is exactly 1.0 (every vector is in >= 1 list); (2) the block reads the device
//! counted equal the layout model, for packed and aligned layouts, for every query; aligned never reads more blocks than packed; (3) balanced
//! k-means respects its capacity; sampled replica sets obey the closure and RNG rules; (4) at a FIXED nprobe, replication (same centroids) never
//! lowers recall (replicated lists are supersets) and raises recall@10 at nprobe=1 by more than 0.1 on low-intrinsic-dimension data;
//! (5) AT EQUAL MEAN BLOCKS READ, dynamic pruning has higher recall than fixed nprobe on clustered and low-intrinsic-dimension data (eps_q 0.05
//! and 0.10) and LOWER recall on near-isotropic data (eps_q 0.10); (6) both indexes reach recall 0.95.
//! HOW TO COMPARE PRUNING HONESTLY: "pruned reads fewer blocks than nprobe=16" proves nothing, because it also has lower recall. Compare recall at
//! the SAME mean I/O. The fixed-nprobe curve between p and p+1 is linearly interpolated; that point is achievable (send a random fraction of
//! queries to p+1). WHERE IT FAILS: when the data is close to one isotropic blob, the ratio d_i(q)/d_1(q) says little about which queries need
//! more lists, and here the uneven budget does worse than giving every query the same number of lists. Measure on your own data before enabling it.
//! The gain where it does work is a few recall points in this miniature, not a multiple.
//! WHY REPLICATION BARELY FIRES ON ISOTROPIC DATA (read the x factor in the first table): centroids are averages of noisy points, so they tend to sit
//! closer to each other than a point sits to them, and the RNG rule then rejects the second centroid.
//! HONEST NOTE: the comparison I/O-per-recall depends on data, entry size and parameters; the graph here keeps SQ8 vectors in RAM (8 bits/dim; DiskANN uses ~32 byte PQ codes)
//! and counts every expansion as a read; SPANN keeps only centroids. Read the numbers as a demonstration of the accounting, not a benchmark verdict.
//! COMMON USES: SPANN (Microsoft SPTAG, Bing-scale), Pinecone/Turbopuffer-style object-storage IVF layouts (a posting list = one object range-GET), DiskANN/Vamana in
//! Azure and in Milvus, Lucene on SSD, Faiss IVF with on-disk inverted lists (`OnDiskInvertedLists`).
//! Read: SPANN: Highly-efficient Billion-scale Approximate Nearest Neighbor Search, Chen et al. https://arxiv.org/abs/2111.08566 ;
//! DiskANN: Fast Accurate Billion-point Nearest Neighbor Search on a Single Node, Subramanya et al. https://proceedings.neurips.cc/paper/2019/file/09853c7fb1d3f8ee67a61b6bf4a7f8e6-Paper.pdf ;
//! FreshDiskANN https://arxiv.org/abs/2105.09613 ; https://arxiv.org/abs/1603.09320 (HNSW).
//!
//! TRY NEXT:
//!   - [S] Predict, then measure: set `R_MAX` to 2, 4 and 8 in `Spann::build`. Plot posting-space amplification against recall at nprobe=3; where do returns diminish?
//!   - [S] Change `BLOCK` to 512 and to 16384. Which layout (packed or aligned) loses more, and does the SPANN-vs-graph block-count ordering flip?
//!   - [M] Replace the brute-force centroid scan in `Spann::search` with a small graph over centroids (reuse the `Graph` type here) and count the extra RAM-only distance computations.
//!   - [M] Real-world port: model an object store (S3-like) where a request costs 15 ms regardless of size up to 1 MiB; rewrite `Device::latency_us` and find the best posting size.
//!   - [A] PROJECT: store the posting entries as int8 codes (see `scalar_binary_quantization_rescoring`) plus a separate f32 file read only for the top `k*oversample`; measure blocks/query, and put the router in front of the segments of `vector_index_updates_persistence`.
use std::cell::Cell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashSet};

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
struct Mat {
    n: usize,
    dim: usize,
    v: Vec<f32>,
}
impl Mat {
    fn row(&self, i: usize) -> &[f32] {
        &self.v[i * self.dim..(i + 1) * self.dim]
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
fn anisotropic(n: usize, dim: usize, latent: usize, rng: &mut Rng) -> Mat {
    let w: Vec<f32> = (0..dim * latent)
        .map(|i| rng.gauss() / (1.0 + (i % latent) as f32 * 0.5))
        .collect();
    let mut v = Vec::with_capacity(n * dim);
    for _ in 0..n {
        let z: Vec<f32> = (0..latent).map(|_| rng.gauss()).collect();
        v.extend((0..dim).map(|d| {
            (0..latent).map(|j| w[d * latent + j] * z[j]).sum::<f32>() + rng.gauss() * 0.05
        }));
    }
    Mat { n, dim, v }
}
fn l2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
fn exact_topk(q: &[f32], data: &Mat, k: usize) -> Vec<u32> {
    let mut v: Vec<(f32, u32)> = (0..data.n)
        .map(|i| (l2(q, data.row(i)), i as u32))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v.into_iter().take(k).map(|p| p.1).collect()
}
fn recall(a: &[u32], e: &[u32]) -> f64 {
    a.iter().filter(|x| e.contains(x)).count() as f64 / e.len() as f64
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

// ---------- the simulated block device ----------
const BLOCK: usize = 4096;
struct Device {
    data: Vec<u8>,
    blocks_read: Cell<u64>,
    requests: Cell<u64>,
}
impl Device {
    fn new(data: Vec<u8>) -> Self {
        Device {
            data,
            blocks_read: Cell::new(0),
            requests: Cell::new(0),
        }
    }
    /// One request for [off, off+len): costs one seek plus every block it touches.
    /// An empty range issues no request (an empty posting list is skipped, matching `model_blocks`).
    fn read(&self, off: usize, len: usize) -> &[u8] {
        if len == 0 {
            return &[];
        }
        let (first, last) = (off / BLOCK, (off + len - 1) / BLOCK);
        self.blocks_read
            .set(self.blocks_read.get() + (last - first + 1) as u64);
        self.requests.set(self.requests.get() + 1);
        &self.data[off..off + len]
    }
    fn reset(&self) {
        self.blocks_read.set(0);
        self.requests.set(0);
    }
    fn latency_us(&self) -> f64 {
        self.requests.get() as f64 * 80.0
            + (self.blocks_read.get() - self.requests.get().min(self.blocks_read.get())) as f64
                * 8.0
    }
    fn size_bytes(&self) -> usize {
        self.data.len()
    }
}
/// What the layout model predicts for a request, independent of the device code.
fn model_blocks(off: usize, len: usize) -> u64 {
    if len == 0 {
        0
    } else {
        ((off % BLOCK + len).div_ceil(BLOCK)) as u64
    }
}

// ---------- balanced k-means (capacity-constrained greedy assignment) ----------
fn balanced_kmeans(
    data: &Mat,
    k: usize,
    cap: usize,
    iters: usize,
    rng: &mut Rng,
) -> (Vec<f32>, Vec<usize>) {
    let dim = data.dim;
    let mut cents: Vec<f32> = (0..k)
        .flat_map(|_| data.row(rng.below(data.n)).to_vec())
        .collect();
    let mut assign = vec![0usize; data.n];
    for _ in 0..iters {
        // each point's centroid preference order; points with the clearest preference choose first
        let mut order: Vec<(f32, usize, Vec<(f32, usize)>)> = (0..data.n)
            .map(|i| {
                let mut ds: Vec<(f32, usize)> = (0..k)
                    .map(|c| (l2(data.row(i), &cents[c * dim..(c + 1) * dim]), c))
                    .collect();
                ds.sort_by(|a, b| a.0.total_cmp(&b.0));
                (ds[0].0, i, ds)
            })
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut load = vec![0usize; k];
        for (_, i, ds) in &order {
            let c = ds.iter().find(|(_, c)| load[*c] < cap).unwrap().1;
            load[c] += 1;
            assign[*i] = c;
        }
        let mut sums = vec![0.0f32; k * dim];
        for i in 0..data.n {
            for d in 0..dim {
                sums[assign[i] * dim + d] += data.row(i)[d];
            }
        }
        for c in 0..k {
            if load[c] > 0 {
                for d in 0..dim {
                    cents[c * dim + d] = sums[c * dim + d] / load[c] as f32;
                }
            } else {
                cents[c * dim..(c + 1) * dim].copy_from_slice(data.row(rng.below(data.n)));
            }
        }
    }
    (cents, assign)
}

// ---------- SPANN-lite ----------
const ENTRY_BYTES_PER_DIM: usize = 4;
/// Closure slack: a centroid within (1+eps) of the nearest one is a replica candidate. In high dimensions distances
/// concentrate (d_2/d_1 is close to 1), so useful eps values are larger than intuition from 2-D pictures suggests.
const REPL_EPS: f32 = 0.25;
#[derive(Clone, Copy, PartialEq)]
enum Layout {
    Packed,
    Aligned,
}
struct Spann {
    dim: usize,
    k: usize,
    cents: Vec<f32>,
    dir: Vec<(usize, usize)>, // (byte offset, entries) per posting list
    dev: Device,
    entry_bytes: usize,
    replicas_total: usize,
    max_replicas: usize,
}
struct QueryStats {
    postings: usize,
    scanned: usize,
}
impl Spann {
    fn centroid(&self, c: usize) -> &[f32] {
        &self.cents[c * self.dim..(c + 1) * self.dim]
    }
    /// `eps_closure` = 0 or r_max = 1 disables replication.
    fn build(
        data: &Mat,
        cents: &[f32],
        k: usize,
        eps_closure: f32,
        r_max: usize,
        layout: Layout,
    ) -> (Spann, Vec<Vec<usize>>) {
        let dim = data.dim;
        let mut lists: Vec<Vec<u32>> = vec![vec![]; k];
        let mut chosen_all = Vec::with_capacity(data.n);
        let cc = |a: usize, b: usize| {
            l2(
                &cents[a * dim..(a + 1) * dim],
                &cents[b * dim..(b + 1) * dim],
            )
        };
        for i in 0..data.n {
            let mut ds: Vec<(f32, usize)> = (0..k)
                .map(|c| (l2(data.row(i), &cents[c * dim..(c + 1) * dim]), c))
                .collect();
            ds.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut chosen: Vec<usize> = vec![ds[0].1];
            for &(d, c) in ds.iter().skip(1).take(15) {
                if chosen.len() >= r_max {
                    break;
                }
                let close_enough = d.sqrt() <= (1.0 + eps_closure) * ds[0].0.sqrt(); // closure: almost as near as the nearest
                let rng_ok = chosen.iter().all(|&s| cc(c, s) >= d); // RNG: not "behind" an already chosen centroid
                if close_enough && rng_ok {
                    chosen.push(c);
                }
            }
            for &c in &chosen {
                lists[c].push(i as u32);
            }
            chosen_all.push(chosen);
        }
        let entry_bytes = 4 + dim * ENTRY_BYTES_PER_DIM;
        let (mut bytes, mut dir) = (Vec::new(), Vec::with_capacity(k));
        for l in &lists {
            if layout == Layout::Aligned {
                bytes.resize(bytes.len().div_ceil(BLOCK) * BLOCK, 0); // pad so every list starts on a block boundary
            }
            dir.push((bytes.len(), l.len()));
            for &id in l {
                bytes.extend_from_slice(&id.to_le_bytes());
                for x in data.row(id as usize) {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
            }
        }
        let replicas_total = lists.iter().map(|l| l.len()).sum();
        let max_replicas = chosen_all.iter().map(|c| c.len()).max().unwrap();
        (
            Spann {
                dim,
                k,
                cents: cents.to_vec(),
                dir,
                dev: Device::new(bytes),
                entry_bytes,
                replicas_total,
                max_replicas,
            },
            chosen_all,
        )
    }
    /// Probe centroids nearest-first while rank < nprobe and d_i <= (1+eps_q) d_1; read each list with one request.
    fn search(&self, q: &[f32], k: usize, nprobe: usize, eps_q: f32) -> (Vec<u32>, QueryStats) {
        let mut cd: Vec<(f32, usize)> = (0..self.k).map(|c| (l2(q, self.centroid(c)), c)).collect();
        cd.sort_by(|a, b| a.0.total_cmp(&b.0));
        let d1 = cd[0].0.sqrt();
        let (mut seen, mut cand, mut st) = (
            HashSet::new(),
            Vec::new(),
            QueryStats {
                postings: 0,
                scanned: 0,
            },
        );
        for (rank, &(d, c)) in cd.iter().enumerate() {
            if rank >= nprobe || d.sqrt() > (1.0 + eps_q) * d1 {
                break;
            }
            let (off, cnt) = self.dir[c];
            let buf = self.dev.read(off, cnt * self.entry_bytes);
            st.postings += 1;
            for e in buf.chunks_exact(self.entry_bytes) {
                let id = u32::from_le_bytes(e[..4].try_into().unwrap());
                st.scanned += 1;
                if !seen.insert(id) {
                    continue; // replica already scored via another list
                }
                let v: Vec<f32> = e[4..]
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                cand.push((l2(q, &v), id));
            }
        }
        cand.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        (cand.into_iter().take(k).map(|p| p.1).collect(), st)
    }
    /// Blocks the layout model says a probe of these centroids must read.
    fn model_probe_blocks(&self, q: &[f32], nprobe: usize, eps_q: f32) -> u64 {
        let mut cd: Vec<(f32, usize)> = (0..self.k).map(|c| (l2(q, self.centroid(c)), c)).collect();
        cd.sort_by(|a, b| a.0.total_cmp(&b.0));
        let d1 = cd[0].0.sqrt();
        cd.iter()
            .enumerate()
            .take_while(|(r, (d, _))| *r < nprobe && d.sqrt() <= (1.0 + eps_q) * d1)
            .map(|(_, (_, c))| model_blocks(self.dir[*c].0, self.dir[*c].1 * self.entry_bytes))
            .sum()
    }
}

// ---------- DiskANN-style baseline: graph + in-RAM SQ8 + on-"disk" node records ----------
const R: usize = 24;
struct Graph {
    nbrs: Vec<Vec<u32>>,
    entry: u32,
}
impl Graph {
    fn build(data: &Mat, m: usize, efc: usize) -> Graph {
        let n = data.n;
        let mut mean = vec![0.0f32; data.dim];
        for i in 0..n {
            for d in 0..data.dim {
                mean[d] += data.row(i)[d] / n as f32;
            }
        }
        let entry = (0..n)
            .min_by(|&a, &b| l2(&mean, data.row(a)).total_cmp(&l2(&mean, data.row(b))))
            .unwrap() as u32;
        let mut g = Graph {
            nbrs: vec![vec![]; n],
            entry,
        };
        let mut order: Vec<usize> = (0..n).filter(|&i| i as u32 != entry).collect();
        let mut rng = Rng(77);
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i + 1));
        }
        for id in order {
            let found = g.search(data, data.row(id), efc);
            let chosen = g.select(data, &found, m);
            for nb in chosen {
                g.nbrs[id].push(nb);
                g.nbrs[nb as usize].push(id as u32);
                if g.nbrs[nb as usize].len() > 2 * m {
                    let mut c: Vec<(f32, u32)> = g.nbrs[nb as usize]
                        .iter()
                        .map(|&x| (l2(data.row(nb as usize), data.row(x as usize)), x))
                        .collect();
                    c.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                    g.nbrs[nb as usize] = g.select(data, &c, 2 * m);
                }
            }
        }
        g
    }
    fn select(&self, data: &Mat, cands: &[(f32, u32)], m: usize) -> Vec<u32> {
        let (mut kept, mut dropped): (Vec<(f32, u32)>, Vec<u32>) = (vec![], vec![]);
        for &(dq, e) in cands {
            if kept.len() >= m {
                break;
            }
            if kept
                .iter()
                .any(|&(_, r)| l2(data.row(e as usize), data.row(r as usize)) < dq)
            {
                dropped.push(e)
            } else {
                kept.push((dq, e))
            }
        }
        let mut out: Vec<u32> = kept.into_iter().map(|k| k.1).collect();
        let room = m - out.len();
        out.extend(dropped.into_iter().take(room));
        out
    }
    fn search(&self, data: &Mat, q: &[f32], ef: usize) -> Vec<(f32, u32)> {
        let mut seen = vec![false; data.n];
        let mut cand: BinaryHeap<Reverse<(F, u32)>> = BinaryHeap::new();
        let mut res: BinaryHeap<(F, u32)> = BinaryHeap::new();
        seen[self.entry as usize] = true;
        let d0 = F(l2(q, data.row(self.entry as usize)));
        cand.push(Reverse((d0, self.entry)));
        res.push((d0, self.entry));
        while let Some(Reverse((d, u))) = cand.pop() {
            if res.len() >= ef && d > res.peek().unwrap().0 {
                break;
            }
            for &v in &self.nbrs[u as usize] {
                if std::mem::replace(&mut seen[v as usize], true) {
                    continue;
                }
                let dv = F(l2(q, data.row(v as usize)));
                if res.len() < ef || dv < res.peek().unwrap().0 {
                    cand.push(Reverse((dv, v)));
                    res.push((dv, v));
                    if res.len() > ef {
                        res.pop();
                    }
                }
            }
        }
        let mut out: Vec<(f32, u32)> = res.into_iter().map(|(d, i)| (d.0, i)).collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        out
    }
}
struct DiskGraph {
    dim: usize,
    entry: u32,
    rec_bytes: usize,
    per_block: usize,
    dev: Device,
    mins: Vec<f32>,
    steps: Vec<f32>,
    sq: Vec<u8>, // in-RAM navigation copy
}
impl DiskGraph {
    fn build(data: &Mat, g: &Graph) -> DiskGraph {
        let dim = data.dim;
        let rec_bytes = 4 + 4 * 2 * 12 + dim * 4; // degree + up to 24 links + vector
        let per_block = BLOCK / rec_bytes; // records never straddle a block
        let blocks = data.n.div_ceil(per_block);
        let mut bytes = vec![0u8; blocks * BLOCK];
        for i in 0..data.n {
            let off = (i / per_block) * BLOCK + (i % per_block) * rec_bytes;
            let nb = &g.nbrs[i];
            assert!(nb.len() <= R);
            bytes[off..off + 4].copy_from_slice(&(nb.len() as u32).to_le_bytes());
            for (j, x) in nb.iter().enumerate() {
                bytes[off + 4 + 4 * j..off + 8 + 4 * j].copy_from_slice(&x.to_le_bytes());
            }
            for (j, x) in data.row(i).iter().enumerate() {
                bytes[off + 4 + 4 * R + 4 * j..off + 8 + 4 * R + 4 * j]
                    .copy_from_slice(&x.to_le_bytes());
            }
        }
        let (mut mins, mut maxs) = (vec![f32::INFINITY; dim], vec![f32::NEG_INFINITY; dim]);
        for i in 0..data.n {
            for d in 0..dim {
                mins[d] = mins[d].min(data.row(i)[d]);
                maxs[d] = maxs[d].max(data.row(i)[d]);
            }
        }
        let steps: Vec<f32> = (0..dim)
            .map(|d| ((maxs[d] - mins[d]) / 255.0).max(1e-9))
            .collect();
        let sq = (0..data.n)
            .flat_map(|i| {
                (0..dim)
                    .map(|d| {
                        ((data.row(i)[d] - mins[d]) / steps[d])
                            .round()
                            .clamp(0.0, 255.0) as u8
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        DiskGraph {
            dim,
            entry: g.entry,
            rec_bytes,
            per_block,
            dev: Device::new(bytes),
            mins,
            steps,
            sq,
        }
    }
    fn sq_dist(&self, q: &[f32], id: u32) -> f32 {
        let c = &self.sq[id as usize * self.dim..(id as usize + 1) * self.dim];
        (0..self.dim)
            .map(|d| (q[d] - (self.mins[d] + c[d] as f32 * self.steps[d])).powi(2))
            .sum()
    }
    /// Beam search with list size `l`; one device request per expanded node. Returns (top-k by exact distance, expansions).
    fn search(&self, q: &[f32], k: usize, l: usize) -> (Vec<u32>, usize) {
        let mut list: Vec<(f32, u32, bool)> =
            vec![(self.sq_dist(q, self.entry), self.entry, false)];
        let mut visited: HashSet<u32> = HashSet::from([self.entry]);
        let mut exact: Vec<(f32, u32)> = vec![];
        let mut expansions = 0;
        while let Some(pos) = list.iter().position(|e| !e.2) {
            let (_, id, _) = list[pos];
            list[pos].2 = true;
            let off = (id as usize / self.per_block) * BLOCK
                + (id as usize % self.per_block) * self.rec_bytes;
            let rec = self.dev.read(off, self.rec_bytes);
            expansions += 1;
            let deg = u32::from_le_bytes(rec[..4].try_into().unwrap()) as usize;
            let v: Vec<f32> = rec[4 + 4 * R..]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            exact.push((l2(q, &v), id));
            for j in 0..deg {
                let nb = u32::from_le_bytes(rec[4 + 4 * j..8 + 4 * j].try_into().unwrap());
                if visited.insert(nb) {
                    list.push((self.sq_dist(q, nb), nb, false));
                }
            }
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            list.truncate(l);
        }
        exact.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        (exact.into_iter().take(k).map(|p| p.1).collect(), expansions)
    }
}

// ---------- experiments ----------
struct Row {
    label: String,
    recall: f64,
    blocks: f64,
    requests: f64,
    lat: f64,
}
fn run_spann(
    sp: &Spann,
    queries: &Mat,
    truth: &[Vec<u32>],
    nprobe: usize,
    eps_q: f32,
    label: &str,
) -> Row {
    sp.dev.reset();
    let mut r = 0.0;
    for (i, t) in truth.iter().enumerate() {
        r += recall(&sp.search(queries.row(i), 10, nprobe, eps_q).0, t);
    }
    let nq = truth.len() as f64;
    Row {
        label: label.into(),
        recall: r / nq,
        blocks: sp.dev.blocks_read.get() as f64 / nq,
        requests: sp.dev.requests.get() as f64 / nq,
        lat: sp.dev.latency_us() / nq,
    }
}
fn print_rows(title: &str, rows: &[Row]) {
    println!("{title}");
    println!(
        "  {:<26} {:>9} {:>10} {:>9} {:>12}",
        "config", "recall@10", "blocks/q", "reads/q", "model us/q"
    );
    for r in rows {
        println!(
            "  {:<26} {:>9.3} {:>10.1} {:>9.1} {:>12.0}",
            r.label, r.recall, r.blocks, r.requests, r.lat
        );
    }
}

fn replication_study(
    name: &str,
    data: &Mat,
    queries: &Mat,
    k_cent: usize,
    must_gain: bool,
    rng: &mut Rng,
) {
    let truth: Vec<Vec<u32>> = (0..queries.n)
        .map(|i| exact_topk(queries.row(i), data, 10))
        .collect();
    let cap = (1.25 * data.n as f64 / k_cent as f64).ceil() as usize;
    let (cents, assign) = balanced_kmeans(data, k_cent, cap, 6, rng);
    let mut load = vec![0usize; k_cent];
    for a in &assign {
        load[*a] += 1;
    }
    assert!(
        *load.iter().max().unwrap() <= cap,
        "balanced k-means exceeded its capacity"
    );
    let (plain, _) = Spann::build(data, &cents, k_cent, 0.0, 1, Layout::Aligned);
    let (repl, chosen) = Spann::build(data, &cents, k_cent, REPL_EPS, 8, Layout::Aligned);
    // closure + RNG invariants for a sample of vectors
    for i in (0..data.n).step_by(37) {
        let ch = &chosen[i];
        let d = |c: usize| l2(data.row(i), &cents[c * data.dim..(c + 1) * data.dim]);
        let best = (0..k_cent).map(d).fold(f32::INFINITY, f32::min);
        assert!(
            (d(ch[0]) - best).abs() <= 1e-4 * best.max(1.0),
            "first replica must be the nearest centroid"
        );
        for (j, &c) in ch.iter().enumerate().skip(1) {
            assert!(
                d(c).sqrt() <= (1.0 + REPL_EPS) * best.sqrt() + 1e-4,
                "closure rule violated"
            );
            for &s in &ch[..j] {
                let cs = l2(
                    &cents[c * data.dim..(c + 1) * data.dim],
                    &cents[s * data.dim..(s + 1) * data.dim],
                );
                assert!(cs >= d(c) - 1e-4, "RNG rule violated");
            }
        }
    }
    // every vector is in at least one list
    assert!(chosen.iter().all(|c| !c.is_empty()));
    let mut line = format!(
        "{name:<18} replication x{:.2} (max {} copies), space {:.1} MiB vs {:.1} MiB |",
        repl.replicas_total as f64 / data.n as f64,
        repl.max_replicas,
        repl.dev.size_bytes() as f64 / 1048576.0,
        plain.dev.size_bytes() as f64 / 1048576.0
    );
    let mut gain_np1 = 0.0;
    for nprobe in [1usize, 2, 3, 5] {
        let (a, b) = (
            run_spann(&plain, queries, &truth, nprobe, 1e9, ""),
            run_spann(&repl, queries, &truth, nprobe, 1e9, ""),
        );
        line += &format!(" np={nprobe}: {:.3}->{:.3}", a.recall, b.recall);
        assert!(
            b.recall + 1e-12 >= a.recall,
            "{name}: replication lowered recall at nprobe {nprobe} ({:.3} -> {:.3})",
            a.recall,
            b.recall
        );
        if nprobe == 1 {
            gain_np1 = b.recall - a.recall;
        }
    }
    println!("{line}");
    if must_gain {
        assert!(
            gain_np1 > 0.1,
            "{name}: replication should raise recall@10 at nprobe=1 on boundary-heavy data (gain {gain_np1:.3})"
        );
    }
}

/// Recall of fixed probing when it is given the same mean number of blocks: linear interpolation on the fixed-nprobe curve.
/// `fixed[i]` is the row for nprobe = i + 1. The chord is achievable by mixing the two neighbouring nprobe values at random.
fn fixed_recall_at(fixed: &[Row], blocks: f64) -> f64 {
    let i = fixed
        .iter()
        .position(|r| r.blocks >= blocks)
        .expect("fixed curve must reach this block budget");
    if i == 0 {
        return fixed[0].recall; // flatters fixed probing slightly: below nprobe=1 it gets the full nprobe=1 recall
    }
    let (a, b) = (&fixed[i - 1], &fixed[i]);
    a.recall + (b.recall - a.recall) * (blocks - a.blocks) / (b.blocks - a.blocks)
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    /// Clusters or a low-dimensional manifold: pruning is expected to win.
    Structured,
    /// Close to one isotropic blob: pruning is expected to lose.
    Isotropic,
}

/// Dynamic pruning against fixed nprobe at EQUAL mean blocks read.
fn pruning_study(
    name: &str,
    data: &Mat,
    queries: &Mat,
    k_cent: usize,
    shape: Shape,
    rng: &mut Rng,
) {
    let truth: Vec<Vec<u32>> = (0..queries.n)
        .map(|i| exact_topk(queries.row(i), data, 10))
        .collect();
    let cap = (1.25 * data.n as f64 / k_cent as f64).ceil() as usize;
    let (cents, _) = balanced_kmeans(data, k_cent, cap, 6, rng);
    let (sp, _) = Spann::build(data, &cents, k_cent, REPL_EPS, 8, Layout::Aligned);
    let fixed: Vec<Row> = (1..=k_cent)
        .map(|p| run_spann(&sp, queries, &truth, p, 1e9, ""))
        .collect();
    let mut line = format!("{name:<18}");
    for eps in [0.05f32, 0.10, 0.20] {
        let pruned = run_spann(&sp, queries, &truth, k_cent, eps, "");
        let delta = pruned.recall - fixed_recall_at(&fixed, pruned.blocks);
        line += &format!(
            " | eps_q={eps:.2}: {:.3} at {:>5.1} blocks ({delta:+.3})",
            pruned.recall, pruned.blocks
        );
        // The data and seed are fixed, so these are deterministic. eps_q=0.20 is printed only: on the manifold set the sign flips there.
        match shape {
            Shape::Structured if eps < 0.15 => assert!(
                delta > 0.005,
                "{name}: pruning should beat fixed nprobe at equal I/O (eps_q {eps}, delta {delta:+.4})"
            ),
            Shape::Isotropic if eps == 0.10 => assert!(
                delta < 0.0,
                "{name}: expected the documented failure case, pruning below fixed nprobe (delta {delta:+.4})"
            ),
            _ => {}
        }
    }
    println!("{line}");
}

fn main() {
    let (n, dim, nq, k_cent) = (16_000usize, 32usize, 150usize, 160usize);
    let mut rng = Rng(0x874_874);

    println!("=== replication at FIXED nprobe (same centroids; recall@10 plain -> replicated) ===");
    for (name, all, must) in [
        (
            "separated",
            clusters(6000 + 100, dim, 40, 3.0, 0.5, &mut rng),
            false,
        ),
        (
            "near-isotropic",
            clusters(6000 + 100, dim, 40, 0.5, 1.0, &mut rng),
            false,
        ),
        // a continuous 8-dim manifold: every cell touches neighbours, so a large share of the points are boundary points
        (
            "boundary-heavy",
            anisotropic(6000 + 100, dim, 8, &mut rng),
            true,
        ),
        ("uniform", uniform(6000 + 100, dim, &mut rng), false),
    ] {
        let queries = Mat {
            n: 100,
            dim,
            v: all.v[6000 * dim..].to_vec(),
        };
        let data = Mat {
            n: 6000,
            dim,
            v: all.v[..6000 * dim].to_vec(),
        };
        replication_study(name, &data, &queries, 60, must, &mut rng);
    }

    println!(
        "\n=== query-aware pruning vs fixed nprobe at EQUAL mean blocks (recall@10; in brackets: pruned minus fixed) ==="
    );
    for (name, all, shape) in [
        (
            "clustered",
            clusters(6000 + 300, dim, 40, 1.2, 1.0, &mut rng),
            Shape::Structured,
        ),
        (
            "separated",
            clusters(6000 + 300, dim, 40, 3.0, 0.5, &mut rng),
            Shape::Structured,
        ),
        (
            "manifold (8-dim)",
            anisotropic(6000 + 300, dim, 8, &mut rng),
            Shape::Structured,
        ),
        (
            "near-isotropic",
            clusters(6000 + 300, dim, 40, 0.5, 1.0, &mut rng),
            Shape::Isotropic,
        ),
        (
            "uniform",
            uniform(6000 + 300, dim, &mut rng),
            Shape::Isotropic,
        ),
    ] {
        let queries = Mat {
            n: 300,
            dim,
            v: all.v[6000 * dim..].to_vec(),
        };
        let data = Mat {
            n: 6000,
            dim,
            v: all.v[..6000 * dim].to_vec(),
        };
        pruning_study(name, &data, &queries, 60, shape, &mut rng);
    }

    println!(
        "\n=== main comparison on an 8-dim manifold embedded in {dim} dims: n={n}, {k_cent} centroids, entry = {} bytes, block = {BLOCK} ===",
        4 + 4 * dim
    );
    let all = anisotropic(n + nq, dim, 8, &mut rng);
    let queries = Mat {
        n: nq,
        dim,
        v: all.v[n * dim..].to_vec(),
    };
    let data = Mat {
        n,
        dim,
        v: all.v[..n * dim].to_vec(),
    };
    let truth: Vec<Vec<u32>> = (0..nq)
        .map(|i| exact_topk(queries.row(i), &data, 10))
        .collect();
    let cap = (1.25 * n as f64 / k_cent as f64).ceil() as usize;
    let t0 = std::time::Instant::now();
    let (cents, _) = balanced_kmeans(&data, k_cent, cap, 6, &mut rng);
    println!(
        "balanced k-means: {:.1}s (cap {cap} per list)",
        t0.elapsed().as_secs_f64()
    );
    let (packed, _) = Spann::build(&data, &cents, k_cent, REPL_EPS, 8, Layout::Packed);
    let (aligned, _) = Spann::build(&data, &cents, k_cent, REPL_EPS, 8, Layout::Aligned);
    let sizes: Vec<usize> = aligned.dir.iter().map(|d| d.1).collect();
    println!(
        "posting lists: min {} / mean {:.0} / max {} entries; replication x{:.2}; packed file {:.1} KiB, aligned file {:.1} KiB (+{:.1}% padding)",
        sizes.iter().min().unwrap(),
        sizes.iter().sum::<usize>() as f64 / k_cent as f64,
        sizes.iter().max().unwrap(),
        aligned.replicas_total as f64 / n as f64,
        packed.dev.size_bytes() as f64 / 1024.0,
        aligned.dev.size_bytes() as f64 / 1024.0,
        100.0 * (aligned.dev.size_bytes() as f64 / packed.dev.size_bytes() as f64 - 1.0)
    );

    // (1) exactness when nothing is pruned and everything is probed
    for i in 0..30 {
        let (got, st) = aligned.search(queries.row(i), 10, k_cent, 1e9);
        assert_eq!(st.postings, k_cent);
        assert_eq!(
            recall(&got, &truth[i]),
            1.0,
            "probing every list must be exact"
        );
    }
    // (2) device counters == layout model, per query, both layouts
    for sp in [&packed, &aligned] {
        for i in 0..nq {
            sp.dev.reset();
            sp.search(queries.row(i), 10, 6, 0.3);
            assert_eq!(
                sp.dev.blocks_read.get(),
                sp.model_probe_blocks(queries.row(i), 6, 0.3),
                "device block count disagrees with the layout model"
            );
        }
    }
    let (mut bp, mut ba) = (0, 0);
    for i in 0..nq {
        packed.dev.reset();
        aligned.dev.reset();
        packed.search(queries.row(i), 10, 8, 1e9);
        aligned.search(queries.row(i), 10, 8, 1e9);
        bp += packed.dev.blocks_read.get();
        ba += aligned.dev.blocks_read.get();
    }
    println!(
        "layout: blocks read for nprobe=8 over {nq} queries: packed {bp}, page-aligned {ba} ({:.1}% fewer)",
        100.0 * (1.0 - ba as f64 / bp as f64)
    );
    assert!(
        ba <= bp,
        "page-aligned lists can never need more blocks than packed ones"
    );

    // SPANN sweep
    let configs: [(usize, f32, &str); 9] = [
        (1, 1e9, "nprobe=1"),
        (2, 1e9, "nprobe=2"),
        (4, 1e9, "nprobe=4"),
        (8, 1e9, "nprobe=8"),
        (16, 1e9, "nprobe=16"),
        (16, 0.05, "nprobe<=16, eps_q=0.05"),
        (16, 0.10, "nprobe<=16, eps_q=0.10"),
        (16, 0.20, "nprobe<=16, eps_q=0.20"),
        (32, 0.30, "nprobe<=32, eps_q=0.30"),
    ];
    let spann_rows: Vec<Row> = configs
        .iter()
        .map(|&(np, e, l)| run_spann(&aligned, &queries, &truth, np, e, l))
        .collect();
    print_rows("SPANN-lite (aligned, replicated):", &spann_rows);

    // graph baseline
    let t0 = std::time::Instant::now();
    let g = Graph::build(&data, 12, 80);
    let dg = DiskGraph::build(&data, &g);
    println!(
        "\ngraph built in {:.1}s; {} records/block, file {:.1} KiB; in-RAM SQ8 copy {:.1} KiB (SPANN keeps only {} centroids = {:.1} KiB in RAM)",
        t0.elapsed().as_secs_f64(),
        dg.per_block,
        dg.dev.size_bytes() as f64 / 1024.0,
        dg.sq.len() as f64 / 1024.0,
        k_cent,
        (k_cent * dim * 4) as f64 / 1024.0
    );
    let mut graph_rows = vec![];
    for l in [10usize, 15, 20, 30, 50, 80] {
        dg.dev.reset();
        let (mut r, mut exp) = (0.0, 0usize);
        for i in 0..nq {
            let (got, e) = dg.search(queries.row(i), 10, l);
            r += recall(&got, &truth[i]);
            exp += e;
        }
        assert_eq!(
            dg.dev.blocks_read.get() as usize,
            exp,
            "one block per expansion (records never straddle blocks)"
        );
        let nqf = nq as f64;
        graph_rows.push(Row {
            label: format!("list size L={l}"),
            recall: r / nqf,
            blocks: exp as f64 / nqf,
            requests: dg.dev.requests.get() as f64 / nqf,
            lat: dg.dev.latency_us() / nqf,
        });
    }
    print_rows(
        "DiskANN-style graph (record per node, SQ8 in RAM):",
        &graph_rows,
    );
    // (6) summary at recall >= 0.95
    let best = |rows: &[Row]| {
        rows.iter()
            .filter(|r| r.recall >= 0.95)
            .min_by(|a, b| a.lat.total_cmp(&b.lat))
            .map(|r| (r.label.clone(), r.blocks, r.requests, r.lat))
    };
    let (s, d) = (best(&spann_rows), best(&graph_rows));
    println!("\ncheapest config with recall >= 0.95:\n  SPANN-lite : {s:?}\n  graph      : {d:?}");
    assert!(
        s.is_some() && d.is_some(),
        "both indexes must be able to reach recall 0.95"
    );
    println!("\nall SPANN-lite checks passed");
}
