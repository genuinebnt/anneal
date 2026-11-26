//! RECIPE 195 (concurrency): data parallelism with rayon: parallel iterators, fold/reduce, join, scope, thread pools.
//! LEVEL: intermediate
//! Use in: CPU-bound loops over big collections: sums, sorts, word counts, image/matrix work, batch transforms.   Run: cargo run --release --example rayon_data_parallelism
#![allow(dead_code)]
//!
//! WHAT: rayon turns `iter()` into `par_iter()` and runs the work on a global pool of OS threads (one per core by default).
//! HOW (work stealing): each worker has its own deque of tasks. `join(a, b)` pushes `b` on the local deque and runs `a`; an
//! idle worker steals the OLDEST task (usually a big, not-yet-split chunk) from a busy worker. Splitting is adaptive, so no
//! manual chunking is needed, and the sequential fallback is cheap when nobody steals.
//! WHY ASSOCIATIVITY: `reduce(identity, op)` and `fold` combine partial results in whatever tree the splitter happens to
//! produce, so `op` MUST be associative (and `identity` a true identity), else the answer depends on scheduling. Sums of integers
//! are fine; subtraction is not; floating point addition is not exactly associative either (tiny nondeterministic differences).
//! WHAT IS DETERMINISTIC: `collect()` into Vec keeps the input order; `sum/min/max/count` on integers are exact; an `op` that
//! is associative gives the same answer as the sequential fold. We assert equality with sequential results below.
//! RAYON vs TOKIO: rayon is for CPU-bound work (it keeps cores busy and blocks threads); tokio is for IO-bound concurrency (many
//! tasks waiting on sockets). Do not do blocking IO inside `par_iter`, and do not run long CPU loops on tokio worker threads
//! (use `spawn_blocking` or hand the work to rayon).
//! PITFALLS: tiny work (see the speedup table: below ~10k cheap items parallel is SLOWER), false sharing (per-thread counters on
//! one cache line), global locks inside the closure, and nested `par_iter` over tiny inner loops.
//! COMMON USES: parallel builds/linters over many files, ripgrep-style file scanning pipelines, image processing, Monte Carlo,
//! batch feature extraction, parallel sort and aggregation in data tools.
//! Read: https://github.com/rayon-rs/rayon , https://docs.rs/rayon/latest/rayon/ , https://docs.rs/rayon/latest/rayon/iter/trait.ParallelIterator.html
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

