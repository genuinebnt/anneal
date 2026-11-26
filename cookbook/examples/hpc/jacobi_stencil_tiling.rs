//! RECIPE 881 (hpc): 2-D Jacobi/heat stencil: naive, tiled, rayon row-parallel, halo exchange with threads and channels
//! LEVEL: advanced
//! Use in: PDE solvers (heat, Poisson), image filters, cellular automata, any nearest-neighbour grid update.   Run: cargo run --release --example jacobi_stencil_tiling
//! COMMON USES: weather/CFD/finite-difference codes, Gaussian blur and convolution kernels, Game of Life, multigrid smoothers;
//! domain decomposition with ghost cells is how MPI codes scale a grid across nodes.
//! Course: CMU 15-418/618 (the grid solver / "ocean" example and message-passing decomposition).
#![allow(dead_code)]
//!
//! One Jacobi sweep: new[i][j] = 0.25 * (up + down + left + right) for interior points; the boundary is fixed (Dirichlet).
//! Because Jacobi reads ONLY the old grid and writes ONLY the new grid, every point is independent: any visiting order
//! (row-major, tiled, parallel) yields bit-identical floats as long as each point evaluates the same expression in the same
//! order. That is why this file can assert EXACT equality (not tolerance) between variants.
//!
//! Variants:
//!   - naive: row-major double loop (reads 5 values, writes 1 per point; neighbours reused from cache if a few rows fit).
//!   - tiled: loop blocking over (bi x bj) tiles. For a grid wider than cache, row-major reuse of row i-1 and i+1 only
//!     works if 3 full rows fit in cache; tiling shrinks the working set to 3*bj values. (Honest note: for a plain 5-point
//!     stencil on modern CPUs the hardware prefetcher already makes naive hard to beat; tiling pays off mostly for 3-D stencils
//!     and, combined with temporal blocking, for multi-sweep reuse.)
//!   - temporal blocking (described, not implemented): do T sweeps on a tile before moving on, shrinking the valid region by
//!     one cell per sweep (a trapezoid / "overlapped tiling"; or diamond/wavefront schemes). Cuts memory traffic by ~T since
//!     the tile stays in cache for T updates. The redundant halo recompute is the price.
//!   - rayon: split the OUTPUT into row bands with `par_chunks_mut`; the borrow checker proves bands are disjoint.
//!   - halo exchange: each "rank" (thread) owns a strip of rows plus 2 ghost rows. Every step: compute, then send my edge
//!     rows to the neighbours over channels and receive theirs into the ghosts. This is exactly the MPI pattern, with
//!     mpsc channels as the network. Convergence uses a reduction (max |delta|) to a coordinator, like MPI_Allreduce.
//!
//! Verification: blocked == naive and parallel == serial bit-for-bit on random grids with awkward sizes; halo result == the
//! global grid after K sweeps; the number of messages equals 2*(P-1) per step; the convergence step count is the same for
//! serial and distributed runs. Timings and GB/s (ideal 16 B/point = 1 read stream + 1 write stream) are printed only.
//! Read: https://en.wikipedia.org/wiki/Stencil_code  and  https://en.wikipedia.org/wiki/Jacobi_method
//! Read: Algorithmica HPC book, cache locality chapters https://en.algorithmica.org/hpc/cpu-cache/
//!
//! TRY NEXT:
//!   - [S] Change `sweep_naive` to Gauss-Seidel (update in place). Predict: do tiled and naive still agree bit-for-bit? Do rayon bands? Test it.
//!   - [S] Sweep the tile width `bj` over 8..4096 on a 4096x4096 grid and print time per sweep; where is the knee, and does it match your L1/L2 size?
//!   - [M] Implement the 2-sweep temporal block described above for one tile (shrinking valid region) and assert it equals two `sweep_naive` calls.
//!   - [M] Port `sweep_naive` to a 3x3 box blur on an image buffer (u8 pixels, clamp edges) and reuse `halo_run`'s strip decomposition to blur a large image in parallel.
//!   - [A] PROJECT: combine with `mpi_collectives_simulation` (recipe 883): replace the coordinator's max-reduction in `halo_run` with the allreduce from that recipe and count messages; and with `scaling_laws_amdahl_gustafson_roofline` (882) to plot strong-scaling efficiency of the halo version.

use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Instant;

// ---------------------------------------------------------------- rng
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
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---------------------------------------------------------------- grid
/// Flat row-major grid: index = i * m + j. A flat Vec (not Vec<Vec<f64>>) keeps rows contiguous and the stencil prefetchable.
#[derive(Clone, PartialEq, Debug)]
struct Grid {
    n: usize, // rows
    m: usize, // cols
    v: Vec<f64>,
}

