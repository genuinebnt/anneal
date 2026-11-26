//! RECIPE 884 (hpc): sparse kernels: CSR/CSC/COO/ELL/SELL-C-sigma SpMV, merge-path SpMV, SpGEMM, SpTRSV, RCM reordering
//! LEVEL: advanced
//! Use in: graph analytics (PageRank = repeated SpMV), finite elements, iterative solvers (CG/GMRES), GNN message passing.   Run: cargo run --release --example sparse_kernels_spmv_csr
//! COMMON USES: PageRank and BFS-as-linear-algebra, sparse linear solvers, recommender factorisation, circuit simulation, sparse tensors in ML.
//! Course: CMU 15-418/618 (parallel computer architecture) for the load-balancing view; Kreutzer et al. for SELL-C-sigma.
#![allow(dead_code)]
//!
//! A sparse matrix stores only its non-zeros, so SpMV (y = A x) does 2 flops per stored value but reads at least
//! 12-16 bytes per value (value + column index + an x gather): it is memory-bound, and the whole game is (a) a layout
//! that streams, (b) keeping the gather on x cache-friendly, (c) balancing work across threads.
//!
//! Formats implemented here, all over i64 so that every comparison against the dense oracle is EXACT:
//!   * COO  : (row, col, val) triples; duplicates allowed and summed. Easy to build, scatter-style SpMV.
//!   * CSR  : row pointers + column indices + values. The default; row-parallel SpMV has no write conflicts.
//!   * CSC  : CSR of the transpose; SpMV scatters into y (column-oriented).
//!   * ELL  : pad every row to the longest row; regular, SIMD-friendly, wasteful when row lengths vary.
//!   * SELL-C-sigma : sort rows by length inside windows of sigma, cut into chunks of C rows, pad each chunk only to
//!     ITS longest row, store column-major inside the chunk so C rows advance in lock-step (one SIMD lane per row).
//! Kernels: serial, row-parallel (thread::scope over disjoint slices of y), merge-path (Merrill & Garland): split the
//! combined sequence (row ends, non-zeros) into equal diagonals so a single dense row cannot starve one thread.
//! Also Gustavson SpGEMM (row-by-row with a dense accumulator and a marker array), SpTRSV with level scheduling
//! (rows in one level are independent), and reverse Cuthill-McKee to shrink matrix bandwidth (locality of x gathers).
//! Everything is checked against a dense oracle on 1000 random matrices, including empty rows/cols and COO duplicates.
//! Read: Kreutzer et al., "A unified sparse matrix data format for efficient general sparse matrix-vector multiplication
//!   on modern processors with wide SIMD units" https://arxiv.org/abs/1307.6209
//! Read: Merrill & Garland, "Merge-based parallel sparse matrix-vector multiplication" (SC16; cited by title, not re-read here)
//! Read: Templates for the Solution of Linear Systems (storage formats) https://www.netlib.org/linalg/html_templates/node91.html
//! Read: Cuthill-McKee https://en.wikipedia.org/wiki/Cuthill%E2%80%93McKee_algorithm and SuiteSparse collection https://sparse.tamu.edu/
//! TRY NEXT:
//!   - [S] Change `Sell::from_csr` to use sigma = 1 (no sorting) and print `stored()` on the skewed matrix; predict first how much padding returns.
//!   - [S] Add a `Csr::spmv_transpose` (y = A^T x) that scatters like CSC; assert it equals `csc_spmv` on the 1000 random matrices.
//!   - [M] Predict then measure: how does `spmv_merge` fixup count (the Vec returned per thread) grow with thread count T on the skewed matrix? Print it.
//!   - [M] Port `spmv_csr` to f32 + `chunks_exact(4)` accumulators (see loop_optimization_ladder) and compare GFLOP/s on the uniform matrix.
//!   - [A] PROJECT: build PageRank (power iteration over a CSR of a random graph) using `spmv_merge`, reorder the graph with `rcm` first, and
//!     measure iterations/s before and after; combine with parallel_sample_sort_radix (sort edges by (src,dst) to build CSR) for the loader.
use std::thread;
use std::time::Instant;

// ---------------------------------------------------------------- rng + dense oracle

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// uniform in 0..n (n > 0)
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    /// small signed value in -3..=3 so products stay tiny and sums exact
    fn val(&mut self) -> i64 {
        self.below(7) as i64 - 3
    }
}

type Dense = Vec<Vec<i64>>;

fn dense_mv(a: &Dense, x: &[i64]) -> Vec<i64> {
    a.iter()
        .map(|row| row.iter().zip(x).map(|(p, q)| p * q).sum())
        .collect()
}

