//! RECIPE 876 (vectors): dimensionality reduction for vector search: Johnson-Lindenstrauss projections (Gaussian, sparse, Hadamard), PCA, whitening, OPQ-style rotation before PQ, Matryoshka-style prefixes.
//! LEVEL: intermediate
//! Use in: shrinking embeddings to save memory and scan time, preparing vectors for product quantisation, two-stage retrieval with short prefixes.   Run: cargo run --release --example dimensionality_reduction_jl_pca
#![allow(dead_code)]
//!
//! TWO FAMILIES. RANDOM PROJECTION never looks at the data: it comes with a worst-case guarantee for ANY point set, but spends dimensions
//! evenly. PCA looks at the data: it is the best linear reduction for squared reconstruction error on the data it was fitted on, promises
//! nothing about individual pairs, and must be refitted when the data drifts.
//!
//! JOHNSON-LINDENSTRAUSS. For m points, a random linear map R^D -> R^k with k = O(log m / eps^2) keeps EVERY pairwise squared distance
//! within a factor 1 +- eps with high probability; k does not depend on D. For a Gaussian matrix with entries N(0, 1/k), one fixed pair has
//!     P( | |Px|^2/|x|^2 - 1 | > eps ) <= 2 exp( -k (eps^2 - eps^3) / 4 ).
//! A union bound over the m(m-1)/2 pairs with total failure probability delta gives  k >= 4 ln( m(m-1)/delta ) / (eps^2 - eps^3).
//! The bound is CONSERVATIVE (union bounds ignore dependence); the run measures how much. Note how fast it grows as eps shrinks, and that it
//! can exceed D: JL is a statement about very high D, not a way to get 128-dim vectors down to 32 with 10% error.
//! Three constructions:
//!   * GAUSSIAN: dense, O(kD) per vector. |Px|^2/|x|^2 is chi-squared(k)/k: mean 1, variance 2/k, for every x.
//!   * SPARSE (Achlioptas; Li, Hastie, Church): entries +-sqrt(s/k) with probability 1/(2s) each, else 0. Only 1/s of the work. The variance
//!     of |Px|^2/|x|^2 is ( 2 + (s - 3) * sum x_i^4 / |x|^4 ) / k. With s = 3 (Achlioptas) it is exactly 2/k for every x. With large s it
//!     depends on how SPIKY x is: for a one-hot vector it is (s - 1)/k.
//!   * SRHT / fast JL (Ailon and Chazelle): sqrt(D/k) * S H D, where D flips signs at random, H is the orthonormal Walsh-Hadamard
//!     transform (O(D log D) by the butterfly) and S keeps k random coordinates. H D smears a spiky vector evenly over all coordinates,
//!     which is what makes plain coordinate sampling safe afterwards. Without H D, sampling a one-hot vector usually returns zero.
//!     Its tail bound has extra log factors, so the Gaussian formula above is NOT a theorem for it; here it is only measured.
//!
//! PCA. Centre the data, form the covariance C = X^T X / n, diagonalise C = V diag(l1 >= l2 >= ...) V^T, keep the first k columns of V. The
//! mean squared reconstruction error on the fitted data is EXACTLY the sum of the discarded eigenvalues (checked for every k), and no other
//! k-dimensional affine subspace does better (Eckart-Young). Two solvers: cyclic JACOBI rotations (all eigenpairs, very accurate, O(d^3)
//! per sweep, fine to a few hundred dims) and POWER ITERATION with deflation (top few only; slow when neighbouring eigenvalues are close).
//! WHITENING divides each PCA coordinate by sqrt(lambda) so the covariance becomes the identity. It CHANGES THE METRIC (to Mahalanobis
//! distance): low-variance directions, often noise, are amplified. It helps when the useful signal is not in the high-variance directions;
//! if plain L2 is your ground truth, it makes results worse (measured below).
//!
//! OPQ-STYLE ROTATION. Product quantisation cuts the vector into m blocks and gives each block the same code budget. If one block holds most
//! of the variance it is quantised badly while the others waste bits. Any orthogonal rotation keeps L2 distances unchanged, so rotate first.
//! PCA ALONE is the wrong rotation: it sorts variance, which packs the large components into the first block. EIGENVALUE ALLOCATION (the
//! parametric solution in Ge et al., "Optimized Product Quantization") deals the principal components to blocks, largest first, always
//! to the block whose PRODUCT of eigenvalues is smallest and still has room. Full OPQ also optimises the rotation iteratively against the
//! codebooks; that step is not implemented here.
//!
//! MATRYOSHKA-STYLE PREFIXES. Matryoshka embedding models are TRAINED so that the first d' coordinates are a usable embedding on their own.
//! Training is out of scope: here PCA ordering stands in (importance sorted by coordinate), and a random rotation of the same data is the
//! control where information is spread evenly. A prefix gives (a) cheaper first-stage search then full-dimension rerank, and (b) for L2 an
//! EXACT LOWER BOUND: partial squared distance only grows as coordinates are added, so a scan can stop early without changing results.
//! For cosine similarity, re-normalise after truncating (and note the lower-bound trick above is for L2 only).
//!
//! VERIFIED: the fast Hadamard transform equals the explicit matrix, H H^T = I, H(H x) = x and |H x| = |x|; for a fixed seed, over 200
//! independent trials per construction at the Gaussian-bound k, the fraction of trials with ANY pair outside 1 +- eps is <= delta (Gaussian
//! and Achlioptas: the theorem's promise; SRHT: measurement only), and at k/12 more than delta of trials fail; the variance of the norm
//! ratio matches the formulas on dense and one-hot vectors; coordinate sampling without H D loses a one-hot vector; eigenvectors are
//! orthonormal with tiny residual |Cv - lv|, trace is preserved, power iteration agrees with Jacobi, reconstruction MSE equals the discarded
//! eigenvalue sum for EVERY k and beats random subspaces; whitened covariance is the identity; rotations keep distances; eigenvalue
//! allocation lowers PQ distortion and raises recall over raw and PCA-only PQ on this anisotropic data; PCA prefixes beat random-rotation
//! prefixes in recall; prefix-pruned exact search returns exactly the brute-force result.
//! LIMITS: the PQ, whitening and prefix comparisons are measurements on one synthetic anisotropic distribution, where the importance of
//! directions is very uneven. On isotropic data none of PCA, OPQ rotation or PCA prefixes has anything to exploit.
//!
//! COMMON USES: Faiss `PCAMatrix`, `OPQMatrix`, `RandomRotationMatrix` pre-transforms, OpenAI/Nomic style truncatable embeddings with
//! rerank, random rotation before 1-bit codes (RaBitQ), sketching in numerical linear algebra, feature hashing's cousin in ML pipelines.
//! Read: Dasgupta and Gupta, "An Elementary Proof of a Theorem of Johnson and Lindenstrauss" (cited by title); Achlioptas,
//! "Database-friendly random projections" (cited by title); Li, Hastie and Church, "Very sparse random projections" (cited by title);
//! Ailon and Chazelle, "The fast Johnson-Lindenstrauss transform" (cited by title); Ge, He, Ke and Sun, "Optimized Product Quantization" (cited
//! by title); Matryoshka Representation Learning, Kusupati et al. https://arxiv.org/abs/2205.13147 ;
//! https://github.com/facebookresearch/faiss/wiki/Pre--and-post-processing
//!
//! TRY NEXT:
//!   - [S] Predict, then measure: halve eps in `jl_trials`. By what factor must k grow, and does the empirical failure rate move?
//!   - [S] Sweep s in {1, 3, 10, 32, 100} for `Sparse` on a one-hot vector and compare the measured variance with (s-1)/k.
//!   - [M] Add the Ky Fan check: for random orthonormal k-frames W, trace(W^T C W) <= l1 + ... + lk. Why is this the same fact as Eckart-Young?
//!   - [M] Implement the non-parametric OPQ loop: alternate (train PQ on RX) and (solve for the orthogonal R that best maps X to its
//!     reconstructions). You will need an SVD; build it from `jacobi` on A^T A and discuss when that loses precision.
//!   - [A] PROJECT: put eigenvalue-allocated PQ or SQ codes behind the rescoring pipeline of `scalar_binary_quantization_rescoring` and
//!     report bytes per vector against recall@10 with and without the rotation.

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
    fn gauss(&mut self) -> f64 {
        let (u, v) = (self.unit(), self.unit());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }
    fn gauss_vec(&mut self, n: usize) -> Vec<f64> {
        (0..n).map(|_| self.gauss()).collect()
    }
}
// f64 throughout: this file is about verifying linear-algebra identities, not about kernel speed.
type Vecs = Vec<Vec<f64>>;
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn norm2(a: &[f64]) -> f64 {
    dot(a, a)
}
fn dist2(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}
fn sub(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}
fn topk(q: &[f64], data: &[Vec<f64>], k: usize) -> Vec<usize> {
    let mut v: Vec<(f64, usize)> = data
        .iter()
        .enumerate()
        .map(|(i, x)| (dist2(q, x), i))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v.into_iter().take(k).map(|p| p.1).collect()
}
fn recall(got: &[usize], truth: &[usize]) -> f64 {
    got.iter().filter(|x| truth.contains(x)).count() as f64 / truth.len() as f64
}
fn mean_recall(queries: &[Vec<f64>], data: &[Vec<f64>], truth: &[Vec<usize>]) -> f64 {
    queries
        .iter()
        .zip(truth)
        .map(|(q, t)| recall(&topk(q, data, t.len()), t))
        .sum::<f64>()
        / queries.len() as f64
}

