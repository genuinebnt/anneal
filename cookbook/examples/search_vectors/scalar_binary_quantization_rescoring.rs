//! RECIPE 871 (vectors): scalar, binary and RaBitQ-style quantisation with oversample-and-rescore, compared with product quantisation.
//! LEVEL: intermediate
//! Use in: cutting vector memory 4x-32x (or more) while keeping recall, by searching compressed codes and re-ranking a few candidates exactly.   Run: cargo run --release --example scalar_binary_quantization_rescoring
#![allow(dead_code)]
//!
//! WHAT: six compressed representations of the same vectors, all searched the same way ("compressed scan, then exact rerank"):
//!   * sq8 (per-dimension min/max): one byte per dimension, step_d = (max_d - min_d)/255. Error per coordinate <= step_d/2.
//!   * sq8 per-vector scale (symmetric i8): x ~ scale * q with scale = max|x|/127, one f32 per vector extra. Error <= scale/2.
//!   * binary (1 bit per dimension): bit = [x_d > mean_d]. Distance = Hamming (XOR + popcount). 32x smaller than f32.
//!   * thermometer (2 bits/dim -> 3 bits): level L in 0..=3 from per-dimension QUARTILES, stored as 3 bits (L>=1, L>=2, L>=3). The
//!     Hamming distance between two thermometer codes is EXACTLY |L1 - L2|, so popcount computes an L1 distance on 4 levels.
//!   * rabitq-style: centre on the data mean, normalise to unit length, rotate by a random orthogonal matrix P, keep only the SIGNS.
//!     The code is a unit vector o_bar = sign/sqrt(D); the unbiased inner-product estimator is <o_bar, q'> / <o_bar, o> (both sides
//!     rotated). Because P is random, <o_bar,o> concentrates near sqrt(2/pi) ~ 0.8 and the estimator's error is O(1/sqrt(D)) with
//!     high probability (RaBitQ proves this bound; we only MEASURE it: mean ~ 0 and sd close to sqrt(1-x^2)/(x sqrt(D-1))).
//!     The query stays float (ASYMMETRIC): a 1-bit code on one side only is far more accurate than 1 bit on both.
//!   * pq (m=8 sub-quantisers x 256 centroids, ADC lookup tables): the classic learned codebook; 8 bytes per vector here.
//! RESCORING: take the top k*oversample candidates by compressed distance, recompute exact distances from the full-precision
//! vectors (kept on slow storage in real systems), return the top k. Recall@10 vs oversample is the knob that trades a few exact
//! reads for the 4x-32x memory saving. Candidate sets are NESTED (prefixes of one sorted list), so recall per query can only go up.
//! DATA: gaussian clusters, uniform noise (hard: no structure to exploit), and an anisotropic "real-like" set whose points live
//! near an 8-dimensional subspace of 64 dims with decaying axis scales (embeddings look like this).
//!
//! VERIFIED: (1) every sq8/i8 round trip error <= half a step, and the ADC distance error obeys the triangle-inequality bound
//! |d(q,x_hat) - d(q,x)| <= |x - x_hat|; (2) Hamming/popcount and thermometer codes equal bit-level oracles; (3) rescoring with
//! k*oversample >= n gives recall exactly 1.0 for every method; (4) the reranked result equals exact top-k RESTRICTED to the
//! candidate set; (5) recall is monotone non-decreasing in oversample for every query and method; (6) RaBitQ estimator bias and sd.
//! COMMON USES: Qdrant scalar/binary quantisation with rescore, Weaviate BQ/SQ/PQ, pgvector halfvec/bit types, Lucene int7/int4/bit
//! vector formats, Elasticsearch "bbq" (better binary quantisation, a RaBitQ-inspired scheme), Faiss `IndexScalarQuantizer`, `IndexIVFPQ`.
//! Read: RaBitQ, Gao and Long, https://arxiv.org/abs/2405.12497 ; Product Quantization for Nearest Neighbor Search, Jegou, Douze and Schmid,
//! IEEE TPAMI 2011 (cited by title; the ieeexplore URL returned 202 so it is not confirmed) ; https://github.com/facebookresearch/faiss/wiki/Faiss-indexes ;
//! https://arxiv.org/abs/1603.09320 (HNSW) for the graph indexes that these codes are usually attached to.
//!
//! TRY NEXT:
//!   - [S] Predict, then measure: in `Binary1::build` threshold at 0 instead of the per-dimension mean. Which dataset loses the most recall at oversample 8, and why does the anisotropic set suffer?
//!   - [S] Change `Thermo` to 3 bits per dimension (7 thermometer bits, thresholds at octiles). Does the recall gain per extra bit shrink? Plot recall@10 at oversample 4 against bits per vector.
//!   - [M] Replace the random rotation `random_orthogonal` in `RaBitq` with the identity and re-run the estimator statistics. Which invariant of `check_rabitq_estimator` breaks first, and on which dataset?
//!   - [M] Real-world port: store the sq8 codes and the f32 vectors in two separate `Vec`s and count bytes touched per query in `rescored_search` (compressed scan = n*bytes, rerank = k*oversample*4*dim). Where is the break-even oversample versus a plain f32 scan?
//!   - [A] PROJECT: attach `Sq8Global` codes to the graph in `hnsw_lite` (search the graph with ADC distances, rescore the final `ef` list exactly) and compare recall/bytes with `product_quantization`; report which codec wins on each dataset.
use std::time::Instant;

