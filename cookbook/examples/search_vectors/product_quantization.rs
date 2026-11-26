//! RECIPE 143 (vectors): Product Quantization: compress vectors to m bytes and search with Asymmetric Distance Computation (ADC).
//! LEVEL: advanced
//! Use in: fitting huge vector collections in RAM (16-64x smaller) while keeping approximate search fast.   Run: cargo run --release --example product_quantization
#![allow(dead_code)]
//!
//! WHAT: split each D-dim vector into `m` sub-vectors of D/m dims. For each subspace run k-means with 256 centroids (a CODEBOOK).
//! A vector is stored as m bytes: the index of the nearest centroid in each subspace. 256 = 2^8 so one centroid id is exactly a u8.
//! The implicit vocabulary is 256^m reconstructed points, far finer than one k-means with 256 centres could give, while the codebooks
//! stay tiny (m * 256 * D/m floats).
//! ADC: for a query, compute once a lookup table T[j][c] = ||q_j - centroid_j[c]||^2 (m * 256 distances). The distance to any
//! stored vector is then sum_j T[j][code[j]]: m table lookups and adds, no float vectors touched. "Asymmetric" because the query stays
//! UNquantised (only the database side is lossy), which is more accurate than comparing two codes.
//! PARAMETERS: `m` = bytes per vector; bigger m means finer sub-vectors, lower error, less compression. Typical: 8 to 64.
//! RE-RANKING: PQ distances are noisy, so take the top R (here 100) by ADC and recompute EXACT distances for just those R; that
//! recovers most of the lost recall for R full-vector reads. The raw vectors may live on slow storage since only R are touched.
//! COMMON USES: FAISS IVFPQ and PQ indexes; the in-RAM compressed sketch in DiskANN-style systems (see vamana_diskann_lite); vector databases
//! with a "product quantization" option to cut memory; coarse-then-fine pipelines (IVF to select, PQ to score, exact to re-rank).
//! Read: Jegou, Douze, Schmid, "Product quantization for nearest neighbor search" (IEEE TPAMI 2011) and https://github.com/facebookresearch/faiss
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