// ---------- linear maps as row lists ----------
fn apply(m: &[Vec<f64>], x: &[f64]) -> Vec<f64> {
    m.iter().map(|row| dot(row, x)).collect()
}
fn apply_all(m: &[Vec<f64>], xs: &[Vec<f64>]) -> Vecs {
    xs.iter().map(|x| apply(m, x)).collect()
}
/// Largest |(W W^T - I)_ij|: zero when the rows of W are orthonormal.
fn orthonormality_error(rows: &[Vec<f64>]) -> f64 {
    let mut worst = 0.0f64;
    for (i, a) in rows.iter().enumerate() {
        for (j, b) in rows.iter().enumerate() {
            worst = worst.max((dot(a, b) - if i == j { 1.0 } else { 0.0 }).abs());
        }
    }
    worst
}
/// Random orthonormal rows (Gram-Schmidt on gaussian vectors, applied twice for numerical safety).
fn random_orthonormal(k: usize, d: usize, rng: &mut Rng) -> Vecs {
    assert!(k <= d);
    let mut rows: Vecs = vec![];
    while rows.len() < k {
        let mut v = rng.gauss_vec(d);
        for _ in 0..2 {
            for r in &rows {
                let p = dot(&v, r);
                v.iter_mut().zip(r).for_each(|(x, y)| *x -= p * y);
            }
        }
        let n = norm2(&v).sqrt();
        if n > 1e-6 {
            v.iter_mut().for_each(|x| *x /= n);
            rows.push(v);
        }
    }
    rows
}

// ---------- Walsh-Hadamard transform ----------
/// In-place orthonormal fast Walsh-Hadamard transform (Sylvester order). Length must be a power of two. O(n log n).
fn fwht(x: &mut [f64]) {
    let n = x.len();
    assert!(n.is_power_of_two());
    let mut h = 1;
    while h < n {
        for block in x.chunks_exact_mut(2 * h) {
            let (a, b) = block.split_at_mut(h);
            for (p, q) in a.iter_mut().zip(b) {
                (*p, *q) = (*p + *q, *p - *q);
            }
        }
        h *= 2;
    }
    let s = 1.0 / (n as f64).sqrt();
    x.iter_mut().for_each(|v| *v *= s);
}
/// Oracle: H[i][j] = (-1)^popcount(i & j) / sqrt(n).
fn hadamard_matrix(n: usize) -> Vecs {
    let s = 1.0 / (n as f64).sqrt();
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| if (i & j).count_ones() % 2 == 0 { s } else { -s })
                .collect()
        })
        .collect()
}