// ---------- shared mini-harness (copied so the file stands alone) ----------
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
/// Row-major matrix of n vectors.
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
fn clusters(n: usize, dim: usize, k: usize, rng: &mut Rng) -> Mat {
    let centres: Vec<f32> = (0..k * dim).map(|_| rng.gauss() * 2.0).collect();
    let mut v = Vec::with_capacity(n * dim);
    for _ in 0..n {
        let c = rng.below(k);
        v.extend((0..dim).map(|d| centres[c * dim + d] + rng.gauss() * 0.8));
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
/// Low intrinsic dimension: x = W z + small noise, z in R^latent, with per-axis decay like a PCA spectrum.
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
fn sorted_by_dist(mut v: Vec<(f32, usize)>) -> Vec<(f32, usize)> {
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v
}
fn exact_topk(q: &[f32], data: &Mat, k: usize) -> Vec<usize> {
    let all = (0..data.n).map(|i| (l2(q, data.row(i)), i)).collect();
    sorted_by_dist(all)
        .into_iter()
        .take(k)
        .map(|p| p.1)
        .collect()
}
fn recall(approx: &[usize], exact: &[usize]) -> f64 {
    approx.iter().filter(|a| exact.contains(a)).count() as f64 / exact.len() as f64
}

// ---------- codecs ----------
trait Codec {
    fn name(&self) -> &'static str;
    fn bytes_per_vec(&self) -> usize;
    /// Approximate (squared-L2-like, smaller = closer) score of the query against every stored vector.
    fn scores(&self, q: &[f32]) -> Vec<f32>;
}

// --- sq8: per-dimension min/max, u8 codes
struct Sq8Global {
    dim: usize,
    mins: Vec<f32>,
    steps: Vec<f32>,
    codes: Vec<u8>,
}
impl Sq8Global {
    fn build(m: &Mat) -> Self {
        let mut mins = vec![f32::INFINITY; m.dim];
        let mut maxs = vec![f32::NEG_INFINITY; m.dim];
        for i in 0..m.n {
            for (d, x) in m.row(i).iter().enumerate() {
                mins[d] = mins[d].min(*x);
                maxs[d] = maxs[d].max(*x);
            }
        }
        let steps: Vec<f32> = (0..m.dim)
            .map(|d| {
                if maxs[d] > mins[d] {
                    (maxs[d] - mins[d]) / 255.0
                } else {
                    1.0
                }
            })
            .collect();
        let mut s = Sq8Global {
            dim: m.dim,
            mins,
            steps,
            codes: Vec::with_capacity(m.n * m.dim),
        };
        for i in 0..m.n {
            let enc = s.encode(m.row(i));
            s.codes.extend(enc);
        }
        s
    }
    fn encode(&self, x: &[f32]) -> Vec<u8> {
        (0..self.dim)
            .map(|d| {
                ((x[d] - self.mins[d]) / self.steps[d])
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect()
    }
    fn decode(&self, code: &[u8]) -> Vec<f32> {
        (0..self.dim)
            .map(|d| self.mins[d] + code[d] as f32 * self.steps[d])
            .collect()
    }
    /// Worst-case |x - x_hat| (L2) for any in-range vector: the step/2 box diagonal.
    fn max_l2_error(&self) -> f32 {
        self.steps
            .iter()
            .map(|s| (s / 2.0) * (s / 2.0))
            .sum::<f32>()
            .sqrt()
    }
}
impl Codec for Sq8Global {
    fn name(&self) -> &'static str {
        "sq8"
    }
    fn bytes_per_vec(&self) -> usize {
        self.dim
    }
    fn scores(&self, q: &[f32]) -> Vec<f32> {
        // ADC: the query stays float, only the database side is dequantised on the fly.
        self.codes
            .chunks_exact(self.dim)
            .map(|c| {
                (0..self.dim)
                    .map(|d| (q[d] - (self.mins[d] + c[d] as f32 * self.steps[d])).powi(2))
                    .sum()
            })
            .collect()
    }
}

// --- i8 with a per-vector scale
struct SqPerVec {
    dim: usize,
    scales: Vec<f32>,
    codes: Vec<i8>,
}
impl SqPerVec {
    fn build(m: &Mat) -> Self {
        let mut s = SqPerVec {
            dim: m.dim,
            scales: vec![],
            codes: vec![],
        };
        for i in 0..m.n {
            let x = m.row(i);
            let mx = x.iter().fold(0.0f32, |a, v| a.max(v.abs()));
            let scale = if mx == 0.0 { 1.0 } else { mx / 127.0 };
            s.scales.push(scale);
            s.codes.extend(
                x.iter()
                    .map(|v| (v / scale).round().clamp(-127.0, 127.0) as i8),
            );
        }
        s
    }
}
impl Codec for SqPerVec {
    fn name(&self) -> &'static str {
        "i8/vec"
    }
    fn bytes_per_vec(&self) -> usize {
        self.dim + 4
    }
    fn scores(&self, q: &[f32]) -> Vec<f32> {
        self.codes
            .chunks_exact(self.dim)
            .zip(&self.scales)
            .map(|(c, s)| {
                c.iter()
                    .zip(q)
                    .map(|(c, q)| (q - *c as f32 * s).powi(2))
                    .sum()
            })
            .collect()
    }
}

// --- bit packing + Hamming
fn pack_bits(bits: impl Iterator<Item = bool>, words: usize) -> Vec<u64> {
    let mut w = vec![0u64; words];
    for (i, b) in bits.enumerate() {
        if b {
            w[i / 64] |= 1u64 << (i % 64);
        }
    }
    w
}
fn hamming(a: &[u64], b: &[u64]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

// --- binary: 1 bit per dim around the per-dimension mean
struct Binary1 {
    words: usize,
    means: Vec<f32>,
    codes: Vec<u64>,
}
impl Binary1 {
    fn encode(means: &[f32], x: &[f32], words: usize) -> Vec<u64> {
        pack_bits(x.iter().zip(means).map(|(x, m)| x > m), words)
    }
    fn build(m: &Mat) -> Self {
        let mut means = vec![0.0f32; m.dim];
        for i in 0..m.n {
            for (d, x) in m.row(i).iter().enumerate() {
                means[d] += x / m.n as f32;
            }
        }
        let words = m.dim.div_ceil(64);
        let codes = (0..m.n)
            .flat_map(|i| Self::encode(&means, m.row(i), words))
            .collect();
        Binary1 {
            words,
            means,
            codes,
        }
    }
}
impl Codec for Binary1 {
    fn name(&self) -> &'static str {
        "binary"
    }
    fn bytes_per_vec(&self) -> usize {
        self.words * 8
    }
    fn scores(&self, q: &[f32]) -> Vec<f32> {
        let qc = Self::encode(&self.means, q, self.words);
        self.codes
            .chunks_exact(self.words)
            .map(|c| hamming(&qc, c) as f32)
            .collect()
    }
}

// --- thermometer: 4 levels from per-dim quartiles -> 3 bits per dim; Hamming == |L1 - L2|
struct Thermo {
    dim: usize,
    words: usize,
    cuts: Vec<[f32; 3]>,
    codes: Vec<u64>,
}
impl Thermo {
    fn level(cuts: &[f32; 3], x: f32) -> usize {
        cuts.iter().filter(|c| x > **c).count()
    }
    fn encode(cuts: &[[f32; 3]], x: &[f32], words: usize) -> Vec<u64> {
        let bits = x.iter().enumerate().flat_map(|(d, v)| {
            let l = Self::level(&cuts[d], *v);
            (1..=3).map(move |t| l >= t)
        });
        pack_bits(bits, words)
    }
    fn build(m: &Mat) -> Self {
        let cuts: Vec<[f32; 3]> = (0..m.dim)
            .map(|d| {
                let mut col: Vec<f32> = (0..m.n).map(|i| m.row(i)[d]).collect();
                col.sort_by(|a, b| a.total_cmp(b));
                [col[m.n / 4], col[m.n / 2], col[3 * m.n / 4]]
            })
            .collect();
        let words = (m.dim * 3).div_ceil(64);
        let codes = (0..m.n)
            .flat_map(|i| Self::encode(&cuts, m.row(i), words))
            .collect();
        Thermo {
            dim: m.dim,
            words,
            cuts,
            codes,
        }
    }
}
impl Codec for Thermo {
    fn name(&self) -> &'static str {
        "thermo2b"
    }
    fn bytes_per_vec(&self) -> usize {
        self.words * 8
    }
    fn scores(&self, q: &[f32]) -> Vec<f32> {
        let qc = Self::encode(&self.cuts, q, self.words);
        self.codes
            .chunks_exact(self.words)
            .map(|c| hamming(&qc, c) as f32)
            .collect()
    }
}

// --- RaBitQ-style: centre, normalise, random rotation, 1 bit (sign) per dim, unbiased IP estimator
fn random_orthogonal(dim: usize, rng: &mut Rng) -> Vec<Vec<f32>> {
    // Gram-Schmidt on gaussian rows -> uniformly random orthonormal basis (rows). Done in f64 for stability.
    let mut rows: Vec<Vec<f64>> = Vec::new();
    while rows.len() < dim {
        let mut v: Vec<f64> = (0..dim).map(|_| rng.gauss() as f64).collect();
        for r in &rows {
            let d: f64 = v.iter().zip(r).map(|(a, b)| a * b).sum();
            for (a, b) in v.iter_mut().zip(r) {
                *a -= d * b;
            }
        }
        let n = v.iter().map(|a| a * a).sum::<f64>().sqrt();
        if n > 1e-6 {
            rows.push(v.iter().map(|a| a / n).collect());
        }
    }
    rows.into_iter()
        .map(|r| r.into_iter().map(|a| a as f32).collect())
        .collect()
}
struct RaBitq {
    dim: usize,
    words: usize,
    centre: Vec<f32>,
    p: Vec<Vec<f32>>,
    codes: Vec<u64>,
    norms: Vec<f32>,  // |x - c|
    obar_o: Vec<f32>, // <o_bar, o>, the correction that makes the estimator unbiased
}
impl RaBitq {
    /// y = P * ((x - c) / |x - c|); returns (y, |x - c|).
    fn rotate(&self, x: &[f32]) -> (Vec<f32>, f32) {
        let diff: Vec<f32> = x.iter().zip(&self.centre).map(|(a, b)| a - b).collect();
        let r = diff.iter().map(|a| a * a).sum::<f32>().sqrt();
        let inv = if r > 0.0 { 1.0 / r } else { 0.0 };
        (
            self.p
                .iter()
                .map(|row| row.iter().zip(&diff).map(|(a, b)| a * b * inv).sum())
                .collect(),
            r,
        )
    }
    fn build(m: &Mat, rng: &mut Rng) -> Self {
        let mut centre = vec![0.0f32; m.dim];
        for i in 0..m.n {
            for (d, x) in m.row(i).iter().enumerate() {
                centre[d] += x / m.n as f32;
            }
        }
        let words = m.dim.div_ceil(64);
        let mut s = RaBitq {
            dim: m.dim,
            words,
            centre,
            p: random_orthogonal(m.dim, rng),
            codes: vec![],
            norms: vec![],
            obar_o: vec![],
        };
        let sq = (m.dim as f32).sqrt();
        for i in 0..m.n {
            let (y, r) = s.rotate(m.row(i));
            s.codes.extend(pack_bits(y.iter().map(|v| *v > 0.0), words));
            s.norms.push(r);
            s.obar_o.push(y.iter().map(|v| v.abs()).sum::<f32>() / sq); // <sign/sqrt(D), y> = sum|y|/sqrt(D)
        }
        s
    }
    /// Estimate <o, o_q> for stored vector i given the rotated, normalised query y_q.
    fn est_ip(&self, i: usize, yq: &[f32]) -> f32 {
        let code = &self.codes[i * self.words..(i + 1) * self.words];
        let mut s = 0.0f32;
        for (d, y) in yq.iter().enumerate() {
            let bit = (code[d / 64] >> (d % 64)) & 1;
            s += if bit == 1 { *y } else { -*y };
        }
        let ob = self.obar_o[i].max(1e-6);
        s / (self.dim as f32).sqrt() / ob
    }
}
impl Codec for RaBitq {
    fn name(&self) -> &'static str {
        "rabitq"
    }
    fn bytes_per_vec(&self) -> usize {
        self.words * 8 + 8
    }
    fn scores(&self, q: &[f32]) -> Vec<f32> {
        let (yq, rq) = self.rotate(q);
        (0..self.norms.len())
            .map(|i| {
                let r = self.norms[i];
                r * r + rq * rq - 2.0 * r * rq * self.est_ip(i, &yq)
            })
            .collect()
    }
}

