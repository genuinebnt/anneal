//! RECIPE 140 (vectors): IVF-Flat: k-means partitions the vectors into inverted lists; search probes only the nearest few lists.
//! LEVEL: advanced
//! Use in: approximate nearest-neighbour search over millions of embeddings when a full scan is too slow.   Run: cargo run --release --example ivf_flat
#![allow(dead_code)]
//!
//! WHAT: train `nlist` centroids with k-means (k-means++ seeding + a fixed number of Lloyd iterations). Every vector goes into the
//! inverted list of its nearest centroid ("flat" = vectors are stored uncompressed). A query finds its `nprobe` nearest centroids
//! and scans only those lists, exactly.
//! WHY: neighbours of a query almost always live in the same or adjacent Voronoi cells, so scanning ~nprobe/nlist of the data finds
//! most true neighbours. Cost per query = nlist centroid distances + (nprobe/nlist)*N vector distances.
//! PARAMETERS: `nlist` ~ sqrt(N) balances the centroid scan against the list scan. `nprobe` is the recall/speed dial: 1 is fastest
//! but misses neighbours sitting just across a cell border; nprobe = nlist degenerates to exact brute force.
//! k-means++: the first centre is random, each next one is drawn with probability proportional to squared distance to the nearest
//! chosen centre, which spreads the seeds and avoids the bad local optima of uniform seeding. Empty clusters (possible with any
//! Lloyd variant) are repaired by stealing the point farthest from its centroid, so no list is wasted.
//! COMMON USES: FAISS IVF indexes (IndexIVFFlat, usually combined with product quantization as IVFPQ, see product_quantization); coarse
//! quantisers that shard a corpus so each shard or disk page holds one cell; pre-filtering step before exact re-ranking.
//! Read: https://github.com/facebookresearch/faiss and https://github.com/facebookresearch/faiss/wiki/Faster-search
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

// ---------- k-means ----------
/// k-means++ seeding: D^2 sampling.
fn kmeanspp(data: &[Vec<f32>], k: usize, rng: &mut Rng) -> Vec<Vec<f32>> {
    let mut centres = vec![data[rng.below(data.len())].clone()];
    let mut d2: Vec<f32> = data.iter().map(|v| l2(v, &centres[0])).collect();
    while centres.len() < k {
        let total: f32 = d2.iter().sum();
        let mut target = rng.unit() * total;
        let mut pick = data.len() - 1;
        for (i, d) in d2.iter().enumerate() {
            target -= d;
            if target <= 0.0 {
                pick = i;
                break;
            }
        }
        centres.push(data[pick].clone());
        for (i, v) in data.iter().enumerate() {
            d2[i] = d2[i].min(l2(v, centres.last().unwrap()));
        }
    }
    centres
}