impl Grid {
    fn random(n: usize, m: usize, rng: &mut Rng) -> Grid {
        Grid {
            n,
            m,
            v: (0..n * m).map(|_| rng.f64() * 100.0).collect(),
        }
    }
    /// Classic heat problem: top edge hot, everything else cold.
    fn heat(n: usize, m: usize) -> Grid {
        let mut v = vec![0.0; n * m];
        v[..m].fill(100.0);
        Grid { n, m, v }
    }
    fn bits_equal(&self, o: &Grid) -> bool {
        self.n == o.n
            && self.m == o.m
            && self
                .v
                .iter()
                .zip(&o.v)
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

/// THE point update. Every variant calls this exact expression so floating-point results match bit-for-bit.
/// (Different association, e.g. (up+down)+(left+right), would still be deterministic but would differ from this one.)
#[inline(always)]
fn point(up: f64, down: f64, left: f64, right: f64) -> f64 {
    0.25 * (up + down + left + right)
}

// ---------------------------------------------------------------- serial variants
/// One sweep, row-major. `nxt` must already hold the same boundary as `cur` (we only write the interior).
/// Returns max |new - old| for the convergence test (max is order independent, hence exactly reproducible).
fn sweep_naive(cur: &Grid, nxt: &mut Grid) -> f64 {
    let (n, m) = (cur.n, cur.m);
    let mut diff: f64 = 0.0;
    for i in 1..n - 1 {
        for j in 1..m - 1 {
            let k = i * m + j;
            let x = point(cur.v[k - m], cur.v[k + m], cur.v[k - 1], cur.v[k + 1]);
            diff = diff.max((x - cur.v[k]).abs());
            nxt.v[k] = x;
        }
    }
    diff
}

/// Same sweep, visiting the interior in (bi x bj) tiles. Tiles at the far edges are clipped.
fn sweep_tiled(cur: &Grid, nxt: &mut Grid, bi: usize, bj: usize) -> f64 {
    let (n, m) = (cur.n, cur.m);
    let mut diff: f64 = 0.0;
    for ib in (1..n - 1).step_by(bi) {
        let iend = (ib + bi).min(n - 1);
        for jb in (1..m - 1).step_by(bj) {
            let jend = (jb + bj).min(m - 1);
            for i in ib..iend {
                for j in jb..jend {
                    let k = i * m + j;
                    let x = point(cur.v[k - m], cur.v[k + m], cur.v[k - 1], cur.v[k + 1]);
                    diff = diff.max((x - cur.v[k]).abs());
                    nxt.v[k] = x;
                }
            }
        }
    }
    diff
}

// ---------------------------------------------------------------- rayon
/// Output split into bands of rows; each rayon task owns one band of `nxt` (disjoint &mut) and shares &cur. No locks, no unsafe.
fn sweep_rayon(cur: &Grid, nxt: &mut Grid) -> f64 {
    let (n, m) = (cur.n, cur.m);
    const BAND: usize = 16; // rows per task: big enough to amortise scheduling, small enough to balance
    nxt.v
        .par_chunks_mut(m * BAND)
        .enumerate()
        .map(|(b, band)| {
            let mut diff: f64 = 0.0;
            for (r, row) in band.chunks_mut(m).enumerate() {
                let i = b * BAND + r;
                if i == 0 || i == n - 1 {
                    continue; // fixed boundary rows
                }
                for j in 1..m - 1 {
                    let k = i * m + j;
                    let x = point(cur.v[k - m], cur.v[k + m], cur.v[k - 1], cur.v[k + 1]);
                    diff = diff.max((x - cur.v[k]).abs());
                    row[j] = x;
                }
            }
            diff
        })
        .reduce(|| 0.0, f64::max)
}

type Sweep = dyn Fn(&Grid, &mut Grid) -> f64;

/// Run exactly `steps` sweeps with ping-pong buffers.
fn run_steps(g: &Grid, steps: usize, sweep: &Sweep) -> Grid {
    let mut cur = g.clone();
    let mut nxt = g.clone(); // clone => boundary already correct in both buffers
    for _ in 0..steps {
        sweep(&cur, &mut nxt);
        std::mem::swap(&mut cur, &mut nxt);
    }
    cur
}

/// Run until max |delta| < tol (or max_steps). Returns (grid, steps taken).
fn run_to_convergence(g: &Grid, tol: f64, max_steps: usize, sweep: &Sweep) -> (Grid, usize) {
    let mut cur = g.clone();
    let mut nxt = g.clone();
    for step in 1..=max_steps {
        let d = sweep(&cur, &mut nxt);
        std::mem::swap(&mut cur, &mut nxt);
        if d < tol {
            return (cur, step);
        }
    }
    (cur, max_steps)
}

// ---------------------------------------------------------------- halo exchange
/// Messages between ranks. A row travels as an owned Vec: "network" = copy.
type Row = Vec<f64>;

struct RankLinks {
    from_above: Option<Receiver<Row>>, // neighbour r-1 sends me its last owned row
    from_below: Option<Receiver<Row>>, // neighbour r+1 sends me its first owned row
    to_above: Option<Sender<Row>>,     // my first owned row -> r-1's bottom ghost
    to_below: Option<Sender<Row>>,     // my last owned row -> r+1's top ghost
    diff_tx: Sender<f64>,              // local max delta -> coordinator
    go_rx: Receiver<bool>,             // coordinator: keep going?
}

/// Split interior rows 1..n-1 into `p` contiguous strips (sizes differ by at most 1).
fn strips(n: usize, p: usize) -> Vec<(usize, usize)> {
    let interior = n - 2;
    let (base, extra) = (interior / p, interior % p);
    let mut out = Vec::new();
    let mut lo = 1;
    for r in 0..p {
        let len = base + (r < extra) as usize;
        out.push((lo, lo + len));
        lo += len;
    }
    out
}

/// Distributed Jacobi. `mode`: Some(steps) = fixed number of steps; None = until tol. Returns (grid, steps, messages sent).
/// Each rank only ever touches its own local array of (owned + 2 ghost) rows: no shared grid at all.
fn halo_run(
    g: &Grid,
    p: usize,
    fixed_steps: Option<usize>,
    tol: f64,
    max_steps: usize,
) -> (Grid, usize, usize) {
    let (n, m) = (g.n, g.m);
    let ranges = strips(n, p);
    assert!(
        ranges.iter().all(|&(a, b)| b > a),
        "need at least one interior row per rank"
    );
    let msgs = AtomicUsize::new(0);

    // channel plumbing: for each adjacent pair (r, r+1) two channels, one per direction
    let mut links: Vec<RankLinks> = Vec::new();
    let (diff_tx, diff_rx) = channel::<f64>();
    let mut go_txs = Vec::new();
    let mut down_tx: Vec<Option<Sender<Row>>> = (0..p).map(|_| None).collect(); // r -> r+1
    let mut down_rx: Vec<Option<Receiver<Row>>> = (0..p).map(|_| None).collect();
    let mut up_tx: Vec<Option<Sender<Row>>> = (0..p).map(|_| None).collect(); // r -> r-1
    let mut up_rx: Vec<Option<Receiver<Row>>> = (0..p).map(|_| None).collect();
    for r in 0..p.saturating_sub(1) {
        let (t, rx) = channel();
        down_tx[r] = Some(t);
        down_rx[r + 1] = Some(rx);
        let (t, rx) = channel();
        up_tx[r + 1] = Some(t);
        up_rx[r] = Some(rx);
    }
    for r in 0..p {
        let (go_tx, go_rx) = channel();
        go_txs.push(go_tx);
        links.push(RankLinks {
            from_above: down_rx[r].take(),
            from_below: up_rx[r].take(),
            to_above: up_tx[r].take(),
            to_below: down_tx[r].take(),
            diff_tx: diff_tx.clone(),
            go_rx,
        });
    }
    drop(diff_tx);

    let mut steps_taken = 0;
    let owned: Vec<(usize, Vec<f64>)> = thread::scope(|s| {
        let handles: Vec<_> = links
            .into_iter()
            .zip(&ranges)
            .map(|(lk, &(lo, hi))| {
                let msgs = &msgs;
                s.spawn(move || {
                    let cnt = hi - lo;
                    // local rows: 0 = ghost(lo-1), 1..=cnt owned, cnt+1 = ghost(hi). Edge ranks' ghosts are the real fixed boundary.
                    let mut cur: Vec<f64> = g.v[(lo - 1) * m..(hi + 1) * m].to_vec();
                    let mut nxt = cur.clone();
                    let mut step = 0usize;
                    loop {
                        // compute owned rows from cur into nxt (ghost rows of cur are up to date)
                        let mut d: f64 = 0.0;
                        for li in 1..=cnt {
                            for j in 1..m - 1 {
                                let k = li * m + j;
                                let x = point(cur[k - m], cur[k + m], cur[k - 1], cur[k + 1]);
                                d = d.max((x - cur[k]).abs());
                                nxt[k] = x;
                            }
                        }
                        std::mem::swap(&mut cur, &mut nxt);
                        step += 1;
                        // halo exchange: SEND first (channels are unbounded so this cannot deadlock), then RECEIVE
                        if let Some(tx) = &lk.to_above {
                            tx.send(cur[m..2 * m].to_vec()).unwrap();
                            msgs.fetch_add(1, Ordering::Relaxed);
                        }
                        if let Some(tx) = &lk.to_below {
                            tx.send(cur[cnt * m..(cnt + 1) * m].to_vec()).unwrap();
                            msgs.fetch_add(1, Ordering::Relaxed);
                        }
                        if let Some(rx) = &lk.from_above {
                            cur[..m].copy_from_slice(&rx.recv().unwrap());
                        }
                        if let Some(rx) = &lk.from_below {
                            cur[(cnt + 1) * m..(cnt + 2) * m].copy_from_slice(&rx.recv().unwrap());
                        }
                        // termination: fixed step count is known locally; convergence needs a global max (allreduce-lite)
                        let more = match fixed_steps {
                            Some(k) => step < k,
                            None => {
                                lk.diff_tx.send(d).unwrap();
                                lk.go_rx.recv().unwrap()
                            }
                        };
                        if !more {
                            break;
                        }
                    }
                    (lo, cur[m..(cnt + 1) * m].to_vec())
                })
            })
            .collect();
        // coordinator (this thread): reduce max-diff, broadcast verdict
        if fixed_steps.is_none() {
            loop {
                let mut gd: f64 = 0.0;
                for _ in 0..p {
                    gd = gd.max(diff_rx.recv().unwrap());
                }
                steps_taken += 1;
                let more = !(gd < tol) && steps_taken < max_steps;
                for t in &go_txs {
                    t.send(more).unwrap();
                }
                if !more {
                    break;
                }
            }
        } else {
            steps_taken = fixed_steps.unwrap();
        }
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let mut out = g.clone();
    for (lo, rows) in owned {
        out.v[lo * m..lo * m + rows.len()].copy_from_slice(&rows);
    }
    (out, steps_taken, msgs.load(Ordering::Relaxed))
}

// ---------------------------------------------------------------- verification
fn check_tiling_and_rayon(rng: &mut Rng) {
    for case in 0..300 {
        let n = 3 + rng.below(40) as usize;
        let m = 3 + rng.below(40) as usize;
        let g = Grid::random(n, m, rng);
        let steps = 1 + rng.below(4) as usize;
        let want = run_steps(&g, steps, &|a, b| sweep_naive(a, b));
        let (bi, bj) = (1 + rng.below(9) as usize, 1 + rng.below(9) as usize);
        let tiled = run_steps(&g, steps, &move |a, b| sweep_tiled(a, b, bi, bj));
        assert!(
            tiled.bits_equal(&want),
            "tiled != naive (case {case}: {n}x{m} tile {bi}x{bj})"
        );
        let par = run_steps(&g, steps, &|a, b| sweep_rayon(a, b));
        assert!(par.bits_equal(&want), "rayon != serial (case {case})");
        // boundary must never change
        for j in 0..m {
            assert_eq!(want.v[j].to_bits(), g.v[j].to_bits());
            assert_eq!(
                want.v[(n - 1) * m + j].to_bits(),
                g.v[(n - 1) * m + j].to_bits()
            );
        }
        // max delta equal across variants
        let mut a = g.clone();
        let mut b = g.clone();
        let mut c = g.clone();
        let d0 = sweep_naive(&g, &mut a);
        assert_eq!(d0.to_bits(), sweep_tiled(&g, &mut b, bi, bj).to_bits());
        assert_eq!(d0.to_bits(), sweep_rayon(&g, &mut c).to_bits());
    }
    println!(
        "tiled == naive and rayon == serial, bit-exact, on 300 random grids (3..42 per side, random tiles, 1-4 sweeps)"
    );
}

fn check_halo(rng: &mut Rng) {
    let mut checked = 0;
    for _ in 0..60 {
        let p = 1 + rng.below(6) as usize;
        let n = 2 + p + rng.below(20) as usize; // interior rows >= p
        let m = 3 + rng.below(20) as usize;
        let g = Grid::random(n, m, rng);
        let steps = 1 + rng.below(6) as usize;
        let want = run_steps(&g, steps, &|a, b| sweep_naive(a, b));
        let (got, taken, msgs) = halo_run(&g, p, Some(steps), 0.0, 0);
        assert!(
            got.bits_equal(&want),
            "halo != global (p={p}, {n}x{m}, {steps} steps)"
        );
        assert_eq!(taken, steps);
        // each step every adjacent pair exchanges one row in each direction: 2*(p-1) messages
        assert_eq!(msgs, steps * 2 * (p - 1), "message count");
        checked += 1;
    }
    println!(
        "halo exchange == global grid, bit-exact, on {checked} random decompositions; messages == steps*2*(P-1)"
    );
}

fn check_convergence() {
    let g = Grid::heat(24, 24);
    let tol = 1e-3;
    let (s_grid, s_steps) = run_to_convergence(&g, tol, 100_000, &|a, b| sweep_naive(a, b));
    let (r_grid, r_steps) = run_to_convergence(&g, tol, 100_000, &|a, b| sweep_rayon(a, b));
    assert_eq!(s_steps, r_steps);
    assert!(s_grid.bits_equal(&r_grid));
    for p in [1, 2, 3, 5] {
        let (h_grid, h_steps, _) = halo_run(&g, p, None, tol, 100_000);
        assert_eq!(
            h_steps, s_steps,
            "distributed convergence step count (p={p})"
        );
        assert!(h_grid.bits_equal(&s_grid), "distributed result (p={p})");
    }
    // physical sanity: maximum principle, all values within boundary extremes; interior near top is hotter than bottom
    assert!(s_grid.v.iter().all(|&x| (0.0..=100.0).contains(&x)));
    assert!(s_grid.v[24 + 12] > s_grid.v[21 * 24 + 12]);
    println!(
        "convergence: tol {tol:e} reached in {s_steps} sweeps; serial, rayon and halo(P=1,2,3,5) agree bit-exactly"
    );
}

// ---------------------------------------------------------------- performance (printed only)
fn gbps(points: usize, sweeps: usize, secs: f64) -> f64 {
    (points * sweeps * 16) as f64 / secs / 1e9
}

fn bench() {
    let (n, m) = (2048, 2048);
    let sweeps = 20;
    let g = Grid::heat(n, m);
    let pts = (n - 2) * (m - 2);
    println!(
        "\nperformance, {n}x{m} grid, {sweeps} sweeps (printed only; ideal traffic 16 B/point):"
    );
    let t = Instant::now();
    let want = run_steps(&g, sweeps, &|a, b| sweep_naive(a, b));
    let tn = t.elapsed().as_secs_f64();
    println!(
        "  naive           {:>8.1} ms  {:>6.2} GB/s",
        tn * 1e3,
        gbps(pts, sweeps, tn)
    );
    for (bi, bj) in [(16, 256), (64, 512), (8, 2046)] {
        let t = Instant::now();
        let got = run_steps(&g, sweeps, &move |a, b| sweep_tiled(a, b, bi, bj));
        let tt = t.elapsed().as_secs_f64();
        assert!(got.bits_equal(&want));
        println!(
            "  tiled {bi:>3}x{bj:<4} {:>8.1} ms  {:>6.2} GB/s  speedup {:.2}x",
            tt * 1e3,
            gbps(pts, sweeps, tt),
            tn / tt
        );
    }
    let t = Instant::now();
    let got = run_steps(&g, sweeps, &|a, b| sweep_rayon(a, b));
    let tr = t.elapsed().as_secs_f64();
    assert!(got.bits_equal(&want));
    println!(
        "  rayon ({} thr)  {:>8.1} ms  {:>6.2} GB/s  speedup {:.2}x",
        rayon::current_num_threads(),
        tr * 1e3,
        gbps(pts, sweeps, tr),
        tn / tr
    );
    for p in [2, 4] {
        let t = Instant::now();
        let (got, _, msgs) = halo_run(&g, p, Some(sweeps), 0.0, 0);
        let th = t.elapsed().as_secs_f64();
        assert!(got.bits_equal(&want));
        println!(
            "  halo P={p}        {:>8.1} ms  {:>6.2} GB/s  speedup {:.2}x  ({msgs} row messages)",
            th * 1e3,
            gbps(pts, sweeps, th),
            tn / th
        );
    }
}

fn demo() {
    let g = Grid::heat(8, 8);
    let r = run_steps(&g, 40, &|a, b| sweep_naive(a, b));
    println!("8x8 plate, top edge 100, after 40 sweeps (middle column from top):");
    let col: Vec<String> = (0..8).map(|i| format!("{:.1}", r.v[i * 8 + 4])).collect();
    println!("  {}", col.join("  "));
}

fn main() {
    demo();
    let mut rng = Rng(0x0bad_cafe_1234_5679);
    check_tiling_and_rayon(&mut rng);
    check_halo(&mut rng);
    check_convergence();
    bench();
    println!("\nall stencil checks passed");
}
