# Chapter `perf`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`bufwriter`](../../examples/perf/bufwriter.rs) | simple | buffered output (and why `println!` in a loop is slow). |
| 2 | [`perf_checklist`](../../examples/perf/perf_checklist.rs) | simple | the performance workflow and checklist: what to try, in what order, and how to verify. |
| 3 | [`bench_harness`](../../examples/perf/bench_harness.rs) | intermediate | measuring properly: warm-up, many samples, median, `black_box`, release mode. |
| 4 | [`branches_vectorization`](../../examples/perf/branches_vectorization.rs) | intermediate | branch prediction, branchless code, bounds checks, and auto-vectorisation (SIMD) friendliness. |
| 5 | [`byte_order_keys`](../../examples/perf/byte_order_keys.rs) | intermediate | byte order: endianness, memcmp-sortable keys, bulk byte swaps, unaligned reads, and `align_to`. |
| 6 | [`cache_locality`](../../examples/perf/cache_locality.rs) | intermediate | cache locality: row vs column order, the stride experiment, pointer chasing, and blocked algorithms. |
| 7 | [`data_layout`](../../examples/perf/data_layout.rs) | intermediate | memory layout: struct size/padding, niches, and array-of-structs vs struct-of-arrays. |
| 8 | [`fast_hashing`](../../examples/perf/fast_hashing.rs) | intermediate | the default HashMap hasher (SipHash) vs a fast non-cryptographic one, and when NOT to swap it. |
| 9 | [`matmul_optimization`](../../examples/perf/matmul_optimization.rs) | intermediate | matrix multiplication optimisation ladder: loop order, transposition, tiling, register blocking, SIMD, threads. |
| 10 | [`struct_packing`](../../examples/perf/struct_packing.rs) | intermediate | struct layout: padding, field order, repr(C/packed/align), niches, bit-packing, and compile-time size checks. |
| 11 | [`counting_allocator`](../../examples/perf/counting_allocator.rs) | advanced | count heap allocations with a custom global allocator, then remove them. |
| 12 | [`false_sharing`](../../examples/perf/false_sharing.rs) | advanced | false sharing: two threads, two separate counters, one cache line, 5-10x slower. |
| 13 | [`inlining_codegen`](../../examples/perf/inlining_codegen.rs) | advanced | inlining, `#[inline]`, `#[cold]`, dynamic dispatch cost, and the build settings that matter. |
| 14 | [`simd_intrinsics`](../../examples/perf/simd_intrinsics.rs) | advanced | explicit SIMD with `std::arch`: NEON and SSE2/AVX2 kernels, runtime dispatch, and tail handling. |
| 15 | [`small_string_inline`](../../examples/perf/small_string_inline.rs) | advanced | small-value optimisation: store short strings INLINE and avoid the heap entirely. |