fn dense_mm(a: &Dense, b: &Dense) -> Dense {
    let (n, k, m) = (a.len(), b.len(), b.first().map_or(0, |r| r.len()));
    let mut c = vec![vec![0i64; m]; n];
    for i in 0..n {
        for t in 0..k {
            for j in 0..m {
                c[i][j] += a[i][t] * b[t][j];
            }
        }
    }
    c
}

// ---------------------------------------------------------------- COO

#[derive(Clone, Debug)]
struct Coo {
    nrows: usize,
    ncols: usize,
    ent: Vec<(usize, usize, i64)>,
}

impl Coo {
    fn random(rng: &mut Rng, nrows: usize, ncols: usize, max_nnz: usize) -> Coo {
        let nnz = rng.below(max_nnz + 1);
        let ent = (0..nnz)
            .map(|_| (rng.below(nrows), rng.below(ncols), rng.val()))
            .collect();
        Coo { nrows, ncols, ent }
    }

    /// Duplicates are SUMMED (the convention used by scipy and most libraries).
    fn to_dense(&self) -> Dense {
        let mut d = vec![vec![0i64; self.ncols]; self.nrows];
        for &(r, c, v) in &self.ent {
            d[r][c] += v;
        }
        d
    }

    /// Scatter SpMV: no structure needed, but y is written at random places.
    fn spmv(&self, x: &[i64]) -> Vec<i64> {
        let mut y = vec![0i64; self.nrows];
        for &(r, c, v) in &self.ent {
            y[r] += v * x[c];
        }
        y
    }
}

// ---------------------------------------------------------------- CSR

#[derive(Clone, Debug, PartialEq)]
struct Csr {
    nrows: usize,
    ncols: usize,
    indptr: Vec<usize>,
    indices: Vec<usize>,
    data: Vec<i64>,
}

impl Csr {
    fn from_coo(c: &Coo) -> Csr {
        let mut e = c.ent.clone();
        e.sort_by_key(|&(r, col, _)| (r, col));
        // merge duplicates: after sorting they are adjacent
        let mut merged: Vec<(usize, usize, i64)> = Vec::with_capacity(e.len());
        for (r, col, v) in e {
            if let Some(last) = merged.last_mut() {
                if last.0 == r && last.1 == col {
                    last.2 += v;
                    continue;
                }
            }
            merged.push((r, col, v));
        }
        let mut indptr = vec![0usize; c.nrows + 1];
        for &(r, _, _) in &merged {
            indptr[r + 1] += 1;
        }
        for i in 0..c.nrows {
            indptr[i + 1] += indptr[i];
        }
        Csr {
            nrows: c.nrows,
            ncols: c.ncols,
            indptr,
            indices: merged.iter().map(|t| t.1).collect(),
            data: merged.iter().map(|t| t.2).collect(),
        }
    }

    fn nnz(&self) -> usize {
        self.data.len()
    }
    fn row_len(&self, i: usize) -> usize {
        self.indptr[i + 1] - self.indptr[i]
    }

    fn to_dense(&self) -> Dense {
        let mut d = vec![vec![0i64; self.ncols]; self.nrows];
        for i in 0..self.nrows {
            for k in self.indptr[i]..self.indptr[i + 1] {
                d[i][self.indices[k]] += self.data[k];
            }
        }
        d
    }

    fn row_dot(&self, i: usize, x: &[i64]) -> i64 {
        let (lo, hi) = (self.indptr[i], self.indptr[i + 1]);
        self.indices[lo..hi]
            .iter()
            .zip(&self.data[lo..hi])
            .map(|(&c, &v)| v * x[c])
            .sum()
    }

    fn spmv(&self, x: &[i64]) -> Vec<i64> {
        (0..self.nrows).map(|i| self.row_dot(i, x)).collect()
    }

    /// Counting-sort transpose, O(nnz + ncols). Output rows come out sorted because we sweep source rows in order.
    fn transpose(&self) -> Csr {
        let mut ptr = vec![0usize; self.ncols + 1];
        for &c in &self.indices {
            ptr[c + 1] += 1;
        }
        for i in 0..self.ncols {
            ptr[i + 1] += ptr[i];
        }
        let mut next = ptr.clone();
        let mut idx = vec![0usize; self.nnz()];
        let mut dat = vec![0i64; self.nnz()];
        for r in 0..self.nrows {
            for k in self.indptr[r]..self.indptr[r + 1] {
                let c = self.indices[k];
                let p = next[c];
                next[c] += 1;
                idx[p] = r;
                dat[p] = self.data[k];
            }
        }
        Csr {
            nrows: self.ncols,
            ncols: self.nrows,
            indptr: ptr,
            indices: idx,
            data: dat,
        }
    }