// ---------- random projections ----------
trait Projection {
    fn project(&self, x: &[f64]) -> Vec<f64>;
}
struct Dense(Vecs);
impl Dense {
    fn gaussian(k: usize, d: usize, rng: &mut Rng) -> Self {
        let s = 1.0 / (k as f64).sqrt();
        Dense(
            (0..k)
                .map(|_| (0..d).map(|_| rng.gauss() * s).collect())
                .collect(),
        )
    }
}
impl Projection for Dense {
    fn project(&self, x: &[f64]) -> Vec<f64> {
        apply(&self.0, x)
    }
}
/// Sparse sign matrix stored as per-row lists of (column, sign).
struct Sparse {
    rows: Vec<Vec<(u32, bool)>>,
    scale: f64,
}
impl Sparse {
    fn new(k: usize, d: usize, s: usize, rng: &mut Rng) -> Self {
        let rows = (0..k)
            .map(|_| {
                (0..d as u32)
                    .filter_map(|j| {
                        // of 2s equally likely outcomes, one is "+", one is "-", the rest are zero
                        match rng.below(2 * s) {
                            0 => Some((j, true)),
                            1 => Some((j, false)),
                            _ => None,
                        }
                    })
                    .collect()
            })
            .collect();
        Sparse {
            rows,
            scale: (s as f64 / k as f64).sqrt(),
        }
    }
    fn nonzeros(&self) -> usize {
        self.rows.iter().map(|r| r.len()).sum()
    }
}
impl Projection for Sparse {
    fn project(&self, x: &[f64]) -> Vec<f64> {
        self.rows
            .iter()
            .map(|r| {
                self.scale
                    * r.iter()
                        .map(|&(j, plus)| if plus { x[j as usize] } else { -x[j as usize] })
                        .sum::<f64>()
            })
            .collect()
    }
}
struct Srht {
    signs: Vec<f64>,
    picks: Vec<usize>,
}
impl Srht {
    fn new(k: usize, d: usize, rng: &mut Rng) -> Self {
        assert!(d.is_power_of_two() && k <= d);
        let signs = (0..d)
            .map(|_| if rng.next_u64() >> 63 == 0 { 1.0 } else { -1.0 })
            .collect();
        let mut all: Vec<usize> = (0..d).collect();
        for i in 0..k {
            all.swap(i, i + rng.below(d - i)); // sample without replacement
        }
        all.truncate(k);
        Srht { signs, picks: all }
    }
}
impl Projection for Srht {
    fn project(&self, x: &[f64]) -> Vec<f64> {
        let mut y: Vec<f64> = x.iter().zip(&self.signs).map(|(v, s)| v * s).collect();
        fwht(&mut y);
        let scale = (y.len() as f64 / self.picks.len() as f64).sqrt();
        self.picks.iter().map(|&i| y[i] * scale).collect()
    }
}

fn jl_dim(m: usize, eps: f64, delta: f64) -> usize {
    (4.0 * ((m * (m - 1)) as f64 / delta).ln() / (eps * eps - eps * eps * eps)).ceil() as usize
}
/// Worst | |P(a-b)|^2 / |a-b|^2 - 1 | over all pairs, for pre-computed difference vectors with their squared norms.
fn max_distortion(p: &dyn Projection, points: &[Vec<f64>]) -> f64 {
    // the map is linear, so project each point once and compare pair distances
    let proj: Vecs = points.iter().map(|x| p.project(x)).collect();
    let mut worst = 0.0f64;
    for i in 0..points.len() {
        for j in 0..i {
            worst =
                worst.max((dist2(&proj[i], &proj[j]) / dist2(&points[i], &points[j]) - 1.0).abs());
        }
    }
    worst
}
type Maker = Box<dyn Fn(usize, usize, &mut Rng) -> Box<dyn Projection>>;
fn constructions() -> Vec<(&'static str, bool, Maker)> {
    vec![
        (
            "gaussian",
            true,
            Box::new(|k, d, r: &mut Rng| Box::new(Dense::gaussian(k, d, r)) as Box<dyn Projection>),
        ),
        (
            "sparse s=3",
            true,
            Box::new(|k, d, r: &mut Rng| Box::new(Sparse::new(k, d, 3, r)) as Box<dyn Projection>),
        ),
        (
            "srht",
            false,
            Box::new(|k, d, r: &mut Rng| Box::new(Srht::new(k, d, r)) as Box<dyn Projection>),
        ),
    ]
}

fn part_hadamard(rng: &mut Rng) {
    for log in 0..=7 {
        let n = 1usize << log;
        let h = hadamard_matrix(n);
        assert!(orthonormality_error(&h) < 1e-12, "H H^T != I at n={n}");
        for _ in 0..20 {
            let x = rng.gauss_vec(n);
            let mut y = x.clone();
            fwht(&mut y);
            assert!(
                dist2(&y, &apply(&h, &x)) < 1e-20,
                "butterfly disagrees with the matrix at n={n}"
            );
            assert!(
                (norm2(&y) - norm2(&x)).abs() <= 1e-10 * norm2(&x),
                "not an isometry"
            );
            fwht(&mut y);
            assert!(dist2(&y, &x) < 1e-20, "H is its own inverse");
        }
    }
    println!(
        "walsh-hadamard: butterfly = explicit matrix, orthogonal, involutive and norm-preserving for n = 1..128"
    );
}

