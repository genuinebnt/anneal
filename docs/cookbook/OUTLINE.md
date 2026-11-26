> **A proposal, not a description of the app.** It lists everything that could go in the cookbook. For what exists now see [../COOKBOOK.md](../COOKBOOK.md).

# Cookbook outline: every module, chapter and section

Generated on 2026-11-26 from the FlowForge cookbook and the two guides (`~/dbms.html`, `~/compiler.html`). Plan: [../COOKBOOK.md](../COOKBOOK.md).
**Planned structure only; no pages are written.** A script checked that every recipe file and guide section found then is listed exactly once.
It does **not** check that a source is good, complete or in the right place: the chapter assignment is a first proposal to review.

- **Section** = one reference page. Several sources on one topic should be merged into one page when written (not decided per pair yet).
- **recipe** = a runnable cookbook file; **db guide** / **compiler guide** = a section of a guide; **planned** = no source yet.
- Recipe descriptions come from file headers and were not checked. Levels are the files' own labels.
- Resources: every link is in [resources.tsv](resources.tsv) with its type (guessed from the URL), status and where it was found.
  "loads" means HTTP 200 on 2026-11-26, **not** that the content was read or supports a claim. "unconfirmed" is mostly a 403 refusal of the check.

## Summary

| Module | Chapters | Recipes | Guide sections | Links |
|---|---|---|---|---|
| 1. Rust language | 7 | 61 | 5 | 113 |
| 2. Standard library and core | 9 | 45 | 0 | 123 |
| 3. Crates and tooling | 2 | 21 | 0 | 107 |
| 4. Testing and benchmarking | 15 | 10 | 8 | 44 |
| 5. Data structures and algorithms | 7 | 73 | 5 | 192 |
| 6. Concurrency and async | 4 | 35 | 1 | 65 |
| 7. Performance and systems | 4 | 34 | 3 | 112 |
| 8. Databases | 9 | 50 | 102 | 592 |
| 9. Search and vectors | 3 | 15 | 2 | 33 |
| 10. Distributed systems | 5 | 26 | 6 | 81 |
| 11. Web, networking and security | 5 | 46 | 0 | 115 |
| 12. Compilers and languages | 7 | 26 | 62 | 118 |
| 13. Complete projects | 2 | 22 | 0 | 46 |
| 14. Worked questions | 2 | 17 | 0 | 28 |
| 15. Appendices | 1 | 0 | 16 | 222 |

Distinct links: 1788 (load: 1689, broken: 37, unconfirmed: 62).

**Resource types still thin:** blogs and writeups are not told apart from articles by URL, and few series or books were detected. Adding them is curation work.

## 1. Rust language

The language itself: ownership, types, errors, patterns, memory and unsafe.

### 1.1 Ownership and borrowing

| # | Section | Source | About |
|---|---|---|---|
| 1.1.1 | `lifetimes` | recipe · simple | lifetimes: what they are and the 5 situations you will actually hit |
| 1.1.2 | `borrowck_patterns` | recipe · intermediate | the borrow checker: 8 common errors and how experienced Rust avoids them |
| 1.1.3 | `cow` | recipe · simple | `Cow`: borrow when you can, allocate only when you must |
| 1.1.4 | `ref_patterns` | recipe · simple | ref / ref mut in patterns, match ergonomics, as_ref/as_deref, @ bindings, partial moves |
| 1.1.5 | `drop_raii` | recipe · simple | `Drop` and RAII: cleanup that cannot be forgotten |
| 1.1.6 | `smart_pointers` | recipe · simple | smart pointers: Box, Rc, Arc, Cell, RefCell, Weak, and when to use each |
| 1.1.7 | `mem_functions` | recipe · intermediate | std::mem and std::ptr toolbox: replace/take/swap/forget/discriminant/copy/transmute and how they combine |
| 1.1.8 | `static_const_lazy` | recipe · simple | const, static, OnceLock, LazyLock, thread_local, and global atomics |

**Further resources:** 13 docs load; 0 broken, 0 unconfirmed.

### 1.2 Types, traits and generics

| # | Section | Source | About |
|---|---|---|---|
| 1.2.1 | `generics_bounds` | recipe · intermediate | generics, trait bounds, `impl Trait`, associated types, const generics, blanket impls |
| 1.2.2 | `generics_lifetimes_structs` | recipe · advanced | generics + lifetimes together in structs, impls, iterators, closures, and higher-ranked bounds |
| 1.2.3 | `hrtb_gats` | recipe · advanced | higher-ranked trait bounds (`for<'a>`) and generic associated types (lending iterators) |
| 1.2.4 | `dyn_vs_generic` | recipe · intermediate | trait with generics (static dispatch) vs `dyn` (dynamic dispatch), and fakes for tests |
| 1.2.5 | `common_traits` | recipe · simple | the std traits you will implement or derive constantly |
| 1.2.6 | `conversions` | recipe · simple | From/Into, TryFrom, AsRef, Borrow, ToString, `as`: choosing the right conversion |
| 1.2.7 | `implementing_std_traits` | recipe · intermediate | implementing the std traits for your own collection and value types |
| 1.2.8 | `extension_traits_ops` | recipe · intermediate | extension traits, custom iterator adapters, and operator overloading on a generic type |
| 1.2.9 | `phantomdata_typed` | recipe · intermediate | PhantomData: typed ids, units, permissions, variance, auto-traits, drop check |
| 1.2.10 | `newtype_ids` | recipe · simple | newtype ids so the compiler stops you mixing things up |
| 1.2.11 | `const_and_generics_advanced` | recipe · advanced | const evaluation and generics at the edge: tables, typed dimensions, const asserts, ZST tricks |
| 1.2.12 | `library_api_polish` | recipe · intermediate | the polish high-quality libraries apply to a public API |

**Further resources:** 19 docs, 1 code, 1 article or blog load; 1 broken, 0 unconfirmed.

### 1.3 Everyday language

| # | Section | Source | About |
|---|---|---|---|
| 1.3.1 | Language features from Rust | compiler guide | |
| 1.3.2 | `closures_hof` | recipe · simple | closures and higher-order functions: Fn / FnMut / FnOnce, returning closures, callbacks |
| 1.3.3 | `iterators` | recipe · simple | iterator pipelines and `collect` into Result / maps |
| 1.3.4 | `iterator_adapters` | recipe · intermediate | a tour of adapters and consumers, plus writing your own `Iterator` |
| 1.3.5 | `pattern_matching` | recipe · simple | pattern matching: destructuring, guards, bindings, slice patterns, let-else |
| 1.3.6 | `strings` | recipe · simple | strings: String vs &str, UTF-8 facts, and the methods you will use daily |
| 1.3.7 | `bytes` | recipe · intermediate | bytes: `&[u8]`, endianness, bit tricks, hex, UTF-8 validation, and a varint codec |
| 1.3.8 | `collections_tour` | recipe · simple | Vec, HashMap, HashSet, BTreeMap, VecDeque: the methods that save you loops |
| 1.3.9 | `io_files_paths` | recipe · intermediate | files, paths, `Read`/`Write`, buffering, and crash-safe writes |
| 1.3.10 | `macro_rules` | recipe · advanced | macro_rules! in depth: repetition, fragments, tt-munchers, internal rules, hygiene, $crate |
| 1.3.11 | `modules_visibility` | recipe · simple | modules, visibility, re-exports, and enforcing invariants with private fields |
| 1.3.12 | `edition_2024_changes` | recipe · intermediate | what EDITION 2024 changed and how it shows up in real code (before/after, with assertions) |
| 1.3.13 | `recent_stable_language_features` | recipe · intermediate | recent stable LANGUAGE features, each with a realistic use case and the workaround it replaces |
| 1.3.14 | `recent_std_apis` | recipe · intermediate | recent STANDARD-LIBRARY additions, grouped by area, each with a use case and an assert |

**Further resources:** 19 docs, 4 code, 1 book load; 0 broken, 0 unconfirmed.

### 1.4 Errors and diagnostics

| # | Section | Source | About |
|---|---|---|---|
| 1.4.1 | `anyhow_context` | recipe · simple | `anyhow`: context chains for application-level errors, and downcasting back to the cause |
| 1.4.2 | `diagnostics_spans` | recipe · intermediate | compiler-style diagnostics: byte spans, line/column, and a caret under the problem |
| 1.4.3 | `error_chain_retryable` | recipe · intermediate | library errors with `#[source]`, error chains, and classifying errors (retryable vs fatal) |
| 1.4.4 | `error_enum` | recipe · simple |  |
| 1.4.5 | `option_result_toolbox` | recipe · simple | the Option/Result combinator toolbox (and when to just use `match` or `?`) |

**Further resources:** 5 code, 2 docs, 1 article or blog load; 0 broken, 0 unconfirmed.

### 1.5 Design patterns

| # | Section | Source | About |
|---|---|---|---|
| 1.5.1 | `design_patterns_high` | recipe · advanced | high-level design patterns in idiomatic Rust: builder, strategy, observer, visitor, command, DI and more |
| 1.5.2 | `design_patterns_low` | recipe · advanced | low-level, data-oriented patterns: arenas, interning, SoA, zero-copy, pools, bitfields, inline storage |
| 1.5.3 | `typestate_builder` | recipe · intermediate | typestate builder: invalid states do not compile |
| 1.5.4 | `typestate_protocol` | recipe · advanced | typestate protocol: a session whose legal operations depend on its state, enforced by the compiler |
| 1.5.5 | `state_machine` | recipe · intermediate | an enum state machine where illegal transitions are impossible to write silently |
| 1.5.6 | `generic_state_machine` | recipe · advanced | a generic state-machine trait, replaying event logs, and table-driven transitions |
| 1.5.7 | `type_erasure_vtable` | recipe · advanced | type erasure side by side: `dyn Trait`, `dyn Any`, `Box<dyn Fn>`, `clone_box`, and a hand-written vtable |
| 1.5.8 | `any_event_bus` | recipe · advanced | a type-safe event bus built on `TypeId` and `dyn Any`: runtime dispatch with compile-time-typed handlers |
| 1.5.9 | `tower_service_layer` | recipe · advanced | a mini re-implementation of tower's Service/Layer model with timeout, retry, limits and a circuit breaker |
| 1.5.10 | `sans_io_protocol` | recipe · advanced | sans-IO protocol design: a pure state machine driven by a simulator, a blocking socket and tokio |
| 1.5.11 | `undo_redo_editors` | recipe · advanced | undo/redo for a text editor: command, memento, persistent rope, piece table and undo tree, tested against a model |

**Further resources:** 14 docs, 10 article or blog, 3 code load; 0 broken, 0 unconfirmed.

### 1.6 Memory and allocators

| # | Section | Source | About |
|---|---|---|---|
| 1.6.1 | Concurrent memory | db guide | |
| 1.6.2 | Garbage collection | compiler guide | |
| 1.6.3 | Graph algorithms, GC and life without GC | compiler guide | |
| 1.6.4 | `arena_ids` | recipe · simple | arena of nodes addressed by typed ids (no Box per node) |
| 1.6.5 | `bump_arena_allocator` | recipe · advanced | a bump (arena) allocator: alignment, checkpoints, drop glue, chunked growth |
| 1.6.6 | `garbage_collectors` | recipe · advanced | garbage collection algorithms on a simulated managed heap, checked by a reachability oracle |
| 1.6.7 | `size_class_allocator` | recipe · advanced | a toy general-purpose allocator: boundary tags, size-class free lists, splitting, coalescing, realloc |
| 1.6.8 | `slab_generational` | recipe · advanced | a slab / object pool with a free list through vacant slots and GENERATIONAL typed handles |

**Further resources:** 6 code, 5 docs, 1 book load; 0 broken, 0 unconfirmed.

### 1.7 Unsafe and FFI

| # | Section | Source | About |
|---|---|---|---|
| 1.7.1 | Rust for systems code | db guide | |
| 1.7.2 | `unsafe_basics` | recipe · intermediate | unsafe basics: the five superpowers, safe wrappers, and how to test them |
| 1.7.3 | `unsafecell_cells` | recipe · advanced | UnsafeCell: build your own `Cell` and `RefCell` (interior mutability from first principles) |
| 1.7.4 | `maybeuninit_fixedvec` | recipe · advanced | MaybeUninit and ManuallyDrop: an inline, heap-free `FixedVec<T, N>` |
| 1.7.5 | `nonnull_linked_list` | recipe · advanced | a doubly linked `Deque<T>` with `NonNull`, `Box::into_raw` and `PhantomData` |
| 1.7.6 | `pin_projection` | recipe · advanced | Pin, Unpin, PhantomPinned and structural pin projection (a hand-made `Future::map`) |
| 1.7.7 | `repr_layout_ffi` | recipe · intermediate | repr(transparent/u8/C/packed), offset_of!, extern "C" fn pointers, CStr, and a binary round trip |
| 1.7.8 | `ffi_c_interop` | recipe · advanced | C interop both ways with only `libc`: extern "C" calls, callbacks, opaque handles, CString ownership, panic safety |

**Further resources:** 11 docs, 3 code, 1 article or blog load; 0 broken, 0 unconfirmed.

## 2. Standard library and core

Look-up pages for std, alloc and core, each labelled with where the API is defined.

### 2.1 Option, Result, conversions, ownership types

| # | Section | Source | About |
|---|---|---|---|
| 2.1.1 | `std_option_methods` | recipe · simple | Option<T> look-up page: every common method, with the scenario where it is the right tool |
| 2.1.2 | `std_result_methods` | recipe · simple | Result<T, E> look-up page: combinators, collecting, `?` with From, Box<dyn Error>, exit codes |
| 2.1.3 | `std_convert_from_into_asref` | recipe · intermediate | conversion traits: From/Into/TryFrom/AsRef/Borrow/FromStr/Display/Default/FromIterator/IntoIterator/Extend/Deref |
| 2.1.4 | `std_cow_borrow_toowned` | recipe · intermediate | Cow<'a, B>, Borrow, ToOwned: allocate only when you must; Borrow vs AsRef vs Deref |
| 2.1.5 | `std_smart_pointers_use_cases` | recipe · intermediate | Box, Rc/Weak, Arc, Cell/RefCell/OnceCell/LazyCell/OnceLock/LazyLock: one real use case each |

**Further resources:** 19 docs load; 0 broken, 0 unconfirmed.

### 2.2 Text

| # | Section | Source | About |
|---|---|---|---|
| 2.2.1 | `std_char_ascii_unicode` | recipe · simple | `char` and `u8` ASCII/Unicode methods: classification, case, digits, conversions, UTF-16, and byte-level lexers |
| 2.2.2 | `std_str_search_split_parse` | recipe · simple | `str` end to end: search, split, trim, case, parse, UTF-8 boundaries, and the scenarios they are used for |
| 2.2.3 | `std_string_building_utf8` | recipe · intermediate | `String` building and mutation, `fmt::Write`, allocation counting, UTF-8 encoding by hand, and string-type memory sizes |

**Further resources:** 7 docs, 4 article or blog load; 0 broken, 0 unconfirmed.

### 2.3 Iterators and collections

| # | Section | Source | About |
|---|---|---|---|
| 2.3.1 | `std_iterator_adapters_lazy` | recipe · intermediate | every lazy Iterator adapter, with laziness proven by side-effect logs |
| 2.3.2 | `std_iterator_consumers` | recipe · intermediate | every consuming Iterator method, with a use case each |
| 2.3.3 | `std_iterator_sources_custom` | recipe · advanced | iterator sources, writing your own iterators, and iterator-returning APIs |
| 2.3.4 | `std_hashmap_entry_api_patterns` | recipe · intermediate | HashMap and HashSet in depth, with the Entry API as the centrepiece |

**Further resources:** 7 docs, 1 article or blog load; 0 broken, 0 unconfirmed.

### 2.4 Vectors, slices and collections

| # | Section | Source | About |
|---|---|---|---|
| 2.4.1 | `std_vec_vecdeque_methods` | recipe · simple | Vec<T> and VecDeque<T> look-up page: editing, bulk moves, capacity, ring-buffer queues |
| 2.4.2 | `std_slice_search_sort_binary` | recipe · intermediate | slice look-up page: access, search, binary search, sorting, selection, rearranging, copying |
| 2.4.3 | `std_slice_chunks_windows_split` | recipe · intermediate | slicing a slice: chunks, chunks_exact, rchunks, windows, as_chunks, chunk_by, split*, split_at_mut |
| 2.4.4 | `std_collections_choice_guide` | recipe · intermediate | which collection? Ten workloads, each solved with the right structure, with COUNTED comparisons |
| 2.4.5 | `std_array_tuple_unit_never` | recipe · intermediate | arrays [T; N], tuples, the unit type (), and "never" (`!`, Infallible, empty enums), plus zero-sized types |

**Further resources:** 10 docs load; 0 broken, 0 unconfirmed.

### 2.5 Numbers, traits and formatting

