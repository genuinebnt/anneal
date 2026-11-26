# Track: Performance engineering

Measure first. About 3 weeks.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Measure

**Goal:** Never optimise unmeasured.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`bench_harness`](../../examples/perf/bench_harness.rs) · intermediate | A trustworthy micro-benchmark. |
| 1.2 | [`counting_allocator`](../../examples/perf/counting_allocator.rs) · advanced | Count allocations. |
| 1.3 | [`perf_checklist`](../../examples/perf/perf_checklist.rs) · simple | Order of investigation. |

### Step 2: Memory behaviour

**Goal:** Most speed-ups live here.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`cache_locality`](../../examples/perf/cache_locality.rs) · intermediate | Row vs column, strides. |
| 2.2 | [`false_sharing`](../../examples/perf/false_sharing.rs) · advanced | Cache lines across threads. |
| 2.3 | [`data_layout`](../../examples/perf/data_layout.rs) · intermediate | SoA vs AoS. |
| 2.4 | [`small_string_inline`](../../examples/perf/small_string_inline.rs) · advanced | Avoid allocations. |
| 2.5 | [`byte_order_keys`](../../examples/perf/byte_order_keys.rs) · intermediate | Order-preserving encodings. |

### Step 3: CPU behaviour

**Goal:** Branches and SIMD.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`branches_vectorization`](../../examples/perf/branches_vectorization.rs) · intermediate | Help the autovectoriser. |
| 3.2 | [`simd_intrinsics`](../../examples/perf/simd_intrinsics.rs) · advanced | Explicit SIMD. |
| 3.3 | [`matmul_optimization`](../../examples/perf/matmul_optimization.rs) · intermediate | The classic ladder. |
| 3.4 | [`inlining_codegen`](../../examples/perf/inlining_codegen.rs) · advanced | Read the assembly. |
| 3.5 | [`fast_hashing`](../../examples/perf/fast_hashing.rs) · intermediate | Hasher choice. |
| 3.6 | [`bufwriter`](../../examples/perf/bufwriter.rs) · simple | I/O batching. |