fn part_jl(rng: &mut Rng) {
    let (m, d, eps, delta, trials) = (24usize, 1024usize, 0.5f64, 0.05f64, 200usize);
    let k = jl_dim(m, eps, delta);
    // a point set with both dense and spiky members (spiky points are the hard case for sparse sampling)
    let points: Vecs = (0..m)
        .map(|i| {
            if i % 3 == 0 {
                let mut v = vec![0.0; d];
                v[rng.below(d)] = 10.0;
                v[rng.below(d)] += 3.0 + i as f64;
                v
            } else {
                rng.gauss_vec(d)
            }
        })
        .collect();
    println!(
        "\n=== Johnson-Lindenstrauss: m={m} points in R^{d}, eps={eps}, delta={delta}: bound k = {k} ==="
    );
    println!(
        "  {:<12} {:>5} {:>8} {:>16} {:>14} {:>12}",
        "projection", "k", "trials", "failed (any pair)", "mean worst", "max worst"
    );
    for (name, has_theorem, make) in constructions() {
        for (kk, tr) in [(k / 12, trials), (k / 4, trials), (k, trials)] {
            let worst: Vec<f64> = (0..tr)
                .map(|_| max_distortion(make(kk, d, rng).as_ref(), &points))
                .collect();
            let failed = worst.iter().filter(|&&w| w > eps).count();
            println!(
                "  {name:<12} {kk:>5} {tr:>8} {failed:>16} {:>14.3} {:>12.3}",
                worst.iter().sum::<f64>() / tr as f64,
                worst.iter().fold(0.0f64, |a, &b| a.max(b))
            );
            if kk == k {
                assert!(
                    failed as f64 <= delta * tr as f64,
                    "{name}: {failed}/{tr} trials broke eps at the bound (theorem applies: {has_theorem})"
                );
            }
            if kk == k / 12 {
                assert!(
                    failed as f64 > delta * tr as f64,
                    "{name}: k/12 should be too small, otherwise this experiment shows nothing"
                );
            }
        }
    }
    println!(
        "  (compare the failures at the bound k with the {} the theorem allows: that slack is its conservatism. The smaller k rows show k still matters)",
        (delta * trials as f64) as usize
    );
    for (e, mm) in [(0.1, 1_000_000), (0.2, 1_000_000), (0.5, 1_000_000)] {
        println!(
            "  bound for 1e6 points, eps={e}: k = {}",
            jl_dim(mm, e, 0.05)
        );
    }
}

/// Sample variance of |Px|^2/|x|^2 over fresh projections, and the mean.
fn ratio_stats(
    x: &[f64],
    trials: usize,
    rng: &mut Rng,
    make: &dyn Fn(&mut Rng) -> Box<dyn Projection>,
) -> (f64, f64) {
    let r: Vec<f64> = (0..trials)
        .map(|_| norm2(&make(rng).project(x)) / norm2(x))
        .collect();
    let mean = r.iter().sum::<f64>() / trials as f64;
    (
        mean,
        r.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (trials - 1) as f64,
    )
}

fn part_variance(rng: &mut Rng) {
    let (d, k, trials) = (256usize, 32usize, 3000usize);
    let dense = rng.gauss_vec(d);
    let mut onehot = vec![0.0; d];
    onehot[17] = 4.0;
    println!("\n=== variance of |Px|^2/|x|^2 (d={d}, k={k}, {trials} projections each) ===");
    println!(
        "  {:<12} {:<8} {:>8} {:>10} {:>10}",
        "projection", "vector", "mean", "variance", "formula"
    );
    for (vname, x) in [("dense", &dense), ("one-hot", &onehot)] {
        let spiky = x.iter().map(|v| v.powi(4)).sum::<f64>() / norm2(x).powi(2);
        for s in [0usize, 3, 32] {
            let name = if s == 0 {
                "gaussian".to_string()
            } else {
                format!("sparse s={s}")
            };
            let formula = if s == 0 {
                2.0 / k as f64
            } else {
                (2.0 + (s as f64 - 3.0) * spiky) / k as f64
            };
            let make = |r: &mut Rng| -> Box<dyn Projection> {
                if s == 0 {
                    Box::new(Dense::gaussian(k, d, r))
                } else {
                    Box::new(Sparse::new(k, d, s, r))
                }
            };
            let (mean, var) = ratio_stats(x, trials, rng, &make);
            println!("  {name:<12} {vname:<8} {mean:>8.3} {var:>10.4} {formula:>10.4}");
            assert!(
                (mean - 1.0).abs() < 0.04,
                "projection must be unbiased: mean {mean}"
            );
            assert!(
                (var / formula - 1.0).abs() < 0.2,
                "{name} on {vname}: variance {var} vs formula {formula}"
            );
        }
    }
    let (m, v) = ratio_stats(&onehot, 500, rng, &|r| Box::new(Srht::new(k, d, r)));
    println!(
        "  {:<12} {:<8} {m:>8.3} {v:>10.4}   H D spreads the spike over all coordinates",
        "srht", "one-hot"
    );
    assert!(
        (m - 1.0).abs() < 1e-9 && v < 1e-18,
        "every coordinate of H D e_i has magnitude 1/sqrt(d), so any sample is exact"
    );
    // the same coordinate sampling WITHOUT H D: almost always loses the spike entirely
    let lost = (0..500)
        .filter(|_| !Srht::new(k, d, rng).picks.contains(&17))
        .count();
    println!(
        "  sampling k of d coordinates without H D loses the one-hot vector completely in {lost}/500 draws"
    );
    assert!(lost > 350);
    let sp = Sparse::new(k, d, 32, rng);
    println!(
        "  work per vector: dense {} multiply-adds, sparse s=32 about {} ({} stored in this draw)",
        k * d,
        k * d / 32,
        sp.nonzeros()
    );
}