    /// Half-bandwidth: max |row - col| over stored entries. Small bandwidth = x gathers stay near the diagonal = cache hits.
    fn bandwidth(&self) -> usize {
        let mut bw = 0;
        for i in 0..self.nrows {
            for k in self.indptr[i]..self.indptr[i + 1] {
                bw = bw.max(i.abs_diff(self.indices[k]));
            }
        }
        bw
    }

    /// Symmetric permutation: old index i becomes p[i], i.e. B = P A P^T.
    fn permute(&self, p: &[usize]) -> Csr {
        let mut ent = Vec::with_capacity(self.nnz());
        for i in 0..self.nrows {
            for k in self.indptr[i]..self.indptr[i + 1] {
                ent.push((p[i], p[self.indices[k]], self.data[k]));
            }
        }
        Csr::from_coo(&Coo {
            nrows: self.nrows,
            ncols: self.ncols,
            ent,
        })
    }
}

// ---------------------------------------------------------------- CSC

struct Csc {
    nrows: usize,
    ncols: usize,
    colptr: Vec<usize>,
    rowidx: Vec<usize>,
    data: Vec<i64>,
}

impl Csc {
    fn from_csr(a: &Csr) -> Csc {
        // CSC of A is literally CSR of A^T with the field names changed
        let t = a.transpose();
        Csc {
            nrows: a.nrows,
            ncols: a.ncols,
            colptr: t.indptr,
            rowidx: t.indices,
            data: t.data,
        }
    }
    /// Column oriented: each x[c] is read once and reused for the whole column (good for x), y is scattered.
    fn spmv(&self, x: &[i64]) -> Vec<i64> {
        let mut y = vec![0i64; self.nrows];
        for c in 0..self.ncols {
            let xc = x[c];
            for k in self.colptr[c]..self.colptr[c + 1] {
                y[self.rowidx[k]] += self.data[k] * xc;
            }
        }
        y
    }
}

// ---------------------------------------------------------------- ELL

struct Ell {
    nrows: usize,
    width: usize,
    cols: Vec<usize>, // row-major nrows*width, padding = (col 0, val 0)
    vals: Vec<i64>,
}

impl Ell {
    fn from_csr(a: &Csr) -> Ell {
        let width = (0..a.nrows).map(|i| a.row_len(i)).max().unwrap_or(0);
        let mut cols = vec![0usize; a.nrows * width];
        let mut vals = vec![0i64; a.nrows * width];
        for i in 0..a.nrows {
            for (k, p) in (a.indptr[i]..a.indptr[i + 1]).enumerate() {
                cols[i * width + k] = a.indices[p];
                vals[i * width + k] = a.data[p];
            }
        }
        Ell {
            nrows: a.nrows,
            width,
            cols,
            vals,
        }
    }
    fn spmv(&self, x: &[i64]) -> Vec<i64> {
        let mut y = vec![0i64; self.nrows];
        for (i, out) in y.iter_mut().enumerate() {
            // padding multiplies 0 by x[0]: wasted work but branch-free, which is the point of ELL
            for k in 0..self.width {
                *out += self.vals[i * self.width + k] * x[self.cols[i * self.width + k]];
            }
        }
        y
    }
}

// ---------------------------------------------------------------- SELL-C-sigma

struct Sell {
    nrows: usize,
    c: usize,
    perm: Vec<usize>, // sorted position -> original row (usize::MAX for padding rows)
    chunk_ptr: Vec<usize>,
    chunk_w: Vec<usize>,
    cols: Vec<usize>,
    vals: Vec<i64>,
}

impl Sell {
    fn from_csr(a: &Csr, c: usize, sigma: usize) -> Sell {
        let n = a.nrows;
        let padded = n.div_ceil(c) * c;
        let mut perm: Vec<usize> = (0..padded)
            .map(|i| if i < n { i } else { usize::MAX })
            .collect();
        // sort by descending length inside each window of sigma rows: neighbours get similar lengths,
        // but rows never move more than sigma places, which keeps the permutation of y cache-friendly
        for w in perm.chunks_mut(sigma.max(c)) {
            w.sort_by_key(|&r| std::cmp::Reverse(if r == usize::MAX { 0 } else { a.row_len(r) }));
        }
        let mut chunk_ptr = vec![0usize];
        let mut chunk_w = Vec::new();
        let (mut cols, mut vals) = (Vec::new(), Vec::new());
        for ch in 0..padded / c {
            let rows = &perm[ch * c..(ch + 1) * c];
            let w = rows
                .iter()
                .map(|&r| if r == usize::MAX { 0 } else { a.row_len(r) })
                .max()
                .unwrap();
            for k in 0..w {
                for &r in rows {
                    // column-major inside the chunk: element k of all C rows is contiguous (one SIMD load)
                    if r != usize::MAX && k < a.row_len(r) {
                        cols.push(a.indices[a.indptr[r] + k]);
                        vals.push(a.data[a.indptr[r] + k]);
                    } else {
                        cols.push(0);
                        vals.push(0);
                    }
                }
            }
            chunk_w.push(w);
            chunk_ptr.push(vals.len());
        }
        Sell {
            nrows: n,
            c,
            perm,
            chunk_ptr,
            chunk_w,
            cols,
            vals,
        }
    }