fn xorshift(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}
fn random_vec(n: usize, mut seed: u64) -> Vec<u64> {
    (0..n).map(|_| xorshift(&mut seed) % 1_000_000).collect()
}
fn time<R>(f: impl FnOnce() -> R) -> (R, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

// ---- divide and conquer with join and a sequential cutoff ----
const CUTOFF: usize = 8 * 1024;

fn par_quicksort(v: &mut [u64]) {
    if v.len() <= CUTOFF {
        v.sort_unstable(); // below the cutoff, spawning costs more than it saves
        return;
    }
    // median of three pivot, then Hoare-style three-way-free partition (values equal to pivot may go either side)
    let (a, b, c) = (v[0], v[v.len() / 2], v[v.len() - 1]);
    let pivot = a.max(b).min(a.min(b).max(c));
    let mut i = 0;
    let mut j = v.len() - 1;
    loop {
        while v[i] < pivot {
            i += 1
        }
        while v[j] > pivot {
            j -= 1
        }
        if i >= j {
            break;
        }
        v.swap(i, j);
        i += 1;
        j -= 1;
    }
    let split = (j + 1).clamp(1, v.len() - 1);
    let (l, r) = v.split_at_mut(split); // disjoint halves: safe parallel mutation without unsafe
    rayon::join(|| par_quicksort(l), || par_quicksort(r));
}

fn par_mergesort(v: &mut [u64], buf: &mut [u64]) {
    let n = v.len();
    if n <= CUTOFF {
        v.sort_unstable();
        return;
    }
    let mid = n / 2;
    {
        let (vl, vr) = v.split_at_mut(mid);
        let (bl, br) = buf.split_at_mut(mid);
        rayon::join(|| par_mergesort(vl, bl), || par_mergesort(vr, br));
    }
    let (mut i, mut j, mut k) = (0, mid, 0);
    while i < mid && j < n {
        if v[i] <= v[j] {
            buf[k] = v[i];
            i += 1
        } else {
            buf[k] = v[j];
            j += 1
        }
        k += 1;
    }
    while i < mid {
        buf[k] = v[i];
        i += 1;
        k += 1
    }
    while j < n {
        buf[k] = v[j];
        j += 1;
        k += 1
    }
    v.copy_from_slice(&buf[..n]);
}

#[repr(align(128))]
struct Padded(AtomicU64);

fn false_sharing_demo() {
    const ITERS: u64 = 5_000_000;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let packed: Vec<AtomicU64> = (0..4).map(|_| AtomicU64::new(0)).collect(); // 4 counters share one cache line
    let padded: Vec<Padded> = (0..4).map(|_| Padded(AtomicU64::new(0))).collect(); // one line each
    let (_, t_packed) = time(|| {
        pool.install(|| {
            (0..4usize).into_par_iter().for_each(|t| {
                for _ in 0..ITERS {
                    packed[t].fetch_add(1, Ordering::Relaxed);
                }
            })
        })
    });
    let (_, t_padded) = time(|| {
        pool.install(|| {
            (0..4usize).into_par_iter().for_each(|t| {
                for _ in 0..ITERS {
                    padded[t].0.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
    });
    assert!(packed.iter().all(|c| c.load(Ordering::Relaxed) == ITERS));
    assert!(padded.iter().all(|c| c.0.load(Ordering::Relaxed) == ITERS));
    println!(
        "false sharing: packed counters {t_packed:?} vs cache-line padded {t_padded:?} (same work, timing only)"
    );
}

fn main() {
    println!("rayon threads: {}", rayon::current_num_threads());

    // 1. par_iter().map().sum() equals sequential; par_chunks; collect keeps order
    let data = random_vec(2_000_000, 42);
    let seq: u64 = data.iter().map(|x| x * 3 + 1).sum();
    let par: u64 = data.par_iter().map(|x| x * 3 + 1).sum();
    assert_eq!(seq, par);
    let chunk_sums: Vec<u64> = data
        .par_chunks(100_000)
        .map(|c| c.iter().sum::<u64>())
        .collect();
    assert_eq!(chunk_sums.len(), 20);
    assert_eq!(chunk_sums.iter().sum::<u64>(), data.iter().sum::<u64>());
    let first_chunk: u64 = data[..100_000].iter().sum();
    assert_eq!(chunk_sums[0], first_chunk, "collect keeps input order");
    let squares_seq: Vec<u64> = data.iter().map(|x| x * x).collect();
    let squares_par: Vec<u64> = data.par_iter().map(|x| x * x).collect();
    assert_eq!(squares_seq, squares_par);
    println!("sum/chunks/collect: parallel == sequential ({par})");

    // 2. par_sort and the two join-based sorts agree with std sort
    let mut expect = random_vec(1_000_000, 7);
    let (mut a, mut b, mut c) = (expect.clone(), expect.clone(), expect.clone());
    let (_, t_seq) = time(|| expect.sort_unstable());
    let (_, t_ps) = time(|| a.par_sort_unstable());
    let (_, t_qs) = time(|| par_quicksort(&mut b));
    let mut buf = vec![0; c.len()];
    let (_, t_ms) = time(|| par_mergesort(&mut c, &mut buf));
    assert_eq!(a, expect);
    assert_eq!(b, expect);
    assert_eq!(c, expect);
    println!(
        "sort 1M: std {t_seq:?}, par_sort_unstable {t_ps:?}, join-quicksort {t_qs:?}, join-mergesort {t_ms:?}"
    );

    // 3. fold/reduce: associative op is deterministic; non-associative op is not
    let n = 1_000_000u64;
    let par_sum = (1..=n)
        .into_par_iter()
        .fold(|| 0u64, |acc, x| acc + x)
        .reduce(|| 0, |a, b| a + b);
    assert_eq!(par_sum, n * (n + 1) / 2);
    let (mn, mx) = data
        .par_iter()
        .fold(|| (u64::MAX, 0u64), |(lo, hi), &x| (lo.min(x), hi.max(x)))
        .reduce(|| (u64::MAX, 0), |a, b| (a.0.min(b.0), a.1.max(b.1)));
    assert_eq!(
        (mn, mx),
        (*data.iter().min().unwrap(), *data.iter().max().unwrap())
    );
    // subtraction is NOT associative: the result depends on how the range was split, which depends on the thread count
    let mut seen = Vec::new();
    for threads in [1usize, 2, 3, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let r = pool.install(|| {
            (1..100_001u32)
                .into_par_iter()
                .map(i64::from)
                .with_min_len(1000)
                .reduce(|| 0, |a, b| a - b)
        });
        seen.push((threads, r));
    }
    let distinct: std::collections::BTreeSet<i64> = seen.iter().map(|x| x.1).collect();
    println!(
        "non-associative reduce(a-b) by thread count: {seen:?} -> {} distinct result(s); never rely on it",
        distinct.len()
    );
    // float addition: close but not bit-identical in general (printed only)
    let fs: Vec<f64> = data.iter().map(|&x| x as f64 * 0.1).collect();
    let (s1, s2) = (fs.iter().sum::<f64>(), fs.par_iter().sum::<f64>());
    assert!((s1 - s2).abs() / s1 < 1e-9);
    println!(
        "float sum seq {s1} vs par {s2} (relative diff {:.2e})",
        (s1 - s2).abs() / s1
    );

    // 4. parallel word count: per-thread HashMap via fold, merge via reduce
    let vocab: Vec<String> = (0..500).map(|i| format!("w{i}")).collect();
    let mut seed = 99u64;
    let text: Vec<&str> = (0..400_000)
        .map(|_| vocab[(xorshift(&mut seed) % 500) as usize % 300].as_str())
        .collect();
    let mut seq_counts: HashMap<&str, usize> = HashMap::new();
    for w in &text {
        *seq_counts.entry(w).or_default() += 1;
    }
    let par_counts: HashMap<&str, usize> = text
        .par_iter()
        .fold(HashMap::new, |mut m: HashMap<&str, usize>, w| {
            *m.entry(w).or_default() += 1;
            m
        })
        .reduce(HashMap::new, |mut a, b| {
            for (k, v) in b {
                *a.entry(k).or_default() += v;
            }
            a
        });
    assert_eq!(seq_counts, par_counts);
    println!(
        "word count: {} distinct words, parallel == sequential",
        par_counts.len()
    );

    // 5. scope: spawn borrowed-data tasks that write disjoint output chunks
    let mut out = vec![0u64; 1_000_000];
    let input = &data[..1_000_000];
    rayon::scope(|s| {
        for (ci, oc) in out.chunks_mut(250_000).enumerate() {
            s.spawn(move |_| {
                for (k, o) in oc.iter_mut().enumerate() {
                    *o = input[ci * 250_000 + k] + 1;
                }
            });
        }
    });
    assert!(out.iter().zip(input).all(|(o, i)| *o == i + 1));
    println!("scope: 4 borrowed tasks filled disjoint chunks");

    // 6. custom pool sizes
    for threads in [1usize, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let (s, el) = pool.install(|| {
            assert_eq!(rayon::current_num_threads(), threads);
            time(|| {
                data.par_iter()
                    .map(|x| (x ^ 0x55).wrapping_mul(31))
                    .sum::<u64>()
            })
        });
        assert_eq!(
            s,
            data.iter()
                .map(|x| (x ^ 0x55).wrapping_mul(31))
                .sum::<u64>()
        );
        println!("pool of {threads} thread(s): {el:?}");
    }

    // 7. HashMap / BTreeMap parallel iteration, try_for_each early exit
    let hm: HashMap<u64, u64> = (0..100_000).map(|i| (i, i * 2)).collect();
    let bm: BTreeMap<u64, u64> = hm.iter().map(|(k, v)| (*k, *v)).collect();
    assert_eq!(
        hm.par_iter().map(|(_, v)| *v).sum::<u64>(),
        bm.par_iter().map(|(_, v)| *v).sum::<u64>()
    );
    let visited = AtomicUsize::new(0);
    let r: Result<(), u64> = (0..10_000_000u64).into_par_iter().try_for_each(|x| {
        visited.fetch_add(1, Ordering::Relaxed);
        if x == 3_000_000 { Err(x) } else { Ok(()) }
    });
    assert_eq!(r, Err(3_000_000));
    assert!(
        visited.load(Ordering::Relaxed) < 10_000_000,
        "early exit stopped most of the work"
    );
    println!(
        "try_for_each: stopped after {} of 10M items",
        visited.load(Ordering::Relaxed)
    );

    // 8. when parallel is NOT worth it: speedup table (timing only)
    println!(
        "{:>10} {:>12} {:>12} {:>9}",
        "n", "seq/iter", "par/iter", "speedup"
    );
    for n in [1_000usize, 10_000, 100_000, 1_000_000, 10_000_000] {
        let v: Vec<u64> = (0..n as u64).collect();
        let reps = (20_000_000 / n).clamp(1, 2000);
        let f = |x: &u64| (x.wrapping_mul(2654435761) >> 7) % 1000;
        let (_, ts) = time(|| {
            for _ in 0..reps {
                black_box(v.iter().map(f).sum::<u64>());
            }
        });
        let (_, tp) = time(|| {
            for _ in 0..reps {
                black_box(v.par_iter().map(f).sum::<u64>());
            }
        });
        assert_eq!(
            v.iter().map(f).sum::<u64>(),
            v.par_iter().map(f).sum::<u64>()
        );
        let (ts, tp) = (ts / reps as u32, tp / reps as u32);
        println!(
            "{n:>10} {ts:>12.2?} {tp:>12.2?} {:>8.2}x",
            ts.as_secs_f64() / tp.as_secs_f64()
        );
    }

    false_sharing_demo();
    println!("ALL OK");
}