// ---------- PCA ----------
fn mean_of(xs: &[Vec<f64>]) -> Vec<f64> {
    let mut m = vec![0.0; xs[0].len()];
    for x in xs {
        m.iter_mut()
            .zip(x)
            .for_each(|(a, b)| *a += b / xs.len() as f64);
    }
    m
}
/// Population covariance (divide by n) of already centred rows.
fn covariance(centred: &[Vec<f64>]) -> Vecs {
    let d = centred[0].len();
    let mut c = vec![vec![0.0; d]; d];
    for x in centred {
        for i in 0..d {
            for j in 0..d {
                c[i][j] += x[i] * x[j] / centred.len() as f64;
            }
        }
    }
    c
}
/// Cyclic Jacobi for a symmetric matrix: repeatedly rotate in the (p,q) plane to zero a[p][q]. Returns eigenvalues (descending)
/// and the matching eigenvectors as ROWS.
fn jacobi(sym: &[Vec<f64>]) -> (Vec<f64>, Vecs) {
    let n = sym.len();
    let mut a: Vecs = sym.to_vec();
    let mut v: Vecs = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect(); // rows = eigenvectors
    let scale: f64 = sym.iter().map(|r| norm2(r)).sum::<f64>().sqrt().max(1e-300);
    for _sweep in 0..60 {
        let off: f64 = (0..n)
            .flat_map(|i| (0..n).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        if off.sqrt() <= 1e-13 * scale {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() <= 1e-300 {
                    continue;
                }
                // angle that zeroes a[p][q]; t is the smaller root of t^2 + 2 theta t - 1 = 0, the numerically stable choice
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (x, y) = (row[p], row[q]);
                    row[p] = c * x - s * y;
                    row[q] = s * x + c * y;
                }
                for j in 0..n {
                    let (x, y) = (a[p][j], a[q][j]);
                    a[p][j] = c * x - s * y;
                    a[q][j] = s * x + c * y;
                    let (x, y) = (v[p][j], v[q][j]);
                    v[p][j] = c * x - s * y;
                    v[q][j] = s * x + c * y;
                }
            }
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| a[j][j].total_cmp(&a[i][i]));
    (
        order.iter().map(|&i| a[i][i]).collect(),
        order.iter().map(|&i| v[i].clone()).collect(),
    )
}
/// Top-r eigenpairs by power iteration, deflating after each one.
fn power_iteration(sym: &[Vec<f64>], r: usize, iters: usize, rng: &mut Rng) -> (Vec<f64>, Vecs) {
    let n = sym.len();
    let (mut vals, mut vecs): (Vec<f64>, Vecs) = (vec![], vec![]);
    for _ in 0..r {
        let mut v = rng.gauss_vec(n);
        for _ in 0..iters {
            let mut w = apply(sym, &v);
            // deflation by projection: remove directions already found (stable, no need to modify the matrix)
            for u in &vecs {
                let p = dot(&w, u);
                w.iter_mut().zip(u).for_each(|(x, y)| *x -= p * y);
            }
            let nw = norm2(&w).sqrt();
            v = w.into_iter().map(|x| x / nw).collect();
        }
        vals.push(dot(&v, &apply(sym, &v)));
        vecs.push(v);
    }
    (vals, vecs)
}
struct Pca {
    mean: Vec<f64>,
    values: Vec<f64>,
    /// principal directions as rows, largest variance first
    axes: Vecs,
}
impl Pca {
    fn fit(xs: &[Vec<f64>]) -> Pca {
        let mean = mean_of(xs);
        let centred: Vecs = xs.iter().map(|x| sub(x, &mean)).collect();
        let (values, axes) = jacobi(&covariance(&centred));
        Pca { mean, values, axes }
    }
    fn transform(&self, x: &[f64]) -> Vec<f64> {
        apply(&self.axes, &sub(x, &self.mean))
    }
    fn transform_all(&self, xs: &[Vec<f64>]) -> Vecs {
        xs.iter().map(|x| self.transform(x)).collect()
    }
}
/// Mean squared error of projecting centred data onto the span of the orthonormal rows `basis` and back.
fn reconstruction_mse(xs: &[Vec<f64>], mean: &[f64], basis: &[Vec<f64>]) -> f64 {
    let d = mean.len();
    xs.iter()
        .map(|x| {
            let c = sub(x, mean);
            let mut back = vec![0.0; d];
            for axis in basis {
                let p = dot(axis, &c);
                back.iter_mut().zip(axis).for_each(|(b, a)| *b += p * a);
            }
            dist2(&c, &back)
        })
        .sum::<f64>()
        / xs.len() as f64
}

/// "Real-like" anisotropic data: independent axes with geometrically decaying scale, grouped in clusters, then rotated inside
/// small coordinate blocks so that coordinates are correlated but high variance stays concentrated in the early coordinates.
fn anisotropic(n: usize, d: usize, rng: &mut Rng) -> Vecs {
    let scale: Vec<f64> = (0..d)
        .map(|i| 4.0 * 0.85f64.powi(i as i32) + 0.05)
        .collect();
    let centres: Vecs = (0..20)
        .map(|_| (0..d).map(|i| rng.gauss() * scale[i]).collect())
        .collect();
    let blocks: Vec<Vecs> = (0..d / 4).map(|_| random_orthonormal(4, 4, rng)).collect();
    (0..n)
        .map(|_| {
            let c = &centres[rng.below(centres.len())];
            let raw: Vec<f64> = (0..d)
                .map(|i| c[i] + rng.gauss() * scale[i] * 0.6)
                .collect();
            raw.chunks(4)
                .zip(&blocks)
                .flat_map(|(chunk, r)| apply(r, chunk))
                .collect()
        })
        .collect()
}

