# Performance

Measuring, allocation counting, cache behaviour, layout, inlining, branches, hashing speed, perf workflow.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

15 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`bufwriter`](bufwriter.rs) | buffered output (and why `println!` in a loop is slow) | simple | log writers, CLI output, file exporters: buffering turns millions of syscalls into thousands. | std only |
| [`bench_harness`](bench_harness.rs) | measuring properly: warm-up, many samples, median, `black_box`, release mode | intermediate | every performance question: warm-up, median, black_box, release mode. | std only |
| [`counting_allocator`](counting_allocator.rs) | count heap allocations with a custom global allocator, then remove them | advanced | finding hidden allocations in hot paths; asserting allocation counts in tests. | std only |
| [`data_layout`](data_layout.rs) | memory layout: struct size/padding, niches, and array-of-structs vs struct-of-arrays | intermediate | columnar engines, ECS game engines, analytics: scan one field without dragging cold data through the cache. | std only |
| [`fast_hashing`](fast_hashing.rs) | the default HashMap hasher (SipHash) vs a fast non-cryptographic one, and when NOT to swap it | intermediate | graph algorithms, interning tables, compilers (rustc uses Fx hashing); not for attacker-controlled keys. | std only |
| [`cache_locality`](cache_locality.rs) | cache locality: row vs column order, the stride experiment, pointer chasing, and blocked algorithms | intermediate | anything iterating large arrays, matrices or graphs; choosing layout and traversal order. | std only |
| [`false_sharing`](false_sharing.rs) | false sharing: two threads, two separate counters, one cache line, 5-10x slower | advanced | per-thread counters/metrics, queue head/tail indices, any hot shared atomics. | std only |
| [`inlining_codegen`](inlining_codegen.rs) | inlining, `#[inline]`, `#[cold]`, dynamic dispatch cost, and the build settings that matter | advanced | hot small functions, trait-object-heavy code, deciding generics vs dyn, tuning release profiles. | std only |
| [`struct_packing`](struct_packing.rs) | struct layout: padding, field order, repr(C/packed/align), niches, bit-packing, and compile-time size checks | intermediate | millions of small records: events, graph nodes, index entries; wire/file layouts. | std only |
| [`byte_order_keys`](byte_order_keys.rs) | byte order: endianness, memcmp-sortable keys, bulk byte swaps, unaligned reads, and `align_to` | intermediate | index keys in databases (LSM/B-trees), protocols, file formats, checksums. | std only |
| [`branches_vectorization`](branches_vectorization.rs) | branch prediction, branchless code, bounds checks, and auto-vectorisation (SIMD) friendliness | intermediate | filters/scoring loops, vectorisable numeric kernels, avoiding bounds checks. | std only |
| [`small_string_inline`](small_string_inline.rs) | small-value optimisation: store short strings INLINE and avoid the heap entirely | advanced | interners, symbol tables, DB keys, tags/labels: strings that are almost always short. | std only |
| [`perf_checklist`](perf_checklist.rs) | the performance workflow and checklist: what to try, in what order, and how to verify | simple | the workflow for any perf work: measure, algorithm, allocations, layout, branches, build flags, parallelism. | std only |
| [`simd_intrinsics`](simd_intrinsics.rs) | explicit SIMD with `std::arch`: NEON and SSE2/AVX2 kernels, runtime dispatch, and tail handling | advanced | the `memchr` crate (used by ripgrep, `regex`, `bstr`), simdjson-style parsers and Lemire's SIMD UTF-8 | std only |
| [`matmul_optimization`](matmul_optimization.rs) | matrix multiplication optimisation ladder: loop order, transposition, tiling, register blocking, SIMD, threads | intermediate | the GEMM at the heart of neural-network layers (BLAS `sgemm`/`dgemm`, `ndarray`'s `dot`, the `matrixmultiply` | rayon |
