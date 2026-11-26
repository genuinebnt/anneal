# Practice: perf

**How to use this page.** Perf knowledge is only real once you have been surprised by a number, so write each prediction (with a ratio, not just "faster") before running, and keep a `lab-notes.md` of prediction versus result.
Always `--release`, always median of at least 5 runs, always `black_box` the inputs and outputs (see `bench_harness`). Numbers differ per machine; the ratios and the reasons are the lesson.
Where a task says "assert", prefer deterministic quantities (allocation counts, sizes, instruction-visible behaviour) over timings. FlowForge stage 8 (scale): attempt your stage first, then use these techniques on your own code.

## bench_harness

**Prove you got it**
1. Predict what happens to the reported time of a loop whose result is unused when you remove `black_box`; then check in `--release` (the optimiser can delete the whole loop, giving 0 ns).
2. Measure it: run the same benchmark 30 times and compute min, median and max. Predict the spread. A spread above 20 percent on a quiet machine means your benchmark includes noise (frequency scaling, a background process, first-run page faults); find out which.

**Try next**
- [S] Add a warm-up phase and show with numbers that the first sample is slower than the median (cold cache, lazy page faults).
- [M] Add a statistical comparison function `faster(a, b)` that only reports a win when the 25th to 75th percentile ranges do not overlap; test with two identical closures (must say "no difference" in 95+ of 100 trials).
- [A] Re-do three earlier recipe comparisons (pick from this chapter) in [criterion](https://github.com/bheisler/criterion.rs) and check that your harness and criterion agree on the ranking; document any disagreement and cause.

**Use it in**: any "is this faster?" PR. Real practice: rustc's own [rustc-perf](https://github.com/rust-lang/rustc-perf) benchmarks, and the [Rust Performance Book](https://github.com/nnethercote/perf-book).

**Pitfall**: benchmarking in debug mode; and benchmarking with input that is identical every iteration so the branch predictor and cache memorise it.

## branches_vectorization

**Prove you got it**
1. Predict the speedup of the branchy filter-sum on sorted versus random data, then measure. Then predict whether a branchless version helps on random data and whether the gap on sorted data disappears.
2. Predict whether `sum::<f32>()` is vectorised and whether a 4-accumulator version is faster; then predict if the answer is bit-identical (it is not, necessarily). Check the difference and explain.

**Try next**
- [S] Replace indexing with `iter().zip()` in the dot-product and measure; then inspect assembly (`cargo asm` or `--emit asm`) and count bounds-check branches before and after.
- [M] Make the data 90 percent predictable (a pattern with 10 percent noise) and plot time versus noise level from 0 to 50 percent; the curve shape shows the predictor.
- [A] Take a real scan: count the lines of a 1 GB (or generated) text file that contain a given byte, write a scalar, an iterator, and a chunked-with-accumulators version; compare to `memchr` ([memchr](https://github.com/BurntSushi/memchr)) and explain the gap in one paragraph.

**Use it in**: scoring and filter loops in search and analytics engines. Real code: memchr's SIMD paths.

**Pitfall**: "branchless" rewrites can be slower when the branch is predictable, because you now always compute both sides.

## bufwriter

**Prove you got it**
1. Predict the time ratio of writing 1M lines via `println!`, via a locked stdout, and via `BufWriter`; redirect output to `/dev/null` and to a file and predict whether the ratio changes between the two.
2. Count syscalls: predict the `write` count for 1M tiny writes with an 8 KiB buffer (about N*size/8192), and check with `strace -c` (Linux) or `dtruss` (macOS, may need sudo); or with a counting `Write` wrapper from `buffered_io_internals`.

**Try next**
- [S] Forget `flush()` at the end of the program and observe what is lost; then rely on drop and show when drop can swallow an error.
- [M] Make a log writer that flushes on newline only if the line is level ERROR; assert (with a mock writer) the number of underlying writes for a seeded log stream.
- [A] Write an NDJSON exporter for 5M records that achieves at least 500 MB/s on your machine (profile first), reusing one buffer for serialization and writing via one `BufWriter`; report the allocation count (`counting_allocator`).

**Use it in**: CLI output. Real code: ripgrep uses its own buffered, locked output; see [BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep).

**Pitfall**: `BufWriter` around a socket delays small messages until the buffer fills; interactive protocols need an explicit flush after each reply.

## byte_order_keys

**Prove you got it**
1. Predict whether `-1i32` big-endian bytes sort before `1i32` bytes under `memcmp`; check, then apply the sign-bit flip and predict again.
2. Predict the bytes of `0x01020304u32` in LE and BE and the result of `from_le_bytes` on a BE buffer; write the answer before running.

**Try next**
- [S] Property test (seeded, 100k pairs): for random `i64`, `f64` (excluding NaN) pairs, `enc(a).cmp(&enc(b)) == a.partial_cmp(&b)`.
- [M] Handle NaN and -0.0 explicitly: define a total order (like `f64::total_cmp`) and assert your encoding matches `total_cmp` for a list of 20 special values.
- [A] Build a composite key encoder (string, then i64, then f64, with an order-preserving escape for the string terminator) and a tiny sorted key-value file using it; assert that range scans with byte comparison equal scans with typed comparison (compose with `data_structures/bplus_tree`).

**Use it in**: index and LSM keys. Real code: the [byteorder](https://github.com/BurntSushi/byteorder) crate for explicit endian reads; for key encodings in a real engine, search [TiKV](https://github.com/tikv/tikv) for its codec module and compare its approach with yours (say what you found, not what you assume).

**Pitfall**: `as`-casting or transmuting structs to bytes bakes in the host endianness and padding; always encode field by field.

## cache_locality

**Prove you got it**
1. Predict the row-major versus column-major sum time ratio for a 4096x4096 `u32` matrix, then measure. If the ratio is smaller than you expected, the hardware prefetcher or cache size on your machine is why; test with a larger matrix.
2. Predict the stride sweep curve (stride 1, 2, 4, 8, 16, 32 elements): where does time per element stop increasing, and how does that relate to the cache line size?

**Try next**
- [S] Add a stride of 4097 (bad associativity) versus 4096 and explain any difference.
- [M] Add a linked list versus Vec traversal with shuffled node order versus sequential allocation order; assert checksum equality and report ratios.
- [A] Take a graph BFS on 1M nodes using `Vec<Vec<u32>>` adjacency and re-implement with CSR (flat offsets plus targets); confirm same visit order and measure; relate the result to cache misses (use `perf stat -e cache-misses` on Linux if available).

**Use it in**: graph, matrix, and record scans. Real practice: the layout chapters of the [Rust Performance Book](https://github.com/nnethercote/perf-book) and `matrixmultiply`'s blocking in [matrixmultiply](https://github.com/bluss/matrixmultiply).

**Pitfall**: Big-O says nothing about layout; a 3x asymptotically better pointer-chasing structure often loses.

## counting_allocator

**Prove you got it**
1. Predict the allocation count of building a `String` with `format!` in a loop of 1000 versus `write!` into a reused buffer. Check with the `Counting` allocator.
2. Predict whether `Vec::with_capacity(n)` then `n` pushes allocates once; then `clone` it and predict again. Does `clear()` followed by refilling allocate?

**Try next**
- [S] Add a `#[test]`-style assertion helper `assert_allocs_at_most(n, || f())` and use it to lock in one function's allocation budget.
- [M] Find a hidden allocation in code you wrote (a `to_string`, `collect`, `format!` in a hot loop), lower the count, and show the before/after count and time.
- [A] Use [dhat](https://docs.rs/dhat) (or heaptrack) on a real program of yours; produce a table of the top 5 allocation sites with counts and a plan to remove two.

**Use it in**: tests that pin allocation budgets for hot paths. Real practice: rustc and other compilers track allocations in CI via [rustc-perf](https://github.com/rust-lang/rustc-perf).

**Pitfall**: the counter itself must not allocate or recurse; and thread-local versus global counting changes the answer under multi-threading.

## data_layout

**Prove you got it**
1. Predict `size_of` for each struct in the recipe before running, including field-order effects under `repr(Rust)` versus `repr(C)`.
2. Predict the AoS versus SoA scan time ratio for summing one field out of a 64-byte record with 10M records; change to summing all fields and predict whether SoA still wins.

**Try next**
- [S] Add a `const _: () = assert!(size_of::<TaskAos>() == N)` guard and break it on purpose to see the compile error.
- [M] Implement an AoSoA layout (chunks of 8 records, each field in an array) and compare with AoS and SoA for two access patterns (one field, all fields).
- [A] Convert a real data set (for example 1M log events) from `Vec<Event>` to columns and implement three queries (filter, group-count, top-k); show which query favours which layout, with numbers.

**Use it in**: ECS game engines and columnar analytics (compose with `database/columnar_encodings` and `database/storage_models_nsm_dsm_pax`). Real code: [Rust Performance Book type sizes](https://github.com/nnethercote/perf-book).

**Pitfall**: SoA makes random access to a whole record cost N cache misses.

## false_sharing

**Prove you got it**
1. Predict the slowdown of two threads incrementing adjacent counters versus padded counters (5-10x is the typical claim). Measure; if you see less than 1.5x, check your core topology and padding size (128 bytes on Apple Silicon).
2. Predict what happens with four threads and counters at 8-byte spacing versus 64 versus 128. Plot the three.

**Try next**
- [S] Add a third variant using `crossbeam_utils::CachePadded` ([crossbeam-utils](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-utils)) and verify the same speed as `Padded`.
- [M] Show false sharing inside one struct: an `AtomicU64` producer index next to a consumer index in a queue; measure with and without padding between them.
- [A] Build a sharded counter (one padded slot per thread, summed on read) and a striped histogram; show scaling to all cores against a single atomic; add an assertion that the sum equals total increments.

**Use it in**: per-thread metrics and queue indices. Real code: `CachePadded` in [crossbeam-utils](https://github.com/crossbeam-rs/crossbeam/tree/master/crossbeam-utils) and your own `spsc_ring_buffer` from the concurrency chapter.

**Pitfall**: padding everything wastes cache; pad only what is written by different threads in parallel.

## fast_hashing

**Prove you got it**
1. Predict the speedup of `Fx` over SipHash for `u32` keys versus 32-byte string keys. Check both: the gap usually shrinks for long keys.
2. Predict what happens to `HashMap<u64, _, Fx>` lookup time when all keys are multiples of 1024; measure and explain (low-bit collisions in hashbrown's index).

**Try next**
- [S] Count collisions: for 1M sequential and 1M random keys, print max probe length (or bucket occupancy proxy) for each hasher.
- [M] Craft a hash-flooding input for the Fx map (keys that collide) and show quadratic slowdown; then show SipHash is unaffected.
- [A] Replace a `HashMap<String, usize>` interner in a toy compiler with an arena of strings plus dense ids (compose with `memory/arena_ids` and `language/newtype_ids`); compare time and memory against Fx and SipHash versions.

**Use it in**: compilers and graph code. Real code: [rustc-hash](https://github.com/rust-lang/rustc-hash) and [hashbrown](https://github.com/rust-lang/hashbrown).

**Pitfall**: a fast hash on attacker-controlled keys is a denial-of-service bug.

## inlining_codegen

**Prove you got it**
1. Predict the time ordering: generic call, `dyn` call, function pointer, `#[inline(never)]` call, for a loop of 100M calls. Measure, then explain why `dyn` may match generic when the optimiser can devirtualise.
2. Predict whether `#[inline(always)]` on a big function helps; check binary size and time.

**Try next**
- [S] Move `step_never` into another crate in a workspace without `#[inline]` and with LTO off; show the cost, then enable `lto = "fat"` and `codegen-units = 1` in the release profile and measure again.
- [M] Use `#[cold]` on an error path and compare generated assembly (block layout) with and without it.
- [A] Take a hot function from a project, produce a table of the effect of `opt-level` (2, 3), LTO, `codegen-units`, `panic=abort`, and `target-cpu=native`; explain every change over 3 percent.

**Use it in**: release profile tuning for CLI tools. Real practice: open the release profile in the `Cargo.toml` of [ripgrep](https://github.com/BurntSushi/ripgrep) and the Perf Book's build-configuration chapter ([perf-book](https://github.com/nnethercote/perf-book)); list which settings they chose and why.

**Pitfall**: `target-cpu=native` binaries crash with SIGILL on older machines.

## matmul_optimization

**Prove you got it**
1. Predict GFLOPS for naive `ijk`, `ikj`, and blocked at N=512 and the ratio between best and naive. Then predict why the ratio is bigger at N=1024.
2. Predict the best tile size; run the sweep and compare with L1/L2 cache sizes of your CPU.

**Try next**
- [S] Add a correctness test comparing all variants against naive on odd sizes (N=1, 3, 17, 33) including non-multiples of the tile.
- [M] Write a transposed-B version for `f64` and for `i32`; explain why integer is or is not vectorised differently.
- [A] Write a mini roofline: measure memory bandwidth (a simple stream-copy) and peak flops (an FMA loop) on your machine, plot your best kernel's achieved GFLOPS against the roofline, and compare with [matrixmultiply](https://github.com/bluss/matrixmultiply) or [gemm](https://github.com/sarah-ek/gemm).

**Use it in**: ML inference and linear algebra libraries; real code: matrixmultiply's micro-kernel design.

**Pitfall**: optimising a kernel without checking numerical equality: float reordering changes results slightly, so test with a tolerance.

## perf_checklist

**Prove you got it**
1. Predict the order in which you would apply the checklist to a function that takes 2 s on 1M records; write it down, then profile and see whether your top suspect was right.
2. Predict which of the build flags in `describe_build` matter most for a pointer-heavy workload versus a numeric loop.

**Try next**
- [S] Run `cargo flamegraph` ([flamegraph](https://github.com/flamegraph-rs/flamegraph)) on any example; write down the top three frames and one hypothesis each.
- [M] Optimise a deliberately slow function (write one: nested loops, `String` clones, `Vec::contains`) in 4 steps following the checklist and record time and allocations after each step; require an overall 20x or better.
- [A] Do the same on a real project of yours and write a one-page performance report (method, before/after, what you tried that did NOT help).

**Use it in**: every optimisation PR. Real practice: the [Rust Performance Book](https://github.com/nnethercote/perf-book).

**Pitfall**: skipping step 0 (measure): optimising the function that is not on the profile.

## simd_intrinsics

**Prove you got it**
1. Predict which of the ten kernels gain least from explicit SIMD over the `portable` version and why (float reductions, early exit).
2. Predict the result of `find_byte` on a haystack where the byte is at position 0 versus absent; which is faster per byte processed and why?

**Try next**
- [S] Add tail-handling tests: lengths 0..=70 for every kernel against `scalar`.
- [M] Add a new kernel, `count_ascii_upper`, in scalar, portable and one intrinsic variant for your architecture; run the differential test on 10k random buffers.
- [A] Implement a SIMD UTF-8 validation fast path (ASCII 16 or 32 bytes at a time, fall back to scalar for non-ASCII) and compare with `std::str::from_utf8` on ASCII-heavy and multilingual text; study how [memchr](https://github.com/BurntSushi/memchr) and [portable-simd](https://github.com/rust-lang/portable-simd) structure dispatch.

**Use it in**: byte scanning, checksum, validation. Real code: memchr's architecture-specific modules.

**Pitfall**: unaligned or short tails read out of bounds; always fuzz lengths around the vector width.

## small_string_inline

**Prove you got it**
1. Predict the allocation count for creating 1M `Id`s of length 8 versus length 40; check with the counting allocator.
2. Predict `size_of::<Id>()` and whether `Option<Id>` is larger; verify.

**Try next**
- [S] Add `Eq`/`Hash`/`Ord` implementations that treat inline and heap variants identically; property test with random strings around the 22-byte boundary.
- [M] Add `Deref<Target=str>` and `From<&str>`; assert a round trip for the boundary lengths 21, 22, 23 and for multi-byte UTF-8 straddling the boundary.
- [A] Benchmark your `Id` against `String`, `Box<str>`, and `Arc<str>` (and [compact_str](https://github.com/ParkMyCar/compact_str) as a reference) for creation, clone, hash and compare across a realistic id-length distribution (mostly 8-16 bytes); state which you would pick and why.

**Use it in**: interners and symbol tables. Real code: [compact_str](https://github.com/ParkMyCar/compact_str) and [smallvec](https://github.com/servo/rust-smallvec).

**Pitfall**: an inline buffer makes moves bigger; small-value optimisation is a trade, not a free win.

## struct_packing

**Prove you got it**
1. Predict sizes for `Rust`, `C`, `CSorted`, `Packed` structs; then predict `Option<Box<T>>` and `Option<u32>` sizes.
2. Predict what the compiler says if you take `&packed.field`; produce the error or warning.

**Try next**
- [S] Add compile-time size assertions to every struct and break one by adding a field.
- [M] Bit-pack a `Task` (kind 3 bits, priority 5 bits, flags, 24-bit id) into a `u64` with getters/setters; property test round trips for 100k random values and boundary values.
- [A] Design a 16-byte index entry (key prefix, offset, length) for an on-disk index, with explicit `to_le_bytes` encoding and a fuzzed decoder; compose with `os_and_storage/mmap_files` to read entries straight from a mapped file.

**Use it in**: file formats and graph nodes. Real practice: Perf Book type-size chapter ([perf-book](https://github.com/nnethercote/perf-book)) and [bytemuck](https://github.com/Lokathor/bytemuck) for safe casts.

**Pitfall**: `repr(packed)` plus references is UB; copy fields out.

## Chapter projects

### Small (about 2 h): allocation and layout audit of a toy tokenizer
Combine `counting_allocator`, `small_string_inline`, `bench_harness`, and `text_and_parsing/zero_copy_lexer`.
- M1: baseline tokenizer returning `String` tokens; record allocation count and time on a 50 MB generated input.
- M2: switch to borrowed slices, then small-string for owned cases.
- Accept: allocations drop by at least 100x; output byte-identical across versions on 20 seeded inputs.
- Stretch: SoA token stream (kind array plus offset array).

### Intermediate (a weekend): a fast CSV/NDJSON column scanner
Combine `branches_vectorization`, `simd_intrinsics`, `cache_locality`, `bufwriter`, `byte_order_keys`, and `text_and_parsing/csv_state_machine`.
- M1: generate a 1 GB test file; implement a scalar scanner that counts rows and sums one numeric column.
- M2: memchr-style SIMD newline search; branchless field splitting.
- M3: columnar output with `BufWriter` in an order-preserving binary format.
- Accept: matches a reference implementation byte-for-byte on 50 seeded files including quoted newlines, empty fields, CRLF; reaches at least 1 GB/s on your machine or documents why not (flamegraph).
- Stretch: parallelise with rayon over chunks aligned to newline boundaries (concurrency/rayon_data_parallelism) and measure scaling.

### Advanced (a week): a tiny column-store query engine with a performance budget
Combine `data_layout`, `matmul_optimization` (blocking ideas), `fast_hashing`, `false_sharing`, `inlining_codegen`, `perf_checklist`, plus `database/columnar_encodings`, `database/volcano_vectorized_push`, and `concurrency/rayon_data_parallelism`.
- M1: a typed column store with dictionary and run-length encodings.
- M2: vectorised filter/aggregate operators over batches of 1024 values.
- M3: hash group-by with a custom open-addressing table and Fx-style hash; thread-local partials merged at the end (no false sharing).
- M4: a profile-driven tuning log with at least 6 experiments, including 2 that failed.
- Accept: TPC-H-style Q1 and Q6 equivalents on 10M generated rows match a naive row-store oracle exactly; per-row cost under 20 ns for Q6 on your machine; allocation count during query execution is constant (asserted) regardless of input size.
- Stretch: explicit SIMD for the filter kernel with runtime dispatch.
