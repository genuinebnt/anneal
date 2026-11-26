//! RECIPE 89 (perf): branch prediction, branchless code, bounds checks, and auto-vectorisation (SIMD) friendliness.
//! LEVEL: intermediate
//! Use in: hot filter/sum loops, scoring, scanning arrays of numbers.   Run: cargo run --release --example branches_vectorization
//! COMMON USES: filters/scoring loops, vectorisable numeric kernels, avoiding bounds checks.
#![allow(dead_code)]
//!
//! (1) The CPU GUESSES which way an `if` goes. Random data = ~50% wrong guesses = ~15-20 cycles lost each. SORTED (or otherwise
//!     predictable) data makes the same code fast. Compilers often turn simple `if`s into branch-free `cmov`/masks.
//! (2) Indexing `v[i]` has a bounds check; iterators (`iter()`, `zip`, `chunks_exact`) let the compiler prove safety and drop it.
//! (3) Floating-point addition is NOT associative, so the compiler will not reorder `sum::<f32>()` into SIMD lanes for you.
//!     Integers can be reordered; for floats use several independent accumulators (what SIMD would do) yourself.
//! Explicit SIMD: `std::simd` (nightly), https://docs.rs/wide , https://docs.rs/pulp ; architecture intrinsics in `std::arch`.
//! Read: https://en.algorithmica.org/hpc/pipelining/branching/ and https://nnethercote.github.io/perf-book/bounds-checks.html
use std::hint::black_box;
use std::time::Instant;

fn time<R>(label: &str, f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let r = f();
    println!("{label:<36} {:>9.2?}", t.elapsed());
    r
}

fn main() {
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let data: Vec<u8> = (0..20_000_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 32) as u8
        })
        .collect();
    let mut sorted = data.clone();
    sorted.sort_unstable();

    // 1. branchy sum over random vs sorted data. The branch body calls a non-inlined function, so the compiler cannot turn
    //    it into a conditional move: the CPU must really predict it. (For trivial bodies like `s += b` modern compilers
    //    usually emit a branch-free `cmov` and the gap disappears: check with `cargo asm` before assuming a branch exists.)
    #[inline(never)]
    fn expensive(b: u8) -> u64 {
        (b as u64).wrapping_mul(0x9E37_79B9).rotate_left(7) & 0xff
    }
    fn branchy(v: &[u8]) -> u64 {
        let mut s = 0u64;
        for &b in v {
            if b >= 128 {
                s += expensive(b);
            }
        }
        s
    }
    let r1 = time("if x >= 128 (random order)", || branchy(&data));
    let r2 = time("if x >= 128 (sorted order)", || branchy(&sorted));
    assert_eq!(r1, r2);
    let r3 = time("branchless: always compute, mask (random)", || {
        data.iter()
            .map(|&b| ((b >= 128) as u64) * expensive(b))
            .sum::<u64>()
    });
    assert_eq!(r1, r3);

    // 2. bounds checks vs iterator zip
    let a: Vec<u32> = (0..10_000_000).collect();
    let b: Vec<u32> = (0..10_000_000).map(|i| i * 2).collect();
    let dot_idx = time("dot product, indexed a[i]*b[i]", || {
        let mut s = 0u64;
        for i in 0..a.len().min(b.len()) {
            s = s.wrapping_add(a[i] as u64 * b[i] as u64);
        }
        s
    });
    let dot_zip = time("dot product, zip iterator", || {
        a.iter()
            .zip(&b)
            .fold(0u64, |s, (&x, &y)| s.wrapping_add(x as u64 * y as u64))
    });
    assert_eq!(dot_idx, dot_zip);

    // 3. float sum: one accumulator (serial dependency chain) vs 8 independent lanes
    let f: Vec<f32> = (0..16_000_000).map(|i| (i % 1000) as f32 * 0.001).collect();
    let serial = time("f32 sum, single accumulator", || f.iter().sum::<f32>());
    let lanes = time("f32 sum, 8 accumulators", || {
        let mut acc = [0f32; 8];
        let mut chunks = f.chunks_exact(8);
        for c in &mut chunks {
            for k in 0..8 {
                acc[k] += c[k];
            }
        }
        acc.iter().sum::<f32>() + chunks.remainder().iter().sum::<f32>()
    });
    println!("results differ slightly because float addition order changed: {serial} vs {lanes}");
    assert!((serial - lanes).abs() / serial.abs() < 1e-2);

    let ints: Vec<u32> = (0..64_000_000u32).collect();
    let total = time("u32 sum (compiler vectorises itself)", || {
        ints.iter().fold(0u32, |a, &i| a.wrapping_add(i))
    });
    black_box(total);
}
