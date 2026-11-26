//! RECIPE 880 (hpc): parallel prefix scan and reduction: Hillis-Steele, Blelloch, blocked 3-phase, segmented, compaction
//! LEVEL: advanced
//! Use in: GPU/CPU data-parallel kernels, radix sort offsets, stream compaction, parallel parsing, deterministic sums.   Run: cargo run --release --example parallel_prefix_scan_reduction
//! COMMON USES: radix sort histograms -> offsets, filter/compaction in columnar engines, parallel tokenizers (segmented scan),
//! reproducible reductions (fixed reduction trees) in numerical libraries and ML frameworks, GPU scan primitives (CUB/Thrust style).
//! Course: CMU 15-418/618 (parallel computer architecture and programming), lectures on data-parallel primitives; Blelloch 1990/1993.
#![allow(dead_code)]
//!
//! A scan (prefix sum) looks inherently sequential (out[i] depends on out[i-1]) but any ASSOCIATIVE operator can be
//! re-bracketed, which exposes parallelism. This file implements, with std threads only:
//!   1. Hillis-Steele scan: log2(n) rounds, each element combines with the one `d` to its left. Depth O(log n) but WORK
//!      O(n log n): wasteful when cores < n (the usual CPU case).
//!   2. Blelloch scan: up-sweep (reduce tree) + down-sweep. Work O(n), depth 2 log n. We COUNT operator applications and
//!      assert the work gap (HS ~ n log n, Blelloch ~ 2n).
//!   3. Blocked three-phase scan (what you actually write on a CPU): per-block local reduce, scan the few block sums, per-block
//!      local scan with an offset. 2 passes over memory, work ~2n, only `threads` blocks.
//!   4. Segmented scan: restart the scan at flagged elements; trick = lift to pairs (flag, value) with an associative operator,
//!      then reuse ANY scan unchanged.
//!   5. Reductions: float addition is NOT associative, so a reduction whose bracketing depends on the thread count changes
//!      its answer with the thread count. Fix = fixed-shape tree (fixed leaf size, pairwise combine) independent of threads.
//!   6. Stream compaction: flags -> exclusive scan -> scatter. The scan result IS each survivor's output index.
//!
//! Verification: 2000 seeded random inputs; integers (wrapping add, also max and a non-commutative operator) must match
//! the sequential scan exactly; floats must match within tolerance; the fixed-tree float reduction must be BIT-identical for
//! every thread count tested. Timings are printed, never asserted.
//! Read: Blelloch, "Prefix Sums and Their Applications" https://www.cs.cmu.edu/~guyb/papers/Ble93.pdf
//! Read: GPU Gems 3 ch.39 https://developer.nvidia.com/gpugems/gpugems3/part-vi-gpu-computing/chapter-39-parallel-prefix-sum-scan-cuda
//! Read: Algorithmica, "Prefix sum with SIMD" https://en.algorithmica.org/hpc/algorithms/prefix/   and   https://en.wikipedia.org/wiki/Prefix_sum
//!
//! TRY NEXT:
//!   - [S] `hillis_steele` allocates a new Vec per round; rewrite it with two buffers swapped each round and count allocations.
//!   - [S] Make `blocked_scan` use `threads = 3` on n = 7: predict which chunks are empty, then check `chunk_len` handles it.
//!   - [M] Predict then measure: at what n does `blocked_scan` beat `seq_scan` on your machine, and how does it change with 2 vs 8 threads? (memory-bound: why does it saturate?)
//!   - [M] Port `compact` to a real task: filter log lines whose length > 80 into a packed Vec, in parallel, preserving order, using only the scan to place outputs.
//!   - [A] PROJECT: combine with `parallel_sample_sort_radix` (recipe 885): use `blocked_scan` on per-thread histograms to compute radix scatter offsets, and with `scaling_laws_amdahl_gustafson_roofline` to fit the serial fraction of the three-phase scan.

use std::cell::Cell;
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

