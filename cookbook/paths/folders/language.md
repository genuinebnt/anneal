# Chapter `language`: suggested reading order

Ordered simple -> intermediate -> advanced (then alphabetical). Skim `Use in:` and `COMMON USES` in each header first. Cross-chapter order is in [the tracks](../tracks/).

| # | Recipe | Level | What it is |
|---|---|---|---|
| 1 | [`closures_hof`](../../examples/language/closures_hof.rs) | simple | closures and higher-order functions: Fn / FnMut / FnOnce, returning closures, callbacks. |
| 2 | [`collections_tour`](../../examples/language/collections_tour.rs) | simple | Vec, HashMap, HashSet, BTreeMap, VecDeque: the methods that save you loops. |
| 3 | [`common_traits`](../../examples/language/common_traits.rs) | simple | the std traits you will implement or derive constantly. |
| 4 | [`conversions`](../../examples/language/conversions.rs) | simple | From/Into, TryFrom, AsRef, Borrow, ToString, `as`: choosing the right conversion. |
| 5 | [`cow`](../../examples/language/cow.rs) | simple | `Cow`: borrow when you can, allocate only when you must. |
| 6 | [`drop_raii`](../../examples/language/drop_raii.rs) | simple | `Drop` and RAII: cleanup that cannot be forgotten. |
| 7 | [`iterators`](../../examples/language/iterators.rs) | simple | iterator pipelines and `collect` into Result / maps. |
| 8 | [`lifetimes`](../../examples/language/lifetimes.rs) | simple | lifetimes: what they are and the 5 situations you will actually hit. |
| 9 | [`modules_visibility`](../../examples/language/modules_visibility.rs) | simple | modules, visibility, re-exports, and enforcing invariants with private fields. |
| 10 | [`newtype_ids`](../../examples/language/newtype_ids.rs) | simple | newtype ids so the compiler stops you mixing things up. |
| 11 | [`pattern_matching`](../../examples/language/pattern_matching.rs) | simple | pattern matching: destructuring, guards, bindings, slice patterns, let-else. |
| 12 | [`ref_patterns`](../../examples/language/ref_patterns.rs) | simple | ref / ref mut in patterns, match ergonomics, as_ref/as_deref, @ bindings, partial moves. |
| 13 | [`smart_pointers`](../../examples/language/smart_pointers.rs) | simple | smart pointers: Box, Rc, Arc, Cell, RefCell, Weak, and when to use each. |
| 14 | [`static_const_lazy`](../../examples/language/static_const_lazy.rs) | simple | const, static, OnceLock, LazyLock, thread_local, and global atomics. |
| 15 | [`strings`](../../examples/language/strings.rs) | simple | strings: String vs &str, UTF-8 facts, and the methods you will use daily. |
| 16 | [`borrowck_patterns`](../../examples/language/borrowck_patterns.rs) | intermediate | the borrow checker: 8 common errors and how experienced Rust avoids them. |
| 17 | [`bytes`](../../examples/language/bytes.rs) | intermediate | bytes: `&[u8]`, endianness, bit tricks, hex, UTF-8 validation, and a varint codec. |
| 18 | [`dyn_vs_generic`](../../examples/language/dyn_vs_generic.rs) | intermediate | trait with generics (static dispatch) vs `dyn` (dynamic dispatch), and fakes for tests. |
| 19 | [`edition_2024_changes`](../../examples/language/edition_2024_changes.rs) | intermediate | what EDITION 2024 changed and how it shows up in real code (before/after, with assertions). |
| 20 | [`extension_traits_ops`](../../examples/language/extension_traits_ops.rs) | intermediate | extension traits, custom iterator adapters, and operator overloading on a generic type. |
| 21 | [`generics_bounds`](../../examples/language/generics_bounds.rs) | intermediate | generics, trait bounds, `impl Trait`, associated types, const generics, blanket impls. |
| 22 | [`implementing_std_traits`](../../examples/language/implementing_std_traits.rs) | intermediate | implementing the std traits for your own collection and value types |
| 23 | [`io_files_paths`](../../examples/language/io_files_paths.rs) | intermediate | files, paths, `Read`/`Write`, buffering, and crash-safe writes. |
| 24 | [`iterator_adapters`](../../examples/language/iterator_adapters.rs) | intermediate | a tour of adapters and consumers, plus writing your own `Iterator`. |
| 25 | [`library_api_polish`](../../examples/language/library_api_polish.rs) | intermediate | the polish high-quality libraries apply to a public API. |
| 26 | [`mem_functions`](../../examples/language/mem_functions.rs) | intermediate | std::mem and std::ptr toolbox: replace/take/swap/forget/discriminant/copy/transmute and how they combine. |
| 27 | [`phantomdata_typed`](../../examples/language/phantomdata_typed.rs) | intermediate | PhantomData: typed ids, units, permissions, variance, auto-traits, drop check. |
| 28 | [`recent_stable_language_features`](../../examples/language/recent_stable_language_features.rs) | intermediate | recent stable LANGUAGE features, each with a realistic use case and the workaround it replaces. |
| 29 | [`recent_std_apis`](../../examples/language/recent_std_apis.rs) | intermediate | recent STANDARD-LIBRARY additions, grouped by area, each with a use case and an assert. |
| 30 | [`const_and_generics_advanced`](../../examples/language/const_and_generics_advanced.rs) | advanced | const evaluation and generics at the edge: tables, typed dimensions, const asserts, ZST tricks. |
| 31 | [`generics_lifetimes_structs`](../../examples/language/generics_lifetimes_structs.rs) | advanced | generics + lifetimes together in structs, impls, iterators, closures, and higher-ranked bounds. |
| 32 | [`hrtb_gats`](../../examples/language/hrtb_gats.rs) | advanced | higher-ranked trait bounds (`for<'a>`) and generic associated types (lending iterators). |
| 33 | [`macro_rules`](../../examples/language/macro_rules.rs) | advanced | macro_rules! in depth: repetition, fragments, tt-munchers, internal rules, hygiene, $crate. |
