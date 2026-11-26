# Language

Ownership, lifetimes, generics, traits, closures, strings, iterators, pattern matching, macros and the newest Rust features.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

33 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`newtype_ids`](newtype_ids.rs) | newtype ids so the compiler stops you mixing things up | simple | database ids, API ids, units: stop mixing a UserId with an OrderId at compile time at zero runtime cost. | std only |
| [`cow`](cow.rs) | `Cow`: borrow when you can, allocate only when you must | simple | normalising ids/paths/HTML escaping: return borrowed data when no change is needed, allocate only on change. | std only |
| [`dyn_vs_generic`](dyn_vs_generic.rs) | trait with generics (static dispatch) vs `dyn` (dynamic dispatch), and fakes for tests | intermediate | swappable backends and test fakes: storage, clocks, mailers; generics in hot paths, dyn at plugin boundaries. | std only |
| [`iterators`](iterators.rs) | iterator pipelines and `collect` into Result / maps | simple | data pipelines: parse-validate-transform lines, group/count, windowed diffs without index loops. | std only |
| [`lifetimes`](lifetimes.rs) | lifetimes: what they are and the 5 situations you will actually hit | simple | zero-copy parsers, views over buffers, iterators that borrow: the lifetime patterns behind most Rust libraries. | std only |
| [`generics_bounds`](generics_bounds.rs) | generics, trait bounds, `impl Trait`, associated types, const generics, blanket impls | intermediate | reusable library code: generic containers, trait-bounded algorithms, extension methods on foreign types. | std only |
| [`common_traits`](common_traits.rs) | the std traits you will implement or derive constantly | simple | domain types used as map keys, sorted collections, log output, CLI parsing, defaults for config structs. | std only |
| [`conversions`](conversions.rs) | From/Into, TryFrom, AsRef, Borrow, ToString, `as`: choosing the right conversion | simple | ergonomic APIs that accept &str/String/Path, error conversion with `?`, validated construction. | std only |
| [`borrowck_patterns`](borrowck_patterns.rs) | the borrow checker: 8 common errors and how experienced Rust avoids them | intermediate | day-to-day Rust: mutate collections while reading, move out of &mut, avoid clone-everything. | std only |
| [`drop_raii`](drop_raii.rs) | `Drop` and RAII: cleanup that cannot be forgotten | simple | locks, file handles, temp files, metrics timers, transaction guards: cleanup that cannot be forgotten. | std only |
| [`smart_pointers`](smart_pointers.rs) | smart pointers: Box, Rc, Arc, Cell, RefCell, Weak, and when to use each | simple | trees with parent links (DOM, ASTs), shared config, Arc<Mutex> state, recursive types. | std only |
| [`closures_hof`](closures_hof.rs) | closures and higher-order functions: Fn / FnMut / FnOnce, returning closures, callbacks | simple | callbacks, middleware, retry wrappers, sort keys, event handlers, lazy defaults. | std only |
| [`static_const_lazy`](static_const_lazy.rs) | const, static, OnceLock, LazyLock, thread_local, and global atomics | simple | global config, lookup tables built once, id generators, per-thread caches. | std only |
| [`strings`](strings.rs) | strings: String vs &str, UTF-8 facts, and the methods you will use daily | simple | parsing ids, paths and log lines; building messages; UTF-8-safe truncation. | std only |
| [`bytes`](bytes.rs) | bytes: `&[u8]`, endianness, bit tricks, hex, UTF-8 validation, and a varint codec | intermediate | binary file formats, network protocols, checksums, index keys, compact ids (varints in protobuf/Kafka/SQLite). | std only |
| [`io_files_paths`](io_files_paths.rs) | files, paths, `Read`/`Write`, buffering, and crash-safe writes | intermediate | saving state safely, reading config, testing IO code with in-memory readers/writers. | std only |
| [`collections_tour`](collections_tour.rs) | Vec, HashMap, HashSet, BTreeMap, VecDeque: the methods that save you loops | simple | choosing the right container: counting, grouping, deduping, sorted ranges, queues. | std only |
| [`iterator_adapters`](iterator_adapters.rs) | a tour of adapters and consumers, plus writing your own `Iterator` | intermediate | readable data pipelines, streaming lazily over large inputs, writing your own iterator types. | std only |
| [`pattern_matching`](pattern_matching.rs) | pattern matching: destructuring, guards, bindings, slice patterns, let-else | simple | parsers, state machines, error handling, walking ASTs: exhaustive matching finds every place to update. | std only |
| [`modules_visibility`](modules_visibility.rs) | modules, visibility, re-exports, and enforcing invariants with private fields | simple | structuring a crate into modules, hiding invariants behind private fields, public API surface design. | std only |
| [`generics_lifetimes_structs`](generics_lifetimes_structs.rs) | generics + lifetimes together in structs, impls, iterators, closures, and higher-ranked bounds | advanced | zero-copy views, memoizing caches, borrowed iterators, handler tables holding closures. | std only |
| [`extension_traits_ops`](extension_traits_ops.rs) | extension traits, custom iterator adapters, and operator overloading on a generic type | intermediate | itertools-style helper methods, vector/matrix math types, domain-specific method sugar. | std only |
| [`phantomdata_typed`](phantomdata_typed.rs) | PhantomData: typed ids, units, permissions, variance, auto-traits, drop check | intermediate | typed arena/graph indices, units-of-measure wrappers, read-only vs read-write handles, | std only |
| [`mem_functions`](mem_functions.rs) | std::mem and std::ptr toolbox: replace/take/swap/forget/discriminant/copy/transmute and how they combine | intermediate | `Option::take` in linked lists and trees, `mem::take` of a field inside `Drop`/`finish(self)` code, double-buffered | std only |
| [`ref_patterns`](ref_patterns.rs) | ref / ref mut in patterns, match ergonomics, as_ref/as_deref, @ bindings, partial moves | simple | `opt.as_deref()` to get `Option<&str>` from `Option<String>`, `if let Some(x) = map.get_mut(k)`, | std only |
| [`macro_rules`](macro_rules.rs) | macro_rules! in depth: repetition, fragments, tt-munchers, internal rules, hygiene, $crate | advanced | `vec!`/`format!`-style helpers, `hashmap!` literals (as in the maplit crate), newtype and error-enum boilerplate, | std only |
| [`hrtb_gats`](hrtb_gats.rs) | higher-ranked trait bounds (`for<'a>`) and generic associated types (lending iterators) | advanced | nom-style parser combinators over `&str`/`&[u8]`, `tower`/`axum` handlers generic over borrowed requests, database | std only |
| [`library_api_polish`](library_api_polish.rs) | the polish high-quality libraries apply to a public API | intermediate | reqwest/clap-style builders, `std::io::ErrorKind` (non_exhaustive), `tokio` extension traits (`AsyncReadExt`), | std only |
| [`recent_stable_language_features`](recent_stable_language_features.rs) | recent stable LANGUAGE features, each with a realistic use case and the workaround it replaces | intermediate | config/request validation chains (let chains), callbacks that await (async closures), plugin registries | std only |
| [`recent_std_apis`](recent_std_apis.rs) | recent STANDARD-LIBRARY additions, grouped by area, each with a use case and an assert | intermediate | queue/cache eviction (extract_if, pop_if), batch processing and protocol framing (chunk_by, as_chunks, | std only |
| [`const_and_generics_advanced`](const_and_generics_advanced.rs) | const evaluation and generics at the edge: tables, typed dimensions, const asserts, ZST tricks | advanced | CRC/hash/base64 tables, fixed-capacity buffers (heapless style), matrix/vector libraries, state machines | std only |
| [`edition_2024_changes`](edition_2024_changes.rs) | what EDITION 2024 changed and how it shows up in real code (before/after, with assertions) | intermediate | finding which of your Drop guards (MutexGuard, RefCell borrow, span guards) now release earlier; fixing | std only |
| [`implementing_std_traits`](implementing_std_traits.rs) | implementing the std traits for your own collection and value types | intermediate | sorted/unique collections, case-insensitive map keys (HTTP headers, identifiers), numeric newtypes | std only |