    fn stored(&self) -> usize {
        self.vals.len()
    }

    fn spmv(&self, x: &[i64]) -> Vec<i64> {
        let mut y = vec![0i64; self.nrows];
        let c = self.c;
        let mut acc = vec![0i64; c];
        for ch in 0..self.chunk_w.len() {
            acc.iter_mut().for_each(|a| *a = 0);
            let base = self.chunk_ptr[ch];
            for k in 0..self.chunk_w[ch] {
                for r in 0..c {
                    let p = base + k * c + r;
                    acc[r] += self.vals[p] * x[self.cols[p]];
                }
            }
            for r in 0..c {
                let orig = self.perm[ch * c + r];
                if orig != usize::MAX {
                    y[orig] = acc[r];
                }
            }
        }
        y
    }
}

// ---------------------------------------------------------------- parallel SpMV

/// Row-parallel: equal ROW counts per thread. Writes never conflict because y is split into disjoint slices.
fn spmv_row_parallel(a: &Csr, x: &[i64], t: usize) -> Vec<i64> {
    let mut y = vec![0i64; a.nrows];
    let per = a.nrows.div_ceil(t).max(1);
    thread::scope(|s| {
        for (ci, chunk) in y.chunks_mut(per).enumerate() {
            s.spawn(move || {
                for (k, out) in chunk.iter_mut().enumerate() {
                    *out = a.row_dot(ci * per + k, x);
                }
            });
        }
    });
    y
}

