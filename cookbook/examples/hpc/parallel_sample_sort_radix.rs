//! RECIPE 885 (hpc): parallel sample sort and parallel LSD/MSD radix sort (histogram + prefix-sum scatter)
//! LEVEL: advanced
//! Use in: database sort/merge-join/group-by, external sort run generation, building indexes, GPU/CPU sort libraries.   Run: cargo run --release --example parallel_sample_sort_radix
//! COMMON USES: sorting 64-bit keys fast (columnar databases, graph edge lists, suffix-array construction), partitioning for hash/sort-merge joins, rendering (sort by depth/tile id).
//! Course: CMU 15-445 (sorting and aggregation lecture), CMU 15-418 (parallel sorting).
#![allow(dead_code)]
//!
//! Two families beat `sort_unstable` when n is large and the keys are integers:
//!   * RADIX: no comparisons. Per digit: histogram, exclusive prefix sum (= where each bucket starts), scatter.
//!     LSD (least significant digit first) needs d stable passes over everything; MSD partitions by the top digit and
//!     recurses into independent buckets (which is where the parallelism comes from).
//!   * SAMPLE SORT: pick p-1 splitters from a random sample, partition all keys into p ranges (each thread partitions its
//!     own chunk), then sort the ranges independently and in parallel. Comparison-based, works for any Ord key.
//! Parallel LSD: every thread histograms ITS chunk, a global prefix over (digit, thread) order gives each thread a private
//! write cursor per digit, and the scatter needs no locks. Because chunks are ordered, the pass stays STABLE, which LSD requires.
//! The scatter writes to disjoint but interleaved destinations, which safe Rust cannot express as slices; we use
//! `Vec<AtomicU64>` with Relaxed stores (same machine code as plain stores) instead of `unsafe`.
//! Skew: sorted/reversed/duplicate-heavy inputs are where naive samples collapse. Sample sort uses EQUALITY BUCKETS (keys equal
//! to a splitter go to their own bucket that needs no sorting; IPS4o idea), radix skips passes where one digit holds every key,
//! and MSD starts at the highest byte where keys actually differ.
//! Memory-bandwidth view: one LSD pass reads n*8 and writes n*8 bytes, so 8 passes of 8 bits move ~128 bytes per key; 11-bit digits
//! need 6 passes but the 2048-entry histogram/cursors still fit in L1. The printed table compares against a memcpy baseline.
//! Signed ints and floats sort with the same code after a key transform (see `i64_key`, `f64_key`).
//! Read: Wikipedia, Radix sort https://en.wikipedia.org/wiki/Radix_sort and Samplesort https://en.wikipedia.org/wiki/Samplesort
//! Read: Axtmann et al., "Engineering In-place (Shared-memory) Sorting Algorithms" (IPS4o; cited by title, not re-read here)
//! Read: https://doc.rust-lang.org/std/primitive.slice.html#method.sort_unstable (pdqsort-based baseline) and the rayon crate's `par_sort_unstable`
//! TRY NEXT:
//!   - [S] Change `lsd_radix_serial` to digit widths 4, 8, 11, 16 and print time per key; predict where the L1/L2 cache stops holding the histogram.
//!   - [S] Remove the equality-bucket test in `bucket_of` (always send equal keys right) and watch `max_sorted_bucket` on the all-equal input.
//!   - [M] Make `sample_sort` generic over `T: Ord + Copy + Send + Sync`; what breaks in the counting of equal buckets and why does `Copy` matter?
//!   - [M] Predict, then measure: at what n does `sample_sort` with T=4 start to beat `sort_unstable`? Sweep n = 2^12..2^22 and print the crossover.
//!   - [A] PROJECT: external sort of u64 keys larger than a memory budget: use `lsd_radix_parallel` to make sorted runs, then merge them (see kway_merge_family
//!     and sparse_kernels_spmv_csr for building CSR edge lists sorted by (src, dst)); report passes over the data and bytes moved vs the model.
use rayon::slice::ParallelSliceMut;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::thread;
use std::time::{Duration, Instant};

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
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ---------------------------------------------------------------- key transforms

/// Signed ints: flipping the sign bit maps the two's complement order onto unsigned order.
fn i64_key(x: i64) -> u64 {
    (x as u64) ^ (1 << 63)
}

