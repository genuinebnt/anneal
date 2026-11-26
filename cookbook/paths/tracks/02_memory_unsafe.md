# Track: Memory, layout and unsafe

Only after track 01. About 3 weeks.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Layout and arenas (safe)

**Goal:** See where bytes go before using unsafe.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`data_layout`](../../examples/perf/data_layout.rs) · intermediate | Size, alignment, padding. |
| 1.2 | [`struct_packing`](../../examples/perf/struct_packing.rs) · intermediate | Field order and packing. |
| 1.3 | [`arena_ids`](../../examples/memory/arena_ids.rs) · simple | Vec + index instead of pointers. |
| 1.4 | [`slab_generational`](../../examples/memory/slab_generational.rs) · advanced | Reusing slots safely with generation counters. |
| 1.5 | [`bump_arena_allocator`](../../examples/memory/bump_arena_allocator.rs) · advanced | Allocation as pointer bump. |

### Step 2: Unsafe core

**Goal:** Know the contracts the compiler stops checking.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`unsafe_basics`](../../examples/unsafe_ffi/unsafe_basics.rs) · intermediate | The five unsafe powers and SAFETY comments. |
| 2.2 | [`unsafecell_cells`](../../examples/unsafe_ffi/unsafecell_cells.rs) · advanced | Interior mutability foundations. |
| 2.3 | [`maybeuninit_fixedvec`](../../examples/unsafe_ffi/maybeuninit_fixedvec.rs) · advanced | Uninitialised memory without UB. |
| 2.4 | [`nonnull_linked_list`](../../examples/unsafe_ffi/nonnull_linked_list.rs) · advanced | Raw pointers in a data structure. |
| 2.5 | [`phantomdata_typed`](../../examples/language/phantomdata_typed.rs) · intermediate | Variance and ownership markers. |
| 2.6 | [`pin_projection`](../../examples/unsafe_ffi/pin_projection.rs) · advanced | Why Pin exists. |
| 2.7 | [`repr_layout_ffi`](../../examples/unsafe_ffi/repr_layout_ffi.rs) · intermediate | repr(C), packed, transparent. |

### Step 3: Applying it

**Goal:** Allocators and boundaries.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`size_class_allocator`](../../examples/memory/size_class_allocator.rs) · advanced | A malloc in miniature. |
| 3.2 | [`garbage_collectors`](../../examples/memory/garbage_collectors.rs) · advanced | Tracing collectors. |
| 3.3 | [`ffi_c_interop`](../../examples/unsafe_ffi/ffi_c_interop.rs) · advanced | Calling and being called by C. |
| 3.4 | [`cb-cdylib-c-abi`](../../crates/cb-cdylib-c-abi/) · crate | A real cdylib crate. |
| 3.5 | [`rust_unsafe_memory_layout_qa`](../../examples/interview/rust_unsafe_memory_layout_qa.rs) · advanced | Self-test. |
