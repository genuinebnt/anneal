# 08. Performance and HPC

Prerequisite track: `paths/tracks/05_performance.md` (plus `02_memory_unsafe.md`). Recipe stems are `folder/stem` under `examples/`. 🔜 = planned, not written yet.

House rules for every project here:
1. **Correct first.** Keep a naive version as an oracle and compare outputs on seeded random inputs before timing anything.
2. **Predict, then measure.** Write your predicted ranking of variants in `NOTES.md` before running `--release`.
3. **Report medians of at least 11 runs** with the machine, CPU model, `rustc --version`, and `opt-level`/`lto`/`target-cpu` settings. Never assert on timing in `cargo test`; keep numbers in `bench/` outputs.
4. **Explain every surprise** with a profile (`cargo flamegraph`, `perf stat`, or `samply`) or a counter (cache-misses, branch-misses).

---

## 8.1 `benchlab`: a trustworthy micro-benchmark harness  `S`

**Goal.** A small harness that runs a closure in batches, calibrates iteration count for about 100 ms per sample, collects 30 samples, reports median, MAD, min, and detects outliers, with `black_box` discipline and a `--compare old.json new.json` mode.
**Learn.** Why naive `Instant::now()` loops lie (dead-code elimination, frequency scaling, warm-up), robust statistics, `black_box`, coordinated omission awareness.
**Read before.** `perf/bench_harness`, `perf/perf_checklist`. **During.** `perf/inlining_codegen`, `dev_tools/testing_patterns`.
**Milestones.**
1. Run-batch loop with calibration; `black_box` on inputs and outputs.
2. Median and MAD; remove samples beyond 3 MAD; print `ns/iter +/- MAD`.
3. JSON output and a compare mode that flags changes over 5% only if intervals do not overlap.
4. Demonstrate three benchmarking lies with code: result unused (optimized away), constant-folded input, cold cache first iteration.
5. Add an allocation counter column using the counting allocator recipe.
**Acceptance.** Benchmarking the same function twice yields medians within 3% on a quiet machine; a function that does nothing reports under 2 ns, and with `black_box` removed the harness warns it measured 0; comparing identical runs reports "no change" in 95% of 100 trials.
**Data.** Five kernels: sum of 1M `u64`, `sort` of 100k, `HashMap` insert of 100k, `format!` of a float, clone of a 1 KiB `Vec`.
**Write-up.** Why median instead of mean? How would CPU frequency scaling bias results and how can you detect it?
**Stretch.** Cycle counting with `rdtsc`/`cntvct`; statistical test (Mann-Whitney U) for comparisons.
**Resembles.** [Criterion.rs](https://github.com/bheisler/criterion.rs), [hyperfine](https://github.com/sharkdp/hyperfine).

## 8.2 `strfast`: small-string and interning micro-optimizations  `S`

**Goal.** Build a `SmallStr` that stores up to 22 bytes inline in 24 bytes (like `compact_str`) and a `StringInterner` returning `u32` symbols, then measure the effect on a word-frequency workload.
**Learn.** Enum layout and niche optimization, `repr`, `size_of`, branch cost of inline-vs-heap check, hash cost, cache effects of pointer chasing.
**Read before.** `perf/small_string_inline`, `perf/data_layout`, `perf/struct_packing`. **During.** `perf/fast_hashing`, `language/strings`, `perf/counting_allocator`.
**Milestones.**
1. `SmallStr` with `Deref<Target=str>`, `size_of == 24`, `Option<SmallStr>` also 24.
2. Equality, hash and ordering identical to `String` (test on 100,000 random strings of length 0-40 including multi-byte).
3. Interner using a `HashMap<Box<str>, u32>` + `Vec<Box<str>>`; then arena-backed (one big buffer + offsets).
4. Word-frequency benchmark on 50 MB text with `String`, `SmallStr`, interned ids.
5. Count allocations in each variant.
**Acceptance.** Differential equality to `String` ops; allocation count for SmallStr variant less than 20% of the `String` variant on English-like text (most words under 23 bytes); report speedups with the harness from 8.1.
**Data.** Zipf-weighted text generator with realistic word-length distribution (mean 4.7); optionally Project Gutenberg texts.
**Write-up.** Why does interning make equality O(1) but hurt locality? What are the soundness requirements of any unsafe in your inline representation?
**Stretch.** `Cow<'static, str>`-like variant; thread-safe interner with sharded locks.
**Resembles.** [rustc symbol interner](https://github.com/rust-lang/rust) (`rustc_span::symbol`), [oxc allocator/atoms](https://github.com/oxc-project/oxc).

## 8.3 `gemm`: matrix multiply from naive to blocked and SIMD  `M`

**Goal.** Multiply 1024x1024 f32 matrices through six versions (ijk, ikj, transposed-B, blocked/tiled, blocked + SIMD micro-kernel, multithreaded) and reach at least 30% of your CPU's theoretical peak GFLOP/s (compute peak = cores x freq x flops/cycle).
**Learn.** Cache hierarchy, loop order, tiling parameters from L1/L2 sizes, register blocking (e.g. 4x8 micro-kernel), auto-vectorization checks, scaling across threads.
**Read before.** `perf/matmul_optimization`, `perf/cache_locality`, `perf/simd_intrinsics`. **During.** `perf/branches_vectorization`, `concurrency/rayon_data_parallelism`, `math_science/numerics_and_math`.
**Milestones.**
1. Naive ijk with an oracle (`f64` accumulate) and relative error tolerance 1e-4.
2. Reorder loops to ikj; explain the speedup using the stride analysis.
3. Tile with block size B in {16, 32, 64, 128}: graph GFLOP/s vs B and justify the winner with cache sizes.
4. Micro-kernel with 4x8 register tile using `std::arch` or `wide`; inspect the asm for `fma`.
5. Parallelize over the outer tile loop with rayon; plot 1..N threads.
**Acceptance.** Results match the oracle for sizes 1, 7, 64, 100, 1023 (non-multiples of tile); report a table of GFLOP/s per version; final version at least 20x faster than naive ijk on 1024x1024 and at least 30% of the machine's theoretical f32 peak (state the peak calculation).
**Data.** Random matrices from a seeded generator, plus identity and all-ones special cases.
**Write-up.** Why does ikj beat ijk? What limits your micro-kernel (loads, FMA throughput, register spills)?
**Stretch.** Pack panels for contiguous access (BLIS-style); compare with `matrixmultiply` crate and Accelerate/OpenBLAS.
**Resembles.** the BLIS/GotoBLAS design ("Anatomy of High-Performance Matrix Multiplication" paper) and [matrixmultiply](https://github.com/bluss/matrixmultiply).

## 8.4 `scanfast`: a SIMD byte scanner (memchr-style)  `M`

**Goal.** Implement `find_byte`, `count_byte` and `find_newlines` that beat the naive loop by at least 8x on 1 GB buffers using SSE2/AVX2 or NEON, with a portable fallback and runtime feature detection.
**Learn.** `std::arch`, `target_feature`, `is_x86_feature_detected!`, unaligned loads, tail handling, movemask + trailing_zeros, memory bandwidth bound.
**Read before.** `perf/simd_intrinsics`, `perf/branches_vectorization`, `unsafe_ffi/unsafe_basics`. **During.** `algorithms/bit_tricks`, `perf/cache_locality`, `os_and_storage/mmap_files`.
**Milestones.**
1. Scalar loop and a SWAR (u64 bit-trick) version; both oracle-tested.
2. 16-byte vector version with tail handling; test lengths 0..100 and all alignments 0..63.
3. 32-byte (AVX2) or 2x unrolled 64 B loop; compare.
4. Runtime dispatch via function pointer cached in an atomic (or `OnceLock`).
5. `count_newlines` of a 1 GB file via `mmap`; compare with `wc -l`.
**Acceptance.** For every (length 0..300, alignment 0..63, needle position incl. absent) combination the result equals the naive version; speedup table; throughput within 2x of memory bandwidth (measure with a plain `memcpy` baseline).
**Data.** Random buffers with controlled needle density (0%, 0.1%, 10%, 50%) because density changes which strategy wins.
**Write-up.** Why is the tail handling the source of most bugs? Where is dispatch overhead relevant (short inputs)?
**Stretch.** Two-byte `memmem` with the "generic SIMD" algorithm by Wojciech Muła; Teddy-style multi-pattern prefilter.
**Resembles.** [memchr](https://github.com/BurntSushi/memchr), [aho-corasick](https://github.com/BurntSushi/aho-corasick).

## 8.5 `colscan`: row vs column layouts and branchless filters  `M`

**Goal.** Take a 20M-record table and compare AoS (`Vec<Row>`), SoA (separate `Vec`s) and hybrid layouts for queries `sum(amount) where country=X and ts in range`, using branchy and branchless filter implementations.
**Learn.** Memory bandwidth vs compute, struct padding, prefetching, branch misprediction at 50% selectivity, selection vectors, `perf stat` counters.
**Read before.** `perf/data_layout`, `perf/branches_vectorization`, `perf/struct_packing`. **During.** `database/storage_models_nsm_dsm_pax`, `perf/cache_locality`, `data_structures/bitset_rank_select_roaring`.
**Milestones.**
1. Generators for 20M rows (ts, user_id, country u8, amount f32, flags u16).
2. AoS naive; SoA naive; verify equal sums.
3. Branchless predicate: `mask = (country == x) & (ts >= a) & (ts < b)` then multiply/accumulate.
4. Selection-vector variant (indices of matches) vs bitmask; crossover selectivity found experimentally.
5. Measure `branch-misses` and `cache-misses` via `perf stat` (Linux) or Instruments (macOS) at selectivities 0.1%, 1%, 10%, 50%, 90%.
**Acceptance.** All variants return bit-exact results (use integer cents, not float, for exactness); a chart/table showing the branchy variant is slowest near 50% and the branchless is flat; SoA at least 3x faster than AoS when only 2 of 5 columns are read.
**Data.** The generator above with `country` uniform over 30 values for controllable selectivity.
**Write-up.** Where does the selection vector beat the bitmask and vice versa? How does row width change AoS cost?
**Stretch.** Run it through your 3.7 column file and compare; software prefetch experiments.
**Resembles.** [DuckDB vectorized engine](https://github.com/duckdb/duckdb), [Polars](https://github.com/pola-rs/polars).

## 8.6 `allocs`: an allocator toolbox with a counting wrapper  `M`

**Goal.** Implement three `GlobalAlloc`/`Allocator`-style allocators: bump (arena with reset), slab (fixed-size classes with free lists), and a size-class allocator with thread-local caches; measure each against the system allocator on three allocation traces.
**Learn.** Alignment, fragmentation, free-list design, thread-local caches and cross-thread frees, `GlobalAlloc` safety contract, Miri on allocators, tracing allocation traces.
**Read before.** `memory/bump_arena_allocator`, `memory/slab_generational`, `memory/size_class_allocator`. **During.** `perf/counting_allocator`, `memory/arena_ids`, `unsafe_ffi/maybeuninit_fixedvec`, `memory/garbage_collectors`.
**Milestones.**
1. Counting wrapper around `System` reporting allocs, frees, bytes, peak.
2. Bump allocator with alignment handling and `reset`; property test: no overlapping returned ranges (sort and check) for 100,000 random sizes/aligns.
3. Slab for 64-byte objects; memory reuse after free; no double-free via debug generation checks.
4. Size-class allocator (16, 32, ..., 1024, large = system) with per-thread caches (batch of 32 returned to a global list).
5. Benchmark on traces: (a) parse-and-drop AST nodes, (b) 1M small `Box`es freed in random order, (c) producer/consumer cross-thread free.
**Acceptance.** Miri passes on single-threaded tests; overlap checker and alignment checker never fail across 1,000 seeded runs; report ns/alloc and peak RSS for each allocator and trace; bump is at least 5x faster than system on trace (a).
**Data.** Traces generated from distributions: sizes log-normal (median 48 B), lifetimes exponential; the 3 scenarios above.
**Write-up.** What does your allocator do on cross-thread free? How does fragmentation appear in your measurements?
**Stretch.** Typed arena with `Drop` support; compare with `bumpalo`; huge-page backing via `mmap`.
**Resembles.** [oxc allocator](https://github.com/oxc-project/oxc), [jemalloc](https://github.com/jemalloc/jemalloc).

## 8.7 `onebrc`: the One Billion Row Challenge, in Rust  `L`

**Goal.** Read a 13 GB file of `station;temperature` lines and print `min/mean/max` per station sorted by name, as fast as possible, iterating from a 3-minute baseline to under 10 seconds on your machine.
**Learn.** Systematic optimization workflow: profile, fix the top item, repeat. `mmap`, chunking by newline boundaries, parallelism, custom integer parsing (no floats), custom hash table, SIMD delimiter search.
**Read before.** `os_and_storage/mmap_files`, `perf/fast_hashing`, `perf/perf_checklist`. **During.** `perf/simd_intrinsics`, `data_structures/open_addressing_map`, `concurrency/rayon_data_parallelism`, `perf/bufwriter`.
**Milestones.**
1. Baseline: `BufRead::lines`, `split`, `f64::parse`, `HashMap<String, Stats>` (record the time on a 10M-row file first).
2. Bytes not `String`; parse temperature as fixed-point `i16` (tenths).
3. `mmap` + manual newline scan; avoid allocation per line.
4. Custom open-addressing table keyed by hash of the station bytes (about 413 distinct stations, 16-bit-friendly).
5. Split the file into T chunks aligned to newlines; per-thread tables merged at the end.
6. SIMD for `;` and `\n` search; measure each step's gain in a table.
**Acceptance.** Output byte-identical to the baseline on 1M, 10M and 1B rows (rounding rule: round half up to 1 decimal as in the official spec); each optimization step recorded with before/after times; final within 10x of memory-read speed (`cat file > /dev/null`).
**Data.** The official generator (`gunnarmorling/1brc` `create_measurements`) or write your own with 413 stations and normal-distributed temperatures in -99.9..99.9.
**Write-up.** What did the profile say at each step and which assumption (e.g. about station count) did you exploit? Which tricks are unsafe for general inputs?
**Stretch.** Handle arbitrary stations robustly (no 413 assumption) and note the cost; compare with `polars` and DuckDB.
**Resembles.** The [1BRC challenge](https://github.com/gunnarmorling/1brc) and [DuckDB](https://github.com/duckdb/duckdb) CSV scanning.

## 8.8 `pipeq`: a lock-free pipeline at 10M messages/s  `L`

**Goal.** A 4-stage pipeline (generate -> parse -> transform -> aggregate) connected by SPSC ring buffers pinned to threads, with batched transfers, cache-line-padded indices, and a design that sustains at least 10M msgs/s while preserving order and exactly-once delivery.
**Learn.** SPSC protocol, batching, false sharing, cached head/tail, thread pinning (affinity), NUMA awareness, tail latency under load, back-off strategies (spin, yield, park).
**Read before.** `concurrency/spsc_ring_buffer`, `perf/false_sharing`, `concurrency/memory_ordering_litmus`. **During.** `concurrency/mpmc_bounded_queue`, `perf/cache_locality`, `os_and_storage/scheduler_policies`, `concurrency/seqlock_and_priority_inversion`.
**Milestones.**
1. Single-threaded pipeline as the oracle (checksum of all outputs).
2. Threaded with `crossbeam::channel`; record msgs/s and p99 latency per message (sequence-stamped).
3. Replace with your SPSC rings (item at a time).
4. Batch 32 items per publish; show throughput/latency trade-off curve for batch sizes 1, 8, 32, 128.
5. Pad indices to 128 bytes (Apple M-series) or 64 (x86); remove padding to see the regression.
6. Back-off strategy comparison (busy-spin vs `yield_now` vs park after 1,000 spins) on CPU use and tail latency.
**Acceptance.** Checksum equals oracle over 1B messages in 5 runs; at least 10M msgs/s (report the machine); p99 per-message latency reported in a histogram; zero loss/duplication by sequence numbers; Miri/loom pass on a reduced ring.
**Data.** Messages `{seq: u64, payload: [u8; 24]}`; a skewed workload where stage 3 is 5x slower to show queue buildup.
**Write-up.** Where is the real bottleneck (coherence traffic, parsing, allocation)? Why does batching trade latency for throughput?
**Stretch.** Disruptor-style multi-consumer sequencing; compare with `crossbeam` `ArrayQueue`.
**Resembles.** LMAX Disruptor; [crossbeam](https://github.com/crossbeam-rs/crossbeam).

---

## Katas (30 minutes each)

1. Show `size_of` for a struct with fields ordered `(u8, u64, u8, u32)` vs reordered by size; assert the difference and explain padding with `#[repr(C)]` vs default.
2. Sum a `Vec<u32>` of 100M with an iterator, a manual loop, and 4 independent accumulators; report which is fastest and why (dependency chain).
3. Time `Vec::push` with and without `with_capacity` for 10M elements; count reallocations with the counting allocator (expect about 24 vs 1).
4. Sort 1M `u64` with `sort`, `sort_unstable`, and a radix sort (8 passes of 8 bits); assert equality and report ratios.
5. Replace `x % 8` with `x & 7` on `u32` and show identical asm for unsigned but not signed; paste both asm snippets in a comment.
6. Build a branchy and a branchless `clamp` over 100M random values; measure at data sorted vs shuffled to expose branch prediction.
7. Use `perf stat` (or Instruments) to report IPC for a linked-list traversal vs a `Vec` traversal of 10M nodes; expect IPC to differ by 5x or more.
8. Write `fn popcount_slice(&[u64]) -> u64` three ways (loop of `count_ones`, SWAR, `chunks_exact(4)` unrolled); assert equal, rank by speed.
9. Demonstrate false sharing: two threads incrementing adjacent vs 128-byte-separated `AtomicU64`s, 100M increments each; report the ratio.
10. Compare `HashMap` default (SipHash) vs `FxHash`-style vs `ahash`-style for 1M `u64` and 1M 16-byte string keys; report ns/op and which hasher quality problems you can trigger.