fn nearest(v: &[f32], centres: &[Vec<f32>]) -> (usize, f32) {
    centres
        .iter()
        .enumerate()
        .map(|(i, c)| (i, l2(v, c)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap()
}

/// Lloyd iterations: assign, then move each centre to the mean of its members.
fn kmeans(data: &[Vec<f32>], k: usize, iters: usize, rng: &mut Rng) -> Vec<Vec<f32>> {
    let dim = data[0].len();
    let mut centres = kmeanspp(data, k, rng);
    for _ in 0..iters {
        let assigned: Vec<(usize, f32)> = data.iter().map(|v| nearest(v, &centres)).collect();
        let mut sums = vec![vec![0.0f32; dim]; k];
        let mut counts = vec![0usize; k];
        for (v, &(c, _)) in data.iter().zip(&assigned) {
            counts[c] += 1;
            sums[c].iter_mut().zip(v).for_each(|(s, x)| *s += x);
        }
        for c in 0..k {
            if counts[c] == 0 {
                // Empty cluster: re-seed on the worst-served point (the one its centroid represents worst).
                let far = (0..data.len())
                    .max_by(|&a, &b| assigned[a].1.total_cmp(&assigned[b].1))
                    .unwrap();
                centres[c] = data[far].clone();
            } else {
                centres[c] = sums[c].iter().map(|s| s / counts[c] as f32).collect();
            }
        }
    }
    centres
}

// ---------- the index ----------
struct IvfFlat {
    centroids: Vec<Vec<f32>>,
    lists: Vec<Vec<usize>>, // inverted lists of vector ids
}

impl IvfFlat {
    fn build(data: &[Vec<f32>], nlist: usize, iters: usize, rng: &mut Rng) -> Self {
        let centroids = kmeans(data, nlist, iters, rng);
        let mut lists = vec![Vec::new(); nlist];
        for (id, v) in data.iter().enumerate() {
            lists[nearest(v, &centroids).0].push(id);
        }
        IvfFlat { centroids, lists }
    }

    /// Returns (top-k ids, number of vectors scanned).
    fn search(&self, q: &[f32], data: &[Vec<f32>], k: usize, nprobe: usize) -> (Vec<usize>, usize) {
        let mut order: Vec<(f32, usize)> = self
            .centroids
            .iter()
            .enumerate()
            .map(|(i, c)| (l2(q, c), i))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cand: Vec<(f32, usize)> = Vec::new();
        for &(_, list) in order.iter().take(nprobe) {
            cand.extend(self.lists[list].iter().map(|&id| (l2(q, &data[id]), id)));
        }
        let scanned = cand.len();
        cand.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        (
            cand.into_iter().take(k).map(|(_, id)| id).collect(),
            scanned,
        )
    }
}

fn main() {
    let (n, dim, nq, k, nlist) = (8_000, 32, 200, 10, 64);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut all = make_data(n + nq, dim, 20, &mut rng);
    let queries = all.split_off(n);
    let data = all;
    assert!(cosine_dist(&data[0], &data[0]).abs() < 1e-4);

    let truth: Vec<Vec<usize>> = queries.iter().map(|q| exact_topk(q, &data, k)).collect();
    let index = IvfFlat::build(&data, nlist, 15, &mut rng);
    let sizes: Vec<usize> = index.lists.iter().map(Vec::len).collect();
    assert_eq!(
        sizes.iter().sum::<usize>(),
        n,
        "every vector lives in exactly one list"
    );
    assert!(sizes.iter().all(|&s| s > 0), "no empty lists after repair");
    println!(
        "nlist={nlist} list sizes: min={} max={} (ideal {})",
        sizes.iter().min().unwrap(),
        sizes.iter().max().unwrap(),
        n / nlist
    );

    println!(
        "{:>7} {:>10} {:>12} {:>10}",
        "nprobe", "recall@10", "scanned %", "queries/s"
    );
    let mut rows: Vec<(usize, f64, f64)> = Vec::new();
    for nprobe in [1, 2, 4, 8, 16, 32, 64] {
        let t = std::time::Instant::now();
        let (mut rec, mut scanned) = (0.0, 0usize);
        for (q, tr) in queries.iter().zip(&truth) {
            let (ids, s) = index.search(q, &data, k, nprobe);
            rec += recall(&ids, tr);
            scanned += s;
        }
        let rec = rec / nq as f64;
        let frac = scanned as f64 / (nq * n) as f64;
        println!(
            "{nprobe:>7} {rec:>10.3} {:>11.1}% {:>10.0}",
            frac * 100.0,
            nq as f64 / t.elapsed().as_secs_f64()
        );
        rows.push((nprobe, rec, frac));
    }
    // More probing never hurts recall: lists scanned at nprobe are a subset of those at 2*nprobe.
    assert!(
        rows.windows(2).all(|w| w[1].1 + 1e-9 >= w[0].1),
        "recall must be monotone in nprobe"
    );
    let at16 = rows.iter().find(|r| r.0 == 16).unwrap();
    assert!(at16.1 >= 0.9, "recall@10 at nprobe=16 was {}", at16.1);
    assert!(
        at16.2 < 0.5,
        "nprobe=16 should scan well under half the data, scanned {}",
        at16.2
    );
    assert!(
        rows.last().unwrap().1 > 0.999,
        "nprobe=nlist is exact search"
    );
    assert!(rows[0].2 < 0.1, "nprobe=1 scans a small fraction");
}