fn part_pca(data: &[Vec<f64>], rng: &mut Rng) -> Pca {
    let d = data[0].len();
    let pca = Pca::fit(data);
    let centred: Vecs = data.iter().map(|x| sub(x, &pca.mean)).collect();
    let cov = covariance(&centred);
    let ortho = orthonormality_error(&pca.axes);
    let residual = pca
        .axes
        .iter()
        .zip(&pca.values)
        .map(|(v, &l)| {
            dist2(
                &apply(&cov, v),
                &v.iter().map(|x| x * l).collect::<Vec<_>>(),
            )
            .sqrt()
        })
        .fold(0.0, f64::max);
    let trace: f64 = (0..d).map(|i| cov[i][i]).sum();
    let total: f64 = pca.values.iter().sum();
    println!(
        "\n=== PCA on {} anisotropic points in R^{d} (Jacobi) ===",
        data.len()
    );
    println!(
        "  |V V^T - I|max = {ortho:.1e}, max |Cv - lv| = {residual:.1e}, trace {trace:.6} vs sum of eigenvalues {total:.6}"
    );
    assert!(ortho < 1e-10, "eigenvectors not orthonormal: {ortho:e}");
    assert!(
        residual < 1e-9 * pca.values[0],
        "eigen residual {residual:e}"
    );
    assert!((trace - total).abs() < 1e-9 * trace);
    assert!(
        pca.values.windows(2).all(|w| w[0] >= w[1]) && pca.values[d - 1] > -1e-9,
        "covariance is positive semi-definite"
    );

    let mut worst_gap = 0.0f64;
    for k in 0..=d {
        let mse = reconstruction_mse(data, &pca.mean, &pca.axes[..k]);
        let discarded: f64 = pca.values[k..].iter().sum();
        worst_gap = worst_gap.max((mse - discarded).abs() / total);
        if k % 8 == 0 {
            let random = reconstruction_mse(data, &pca.mean, &random_orthonormal(k, d, rng));
            println!(
                "  k={k:>2}: reconstruction MSE {mse:>10.5} = discarded eigenvalue sum {discarded:>10.5} | random subspace {random:>10.5}"
            );
            for _ in 0..5 {
                let other = reconstruction_mse(data, &pca.mean, &random_orthonormal(k, d, rng));
                assert!(
                    other + 1e-9 >= mse,
                    "a random {k}-subspace beat PCA: {other} < {mse}"
                );
            }
        }
    }
    assert!(
        worst_gap < 1e-10,
        "reconstruction error must equal the discarded eigenvalue sum (relative gap {worst_gap:e})"
    );

    let (pv, pvec) = power_iteration(&cov, 4, 3000, rng);
    for i in 0..4 {
        let cos = dot(&pvec[i], &pca.axes[i]).abs(); // an eigenvector is defined up to sign
        assert!(
            (pv[i] - pca.values[i]).abs() < 1e-6 * pca.values[0],
            "power iteration eigenvalue {i}: {} vs {}",
            pv[i],
            pca.values[i]
        );
        assert!(
            cos > 0.9999,
            "power iteration eigenvector {i}: |cos| = {cos}"
        );
    }
    println!("  power iteration + deflation agrees with Jacobi on the top 4 pairs");
    pca
}

fn part_whitening(pca: &Pca, data: &[Vec<f64>], queries: &[Vec<f64>], truth: &[Vec<usize>]) {
    let white = |x: &[f64]| -> Vec<f64> {
        pca.transform(x)
            .iter()
            .zip(&pca.values)
            .map(|(v, l)| v / l.sqrt())
            .collect()
    };
    let wd: Vecs = data.iter().map(|x| white(x)).collect();
    let wq: Vecs = queries.iter().map(|x| white(x)).collect();
    let c = covariance(&wd);
    let err = c
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.iter()
                .enumerate()
                .map(move |(j, v)| (v - if i == j { 1.0 } else { 0.0 }).abs())
        })
        .fold(0.0, f64::max);
    let rot = mean_recall(&pca.transform_all(queries), &pca.transform_all(data), truth);
    let w = mean_recall(&wq, &wd, truth);
    println!(
        "\n=== whitening ===\n  |cov(whitened) - I|max = {err:.1e}; recall@10 against L2 truth: PCA rotation {rot:.3}, whitened {w:.3} (the metric changed)"
    );
    assert!(
        err < 1e-7,
        "whitened covariance is not the identity: {err:e}"
    );
    assert!(
        (rot - 1.0).abs() < 1e-12,
        "a full-rank rotation plus translation preserves every L2 neighbour"
    );
    assert!(
        w < 0.7,
        "whitening should disagree with L2 neighbours on strongly anisotropic data"
    );
}

// ---------- tiny product quantiser ----------
struct Pq {
    block: usize,
    /// books[block][code] = centroid
    books: Vec<Vecs>,
}
impl Pq {
    fn train(xs: &[Vec<f64>], m: usize, ksub: usize, iters: usize, rng: &mut Rng) -> Pq {
        let d = xs[0].len();
        assert_eq!(d % m, 0);
        let block = d / m;
        let books = (0..m)
            .map(|b| {
                let part: Vecs = xs
                    .iter()
                    .map(|x| x[b * block..(b + 1) * block].to_vec())
                    .collect();
                kmeans(&part, ksub, iters, rng)
            })
            .collect();
        Pq { block, books }
    }
    fn encode(&self, x: &[f64]) -> Vec<usize> {
        x.chunks(self.block)
            .zip(&self.books)
            .map(|(c, book)| nearest(c, book))
            .collect()
    }
    fn decode(&self, code: &[usize]) -> Vec<f64> {
        code.iter()
            .zip(&self.books)
            .flat_map(|(&c, b)| b[c].clone())
            .collect()
    }
}
fn nearest(x: &[f64], cents: &[Vec<f64>]) -> usize {
    (0..cents.len())
        .min_by(|&a, &b| dist2(x, &cents[a]).total_cmp(&dist2(x, &cents[b])))
        .unwrap()
}
/// Lloyd with k-means++ seeding (plain random seeds make the comparison between rotations too noisy).
fn kmeans(xs: &[Vec<f64>], k: usize, iters: usize, rng: &mut Rng) -> Vecs {
    let mut cents = vec![xs[rng.below(xs.len())].clone()];
    let mut best: Vec<f64> = xs.iter().map(|x| dist2(x, &cents[0])).collect();
    while cents.len() < k {
        let total: f64 = best.iter().sum();
        let mut target = rng.unit() * total;
        let mut pick = xs.len() - 1;
        for (i, b) in best.iter().enumerate() {
            target -= b;
            if target <= 0.0 {
                pick = i;
                break;
            }
        }
        cents.push(xs[pick].clone());
        best.iter_mut()
            .zip(xs)
            .for_each(|(b, x)| *b = b.min(dist2(x, &xs[pick])));
    }
    for _ in 0..iters {
        let mut sum = vec![vec![0.0; xs[0].len()]; k];
        let mut cnt = vec![0usize; k];
        for x in xs {
            let c = nearest(x, &cents);
            cnt[c] += 1;
            sum[c].iter_mut().zip(x).for_each(|(s, v)| *s += v);
        }
        for c in 0..k {
            if cnt[c] > 0 {
                cents[c] = sum[c].iter().map(|s| s / cnt[c] as f64).collect();
            }
        }
    }
    cents
}
/// Eigenvalue allocation: returns for each output block the list of principal component indices it receives.
fn allocate(values: &[f64], m: usize) -> Vec<Vec<usize>> {
    let cap = values.len() / m;
    let mut blocks = vec![Vec::new(); m];
    let mut log_product = vec![0.0f64; m];
    // `values` is already descending. Logs are taken relative to the smallest eigenvalue so every term is >= 0; otherwise values
    // below 1 would make a block look "smaller" the more components it already has.
    let floor = values.last().unwrap().max(1e-12);
    for (i, &l) in values.iter().enumerate() {
        let b = (0..m)
            .filter(|&b| blocks[b].len() < cap)
            .min_by(|&a, &b| {
                log_product[a]
                    .total_cmp(&log_product[b])
                    .then(blocks[a].len().cmp(&blocks[b].len()))
            })
            .unwrap();
        blocks[b].push(i);
        log_product[b] += (l.max(floor) / floor).ln();
    }
    blocks
}