/// Find where diagonal d crosses the merge of A = row_end_offsets (indptr[1..]) and B = 0..nnz (non-zero indices).
/// Returns (rows consumed, non-zeros consumed) with rows + nz == d.  Binary search: O(log(rows)).
fn merge_path_search(indptr: &[usize], nrows: usize, nnz: usize, d: usize) -> (usize, usize) {
    let (mut lo, mut hi) = (d.saturating_sub(nnz), d.min(nrows));
    while lo < hi {
        let mid = (lo + hi) / 2;
        // row `mid` ends at indptr[mid+1]; it is consumed before non-zero (d-mid-1) iff it ends at or before it
        if indptr[mid + 1] <= d - mid - 1 {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    (lo, d - lo)
}

/// Merge-path SpMV: every thread gets exactly ceil((nrows+nnz)/T) "items" regardless of row-length skew.
/// Threads return (row, partial_sum) pairs; rows split across thread boundaries are fixed up by adding partials.
/// (Production kernels write complete rows directly and only carry the two boundary rows out; adding everything
/// is the same arithmetic and keeps this version short.)
fn spmv_merge(a: &Csr, x: &[i64], t: usize) -> Vec<i64> {
    let total = a.nrows + a.nnz();
    let coords: Vec<(usize, usize)> = (0..=t)
        .map(|k| merge_path_search(&a.indptr, a.nrows, a.nnz(), k * total / t))
        .collect();
    let parts: Vec<Vec<(usize, i64)>> = thread::scope(|s| {
        let handles: Vec<_> = (0..t)
            .map(|k| {
                let (start, end) = (coords[k], coords[k + 1]);
                s.spawn(move || {
                    let mut out = Vec::new();
                    let (mut i, mut j) = start;
                    while i < end.0 {
                        let mut sum = 0;
                        while j < a.indptr[i + 1] {
                            sum += a.data[j] * x[a.indices[j]];
                            j += 1;
                        }
                        out.push((i, sum));
                        i += 1;
                    }
                    // trailing partial row: its remaining non-zeros belong to the next thread
                    let mut sum = 0;
                    while j < end.1 {
                        sum += a.data[j] * x[a.indices[j]];
                        j += 1;
                    }
                    if i < a.nrows {
                        out.push((i, sum));
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut y = vec![0i64; a.nrows];
    for (i, v) in parts.into_iter().flatten() {
        y[i] += v;
    }
    y
}

/// Max (rows + nnz) any thread processes under the naive equal-rows split.
fn row_split_max_work(a: &Csr, t: usize) -> usize {
    let per = a.nrows.div_ceil(t).max(1);
    (0..a.nrows)
        .step_by(per)
        .map(|lo| {
            let hi = (lo + per).min(a.nrows);
            (hi - lo) + (a.indptr[hi] - a.indptr[lo])
        })
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------- SpGEMM (Gustavson)

/// C = A * B, row by row. For row i of A, every a_ik scales row k of B into a dense accumulator.
/// `mark[c] == i` tells whether column c was already touched for this row, so we never clear the whole accumulator.
fn spgemm(a: &Csr, b: &Csr) -> Csr {
    assert_eq!(a.ncols, b.nrows);
    let mut acc = vec![0i64; b.ncols];
    let mut mark = vec![usize::MAX; b.ncols];
    let (mut indptr, mut indices, mut data) = (vec![0usize], Vec::new(), Vec::new());
    for i in 0..a.nrows {
        let mut cols: Vec<usize> = Vec::new();
        for k in a.indptr[i]..a.indptr[i + 1] {
            let (j, av) = (a.indices[k], a.data[k]);
            for p in b.indptr[j]..b.indptr[j + 1] {
                let c = b.indices[p];
                if mark[c] != i {
                    mark[c] = i;
                    acc[c] = 0;
                    cols.push(c);
                }
                acc[c] += av * b.data[p];
            }
        }
        cols.sort_unstable(); // keep CSR rows sorted; hash-based SpGEMMs skip this or sort lazily
        for c in cols {
            indices.push(c);
            data.push(acc[c]);
        }
        indptr.push(indices.len());
    }
    Csr {
        nrows: a.nrows,
        ncols: b.ncols,
        indptr,
        indices,
        data,
    }
}

// ---------------------------------------------------------------- SpTRSV with level scheduling

/// Random UNIT lower-triangular matrix: diagonal 1 (stored last in each row), a few entries strictly left of it.
fn random_unit_lower(rng: &mut Rng, n: usize, per_row: usize) -> Csr {
    let mut ent = Vec::new();
    for i in 0..n {
        ent.push((i, i, 1));
        if i > 0 {
            for _ in 0..rng.below(per_row + 1) {
                ent.push((i, rng.below(i), rng.val()));
            }
        }
    }
    Csr::from_coo(&Coo {
        nrows: n,
        ncols: n,
        ent,
    })
}

fn solve_row(l: &Csr, b: &[i64], x: &[i64], i: usize) -> i64 {
    let (lo, hi) = (l.indptr[i], l.indptr[i + 1] - 1); // last entry is the unit diagonal
    let mut s = b[i];
    for k in lo..hi {
        s = s.wrapping_sub(l.data[k].wrapping_mul(x[l.indices[k]]));
    }
    s
}

fn sptrsv_serial(l: &Csr, b: &[i64]) -> Vec<i64> {
    let mut x = vec![0i64; l.nrows];
    for i in 0..l.nrows {
        x[i] = solve_row(l, b, &x, i);
    }
    x
}

/// level[i] = 1 + max level of the rows it depends on. All rows of one level are mutually independent.
fn level_sets(l: &Csr) -> Vec<Vec<usize>> {
    let mut level = vec![0usize; l.nrows];
    let mut levels: Vec<Vec<usize>> = Vec::new();
    for i in 0..l.nrows {
        let hi = l.indptr[i + 1] - 1;
        level[i] = l.indices[l.indptr[i]..hi]
            .iter()
            .map(|&j| level[j] + 1)
            .max()
            .unwrap_or(0);
        if level[i] == levels.len() {
            levels.push(Vec::new());
        }
        levels[level[i]].push(i);
    }
    levels
}

fn sptrsv_levels(l: &Csr, b: &[i64], levels: &[Vec<usize>], t: usize) -> Vec<i64> {
    let mut x = vec![0i64; l.nrows];
    for rows in levels {
        if t > 1 && rows.len() >= 256 {
            let per = rows.len().div_ceil(t);
            let xr = &x;
            let results: Vec<Vec<(usize, i64)>> = thread::scope(|s| {
                let hs: Vec<_> = rows
                    .chunks(per)
                    .map(|chunk| {
                        s.spawn(move || {
                            chunk
                                .iter()
                                .map(|&i| (i, solve_row(l, b, xr, i)))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                hs.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for (i, v) in results.into_iter().flatten() {
                x[i] = v;
            }
        } else {
            for &i in rows {
                x[i] = solve_row(l, b, &x, i);
            }
        }
    }
    x
}

fn lower_mul_wrapping(l: &Csr, x: &[i64]) -> Vec<i64> {
    (0..l.nrows)
        .map(|i| {
            (l.indptr[i]..l.indptr[i + 1]).fold(0i64, |s, k| {
                s.wrapping_add(l.data[k].wrapping_mul(x[l.indices[k]]))
            })
        })
        .collect()
}

// ---------------------------------------------------------------- reverse Cuthill-McKee

/// Returns perm with perm[old] = new. BFS from a minimum-degree node, visiting neighbours by increasing degree,
/// then REVERSE the order (the "reverse" in RCM reduces fill-in for factorisations; bandwidth is the same either way).
/// A pseudo-peripheral start (George-Liu) is better; min-degree is the cheap stand-in used here.
fn rcm(a: &Csr) -> Vec<usize> {
    let n = a.nrows;
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for k in a.indptr[i]..a.indptr[i + 1] {
            let j = a.indices[k];
            if i != j {
                adj[i].push(j);
                adj[j].push(i); // symmetrise the pattern
            }
        }
    }
    for l in adj.iter_mut() {
        l.sort_unstable();
        l.dedup();
    }
    let deg: Vec<usize> = adj.iter().map(|l| l.len()).collect();
    let mut starts: Vec<usize> = (0..n).collect();
    starts.sort_by_key(|&i| (deg[i], i));
    let mut visited = vec![false; n];
    let mut order: Vec<usize> = Vec::with_capacity(n); // doubles as the BFS queue
    let mut head = 0;
    for &s in &starts {
        if visited[s] {
            continue; // another connected component: restart BFS
        }
        visited[s] = true;
        order.push(s);
        while head < order.len() {
            let u = order[head];
            head += 1;
            let mut nb: Vec<usize> = adj[u].iter().copied().filter(|&v| !visited[v]).collect();
            nb.sort_by_key(|&v| (deg[v], v));
            for v in nb {
                visited[v] = true;
                order.push(v);
            }
        }
    }
    order.reverse();
    let mut perm = vec![0usize; n];
    for (new, &old) in order.iter().enumerate() {
        perm[old] = new;
    }
    perm
}

fn random_perm(rng: &mut Rng, n: usize) -> Vec<usize> {
    let mut p: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        p.swap(i, rng.below(i + 1));
    }
    p
}

/// Symmetric random banded matrix with half-bandwidth `b`.
fn banded(rng: &mut Rng, n: usize, b: usize) -> Csr {
    let mut ent = Vec::new();
    for i in 0..n {
        ent.push((i, i, 1));
        for j in i + 1..(i + b + 1).min(n) {
            if rng.below(2) == 0 {
                let v = rng.val();
                ent.push((i, j, v));
                ent.push((j, i, v));
            }
        }
    }
    Csr::from_coo(&Coo {
        nrows: n,
        ncols: n,
        ent,
    })
}

// ---------------------------------------------------------------- tests

fn random_x(rng: &mut Rng, n: usize) -> Vec<i64> {
    (0..n).map(|_| rng.val()).collect()
}

fn test_formats_against_dense(rng: &mut Rng) {
    let mut seen_empty_row = false;
    let mut seen_dup = false;
    for _ in 0..1000 {
        let (nr, nc) = (1 + rng.below(12), 1 + rng.below(12));
        let coo = Coo::random(rng, nr, nc, 40);
        let dense = coo.to_dense();
        let x = random_x(rng, nc);
        let want = dense_mv(&dense, &x);

        let csr = Csr::from_coo(&coo);
        assert_eq!(
            csr.to_dense(),
            dense,
            "CSR round trip (duplicates must be summed)"
        );
        assert_eq!(
            csr.transpose().transpose(),
            csr,
            "transpose is an involution"
        );
        seen_empty_row |= (0..nr).any(|i| csr.row_len(i) == 0);
        seen_dup |= csr.nnz() < coo.ent.len();

        assert_eq!(coo.spmv(&x), want, "COO");
        assert_eq!(csr.spmv(&x), want, "CSR");
        assert_eq!(Csc::from_csr(&csr).spmv(&x), want, "CSC");
        assert_eq!(Ell::from_csr(&csr).spmv(&x), want, "ELL");
        for (c, sigma) in [(1, 1), (2, 1), (4, 8), (3, 7), (8, 64)] {
            assert_eq!(
                Sell::from_csr(&csr, c, sigma).spmv(&x),
                want,
                "SELL C={c} sigma={sigma}"
            );
        }
        for t in 1..=5 {
            assert_eq!(spmv_row_parallel(&csr, &x, t), want, "row-parallel T={t}");
            assert_eq!(spmv_merge(&csr, &x, t), want, "merge-path T={t}");
        }
        // SpGEMM against dense matrix product
        let m = 1 + rng.below(12);
        let b_coo = Coo::random(rng, nc, m, 40);
        let b = Csr::from_coo(&b_coo);
        assert_eq!(
            spgemm(&csr, &b).to_dense(),
            dense_mm(&dense, &b_coo.to_dense()),
            "SpGEMM"
        );
    }
    assert!(
        seen_empty_row && seen_dup,
        "the generator must exercise empty rows and COO duplicates"
    );
    println!(
        "1000 random matrices: COO/CSR/CSC/ELL/SELL/row-parallel/merge-path/SpGEMM all equal the dense oracle"
    );
}

fn test_merge_path_partition(rng: &mut Rng) {
    for _ in 0..500 {
        let n = 1 + rng.below(30);
        let csr = Csr::from_coo(&Coo::random(rng, n, n, 60));
        let total = csr.nrows + csr.nnz();
        let mut prev = (0, 0);
        for d in 0..=total {
            let (i, j) = merge_path_search(&csr.indptr, csr.nrows, csr.nnz(), d);
            assert_eq!(i + j, d);
            assert!(i >= prev.0 && j >= prev.1, "the path is monotone");
            // consumed rows must have ended at or before consumed non-zeros, unconsumed rows must end after
            if i > 0 {
                assert!(csr.indptr[i] <= j);
            }
            if i < csr.nrows && j > 0 {
                assert!(csr.indptr[i + 1] > j - 1);
            }
            prev = (i, j);
        }
    }
    println!("merge-path diagonals: monotone, consistent with row ends on 500 random matrices");
}

fn test_sptrsv(rng: &mut Rng) {
    for _ in 0..300 {
        let n = 1 + rng.below(16);
        let l = random_unit_lower(rng, n, 3);
        let b = random_x(rng, n);
        let x = sptrsv_serial(&l, &b);
        assert_eq!(lower_mul_wrapping(&l, &x), b, "L x == b");
        let levels = level_sets(&l);
        assert_eq!(levels.iter().map(|v| v.len()).sum::<usize>(), n);
        assert_eq!(
            sptrsv_levels(&l, &b, &levels, 4),
            x,
            "level scheduled == serial"
        );
    }
    // big enough to take the threaded path (levels with >= 256 rows)
    let n = 20_000;
    let l = random_unit_lower(rng, n, 3);
    let b = random_x(rng, n);
    let levels = level_sets(&l);
    let t0 = Instant::now();
    let xs = sptrsv_serial(&l, &b);
    let ser = t0.elapsed();
    let t0 = Instant::now();
    let xp = sptrsv_levels(&l, &b, &levels, 4);
    let par = t0.elapsed();
    assert_eq!(xs, xp);
    assert_eq!(lower_mul_wrapping(&l, &xp), b);
    let widest = levels.iter().map(|v| v.len()).max().unwrap();
    println!(
        "SpTRSV n={n}: {} levels (parallelism = rows/levels = {:.0}), widest {widest}; serial {ser:?}, level-parallel {par:?}",
        levels.len(),
        n as f64 / levels.len() as f64
    );
    assert!(
        levels.len() < n / 10,
        "random DAGs are shallow: that is why level scheduling pays"
    );
}

fn test_rcm(rng: &mut Rng) {
    // a path graph scrambled: RCM must recover bandwidth 1 exactly
    for _ in 0..50 {
        let n = 2 + rng.below(60);
        let path = banded_path(n);
        let p = random_perm(rng, n);
        let scrambled = path.permute(&p);
        let q = rcm(&scrambled);
        let fixed = scrambled.permute(&q);
        assert_eq!(fixed.bandwidth(), 1, "RCM recovers a path");
    }
    // random banded matrices: RCM should crush the scrambled bandwidth
    let (mut sum_scr, mut sum_rcm, mut sum_orig) = (0, 0, 0);
    for _ in 0..100 {
        let n = 40 + rng.below(60);
        let a = banded(rng, n, 4);
        let scrambled = a.permute(&random_perm(rng, n));
        let q = rcm(&scrambled);
        let r = scrambled.permute(&q);
        sum_orig += a.bandwidth();
        sum_scr += scrambled.bandwidth();
        sum_rcm += r.bandwidth();
        // reordering must not change the mathematics: (P A P^T)(P x) == P (A x)
        let x = random_x(rng, n);
        let mut px = vec![0; n];
        for i in 0..n {
            px[q[i]] = x[i];
        }
        let y = scrambled.spmv(&x);
        let yr = r.spmv(&px);
        for i in 0..n {
            assert_eq!(yr[q[i]], y[i]);
        }
    }
    println!(
        "RCM on 100 scrambled banded matrices: total bandwidth original {sum_orig}, scrambled {sum_scr}, after RCM {sum_rcm}"
    );
    assert!(
        sum_rcm * 3 < sum_scr,
        "RCM must remove most of the scrambling"
    );
}

fn banded_path(n: usize) -> Csr {
    let mut ent = Vec::new();
    for i in 0..n {
        ent.push((i, i, 1));
        if i + 1 < n {
            ent.push((i, i + 1, 1));
            ent.push((i + 1, i, 1));
        }
    }
    Csr::from_coo(&Coo {
        nrows: n,
        ncols: n,
        ent,
    })
}

// ---------------------------------------------------------------- demos

fn gflops(nnz: usize, reps: usize, secs: f64) -> f64 {
    2.0 * nnz as f64 * reps as f64 / secs / 1e9
}

fn time_spmv(label: &str, nnz: usize, reps: usize, mut f: impl FnMut() -> Vec<i64>, want: &[i64]) {
    let t0 = Instant::now();
    let mut last = Vec::new();
    for _ in 0..reps {
        last = f();
    }
    let s = t0.elapsed().as_secs_f64();
    assert_eq!(last, want);
    println!(
        "  {label:<22} {:>8.2} ms/spmv  {:>6.2} GFLOP/s",
        s * 1e3 / reps as f64,
        gflops(nnz, reps, s)
    );
}

fn demo_uniform(rng: &mut Rng) {
    let n = 200_000;
    let ent: Vec<_> = (0..n * 10)
        .map(|_| (rng.below(n), rng.below(n), rng.val()))
        .collect();
    let a = Csr::from_coo(&Coo {
        nrows: n,
        ncols: n,
        ent,
    });
    let x = random_x(rng, n);
    let want = a.spmv(&x);
    println!(
        "\nuniform random matrix: {n} rows, {} nnz (~{:.0} MB of CSR)",
        a.nnz(),
        (a.nnz() * 16) as f64 / 1e6
    );
    let sell = Sell::from_csr(&a, 8, 256);
    time_spmv("CSR serial", a.nnz(), 10, || a.spmv(&x), &want);
    time_spmv(
        "CSR row-parallel T=4",
        a.nnz(),
        10,
        || spmv_row_parallel(&a, &x, 4),
        &want,
    );
    time_spmv(
        "merge-path T=4",
        a.nnz(),
        10,
        || spmv_merge(&a, &x, 4),
        &want,
    );
    time_spmv("SELL-8-256 serial", a.nnz(), 10, || sell.spmv(&x), &want);
    println!(
        "  SELL padding overhead: stored {} vs nnz {} ({:.2}x)",
        sell.stored(),
        a.nnz(),
        sell.stored() as f64 / a.nnz() as f64
    );
}

fn demo_skewed(rng: &mut Rng) {
    // power-law-ish extreme: row 0 is completely dense, everything else has 2 entries
    let n = 50_000;
    let mut ent: Vec<(usize, usize, i64)> = (0..n).map(|c| (0, c, 1)).collect();
    for r in 1..n {
        ent.push((r, rng.below(n), rng.val()));
        ent.push((r, rng.below(n), rng.val()));
    }
    let a = Csr::from_coo(&Coo {
        nrows: n,
        ncols: n,
        ent,
    });
    let x = random_x(rng, n);
    let want = a.spmv(&x);
    let t = 4;
    let total = a.nrows + a.nnz();
    let merge_max = (0..t)
        .map(|k| {
            let lo = merge_path_search(&a.indptr, a.nrows, a.nnz(), k * total / t);
            let hi = merge_path_search(&a.indptr, a.nrows, a.nnz(), (k + 1) * total / t);
            (hi.0 - lo.0) + (hi.1 - lo.1)
        })
        .max()
        .unwrap();
    let row_max = row_split_max_work(&a, t);
    println!(
        "\nskewed matrix (one dense row): T={t}, ideal per-thread work {} items",
        total / t
    );
    println!(
        "  equal-rows split: busiest thread {row_max} items; merge-path: busiest {merge_max} items"
    );
    assert!(
        merge_max <= total.div_ceil(t),
        "merge path is perfectly balanced by construction"
    );
    assert!(
        row_max * 2 > merge_max * 3,
        "row split should be at least 1.5x worse on this matrix"
    );
    assert_eq!(spmv_merge(&a, &x, t), want);
    assert_eq!(spmv_row_parallel(&a, &x, t), want);

    let maxw = (0..n).map(|i| a.row_len(i)).max().unwrap();
    let ell_stored = n * maxw; // do not materialise: it would be 2.5e9 entries
    let sell = Sell::from_csr(&a, 8, 4096);
    assert_eq!(sell.spmv(&x), want);
    println!(
        "  storage: nnz {} | ELL would store {ell_stored} | SELL-8-4096 stores {}",
        a.nnz(),
        sell.stored()
    );
    assert!(
        sell.stored() * 100 < ell_stored,
        "chunk-local padding avoids the global-maximum blow-up"
    );
}

fn main() {
    let mut rng = Rng::new(884);
    test_formats_against_dense(&mut rng);
    test_merge_path_partition(&mut rng);
    test_sptrsv(&mut rng);
    test_rcm(&mut rng);
    demo_uniform(&mut rng);
    demo_skewed(&mut rng);
    println!("\nall sparse-kernel checks passed");
}