/// Floats: negative numbers have the sign bit set and sort BACKWARDS by magnitude, so flip all bits;
/// positives only need the sign bit set. The result orders exactly like `f64::total_cmp` (NaNs and -0.0 included).
fn f64_key(x: f64) -> u64 {
    let b = x.to_bits();
    if b >> 63 == 1 { !b } else { b | (1 << 63) }
}

// ---------------------------------------------------------------- helpers

/// Longest-processing-time-first assignment of weighted jobs to `t` workers (greedy 4/3 makespan approximation).
fn lpt_assign<J>(mut jobs: Vec<(usize, J)>, t: usize) -> Vec<Vec<J>> {
    jobs.sort_by_key(|j| std::cmp::Reverse(j.0));
    let mut loads = vec![0usize; t];
    let mut out: Vec<Vec<J>> = (0..t).map(|_| Vec::new()).collect();
    for (w, j) in jobs {
        let k = (0..t).min_by_key(|&k| loads[k]).unwrap();
        loads[k] += w;
        out[k].push(j);
    }
    out
}

fn insertion_sort(v: &mut [u64]) {
    for i in 1..v.len() {
        let x = v[i];
        let mut j = i;
        while j > 0 && v[j - 1] > x {
            v[j] = v[j - 1];
            j -= 1;
        }
        v[j] = x;
    }
}

// ---------------------------------------------------------------- LSD radix, serial

/// Returns the number of scatter passes actually executed (trivial passes are skipped).
fn lsd_radix_serial(v: &mut Vec<u64>, bits: u32) -> usize {
    let n = v.len();
    if n < 2 {
        return 0;
    }
    let radix = 1usize << bits;
    let mask = (radix - 1) as u64;
    let passes = 64u32.div_ceil(bits) as usize;
    // one read of the input builds EVERY digit's histogram (classic trick: histograms are independent of the permutation
    // because each pass only reorders, it never changes the multiset of digits)
    let mut hist = vec![vec![0usize; radix]; passes];
    for &x in v.iter() {
        for (p, h) in hist.iter_mut().enumerate() {
            h[((x >> (p as u32 * bits)) & mask) as usize] += 1;
        }
    }
    let mut tmp = vec![0u64; n];
    let mut run = 0;
    for (p, h) in hist.iter_mut().enumerate() {
        if h.iter().any(|&c| c == n) {
            continue; // every key has the same digit: the pass would be the identity
        }
        let mut sum = 0;
        for c in h.iter_mut() {
            let t = *c;
            *c = sum; // exclusive prefix sum: start offset of this bucket
            sum += t;
        }
        let shift = p as u32 * bits;
        for &x in v.iter() {
            let d = ((x >> shift) & mask) as usize;
            tmp[h[d]] = x;
            h[d] += 1;
        }
        std::mem::swap(v, &mut tmp);
        run += 1;
    }
    run
}

// ---------------------------------------------------------------- LSD radix, parallel

