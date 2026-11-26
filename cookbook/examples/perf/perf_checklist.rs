//! RECIPE 91 (perf): the performance workflow and checklist: what to try, in what order, and how to verify.
//! LEVEL: simple
//! Use in: any time something is "too slow"; stage 8 and every later benchmark.   Run: cargo run --release --example perf_checklist
//! COMMON USES: the workflow for any perf work: measure, algorithm, allocations, layout, branches, build flags, parallelism.
#![allow(dead_code)]
//!
//! Order of operations (cheapest and highest-impact first):
//!   0. MEASURE in --release with a realistic input (bench_harness). Profile before touching code: `cargo flamegraph` (needs `debug = 1`).
//!   1. Algorithm and data structure: O(n^2) -> O(n log n); Vec scan -> HashSet/index (bench_harness); dense ids instead of strings (index_graph).
//!   2. Allocations: count them (counting_allocator), `with_capacity`, reuse buffers, avoid `clone`/`format!` in loops, small-value storage (small_string_inline).
//!   3. Memory layout and access order: struct size (struct_packing), SoA (data_layout), cache-friendly traversal (cache_locality), no false sharing (false_sharing).
//!   4. Branches and bounds checks; SIMD-friendly loops (branches_vectorization); byte-order-friendly formats (byte_order_keys).
//!   5. Dispatch and inlining: generics over `dyn` in hot loops, `#[inline]` for tiny lib functions, `#[cold]` error paths (inlining_codegen).
//!   6. Build settings: lto, codegen-units=1, panic=abort, target-cpu, PGO, allocator (mimalloc/jemalloc).
//!   7. Parallelism: rayon for data-parallel CPU work https://docs.rs/rayon ; tokio for IO concurrency (async_patterns). Measure scaling.
//! Never skip step 0. Verify EVERY change with the same benchmark and an equality check on the output.
//! Read: https://nnethercote.github.io/perf-book/ and https://github.com/flamegraph-rs/flamegraph
use std::hint::black_box;

fn describe_build() {
    println!("build info:");
    println!("  optimised build        : {}", !cfg!(debug_assertions));
    println!(
        "  target arch / os       : {} / {}",
        std::env::consts::ARCH,
        std::env::consts::OS
    );
    println!("  pointer width          : {} bits", usize::BITS);
    println!(
        "  endianness             : {}",
        if cfg!(target_endian = "little") {
            "little"
        } else {
            "big"
        }
    );
    println!(
        "  debug assertions       : {} (integer overflow checks follow this by default)",
        cfg!(debug_assertions)
    );
    #[cfg(target_arch = "x86_64")]
    println!(
        "  avx2 available (runtime): {}",
        is_x86_feature_detected!("avx2")
    );
    #[cfg(target_arch = "aarch64")]
    println!(
        "  neon available          : {}",
        std::arch::is_aarch64_feature_detected!("neon")
    );
    println!(
        "  available parallelism  : {:?}",
        std::thread::available_parallelism().map(|n| n.get())
    );
}

/// A scoring helper you can adapt: run the same closure N times, return the median ns, black_box the result.
fn median_ns<R>(runs: usize, mut f: impl FnMut() -> R) -> u128 {
    let mut ts: Vec<u128> = (0..runs)
        .map(|_| {
            let t = std::time::Instant::now();
            black_box(f());
            t.elapsed().as_nanos()
        })
        .collect();
    ts.sort_unstable();
    ts[ts.len() / 2]
}

fn main() {
    describe_build();
    println!(
        "\nrelease profile used by ripgrep (see its Cargo.toml): release keeps `debug = 1` for profiling; a separate"
    );
    println!(
        "`release-lto` profile (opt-level 3, strip symbols, no debug assertions) is used for shipped binaries."
    );
    println!("\nexample measurement: sorting 100k numbers, median of 15 runs");
    let base: Vec<u32> = (0..100_000u32)
        .map(|i| i.wrapping_mul(2_654_435_761))
        .collect();
    let stable = median_ns(15, || {
        let mut v = base.clone();
        v.sort();
        v
    });
    let unstable = median_ns(15, || {
        let mut v = base.clone();
        v.sort_unstable();
        v
    });
    println!(
        "  sort          : {stable} ns\n  sort_unstable : {unstable} ns   (unstable sort skips the stability guarantee and is usually faster)"
    );
    println!(
        "\nchecklist: measure -> algorithm -> allocations -> layout -> branches -> dispatch/inlining -> build flags -> parallelism"
    );
}