| # | Section | Source | About |
|---|---|---|---|
| 2.5.1 | `std_integer_methods_overflow` | recipe · intermediate | integer methods look-up page: overflow families, division, bits, bytes, `as` casts, NonZero |
| 2.5.2 | `std_float_methods_total_order` | recipe · intermediate | f32/f64 look-up page: rounding, special values, comparison and total order, approximate equality, |
| 2.5.3 | `std_num_parsing_formatting` | recipe · intermediate | number parsing and formatting end to end: FromStr with every error, radix, grouping, hex dumps, |
| 2.5.4 | `std_cmp_hash_default_traits` | recipe · intermediate | PartialEq/Eq/PartialOrd/Ord, Ordering, Reverse, sorting by key, Hash/Hasher/BuildHasher, Default |
| 2.5.5 | `std_ops_traits_overloading` | recipe · intermediate | std::ops look-up page: arithmetic and bit operators, Index, Deref, Drop, closure traits, ranges, |
| 2.5.6 | `std_any_error_marker_traits` | recipe · intermediate | std::any (Any, TypeId, downcast), std::error::Error (source chains, boxing), marker and auto traits |
| 2.5.7 | `std_fmt_display_debug_deep` | recipe · intermediate | the format string mini-language, Display and Debug by hand, Formatter methods, fmt::Write vs io::Write |

**Further resources:** 16 docs load; 0 broken, 0 unconfirmed.

### 2.6 Memory and low level

| # | Section | Source | About |
|---|---|---|---|
| 2.6.1 | `std_alloc_layout_global_allocator` | recipe · advanced | the core / alloc / std layers, Layout arithmetic, the raw allocation API and a counting |
| 2.6.2 | `std_mem_replace_take_swap_forget` | recipe · intermediate | `std::mem` by use case: take, replace, swap, drop, forget, sizes, discriminants, `ManuallyDrop`, `MaybeUninit` |
| 2.6.3 | `std_ptr_nonnull_slices_raw` | recipe · advanced | raw pointers, `NonNull`, slices from raw parts, pointer arithmetic, alignment and provenance, each unsafe step justified |
| 2.6.4 | `std_btree_binaryheap_ordered` | recipe · intermediate | ordered collections: `BTreeMap`, `BTreeSet` and `BinaryHeap` by use case |
| 2.6.5 | `std_hash_collections_io_serialization_formats` | recipe · advanced | hand-rolled serialisation on `std::io`: endian integers, varints, length-prefixed records, checksums, versioned heade... |
| 2.6.6 | `std_vecdeque_linkedlist_ring_buffers` | recipe · intermediate | `VecDeque` as the standard ring buffer, when `LinkedList` is justified, and how to choose a priority structure by cou... |

**Further resources:** 14 docs, 3 article or blog load; 0 broken, 0 unconfirmed.

### 2.7 Threads, synchronisation and time

| # | Section | Source | About |
|---|---|---|---|
| 2.7.1 | `std_atomics_ordering_use_cases` | recipe · advanced | atomic types by method and by use case, with the ordering each use needs and why |
| 2.7.2 | `std_sync_primitives_use_cases` | recipe · intermediate | `std::sync` and thread coordination by use case: Mutex, RwLock, Condvar, Once, OnceLock, LazyLock, Barrier, mpsc, Arc |
| 2.7.3 | `std_thread_scope_local_park` | recipe · intermediate | `std::thread` in practice: ownership across threads, scoped parallelism, panic handling, worker pools, thread locals,... |
| 2.7.4 | `std_time_instant_duration_systemtime` | recipe · intermediate | `std::time` by use case: Duration, Instant, SystemTime, deadlines, backoff, testable clocks |

**Further resources:** 11 docs, 3 book, 1 article or blog load; 0 broken, 0 unconfirmed.

### 2.8 Processes, signals and networking

| # | Section | Source | About |
|---|---|---|---|
| 2.8.1 | `std_env_args_vars` | recipe · intermediate | std::env (args, environment variables, process paths), a hand-written CLI parser, layered config, |
| 2.8.2 | `std_panic_process_exit_catch` | recipe · intermediate | panics (payloads, catch_unwind, hooks, drop order), ending a process (exit, abort, ExitCode) and |
| 2.8.3 | `std_process_command_pipelines` | recipe · intermediate | `std::process` by use case: Command, pipes, pipelines, exit statuses, timeouts, signals |
| 2.8.4 | `std_signals_env_exit_hooks_ctrlc` | recipe · advanced | Unix signals with `libc`, Ctrl-C and SIGTERM shutdown, SIGPIPE, environment variables, exit hooks, and which destruct... |
| 2.8.5 | `std_net_tcp_udp_ip_addr` | recipe · intermediate | `std::net` by use case: IP and socket addresses, CIDR checks, blocking TCP and UDP on loopback, framing, timeouts, sh... |
| 2.8.6 | `std_io_stdio_terminal_text_processing` | recipe · intermediate | stdin, stdout, stderr and terminal behaviour: writing command-line filters that are correct in a pipe and testable wi... |
| 2.8.7 | `std_fs_watch_polling_tempdir_testing` | recipe · intermediate | testing code that touches the filesystem, a polling directory watcher, and single-instance guards, using only std |

**Further resources:** 19 docs, 2 article or blog, 1 book load; 0 broken, 0 unconfirmed.

### 2.9 I/O, files and paths

| # | Section | Source | About |
|---|---|---|---|
| 2.9.1 | `std_bufreader_bufwriter_patterns` | recipe · intermediate | `BufReader`, `BufWriter`, `LineWriter` in depth, with syscall counting |
| 2.9.2 | `std_io_read_write_seek_traits` | recipe · intermediate | `Read`, `Write`, `BufRead`, `Seek`, `Cursor` and `io::Error` by realistic example |
| 2.9.3 | `std_fs_file_operations_usecases` | recipe · intermediate | `std::fs` by use case: OpenOptions matrix, metadata, directories, atomic writes, walks, append log, rotation |
| 2.9.4 | `std_path_pathbuf_ffi_strings` | recipe · intermediate | `Path`, `PathBuf`, `OsStr`, `OsString`, `CString`, `CStr`: paths, safe joins, platform strings, FFI strings |

**Further resources:** 10 docs load; 0 broken, 0 unconfirmed.

### Planned pages (no source yet)

Named in the interrupted std briefs; the list is extracted from their text, so check it before writing. There is also no dedicated `core` or `alloc` reference:

`std_iterator_trait_deep`.

## 3. Crates and tooling

Popular crates and the tools around a Rust project.

### 3.1 Data, text and encoding crates

| # | Section | Source | About |
|---|---|---|---|
| 3.1.1 | `serde_attributes_enum_representations` | recipe · intermediate | serde derive attributes in depth: enum representations, defaults, flatten, with, borrow, Value, RawValue, streaming |
| 3.1.2 | `serde_formats_json_toml_yaml_bincode` | recipe · intermediate | one data model, seven formats: JSON, TOML, YAML, bincode, postcard, MessagePack, CBOR, plus CSV |
| 3.1.3 | `serde_custom` | recipe · advanced | serde beyond derive: hand-written impls, attributes, borrowing, seeds, and a custom data format |
| 3.1.4 | `serde_defaults` | recipe · simple | typed JSON parsing with defaults and strict rejection |
| 3.1.5 | `bytes_buf_buf_mut_framing` | recipe · intermediate | bytes: Bytes, BytesMut, Buf, BufMut, zero-copy slicing and an incremental length-delimited decoder |
| 3.1.6 | `regex_text_processing` | recipe · intermediate | regex: captures, replacement, sets, builders, bytes, and the linear-time guarantee |
| 3.1.7 | `url_base64_hex_percent_encoding` | recipe · intermediate | url, percent-encoding, base64, hex and data-encoding |
| 3.1.8 | `uuid_rand_ids_tokens` | recipe · intermediate | uuid and rand: random, time-ordered and name-based ids; seeded simulation; distributions; secret tokens |
| 3.1.9 | `indexmap_smallvec_arrayvec_collections` | recipe · intermediate | collection crates: indexmap, smallvec, arrayvec, tinyvec, hashbrown, fast hashers, bitflags, bit-set, bitvec |
| 3.1.10 | `itertools_adaptors_cookbook` | recipe · intermediate | itertools: grouping, merging, windows, combinatorics and folding, each checked against a std-only version |
| 3.1.11 | `chrono_dates_times_zones` | recipe · intermediate | chrono: dates, times, durations, formatting, time zones, DST and schedules |
| 3.1.12 | `time_crate_dates_formatting` | recipe · intermediate | the `time` crate: Date/Time/OffsetDateTime, format descriptions, well-known formats, chrono comparison |
| 3.1.13 | `tempfile_walkdir_glob_ignore` | recipe · intermediate | tempfile, walkdir, globset, ignore, same-file and dirs: temporary files, tree walking and matching |

**Further resources:** 49 docs, 19 article or blog, 4 code, 2 book load; 0 broken, 0 unconfirmed.

### 3.2 Application crates

| # | Section | Source | About |
|---|---|---|---|
| 3.2.1 | `anyhow_thiserror_eyre_error_design` | recipe · intermediate | error design with thiserror, anyhow, eyre and miette: sources, context, downcasting, codes, reports and testing |
| 3.2.2 | `clap_derive_cli_tool` | recipe · intermediate | clap derive: subcommands, value enums, constraints, env fallback, custom parsers, completions, testing |
| 3.2.3 | `clap_cli` | recipe · intermediate | a CLI with clap derive: subcommands, flags, enums, validation, and testing arg parsing |
| 3.2.4 | `tracing_spans_subscriber_layers` | recipe · intermediate | tracing: events, spans, #[instrument], filtering, fmt/JSON output, custom layers and async context |
| 3.2.5 | `tracing_logs` | recipe · intermediate | structured logging with `tracing`: events, spans, fields, levels |
| 3.2.6 | `tokio_runtime_tasks_channels_timers` | recipe · intermediate | tokio essentials on a paused clock: tasks, select, timers, channels, sync primitives, cancellation, shutdown |
| 3.2.7 | `reqwest_axum_http_roundtrip` | recipe · intermediate | axum server and reqwest client in one process: extractors, errors, timeouts, redirects, retries, streaming, multipart... |
| 3.2.8 | `sqlx_sqlite_queries_transactions` | recipe · intermediate | sqlx with SQLite: pools, migrations, typed queries, transactions, savepoints, upserts, pagination, bulk insert |

**Further resources:** 25 docs, 5 article or blog, 3 code load; 0 broken, 0 unconfirmed.

## 4. Testing and benchmarking

How to know code is correct and how fast it is: guides by method and by kind of system.

### 4.1 Methods: test design and TDD

| # | Section | Source | About |
|---|---|---|---|
| 4.1.1 | Toolchain | db guide | |
| 4.1.2 | `testing_patterns` | recipe · intermediate | unit-test patterns: table-driven, Result-returning, should_panic, fakes, builders, seeded randomness |