struct PqResult {
    distortion: f64,
    recall10: f64,
    recall10_at_50: f64,
    block_share: Vec<f64>,
}
fn eval_pq(
    data: &[Vec<f64>],
    queries: &[Vec<f64>],
    truth: &[Vec<usize>],
    m: usize,
    rng: &mut Rng,
) -> PqResult {
    let pq = Pq::train(data, m, 16, 10, rng);
    let recon: Vecs = data.iter().map(|x| pq.decode(&pq.encode(x))).collect();
    let distortion = data
        .iter()
        .zip(&recon)
        .map(|(a, b)| dist2(a, b))
        .sum::<f64>()
        / data.len() as f64;
    // asymmetric distance (query exact, database reconstructed) equals what ADC lookup tables would compute
    let (mut r10, mut r50) = (0.0, 0.0);
    for (q, t) in queries.iter().zip(truth) {
        let cand = topk(q, &recon, 50);
        r10 += recall(&cand[..10], t);
        r50 += recall(&cand, t);
    }
    let mean = mean_of(data);
    let centred: Vecs = data.iter().map(|x| sub(x, &mean)).collect();
    let var: Vec<f64> = (0..mean.len())
        .map(|j| centred.iter().map(|x| x[j] * x[j]).sum::<f64>() / data.len() as f64)
        .collect();
    let total: f64 = var.iter().sum();
    let block_share = var
        .chunks(mean.len() / m)
        .map(|c| c.iter().sum::<f64>() / total)
        .collect();
    let nq = queries.len() as f64;
    PqResult {
        distortion,
        recall10: r10 / nq,
        recall10_at_50: r50 / nq,
        block_share,
    }
}

fn part_opq(
    pca: &Pca,
    data: &[Vec<f64>],
    queries: &[Vec<f64>],
    truth: &[Vec<usize>],
    rng: &mut Rng,
) {
    let (d, m) = (data[0].len(), 8);
    let alloc = allocate(&pca.values, m);
    let mut seen: Vec<usize> = alloc.concat();
    seen.sort();
    assert_eq!(
        seen,
        (0..d).collect::<Vec<_>>(),
        "allocation must be a permutation"
    );
    assert!(
        alloc.iter().all(|b| b.len() == d / m),
        "blocks must be equally sized"
    );
    let opq_axes: Vecs = alloc
        .concat()
        .into_iter()
        .map(|i| pca.axes[i].clone())
        .collect();
    assert!(orthonormality_error(&opq_axes) < 1e-10);
    let random = random_orthonormal(d, d, rng);
    let centred = |xs: &[Vec<f64>]| -> Vecs { xs.iter().map(|x| sub(x, &pca.mean)).collect() };
    let (cd, cq) = (centred(data), centred(queries));
    let setups: [(&str, Vecs, Vecs); 4] = [
        ("raw coordinates", cd.clone(), cq.clone()),
        (
            "PCA only (sorted)",
            apply_all(&pca.axes, &cd),
            apply_all(&pca.axes, &cq),
        ),
        (
            "random rotation",
            apply_all(&random, &cd),
            apply_all(&random, &cq),
        ),
        (
            "eigenvalue allocation",
            apply_all(&opq_axes, &cd),
            apply_all(&opq_axes, &cq),
        ),
    ];
    for (name, xd, xq) in &setups {
        // rotation invariance: query-to-point distances are unchanged, so the exact ground truth is the same
        for i in 0..20 {
            let (a, b) = (dist2(&cq[i], &cd[i * 7]), dist2(&xq[i], &xd[i * 7]));
            assert!(
                (a - b).abs() < 1e-8 * a.max(1.0),
                "{name} changed a distance"
            );
        }
    }
    println!(
        "\n=== OPQ-style rotation before PQ (m={m} blocks x 16 centroids = {} bits/vector; 3 k-means seeds averaged) ===",
        m * 4
    );
    println!(
        "  {:<22} {:>12} {:>10} {:>12}  variance share per block",
        "rotation", "distortion", "recall@10", "recall@10|50"
    );
    let mut out = vec![];
    for (name, xd, xq) in &setups {
        let runs: Vec<PqResult> = (0..3).map(|_| eval_pq(xd, xq, truth, m, rng)).collect();
        let avg =
            |f: &dyn Fn(&PqResult) -> f64| runs.iter().map(f).sum::<f64>() / runs.len() as f64;
        let (dist, r10, r50) = (
            avg(&|r| r.distortion),
            avg(&|r| r.recall10),
            avg(&|r| r.recall10_at_50),
        );
        let shares: Vec<String> = runs[0]
            .block_share
            .iter()
            .map(|s| format!("{:.0}%", 100.0 * s))
            .collect();
        println!(
            "  {name:<22} {dist:>12.3} {r10:>10.3} {r50:>12.3}  {}",
            shares.join(" ")
        );
        out.push((dist, r10));
    }
    let (raw, sorted, alloc_r) = (out[0], out[1], out[3]);
    assert!(
        alloc_r.0 < raw.0 && alloc_r.0 < sorted.0,
        "eigenvalue allocation should quantise with less error: {out:?}"
    );
    assert!(
        alloc_r.1 > raw.1 + 0.03 && alloc_r.1 > sorted.1 + 0.03,
        "and recall better than raw and PCA-only PQ: {out:?}"
    );
}