fn lsd_radix_parallel(v: &mut Vec<u64>, bits: u32, t: usize) -> usize {
    let n = v.len();
    if n < 2 {
        return 0;
    }
    let t = t.clamp(1, n);
    let radix = 1usize << bits;
    let mask = (radix - 1) as u64;
    let passes = 64u32.div_ceil(bits);
    let chunk = n.div_ceil(t);
    let range = |ci: usize| (ci * chunk).min(n)..((ci + 1) * chunk).min(n);
    let mut src: Vec<AtomicU64> = v.iter().map(|&x| AtomicU64::new(x)).collect();
    let mut dst: Vec<AtomicU64> = (0..n).map(|_| AtomicU64::new(0)).collect();
    let mut run = 0;
    for p in 0..passes {
        let shift = p * bits;
        let srcr = &src;
        // phase 1: per-thread histograms of this pass's digit
        let hist: Vec<Vec<usize>> = thread::scope(|s| {
            let hs: Vec<_> = (0..t)
                .map(|ci| {
                    s.spawn(move || {
                        let mut h = vec![0usize; radix];
                        for i in range(ci) {
                            h[((srcr[i].load(Relaxed) >> shift) & mask) as usize] += 1;
                        }
                        h
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        if (0..radix).any(|d| hist.iter().map(|h| h[d]).sum::<usize>() == n) {
            continue;
        }
        // phase 2: cursor of (thread, digit) = everything with a smaller digit + same digit in earlier threads.
        // Walking digits outer / threads inner is what makes the scatter stable.
        let mut cursor = vec![vec![0usize; radix]; t];
        let mut running = 0;
        for d in 0..radix {
            for ci in 0..t {
                cursor[ci][d] = running;
                running += hist[ci][d];
            }
        }
        // phase 3: each thread scatters its chunk using its private cursors; destinations are disjoint by construction
        let dstr = &dst;
        thread::scope(|s| {
            for (ci, mut cur) in cursor.into_iter().enumerate() {
                s.spawn(move || {
                    for i in range(ci) {
                        let x = srcr[i].load(Relaxed);
                        let d = ((x >> shift) & mask) as usize;
                        dstr[cur[d]].store(x, Relaxed);
                        cur[d] += 1;
                    }
                });
            }
        });
        std::mem::swap(&mut src, &mut dst);
        run += 1;
    }
    for (o, a) in v.iter_mut().zip(&src) {
        *o = a.load(Relaxed);
    }
    run
}

// ---------------------------------------------------------------- MSD radix

/// Out-of-place MSD pass on one byte; recurses into buckets with the next byte. `tmp` has the same length as `v`.
fn msd_radix(v: &mut [u64], tmp: &mut [u64], shift: u32) {
    if v.len() <= 32 {
        insertion_sort(v); // recursion on tiny buckets costs more than it saves
        return;
    }
    let bounds = msd_partition(v, tmp, shift);
    if shift == 0 {
        return;
    }
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b - a > 1 {
            msd_radix(&mut v[a..b], &mut tmp[a..b], shift - 8);
        }
    }
}

/// Partition `v` by the byte at `shift` (stable, via tmp), return the 257 bucket boundaries.
fn msd_partition(v: &mut [u64], tmp: &mut [u64], shift: u32) -> Vec<usize> {
    let mut start = vec![0usize; 257];
    for &x in v.iter() {
        start[((x >> shift) & 255) as usize + 1] += 1;
    }
    for d in 0..256 {
        start[d + 1] += start[d];
    }
    let bounds = start.clone();
    for &x in v.iter() {
        let d = ((x >> shift) & 255) as usize;
        tmp[start[d]] = x;
        start[d] += 1;
    }
    v.copy_from_slice(tmp);
    bounds
}

/// Highest byte position at which the keys differ (shift amount), or None if all keys are equal.
fn first_differing_shift(v: &[u64]) -> Option<u32> {
    let first = *v.first()?;
    let diff = v.iter().fold(0u64, |acc, &x| acc | (x ^ first));
    if diff == 0 {
        None
    } else {
        Some((63 - diff.leading_zeros()) / 8 * 8)
    }
}

fn msd_radix_serial(v: &mut [u64]) {
    if let Some(shift) = first_differing_shift(v) {
        let mut tmp = vec![0u64; v.len()];
        msd_radix(v, &mut tmp, shift);
    }
}

/// Top level partition is serial (one pass); the 256 buckets are independent, so they are handed to threads LPT-style.
fn msd_radix_parallel(v: &mut [u64], t: usize) {
    let Some(shift) = first_differing_shift(v) else {
        return;
    };
    let mut tmp = vec![0u64; v.len()];
    let bounds = msd_partition(v, &mut tmp, shift);
    if shift == 0 {
        return;
    }
    let (mut vr, mut tr): (&mut [u64], &mut [u64]) = (v, &mut tmp);
    let mut jobs = Vec::new();
    for w in bounds.windows(2) {
        let len = w[1] - w[0];
        let (vh, vt) = vr.split_at_mut(len);
        let (th, tt) = tr.split_at_mut(len);
        vr = vt;
        tr = tt;
        if len > 1 {
            jobs.push((len, (vh, th)));
        }
    }
    let lists = lpt_assign(jobs, t.max(1));
    thread::scope(|s| {
        for list in lists {
            s.spawn(move || {
                for (vh, th) in list {
                    msd_radix(vh, th, shift - 8);
                }
            });
        }
    });
}

// ---------------------------------------------------------------- sample sort

#[derive(Debug, Default)]
struct SampleStats {
    buckets: usize,
    equal_buckets: usize,
    max_sorted_bucket: usize,
}

/// Bucket layout for splitters s_0 < s_1 < ... : 2i = keys strictly between s_{i-1} and s_i, 2i+1 = keys == s_i.
/// Odd buckets are all-equal and are never sorted.
fn bucket_of(splitters: &[u64], key: u64) -> usize {
    let i = splitters.partition_point(|&s| s < key);
    if i < splitters.len() && splitters[i] == key {
        2 * i + 1
    } else {
        2 * i
    }
}

/// `cutoff`: below this many keys fall back to sort_unstable (tests pass a tiny cutoff to exercise the parallel path).
fn sample_sort(v: &mut [u64], t: usize, cutoff: usize, rng: &mut Rng) -> SampleStats {
    let n = v.len();
    if t <= 1 || n < cutoff.max(2 * t) {
        v.sort_unstable();
        return SampleStats {
            buckets: 1,
            equal_buckets: 0,
            max_sorted_bucket: n,
        };
    }
    // 1. splitters: oversample (16 per bucket) so bucket sizes concentrate around n/buckets
    let k = 4 * t - 1; // more buckets than threads so LPT can even out the load
    let oversample = 16;
    let mut sample: Vec<u64> = (0..(k + 1) * oversample).map(|_| v[rng.below(n)]).collect();
    sample.sort_unstable();
    let mut splitters: Vec<u64> = (1..=k)
        .map(|i| sample[i * sample.len() / (k + 1)])
        .collect();
    splitters.dedup();
    let nb = 2 * splitters.len() + 1;

    // 2. each thread partitions its own chunk into private per-bucket vectors (no sharing, no atomics)
    let chunk = n.div_ceil(t);
    let spl = &splitters;
    let locals: Vec<Vec<Vec<u64>>> = thread::scope(|s| {
        let hs: Vec<_> = v
            .chunks(chunk)
            .map(|c| {
                s.spawn(move || {
                    let mut b: Vec<Vec<u64>> = vec![Vec::new(); nb];
                    for &x in c {
                        b[bucket_of(spl, x)].push(x);
                    }
                    b
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });

    // 3. bucket sizes fix the output layout; split the output into one disjoint slice per bucket
    let sizes: Vec<usize> = (0..nb)
        .map(|b| locals.iter().map(|l| l[b].len()).sum())
        .collect();
    debug_assert_eq!(sizes.iter().sum::<usize>(), n);
    let mut rest: &mut [u64] = v;
    let mut jobs = Vec::new();
    for (b, &sz) in sizes.iter().enumerate() {
        let (head, tail) = rest.split_at_mut(sz);
        rest = tail;
        let weight = if b % 2 == 0 { sz * 4 } else { sz }; // sorting costs more than copying
        jobs.push((weight, (b, head)));
    }
    // 4. gather each bucket's pieces from all threads, then sort it (unless it is an equality bucket)
    let lists = lpt_assign(jobs, t);
    let locr = &locals;
    thread::scope(|s| {
        for list in lists {
            s.spawn(move || {
                for (b, slice) in list {
                    let mut pos = 0;
                    for l in locr {
                        slice[pos..pos + l[b].len()].copy_from_slice(&l[b]);
                        pos += l[b].len();
                    }
                    if b % 2 == 0 {
                        slice.sort_unstable();
                    }
                }
            });
        }
    });
    SampleStats {
        buckets: nb,
        equal_buckets: splitters.len(),
        max_sorted_bucket: (0..nb)
            .filter(|b| b % 2 == 0)
            .map(|b| sizes[b])
            .max()
            .unwrap_or(0),
    }
}

// ---------------------------------------------------------------- input distributions

#[derive(Clone, Copy, Debug)]
enum Dist {
    Uniform,
    Sorted,
    Reverse,
    Few16,
    AllEqual,
    SmallRange,
    Skewed,
    NearlySorted,
    OrganPipe,
}

const DISTS: [Dist; 9] = [
    Dist::Uniform,
    Dist::Sorted,
    Dist::Reverse,
    Dist::Few16,
    Dist::AllEqual,
    Dist::SmallRange,
    Dist::Skewed,
    Dist::NearlySorted,
    Dist::OrganPipe,
];

fn make_input(rng: &mut Rng, d: Dist, n: usize) -> Vec<u64> {
    let mut v: Vec<u64> = match d {
        Dist::Uniform => (0..n).map(|_| rng.next()).collect(),
        Dist::Sorted => (0..n as u64).collect(),
        Dist::Reverse => (0..n as u64).rev().collect(),
        Dist::Few16 => (0..n).map(|_| rng.next() % 16 * 0x0101_0101_0101).collect(),
        Dist::AllEqual => vec![42; n],
        Dist::SmallRange => (0..n).map(|_| rng.next() % 65_536).collect(),
        // min of three uniforms: heavily piled towards 0 (a crude power law)
        Dist::Skewed => (0..n)
            .map(|_| rng.next().min(rng.next()).min(rng.next()))
            .collect(),
        Dist::NearlySorted => (0..n as u64).map(|x| x * 3).collect(),
        Dist::OrganPipe => (0..n as u64)
            .map(|x| if x < n as u64 / 2 { x } else { n as u64 - x })
            .collect(),
    };
    if let Dist::NearlySorted = d {
        for _ in 0..n / 100 {
            let (a, b) = (rng.below(n), rng.below(n));
            v.swap(a, b);
        }
    }
    v
}

// ---------------------------------------------------------------- verification

fn check(name: &str, got: &[u64], want: &[u64]) {
    // `want` is std's sorted copy: equality proves BOTH sortedness and that the output is a permutation of the input
    assert!(
        got == want,
        "{name}: output differs from the sorted oracle (len {} vs {})",
        got.len(),
        want.len()
    );
}

fn verify_all(rng: &mut Rng) {
    for case in 0..2000 {
        let n = match case % 4 {
            0 => rng.below(40),
            1 => rng.below(400),
            2 => rng.below(3000),
            _ => 1 + rng.below(8),
        };
        let dist = DISTS[rng.below(DISTS.len())];
        let input = make_input(rng, dist, n);
        let mut want = input.clone();
        want.sort_unstable();

        for bits in [8, 11, 16] {
            let mut v = input.clone();
            lsd_radix_serial(&mut v, bits);
            check(&format!("lsd serial {bits}b {dist:?}"), &v, &want);
        }
        for t in [1, 2, 3, 4] {
            let mut v = input.clone();
            lsd_radix_parallel(&mut v, 8, t);
            check(&format!("lsd parallel T={t} {dist:?}"), &v, &want);
            let mut v = input.clone();
            msd_radix_parallel(&mut v, t);
            check(&format!("msd parallel T={t} {dist:?}"), &v, &want);
        }
        let mut v = input.clone();
        msd_radix_serial(&mut v);
        check(&format!("msd serial {dist:?}"), &v, &want);
        for t in [1, 2, 3, 5] {
            let mut v = input.clone();
            sample_sort(&mut v, t, 4, rng); // cutoff 4: force the parallel path even on tiny arrays
            check(&format!("sample sort T={t} {dist:?}"), &v, &want);
        }
    }
    println!(
        "2000 random arrays x 9 distributions: LSD(8/11/16 bit), parallel LSD, MSD, parallel MSD, sample sort all match std sort"
    );

    // key transforms
    for _ in 0..200 {
        let n = rng.below(200);
        let fs: Vec<f64> = (0..n)
            .map(|i| match rng.below(8) {
                0 => f64::NAN,
                1 => -0.0,
                2 => 0.0,
                3 => f64::INFINITY,
                4 => f64::NEG_INFINITY,
                _ => f64::from_bits(rng.next()) * if i % 2 == 0 { 1.0 } else { -1.0 },
            })
            .collect();
        let mut keys: Vec<u64> = fs.iter().map(|&f| f64_key(f)).collect();
        lsd_radix_serial(&mut keys, 8);
        let mut want = fs.clone();
        want.sort_by(f64::total_cmp);
        let want_keys: Vec<u64> = want.iter().map(|&f| f64_key(f)).collect();
        assert_eq!(keys, want_keys, "f64 key order == total_cmp");
        let is: Vec<i64> = (0..n).map(|_| rng.next() as i64 >> rng.below(60)).collect();
        let mut ik: Vec<u64> = is.iter().map(|&x| i64_key(x)).collect();
        lsd_radix_serial(&mut ik, 11);
        let mut want_i = is.clone();
        want_i.sort_unstable();
        assert_eq!(ik, want_i.iter().map(|&x| i64_key(x)).collect::<Vec<_>>());
    }
    println!(
        "f64 and i64 key transforms order like total_cmp / signed compare on 200 random arrays"
    );
}

fn verify_skew_handling(rng: &mut Rng) {
    let n = 1 << 18;
    let t = 4;
    let mut v = make_input(rng, Dist::AllEqual, n);
    let st = sample_sort(&mut v, t, 1 << 12, rng);
    println!("sample sort all-equal: {st:?}");
    assert_eq!(
        st.max_sorted_bucket, 0,
        "equal keys must land in an equality bucket that is never sorted"
    );

    let mut v = make_input(rng, Dist::Uniform, n);
    let st = sample_sort(&mut v, t, 1 << 12, rng);
    let avg = n / (st.buckets / 2 + 1);
    println!("sample sort uniform:   {st:?} (average even bucket ~{avg})");
    assert!(
        st.max_sorted_bucket < 3 * avg,
        "oversampling keeps buckets within ~3x of the mean"
    );

    // trivial radix passes are skipped: keys < 2^16 need only 2 of 8 byte passes
    let mut v = make_input(rng, Dist::SmallRange, n);
    let passes = lsd_radix_serial(&mut v, 8);
    assert_eq!(passes, 2);
    let mut v = make_input(rng, Dist::AllEqual, n);
    assert_eq!(lsd_radix_serial(&mut v, 8), 0);
    println!(
        "LSD radix skips trivial passes: small-range keys run {passes}/8 passes, all-equal 0/8"
    );
}

// ---------------------------------------------------------------- timings

fn time_it(f: impl FnOnce()) -> Duration {
    let t0 = Instant::now();
    f();
    t0.elapsed()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn bench_table(rng: &mut Rng) {
    let n = 1 << 21;
    let t = 4;
    println!(
        "\ntimings in ms, n = {n} u64 keys ({} MB), T = {t} threads (machine dependent, never asserted)",
        n * 8 >> 20
    );
    println!(
        "{:<13}{:>9}{:>9}{:>9}{:>9}{:>9}{:>9}{:>9}",
        "dist", "std", "rayon", "lsd8", "lsd11", "lsd8-par", "msd-par", "sample"
    );
    let base = {
        let src = make_input(rng, Dist::Uniform, n);
        time_it(|| {
            let c = src.clone();
            std::hint::black_box(&c);
        })
    };
    for d in DISTS {
        let input = make_input(rng, d, n);
        let mut want = input.clone();
        let t_std = time_it(|| want.sort_unstable());
        let run = |f: &dyn Fn(&mut Vec<u64>)| {
            let mut v = input.clone();
            let el = time_it(|| f(&mut v));
            check(&format!("{d:?}"), &v, &want);
            ms(el)
        };
        let r = run(&|v| v.par_sort_unstable());
        let l8 = run(&|v| {
            lsd_radix_serial(v, 8);
        });
        let l11 = run(&|v| {
            lsd_radix_serial(v, 11);
        });
        let lp = run(&|v| {
            lsd_radix_parallel(v, 8, t);
        });
        let mp = run(&|v| msd_radix_parallel(v, t));
        let sm = run(&|v| {
            sample_sort(v, t, 1 << 12, &mut Rng::new(7));
        });
        println!(
            "{:<13}{:>9.1}{:>9.1}{:>9.1}{:>9.1}{:>9.1}{:>9.1}{:>9.1}",
            format!("{d:?}"),
            ms(t_std),
            r,
            l8,
            l11,
            lp,
            mp,
            sm
        );
    }

    // bandwidth model: bytes moved per key by LSD = passes * 16 (read + write 8 bytes) + 8 for the histogram read
    let mut v = make_input(rng, Dist::Uniform, n);
    let mut passes = 0;
    let el = time_it(|| passes = lsd_radix_serial(&mut v, 8));
    let bytes = (n * 8 * (2 * passes + 1)) as f64;
    println!(
        "\nbandwidth model: LSD8 ran {passes} passes = {:.0} MB moved in {:.1} ms = {:.1} GB/s; memcpy-style clone of the array: {:.1} ms ({:.1} GB/s for read+write)",
        bytes / 1e6,
        ms(el),
        bytes / el.as_secs_f64() / 1e9,
        ms(base),
        (2 * n * 8) as f64 / base.as_secs_f64() / 1e9
    );
    println!(
        "rule of thumb: if LSD GB/s is far below the memcpy figure the scatter is cache/TLB bound, not bandwidth bound (try 11-bit digits)."
    );
}

fn main() {
    let mut rng = Rng::new(885);
    verify_all(&mut rng);
    verify_skew_handling(&mut rng);
    bench_table(&mut rng);
    println!("\nall sort checks passed");
}