// --- product quantisation (small k-means, ADC tables)
struct Pq {
    m: usize,
    sub: usize,
    books: Vec<Vec<f32>>, // [m][256 * sub]
    codes: Vec<u8>,
}
fn kmeans(points: &[Vec<f32>], k: usize, iters: usize, rng: &mut Rng) -> Vec<Vec<f32>> {
    let mut cents: Vec<Vec<f32>> = (0..k)
        .map(|_| points[rng.below(points.len())].clone())
        .collect();
    for _ in 0..iters {
        let mut sums = vec![vec![0.0f32; points[0].len()]; k];
        let mut cnt = vec![0usize; k];
        for p in points {
            let c = (0..k)
                .min_by(|&a, &b| l2(p, &cents[a]).total_cmp(&l2(p, &cents[b])))
                .unwrap();
            cnt[c] += 1;
            for (s, x) in sums[c].iter_mut().zip(p) {
                *s += x;
            }
        }
        for c in 0..k {
            if cnt[c] > 0 {
                cents[c] = sums[c].iter().map(|s| s / cnt[c] as f32).collect();
            } else {
                cents[c] = points[rng.below(points.len())].clone(); // re-seed empty cells
            }
        }
    }
    cents
}
impl Pq {
    fn build(mat: &Mat, m: usize, rng: &mut Rng) -> Self {
        let sub = mat.dim / m;
        let mut books = Vec::new();
        let mut codes = vec![0u8; mat.n * m];
        for j in 0..m {
            let pts: Vec<Vec<f32>> = (0..mat.n)
                .map(|i| mat.row(i)[j * sub..(j + 1) * sub].to_vec())
                .collect();
            let cents = kmeans(&pts, 256, 6, rng);
            for (i, p) in pts.iter().enumerate() {
                codes[i * m + j] = (0..256)
                    .min_by(|&a, &b| l2(p, &cents[a]).total_cmp(&l2(p, &cents[b])))
                    .unwrap() as u8;
            }
            books.push(cents.concat());
        }
        Pq {
            m,
            sub,
            books,
            codes,
        }
    }
}
impl Codec for Pq {
    fn name(&self) -> &'static str {
        "pq8x8"
    }
    fn bytes_per_vec(&self) -> usize {
        self.m
    }
    fn scores(&self, q: &[f32]) -> Vec<f32> {
        // one lookup table per sub-space: distance from the query chunk to each of the 256 centroids
        let lut: Vec<Vec<f32>> = (0..self.m)
            .map(|j| {
                (0..256)
                    .map(|c| {
                        l2(
                            &q[j * self.sub..(j + 1) * self.sub],
                            &self.books[j][c * self.sub..(c + 1) * self.sub],
                        )
                    })
                    .collect()
            })
            .collect();
        self.codes
            .chunks_exact(self.m)
            .map(|c| {
                c.iter()
                    .enumerate()
                    .map(|(j, &code)| lut[j][code as usize])
                    .sum()
            })
            .collect()
    }
}