**Further resources:** 3 docs, 2 article or blog, 1 code load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Cargo book, (lints)](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section) (docs); [Lightweight Formal Methods for S3](https://www.amazon.science/publications/using-lightweight-formal-methods-to-validate-a-key-value-storage-node-in-amazon-s3) (article or blog).

### 4.2 Methods: property-based testing

| # | Section | Source | About |
|---|---|---|---|
| 4.2.1 | `shrinking` | recipe · advanced | property testing with SHRINKING (what quickcheck/proptest do for you) |
| 4.2.2 | `criterion_proptest_insta_testing` | recipe · intermediate | testing tools: proptest (strategies, shrinking, stateful), insta inline snapshots, assert_cmd + predicates, hand-writ... |

**Further resources:** 5 docs, 2 code, 2 article or blog, 1 book load; 0 broken, 0 unconfirmed.

### 4.3 Methods: fuzzing

| # | Section | Source | About |
|---|---|---|---|
| 4.3.1 | Tooling | compiler guide | |

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 4.4 Methods: mutation testing

| # | Section | Source | About |
|---|---|---|---|

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 4.5 Methods: integration, smoke and black-box testing

| # | Section | Source | About |
|---|---|---|---|

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 4.6 Methods: API testing

| # | Section | Source | About |
|---|---|---|---|

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 4.7 Methods: async testing

| # | Section | Source | About |
|---|---|---|---|

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 4.8 Methods: concurrency testing

| # | Section | Source | About |
|---|---|---|---|
| 4.8.1 | `model_checker_for_concurrent_code` | recipe · advanced | a mini loom: exhaustive interleaving exploration with sleep sets and a store-buffer (TSO) memory model |

**Further resources:** 1 code, 1 docs, 1 paper load; 0 broken, 0 unconfirmed.

### 4.9 Methods: memory-ordering bugs

| # | Section | Source | About |
|---|---|---|---|
| 4.9.1 | `memory_ordering_litmus` | recipe · advanced | memory-ordering litmus tests with real threads: message passing, store buffering, spinlock, fences |

**Further resources:** 1 code, 1 book, 1 docs load; 0 broken, 0 unconfirmed.

### 4.10 Methods: unsafe code and unsoundness

| # | Section | Source | About |
|---|---|---|---|

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 4.11 Methods: security and vulnerability checking

| # | Section | Source | About |
|---|---|---|---|

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 4.12 Methods: benchmarking and profiling

| # | Section | Source | About |
|---|---|---|---|
| 4.12.1 | Benchmarking | db guide | |
| 4.12.2 | Profiling and observability | db guide | |
| 4.12.3 | `bench_harness` | recipe · intermediate | measuring properly: warm-up, many samples, median, `black_box`, release mode |
| 4.12.4 | `metrics_prometheus_histograms_load_testing` | recipe · advanced | a small metrics library with Prometheus text output, and a load-testing lab: coordinated omission, Little's law, M/M/... |

**Further resources:** 3 code, 3 article or blog, 2 video, 1 paper load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Performance testing at MongoDB](https://www.youtube.com/watch?v=elRLJ997p6A) (video).

### 4.13 Systems: distributed systems

| # | Section | Source | About |
|---|---|---|---|
| 4.13.1 | Deterministic simulation | db guide | |
| 4.13.2 | `deterministic_simulation_testing` | recipe · advanced | deterministic simulation testing (FoundationDB / TigerBeetle style): a Raft KV store in one thread with simulated tim... |
| 4.13.3 | `linearizability_checker` | recipe · advanced | linearizability checker (Wing-Gong search with memoisation) vs sequential models |

**Further resources:** 2 video, 1 paper, 1 code, 1 article or blog load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [FoundationDB: simulation testing](https://www.youtube.com/watch?v=OJb8A6h9jQQ) (video).

### 4.14 Systems: databases

| # | Section | Source | About |
|---|---|---|---|
| 4.14.1 | Testing a database | db guide | |
| 4.14.2 | `consistent_snapshots_sql_logic_tests` | recipe · intermediate | how database engines are tested: sqllogictest scripts, differential testing, TLP and NoREC, test-case reduction, cove... |

**Further resources:** 2 code, 2 video, 1 docs, 1 paper load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Finding logic bugs in DBMSs (SQLancer)](https://www.youtube.com/watch?v=Np46NQ6lqP8) (video).

### 4.15 Systems: compilers and languages

| # | Section | Source | About |
|---|---|---|---|
| 4.15.1 | Testing and debugging compilers | compiler guide | |
| 4.15.2 | Benchmarking compilers | compiler guide | |

**Further resources:** 1 book, 1 article or blog, 1 code load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Zeller et al., The Fuzzing Book](https://www.fuzzingbook.org) (book); [rustc dev guide: testing](https://rustc-dev-guide.rust-lang.org/tests/intro.html) (article or blog); [rustc-perf](https://github.com/rust-lang/rustc-perf) (code).

### Planned pages and curated resources

Every guide answers the same six questions: **what can go wrong; what tells you the answer is right (the oracle); how to find bugs; how to show your check can fail; what to benchmark; how to report it.** Not every question applies to every method.

The tool resources were checked to load. They have **not** been read for these pages yet; verify each tool's current limits before writing about it.


#### Methods: test design and TDD

**Planned pages:**

- Which test for which risk: the test portfolio
- Test-driven development in Rust: red, green, refactor on a real change
- When TDD helps and when a spike comes first
- Test doubles: fakes, stubs and trait seams
- Snapshot, golden and regression tests
- Coverage: what it shows and what it hides
- Flaky tests: causes, diagnosis and what not to do
- CI: what runs per commit and what runs nightly

Related chapters elsewhere: 3.2 Application crates.

**Start with:** [The Rust Book: writing tests](https://doc.rust-lang.org/book/ch11-00-testing.html) (book); [proptest](https://github.com/proptest-rs/proptest) (code); [The proptest book](https://proptest-rs.github.io/proptest/intro.html) (docs); [insta snapshot testing](https://github.com/mitsuhiko/insta) (code); [cargo-mutants](https://github.com/sourcefrog/cargo-mutants) (code); [Rust Fuzz Book](https://rust-fuzz.github.io/book/) (book); [cargo-nextest](https://github.com/nextest-rs/nextest) (code); [matklad: How to Test](https://matklad.github.io/2021/05/31/how-to-test.html) (blog); [Martin Fowler: Test Driven Development](https://martinfowler.com/bliki/TestDrivenDevelopment.html) (article); [Martin Fowler: Test Double](https://martinfowler.com/bliki/TestDouble.html) (article); [Google Testing Blog](https://testing.googleblog.com/) (blog); [cargo-llvm-cov (coverage)](https://github.com/taiki-e/cargo-llvm-cov) (code); [Google: flaky tests and how we mitigate them](https://testing.googleblog.com/2016/05/flaky-tests-at-google-and-how-we.html) (blog).

#### Methods: property-based testing

**Planned pages:**

- Finding properties: round trip, invariant, oracle, idempotence, metamorphic
- Generators and shrinking with proptest
- Stateful and model-based testing
- quickcheck compared with proptest
- Seeds, regression files and reproducing failures

Related chapters elsewhere: 4. Data structures and algorithms.

**Start with:** [Hypothesis: what is property based testing](https://hypothesis.works/articles/what-is-property-based-testing/) (article); [quickcheck](https://github.com/BurntSushi/quickcheck) (code).

#### Methods: fuzzing

**Planned pages:**

- cargo-fuzz and libFuzzer: first target
- Structured fuzzing with arbitrary
- Corpora, dictionaries and coverage guidance
- Differential fuzzing against a reference
- Triage: minimise, deduplicate and turn crashes into tests
- AFL.rs and when to use it

Related chapters elsewhere: 11.1 Lexing and parsing.

**Start with:** [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (code); [AFL for Rust](https://github.com/rust-fuzz/afl.rs) (code); [arbitrary (structured fuzzing)](https://github.com/rust-fuzz/arbitrary) (code); [libFuzzer](https://llvm.org/docs/LibFuzzer.html) (docs); [The Fuzzing Book](https://www.fuzzingbook.org/) (book).

#### Methods: mutation testing

**Planned pages:**

- Can your tests fail? Planting bugs by hand
- cargo-mutants: running and reading results
- Equivalent mutants and what survivors mean
- Mutation testing in CI without it taking all night

**Start with:** [cargo-mutants book](https://mutants.rs/) (docs).

#### Methods: integration, smoke and black-box testing

**Planned pages:**

- Unit, integration and end-to-end boundaries
- Black-box tests of a command-line program
- Smoke tests after a build or deploy
- Real dependencies with containers; isolation and cleanup
- Testing configuration, startup and graceful shutdown

Related chapters elsewhere: 3.2 Application crates.

**Start with:** [assert_cmd (command-line black-box tests)](https://github.com/assert-rs/assert_cmd) (code); [testcontainers for Rust](https://github.com/testcontainers/testcontainers-rs) (code); [Martin Fowler: Integration Test](https://martinfowler.com/bliki/IntegrationTest.html) (article).

#### Methods: API testing

**Planned pages:**

- Testing HTTP handlers without a socket
- Testing over a real socket on loopback
- Mocking the services you call
- Contract tests and schema-driven tests
- Errors, auth, pagination, idempotency and rate limits
- Load testing an API

Related chapters elsewhere: 10.1 Web services; 10.2 Authentication and web security.

**Start with:** [k6 load testing](https://github.com/grafana/k6) (code); [oha load generator](https://github.com/hatoo/oha) (code); [wrk2 (coordinated omission)](https://github.com/giltene/wrk2) (code); [wiremock-rs](https://github.com/LukeMathWalker/wiremock-rs) (code); [Hurl: HTTP tests as plain text](https://github.com/Orange-OpenSource/hurl) (code); [Schemathesis (tests from an OpenAPI schema)](https://github.com/schemathesis/schemathesis) (code); [Pact contract testing](https://docs.pact.io/) (docs).

#### Methods: async testing

**Planned pages:**

- Async test runtimes and what they hide
- Controlling time: paused clocks instead of sleeps
- Testing cancellation safety and drop
- Testing timeouts, backpressure and shutdown
- Deadlocks and stuck tasks: finding and observing them

Related chapters elsewhere: 5.4 Async Rust.

**Start with:** [Tokio: unit testing](https://tokio.rs/tokio/topics/testing) (docs); [tokio::time::pause](https://docs.rs/tokio/latest/tokio/time/fn.pause.html) (docs); [tokio-test](https://github.com/tokio-rs/tokio/tree/master/tokio-test) (code); [tokio-console](https://github.com/tokio-rs/console) (docs).

#### Methods: concurrency testing

**Planned pages:**

- Why a thousand passing runs prove little
- loom: exploring interleavings
- shuttle: randomised schedules
- Linearizability for concurrent data structures
- Stress tests without timing assertions
- ThreadSanitizer: setup and reading reports
- Benchmarking under contention

Related chapters elsewhere: 5.3 Concurrent data structures.

**Start with:** [loom](https://github.com/tokio-rs/loom) (code); [shuttle](https://github.com/awslabs/shuttle) (code); [Mara Bos: Rust Atomics and Locks](https://marabos.nl/atomics/) (book).

#### Methods: memory-ordering bugs

**Planned pages:**

- The memory model in practice: acquire, release, sequential consistency
- Litmus tests and why x86 can hide an ARM bug
- Finding ordering bugs with loom, Miri and ThreadSanitizer
- Case studies: publication, double-checked init, reference counting
- What each tool cannot detect

Related chapters elsewhere: 5.2 Atomics and memory ordering.

**Start with:** [std::sync::atomic::Ordering](https://doc.rust-lang.org/std/sync/atomic/enum.Ordering.html) (docs); [herdtools7 litmus tools](https://github.com/herd/herdtools7) (code); [Preshing: memory ordering at compile time](https://preshing.com/20120625/memory-ordering-at-compile-time/) (blog); [Russ Cox: hardware memory models](https://research.swtch.com/hwmm) (blog); [Clang ThreadSanitizer](https://clang.llvm.org/docs/ThreadSanitizer.html) (docs).

#### Methods: unsafe code and unsoundness

**Planned pages:**

- Unsound compared with merely buggy: the safe API must never allow undefined behaviour
- Writing invariants and safety comments you can test
- Miri: aliasing, uninitialised memory, leaks and its limits
- Sanitizers: address, memory, leak
- Attacking your own safe API: hostile trait impls, panics, re-entrancy
- Fuzzing safe wrappers over unsafe code
- Bounded model checking with Kani
- Compile-fail tests for what must not type-check

Related chapters elsewhere: 1.7 Unsafe and FFI.

**Start with:** [Miri](https://github.com/rust-lang/miri) (code); [The Rustonomicon](https://doc.rust-lang.org/nomicon/) (book); [Rust sanitizers (unstable book)](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html) (docs); [Kani verifier](https://github.com/model-checking/kani) (code); [cargo-careful](https://github.com/RalfJung/cargo-careful) (code); [Rust Unsafe Code Guidelines](https://rust-lang.github.io/unsafe-code-guidelines/) (book); [Rudra: finding unsound Rust (research tool)](https://github.com/sslab-gatech/Rudra) (code); [Rust reference: behaviour considered undefined](https://doc.rust-lang.org/reference/behavior-considered-undefined.html) (docs); [cargo-geiger (counts unsafe use)](https://github.com/geiger-rs/cargo-geiger) (code).

#### Methods: security and vulnerability checking

**Planned pages:**

- Dependency advisories: cargo-audit and OSV
- Policy: cargo-deny for licences, sources and bans
- Supply chain review with cargo-vet
- Static analysis: Clippy, Semgrep, CodeQL
- Security tests: injection, path traversal, auth bypass, zip bombs
- Secrets, logs and error leaks
- Handling a reported vulnerability

Related chapters elsewhere: 10.2 Authentication and web security.

**Start with:** [RustSec: cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit) (code); [RustSec advisory database](https://rustsec.org/) (docs); [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) (code); [cargo-vet](https://github.com/mozilla/cargo-vet) (code); [OSV vulnerability database](https://osv.dev/) (docs); [Semgrep](https://github.com/semgrep/semgrep) (code); [CodeQL](https://codeql.github.com/) (docs); [OWASP Web Security Testing Guide](https://owasp.org/www-project-web-security-testing-guide/) (docs).

#### Methods: benchmarking and profiling

**Planned pages:**

- Reading a benchmark: noise, warm-up, statistics
- Microbenchmarks: criterion, divan, black_box
- Instruction counts for stable regression gates
- Profiling: sampling, flame graphs, allocation profiles
- Latency percentiles, throughput and coordinated omission
- Load, stress and soak tests
- Reporting results: machine, inputs, versions

Related chapters elsewhere: 6.1 Measuring and CPU performance; 6.4 High-performance computing.

**Start with:** [Criterion.rs book](https://bheisler.github.io/criterion.rs/book/) (docs); [divan](https://github.com/nvzqz/divan) (code); [iai-callgrind (now gungraun)](https://github.com/iai-callgrind/iai-callgrind) (code); [The Rust Performance Book: benchmarking](https://nnethercote.github.io/perf-book/benchmarking.html) (book); [The Rust Performance Book: profiling](https://nnethercote.github.io/perf-book/profiling.html) (book); [samply profiler](https://github.com/mstange/samply) (code); [cargo flamegraph](https://github.com/flamegraph-rs/flamegraph) (code); [Brendan Gregg: flame graphs](https://www.brendangregg.com/flamegraphs.html) (blog); [hyperfine](https://github.com/sharkdp/hyperfine) (code).

#### Systems: distributed systems

**Planned pages:**

- Failure model first: what can break
- Deterministic simulation: clock, network, disk and randomness under test control
- Fault injection and chaos: partitions, crashes, reordering, fsync failure
- Safety and liveness checks, with a fault-free phase for liveness
- Jepsen and Maelstrom
- turmoil and madsim
- Benchmarking: tail latency, load shape, recovery time

Related chapters elsewhere: 10. Distributed systems.

**Start with:** [turmoil](https://github.com/tokio-rs/turmoil) (code); [madsim](https://github.com/madsim-rs/madsim) (code); [Jepsen](https://github.com/jepsen-io/jepsen) (code); [Jepsen analyses](https://jepsen.io/analyses) (article); [Maelstrom workbench](https://github.com/jepsen-io/maelstrom) (code); [Porcupine linearizability checker](https://github.com/anishathalye/porcupine) (code); [FoundationDB: simulation and testing](https://apple.github.io/foundationdb/testing.html) (docs); [TigerBeetle safety documentation](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/concepts/safety.md) (blog); [Principles of Chaos Engineering](https://principlesofchaos.org/) (article).

#### Systems: databases

**Planned pages:**

- Oracles for each layer
- sqllogictest and differential testing
- Logic-bug oracles: TLP and NoREC
- Isolation anomaly testing
- Crash and recovery testing: torn writes and fsync
- Benchmarking: TPC, YCSB, amplification and cache effects

Related chapters elsewhere: 8. Databases.

**Start with:** [How SQLite is tested](https://www.sqlite.org/testing.html) (docs); [sqllogictest](https://www.sqlite.org/sqllogictest/doc/trunk/about.wiki) (docs); [sqllogictest-rs](https://github.com/risinglightdb/sqllogictest-rs) (code); [SQLancer](https://github.com/sqlancer/sqlancer) (code); [YCSB](https://github.com/brianfrankcooper/YCSB) (code); [TPC benchmarks](https://www.tpc.org/) (docs); [ClickBench](https://github.com/ClickHouse/ClickBench) (docs).

#### Systems: compilers and languages

**Planned pages:**

- Golden and compile-fail tests
- Parser robustness: fuzzing and round trips
- Differential testing: interpreter against compiled output
- Random program generation and test-case reduction
- IR verifiers and per-pass checks
- Benchmarking: compile time, run time and code size

Related chapters elsewhere: 12. Compilers and languages.

**Start with:** [rustc dev guide: testing](https://rustc-dev-guide.rust-lang.org/tests/intro.html) (docs); [compiletest-rs](https://github.com/Manishearth/compiletest-rs) (code); [trybuild](https://github.com/dtolnay/trybuild) (code); [LLVM FileCheck](https://llvm.org/docs/CommandGuide/FileCheck.html) (docs); [Csmith](https://github.com/csmith-project/csmith) (code); [C-Reduce](https://github.com/csmith-project/creduce) (code); [rustc-perf](https://github.com/rust-lang/rustc-perf) (docs).

## 5. Data structures and algorithms

The structures and algorithms other modules build on.

### 5.1 Hashing and probabilistic structures

| # | Section | Source | About |
|---|---|---|---|
| 5.1.1 | Count-min sketch | db guide | |
| 5.1.2 | More primers | db guide | |
| 5.1.3 | Hash tables | db guide | |
| 5.1.4 | `hash_functions` | recipe · intermediate | hash functions from scratch (FNV-1a, Fx, fmix64, a wyhash-style mixer) and how to MEASURE their quality |
| 5.1.5 | `open_addressing_map` | recipe · advanced | an open-addressing hash map with linear probing, Robin Hood displacement and backward-shift deletion |
| 5.1.6 | `cuckoo_hashing` | recipe · advanced | cuckoo hashing: a two-table cuckoo hash map AND a cuckoo filter (approximate membership with delete) |
| 5.1.7 | `count_min_sketch` | recipe · intermediate | Count-Min Sketch + heavy hitters (top-k) in sublinear memory |
| 5.1.8 | `hyperloglog` | recipe · advanced | HyperLogLog cardinality estimation from scratch |
| 5.1.9 | `minhash_simhash_lsh` | recipe · advanced | MinHash + banded LSH + SimHash for near-duplicate detection |
| 5.1.10 | `sampling` | recipe · intermediate | sampling: reservoir (Algorithm R), weighted (A-Res), shuffles, replacement |
| 5.1.11 | `bloom_filter` | recipe · intermediate | Bloom filter: "definitely not present" vs "probably present" |

**Further resources:** 14 article or blog, 6 paper, 4 docs, 3 code, 3 video, 1 book, 1 slides load; 4 broken, 1 unconfirmed.
Examples with titles from the guides: [HyperLogLog: the analysis of a near-optimal cardinality estimation algorithm](https://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf) (paper); [Nomicon, PhantomData](https://doc.rust-lang.org/nomicon/phantom-data.html) (docs); [Rust Atomics and Locks](https://marabos.nl/atomics/) (book); [Using Rust for Game Development](https://kyren.github.io/2018/09/14/rustconf-talk.html) (article or blog); [L07 · Database Hash Tables (F25)](https://www.youtube.com/watch?v=nuNW8IfgPNU) (video).

### 5.2 Trees and ordered structures

| # | Section | Source | About |
|---|---|---|---|
| 5.2.1 | `bplus_tree` | recipe · advanced | an in-memory B+ tree map with leaf chain, split, borrow and merge, arena-based |
| 5.2.2 | `b_epsilon_tree` | recipe · advanced | the B^epsilon-tree: a B-tree whose internal nodes carry a MESSAGE BUFFER, trading a little read cost for far fewer wr... |
| 5.2.3 | `btree_bulk_load_prefix_compression` | recipe · advanced | production B+tree features: bulk loading, prefix compression, suffix truncation, duplicates, split policy |
| 5.2.4 | `bw_tree_latch_free` | recipe · advanced | a Bw-tree style latch-free B+tree: mapping table, delta chains, consolidation, split deltas, epochs |
| 5.2.5 | `optimistic_lock_coupling_btree` | recipe · advanced | B+tree concurrency control: latch crabbing, optimistic crabbing, and optimistic lock coupling (OLC) |
| 5.2.6 | `adaptive_radix_tree` | recipe · advanced | the Adaptive Radix Tree (ART): Node4/16/48/256, path compression, lazy leaf expansion, ordered scans |
| 5.2.7 | `trie_radix` | recipe · intermediate | a trie and a compressed radix tree (Patricia) over byte strings, arena-based |
| 5.2.8 | `skiplist` | recipe · advanced | skip list: a probabilistically balanced ordered map (arena, no unsafe) |
| 5.2.9 | `treap_interval` | recipe · advanced | a split/merge treap with pluggable augmentation: order statistics and an interval tree |
| 5.2.10 | `segment_fenwick` | recipe · intermediate | Fenwick tree, monoid segment tree, lazy range-add segment tree, sparse table |
| 5.2.11 | `gist_generalized_search_tree` | recipe · advanced | GiST, the Generalized Search Tree: ONE balanced tree, two index types (B-tree-like ranges and an R-tree) |
| 5.2.12 | `learned_index_rmi` | recipe · advanced | learned indexes: a two-stage Recursive Model Index (RMI), a PGM-style piecewise-linear index with a hard epsilon, and... |

**Further resources:** 10 paper, 8 article or blog, 2 docs load; 0 broken, 0 unconfirmed.

### 5.3 Caches, queues, heaps and bit sets

| # | Section | Source | About |
|---|---|---|---|
| 5.3.1 | `cache_policy_zoo` | recipe · advanced | a cache replacement policy zoo: FIFO, LRU, LFU, CLOCK, 2Q, ARC, SIEVE, W-TinyLFU-lite and Belady's OPT |
| 5.3.2 | `lru_cache` | recipe · advanced | an LRU cache: HashMap + a doubly linked list stored in a Vec with INDICES (no unsafe, no Rc<RefCell>) |
| 5.3.3 | `ring_buffer_zoo` | recipe · intermediate | a ring buffer zoo: fixed ring, growable deque, flight recorder, byte ring, sliding-window stats |
| 5.3.4 | `priority_queue_zoo` | recipe · advanced | a priority queue zoo: pairing heap, leftist heap, indexed 4-ary heap, Dial's bucket queue, radix heap |
| 5.3.5 | `heap_patterns` | recipe · intermediate | heaps from scratch (binary, d-ary, indexed) and the classic heap patterns |
| 5.3.6 | `heap_tiebreak` | recipe · simple | BinaryHeap as a min-heap with a deterministic tie-break |
| 5.3.7 | `bitset_rank_select_roaring` | recipe · advanced | bitsets, rank/select, and Roaring-style compressed bitmaps |
| 5.3.8 | `bdd_zdd` | recipe · advanced | reduced ordered BDDs (Bryant) and ZDDs (Minato): canonical boolean functions and families of sets |

**Further resources:** 12 article or blog, 4 code, 3 paper, 3 docs, 1 book load; 0 broken, 0 unconfirmed.

### 5.4 Graph algorithms

| # | Section | Source | About |
|---|---|---|---|
| 5.4.1 | `csr_graph` | recipe · intermediate | CSR (compressed sparse row) graph: counting-sort build, reverse graph, BFS and PageRank vs Vec<Vec<u32>> |
| 5.4.2 | `index_graph` | recipe · intermediate | index-based graph, interning names once, iterative BFS |
| 5.4.3 | `dijkstra` | recipe · intermediate | Dijkstra's shortest paths with a binary heap (lazy deletion) and path reconstruction |
| 5.4.4 | `shortest_path_zoo` | recipe · intermediate | shortest-path zoo: Bellman-Ford, Floyd-Warshall, A*, bidirectional BFS, 0-1 BFS (and which to choose) |
| 5.4.5 | `mst_bridges` | recipe · advanced | minimum spanning tree (Prim, Kruskal) and bridges / articulation points via iterative low-link DFS |
| 5.4.6 | `bipartite_matching` | recipe · intermediate | bipartite matching (Kuhn's augmenting paths): assign each worker to at most one job |
| 5.4.7 | `max_flow_dinic` | recipe · advanced | max-flow with Dinic's algorithm, min-cut extraction, bipartite matching and project selection |
| 5.4.8 | `union_find` | recipe · intermediate | union-find (disjoint sets): "are these two connected?" in near-constant time |
| 5.4.9 | `tarjan_scc` | recipe · advanced | strongly connected components (iterative Tarjan): find circular groups in a directed graph |

**Further resources:** 15 article or blog, 1 code load; 1 broken, 0 unconfirmed.

### 5.5 Dynamic programming, greedy and search

| # | Section | Source | About |
|---|---|---|---|
| 5.5.1 | DP, trees and specialized algorithms | compiler guide | |
| 5.5.2 | Algorithms and concurrency in code | compiler guide | |
| 5.5.3 | `dp_and_backtracking_questions` | recipe · advanced | dynamic programming and backtracking: the most-asked coding questions, answered in Rust |
| 5.5.4 | `dp_count_paths` | recipe · intermediate | counting routes through a DAG: top-down memoization vs bottom-up, on the same recurrence |
| 5.5.5 | `dp_edit_distance` | recipe · intermediate | edit distance (Levenshtein) and "did you mean ...?" suggestions |
| 5.5.6 | `dp_knapsack` | recipe · intermediate | 0/1 knapsack: pick items under a capacity to maximise value, and recover WHICH items |
| 5.5.7 | `dp_lcs_diff` | recipe · intermediate | longest common subsequence, and turning it into a diff between two versions |
| 5.5.8 | `dp_memoization` | recipe · simple | memoization vs tabulation on coin change, and why greedy is not enough |
| 5.5.9 | `dp_zoo` | recipe · advanced | a zoo of DP shapes, each with a brute-force oracle and seeded differential tests |
| 5.5.10 | `greedy_and_interval_algorithms` | recipe · intermediate | greedy algorithms and interval problems, each with its proof technique AND a counterexample where the tempting greedy... |
| 5.5.11 | `backtracking_branch_bound` | recipe · advanced | backtracking, branch and bound, and constraint propagation on NP-hard problems |
| 5.5.12 | `heuristics_zoo` | recipe · advanced | heuristics and approximation zoo, each checked against an exact solver on small instances |
| 5.5.13 | `problem_modeling_reductions` | recipe · advanced | problem modeling by reduction (Skiena): puzzles as shortest paths, 2-SAT, assignment, bipartite, scheduling, Steiner,... |
| 5.5.14 | `combinatorial_generation` | recipe · advanced | generating combinatorial objects as iterators (Gray codes, combinations, permutations, partitions, Catalan, necklaces... |
| 5.5.15 | `combo_algorithms` | recipe · advanced | problems that need several data structures working together |
| 5.5.16 | `sat_solver_cdcl` | recipe · advanced | a SAT solver, from DPLL to CDCL (watched literals, 1UIP learning, VSIDS, restarts), plus problem encodings |
| 5.5.17 | `procedural_puzzles` | recipe · advanced | procedural puzzles with ASCII pictures: mazes, Sudoku, queens, WFC, tours, Hanoi, Gray codes |

**Further resources:** 28 article or blog, 2 code, 2 paper, 1 book load; 1 broken, 0 unconfirmed.

### 5.6 Sorting, strings, bits and theory

| # | Section | Source | About |
|---|---|---|---|
| 5.6.1 | `sorting_searching` | recipe · advanced | sorting and searching toolkit, differential-tested against std |
| 5.6.2 | `sorting_networks_external_sort` | recipe · advanced | TAOCP Vol 3 sorting: sorting networks (0-1 principle), merge insertion, replacement selection, external k-way merge, ... |
| 5.6.3 | `kway_merge_family` | recipe · advanced | the k-way merge family: heap, loser tree, lazy iterator, LSM compaction, merge path |
| 5.6.4 | `string_algos` | recipe · advanced | KMP, Z-algorithm, Rabin-Karp and the suffix array + Kasai LCP, with differential tests |
| 5.6.5 | `aho_corasick` | recipe · advanced | Aho-Corasick multi-pattern matching: trie + failure links + dictionary links |
| 5.6.6 | `bit_tricks` | recipe · intermediate | bit-twiddling toolbox (Hacker's Delight / TAOCP 7.1.3), every trick checked against std or a naive loop |
| 5.6.7 | `bit_manipulation_real_world` | recipe · intermediate | bit manipulation with REAL examples: flags, CIDR, packed headers, Snowflake/ULID ids, bit streams, chess bitboards, b... |
| 5.6.8 | `algorithmic_optimization_toolbox` | recipe · advanced | algorithmic optimisation toolbox: naive vs optimised pairs with asserted operation counts |
| 5.6.9 | `amortized_analysis_online_algorithms` | recipe · advanced | amortised analysis (aggregate / accounting / potential) and online algorithms (competitive analysis) |
| 5.6.10 | `approximation_randomized_algorithms` | recipe · advanced | approximation algorithms with proven ratios, and randomised algorithms with measured error probabilities |
| 5.6.11 | `linear_programming_simplex` | recipe · advanced | linear programming: the simplex method, duality, sensitivity, LP models, branch and bound |

**Further resources:** 26 article or blog, 5 code, 2 docs load; 0 broken, 0 unconfirmed.

### 5.7 Mathematics and numerics

| # | Section | Source | About |
|---|---|---|---|
| 5.7.1 | `bigint_arithmetic` | recipe · advanced | arbitrary-precision unsigned integers: limbs, Karatsuba, Knuth division, gcd |
| 5.7.2 | `computational_geometry` | recipe · advanced | computational geometry: exact predicates, hulls, triangulations, collision |
| 5.7.3 | `decimal_fixed_point_money` | recipe · intermediate | money done right: integer minor units, decimal fixed point, rounding modes, allocation, ledgers |
| 5.7.4 | `floating_point_ieee754` | recipe · advanced | IEEE 754 in depth: bit layouts, soft-float rounding, ulps, cancellation, summation, comparison, round-trip printing |
| 5.7.5 | `number_theory_primes` | recipe · advanced | number-theory toolkit: sieves (plain/odd/wheel/segmented/linear), phi & mu, Pollard-Brent, CRT, BSGS, Tonelli-Shanks,... |
| 5.7.6 | `numerics_and_math` | recipe · advanced | numerical methods and floating-point wisdom, each checked against a closed form |
| 5.7.7 | `physics_simulation` | recipe · advanced | physics simulation: integrators, fixed timestep, cloth, collisions, N-body |
| 5.7.8 | `prng_zoo` | recipe · intermediate | a PRNG zoo: LCG, xorshift64*, SplitMix64, PCG32, ChaCha8, and how to test them |
| 5.7.9 | `space_filling_curves_zorder_hilbert` | recipe · intermediate | space-filling curves: Morton/Z-order, Hilbert, BIGMIN/LITMAX, range decomposition, data-skipping layouts |
| 5.7.10 | `spatial_search_structures` | recipe · advanced | spatial search structures: k-d tree, quadtree, STR R-tree, grid hash, BK-tree, VP-tree |

**Further resources:** 30 article or blog, 3 docs, 1 code, 1 paper load; 0 broken, 0 unconfirmed.

## 6. Concurrency and async

Threads, atomics, lock-free structures and async Rust.

### 6.1 Threads, locks and channels

| # | Section | Source | About |
|---|---|---|---|
| 6.1.1 | `threads_channels` | recipe · intermediate | OS threads: spawn/join, scoped threads, channels, and what `Send`/`Sync` mean |
| 6.1.2 | `mutex_condvar_channel` | recipe · advanced | build locks, a condvar, a bounded MPMC channel, a semaphore and a barrier from simpler pieces |
| 6.1.3 | `shared_state` | recipe · intermediate | shared state: `Arc<Mutex<_>>` vs an actor that owns the state |
| 6.1.4 | `sharded_map` | recipe · intermediate | a concurrent HashMap by lock striping: N shards, each `RwLock<HashMap>` |
| 6.1.5 | `locks_deadlocks` | recipe · advanced | Mutex, RwLock, poisoning, lock ordering, try_lock, and Condvar |
| 6.1.6 | `actor_model` | recipe · advanced | a small actor framework on tokio: mailboxes, request/reply, supervision with restarts, registry |
| 6.1.7 | `crossbeam_select_channels` | recipe · intermediate | crossbeam-channel patterns: rendezvous, select!, timers, pipelines, request/reply, broadcast |
| 6.1.8 | `rayon_data_parallelism` | recipe · intermediate | data parallelism with rayon: parallel iterators, fold/reduce, join, scope, thread pools |
| 6.1.9 | `thread_pool_work_stealing` | recipe · advanced | a work-stealing thread pool on crossbeam-deque vs a single shared-queue pool |
| 6.1.10 | `joinset_semaphore` | recipe · intermediate | bounded concurrency with JoinSet + Semaphore, and PROVING the bound |

**Further resources:** 12 docs, 4 code, 2 book, 2 article or blog load; 0 broken, 0 unconfirmed.

### 6.2 Atomics and memory ordering

| # | Section | Source | About |
|---|---|---|---|
| 6.2.1 | Effects, async and memory models | compiler guide | |
| 6.2.2 | `atomics_ordering` | recipe · advanced | atomics, and what `Ordering` actually buys you |
| 6.2.3 | `own_arc_refcount` | recipe · advanced | your own `Arc<T>` and `Weak<T>` from scratch: atomic refcounts, Release/Acquire on drop, CAS upgrade |
| 6.2.4 | `seqlock_and_priority_inversion` | recipe · advanced | seqlock for tear-free multi-word reads, and priority inversion vs inheritance vs ceiling |
| 6.2.5 | `false_sharing` | recipe · advanced | false sharing: two threads, two separate counters, one cache line, 5-10x slower |

**Further resources:** 5 article or blog, 3 docs, 2 book load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Bos, Rust Atomics and Locks](https://marabos.nl/atomics/) (book); [Sewell et al., C/C++11 mappings to processors](https://www.cl.cam.ac.uk/~pes20/cpp/cpp0xmappings.html) (article or blog).

### 6.3 Concurrent data structures

| # | Section | Source | About |
|---|---|---|---|
| 6.3.1 | `spsc_ring_buffer` | recipe · advanced | a lock-free single-producer single-consumer (SPSC) bounded ring buffer |
| 6.3.2 | `mpmc_bounded_queue` | recipe · advanced | bounded lock-free MPMC queue (Vyukov style): a ring of sequence-numbered cells |
| 6.3.3 | `mpmc_queues_michael_scott_and_combining` | recipe · advanced | four MPMC queues side by side: Michael-Scott (tagged indices), two-lock, flat combining, bounded ring |
| 6.3.4 | `lockfree_stack_aba` | recipe · advanced | lock-free Treiber stack, the ABA problem, and tagged heads that defeat it |
| 6.3.5 | `concurrent_counters_and_statistics` | recipe · advanced | concurrent counters and statistics: sharded, thread-local+flush, sloppy, Morris, histograms, HyperLogLog merge |
| 6.3.6 | `concurrent_hashmap_designs` | recipe · advanced | concurrent hash map designs: lock striping, per-bucket RwLock, seqlock readers, incremental resize |
| 6.3.7 | `concurrent_skiplist_btree_ordered_map` | recipe · advanced | concurrent ordered map: the lazy skip list (Herlihy, Lev, Luchangco, Shavit) with range scans |
| 6.3.8 | `epoch_hazard_pointer_reclamation` | recipe · advanced | safe memory reclamation for lock-free structures: leak vs hazard pointers vs epochs |
| 6.3.9 | `read_copy_update_and_hazard_patterns` | recipe · advanced | read-mostly sharing patterns: RCU with grace periods, left-right, and cached ArcSwap-style pointers |
| 6.3.10 | `snapshot_cow_rcu` | recipe · advanced | read-mostly shared state with copy-on-write snapshots (RCU style) |

**Further resources:** 9 docs, 7 article or blog, 3 code, 3 paper, 1 book load; 0 broken, 0 unconfirmed.

### 6.4 Async Rust

| # | Section | Source | About |
|---|---|---|---|
| 6.4.1 | `async_cancel_safety` | recipe · advanced | cancellation safety: why `select!` can silently lose data, and the fix |
| 6.4.2 | `async_patterns` | recipe · intermediate | join!, spawn, JoinSet, spawn_blocking, timeouts, and panic isolation in tokio |
| 6.4.3 | `async_streams_concurrency` | recipe · intermediate | bounded concurrency with Streams: `buffer_unordered`, `buffered`, `FuturesUnordered`, `for_each_concurrent` |
| 6.4.4 | `async_traits_dyn` | recipe · advanced | `async fn` in traits, why `dyn` needs boxing, and an adapter that makes any impl object-safe |
| 6.4.5 | `futures_by_hand` | recipe · advanced | what a Future really is: poll it by hand with a 10-line executor |
| 6.4.6 | `waker_executor` | recipe · advanced | build a real executor: tasks, wakers, a ready-queue, and a timer future that wakes from another thread |
| 6.4.7 | `select_cancel` | recipe · intermediate | cancellation with `select!`, and timeouts |
| 6.4.8 | `structured_concurrency_cancellation` | recipe · advanced | structured concurrency on tokio: CancellationToken tree, TaskTracker, fail-fast Scope, shutdown ordering |
| 6.4.9 | `graceful_shutdown` | recipe · intermediate | graceful shutdown: stop accepting work, drain what is queued, then exit |
| 6.4.10 | `bounded_channel` | recipe · intermediate | bounded channels and backpressure |
| 6.4.11 | `worker_pool` | recipe · advanced | a generic async worker pool: bounded queue, bounded concurrency, panic isolation, graceful drain |

**Further resources:** 11 docs, 5 article or blog, 1 code load; 0 broken, 0 unconfirmed.

## 7. Performance and systems

Making code fast and understanding the machine and operating system.

### 7.1 Measuring and CPU performance

| # | Section | Source | About |
|---|---|---|---|
| 7.1.1 | Performance: data layout, SIMD, threads and lock-free structures | compiler guide | |
| 7.1.2 | `perf_checklist` | recipe · simple | the performance workflow and checklist: what to try, in what order, and how to verify |
| 7.1.3 | `branches_vectorization` | recipe · intermediate | branch prediction, branchless code, bounds checks, and auto-vectorisation (SIMD) friendliness |
| 7.1.4 | `inlining_codegen` | recipe · advanced | inlining, `#[inline]`, `#[cold]`, dynamic dispatch cost, and the build settings that matter |
| 7.1.5 | `simd_intrinsics` | recipe · advanced | explicit SIMD with `std::arch`: NEON and SSE2/AVX2 kernels, runtime dispatch, and tail handling |
| 7.1.6 | `matmul_optimization` | recipe · intermediate | matrix multiplication optimisation ladder: loop order, transposition, tiling, register blocking, SIMD, threads |
| 7.1.7 | `fast_hashing` | recipe · intermediate | the default HashMap hasher (SipHash) vs a fast non-cryptographic one, and when NOT to swap it |

**Further resources:** 12 article or blog, 5 code, 4 docs, 2 paper load; 0 broken, 0 unconfirmed.

### 7.2 Memory layout and allocation

| # | Section | Source | About |
|---|---|---|---|
| 7.2.1 | `cache_locality` | recipe · intermediate | cache locality: row vs column order, the stride experiment, pointer chasing, and blocked algorithms |
| 7.2.2 | `data_layout` | recipe · intermediate | memory layout: struct size/padding, niches, and array-of-structs vs struct-of-arrays |
| 7.2.3 | `struct_packing` | recipe · intermediate | struct layout: padding, field order, repr(C/packed/align), niches, bit-packing, and compile-time size checks |
| 7.2.4 | `small_string_inline` | recipe · advanced | small-value optimisation: store short strings INLINE and avoid the heap entirely |
| 7.2.5 | `counting_allocator` | recipe · advanced | count heap allocations with a custom global allocator, then remove them |
| 7.2.6 | `byte_order_keys` | recipe · intermediate | byte order: endianness, memcmp-sortable keys, bulk byte swaps, unaligned reads, and `align_to` |
| 7.2.7 | `bufwriter` | recipe · simple | buffered output (and why `println!` in a loop is slow) |

**Further resources:** 7 docs, 4 article or blog, 1 code load; 0 broken, 0 unconfirmed.

### 7.3 Files, processes and the OS

| # | Section | Source | About |
|---|---|---|---|
| 7.3.1 | Disk I/O: fsync, the page cache and queues | db guide | |
| 7.3.2 | Databases on new hardware | db guide | |
| 7.3.3 | `file_disk_io_patterns` | recipe · advanced | file and disk IO patterns: pread/pwrite, vectored IO, page cache, durability, sparse files, links |
| 7.3.4 | `file_locking_atomic_replace` | recipe · intermediate | crash-safe file replacement (temp + fsync + rename + dir fsync) and file locking (flock, PID lock, O_EXCL) |
| 7.3.5 | `buffered_io_internals` | recipe · advanced | buffered IO internals: build BufReader, BufWriter, LineWriter and a prefetching reader from scratch |
| 7.3.6 | `mmap_files` | recipe · intermediate | memory-mapped files with `memmap2`: zero-copy reads, durable writes, growth, and an mmap-backed record store |
| 7.3.7 | `io_uring_model` | recipe · advanced | the io_uring DESIGN modelled in portable Rust: shared SQ/CQ rings, batching, linked SQEs, fixed buffers, async integr... |
| 7.3.8 | `zero_copy_sendfile_splice` | recipe · intermediate | zero-copy IO: read/write loop vs sendfile, io::copy, clonefile/copy_file_range, mmap, splice, bytes::Bytes |
| 7.3.9 | `process_pipes_signals` | recipe · intermediate | child processes, pipes, exit status vs signals, timeouts, and async-signal-safe signal handlers |
| 7.3.10 | `syscalls_portable` | recipe · intermediate | syscalls: what they are, how Rust reaches the kernel, fd ownership, cost, and counting them |
| 7.3.11 | `virtual_memory_sim` | recipe · advanced | virtual memory: a 4-level page-table + TLB + demand-paging simulator, and the real thing via mmap/mprotect |
| 7.3.12 | `scheduler_policies` | recipe · advanced | CPU scheduling policies: FCFS, SJF/SRTF, round-robin, priority+aging, MLFQ, DRR, stride |
| 7.3.13 | `disk_scheduling_raid` | recipe · intermediate | disk-head scheduling (FCFS/SSTF/SCAN/C-SCAN/LOOK/C-LOOK), RAID 0/1/5, and checksum scrubbing |
| 7.3.14 | `timing_wheel` | recipe · advanced | hierarchical timing wheel with O(1) insert/cancel, cascading and an overflow list |
| 7.3.15 | `civil_dates_time` | recipe · intermediate | civil dates and time from scratch: days<->civil, RFC 3339, month arithmetic, clocks |
| 7.3.16 | `wal_append_log` | recipe · advanced | a write-ahead log: length-prefixed + checksummed records, torn-write detection, and crash recovery |

**Further resources:** 28 docs, 11 paper, 7 code, 3 article or blog, 3 video load; 0 broken, 2 unconfirmed.
Examples with titles from the guides: [ScyllaDB: no-compromise performance](https://www.youtube.com/watch?v=0S6i9BmuF8U) (video); [Write-Behind Logging](https://www.vldb.org/pvldb/vol10/p337-arulraj.pdf) (paper).

### 7.4 High-performance computing

| # | Section | Source | About |
|---|---|---|---|
| 7.4.1 | `jacobi_stencil_tiling` | recipe · advanced | 2-D Jacobi/heat stencil: naive, tiled, rayon row-parallel, halo exchange with threads and channels |
| 7.4.2 | `loop_optimization_ladder` | recipe · advanced | the loop optimisation ladder: measured steps on dot product, prefix sum, convolution, histogram and friends |
| 7.4.3 | `mpi_collectives_simulation` | recipe · advanced | MPI-style collectives over a simulated message-passing network (threads + channels) with alpha-beta cost models |
| 7.4.4 | `parallel_prefix_scan_reduction` | recipe · advanced | parallel prefix scan and reduction: Hillis-Steele, Blelloch, blocked 3-phase, segmented, compaction |
| 7.4.5 | `parallel_sample_sort_radix` | recipe · advanced | parallel sample sort and parallel LSD/MSD radix sort (histogram + prefix-sum scatter) |
| 7.4.6 | `scaling_laws_amdahl_gustafson_roofline` | recipe · intermediate | scaling laws: Amdahl, Gustafson, Karp-Flatt, USL fit, STREAM bandwidth and the roofline model |
| 7.4.7 | `sparse_kernels_spmv_csr` | recipe · advanced | sparse kernels: CSR/CSC/COO/ELL/SELL-C-sigma SpMV, merge-path SpMV, SpGEMM, SpTRSV, RCM reordering |

**Further resources:** 19 article or blog, 3 docs, 2 paper load; 0 broken, 2 unconfirmed.

## 8. Databases

Layer by layer, following the database guide.

### 8.1 Foundations and SQL

| # | Section | Source | About |
|---|---|---|---|
| 8.1.1 | Relational Model & Algebra | db guide | |
| 8.1.2 | Modern SQL | db guide | |
| 8.1.3 | The DBMS stack | db guide | |
| 8.1.4 | Cairn layout | db guide | |
| 8.1.5 | `relational_algebra_engine` | recipe · intermediate | relational model and algebra: bags vs sets, operators, a typed expression evaluator, equivalence rewrites |
| 8.1.6 | `sql_window_functions_ctes` | recipe · advanced | modern SQL over in-memory tables: window functions, CTEs, recursive CTEs, LATERAL, GROUPING SETS |

**Further resources:** 8 docs, 5 article or blog, 5 paper, 4 video, 2 slides load; 0 broken, 1 unconfirmed.
Examples with titles from the guides: [WITH Queries](https://www.postgresql.org/docs/current/queries-with.html) (docs); [L01 · Relational Model & Algebra (F25)](https://www.youtube.com/watch?v=7NPIENPr-zk) (video); [What Goes Around Comes Around… And Around…](https://db.cs.cmu.edu/papers/2024/whatgoesaround-sigmodrec2024.pdf) (paper); [Homework 1](https://15445.courses.cs.cmu.edu/fall2025/homework1/) (article or blog).

### 8.2 Storage and buffer management

| # | Section | Source | About |
|---|---|---|---|
| 8.2.1 | Buffer pools | db guide | |
| 8.2.2 | ARC replacer | db guide | |
| 8.2.3 | Disk I/O | db guide | |
| 8.2.4 | Disk scheduler | db guide | |
| 8.2.5 | Buffer pool manager | db guide | |
| 8.2.6 | Pages and tuples | db guide | |
| 8.2.7 | Table heap | db guide | |
| 8.2.8 | Log-structured storage | db guide | |
| 8.2.9 | Storage models and compression | db guide | |
| 8.2.10 | In-memory DBMS design | db guide | |
| 8.2.11 | Larger-than-memory databases | db guide | |
| 8.2.12 | Columnar formats and nested data | db guide | |
| 8.2.13 | Modern encodings: BtrBlocks, FastLanes, BitWeaving | db guide | |
| 8.2.14 | `slotted_pages_heap_file` | recipe · advanced | slotted pages and a heap file: variable-length tuples, stable record ids, compaction, forwarding, overflow pages |
| 8.2.15 | `storage_models_nsm_dsm_pax` | recipe · advanced | storage models: row store (NSM), column store (DSM) and PAX, with an IO and cache-line cost meter |
| 8.2.16 | `columnar_encodings` | recipe · advanced | column-store encodings, zone maps and vectorised scans over encoded data |
| 8.2.17 | `buffer_pool_clock` | recipe · advanced | a buffer pool: page frames, pin counts with an RAII guard, dirty write-back, CLOCK / LRU / LRU-2 |
| 8.2.18 | `lru_k_replacer_disk_scheduler` | recipe · advanced | LRU-K replacer, a background disk scheduler with futures, and a concurrent buffer pool with page guards |
| 8.2.19 | `buffer_management_vmcache_swizzling` | recipe · advanced | modern buffer management: pointer swizzling + cooling FIFO (LeanStore), page-state words + optimistic latches (vmcache) |
| 8.2.20 | `lsm_compaction_strategies` | recipe · advanced | LSM compaction strategies (leveled, size-tiered, lazy leveling, FIFO) with exact amplification accounting and Monkey-... |

**Further resources:** 32 paper, 21 video, 18 docs, 10 article or blog, 7 slides, 6 code, 1 book load; 9 broken, 9 unconfirmed.
Examples with titles from the guides: [Are You Sure You Want to Use MMAP in Your DBMS?](https://db.cs.cmu.edu/papers/2022/cidr2022-p13-crotty.pdf) (paper); [format specification](https://parquet.apache.org/docs/file-format/) (article or blog); [L04 · Memory Management & Buffer Pools (F25)](https://www.youtube.com/watch?v=8-2yv4z0VZc) (video); [src/backend/storage/buffer/README](https://github.com/postgres/postgres/blob/master/src/backend/storage/buffer/README) (code); [slice::get_disjoint_mut](https://doc.rust-lang.org/std/primitive.slice.html#method.get_disjoint_mut) (docs).

### 8.3 Indexes

| # | Section | Source | About |
|---|---|---|---|
| 8.3.1 | B+Trees | db guide | |
| 8.3.2 | B+Tree pages | db guide | |
| 8.3.3 | Search and insert | db guide | |
| 8.3.4 | Delete and tombstones | db guide | |
| 8.3.5 | Index iterator | db guide | |
| 8.3.6 | Filters, tries, inverted and vector indexes | db guide | |
| 8.3.7 | Index concurrency control | db guide | |
| 8.3.8 | B+Tree concurrency | db guide | |
| 8.3.9 | Latch-free indexes and the Bw-tree | db guide | |
| 8.3.10 | Tries and the adaptive radix tree | db guide | |
| 8.3.11 | `hash_and_bitmap_indexes` | recipe · advanced | hash indexes (extendible, linear) and compressed bitmap indexes (equality/range encoded, WAH-style) |
| 8.3.12 | `succinct_wavelet_fm_index` | recipe · advanced | succinct data structures: rank/select bitvector, wavelet matrix, BWT, FM-index, Elias-Fano |
| 8.3.13 | `database_cracking_zone_maps_imprints` | recipe · intermediate | lightweight adaptive indexing: zone maps, column imprints and database cracking |
| 8.3.14 | `bitweaving_bit_sliced_scans` | recipe · advanced | bit-parallel scans over compressed codes: BitWeaving/V, BitWeaving/H and ByteSlice |

**Further resources:** 20 paper, 11 video, 9 article or blog, 4 code, 3 docs, 3 slides, 1 book load; 3 broken, 5 unconfirmed.
Examples with titles from the guides: [L08 · B+Trees: The Best Data Structure in the World (F25)](https://www.youtube.com/watch?v=u7ii_Lvm9rM) (video); [The Ubiquitous B-Tree](http://carlosproal.com/ir/papers/p121-comer.pdf) (paper); [src/backend/access/nbtree/README](https://github.com/postgres/postgres/blob/master/src/backend/access/nbtree/README) (code); [Product Quantization for Nearest Neighbor Search](https://inria.hal.science/inria-00514462/document) (article or blog); [Rust Atomics and Locks, ch. 3 and 9](https://marabos.nl/atomics/) (book).

### 8.4 Query execution

| # | Section | Source | About |
|---|---|---|---|
| 8.4.1 | Sorting and aggregation | db guide | |
| 8.4.2 | Join algorithms | db guide | |
| 8.4.3 | Query execution | db guide | |
| 8.4.4 | Catalog, types, expressions | db guide | |
| 8.4.5 | SQL front end | db guide | |
| 8.4.6 | Access-method executors | db guide | |
| 8.4.7 | Aggregation and nested-loop joins | db guide | |
| 8.4.8 | Grace hash join and spilling | db guide | |
| 8.4.9 | Sort, top-N, limit, window | db guide | |
| 8.4.10 | Parallel execution | db guide | |
| 8.4.11 | Vectorized execution with SIMD | db guide | |
| 8.4.12 | Query compilation and code generation | db guide | |
| 8.4.13 | Parallel hash joins | db guide | |
| 8.4.14 | Parallel sort-merge joins | db guide | |
| 8.4.15 | Multi-way and worst-case optimal joins | db guide | |
| 8.4.16 | Server-side logic and UDFs | db guide | |
| 8.4.17 | Execution pipelines | db guide | |
| 8.4.18 | Query scheduling | db guide | |
| 8.4.19 | DSLs and SQL parsing | compiler guide | |
| 8.4.20 | `volcano_vectorized_push` | recipe · advanced | one query, four execution models: Volcano, vectorized, push/data-centric, fused loop |
| 8.4.21 | `join_algorithms` | recipe · advanced | join algorithms compared: nested loop, block NL, index NL, hash, GRACE hash, sort-merge, outer/semi/anti, Bloom |
| 8.4.22 | `radix_partitioned_hash_join` | recipe · advanced | in-memory hash joins: no-partition join vs radix-partitioned join vs sort-merge |
| 8.4.23 | `external_sort_hash_aggregation` | recipe · advanced | external merge sort, top-N, and hash aggregation that spills (partition, recurse on skew), with IO counted in pages |
| 8.4.24 | `parallel_query_morsel_exchange` | recipe · advanced | parallel query execution: morsel-driven pipelines, exchange operators, parallel build/probe, sample sort |
| 8.4.25 | `query_compilation_data_centric_codegen` | recipe · advanced | query compilation: data-centric produce/consume codegen (closures, register bytecode, Rust source) vs Volcano and vec... |
| 8.4.26 | `leapfrog_triejoin_worst_case_optimal` | recipe · advanced | worst-case optimal joins: AGM bound, Generic Join and Leapfrog Triejoin vs binary hash-join plans |
| 8.4.27 | `dremel_nested_columnar_shredding` | recipe · advanced | Dremel/Parquet nested record shredding: repetition and definition levels, the assembly FSM, level encoding |
| 8.4.28 | `distributed_query_shuffle_broadcast_join` | recipe · advanced | distributed query processing: partitioning, pruning, two-phase aggregation, co-located/broadcast/shuffle/Bloom joins,... |

**Further resources:** 32 video, 20 paper, 20 docs, 16 article or blog, 8 code, 5 slides load; 4 broken, 15 unconfirmed.
Examples with titles from the guides: [Multi-Core, Main-Memory Joins: Sort vs. Hash Revisited](https://www.vldb.org/pvldb/vol7/p85-balkesen.pdf) (paper); [L11 · Sorting & Aggregation Algorithms (F25)](https://www.youtube.com/watch?v=LzyKTpeIgts) (video); [Fastest Table Sort in the West](https://duckdb.org/2021/08/27/external-sorting.html) (article or blog); [sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs) (code); [Rust Reference: trait object lifetime bounds](https://doc.rust-lang.org/reference/lifetime-elision.html#default-trait-object-lifetimes) (docs).

### 8.5 Query optimization

| # | Section | Source | About |
|---|---|---|---|
| 8.5.1 | Query planning and rewriting | db guide | |
| 8.5.2 | Cost models and join ordering | db guide | |
| 8.5.3 | Optimizer rules and the engine | db guide | |
| 8.5.4 | Optimizer architectures | db guide | |
| 8.5.5 | A Cascades memo in Rust | db guide | |
| 8.5.6 | Plan transformations and rule soundness | db guide | |
| 8.5.7 | Join ordering | db guide | |
| 8.5.8 | Parallel search | db guide | |
| 8.5.9 | Unnesting subqueries | db guide | |
| 8.5.10 | Statistics | db guide | |
| 8.5.11 | Cardinality estimation | db guide | |
| 8.5.12 | Cost models | db guide | |
| 8.5.13 | Adaptive and robust | db guide | |
| 8.5.14 | Production optimizers | db guide | |
| 8.5.15 | `cascades_memo_optimizer` | recipe · advanced | a Cascades-style top-down optimiser: memo of groups, transformation + implementation rules, enforcers, branch-and-bound |
| 8.5.16 | `join_enumeration_dpccp_dphyp` | recipe · advanced | join enumeration: DPsize, DPsub, DPccp, DPhyp, top-down partitioning, and GOO/IDP/linearised heuristics for huge queries |
| 8.5.17 | `cardinality_estimation_lab` | recipe · advanced | cardinality estimation lab: independence, histogram-join, wander-join sampling, Fast-AGMS sketches and a learned regr... |
| 8.5.18 | `statistics_histograms_sketches` | recipe · advanced | optimizer statistics: histograms (equi-width, equi-depth, V-optimal, MaxDiff), MCV lists, NDV sketches, sampling, mul... |
| 8.5.19 | `subquery_unnesting_decorrelation` | recipe · advanced | subquery unnesting and decorrelation with dependent joins (correlated scalar/EXISTS/IN/ANY/ALL/LATERAL -> joins) |
| 8.5.20 | `query_rewrite_rules_magic_sets` | recipe · advanced | query rewrite rules over a Query Graph Model (view merging, pushdown, join/DISTINCT elimination, outer-join simplific... |
| 8.5.21 | `parametric_query_optimization` | recipe · advanced | parametric query optimization: plan diagrams, anorexic reduction, robust plans, bouquets, generic vs custom plans |
| 8.5.22 | `adaptive_feedback_reoptimization` | recipe · advanced | learning from execution: LEO-style feedback repository, mid-query re-optimization with checkpoints, plan stitching |
| 8.5.23 | `adaptive_query_processing_eddies_sip` | recipe · advanced | adaptive query processing at runtime: eddies, runtime filters (sideways information passing), join switching, skew re... |
| 8.5.24 | `plan_cache_multiplan_trial` | recipe · advanced | plan cache, plan baselines, MongoDB-style multi-plan trial, UDF optimisation and hints |

**Further resources:** 31 video, 9 paper, 8 article or blog, 2 docs, 2 slides, 1 code load; 2 broken, 12 unconfirmed.
Examples with titles from the guides: [L15 · Database Query Optimizers Part 1 (F25)](https://www.youtube.com/watch?v=b53huOGcsZ8) (video); [The Cascades Framework for Query Optimization](https://15721.courses.cs.cmu.edu/spring2019/papers/22-optimizer1/graefe-ieee1995.pdf) (paper).

### 8.6 Transactions and concurrency control

| # | Section | Source | About |
|---|---|---|---|
| 8.6.1 | Concurrency control theory | db guide | |
| 8.6.2 | Two-phase locking | db guide | |
| 8.6.3 | Timestamp ordering and OCC | db guide | |
| 8.6.4 | Multi-version concurrency control | db guide | |
| 8.6.5 | Project 4 · MVOCC | db guide | |
| 8.6.6 | MVCC protocols compared | db guide | |
| 8.6.7 | MVCC garbage collection | db guide | |
| 8.6.8 | `transaction_theory_serializability` | recipe · advanced | Transaction theory: schedules, conflict/view serializability, recoverability classes, ACID, and Adya's isolation phen... |
| 8.6.9 | `lock_manager_deadlock` | recipe · advanced | a lock manager: multi-granularity intention locks, FIFO queues, upgrades, deadlock detection and prevention |
| 8.6.10 | `timestamp_ordering_occ` | recipe · advanced | Non-locking concurrency control: basic timestamp ordering, the Thomas write rule, optimistic CC (Kung-Robinson), and ... |
| 8.6.11 | `mvcc_snapshot_isolation` | recipe · advanced | MVCC snapshot isolation: version chains, first-committer-wins, GC, write skew, and SSI |
| 8.6.12 | `mvcc_version_storage_gc` | recipe · advanced | MVCC implementation choices: version storage (append-only O2N/N2O, time-travel, delta), garbage collection, index poi... |
| 8.6.13 | `hybrid_logical_clock_snapshots_percolator` | recipe · advanced | transactions without a classic 2PC coordinator: PERCOLATOR (client-driven snapshot isolation) and CALVIN (determinist... |

**Further resources:** 14 paper, 11 video, 5 article or blog, 4 slides, 3 docs, 1 code, 1 book load; 2 broken, 5 unconfirmed.
Examples with titles from the guides: [explicit locking](https://www.postgresql.org/docs/current/explicit-locking.html) (docs); [On Optimistic Methods for Concurrency Control](https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf) (paper); [L17 · Concurrency Control Theory (F25)](https://www.youtube.com/watch?v=tMFAgvDViAI) (video); [Hermitage](https://github.com/ept/hermitage) (code); [Concurrency Control and Recovery in Database Systems](https://www.microsoft.com/en-us/research/people/philbe/book/) (book).

### 8.7 Logging and recovery

| # | Section | Source | About |
|---|---|---|---|
| 8.7.1 | Logging | db guide | |
| 8.7.2 | Crash recovery (ARIES) | db guide | |
| 8.7.3 | Build WAL and ARIES | db guide | |
| 8.7.4 | Recovery for main-memory systems | db guide | |
| 8.7.5 | `aries_recovery` | recipe · advanced | ARIES crash recovery: WAL with LSNs and CLRs, steal/no-force buffer pool, fuzzy checkpoints, analysis/redo/undo |
| 8.7.6 | `logging_schemes_group_commit` | recipe · advanced | Write-ahead logging schemes (physical, logical, physiological), group commit, async commit, log ring buffer, checkpoints |

**Further resources:** 8 paper, 5 video, 4 docs, 2 slides, 2 article or blog load; 0 broken, 2 unconfirmed.
Examples with titles from the guides: [ARIES: A Transaction Recovery Method Supporting Fine-Granularity Locking and Par](https://web.stanford.edu/class/cs345d-01/rl/aries.pdf) (paper); [WAL internals](https://www.postgresql.org/docs/current/wal-internals.html) (docs); [L21 · Write-Ahead Logging + Shadow Paging (F25)](https://www.youtube.com/watch?v=CedEy54pe3g) (video); [Amazon Aurora: Design Considerations for High Throughput Cloud-Native Relational](https://www.amazon.science/publications/amazon-aurora-design-considerations-for-high-throughput-cloud-native-relational-databases) (article or blog).

### 8.8 Analytics, time series and streaming

| # | Section | Source | About |
|---|---|---|---|
| 8.8.1 | Modern OLAP systems | db guide | |
| 8.8.2 | Time-series databases | db guide | |
| 8.8.3 | Streaming and incremental views | db guide | |
| 8.8.4 | Lakehouses and table formats | db guide | |
| 8.8.5 | Cloud-native OLTP | db guide | |
| 8.8.6 | Building blocks | db guide | |
| 8.8.7 | Learned tuning and in-database ML | db guide | |
| 8.8.8 | Hardware-accelerated databases | db guide | |
| 8.8.9 | Query languages | db guide | |
| 8.8.10 | Embedded engines | db guide | |
| 8.8.11 | Industry systems and where next | db guide | |
| 8.8.12 | History of databases | db guide | |
| 8.8.13 | `tsm_time_series_engine` | recipe · advanced | a TSM/Prometheus-TSDB-style time-series engine: tag index with postings, WAL + head, immutable blocks, compaction, re... |
| 8.8.14 | `hypertables_continuous_aggregates` | recipe · advanced | TimescaleDB-style hypertables: time/space chunking, chunk exclusion, columnar chunk compression, retention, continuou... |
| 8.8.15 | `asof_joins_gap_filling` | recipe · intermediate | time-series joins and alignment: ASOF/window/interval joins, resampling and gap filling, ordering and duplicate polic... |
| 8.8.16 | `timeseries_sketches_downsampling_anomaly` | recipe · advanced | analytics on sample streams: LTTB / min-max downsampling, DDSketch and t-digest quantiles, irregular EWMA, Holt-Winte... |
| 8.8.17 | `incremental_view_maintenance_dbsp` | recipe · advanced | incremental view maintenance with z-sets and DBSP: I, D, z^-1, bilinear join, distinct, aggregates, recursion |
| 8.8.18 | `datalog_engine_semi_naive` | recipe · advanced | a Datalog engine: parser, safety checks, stratified negation/aggregation, naive vs semi-naive, indexes, magic sets |
| 8.8.19 | `delta_lake_transaction_log` | recipe · advanced | Delta Lake style table format: a transaction log over an object store with optimistic concurrency |
| 8.8.20 | `cloud_warehouse_micro_partitions` | recipe · advanced | cloud data warehouse core ideas: micro-partitions, pruning metadata, clustering, time travel, zero-copy clone, elasti... |
| 8.8.21 | `cdc_logical_replication_log` | recipe · advanced | change data capture and logical replication: WAL decoding in commit order, replication slots, snapshot+stream bootstr... |
| 8.8.22 | `online_schema_change` | recipe · advanced | online schema change without locking: shadow-table copy + change log (gh-ost style), expand/contract with batched bac... |

**Further resources:** 84 video, 15 paper, 14 article or blog, 8 docs, 4 code, 1 slides load; 1 broken, 4 unconfirmed.
Examples with titles from the guides: [Gorilla: A Fast, Scalable, In-Memory Time Series Database](https://www.vldb.org/pvldb/vol8/p1816-teller.pdf) (paper); [Delta transaction log protocol](https://github.com/delta-io/delta/blob/master/PROTOCOL.md) (code); [15-721 S2024 #1 · Modern OLAP Database Systems](https://www.youtube.com/watch?v=5J-I8Mj8tss) (video); [Amazon Redshift Re-invented](https://www.amazon.science/publications/amazon-redshift-re-invented) (article or blog); [How SQLite Is Tested](https://sqlite.org/testing.html) (docs).

### 8.9 Building complete engines

| # | Section | Source | About |
|---|---|---|---|
| 8.9.1 | One key space | db guide | |
| 8.9.2 | Transactions on the key space | db guide | |
| 8.9.3 | A column store | db guide | |
| 8.9.4 | A vectorized engine | db guide | |
| 8.9.5 | A bytecode virtual machine | db guide | |
| 8.9.6 | HTAP: one table, two engines | db guide | |
| 8.9.7 | A durable LSM tree | db guide | |
| 8.9.8 | The graph store | db guide | |
| 8.9.9 | The document store | db guide | |
| 8.9.10 | The time-series engine | db guide | |
| 8.9.11 | Full-text search | db guide | |
| 8.9.12 | Streams and windows | db guide | |
| 8.9.13 | One planner for many models | db guide | |
| 8.9.14 | The PostgreSQL wire protocol | db guide | |
| 8.9.15 | The engine in WebAssembly | db guide | |
| 8.9.16 | Passwords and SCRAM | db guide | |
| 8.9.17 | Roles, privileges and row-level security | db guide | |
| 8.9.18 | Passing BusTub's tests | db guide | |
| 8.9.19 | Networking protocols | db guide | |

**Further resources:** 11 docs, 7 article or blog, 6 paper, 3 video, 2 code load; 5 broken, 0 unconfirmed.
Examples with titles from the guides: [Tuple layer specification](https://github.com/apple/foundationdb/blob/main/design/tuple.md) (code); [Computing / Storage mapping](https://docs.pingcap.com/tidb/stable/tidb-computing) (docs); [A Critique of ANSI SQL Isolation Levels](https://arxiv.org/abs/cs/0701157) (paper); [Large-scale Incremental Processing Using Distributed Transactions and Notificati](https://research.google/pubs/large-scale-incremental-processing-using-distributed-transactions-and-notifications/) (article or blog); [15-721 S2024 #12 · Database Networking Protocols](https://www.youtube.com/watch?v=0tfDldfzlR8) (video).

## 9. Search and vectors

Text search, similarity and approximate nearest neighbours.

### 9.1 Text search and autocomplete

| # | Section | Source | About |
|---|---|---|---|
| 9.1.1 | `trigram_index` | recipe · advanced | trigram inverted index: filter with the index, then verify |
| 9.1.2 | `autocomplete_engine` | recipe · advanced | type-ahead autocomplete: trie with cached top-k, decayed scores, bounded fuzzy search |

**Further resources:** 3 article or blog, 1 code load; 0 broken, 0 unconfirmed.

### 9.2 Vector search

| # | Section | Source | About |
|---|---|---|---|
| 9.2.1 | Vector databases | db guide | |
| 9.2.2 | `knn_bruteforce` | recipe · intermediate | brute-force cosine kNN with a top-k heap: the baseline every ANN index is measured against |
| 9.2.3 | `ivf_flat` | recipe · advanced | IVF-Flat: k-means partitions the vectors into inverted lists; search probes only the nearest few lists |
| 9.2.4 | `hnsw_lite` | recipe · advanced | a simplified HNSW: layered proximity graphs, greedy descent, best-first search with `ef` |
| 9.2.5 | `vamana_diskann_lite` | recipe · advanced | a DiskANN / Vamana-style single-layer graph index with GreedySearch, RobustPrune and "node read" accounting |
| 9.2.6 | `spann_disk_ivf_lite` | recipe · advanced | SPANN-style disk-resident ANN: centroids in RAM, posting lists on a simulated SSD, boundary replication, query-aware ... |
| 9.2.7 | `product_quantization` | recipe · advanced | Product Quantization: compress vectors to m bytes and search with Asymmetric Distance Computation (ADC) |
| 9.2.8 | `scalar_binary_quantization_rescoring` | recipe · intermediate | scalar, binary and RaBitQ-style quantisation with oversample-and-rescore, compared with product quantisation |
| 9.2.9 | `vector_distance_kernels_simd` | recipe · intermediate | distance kernels for vector search: scalar, multi-accumulator, autovectorised, AVX2/FMA and NEON, f16/bf16/i8 storage |
| 9.2.10 | `dimensionality_reduction_jl_pca` | recipe · intermediate | dimensionality reduction for vector search: Johnson-Lindenstrauss projections (Gaussian, sparse, Hadamard), PCA, whit... |

**Further resources:** 8 video, 7 paper, 6 code, 2 article or blog, 1 docs load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Efficient and Robust Approximate Nearest Neighbor Search Using Hierarchical Navi](https://arxiv.org/abs/1603.09320) (paper); [ML⇄DB #1 · Qdrant (Andrey Vasnetsov)](https://www.youtube.com/watch?v=bU38Ovdh3NY) (video).

### 9.3 Running a vector database

| # | Section | Source | About |
|---|---|---|---|
| 9.3.1 | The vector store | db guide | |
| 9.3.2 | `filtered_ann_hybrid_search` | recipe · advanced | filtered ANN (pre / post / in-algorithm filtering with a selectivity planner) and hybrid BM25 + vector search with RRF |
| 9.3.3 | `vector_index_updates_persistence` | recipe · advanced | operating a vector index: deletes (tombstone vs graph repair vs rebuild), LSM segments, WAL, snapshots, crash recovery |
| 9.3.4 | `ann_evaluation_harness` | recipe · intermediate | evaluating ANN indexes honestly: recall definitions, Pareto curves, tail latency, dataset difficulty (contrast, LID, ... |
| 9.3.5 | `mini_vector_database` | recipe · capstone | an embedded mini vector database: schema'd collections, WAL + memtable + immutable HNSW-lite segments, SQ8 with resco... |

**Further resources:** 7 paper, 3 article or blog load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [DiskANN: Fast Accurate Billion-point Nearest Neighbor Search on a Single Node](https://proceedings.neurips.cc/paper/2019/hash/09853c7fb1d3f8ee67a61b6bf4a7f8e6-Abstract.html) (article or blog).

## 10. Distributed systems

Replication, consensus, collaboration and streams.

### 10.1 Time, partitioning and replication

| # | Section | Source | About |
|---|---|---|---|
| 10.1.1 | Distributed databases I | db guide | |
| 10.1.2 | Sharding | db guide | |
| 10.1.3 | `consistent_hashing` | recipe · intermediate | consistent hashing ring with virtual nodes |
| 10.1.4 | `hash_partitioning` | recipe · intermediate | partitioning keys across N nodes: modulo, consistent-hash ring, rendezvous (HRW), Jump Consistent Hash, Maglev table |
| 10.1.5 | `vector_clocks_hlc` | recipe · intermediate | Lamport clocks, vector clocks / version vectors, hybrid logical clocks and causal delivery |
| 10.1.6 | `clock_sync_and_trace_context` | recipe · intermediate | clock synchronisation (Cristian, NTP, Berkeley, slew vs step) and W3C Trace Context |
| 10.1.7 | `quorum_replication_dynamo` | recipe · advanced | Dynamo-style quorum replication, hinted handoff, read repair, version vectors |
| 10.1.8 | `swim_gossip` | recipe · advanced | SWIM failure detection and gossip membership, simulated |
| 10.1.9 | `chandy_lamport_snapshot` | recipe · advanced | Chandy-Lamport distributed snapshot (consistent cuts) on FIFO channels |

**Further resources:** 8 article or blog, 6 paper, 1 docs, 1 code, 1 video, 1 slides load; 0 broken, 1 unconfirmed.
Examples with titles from the guides: [A Fast, Minimal Memory, Consistent Hash Algorithm](https://arxiv.org/abs/1406.2294) (paper); [L23 · Distributed Databases Pt. 1 (F25)](https://www.youtube.com/watch?v=IFLQBWY6dlE) (video); [architecture docs](https://www.cockroachlabs.com/docs/stable/architecture/overview.html) (article or blog).

### 10.2 Consensus and distributed transactions

| # | Section | Source | About |
|---|---|---|---|
| 10.2.1 | Distributed databases II | db guide | |
| 10.2.2 | Raft | db guide | |
| 10.2.3 | Distributed transactions | db guide | |
| 10.2.4 | `raft_consensus` | recipe · advanced | Raft consensus in a deterministic simulation: elections, log replication, repair, crashes and partitions |
| 10.2.5 | `paxos_basic_and_multi` | recipe · advanced | Paxos: single-decree safety under chaos, dueling-proposer livelock, and Multi-Paxos replicated log |
| 10.2.6 | `two_phase_commit` | recipe · advanced | atomic commit: two-phase commit with write-ahead logs, crash recovery, blocking, presumed abort/commit, and 3PC |
| 10.2.7 | `saga_orchestrator` | recipe · advanced | the saga pattern: distributed transactions without 2PC, via compensation, a durable log and idempotent steps |
| 10.2.8 | `spanner_truetime_commit_wait` | recipe · advanced | Spanner external consistency: TrueTime intervals, commit wait, 2PL + 2PC over replicated groups, snapshot reads, safe... |

**Further resources:** 12 article or blog, 10 paper, 3 video, 1 book, 1 slides load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [In Search of an Understandable Consensus Algorithm (Raft)](https://raft.github.io/raft.pdf) (paper); [Consensus on Transaction Commit](https://www.microsoft.com/en-us/research/publication/consensus-on-transaction-commit/) (article or blog); [L24 · Distributed Databases Pt. 2 (F25)](https://www.youtube.com/watch?v=pQh5fka3FC0) (video).

### 10.3 Collaboration and CRDTs

| # | Section | Source | About |
|---|---|---|---|
| 10.3.1 | CRDTs | db guide | |
| 10.3.2 | `crdts` | recipe · advanced | state-based CRDTs (counters, registers, sets, map) plus an RGA sequence CRDT, verified by law tests |
| 10.3.3 | `crdt_todo_app_no_db` | recipe · intermediate | an offline-first collaborative TODO list with NO database: composed CRDTs + three list orderings (RGA, Logoot/LSEQ, F... |
| 10.3.4 | `crdt_fugue_text_editor` | recipe · advanced | a collaborative plain-text engine: Fugue (and a YATA/Yjs-style integrate to compare) with an O(log n) order-statistic... |
| 10.3.5 | `crdt_block_editor_notion_like` | recipe · advanced | a Notion/Outline-like block editor as a CRDT: a movable block TREE, Fugue text per block, Peritext-style marks, split... |
| 10.3.6 | `operational_transform` | recipe · advanced | operational transformation (OT) for plain text, with a central-server client/server model |

**Further resources:** 6 article or blog, 3 paper, 1 code load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Conflict-free Replicated Data Types](https://inria.hal.science/inria-00609399) (article or blog).

### 10.4 Streams, messaging and reliability

| # | Section | Source | About |
|---|---|---|---|
| 10.4.1 | `kafka_log_model` | recipe · advanced | a model of Kafka's core: partitioned segmented logs, retention, compaction, idempotent producer, consumer groups, rep... |
| 10.4.2 | `record_batch_codec` | recipe · intermediate | a Kafka-style record batch codec: zigzag varints, CRC32C, delta-encoded records, zero-copy decode |
| 10.4.3 | `stream_processing_windows` | recipe · advanced | stream processing: event-time windows, watermarks, triggers, late data, aligned checkpoint barriers, exactly-once eff... |
| 10.4.4 | `retry_backoff_jitter` | recipe · simple | exponential backoff with full jitter |
| 10.4.5 | `token_bucket` | recipe · intermediate | token-bucket rate limiter with an injectable clock |

**Further resources:** 7 article or blog, 3 paper, 2 code, 1 docs load; 1 broken, 0 unconfirmed.

### 10.5 Microservice patterns

| # | Section | Source | About |
|---|---|---|---|
| 10.5.1 | `api_gateway_service_discovery` | recipe · advanced | API gateway and service discovery: routing, TTL registry, load balancing, circuit breaker, bulkhead, retry budget, de... |
| 10.5.2 | `idempotency_dedup_leader_fencing_store` | recipe · advanced | doing things once: idempotency keys, consumer dedup windows, CAS leader election with fencing, and a fenced resource ... |
| 10.5.3 | `strangler_fig_shadow_traffic` | recipe · advanced | migrating off a legacy service: strangler-fig routing, shadow-traffic diffing, canary analysis with rollback, dual wr... |
| 10.5.4 | `transactional_outbox_inbox_sqlite` | recipe · intermediate | transactional OUTBOX + INBOX on SQLite: atomic state+event, leased relays, idempotent consumers, crash injection |

**Further resources:** 10 article or blog, 2 docs, 1 code load; 0 broken, 0 unconfirmed.

## 11. Web, networking and security

HTTP services, wire protocols, cryptography and encodings.

### 11.1 Web services

| # | Section | Source | About |
|---|---|---|---|
| 11.1.1 | `axum_errors` | recipe · intermediate | one error type -> consistent HTTP error responses |
| 11.1.2 | `axum_idempotency` | recipe · intermediate | idempotency keys for POST |
| 11.1.3 | `axum_middleware` | recipe · intermediate | middleware with `from_fn`: correlation ids on every request and response |
| 11.1.4 | `axum_router_state` | recipe · intermediate | routes, shared state, JSON in/out, path params, and testing a router WITHOUT a socket |
| 11.1.5 | `axum_sse_replay` | recipe · intermediate | Server-Sent Events with `Last-Event-ID` replay |
| 11.1.6 | `rest_api_patterns` | recipe · advanced | REST API design patterns, shown as real HTTP transcripts |
| 11.1.7 | `backend_request_validation_extractors` | recipe · intermediate | request parsing + validation: custom extractors, composable rules, problem+json `errors` map |
| 11.1.8 | `jsonrpc_graphql_dataloader` | recipe · advanced | JSON-RPC 2.0, a mini GraphQL executor, and the DataLoader fix for N+1 |
| 11.1.9 | `multipart_forms_uploads` | recipe · intermediate | form posts and file uploads: urlencoded + multipart/form-data parsed from scratch |
| 11.1.10 | `webhooks_signatures` | recipe · advanced | webhooks end to end: HMAC signatures, retries with backoff, replay protection, dead letters |
| 11.1.11 | `http_caching_conditional` | recipe · advanced | HTTP caching and conditional requests: ETag, Last-Modified, Cache-Control, Vary, Range, If-Match |
| 11.1.12 | `http_client_resilience` | recipe · advanced | a resilient HTTP client against a deliberately flaky server |
| 11.1.13 | `http_client_server_loopback` | recipe · intermediate | the anatomy of an HTTP/1.1 exchange: raw bytes on the wire, then the same endpoints with reqwest |
| 11.1.14 | `reverse_proxy_load_balancer` | recipe · advanced | a reverse proxy / load balancer in front of three backends (axum + reqwest) |

**Further resources:** 21 docs, 4 article or blog load; 0 broken, 0 unconfirmed.

### 11.2 Authentication and web security

| # | Section | Source | About |
|---|---|---|---|
| 11.2.1 | `auth_basic_bearer_jwt` | recipe · advanced | authentication on the wire: Basic, API keys, Bearer tokens and a from-scratch HS256 JWT |
| 11.2.2 | `oauth2_pkce_flow` | recipe · advanced | OAuth 2.0 authorization code flow with PKCE, three servers, full redirect chain |
| 11.2.3 | `cookies_sessions_csrf` | recipe · advanced | cookies, server-side sessions, signed cookies and CSRF protection, by hand |
| 11.2.4 | `cors_security_headers` | recipe · intermediate | CORS from first principles, security headers, body limits, timeouts and Retry-After |
| 11.2.5 | `backend_authz_rbac_abac_policies` | recipe · advanced | authorisation: RBAC + ABAC + ownership + tenant scoping as a pure, truth-table-tested policy |
| 11.2.6 | `backend_password_accounts_totp_mfa` | recipe · advanced | account lifecycle: password hashing (PBKDF2 from scratch), email verify, reset, lockout, sessions, TOTP + recovery codes |
| 11.2.7 | `security_pitfalls_secure_coding` | recipe · intermediate | security pitfalls as testable vulnerable/fixed pairs: injection, traversal, SSRF, XSS, ReDoS, timing, randomness, lim... |

**Further resources:** 16 docs, 5 article or blog load; 0 broken, 0 unconfirmed.

### 11.3 Networking and protocols

| # | Section | Source | About |
|---|---|---|---|
| 11.3.1 | `dns_message_codec` | recipe · advanced | DNS wire format (RFC 1035): header, questions, A/AAAA/CNAME/NS/MX/TXT records, name compression |
| 11.3.2 | `http1_parser` | recipe · advanced | HTTP/1.1 message parser and serializer: framing, request smuggling defence, pipelining, fuzzing |
| 11.3.3 | `kqueue_epoll_raw` | recipe · advanced | a raw readiness poller over kqueue (macOS/BSD) and epoll (Linux) behind one trait; level vs edge; 20-client echo server |
| 11.3.4 | `mio_event_loop` | recipe · advanced | a single-threaded readiness event loop with `mio`: edge-triggered IO, per-connection state machines, a `Waker` |
| 11.3.5 | `packet_parsing_checksums` | recipe · advanced | parsing and building packets from raw bytes: Ethernet/VLAN, ARP, IPv4/IPv6, TCP, UDP, ICMP, Internet checksum, fragme... |
| 11.3.6 | `protobuf_grpc_wire_format` | recipe · advanced | Protocol Buffers wire format by hand (schema parser, reflective codec, compatibility rules) and the gRPC framing layer |
| 11.3.7 | `resp_protocol` | recipe · advanced | RESP2, the Redis wire protocol: an incremental zero-copy parser, serializer, limits and a mini server core |
| 11.3.8 | `tcp_congestion_control_sim` | recipe · advanced | discrete-event simulation of TCP congestion control (Reno, NewReno, CUBIC, simplified BBR) over a drop-tail bottleneck |
| 11.3.9 | `tcp_framed_server` | recipe · advanced | a length-prefixed TCP message server, built twice: thread-per-connection (std) and task-per-connection (tokio) |
| 11.3.10 | `tls_handshake_x509_der` | recipe · advanced | ASN.1 DER, an X.509 certificate parser and chain validator, and a model of the TLS 1.3 handshake with its HKDF key sc... |
| 11.3.11 | `udp_sockets_reliable` | recipe · advanced | real UDP on loopback, socket options, then RELIABLE delivery built on a lossy datagram channel (stop-and-wait, Go-Bac... |
| 11.3.12 | `websocket_frames` | recipe · intermediate | WebSocket framing (RFC 6455): handshake accept key (SHA-1 + base64), frames, masking, fragmentation |

**Further resources:** 22 docs, 9 article or blog, 2 code load; 0 broken, 0 unconfirmed.

### 11.4 Cryptography

| # | Section | Source | About |
|---|---|---|---|
| 11.4.1 | `aes_modes` | recipe · advanced | AES-128 from scratch with ECB, CBC (+padding oracle), CTR and GCM |
| 11.4.2 | `chacha20_poly1305` | recipe · advanced | ChaCha20, Poly1305 and the RFC 8439 AEAD, plus nonce-reuse and tamper demos |
| 11.4.3 | `constant_time_side_channels` | recipe · intermediate | constant-time code and side channels: timing leaks in `==`, a timing attack, ct_select/ct_eq, cache leaks, zeroization |
| 11.4.4 | `hash_based_signatures` | recipe · advanced | hash-based signatures: Lamport, Winternitz (WOTS, w=16), and a Merkle signature scheme (XMSS-like) |
| 11.4.5 | `hmac_hkdf_pbkdf2` | recipe · advanced | SHA-256 from scratch, HMAC, HKDF, PBKDF2, and the length-extension attack |
| 11.4.6 | `merkle_tree` | recipe · advanced | SHA-256 from scratch + Merkle tree, inclusion proofs, append-only log, anti-entropy |
| 11.4.7 | `modular_arithmetic_dh_rsa` | recipe · advanced | modular arithmetic, Miller-Rabin, Diffie-Hellman, toy RSA (+CRT, malleability), Shamir secret sharing |
| 11.4.8 | `secure_random_and_passwords` | recipe · intermediate | secure randomness, nonces and the birthday bound, UUID v4/v7, password hashing (PBKDF2, memory-hard toy), entropy |

**Further resources:** 15 docs, 8 article or blog, 3 code, 2 paper load; 1 broken, 1 unconfirmed.

### 11.5 Encodings and compression

| # | Section | Source | About |
|---|---|---|---|
| 11.5.1 | `compression_lz77_huffman` | recipe · advanced | a mini DEFLATE: LZ77 hash-chain matcher + canonical Huffman + CRC32 framing |
| 11.5.2 | `encodings_codecs` | recipe · intermediate | from-scratch codecs: hex, base64/32, percent-encoding, UTF-8/16, varints, checksums |
| 11.5.3 | `timeseries_gorilla` | recipe · advanced | Gorilla time-series compression: delta-of-delta timestamps, XOR floats, Chimp/Chimp128, an Elf-style eraser, simple8b... |
| 11.5.4 | `binary_format_parsing` | recipe · advanced | safe binary format parsing: a bounds-checked Reader, TLV, a PNG-like chunk container, bit reading, zero-copy |
| 11.5.5 | `ndjson_serde` | recipe · simple | serde beyond structs: tagged enums, renames, streaming NDJSON, tolerant line-by-line parsing |

**Further resources:** 5 docs, 4 article or blog, 2 paper load; 0 broken, 0 unconfirmed.

## 12. Compilers and languages

From text to running code, following the Lox guide and beyond.

### 12.1 Lexing and parsing

| # | Section | Source | About |
|---|---|---|---|
| 12.1.1 | The pipeline and where tests attach | compiler guide | |
| 12.1.2 | Parsing in depth | compiler guide | |
| 12.1.3 | Fully recoverable parsers | compiler guide | |
| 12.1.4 | Recoverable parsing and advanced compiler theory | compiler guide | |
| 12.1.5 | Rust techniques | compiler guide | |
| 12.1.6 | How rustc, swc and oxc handle the same problems | compiler guide | |
| 12.1.7 | `zero_copy_lexer` | recipe · intermediate | a zero-copy lexer: tokens borrow from the input via a lifetime |
| 12.1.8 | `pratt_parser` | recipe · advanced | Pratt parser: precedence and associativity with "binding power" |
| 12.1.9 | `expression_parsing_three_ways` | recipe · advanced | one expression language, four parsers (recursive descent, precedence climbing, Pratt, shunting-yard) that must agree |
| 12.1.10 | `parser_combinators` | recipe · intermediate | hand-written parser combinators: small functions that compose into a parser |
| 12.1.11 | `parser_generators_lr_peg_earley` | recipe · advanced | three general parsing techniques on the same grammars: packrat PEG, SLR(1) LR generator, Earley |
| 12.1.12 | `edge_dsl_parser` | recipe · advanced | a tiny language: tokenizer + recursive-descent parser + errors with spans |
| 12.1.13 | `json_parser` | recipe · advanced | a complete recursive-descent JSON parser with error positions, escapes, and a depth limit |
| 12.1.14 | `csv_state_machine` | recipe · intermediate | a CSV parser as an explicit character state machine, plus a round-trip property test |
| 12.1.15 | `markdown_block_parser` | recipe · intermediate | a CommonMark-subset parser that produces the BLOCK model of a block editor: two-phase parsing, delimiter-run emphasis... |

**Further resources:** 8 article or blog, 7 code, 2 docs, 1 book, 1 paper load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [matklad, Resilient LL Parsing](https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html) (article or blog).

### 12.2 A tree-walking interpreter

| # | Section | Source | About |
|---|---|---|---|
| 12.2.1 | The slice loop | compiler guide | |
| 12.2.2 | Fix these first in rlox | compiler guide | |
| 12.2.3 | Project layout | compiler guide | |
| 12.2.4 | Test harness | compiler guide | |
| 12.2.5 | Arithmetic | compiler guide | |
| 12.2.6 | Values and comparison | compiler guide | |
| 12.2.7 | Statements and globals | compiler guide | |
| 12.2.8 | Blocks and scope | compiler guide | |
| 12.2.9 | Control flow | compiler guide | |
| 12.2.10 | Functions and closures | compiler guide | |
| 12.2.11 | Resolver | compiler guide | |
| 12.2.12 | Classes and inheritance | compiler guide | |

**Further resources:** none loaded load; 0 broken, 0 unconfirmed.

### 12.3 Bytecode and virtual machines

| # | Section | Source | About |
|---|---|---|---|
| 12.3.1 | From jlox to clox | compiler guide | |
| 12.3.2 | Workspace and shared tests | compiler guide | |
| 12.3.3 | Chunks and the disassembler | compiler guide | |
| 12.3.4 | VM and the Pratt compiler | compiler guide | |
| 12.3.5 | Types, strings and interning | compiler guide | |
| 12.3.6 | Globals, locals and statements | compiler guide | |
| 12.3.7 | Jumps and control flow | compiler guide | |
| 12.3.8 | Calls and functions | compiler guide | |
| 12.3.9 | Closures and upvalues | compiler guide | |
| 12.3.10 | Classes, methods and super | compiler guide | |
| 12.3.11 | Optimization | compiler guide | |
| 12.3.12 | VM techniques in Rust | compiler guide | |
| 12.3.13 | clox tooling | compiler guide | |
| 12.3.14 | WebAssembly as a compilation target | compiler guide | |
| 12.3.15 | JIT compilation | compiler guide | |
| 12.3.16 | Runtime systems and GC implementation | compiler guide | |
| 12.3.17 | `bytecode_vm` | recipe · advanced | a mini compiler pipeline: lex -> Pratt parse -> AST -> optimise -> bytecode -> stack VM |
| 12.3.18 | `wasm_binary_interpreter` | recipe · advanced | WebAssembly (MVP subset) binary decoder + validator + interpreter, with an assembler |

**Further resources:** 6 article or blog, 3 code, 1 book, 1 docs, 1 paper load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [WebAssembly specification](https://webassembly.github.io/spec/) (article or blog); [wasmtime documentation](https://docs.wasmtime.dev) (docs); [Xu and Kjolstad 2021, “Copy-and-Patch Compilation”](https://arxiv.org/abs/2011.13127) (paper); [cranelift-jit-demo](https://github.com/bytecodealliance/cranelift-jit-demo) (code).

### 12.4 Types and semantics

| # | Section | Source | About |
|---|---|---|---|
| 12.4.1 | Semantics and the lambda calculus | compiler guide | |
| 12.4.2 | Hindley–Milner type inference | compiler guide | |
| 12.4.3 | Bidirectional typing and dependent types | compiler guide | |
| 12.4.4 | Pattern matching: compilation and exhaustiveness | compiler guide | |
| 12.4.5 | Name resolution, hygiene and modules | compiler guide | |
| 12.4.6 | Implementing Rust's hardest features | compiler guide | |
| 12.4.7 | Formal verification and mechanized semantics | compiler guide | |
| 12.4.8 | Compiling functional languages | compiler guide | |
| 12.4.9 | Metaprogramming, macros and staging | compiler guide | |
| 12.4.10 | Designing a language | compiler guide | |
| 12.4.11 | `lambda_calculus_toolkit` | recipe · intermediate | untyped lambda calculus toolkit: substitution, de Bruijn, reduction strategies, Church/Scott encodings, SKI |
| 12.4.12 | `hindley_milner_inference` | recipe · advanced | Hindley-Milner type inference with let-polymorphism, level-based generalisation and source-span errors |
| 12.4.13 | `system_f_bidirectional_typing` | recipe · advanced | System F, higher-rank bidirectional typing (Dunfield-Krishnaswami), record subtyping and variance |
| 12.4.14 | `dependent_types_nbe` | recipe · advanced | a tiny dependent type theory: normalisation by evaluation, bidirectional elaboration with holes, proofs by induction |
| 12.4.15 | `pattern_match_compilation_decision_trees` | recipe · intermediate | compiling pattern matching: decision trees, backtracking automata, sharing, exhaustiveness and redundancy |
| 12.4.16 | `algebraic_effects_handlers` | recipe · advanced | algebraic effects and handlers, three ways: CPS with delimited continuations, a free monad, and a stack machine |
| 12.4.17 | `term_rewriting_unification_logic_programming` | recipe · advanced | unification, term rewriting + Knuth-Bendix completion, a Prolog engine, miniKanren and Datalog |

**Further resources:** 28 article or blog, 3 code, 2 paper, 1 docs load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Kiselyov, “How OCaml type checker works”](https://okmij.org/ftp/ML/generalization.html) (article or blog); [Dunfield and Krishnaswami 2013, “Complete and Easy Bidirectional Typechecking fo](https://arxiv.org/abs/1306.6032) (paper); [Kovács, elaboration-zoo](https://github.com/AndrasKovacs/elaboration-zoo) (code).

### 12.5 Middle end and analysis

| # | Section | Source | About |
|---|---|---|---|
| 12.5.1 | Designing an IR in Rust | compiler guide | |
| 12.5.2 | Dataflow analysis and abstract interpretation | compiler guide | |
| 12.5.3 | Dominators, loops and control flow structure | compiler guide | |
| 12.5.4 | SSA construction and destruction | compiler guide | |
| 12.5.5 | Scalar optimizations | compiler guide | |
| 12.5.6 | E-graphs and equality saturation | compiler guide | |
| 12.5.7 | Loop optimization and vectorization | compiler guide | |
| 12.5.8 | `dataflow_analysis_framework` | recipe · intermediate | a generic monotone dataflow framework (lattice + transfer + worklist) with six classic analyses |
| 12.5.9 | `abstract_interpretation_intervals` | recipe · advanced | abstract interpretation: interval, sign, parity and reduced-product domains, widening/narrowing, difference bounds |
| 12.5.10 | `alias_analysis_points_to` | recipe · advanced | pointer / alias analysis: Andersen (inclusion), Steensgaard (unification), field, flow and context sensitivity |
| 12.5.11 | `egraph_equality_saturation` | recipe · advanced | e-graphs and equality saturation (egg): hash-consed e-nodes, congruence closure with deferred rebuilding, e-matching,... |
| 12.5.12 | `loop_optimizations` | recipe · intermediate | loop optimisations on a CFG IR: natural loops, LICM, strength reduction, unrolling, bounds-check hoisting, plus depen... |
| 12.5.13 | `symbolic_execution_smt_lite` | recipe · advanced | symbolic and concolic execution with a DPLL(T)-lite solver (boolean core + bounded linear-integer theory), state merg... |
| 12.5.14 | `cps_closure_conversion_lambda_lifting` | recipe · advanced | ANF, CPS (Plotkin and one-pass), defunctionalisation, closure conversion, lambda lifting and trampolines |

**Further resources:** 28 article or blog, 3 paper, 2 docs, 2 code load; 1 broken, 0 unconfirmed.
Examples with titles from the guides: [Møller and Schwartzbach, Static Program Analysis](https://cs.au.dk/~amoeller/spa/) (article or blog); [Willsey et al. 2021, “egg: Fast and Extensible Equality Saturation”](https://arxiv.org/abs/2004.03082) (paper); [Cranelift IR reference](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/ir.md) (code).

### 12.6 Back ends

| # | Section | Source | About |
|---|---|---|---|
| 12.6.1 | Backends: HIR, MIR, LLVM and Cranelift | compiler guide | |
| 12.6.2 | Instruction selection, scheduling and emission | compiler guide | |
| 12.6.3 | Register allocation | compiler guide | |
| 12.6.4 | ABIs, object files, linking and unwinding | compiler guide | |
| 12.6.5 | GPU and accelerator compilation | compiler guide | |
| 12.6.6 | Security hardening and sanitizers | compiler guide | |
| 12.6.7 | `ssa_register_allocation` | recipe · advanced | a mini compiler backend: CFG, dominators, SSA, optimisation, liveness, register allocation, out-of-SSA |

**Further resources:** 8 article or blog, 1 paper load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [Agner Fog's optimization manuals and instruction tables](https://www.agner.org/optimize/) (article or blog).

### 12.7 Language tooling and production

| # | Section | Source | About |
|---|---|---|---|
| 12.7.1 | Rust-quality error messages | compiler guide | |
| 12.7.2 | Tooling around a new language | compiler guide | |
| 12.7.3 | Tooling in code | compiler guide | |
| 12.7.4 | Production compiler architecture | compiler guide | |
| 12.7.5 | Reading rustc and contributing to it | compiler guide | |

**Further resources:** 9 article or blog, 1 paper load; 0 broken, 0 unconfirmed.
Examples with titles from the guides: [rustc dev guide: errors and lints](https://rustc-dev-guide.rust-lang.org/diagnostics.html) (article or blog); [“A Prettier Printer”](https://homepages.inf.ed.ac.uk/wadler/papers/prettier/prettier.pdf) (paper).

## 13. Complete projects

Several ideas working together (page kind: build).

### 13.1 Capstones

| # | Section | Source | About |
|---|---|---|---|
| 13.1.1 | `event_sourced_ledger` | recipe · capstone | an event-sourced bank ledger with CQRS projections, snapshots, upcasters and an outbox |
| 13.1.2 | `job_service_axum` | recipe · capstone | an in-process image-resize job service (idempotent API, workers, SSE, graceful shutdown) |
| 13.1.3 | `lease_fencing_sim` | recipe · capstone | deterministic simulation of leases, pauses and fencing tokens |
| 13.1.4 | `mini_async_runtime` | recipe · capstone | a miniature tokio: tasks, work-stealing scheduler, timer driver, sync primitives, mio reactor |
| 13.1.5 | `mini_http_server_thread_pool` | recipe · capstone | a mini multithreaded HTTP/1.1 server from scratch on std::net (thread pool, keep-alive, pipelining, router, static fi... |
| 13.1.6 | `mini_kv_store` | recipe · capstone | an LSM-tree-lite key-value store: WAL + memtable + SSTables with bloom filters + k-way merge compaction |
| 13.1.7 | `mini_redis` | recipe · capstone | a mini Redis: RESP2 over TCP, five data types, expiry, pub/sub, transactions, AOF + snapshots |
| 13.1.8 | `query_optimizer` | recipe · capstone | a cost-based query optimizer in miniature: statistics -> cardinality estimates -> rewrite rules -> physical costing -... |
| 13.1.9 | `rate_limiter_lab` | recipe · capstone | a deterministic laboratory comparing six rate-limit algorithms |
| 13.1.10 | `ray_tracer` | recipe · capstone | a Whitted + path-tracing ray tracer with BVH, glass, soft shadows, depth of field and PNG output |
| 13.1.11 | `regex_nfa_engine` | recipe · capstone | a regex engine: recursive-descent parser -> Thompson NFA -> linear-time simulation, checked against a backtracker |
| 13.1.12 | `search_engine` | recipe · capstone | a mini full-text search engine: positional postings, BM25, boolean/phrase/wildcard queries, segments |
| 13.1.13 | `tiny_sql_engine` | recipe · capstone | a tiny in-memory SQL engine: lex -> Pratt parse -> AST -> bind/plan -> volcano executor |
| 13.1.14 | `toy_filesystem` | recipe · capstone | a toy UNIX filesystem on a block device: inodes, indirect blocks, directories, a write-ahead journal, fsck, and a buf... |
| 13.1.15 | `toy_language_interpreter` | recipe · capstone | a Lox-style scripting language: scanner, Pratt/recursive-descent parser, resolver, tree-walk interpreter AND a byteco... |
| 13.1.16 | `typeahead_service` | recipe · capstone | a type-ahead service (axum): prefix trie + trigram typo index, merged ranking, LRU cache, live updates |

**Further resources:** 24 article or blog, 8 code, 7 docs, 3 paper, 2 book load; 0 broken, 0 unconfirmed.

### 13.2 Graphics

| # | Section | Source | About |
|---|---|---|---|
| 13.2.1 | `fractals_noise_automata` | recipe · intermediate | generative graphics: fractals, noise, L-systems, cellular automata, reaction-diffusion, contours |
| 13.2.2 | `image_processing` | recipe · intermediate | classic image processing: convolution, edges, histograms, dithering, resampling, colour spaces, alpha |
| 13.2.3 | `lines_curves_fills` | recipe · intermediate | 2D drawing primitives: lines, circles, fills, Bezier and Catmull-Rom curves, SDFs, bitmap text |
| 13.2.4 | `png_ppm_writer` | recipe · intermediate | write and read images without crates: PPM (P6) and a minimal PNG encoder + decoder, sRGB, ASCII preview |
| 13.2.5 | `software_rasterizer` | recipe · advanced | a complete software rasterizer: transforms, near-plane clipping, edge functions, z-buffer, shading, textures |
| 13.2.6 | `vec3_matrix_quaternion` | recipe · intermediate | 3D math from scratch: Vec3, Mat4, projections, quaternions, gimbal lock, ray intersections |

**Further resources:** 2 article or blog, 1 code, 1 book load; 0 broken, 0 unconfirmed.

## 14. Worked questions

Interview-style questions with solutions, kept as reference. Open decision: include or leave out.

### 14.1 Rust concept answers

| # | Section | Source | About |
|---|---|---|---|
| 14.1.1 | `rust_async_qa` | recipe · advanced | Async Rust: futures, Pin, Send across await, blocking the runtime, cancellation and select! |
| 14.1.2 | `rust_errors_iterators_closures_qa` | recipe · intermediate | Error handling, iterators and closures: the everyday idioms interviewers probe |
| 14.1.3 | `rust_ownership_borrowing_qa` | recipe · intermediate | Ownership, borrowing, lifetimes and Drop: the questions every Rust interview opens with |
| 14.1.4 | `rust_smart_pointers_send_sync_qa` | recipe · advanced | Smart pointers, interior mutability, reference cycles, Send/Sync and shared-state concurrency |
| 14.1.5 | `rust_traits_generics_dispatch_qa` | recipe · advanced | Traits, generics, dyn dispatch, closures as traits, and the standard conversion/comparison traits |
| 14.1.6 | `rust_unsafe_memory_layout_qa` | recipe · advanced | Unsafe Rust, memory layout, numeric edge cases, Drop/leaks, statics and FFI |

**Further resources:** 9 article or blog, 1 code load; 0 broken, 0 unconfirmed.

### 14.2 Coding and design questions

| # | Section | Source | About |
|---|---|---|---|
| 14.2.1 | `arrays_strings_hashmap_questions` | recipe · intermediate | arrays, strings, hash maps and bit tricks: the most-asked coding questions, answered in Rust |
| 14.2.2 | `concurrency_interview_questions` | recipe · advanced | concurrency interview questions: blocking queue, rate limiter, LRU, RW lock, philosophers, barriers, pools, orderings... |
| 14.2.3 | `graphs_and_union_find_questions` | recipe · advanced | graphs, grids, BFS/DFS, union-find, shortest paths and MST: the most-asked coding questions in Rust |
| 14.2.4 | `intervals_greedy_bit_questions` | recipe · intermediate | interval, greedy and bit-manipulation interview questions in Rust (sort + sweep, exchange arguments, XOR/mask tricks)... |
| 14.2.5 | `stacks_queues_heaps_questions` | recipe · intermediate | stack, queue and heap interview questions in Rust (monotonic stacks, deques, BinaryHeap with Reverse, k-way merge) |
| 14.2.6 | `tries_advanced_questions` | recipe · advanced | advanced trie interview questions in Rust: Word Search II, XOR tries, Aho-Corasick streams, prefix+suffix search, pal... |
| 14.2.7 | `tries_core_questions` | recipe · intermediate | trie interview questions in Rust: three ways to build a trie, wildcards, prefix tricks, autocomplete, radix tries, Un... |
| 14.2.8 | `linked_lists_in_rust` | recipe · intermediate | linked-list interview questions in Rust (Box lists, Rc sharing, Weak back-pointers, arenas, iterative Drop) |
| 14.2.9 | `binary_trees_in_rust` | recipe · intermediate | binary-tree interview questions in Rust (Option<Box<Tree>>, Option<Rc<RefCell<Tree>>>, Weak parents, arenas) |
| 14.2.10 | `implement_std_from_scratch` | recipe · advanced | "implement it yourself" Rust questions: Vec, Box, Rc/Weak, HashMap, String, iterator adapters, Mutex, scope, Cow, err... |
| 14.2.11 | `systems_design_in_code` | recipe · advanced | "design it, then code the core": 16 systems-design questions, each with requirements, numbers, a tested core and fail... |

**Further resources:** 11 article or blog, 6 docs, 2 book load; 0 broken, 0 unconfirmed.

## 15. Appendices

Reading lists, maps and background that are not topic pages.

### 15.1 Courses, reading and glossary

| # | Section | Source | About |
|---|---|---|---|
| 15.1.1 | How to use this guide | db guide | |
| 15.1.2 | Courses and books | db guide | |
| 15.1.3 | Tech talks | db guide | |
| 15.1.4 | Reading database research | db guide | |
| 15.1.5 | Contributing to a database | db guide | |
| 15.1.6 | The job landscape | db guide | |
| 15.1.7 | Interviews and a portfolio | db guide | |
| 15.1.8 | Reading map | db guide | |
| 15.1.9 | Glossary | db guide | |
| 15.1.10 | University courses | compiler guide | |
| 15.1.11 | Reading for Parts I and II | compiler guide | |
| 15.1.12 | What to build next | compiler guide | |
| 15.1.13 | Reading map, from front end to back end | compiler guide | |
| 15.1.14 | How to use Part IV | compiler guide | |
| 15.1.15 | Doing PL research | compiler guide | |
| 15.1.16 | Glossary | compiler guide | |

**Further resources:** 102 video, 62 article or blog, 33 code, 11 paper, 7 book, 2 docs load; 3 broken, 2 unconfirmed.
Examples with titles from the guides: [YouTube (25 lectures)](https://www.youtube.com/playlist?list=PLSE8ODhjZXjYMAgsGH-GtY5rJYZ6zjsd5) (video); [15445.courses.cs.cmu.edu/fall2025](https://15445.courses.cs.cmu.edu/fall2025/) (article or blog); [tamnd/cmu-database-group-videos-index](https://github.com/tamnd/cmu-database-group-videos-index) (code); [Database System Concepts, 7th ed.](https://db-book.com/) (book); [Architecture of a Database System](https://dsf.berkeley.edu/papers/fntdb07-architecture.pdf) (paper).
