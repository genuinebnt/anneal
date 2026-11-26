//! RECIPE 84 (perf): cache locality: row vs column order, the stride experiment, pointer chasing, and blocked algorithms.
//! LEVEL: intermediate
//! Use in: any loop over big arrays (graph adjacency, matrices, records); explains why "same Big-O" can differ 10x.   Run: cargo run --release --example cache_locality
//! COMMON USES: anything iterating large arrays, matrices or graphs; choosing layout and traversal order.
#![allow(dead_code)]
//!
//! The CPU loads memory in 64-byte CACHE LINES (16 u32s). Touching the next element is nearly free (same line, and the
//! hardware prefetcher guesses sequential access). Jumping around costs a trip to L2/L3/RAM (~4ns / ~15ns / ~80ns).
//! (On Apple Silicon the cache line is 128 bytes and the prefetcher is aggressive, so exact numbers differ from x86.)
//! Rules: iterate in memory order; keep data that is used together close together; prefer Vec<T> over linked structures;
//! process big data in BLOCKS that fit in cache. Numbers vary by machine; the RATIOS are the lesson.
//! Read: https://en.algorithmica.org/hpc/cpu-cache/ and https://nnethercote.github.io/perf-book/
use std::hint::black_box;
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn time<R>(label: &str, f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let r = f();
    println!("{label:<34} {:>9.2?}", t.elapsed());
    r
}

fn main() {
    // 1. same work, different order: row-major (contiguous) vs column-major (stride n)
    let n = 4096;
    let m: Vec<u32> = (0..n * n).map(|i| i as u32).collect(); // 64 MB
    let by_rows = time("sum rows (contiguous)", || {
        let mut s = 0u32;
        for r in 0..n {
            for c in 0..n {
                s = s.wrapping_add(m[r * n + c]);
            }
        }
        black_box(s)
    });
    let by_cols = time("sum columns (stride 16 KB)", || {
        let mut s = 0u32;
        for c in 0..n {
            for r in 0..n {
                s = s.wrapping_add(m[r * n + c]);
            }
        }
        black_box(s)
    });
    assert_eq!(by_rows, by_cols);

    // 2. stride experiment: touch every k-th u32 of a 64 MB array. We pay for CACHE LINES (16 u32s each), not elements:
    //    doing 16x fewer accesses (stride 16) does NOT make it 16x faster, because each access still pulls a whole new line.
    //    (The array is written once first so we do not measure first-touch page faults, and we take the best of 3 runs.)
    println!("\nstride experiment over 64 MB (compare time with the NUMBER of accesses):");
    let mut a = vec![1u32; 1 << 24];
    for stride in [1usize, 2, 4, 8, 16, 32, 64] {
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            let mut i = 0;
            while i < a.len() {
                a[i] = a[i].wrapping_add(1);
                i += stride;
            }
            best = best.min(t.elapsed());
        }
        println!(
            "  stride {stride:>2}: {best:>9.2?} for {:>8} accesses = {:>5.2} ns/access",
            a.len() / stride,
            best.as_secs_f64() * 1e9 / (a.len() / stride) as f64
        );
    }
    black_box(&a);

    // 3. pointer chasing (each load depends on the previous, random addresses) vs sequential
    println!();
    let len = 1 << 22; // 4M entries = 16 MB
    let mut order: Vec<u32> = (0..len as u32).collect();
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for i in (1..order.len()).rev() {
        order.swap(i, (rng.next() % (i as u64 + 1)) as usize); // random permutation
    }
    // build ONE big cycle through all slots in random order
    let mut next = vec![0u32; len];
    for w in 0..len {
        next[order[w] as usize] = order[(w + 1) % len];
    }
    let chased = time("pointer chase (random, dependent)", || {
        let mut i = 0usize;
        for _ in 0..len {
            i = next[i] as usize;
        }
        black_box(i)
    });
    let seq = time("sequential sum (same count)", || {
        let mut s = 0u64;
        for &x in &next {
            s += x as u64;
        }
        black_box(s)
    });
    let _ = (chased, seq);

    // 4. blocked (tiled) transpose: work on small tiles that stay in cache
    println!();
    let n = 2048;
    let src: Vec<u32> = (0..n * n).map(|i| i as u32).collect();
    let mut naive = vec![0u32; n * n];
    time("naive transpose", || {
        for r in 0..n {
            for c in 0..n {
                naive[c * n + r] = src[r * n + c]; // writes jump by n
            }
        }
    });
    let mut tiled = vec![0u32; n * n];
    time("tiled transpose (32x32 blocks)", || {
        const B: usize = 32;
        for rb in (0..n).step_by(B) {
            for cb in (0..n).step_by(B) {
                for r in rb..rb + B {
                    for c in cb..cb + B {
                        tiled[c * n + r] = src[r * n + c];
                    }
                }
            }
        }
    });
    assert_eq!(naive, tiled);
    println!(
        "\nlesson: same results, very different speed. Memory layout and access ORDER are algorithmic choices too."
    );
}