// ---------- rescored search ----------
/// Candidate list from compressed scores: ids sorted by (score, id). Nested prefixes => monotone recall.
fn ranked_candidates(scores: &[f32]) -> Vec<usize> {
    sorted_by_dist(scores.iter().copied().zip(0..).collect())
        .into_iter()
        .map(|p| p.1)
        .collect()
}
fn rescore(q: &[f32], data: &Mat, ranked: &[usize], k: usize, oversample: usize) -> Vec<usize> {
    let cands = &ranked[..(k * oversample).min(ranked.len())];
    sorted_by_dist(cands.iter().map(|&i| (l2(q, data.row(i)), i)).collect())
        .into_iter()
        .take(k)
        .map(|p| p.1)
        .collect()
}

// ---------- checks ----------
fn check_codes(data: &Mat, rng: &mut Rng) {
    let sq = Sq8Global::build(data);
    let bound = sq.max_l2_error();
    let mut worst = 0.0f32;
    for i in 0..data.n {
        let x = data.row(i);
        let xh = sq.decode(&sq.encode(x));
        for d in 0..data.dim {
            assert!(
                (x[d] - xh[d]).abs() <= sq.steps[d] / 2.0 * 1.0001 + 1e-6,
                "sq8 coordinate error above step/2"
            );
        }
        worst = worst.max(l2(x, &xh).sqrt());
    }
    assert!(worst <= bound * 1.001 + 1e-5);
    let pv = SqPerVec::build(data);
    for i in 0..data.n {
        for d in 0..data.dim {
            let err = (data.row(i)[d] - pv.codes[i * data.dim + d] as f32 * pv.scales[i]).abs();
            assert!(
                err <= pv.scales[i] / 2.0 * 1.0001 + 1e-6,
                "i8/vec error above scale/2"
            );
        }
    }
    // ADC error bound by the triangle inequality: | |q - x_hat| - |q - x| | <= |x - x_hat|
    for _ in 0..500 {
        let (q, i) = (data.row(rng.below(data.n)), rng.below(data.n));
        let xh = sq.decode(&sq.encode(data.row(i)));
        let adc = sq.scores(q)[i].sqrt();
        let truth = l2(q, data.row(i)).sqrt();
        assert!(
            (adc - truth).abs() <= l2(data.row(i), &xh).sqrt() + 1e-3,
            "ADC violates the triangle bound"
        );
    }
    // bit-level oracles
    let b = Binary1::build(data);
    for _ in 0..300 {
        let (i, j) = (rng.below(data.n), rng.below(data.n));
        let want = (0..data.dim)
            .filter(|&d| (data.row(i)[d] > b.means[d]) != (data.row(j)[d] > b.means[d]))
            .count() as u32;
        let (ci, cj) = (
            &b.codes[i * b.words..(i + 1) * b.words],
            &b.codes[j * b.words..(j + 1) * b.words],
        );
        assert_eq!(hamming(ci, cj), want);
    }
    let t = Thermo::build(data);
    for _ in 0..300 {
        let (i, j) = (rng.below(data.n), rng.below(data.n));
        let want: i32 = (0..data.dim)
            .map(|d| {
                (Thermo::level(&t.cuts[d], data.row(i)[d]) as i32
                    - Thermo::level(&t.cuts[d], data.row(j)[d]) as i32)
                    .abs()
            })
            .sum();
        let (ci, cj) = (
            &t.codes[i * t.words..(i + 1) * t.words],
            &t.codes[j * t.words..(j + 1) * t.words],
        );
        assert_eq!(
            hamming(ci, cj) as i32,
            want,
            "thermometer Hamming must equal the L1 distance of levels"
        );
    }
}