// ---------- k-means (as in ivf_flat, used here per subspace) ----------
fn nearest(v: &[f32], centres: &[Vec<f32>]) -> (usize, f32) {
    centres
        .iter()
        .enumerate()
        .map(|(i, c)| (i, l2(v, c)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap()
}

/// Seeds by sampling k DISTINCT points: simple and enough here (ivf_flat shows k-means++ seeding).
fn kmeans(data: &[Vec<f32>], k: usize, iters: usize, rng: &mut Rng) -> Vec<Vec<f32>> {
    let dim = data[0].len();
    let mut centres: Vec<Vec<f32>> = Vec::new();
    let mut idx: Vec<usize> = (0..data.len()).collect();
    for i in 0..k.min(data.len()) {
        idx.swap(i, i + rng.below(data.len() - i));
        centres.push(data[idx[i]].clone());
    }
    for _ in 0..iters {
        let assigned: Vec<(usize, f32)> = data.iter().map(|v| nearest(v, &centres)).collect();
        let mut sums = vec![vec![0.0f32; dim]; centres.len()];
        let mut counts = vec![0usize; centres.len()];
        for (v, &(c, _)) in data.iter().zip(&assigned) {
            counts[c] += 1;
            sums[c].iter_mut().zip(v).for_each(|(s, x)| *s += x);
        }
        for c in 0..centres.len() {
            if counts[c] > 0 {
                centres[c] = sums[c].iter().map(|s| s / counts[c] as f32).collect();
            } else {
                // Empty cluster: jump to the worst-served point.
                let far = (0..data.len())
                    .max_by(|&a, &b| assigned[a].1.total_cmp(&assigned[b].1))
                    .unwrap();
                centres[c] = data[far].clone();
            }
        }
    }
    centres
}

// ---------- product quantizer ----------
struct Pq {
    m: usize,
    sub: usize,                // dims per sub-vector
    books: Vec<Vec<Vec<f32>>>, // books[j][c] = centroid c of subspace j
}

impl Pq {
    fn train(data: &[Vec<f32>], m: usize, iters: usize, rng: &mut Rng) -> Self {
        let sub = data[0].len() / m;
        assert_eq!(sub * m, data[0].len(), "dim must be divisible by m");
        let books = (0..m)
            .map(|j| {
                let part: Vec<Vec<f32>> = data
                    .iter()
                    .map(|v| v[j * sub..(j + 1) * sub].to_vec())
                    .collect();
                kmeans(&part, 256, iters, rng)
            })
            .collect();
        Pq { m, sub, books }
    }

    /// m bytes per vector.
    fn encode(&self, v: &[f32]) -> Vec<u8> {
        (0..self.m)
            .map(|j| nearest(&v[j * self.sub..(j + 1) * self.sub], &self.books[j]).0 as u8)
            .collect()
    }

    fn decode(&self, code: &[u8]) -> Vec<f32> {
        code.iter()
            .enumerate()
            .flat_map(|(j, &c)| self.books[j][c as usize].iter().copied())
            .collect()
    }

    /// Per-query lookup tables: m x 256 sub-distances, computed once per query and reused for every database vector.
    fn tables(&self, q: &[f32]) -> Vec<[f32; 256]> {
        (0..self.m)
            .map(|j| {
                let qs = &q[j * self.sub..(j + 1) * self.sub];
                let mut t = [0.0f32; 256];
                for (c, cent) in self.books[j].iter().enumerate() {
                    t[c] = l2(qs, cent);
                }
                t
            })
            .collect()
    }

    /// ADC distance: m lookups and adds.
    fn adc(tables: &[[f32; 256]], code: &[u8]) -> f32 {
        code.iter().zip(tables).map(|(&c, t)| t[c as usize]).sum()
    }
}

/// Top-`r` ids by ADC distance over all codes.
fn adc_topk(pq: &Pq, codes: &[Vec<u8>], q: &[f32], r: usize) -> Vec<usize> {
    let tables = pq.tables(q);
    let mut scored: Vec<(f32, usize)> = codes
        .iter()
        .enumerate()
        .map(|(i, c)| (Pq::adc(&tables, c), i))
        .collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(r).map(|s| s.1).collect()
}

/// Exact re-ranking of a shortlist: the only place full vectors are read.
fn rerank(q: &[f32], data: &[Vec<f32>], shortlist: &[usize], k: usize) -> Vec<usize> {
    let mut s: Vec<(f32, usize)> = shortlist.iter().map(|&i| (l2(q, &data[i]), i)).collect();
    s.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    s.into_iter().take(k).map(|x| x.1).collect()
}

fn mean_recall(
    f: impl Fn(&[f32]) -> Vec<usize>,
    queries: &[Vec<f32>],
    truth: &[Vec<usize>],
) -> f64 {
    queries
        .iter()
        .zip(truth)
        .map(|(q, t)| recall(&f(q), t))
        .sum::<f64>()
        / queries.len() as f64
}

fn main() {
    let (n, dim, nq, k) = (8_000, 32, 200, 10);
    let mut rng = Rng(0xE703_7ED1_A0B4_28DB);
    let mut all = make_data(n + nq, dim, 20, &mut rng);
    let queries = all.split_off(n);
    let data = all;
    assert!(cosine_dist(&data[0], &data[1]) > 0.0);
    let truth: Vec<Vec<usize>> = queries.iter().map(|q| exact_topk(q, &data, k)).collect();

    println!(
        "{:>3} {:>7} {:>11} {:>10} {:>12} {:>12}",
        "m", "bytes", "compression", "mse", "PQ recall@10", "+rerank 100"
    );
    let mut rows: Vec<(usize, f64, f64, f64)> = Vec::new();
    for m in [4, 8, 16] {
        let pq = Pq::train(&data, m, 10, &mut rng);
        let codes: Vec<Vec<u8>> = data.iter().map(|v| pq.encode(v)).collect();
        let mse = data
            .iter()
            .zip(&codes)
            .map(|(v, c)| l2(v, &pq.decode(c)) as f64)
            .sum::<f64>()
            / n as f64;
        let ratio = (dim * 4) as f64 / m as f64; // f32 vector bytes / code bytes
        let plain = mean_recall(|q| adc_topk(&pq, &codes, q, k), &queries, &truth);
        let rr = mean_recall(
            |q| rerank(q, &data, &adc_topk(&pq, &codes, q, 100), k),
            &queries,
            &truth,
        );
        println!("{m:>3} {m:>7} {ratio:>10.0}x {mse:>10.3} {plain:>12.3} {rr:>12.3}");
        rows.push((m, mse, plain, rr));
        // ADC sanity: the table sum equals the distance to the DECODED vector (that is exactly what it approximates).
        let t = pq.tables(&queries[0]);
        let direct = l2(&queries[0], &pq.decode(&codes[0]));
        assert!(
            (Pq::adc(&t, &codes[0]) - direct).abs() < 1e-2 * direct.max(1.0),
            "ADC == distance to reconstruction"
        );
    }
    assert!(
        rows.windows(2).all(|w| w[1].1 < w[0].1),
        "more sub-quantizers must reduce reconstruction error"
    );
    assert!(
        rows.windows(2).all(|w| w[1].2 + 0.01 >= w[0].2),
        "more bytes must not meaningfully lower PQ recall"
    );
    for r in &rows {
        assert!(
            r.3 >= r.2,
            "re-ranking never hurts: m={} {} vs {}",
            r.0,
            r.3,
            r.2
        );
    }
    let m8 = rows.iter().find(|r| r.0 == 8).unwrap();
    assert!(m8.3 >= 0.9, "PQ(m=8) + rerank-100 recall@10 was {}", m8.3);
    assert!(
        m8.2 < m8.3,
        "re-ranking must recover recall that PQ alone lost"
    );

    // Shortlist size is the effort dial for the re-rank stage.
    let pq = Pq::train(&data, 8, 10, &mut rng);
    let codes: Vec<Vec<u8>> = data.iter().map(|v| pq.encode(v)).collect();
    println!("\nm=8, re-rank shortlist size:");
    let mut prev = 0.0;
    for r in [10, 25, 50, 100, 200] {
        let rec = mean_recall(
            |q| rerank(q, &data, &adc_topk(&pq, &codes, q, r), k),
            &queries,
            &truth,
        );
        println!("  R={r:>3}  recall@10={rec:.3}");
        assert!(rec + 0.01 >= prev, "bigger shortlist must not hurt");
        prev = rec;
    }
}