// ---------------------------------------------------------------- sequential oracle
/// Inclusive scan: out[i] = x[0] op ... op x[i].
fn seq_scan<T: Copy>(x: &[T], op: impl Fn(T, T) -> T) -> Vec<T> {
    let mut out = Vec::with_capacity(x.len());
    let mut acc: Option<T> = None;
    for &v in x {
        let nv = match acc {
            None => v,
            Some(a) => op(a, v),
        };
        out.push(nv);
        acc = Some(nv);
    }
    out
}

// ---------------------------------------------------------------- 1. Hillis-Steele
/// Round d=1,2,4,..: next[i] = cur[i-d] op cur[i] (for i >= d). Each round is perfectly parallel (every i independent);
/// we run it serially but count operator applications to show the O(n log n) work.
/// Correct for non-commutative ops because the left operand is always the earlier element.
fn hillis_steele<T: Copy>(x: &[T], op: impl Fn(T, T) -> T) -> Vec<T> {
    let n = x.len();
    let mut cur = x.to_vec();
    let mut d = 1;
    while d < n {
        // reads only `cur`, writes `next`: that double buffer is what makes the round data-parallel
        let next: Vec<T> = (0..n)
            .map(|i| {
                if i >= d {
                    op(cur[i - d], cur[i])
                } else {
                    cur[i]
                }
            })
            .collect();
        cur = next;
        d *= 2;
    }
    cur
}

// ---------------------------------------------------------------- 2. Blelloch
/// Work-efficient EXCLUSIVE scan on a power-of-two padded copy. `id` must be the identity of `op`.
/// Up-sweep builds partial sums in place (a reduction tree), then the root is zeroed and the down-sweep pushes prefixes
/// down: left child gets parent's prefix, right child gets prefix op left-subtree-sum.
fn blelloch_exclusive<T: Copy>(x: &[T], id: T, op: impl Fn(T, T) -> T) -> Vec<T> {
    let n = x.len();
    if n == 0 {
        return vec![];
    }
    let m = n.next_power_of_two();
    let mut a = x.to_vec();
    a.resize(m, id); // padding with the identity does not change any real prefix
    let mut d = 1;
    while d < m {
        // up-sweep: nodes at stride 2d combine their two halves
        let mut i = 2 * d - 1;
        while i < m {
            a[i] = op(a[i - d], a[i]);
            i += 2 * d;
        }
        d *= 2;
    }
    a[m - 1] = id;
    let mut d = m / 2;
    while d >= 1 {
        let mut i = 2 * d - 1;
        while i < m {
            let left = a[i - d];
            a[i - d] = a[i]; // left child inherits the parent prefix
            a[i] = op(a[i], left); // right child = parent prefix op left sum (order matters for non-commutative ops)
            i += 2 * d;
        }
        d /= 2;
    }
    a.truncate(n);
    a
}

/// Inclusive = exclusive op x[i].
fn blelloch_inclusive<T: Copy>(x: &[T], id: T, op: impl Fn(T, T) -> T + Copy) -> Vec<T> {
    let ex = blelloch_exclusive(x, id, op);
    ex.iter().zip(x).map(|(&e, &v)| op(e, v)).collect()
}

// ---------------------------------------------------------------- 3. blocked three-phase scan
fn chunk_len(n: usize, threads: usize) -> usize {
    n.div_ceil(threads.max(1)).max(1)
}