fn check_rabitq_estimator(data: &Mat, rng: &mut Rng, label: &str) {
    let rb = RaBitq::build(data, rng);
    let mut errs = Vec::new();
    let mut predicted = 0.0f64;
    for _ in 0..2000 {
        let (i, j) = (rng.below(data.n), rng.below(data.n));
        let (yq, _) = rb.rotate(data.row(j));
        let (yi, _) = rb.rotate(data.row(i));
        let truth: f32 = yi.iter().zip(&yq).map(|(a, b)| a * b).sum(); // rotation preserves inner products
        errs.push((rb.est_ip(i, &yq) - truth) as f64);
        let x = rb.obar_o[i] as f64;
        predicted += (1.0 - x * x).max(0.0).sqrt() / (x * ((data.dim - 1) as f64).sqrt());
    }
    predicted /= 2000.0;
    let mean = errs.iter().sum::<f64>() / errs.len() as f64;
    let sd = (errs.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / errs.len() as f64).sqrt();
    let avg_x: f32 = rb.obar_o.iter().sum::<f32>() / data.n as f32;
    println!(
        "  rabitq estimator [{label}]: mean <o_bar,o> = {avg_x:.3} (sqrt(2/pi) = 0.798), bias {mean:+.4}, sd {sd:.4} vs formula {predicted:.4}"
    );
    assert!(mean.abs() < 0.02, "estimator should be (nearly) unbiased");
    assert!(
        sd < 2.0 * predicted + 0.02,
        "error should be O(1/sqrt(D)) as the formula says"
    );
}

