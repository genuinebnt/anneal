# Track: Rust foundations: from syntax to idiomatic code

About 3-4 weeks at 6 h/week.

**How to work each step:** read the header, predict the asserts, blank the file and rebuild it from memory (`./study.sh NAME`), then mutate it. Details in [HOW_TO_STUDY.md](../../HOW_TO_STUDY.md). Checkpoint = you can explain the step's goal in five lines without looking.

### Step 1: Values, ownership, strings

**Goal:** Stop fighting the borrow checker; know where data lives.

| # | Recipe | Why here |
|---|---|---|
| 1.1 | [`strings`](../../examples/language/strings.rs) · simple | String vs &str vs bytes is the first wall everyone hits. |
| 1.2 | [`bytes`](../../examples/language/bytes.rs) · intermediate | Byte slices and the useful methods. |
| 1.3 | [`borrowck_patterns`](../../examples/language/borrowck_patterns.rs) · intermediate | The handful of shapes the borrow checker accepts and rejects. |
| 1.4 | [`drop_raii`](../../examples/language/drop_raii.rs) · simple | Ownership ends at Drop; guards depend on it. |
| 1.5 | [`mem_functions`](../../examples/language/mem_functions.rs) · intermediate | take/replace/swap: how to move out of &mut safely. |

### Step 2: Collections, iterators, closures

**Goal:** Write loops as iterator pipelines; pick the right collection.

| # | Recipe | Why here |
|---|---|---|
| 2.1 | [`collections_tour`](../../examples/language/collections_tour.rs) · simple | Vec, HashMap, BTreeMap, VecDeque, BinaryHeap: when each wins. |
| 2.2 | [`iterators`](../../examples/language/iterators.rs) · simple | The Iterator contract. |
| 2.3 | [`iterator_adapters`](../../examples/language/iterator_adapters.rs) · intermediate | map/filter/fold/windows/chunks/zip combinations. |
| 2.4 | [`closures_hof`](../../examples/language/closures_hof.rs) · simple | Fn/FnMut/FnOnce and returning closures. |

### Step 3: Traits and generics

**Goal:** Design APIs with traits instead of copy-paste.

| # | Recipe | Why here |
|---|---|---|
| 3.1 | [`common_traits`](../../examples/language/common_traits.rs) · simple | Debug, Clone, Default, PartialOrd, Hash, From: derive vs hand-write. |
| 3.2 | [`conversions`](../../examples/language/conversions.rs) · simple | From/Into/TryFrom/AsRef. |
| 3.3 | [`generics_bounds`](../../examples/language/generics_bounds.rs) · intermediate | Bounds and where clauses. |
| 3.4 | [`generics_lifetimes_structs`](../../examples/language/generics_lifetimes_structs.rs) · advanced | Structs and methods holding references. |
| 3.5 | [`dyn_vs_generic`](../../examples/language/dyn_vs_generic.rs) · intermediate | Static vs dynamic dispatch; object safety. |
| 3.6 | [`implementing_std_traits`](../../examples/language/implementing_std_traits.rs) · intermediate | Make your type feel like std. |

### Step 4: Errors

**Goal:** Design the failure paths before the happy path.

| # | Recipe | Why here |
|---|---|---|
| 4.1 | [`option_result_toolbox`](../../examples/errors/option_result_toolbox.rs) · simple | Combinators and ?. |
| 4.2 | [`error_enum`](../../examples/errors/error_enum.rs) · simple | A typed error enum with From. |
| 4.3 | [`anyhow_context`](../../examples/errors/anyhow_context.rs) · simple | Applications vs libraries. |
| 4.4 | [`error_chain_retryable`](../../examples/errors/error_chain_retryable.rs) · intermediate | Sources, classification, retryable vs fatal. |

### Step 5: Structure and polish

**Goal:** Code a reviewer would merge.

| # | Recipe | Why here |
|---|---|---|
| 5.1 | [`modules_visibility`](../../examples/language/modules_visibility.rs) · simple | pub(crate), re-exports, layout. |
| 5.2 | [`newtype_ids`](../../examples/language/newtype_ids.rs) · simple | Type-safe IDs (you need this in FlowForge). |
| 5.3 | [`pattern_matching`](../../examples/language/pattern_matching.rs) · simple | Exhaustive matching and bindings. |
| 5.4 | [`smart_pointers`](../../examples/language/smart_pointers.rs) · simple | Box/Rc/Arc/RefCell/Cow in practice. |
| 5.5 | [`library_api_polish`](../../examples/language/library_api_polish.rs) · intermediate | API Guidelines checklist applied. |
| 5.6 | [`testing_patterns`](../../examples/dev_tools/testing_patterns.rs) · intermediate | Unit, property and golden tests. |
| 5.7 | [`rust_ownership_borrowing_qa`](../../examples/interview/rust_ownership_borrowing_qa.rs) · intermediate | Self-test: close the file and answer first. |