/// Phase 1: each thread reduces its chunk. Phase 2: scan the (few) chunk totals serially. Phase 3: each thread rescans
/// its chunk seeded with the offset of everything before it. Total work ~2n, which is why this beats Hillis-Steele on CPUs.
fn blocked_scan<T>(x: &[T], id: T, op: &(impl Fn(T, T) -> T + Sync), threads: usize) -> Vec<T>
where
    T: Copy + Send + Sync,
{
    let n = x.len();
    if n == 0 {
        return vec![];
    }
    let cl = chunk_len(n, threads);
    // phase 1: local reductions
    let sums: Vec<T> = thread::scope(|s| {
        let hs: Vec<_> = x
            .chunks(cl)
            .map(|c| s.spawn(move || c.iter().fold(id, |a, &v| op(a, v))))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    // phase 2: exclusive scan over chunk totals (length = threads, negligible)
    let mut offsets = Vec::with_capacity(sums.len());
    let mut acc = id;
    for &s in &sums {
        offsets.push(acc);
        acc = op(acc, s);
    }
    // phase 3: seeded local scans, each thread writes a disjoint chunk of `out`
    let mut out = vec![id; n];
    thread::scope(|s| {
        for ((oc, xc), &off) in out.chunks_mut(cl).zip(x.chunks(cl)).zip(&offsets) {
            s.spawn(move || {
                let mut a = off;
                for (o, &v) in oc.iter_mut().zip(xc) {
                    a = op(a, v);
                    *o = a;
                }
            });
        }
    });
    out
}

// ---------------------------------------------------------------- 4. segmented scan
/// Lift (flag, value) pairs. flag=true starts a new segment. The operator
///   (f1,v1) (+) (f2,v2) = (f1 || f2, if f2 { v2 } else { v1 + v2 })
/// is associative (a segment boundary "absorbs" everything to its left), so any parallel scan works on it unchanged.
type Seg = (bool, i64);
fn seg_op(a: Seg, b: Seg) -> Seg {
    (a.0 || b.0, if b.0 { b.1 } else { a.1.wrapping_add(b.1) })
}
const SEG_ID: Seg = (false, 0);

fn seq_segmented(vals: &[i64], flags: &[bool]) -> Vec<i64> {
    let mut out = Vec::with_capacity(vals.len());
    let mut acc = 0i64;
    for (i, (&v, &f)) in vals.iter().zip(flags).enumerate() {
        acc = if f || i == 0 { v } else { acc.wrapping_add(v) };
        out.push(acc);
    }
    out
}

fn segmented_scan(vals: &[i64], flags: &[bool], threads: usize) -> Vec<i64> {
    let pairs: Vec<Seg> = vals.iter().zip(flags).map(|(&v, &f)| (f, v)).collect();
    blocked_scan(&pairs, SEG_ID, &seg_op, threads)
        .into_iter()
        .map(|p| p.1)
        .collect()
}

// ---------------------------------------------------------------- 5. reductions
/// Contiguous split per thread, combine partials in thread order. The BRACKETING depends on `threads`,
/// so for floats the result depends on the thread count (and with work stealing / atomics it would depend on timing).
fn sum_by_threads(x: &[f64], threads: usize) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    let cl = chunk_len(x.len(), threads);
    let parts: Vec<f64> = thread::scope(|s| {
        let hs: Vec<_> = x
            .chunks(cl)
            .map(|c| s.spawn(move || c.iter().sum::<f64>()))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    parts.iter().sum()
}

/// Fixed leaf size: the leaf partition and the tree shape are functions of n only. Threads merely decide WHO computes
/// each leaf, never how the additions are bracketed, so the answer is bit-identical for any thread count.
const LEAF: usize = 64;
fn sum_fixed_tree(x: &[f64], threads: usize) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    let leaves: Vec<&[f64]> = x.chunks(LEAF).collect();
    let per = chunk_len(leaves.len(), threads);
    let mut sums: Vec<f64> = thread::scope(|s| {
        let hs: Vec<_> = leaves
            .chunks(per)
            .map(|grp| {
                s.spawn(move || {
                    grp.iter()
                        .map(|l| l.iter().sum::<f64>())
                        .collect::<Vec<f64>>()
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    // pairwise tree over leaf sums; odd element is carried up unchanged (shape depends on count only)
    while sums.len() > 1 {
        sums = sums
            .chunks(2)
            .map(|p| if p.len() == 2 { p[0] + p[1] } else { p[0] })
            .collect();
    }
    sums[0]
}

// ---------------------------------------------------------------- 6. stream compaction
/// keep = predicate flags (0/1). exclusive scan of keep gives each survivor its output slot.
fn compact<T: Copy + Send + Sync + Default>(
    x: &[T],
    keep: impl Fn(&T) -> bool + Sync,
    threads: usize,
) -> Vec<T> {
    let flags: Vec<u64> = x.iter().map(|v| keep(v) as u64).collect();
    let inc = blocked_scan(&flags, 0u64, &|a, b| a + b, threads);
    let total = inc.last().copied().unwrap_or(0) as usize;
    let mut out = vec![T::default(); total];
    // scatter: survivor i goes to inc[i]-1. Done serially here; in a real kernel each thread scatters its chunk
    // (writes are disjoint by construction, which is the whole point of scan-based compaction).
    for i in 0..x.len() {
        if flags[i] == 1 {
            out[inc[i] as usize - 1] = x[i];
        }
    }
    out
}

// ---------------------------------------------------------------- verification
/// A deliberately NON-commutative associative operator: affine map composition (a,b) means x -> a*x+b (wrapping).
/// compose(f, g) = apply f first then g. Catches any scan that swaps operand order.
type Aff = (u64, u64);
fn aff_op(f: Aff, g: Aff) -> Aff {
    (
        f.0.wrapping_mul(g.0),
        f.1.wrapping_mul(g.0).wrapping_add(g.1),
    )
}
const AFF_ID: Aff = (1, 0);

fn check_integers(rng: &mut Rng) {
    for case in 0..2000 {
        let n = if case % 50 == 0 {
            0
        } else {
            rng.below(300) as usize
        };
        let x: Vec<u64> = (0..n).map(|_| rng.next() >> 20).collect();
        let want = seq_scan(&x, |a, b| a.wrapping_add(b));
        assert_eq!(
            hillis_steele(&x, |a, b| a.wrapping_add(b)),
            want,
            "HS sum n={n}"
        );
        assert_eq!(
            blelloch_inclusive(&x, 0, |a: u64, b| a.wrapping_add(b)),
            want,
            "Blelloch sum n={n}"
        );
        for threads in [1, 2, 3, 7, 16] {
            assert_eq!(
                blocked_scan(&x, 0, &|a: u64, b| a.wrapping_add(b), threads),
                want,
                "blocked n={n} t={threads}"
            );
        }
        // max: idempotent, identity 0 for u64
        let wmax = seq_scan(&x, |a, b| a.max(b));
        assert_eq!(blelloch_inclusive(&x, 0, |a: u64, b| a.max(b)), wmax);
        assert_eq!(blocked_scan(&x, 0, &|a: u64, b| a.max(b), 5), wmax);
        // non-commutative
        let f: Vec<Aff> = (0..n).map(|_| (rng.next() | 1, rng.next())).collect();
        let wf = seq_scan(&f, aff_op);
        assert_eq!(hillis_steele(&f, aff_op), wf, "HS affine");
        assert_eq!(
            blelloch_inclusive(&f, AFF_ID, aff_op),
            wf,
            "Blelloch affine"
        );
        assert_eq!(blocked_scan(&f, AFF_ID, &aff_op, 4), wf, "blocked affine");
    }
    println!(
        "integers: HS, Blelloch, blocked == sequential on 2000 inputs (sum, max, non-commutative affine)"
    );
}

fn check_segmented(rng: &mut Rng) {
    for _ in 0..2000 {
        let n = rng.below(200) as usize;
        let vals: Vec<i64> = (0..n).map(|_| (rng.below(2000) as i64) - 1000).collect();
        let p = rng.below(100);
        let flags: Vec<bool> = (0..n).map(|_| rng.below(100) < p).collect();
        let want = seq_segmented(&vals, &flags);
        for t in [1, 3, 8] {
            assert_eq!(segmented_scan(&vals, &flags, t), want, "segmented t={t}");
        }
    }
    println!("segmented scan == sequential restart-at-flag on 2000 inputs (flag density 0..100%)");
}

fn check_compaction(rng: &mut Rng) {
    for _ in 0..2000 {
        let n = rng.below(300) as usize;
        let x: Vec<u32> = (0..n).map(|_| rng.below(1000) as u32).collect();
        let thr = rng.below(1000) as u32;
        let want: Vec<u32> = x.iter().copied().filter(|&v| v >= thr).collect();
        for t in [1, 2, 5] {
            assert_eq!(compact(&x, |&v| v >= thr, t), want);
        }
    }
    println!("compaction via scan == stable filter on 2000 inputs (order preserved)");
}

fn check_floats(rng: &mut Rng) {
    let mut max_rel: f64 = 0.0;
    let mut differing_thread_results = 0;
    for case in 0..2000 {
        let n = rng.below(700) as usize;
        // wide dynamic range makes rounding order visible
        let x: Vec<f64> = (0..n)
            .map(|_| {
                let sign = if rng.below(2) == 0 { 1.0 } else { -1.0 };
                sign * rng.f64() * 10f64.powi(rng.below(9) as i32 - 4)
            })
            .collect();
        let exact_ish: f64 = {
            // reference: sort by magnitude then sum in f64 is not exact, so use pairwise-compensated Neumaier
            let (mut s, mut c) = (0.0f64, 0.0f64);
            for &v in &x {
                let t = s + v;
                c += if s.abs() >= v.abs() {
                    (s - t) + v
                } else {
                    (v - t) + s
                };
                s = t;
            }
            s + c
        };
        let scale: f64 = x.iter().map(|v| v.abs()).sum::<f64>().max(1e-300);
        let mut results = Vec::new();
        for t in [1, 2, 3, 5, 8] {
            let a = sum_fixed_tree(&x, t);
            results.push(a.to_bits());
            let b = sum_by_threads(&x, t);
            // error is bounded by ~ n*eps*sum|x|; use a generous tolerance relative to sum of magnitudes
            max_rel =
                max_rel.max(((a - exact_ish).abs() / scale).max((b - exact_ish).abs() / scale));
        }
        assert!(
            results.windows(2).all(|w| w[0] == w[1]),
            "fixed tree must be bit-identical across thread counts (case {case})"
        );
        let by: Vec<u64> = [1, 2, 3, 5, 8]
            .iter()
            .map(|&t| sum_by_threads(&x, t).to_bits())
            .collect();
        if by.windows(2).any(|w| w[0] != w[1]) {
            differing_thread_results += 1;
        }
        // float scan: blocked vs sequential within tolerance (different bracketing)
        let want = seq_scan(&x, |a, b| a + b);
        let got = blocked_scan(&x, 0.0, &|a: f64, b| a + b, 4);
        let run_scale =
            x.iter().map(|v| v.abs()).fold(0.0, f64::max).max(1e-300) * (n as f64).max(1.0);
        for (w, g) in want.iter().zip(&got) {
            assert!(
                (w - g).abs() <= 1e-12 * run_scale,
                "float scan drift too large"
            );
        }
    }
    assert!(
        max_rel < 1e-12,
        "float reductions should be accurate to ~n*eps relative to sum|x|, got {max_rel:e}"
    );
    println!(
        "floats: fixed-tree sum bit-identical for threads 1,2,3,5,8 on 2000 inputs; max rel err vs Neumaier {max_rel:.1e}"
    );
    println!(
        "        naive per-thread split gave DIFFERENT bits for different thread counts in {differing_thread_results}/2000 inputs (the reproducibility problem)"
    );
    assert!(
        differing_thread_results > 0,
        "expected at least one visible non-reproducible case"
    );
}

// ---------------------------------------------------------------- work counting
fn work_counts() {
    println!("\noperator applications (work), n = power of two:");
    println!(
        "{:>8} {:>14} {:>12} {:>10} {:>10}",
        "n", "Hillis-Steele", "Blelloch", "HS/n", "Bl/n"
    );
    for lg in [4u32, 8, 12, 16] {
        let n = 1usize << lg;
        let x: Vec<u64> = (0..n as u64).collect();
        let c = Cell::new(0u64);
        let _ = hillis_steele(&x, |a, b| {
            c.set(c.get() + 1);
            a + b
        });
        let hs = c.replace(0);
        let _ = blelloch_exclusive(&x, 0, |a, b| {
            c.set(c.get() + 1);
            a + b
        });
        let bl = c.get();
        println!(
            "{n:>8} {hs:>14} {bl:>12} {:>10.2} {:>10.2}",
            hs as f64 / n as f64,
            bl as f64 / n as f64
        );
        // exact formulas: HS applies op to (n - d) elements per round; Blelloch does (n-1) up + (n-1) down
        let want_hs: u64 = (0..lg).map(|k| (n - (1usize << k)) as u64).sum();
        assert_eq!(hs, want_hs, "Hillis-Steele work formula");
        assert_eq!(bl, 2 * (n as u64 - 1), "Blelloch work = 2(n-1)");
        assert!(lg < 8 || hs > 4 * bl / 2, "asymptotic gap");
    }
}

// ---------------------------------------------------------------- timings
fn time<R>(label: &str, bytes: f64, f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let r = f();
    let s = t.elapsed().as_secs_f64();
    println!(
        "  {label:<34} {:>8.2} ms  {:>7.2} GB/s (touched)",
        s * 1e3,
        bytes / s / 1e9
    );
    r
}

fn timings() {
    let n = 8_000_000usize;
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let x: Vec<u64> = (0..n).map(|_| rng.next() >> 40).collect();
    let add = |a: u64, b: u64| a.wrapping_add(b);
    let bytes = (n * 8 * 2) as f64;
    println!("\ntimings, n = {n} u64 (printed only; machine dependent):");
    let want = time("sequential scan", bytes, || seq_scan(&x, add));
    for t in [1, 2, 4, 8] {
        let got = time(
            &format!("blocked 3-phase, {t} threads"),
            bytes * 1.5,
            || blocked_scan(&x, 0, &add, t),
        );
        assert_eq!(got, want);
    }
    let small = &x[..1 << 16];
    let w2 = seq_scan(small, add);
    let g = time("Hillis-Steele (n=65536, serial)", 0.0, || {
        hillis_steele(small, add)
    });
    assert_eq!(g, w2);
    let g = time("Blelloch (n=65536, serial)", 0.0, || {
        blelloch_inclusive(small, 0, add)
    });
    assert_eq!(g, w2);
    let xf: Vec<f64> = (0..n).map(|_| rng.f64()).collect();
    println!("float reductions (value shown to reveal bracketing differences):");
    for t in [1, 2, 3, 4, 8] {
        let a = sum_by_threads(&xf, t);
        let b = sum_fixed_tree(&xf, t);
        println!(
            "  threads {t}: per-thread-split {a:.10} bits {:016x} | fixed-tree {b:.10} bits {:016x}",
            a.to_bits(),
            b.to_bits()
        );
    }
}

fn demo() {
    let x = [3u64, 1, 7, 0, 4, 1, 6, 3];
    println!("input              {x:?}");
    println!("inclusive scan     {:?}", seq_scan(&x, |a, b| a + b));
    println!("Hillis-Steele      {:?}", hillis_steele(&x, |a, b| a + b));
    println!(
        "Blelloch exclusive {:?}",
        blelloch_exclusive(&x, 0, |a, b| a + b)
    );
    println!(
        "blocked (3 thr)    {:?}",
        blocked_scan(&x, 0, &|a: u64, b| a + b, 3)
    );
    let flags = [true, false, false, true, false, true, false, false];
    let v: Vec<i64> = x.iter().map(|&a| a as i64).collect();
    println!("segmented flags    {flags:?}");
    println!("segmented scan     {:?}", segmented_scan(&v, &flags, 3));
    println!("compact >=3        {:?}", compact(&x, |&a| a >= 3, 3));
}

fn main() {
    demo();
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    check_integers(&mut rng);
    check_segmented(&mut rng);
    check_compaction(&mut rng);
    check_floats(&mut rng);
    work_counts();
    timings();
    println!("\nall scan/reduction checks passed");
}