fn evaluate(label: &str, data: &Mat, queries: &Mat, rng: &mut Rng) {
    let k = 10;
    let truth: Vec<Vec<usize>> = (0..queries.n)
        .map(|i| exact_topk(queries.row(i), data, k))
        .collect();
    check_codes(data, rng);
    let codecs: Vec<Box<dyn Codec>> = vec![
        Box::new(Sq8Global::build(data)),
        Box::new(SqPerVec::build(data)),
        Box::new(Binary1::build(data)),
        Box::new(Thermo::build(data)),
        Box::new(RaBitq::build(data, rng)),
        Box::new(Pq::build(data, 8, rng)),
    ];
    let overs = [1usize, 2, 4, 8, 16, 32, data.n / k];
    println!(
        "\n=== {label}: n={} dim={} (f32 = {} bytes/vector); recall@10 after rescoring ===",
        data.n,
        data.dim,
        data.dim * 4
    );
    println!(
        "{:<10} {:>6} {:>5} {}",
        "codec",
        "bytes",
        "x",
        overs
            .iter()
            .map(|o| format!(
                "{:>6}",
                if *o == data.n / k {
                    "all".to_string()
                } else {
                    format!("o={o}")
                }
            ))
            .collect::<String>()
    );
    for c in &codecs {
        let t = Instant::now();
        let ranked: Vec<Vec<usize>> = (0..queries.n)
            .map(|i| ranked_candidates(&c.scores(queries.row(i))))
            .collect();
        let scan_us = t.elapsed().as_secs_f64() * 1e6 / queries.n as f64;
        let mut means = vec![0.0f64; overs.len()];
        let mut prev = vec![0.0f64; queries.n];
        for (oi, &o) in overs.iter().enumerate() {
            for qi in 0..queries.n {
                let q = queries.row(qi);
                let got = rescore(q, data, &ranked[qi], k, o);
                let r = recall(&got, &truth[qi]);
                // (5) monotone per query: candidate sets are nested prefixes
                assert!(
                    r + 1e-12 >= prev[qi],
                    "{}: recall dropped with more oversample (query {qi})",
                    c.name()
                );
                prev[qi] = r;
                // (4) rescored result == exact top-k restricted to the candidate set
                if o == 4 && qi % 10 == 0 {
                    let cands = &ranked[qi][..(k * o).min(data.n)];
                    let restricted: Vec<usize> =
                        sorted_by_dist(cands.iter().map(|&i| (l2(q, data.row(i)), i)).collect())
                            .into_iter()
                            .take(k)
                            .map(|p| p.1)
                            .collect();
                    assert_eq!(got, restricted);
                }
                means[oi] += r / queries.n as f64;
            }
        }
        // (3) candidate set = everything => exact
        assert!(
            (means[overs.len() - 1] - 1.0).abs() < 1e-12,
            "{}: full candidate set must give recall 1.0",
            c.name()
        );
        println!(
            "{:<10} {:>6} {:>4}x {}   scan {:.0} us/q",
            c.name(),
            c.bytes_per_vec(),
            data.dim * 4 / c.bytes_per_vec(),
            means
                .iter()
                .map(|m| format!("{m:>6.3}"))
                .collect::<String>(),
            scan_us
        );
        if c.name() == "sq8" || c.name() == "i8/vec" {
            assert!(
                means[0] > 0.9,
                "{}: 8-bit scalar quantisation should already be near-exact at o=1",
                c.name()
            );
        }
        if c.name() == "thermo2b" || c.name() == "rabitq" || c.name() == "pq8x8" {
            assert!(
                means[4] > 0.5 || label.starts_with("uniform"),
                "{}: oversample 16 should recover much of the recall",
                c.name()
            );
        }
        if label.starts_with("clusters") || label.starts_with("anisotropic") {
            assert!(
                means[5] > 0.85,
                "{}: o=32 recall {:.3} too low on structured data",
                c.name(),
                means[5]
            );
        }
    }
    check_rabitq_estimator(data, rng, label);
}

fn main() {
    let (n, dim, nq) = (6000, 64, 100);
    let mut rng = Rng(0x871_871_871);
    for label in ["clusters(20)", "uniform", "anisotropic(latent 8)"] {
        let all = match label {
            "clusters(20)" => clusters(n + nq, dim, 20, &mut rng),
            "uniform" => uniform(n + nq, dim, &mut rng),
            _ => anisotropic(n + nq, dim, 8, &mut rng),
        };
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
        evaluate(label, &data, &queries, &mut rng);
    }
    println!("\nall quantisation and rescoring checks passed");
}