fn part_prefixes(
    pca: &Pca,
    data: &[Vec<f64>],
    queries: &[Vec<f64>],
    truth: &[Vec<usize>],
    rng: &mut Rng,
) {
    let d = data[0].len();
    let (pd, pq) = (pca.transform_all(data), pca.transform_all(queries));
    let random = random_orthonormal(d, d, rng);
    let (rd, rq) = (apply_all(&random, &pd), apply_all(&random, &pq));
    let cut = |xs: &[Vec<f64>], k: usize| -> Vecs { xs.iter().map(|x| x[..k].to_vec()).collect() };
    println!(
        "\n=== Matryoshka-style prefixes: recall@10 using only the first d' of {d} coordinates ==="
    );
    println!(
        "  {:>4} {:>12} {:>20} {:>14} {:>22}",
        "d'", "PCA prefix", "PCA shortlist 50+rerank", "random prefix", "random shortlist+rerank"
    );
    let mut prev = 0.0;
    for k in [4usize, 8, 16, 32, d] {
        let mut row = vec![];
        for (xd, xq) in [(&pd, &pq), (&rd, &rq)] {
            let (sd, sq) = (cut(xd, k), cut(xq, k));
            let plain = mean_recall(&sq, &sd, truth);
            // two stage: shortlist 50 on the prefix, rerank with every coordinate
            let two: f64 = (0..xq.len())
                .map(|i| {
                    let mut c: Vec<(f64, usize)> = topk(&sq[i], &sd, 50)
                        .into_iter()
                        .map(|j| (dist2(&xq[i], &xd[j]), j))
                        .collect();
                    c.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                    recall(
                        &c.iter().take(10).map(|p| p.1).collect::<Vec<_>>(),
                        &truth[i],
                    )
                })
                .sum::<f64>()
                / xq.len() as f64;
            assert!(
                two + 1e-12 >= plain,
                "reranking a superset of the prefix top-10 cannot lose a true neighbour"
            );
            row.push((plain, two));
        }
        println!(
            "  {k:>4} {:>12.3} {:>20.3} {:>14.3} {:>22.3}",
            row[0].0, row[0].1, row[1].0, row[1].1
        );
        if k < d {
            assert!(
                row[0].0 > row[1].0,
                "at d'={k} the importance-ordered prefix should win"
            );
        } else {
            assert!(
                (row[0].0 - 1.0).abs() < 1e-12 && (row[1].0 - 1.0).abs() < 1e-12,
                "full dimension is exact in any rotation"
            );
        }
        assert!(row[0].0 + 1e-9 >= prev);
        prev = row[0].0;
    }
    // exact top-10 with early abandonment: the partial squared distance is a lower bound on the full one
    let mut read = [0usize; 2];
    for (s, (xd, xq)) in [(&pd, &pq), (&rd, &rq)].into_iter().enumerate() {
        for (qi, q) in xq.iter().enumerate() {
            let mut best: Vec<(f64, usize)> = vec![]; // sorted, at most 10
            for (i, x) in xd.iter().enumerate() {
                let bound = if best.len() < 10 {
                    f64::INFINITY
                } else {
                    best[9].0
                };
                let mut acc = 0.0;
                let mut used = 0;
                for (a, b) in q.chunks(4).zip(x.chunks(4)) {
                    acc += dist2(a, b);
                    used += a.len();
                    if acc > bound {
                        break;
                    }
                }
                read[s] += used;
                if acc <= bound {
                    let at = best.partition_point(|e| (e.0, e.1) < (acc, i));
                    best.insert(at, (acc, i));
                    best.truncate(10);
                }
            }
            assert_eq!(
                best.iter().map(|p| p.1).collect::<Vec<_>>(),
                truth[qi],
                "pruned scan must be EXACT (basis {s}, query {qi})"
            );
        }
    }
    let full = queries.len() * data.len() * d;
    println!(
        "  exact search with early abandonment (identical results): coordinates read: PCA basis {:.1}%, random basis {:.1}%",
        100.0 * read[0] as f64 / full as f64,
        100.0 * read[1] as f64 / full as f64
    );
    assert!(
        read[0] * 2 < read[1],
        "PCA ordering should let the scan stop at least twice as early: {read:?}"
    );
}

fn main() {
    let mut rng = Rng(0x876_876);
    part_hadamard(&mut rng);
    part_jl(&mut rng);
    part_variance(&mut rng);

    let (n, nq, d) = (2000usize, 100usize, 48usize);
    let mut all = anisotropic(n + nq, d, &mut rng);
    let queries = all.split_off(n);
    let data = all;
    let truth: Vec<Vec<usize>> = queries.iter().map(|q| topk(q, &data, 10)).collect();
    let pca = part_pca(&data, &mut rng);
    part_whitening(&pca, &data, &queries, &truth);
    part_opq(&pca, &data, &queries, &truth, &mut rng);
    part_prefixes(&pca, &data, &queries, &truth, &mut rng);
    println!("\nall dimensionality reduction checks passed");
}
