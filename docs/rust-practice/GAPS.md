# Cookbook concepts that Rust practice lacks

The goal (owner): runnable practice problems for what the Cookbook covers and Rust practice does not. Spoilers are not a concern (the owner decided this). Made on 2026-11-26 from `content/` as it was that day; the Cookbook and `content/` were being changed by another session.

## How thorough this is

- **Existing problems:** the statements of all 248 Rust problems (every track under `content/tracks` except the three Python practice tracks) were read in full. Their solutions and tests were not read. "Practised" means a statement makes the learner write code using the idea; it does not mean the whole concept is covered.
- **Recipes (481 example files):** judged from each file's header comment (title, uses and design-note headings, cut to about 260 characters). The code itself was opened only for the files behind the first batch (about 30).
- **Guide pages (210):** judged from the title, section headings and the number of Rust blocks. Their text was mostly **not** read, so treat that table as the weakest.
- **Proposals** are not written or tested. Difficulty, kind and crates are estimates. Their source pages are candidates for a Cookbook reference; the level (related, similar, exact) is decided only when the problem exists, after reading the page.
- Ids were checked by script against every current problem id, every `renamed_from` and `content/retired.txt` (325 retired ids): no collision. Crates were checked against `docker/deps/Cargo.toml`.

## Counts

| Recipe files | Count |
|---|---|
| Practised (no further problem proposed) | 70 |
| Gap, runnable now, partly practised | 93 |
| Gap, runnable now, not practised | 212 |
| Needs runner support | 36 |
| Not suited to a graded problem | 70 |
| **Total** | **481** |

The first three rows describe `content/` today. Rows closed by the first batch stay under "gap" until it is merged.

| Other pages | Practised | Runnable gap | Needs runner | Not suited |
|---|---|---|---|---|
| Guide pages (210) | 15 | 110 | 28 | 57 |
| Multi-file crate pages (10) | 0 | 0 | 10 | 0 |
| Draft sample pages (6) | not counted: they restate recipe topics | | | |

**Proposed problems:** 332 runnable now (of which 20 are written in the first batch), plus 3 listed beside a runner limitation. Several close together are variations; expect to drop some when writing them.

### What "needs runner support" means here

| Need | What is missing (from `crates/runner`) | Verified? |
|---|---|---|
| Multi-file projects, features, build scripts, no_std, cdylib | a submission is one `src/lib.rs` with a generated manifest | read in `project.rs` |
| Compile-fail grading (misuse must not compile) | no way to expect a named error | read in `project.rs` |
| Miri, loom, sanitizers, cargo-fuzz | nightly and unvendored crates; real-thread tests cannot reliably fail a wrong memory ordering | from the crate list; Miri not tried |
| Sockets, processes, signals, files | Docker runs `--network none --read-only` with a `/tmp` tmpfs. Loopback and `/tmp` files **may** work: not tried. libc, mio, memmap2, tempfile are not vendored | `exec.rs` read; no Docker run |
| Databases, HTTP clients | sqlx, rusqlite and reqwest are not vendored | crate list |
| Staged projects | `mode = "stage"` has no carry-over of code (docs/RUST_PRACTICE.md 7.3) | from that plan |
| Edition 2024 features | the generated manifest says edition 2021 | read in `project.rs` |

"DSA area" under *not suited* means the topic is an algorithm exercise that the DSA area already owns (LeetCode links and Python practice). It is not graded Rust practice, so the owner may want to overrule this.

## Tracks the proposals use

| Track | Name | State | Proposals | In batch one |
|---|---|---|---|---|
| B1 | HTTP services with axum | planned in CURRICULUM.md, unwritten | 8 | 1 |
| B2 | Tower & middleware | planned in CURRICULUM.md, unwritten | 3 | 1 |
| B4 | Networking & protocols | exists | 9 | 0 |
| B5 | Resilience | planned in CURRICULUM.md, unwritten | 7 | 1 |
| B6 | Observability & production | planned in CURRICULUM.md, unwritten | 4 | 0 |
| B7 | Web security | **new** | 12 | 0 |
| C1 | Threads & shared state | exists | 7 | 1 |
| C2 | Message passing | exists | 4 | 0 |
| C3 | Atomics & lock-free | exists | 3 | 0 |
| C4 | Async & Tokio | planned in CURRICULUM.md, unwritten | 10 | 3 |
| C5 | Async internals | exists | 3 | 0 |
| C6 | Data parallelism | planned in CURRICULUM.md, unwritten | 6 | 0 |
| F1 | Measure & read the machine | planned in CURRICULUM.md, unwritten | 4 | 0 |
| F2 | Data layout | exists | 2 | 0 |
| F3 | Memory & allocation | exists | 4 | 0 |
| F4 | Hashing & purpose-built structures | exists | 14 | 1 |
| F5 | CPU-level tricks | exists | 9 | 0 |
| F6 | Concurrency performance | planned in CURRICULUM.md, unwritten | 3 | 0 |
| F7 | I/O & serialization | exists | 14 | 1 |
| L6 | Closures & functional Rust | planned in CURRICULUM.md, unwritten | 2 | 0 |
| L7 | Enums & pattern matching | planned in CURRICULUM.md, unwritten | 3 | 0 |
| L8 | Error design | planned in CURRICULUM.md, unwritten | 5 | 2 |
| L9 | Modules, crates & Cargo | planned in CURRICULUM.md, unwritten | 2 | 0 |
| L10 | Macros | planned in CURRICULUM.md, unwritten | 2 | 1 |
| M1 | Idioms & patterns | planned in CURRICULUM.md, unwritten | 7 | 0 |
| S2 | Strings & text | exists | 1 | 0 |
| S3 | Vec & slices | exists | 2 | 0 |
| S5 | Queues & heaps | planned in CURRICULUM.md, unwritten | 6 | 0 |
| S6 | Iterators | planned in CURRICULUM.md, unwritten | 9 | 1 |
| S7 | Smart pointers & interior mutability | planned in CURRICULUM.md, unwritten | 4 | 1 |
| S8 | The core traits | planned in CURRICULUM.md, unwritten | 11 | 0 |
| S9 | I/O & filesystem | planned in CURRICULUM.md, unwritten | 7 | 1 |
| S10 | Time, env & processes | planned in CURRICULUM.md, unwritten | 6 | 0 |
| S11 | mem, ptr & alloc | planned in CURRICULUM.md, unwritten | 2 | 0 |
| Y2 | Unsafe Rust | exists | 3 | 0 |
| Y5 | Testing & verification | planned in CURRICULUM.md, unwritten | 11 | 2 |
| Y6 | WebAssembly from scratch | exists | 1 | 0 |
| Y10 | Search engine core | exists | 3 | 0 |
| Y12 | Database engine parts | **new** | 45 | 1 |
| Y13 | Distributed systems parts | **new** | 17 | 1 |
| Y14 | Parsers & languages | **new** | 19 | 1 |
| Y15 | Compiler analyses and back ends | **new** | 12 | 0 |
| Y16 | Vectors, search and OS models | **new** | 17 | 0 |
| Y17 | Analytics and streaming | **new** | 9 | 0 |

`Y11` is left free: docs/RUST_PRACTICE.md proposes it for a build-a-project path. `M1` is in section M, which the Rust tab does not list today.

## Chapter by chapter

Columns: the example file (and its Cookbook page id), status, current problems that practise it, and proposed problems (defined in the last section). **W** marks a problem written in the first batch.

### Rust language / Ownership and borrowing

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `lifetimes` · `lifetimes` | practised | `l3-longest`, `l3-cursor-over-str`, `l3-zero-copy-tokenizer`, `l3-why-a-everywhere-fails`, `l3-fix-return-ref-to-local` |  |
| `borrowck_patterns` · `borrowck-patterns` | practised | `l2-fix-push-while-holding-a-reference`, `l2-fix-remove-while-iterating`, `l2-split-first-mut`, `l2-fix-get-or-insert`, `l1-fix-move-out-of-index` |  |
| `cow` · `cow` | practised | `l1-cow-normalizer`, `s2-cow-str` |  |
| `ref_patterns` · `ref-patterns` | gap (partly practised) | `l1-fix-partial-moves`, `s1-fix-move-out-of-option`, `s1-as-ref-as-mut-as-deref`, `l2-destructure-self` | `l7-fix-binding-modes`. @ bindings and guards have no problem |
| `drop_raii` · `drop-raii` | practised | `l1-predict-drop-order`, `l1-raii-span-guard`, `l1-rollback-guard` |  |
| `smart_pointers` · `smart-pointers` | gap (partly practised) | `l2-cell-for-copy`, `l2-fix-borrowmuterror`, `l2-fix-refcell-guard-across-call`, `l1-box-linked-stack` | `s7-fix-observer-leak` **W**. && Weak had no problem before batch one |
| `mem_functions` · `mem-functions` | practised | `l1-mem-replace-state`, `l1-option-take-batcher`, `l2-fix-swap-without-swap`, `l2-destructure-self` |  |
| `static_const_lazy` · `static-const-lazy` | gap (partly practised) | `l3-static-bound-vs-static-ref`, `l5-const-fn-crc-table` | `s8-lazy-registry` |

### Rust language / Types, traits and generics

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `generics_bounds` · `generics-bounds` | practised | `l5-generic-max-by-key`, `l5-report-any-collection`, `l5-fix-vec2-operators`, `l5-associated-type-vs-parameter`, `l4-blanket-impls` |  |
| `generics_lifetimes_structs` · `generics-lifetimes-structs` | practised | `l3-trait-lifetimes-parse`, `l3-hrtb-borrowed-iteration`, `l3-self-referential-parser` |  |
| `hrtb_gats` · `hrtb-gats` | practised | `l4-lending-iterator`, `l4-fix-hrtb-decode-owned`, `l3-hrtb-borrowed-iteration` |  |
| `dyn_vs_generic` · `dyn-vs-generic` | gap (partly practised) | `l4-shapes-both-ways`, `l4-closures-generic-vs-dyn`, `l5-inner-fn-monomorphization` | `y5-fake-the-clock`. fakes for tests not practised |
| `common_traits` · `common-traits` | gap (partly practised) | `l1-fix-small-struct-copy`, `s4-fix-hash-eq-consistency`, `l4-impl-display` | `s8-ord-eq-hash-key` |
| `conversions` · `conversions` | gap (partly practised) | `s1-fix-question-mark-from`, `s2-fromstr-setting`, `l1-str-parameters`, `l5-invalid-states-unrepresentable` | `s8-checked-conversions` |
| `implementing_std_traits` · `implementing-std-traits` | practised | `l5-stack-plugs-into-std`, `y2-array-vec`, `y2-unsafe-linked-list`, `l4-matrix-operators` |  |
| `extension_traits_ops` · `extension-traits-ops` | practised | `l4-iterator-extension`, `l4-vec2-operators`, `l4-matrix-operators` |  |
| `phantomdata_typed` · `phantomdata-typed` | practised | `l5-phantom-units-of-measure`, `l5-newtype-typestate`, `l5-typestate-http-builder` |  |
| `newtype_ids` · `newtype-ids` | practised | `l5-newtype-typestate`, `l4-fix-orphan-rule`, `f2-four-byte-optional-index` |  |
| `const_and_generics_advanced` · `const-and-generics-advanced` | practised | `l5-fix-const-generic-traps`, `l5-const-fn-crc-table`, `l5-shape-typed-matrix`, `l5-zero-sized-strategies`, `f2-tcp-flags-in-a-byte` |  |
| `library_api_polish` · `library-api-polish` | gap (partly practised) | `l5-stack-plugs-into-std`, `l1-str-parameters` | `l9-semver-safe-api`. compile-fail proof of sealing needs compile-fail grading |

### Rust language / Everyday language

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `closures_hof` · `closures-hof` | gap (partly practised) | `l1-fix-closure-may-outlive`, `l2-fix-closure-borrows`, `l3-fix-closure-returns-its-argument`, `l4-closures-generic-vs-dyn` | `l6-memoize-any-fn`, `l6-fix-fnonce-twice` |
| `iterators` · `iterators` | gap (partly practised) | `s1-collect-into-result`, `s3-chunks-and-windows` | `s6-pipeline-no-collect`. uses count_allocs |
| `iterator_adapters` · `iterator-adapters` | gap (partly practised) | `l4-iterator-extension`, `s2-zero-copy-split-iterator`, `s3-pairs-iterator`, `l2-stride-iter-mut` | `s6-merge-sorted-streams` **W**, `s6-flatten-adapter` |
| `pattern_matching` · `pattern-matching` | gap (partly practised) | `y9-shell-line-parser`, `l1-fix-partial-moves` | `l7-slice-pattern-commands`, `l7-fix-wildcard-arm` |
| `strings` · `strings` | practised | `s2-string-vs-str`, `s2-bytes-chars-char-indices`, `s2-unicode-safe-truncate`, `s2-fix-slicing-mid-utf8` |  |
| `bytes` · `bytes` | practised | `f5-varint-codec`, `y3-repr-c-header`, `l3-binary-frame-parser`, `f7-posting-list-codec` |  |
| `collections_tour` · `collections-tour` | practised | `s4-most-common-word`, `s4-group-by-length`, `s4-btreemap-ranges`, `s3-vec-ops`, `s3-ring-buffer` |  |
| `io_files_paths` · `io-files-paths` | gap (partly practised) | `s2-word-frequency`, `l5-inner-fn-monomorphization` | `s9-atomic-save`. needs a writable temp directory (sandbox has a /tmp tmpfs; not tried). batch one also covers it: `s9-bounded-line-reader` |
| `macro_rules` · `macro-rules` | gap (partly practised) | `s1-my-result-and-try` | `l10-fix-macro-hygiene` **W**, `l10-tt-muncher` |
| `modules_visibility` · `modules-visibility` | gap (partly practised) | `l5-invalid-states-unrepresentable` | `l9-private-invariants`. splitting into files needs multi-file grading |
| `edition_2024_changes` · `edition-2024-changes` | not suited |  | the runner builds edition 2021 (project.rs manifest); needs a per-problem edition setting |
| `recent_stable_language_features` · `recent-stable-language-features` | not suited |  | a tour of syntax; features appear inside other problems |
| `recent_std_apis` · `recent-std-apis` | not suited |  | a catalogue of new APIs, not one skill |

### Rust language / Errors and diagnostics

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `anyhow_context` · `anyhow-context` | **gap** |  | `l8-fix-lost-context` **W** |
| `diagnostics_spans` · `diagnostics-spans` | **gap** |  | `l8-caret-diagnostics` |
| `error_chain_retryable` · `error-chain-retryable` | gap (partly practised) | `l3-box-dyn-error-downcast` | `l8-import-errors` **W** |
| `error_enum` · `error-enum` | gap (partly practised) | `s1-fix-question-mark-from`, `l5-fix-generic-parse-list` | `l8-fix-stringly-errors`. batch one also covers it: `l8-import-errors` |
| `option_result_toolbox` · `option-result-toolbox` | practised | `s1-config-port`, `s1-order-total`, `s1-transpose`, `s1-result-of-option`, `s1-collect-into-result`, `s1-my-option`, `s1-my-option-borrowing`, `s1-sentinel-to-option` |  |

### Rust language / Design patterns

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `design_patterns_high` · `design-patterns-high` | gap (partly practised) | `l5-typestate-http-builder`, `l4-plugin-logger` | `m1-command-with-undo`, `m1-injected-strategy`. batch one also covers it: `s7-fix-observer-leak` |
| `design_patterns_low` · `design-patterns-low` | gap (partly practised) | `f3-string-interner`, `f3-typed-arena`, `f3-generational-slab`, `f2-particles-as-arrays`, `f2-inline-small-strings` | `f3-object-pool` |
| `typestate_builder` · `typestate-builder` | practised | `l5-typestate-http-builder` |  |
| `typestate_protocol` · `typestate-protocol` | gap (partly practised) | `l5-newtype-typestate` | `m1-typestate-protocol`. proving misuse fails needs compile-fail grading |
| `state_machine` · `state-machine` | gap (partly practised) | `l1-mem-replace-state`, `l2-destructure-self` | `m1-order-state-machine` |
| `generic_state_machine` · `generic-state-machine` | **gap** |  | `m1-replay-events` |
| `type_erasure_vtable` · `type-erasure-vtable` | gap (partly practised) | `l4-any-downcasting`, `l4-where-self-sized` | `y2-hand-written-vtable`. soundness is only partly observable without Miri |
| `any_event_bus` · `any-event-bus` | gap (partly practised) | `l3-static-bound-vs-static-ref` | `m1-typed-event-bus` |
| `tower_service_layer` · `tower-service-layer` | **gap** |  | `b2-fix-poll-ready` **W**, `b2-timeout-retry-layer` |
| `sans_io_protocol` · `sans-io-protocol` | practised | `b4-length-prefixed-codec`, `b4-http1-request-parser`, `y7-raft-append-entries` |  |
| `undo_redo_editors` · `undo-redo-editors` | **gap** |  | `m1-piece-table-undo` |

### Rust language / Memory and allocators

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `arena_ids` · `arena-ids` | practised | `f2-eight-byte-instructions`, `f2-four-byte-optional-index`, `s1-fix-option-niche` |  |
| `bump_arena_allocator` · `bump-arena-allocator` | gap (partly practised) | `f3-typed-arena` | `f3-bump-allocator` |
| `garbage_collectors` · `garbage-collectors` | **gap** |  | `f3-mark-sweep-heap` |
| `size_class_allocator` · `size-class-allocator` | gap (partly practised) | `f3-buddy-allocator` | `f3-free-list-allocator` |
| `slab_generational` · `slab-generational` | practised | `f3-generational-slab` |  |

### Rust language / Unsafe and FFI

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `unsafe_basics` · `unsafe-basics` | gap (partly practised) | `y2-array-vec`, `y2-mini-vec` | `y2-review-panic-safety` |
| `unsafecell_cells` · `unsafecell-cells` | gap (partly practised) | `c3-once-cell`, `c3-spinlock`, `l2-cell-for-copy` | `y2-my-refcell`. Send/Sync bounds need a runtime trait probe |
| `maybeuninit_fixedvec` · `maybeuninit-fixedvec` | practised | `y2-array-vec` |  |
| `nonnull_linked_list` · `nonnull-linked-list` | practised | `y2-unsafe-linked-list` |  |
| `pin_projection` · `pin-projection` | gap (partly practised) | `c5-block-on`, `c5-mini-executor` | `c5-hand-written-timeout` |
| `repr_layout_ffi` · `repr-layout-ffi` | practised | `y3-repr-c-header`, `f2-repack-a-submission-entry`, `f2-fix-packed-zip-header` |  |
| `ffi_c_interop` · `ffi-c-interop` | practised | `y3-cstring-boundary`, `y3-qsort-callback`, `y3-opaque-handle-api` |  |

### Standard library and core / Option, Result, conversions, ownership types

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_option_methods` · `std-option-methods` | practised | `s1-my-option`, `s1-my-option-borrowing`, `s1-question-mark-on-option` |  |
| `std_result_methods` · `std-result-methods` | practised | `s1-my-result-and-try`, `s1-collect-into-result`, `s1-result-of-option` |  |
| `std_convert_from_into_asref` · `std-convert-from-into-asref` | practised | `s1-option-ref-to-str`, `l1-str-parameters`, `s2-fromstr-setting` | covered by the conversion proposal above |
| `std_cow_borrow_toowned` · `std-cow-borrow-toowned` | practised | `l1-cow-normalizer`, `s2-cow-str`, `s4-fix-borrowed-lookup` |  |
| `std_smart_pointers_use_cases` · `std-smart-pointers-use-cases` | gap (partly practised) | `l4-blanket-impls`, `c3-mini-arc` | `s7-pick-the-pointer`, `s7-copy-on-write-config`. batch one also covers it: `s7-fix-observer-leak` |

### Standard library and core / Text

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_char_ascii_unicode` · `std-char-ascii-unicode` | practised | `s2-compare-ignoring-case`, `s2-fix-len-counts-bytes`, `s2-reverse-each-word` |  |
| `std_str_search_split_parse` · `std-str-search-split-parse` | practised | `s2-split-trim-parse`, `s2-parse-url-borrowed`, `s2-zero-copy-split-iterator` |  |
| `std_string_building_utf8` · `std-string-building-utf8` | practised | `s2-join-without-join`, `s2-template-formatter`, `f7-itoa-stack-buffer` |  |

### Standard library and core / Iterators and collections

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_iterator_adapters_lazy` · `std-iterator-adapters-lazy` | gap (partly practised) | `l4-iterator-extension` | `s6-fix-side-effects-in-map`. batch one also covers it: `s6-merge-sorted-streams` |
| `std_iterator_consumers` · `std-iterator-consumers` | gap (partly practised) | `s1-collect-into-result` | `s6-single-pass-stats` |
| `std_iterator_sources_custom` · `std-iterator-sources-custom` | gap (partly practised) | `l5-stack-plugs-into-std`, `s3-pairs-iterator` | `s6-paginated-source`, `s6-tree-iter-no-alloc`. batch one also covers it: `s6-merge-sorted-streams` |
| `std_hashmap_entry_api_patterns` · `std-hashmap-entry-api-patterns` | practised | `l2-entry-returns-a-borrow`, `s4-index-by-first-letter`, `s4-fix-borrowed-lookup` |  |

### Standard library and core / Vectors, slices and collections

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_vec_vecdeque_methods` · `std-vec-vecdeque-methods` | gap (partly practised) | `s3-vec-ops`, `s3-drain-splice-split-off`, `s3-retain-and-dedup` | `s5-sliding-window-max` |
| `std_slice_search_sort_binary` · `std-slice-search-sort-binary` | gap (partly practised) | `s3-keep-sorted`, `s3-sort-by-key`, `s4-floor-and-ceiling` | `s3-select-nth-percentiles`. comparison counters, no timing |
| `std_slice_chunks_windows_split` · `std-slice-chunks-windows-split` | practised | `s3-chunks-and-windows`, `s3-split-at-mut`, `l2-pair-mut` |  |
| `std_collections_choice_guide` · `std-collections-choice-guide` | gap (partly practised) | `s4-btreemap-ranges`, `s4-interval-map` | `s5-pick-the-collection` |
| `std_array_tuple_unit_never` · `std-array-tuple-unit-never` | not suited | `l5-shape-typed-matrix` | a language tour; no focused skill beyond what const generic problems cover |

### Standard library and core / Numbers, traits and formatting

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_integer_methods_overflow` · `std-integer-methods-overflow` | gap (partly practised) | `s1-order-total`, `c3-atomic-stats` | `s8-fix-silent-overflow` |
| `std_float_methods_total_order` · `std-float-methods-total-order` | **gap** |  | `s8-fix-sort-floats`, `s8-stable-float-sum` |
| `std_num_parsing_formatting` · `std-num-parsing-formatting` | gap (partly practised) | `s2-format-width-precision`, `s2-split-trim-parse`, `f7-itoa-stack-buffer` | `s8-human-sizes` |
| `std_cmp_hash_default_traits` · `std-cmp-hash-default-traits` | gap (partly practised) | `s3-sort-by-key`, `s4-fix-hash-eq-consistency`, `s4-deterministic-report` | `s5-heap-with-reverse` |
| `std_ops_traits_overloading` · `std-ops-traits-overloading` | practised | `l4-vec2-operators`, `l4-fix-add-for-references`, `l4-matrix-operators`, `l5-fix-vec2-operators` |  |
| `std_any_error_marker_traits` · `std-any-error-marker-traits` | practised | `l4-any-downcasting`, `l3-box-dyn-error-downcast`, `l4-fix-dyn-send` |  |
| `std_fmt_display_debug_deep` · `std-fmt-display-debug-deep` | practised | `l4-impl-display`, `s2-display-vs-debug` |  |

### Standard library and core / Memory and low level

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_alloc_layout_global_allocator` · `std-alloc-layout-global-allocator` | gap (partly practised) | `s3-minivec-core`, `y2-mini-vec` | `s11-counting-allocator`. must not turn on count_allocs, which installs its own |
| `std_mem_replace_take_swap_forget` · `std-mem-replace-take-swap-forget` | practised | `l1-mem-replace-state`, `l1-option-take-batcher`, `l2-fix-swap-without-swap` |  |
| `std_ptr_nonnull_slices_raw` · `std-ptr-nonnull-slices-raw` | gap (partly practised) | `y2-unsafe-linked-list`, `y2-mini-vec` | `s11-split-two-mut-raw` |
| `std_btree_binaryheap_ordered` · `std-btree-binaryheap-ordered` | gap (partly practised) | `s4-btreemap-ranges`, `s4-interval-map` | `s5-lazy-deletion-heap` |
| `std_hash_collections_io_serialization_formats` · `std-hash-collections-io-serialization-formats` | gap (partly practised) | `y9-kv-store-wal`, `l3-binary-frame-parser`, `f5-varint-codec` | `f7-versioned-record-codec` |
| `std_vecdeque_linkedlist_ring_buffers` · `std-vecdeque-linkedlist-ring-buffers` | practised | `s3-ring-buffer` |  |

### Standard library and core / Threads, synchronisation and time

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_atomics_ordering_use_cases` · `std-atomics-ordering-use-cases` | practised | `c3-atomic-stats`, `c3-once-cell`, `c3-spinlock`, `c3-mini-arc`, `c3-spsc-ring` |  |
| `std_sync_primitives_use_cases` · `std-sync-primitives-use-cases` | gap (partly practised) | `c1-blocking-queue`, `c1-fair-semaphore`, `c1-rwlock-writer-preferring`, `c2-thread-pool` | `c1-once-per-key`, `c1-fix-poisoned-registry` |
| `std_thread_scope_local_park` · `std-thread-scope-local-park` | gap (partly practised) | `l1-move-into-threads`, `l3-fix-thread-spawn-static` | `c1-park-unpark-parker` |
| `std_time_instant_duration_systemtime` · `std-time-instant-duration-systemtime` | gap (partly practised) | `c1-blocking-queue` | `s10-injectable-clock-backoff` |

### Standard library and core / Processes, signals and networking

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_env_args_vars` · `std-env-args-vars` | **gap** |  | `s10-layered-config`. inject args and env as values |
| `std_panic_process_exit_catch` · `std-panic-process-exit-catch` | gap (partly practised) | `c2-thread-pool`, `y3-qsort-callback`, `l1-rollback-guard` | `s10-catch-unwind-boundary` |
| `std_process_command_pipelines` · `std-process-command-pipelines` | needs runner | `y9-shell-line-parser` | needs: child processes. the sandbox allows processes up to the pid limit, but the image's available programs were not checked |
| `std_signals_env_exit_hooks_ctrlc` · `std-signals-env-exit-hooks-ctrlc` | needs runner |  | needs: signals and libc. libc is not vendored |
| `std_net_tcp_udp_ip_addr` · `std-net-tcp-udp-ip-addr` | needs runner |  | needs: loopback sockets. Docker runs with --network none; whether loopback tests work was not tried |
| `std_io_stdio_terminal_text_processing` · `std-io-stdio-terminal-text-processing` | **gap** |  | `s9-filter-over-write` |
| `std_fs_watch_polling_tempdir_testing` · `std-fs-watch-polling-tempdir-testing` | needs runner |  | needs: filesystem in tests (probably works in /tmp). try one temp-dir problem in Docker first |

### Standard library and core / I/O, files and paths

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `std_bufreader_bufwriter_patterns` · `std-bufreader-bufwriter-patterns` | **gap** |  | `s9-fix-unflushed-writer`. batch one also covers it: `s9-bounded-line-reader` |
| `std_io_read_write_seek_traits` · `std-io-read-write-seek-traits` | gap (partly practised) | `l5-inner-fn-monomorphization` | `s9-bounded-line-reader` **W**, `s9-implement-read` |
| `std_fs_file_operations_usecases` · `std-fs-file-operations-usecases` | needs runner |  | needs: filesystem in tests (probably works in /tmp) |
| `std_path_pathbuf_ffi_strings` · `std-path-pathbuf-ffi-strings` | gap (partly practised) | `y3-cstring-boundary` | `s9-fix-path-traversal` |

### Crates and tooling / Data, text and encoding crates

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `serde_attributes_enum_representations` · `serde-attributes-enum-representations` | **gap** |  | `f7-wire-format-json` **W** |
| `serde_formats_json_toml_yaml_bincode` · `serde-formats-json-toml-yaml-bincode` | **gap** |  | `f7-toml-json-one-model`. bincode and others are not vendored |
| `serde_custom` · `serde-custom` | **gap** |  | `f7-custom-visitor` |
| `serde_defaults` · `serde-defaults` | **gap** |  | `f7-strict-config`. batch one also covers it: `f7-wire-format-json` |
| `bytes_buf_buf_mut_framing` · `bytes-buf-buf-mut-framing` | gap (partly practised) | `b4-length-prefixed-codec` | `f7-zero-copy-frames-bytes` |
| `regex_text_processing` · `regex-text-processing` | gap (partly practised) | `y8-regex-nfa-matcher` | `s2-regex-log-parser` |
| `url_base64_hex_percent_encoding` · `url-base64-hex-percent-encoding` | **gap** |  | `f7-percent-and-base64`. url is not vendored |
| `uuid_rand_ids_tokens` · `uuid-rand-ids-tokens` | **gap** |  | `s8-seeded-sampling` |
| `indexmap_smallvec_arrayvec_collections` · `indexmap-smallvec-arrayvec-collections` | gap (partly practised) | `f2-inline-edge-lists` | `f4-ordered-index-map`. indexmap and arrayvec are not vendored |
| `itertools_adaptors_cookbook` · `itertools-adaptors-cookbook` | **gap** |  | `s6-itertools-grouping` |
| `chrono_dates_times_zones` · `chrono-dates-times-zones` | **gap** |  | `s10-recurring-schedule`. time zone database (chrono-tz) is not vendored |
| `time_crate_dates_formatting` · `time-crate-dates-formatting` | not suited |  | the time crate is not vendored; chrono covers the skill |
| `tempfile_walkdir_glob_ignore` · `tempfile-walkdir-glob-ignore` | needs runner |  | needs: filesystem in tests (probably works in /tmp). walkdir is vendored; tempfile is not |

### Crates and tooling / Application crates

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `anyhow_thiserror_eyre_error_design` · `anyhow-thiserror-eyre-error-design` | **gap** |  | eyre and miette are not vendored. batch one also covers it: `l8-import-errors`, `l8-fix-lost-context` |
| `clap_derive_cli_tool` · `clap-derive-cli-tool` | **gap** |  | `s10-clap-subcommands` |
| `clap_cli` · `clap-cli` | **gap** |  | same skill as the clap proposal above |
| `tracing_spans_subscriber_layers` · `tracing-spans-subscriber-layers` | **gap** |  | `b6-capture-spans` |
| `tracing_logs` · `tracing-logs` | **gap** |  | `b6-fix-span-across-await` |
| `tokio_runtime_tasks_channels_timers` · `tokio-runtime-tasks-channels-timers` | **gap** |  | `c4-fix-stalled-runtime`, `c4-fix-non-send-future`. batch one also covers it: `c4-fix-cancel-safe-reader`, `c4-bounded-fan-out`, `c4-drain-with-deadline` |
| `reqwest_axum_http_roundtrip` · `reqwest-axum-http-roundtrip` | needs runner |  | needs: reqwest not vendored; real sockets. the server half is practised with oneshot. batch one also covers it: `b1-api-error-response` |
| `sqlx_sqlite_queries_transactions` · `sqlx-sqlite-queries-transactions` | needs runner |  | needs: sqlx and a database not vendored |

### Crates and tooling / Multi-file projects

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| crate `cb-macros` | needs runner |  | needs: multi-file grading and a proc-macro crate (syn, quote not vendored) |
| crate `cb-macros-demo` | needs runner |  | needs: multi-file grading |
| crate `cb-build-script` | needs runner |  | needs: build scripts and multi-file grading |
| crate `cb-no-std` | needs runner |  | needs: crate-level attributes and a target without std |
| crate `cb-features-workspace` | needs runner |  | needs: features and workspaces need multi-file grading |
| crate `cb-cdylib-c-abi` | needs runner |  | needs: cdylib crate type and a C caller |

### Testing and benchmarking / Methods: test design and TDD

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `testing_patterns` · `testing-patterns` | **gap** |  | `y5-catch-the-broken-queue` **W**, `y5-find-the-missed-bug` |

### Testing and benchmarking / Methods: property-based testing

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `shrinking` · `shrinking` | **gap** |  | `y5-shrink-failing-input` **W** |
| `criterion_proptest_insta_testing` · `criterion-proptest-insta-testing` | **gap** |  | `y5-codec-properties`. insta, criterion and assert_cmd are not vendored |

### Testing and benchmarking / Methods: fuzzing

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| (whole chapter) | needs runner |  | needs: cargo-fuzz needs nightly and a fuzzing run. `y5-fuzz-by-hand` |

### Testing and benchmarking / Methods: concurrency testing

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `model_checker_for_concurrent_code` · `model-checker-for-concurrent-code` | **gap** |  | `y5-interleaving-explorer`. loom is not vendored |

### Testing and benchmarking / Methods: memory-ordering bugs

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `memory_ordering_litmus` · `memory-ordering-litmus` | needs runner | `c3-spinlock`, `c3-mini-arc`, `c3-spsc-ring` | needs: Miri or loom to grade orderings. real-thread tests cannot reliably fail a wrong Ordering |

### Testing and benchmarking / Methods: benchmarking and profiling

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `bench_harness` · `bench-harness` | gap (partly practised) | `f2-particles-as-arrays`, `f2-split-hot-from-cold` | `f1-benchmark-that-measures-nothing`. timing-based grading can flake; grade the harness's own statistics |
| `metrics_prometheus_histograms_load_testing` · `metrics-prometheus-histograms-load-testing` | gap (partly practised) | `c3-atomic-stats` | `b6-latency-histogram` |

### Testing and benchmarking / Systems: distributed systems

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `deterministic_simulation_testing` · `deterministic-simulation-testing` | **gap** |  | `y5-simulate-the-network`. turmoil is vendored |
| `linearizability_checker` · `linearizability-checker` | **gap** |  | `y5-linearizability-check` |

### Testing and benchmarking / Systems: databases

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `consistent_snapshots_sql_logic_tests` · `consistent-snapshots-sql-logic-tests` | **gap** |  | `y5-differential-query-test` |

### Testing and benchmarking / Systems: compilers and languages

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| (whole chapter) | **gap** |  | `y5-reduce-a-crashing-program`. close to y5-shrink-failing-input |

### Testing and benchmarking / Multi-file projects

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| crate `cb-testing-tools` | needs runner |  | needs: multi-file grading |

### Data structures and algorithms / Hashing and probabilistic structures

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `hash_functions` · `hash-functions` | **gap** |  | `f4-identity-hasher` |
| `open_addressing_map` · `open-addressing-map` | gap (partly practised) | `s4-open-addressing-map`, `f4-open-addressing-map` | `f4-robin-hood-map` |
| `cuckoo_hashing` · `cuckoo-hashing` | **gap** |  | `f4-cuckoo-filter` |
| `count_min_sketch` · `count-min-sketch-code` | practised | `f4-count-min-sketch` |  |
| `hyperloglog` · `hyperloglog` | **gap** |  | `f4-hyperloglog` |
| `minhash_simhash_lsh` · `minhash-simhash-lsh` | **gap** |  | `f4-minhash-near-duplicates` |
| `sampling` · `sampling` | **gap** |  | `f4-reservoir-sample`. statistical check with a seeded generator |
| `bloom_filter` · `bloom-filter` | practised | `f4-bloom-filter` |  |

### Data structures and algorithms / Trees and ordered structures

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `bplus_tree` · `bplus-tree` | gap (partly practised) | `f2-slotted-page` | `y12-bplus-tree-insert`. also covered by the BusTub course |
| `b_epsilon_tree` · `b-epsilon-tree` | not suited |  | too large for one focused exercise |
| `btree_bulk_load_prefix_compression` · `btree-bulk-load-prefix-compression` | **gap** |  | `y12-bulk-load-and-separators` |
| `bw_tree_latch_free` · `bw-tree-latch-free` | not suited |  | latch-free correctness cannot be graded by unit tests |
| `optimistic_lock_coupling_btree` · `optimistic-lock-coupling-btree` | needs runner |  | needs: Miri or loom to grade the concurrency |
| `adaptive_radix_tree` · `adaptive-radix-tree` | **gap** |  | `f4-adaptive-radix-nodes` |
| `trie_radix` · `trie-radix` | **gap** |  | `f4-compressed-radix-tree` |
| `skiplist` · `skiplist` | **gap** |  | `f4-skiplist-memtable` |
| `treap_interval` · `treap-interval` | gap (partly practised) | `s4-interval-map` | `f4-order-statistics-treap` |
| `segment_fenwick` · `segment-fenwick` | not suited |  | DSA area (LeetCode links and Python practice); not a Rust skill gap |
| `gist_generalized_search_tree` · `gist-generalized-search-tree` | **gap** |  | `y12-gist-trait` |
| `learned_index_rmi` · `learned-index-rmi` | **gap** |  | `y12-learned-index` |

### Data structures and algorithms / Caches, queues, heaps and bit sets

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `cache_policy_zoo` · `cache-policy-zoo` | **gap** |  | `f4-clock-eviction`, `f4-lru-k`. batch one also covers it: `f4-lru-cache-arena` |
| `lru_cache` · `lru-cache` | **gap** |  | `f4-lru-cache-arena` **W** |
| `ring_buffer_zoo` · `ring-buffer-zoo` | practised | `s3-ring-buffer`, `f2-repack-a-submission-entry`, `c3-spsc-ring` |  |
| `priority_queue_zoo` · `priority-queue-zoo` | **gap** |  | `s5-indexed-heap` |
| `heap_patterns` · `heap-patterns` | **gap** |  | `s5-binary-heap-from-scratch` |
| `heap_tiebreak` · `heap-tiebreak` | **gap** |  | same skill as s5-heap-with-reverse above |
| `bitset_rank_select_roaring` · `bitset-rank-select-roaring` | gap (partly practised) | `f5-bitset-iter`, `f2-validity-bitmap-column` | `f5-roaring-containers` |
| `bdd_zdd` · `bdd-zdd` | not suited |  | niche; no Rust skill beyond hash-consing, which the compiler proposals cover |

### Data structures and algorithms / Graph algorithms

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `csr_graph` · `csr-graph` | **gap** |  | `f2-csr-graph`. layout and allocation budget |
| `index_graph` · `index-graph` | practised | `l5-associated-type-vs-parameter`, `f2-inline-edge-lists` |  |
| `dijkstra` · `dijkstra` | not suited |  | DSA area; the heap skill is in the S5 proposals |
| `shortest_path_zoo` · `shortest-path-zoo` | not suited |  | DSA area |
| `mst_bridges` · `mst-bridges` | not suited |  | DSA area |
| `bipartite_matching` · `bipartite-matching` | not suited |  | DSA area |
| `max_flow_dinic` · `max-flow-dinic` | not suited |  | DSA area |
| `union_find` · `union-find` | not suited |  | DSA area |
| `tarjan_scc` · `tarjan-scc` | not suited |  | DSA area |

### Data structures and algorithms / Dynamic programming, greedy and search

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `dp_and_backtracking_questions` · `dp-and-backtracking-questions` | not suited |  | DSA area |
| `dp_count_paths` · `dp-count-paths` | not suited |  | DSA area |
| `dp_edit_distance` · `dp-edit-distance` | **gap** |  | `l8-did-you-mean`. only for the diagnostic use; the DP itself is DSA |
| `dp_knapsack` · `dp-knapsack` | not suited |  | DSA area |
| `dp_lcs_diff` · `dp-lcs-diff` | practised | `y8-shortest-edit-script` |  |
| `dp_memoization` · `dp-memoization` | not suited |  | DSA area; memoisation in Rust is in l6-memoize-any-fn |
| `dp_zoo` · `dp-zoo` | not suited |  | DSA area |
| `greedy_and_interval_algorithms` · `greedy-and-interval-algorithms` | not suited |  | DSA area |
| `backtracking_branch_bound` · `backtracking-branch-bound` | not suited |  | DSA area |
| `heuristics_zoo` · `heuristics-zoo` | not suited |  | open-ended quality, no exact pass or fail |
| `problem_modeling_reductions` · `problem-modeling-reductions` | not suited |  | a modelling skill with many valid answers |
| `combinatorial_generation` · `combinatorial-generation` | **gap** |  | `s6-lazy-permutations` |
| `combo_algorithms` · `combo-algorithms` | not suited |  | several structures at once; too large for one exercise |
| `sat_solver_cdcl` · `sat-solver-cdcl` | **gap** |  | `y14-dpll-sat`. check models, not search order |
| `procedural_puzzles` · `procedural-puzzles` | not suited |  | puzzle demos |

### Data structures and algorithms / Sorting, strings, bits and theory

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `sorting_searching` · `sorting-searching` | not suited | `s3-keep-sorted` | DSA area and std sorting problems |
| `sorting_networks_external_sort` · `sorting-networks-external-sort` | **gap** |  | `y12-external-merge-sort`. files simulated by a trait |
| `kway_merge_family` · `kway-merge-family` | **gap** |  | `y12-lsm-compaction-merge`. batch one also covers it: `s6-merge-sorted-streams` |
| `string_algos` · `string-algos` | not suited |  | DSA area |
| `aho_corasick` · `aho-corasick` | **gap** |  | `y10-aho-corasick` |
| `bit_tricks` · `bit-tricks` | practised | `f5-swar-memchr`, `f5-bitset-iter` |  |
| `bit_manipulation_real_world` · `bit-manipulation-real-world` | gap (partly practised) | `f2-tcp-flags-in-a-byte`, `f5-bit-packed-vec`, `f5-varint-codec` | `f5-snowflake-ids` |
| `algorithmic_optimization_toolbox` · `algorithmic-optimization-toolbox` | **gap** |  | `f1-hidden-quadratic` |
| `amortized_analysis_online_algorithms` · `amortized-analysis-online-algorithms` | not suited | `s3-predict-reallocations` | analysis, not code to grade |
| `approximation_randomized_algorithms` · `approximation-randomized-algorithms` | not suited |  | guarantees are statistical or need an exact solver |
| `linear_programming_simplex` · `linear-programming-simplex` | not suited |  | numerical method with little Rust content |

### Data structures and algorithms / Mathematics and numerics

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `bigint_arithmetic` · `bigint-arithmetic` | **gap** |  | `s8-biguint-add-mul` |
| `computational_geometry` · `computational-geometry` | not suited |  | DSA area |
| `decimal_fixed_point_money` · `decimal-fixed-point-money` | gap (partly practised) | `s1-order-total`, `s2-display-vs-debug` | `s8-split-the-bill` |
| `floating_point_ieee754` · `floating-point-ieee754` | **gap** |  | the float proposals above cover the practical part |
| `number_theory_primes` · `number-theory-primes` | not suited |  | DSA area |
| `numerics_and_math` · `numerics-and-math` | not suited |  | numerical methods, little Rust content |
| `physics_simulation` · `physics-simulation` | not suited |  | no exact oracle |
| `prng_zoo` · `prng-zoo` | **gap** |  | `s8-seedable-prng`. overlaps s8-seeded-sampling |
| `space_filling_curves_zorder_hilbert` · `space-filling-curves-zorder-hilbert` | **gap** |  | `y12-z-order-keys` |
| `spatial_search_structures` · `spatial-search-structures` | **gap** |  | `y16-kd-tree-nearest` |

### Concurrency and async / Threads, locks and channels

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `threads_channels` · `threads-channels` | gap (partly practised) | `l1-move-into-threads`, `l3-fix-thread-spawn-static`, `c2-bounded-channel` | `c2-worker-pipeline` |
| `mutex_condvar_channel` · `mutex-condvar-channel` | gap (partly practised) | `c1-blocking-queue`, `c1-fair-semaphore`, `c2-bounded-channel` | `c1-fix-lost-wakeup`. needs a probe hook as in c1-fix-transfer-deadlock |
| `shared_state` · `shared-state` | **gap** |  | `c2-actor-owns-state`. batch one also covers it: `c4-bounded-fan-out` |
| `sharded_map` · `sharded-map` | **gap** |  | `c1-sharded-map` |
| `locks_deadlocks` · `locks-deadlocks` | **gap** |  | `c1-fix-transfer-deadlock` **W** |
| `actor_model` · `actor-model` | **gap** |  | `c2-supervised-actor` |
| `crossbeam_select_channels` · `crossbeam-select-channels` | **gap** |  | `c2-select-pipeline` |
| `rayon_data_parallelism` · `rayon-data-parallelism` | **gap** |  | `c6-rayon-reduce` |
| `thread_pool_work_stealing` · `thread-pool-work-stealing` | gap (partly practised) | `c2-thread-pool` | `c6-work-stealing-deques` |
| `joinset_semaphore` · `joinset-semaphore` | gap (partly practised) | `c1-fair-semaphore` | `c4-bounded-fan-out` **W** |

### Concurrency and async / Atomics and memory ordering

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `atomics_ordering` · `atomics-ordering` | gap (partly practised) | `c3-atomic-stats`, `c3-once-cell`, `c3-spinlock` | `c3-fix-relaxed-publish`. a test cannot reliably show the bug without Miri or loom |
| `own_arc_refcount` · `own-arc-refcount` | gap (partly practised) | `c3-mini-arc` | `c3-arc-with-weak` |
| `seqlock_and_priority_inversion` · `seqlock-and-priority-inversion` | needs runner |  | needs: Miri or loom to grade the orderings. `c3-seqlock` |
| `false_sharing` · `false-sharing` | **gap** |  | `f6-cache-padded-counters`. grade the layout, not the speed |

### Concurrency and async / Concurrent data structures

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `spsc_ring_buffer` · `spsc-ring-buffer` | practised | `c3-spsc-ring` |  |
| `mpmc_bounded_queue` · `mpmc-bounded-queue` | practised | `c3-bounded-mpmc` |  |
| `mpmc_queues_michael_scott_and_combining` · `mpmc-queues-michael-scott-and-combining` | gap (partly practised) | `c3-bounded-mpmc` | `c3-two-lock-queue` |
| `lockfree_stack_aba` · `lockfree-stack-aba` | practised | `c3-index-stack` |  |
| `concurrent_counters_and_statistics` · `concurrent-counters-and-statistics` | gap (partly practised) | `c3-atomic-stats` | `f6-sharded-counter` |
| `concurrent_hashmap_designs` · `concurrent-hashmap-designs` | **gap** |  | the sharded map proposal covers the practical part |
| `concurrent_skiplist_btree_ordered_map` · `concurrent-skiplist-btree-ordered-map` | needs runner |  | needs: Miri or loom |
| `epoch_hazard_pointer_reclamation` · `epoch-hazard-pointer-reclamation` | needs runner | `c3-index-stack` | needs: Miri to grade use-after-free |
| `read_copy_update_and_hazard_patterns` · `read-copy-update-and-hazard-patterns` | **gap** |  | `f6-arcswap-config` |
| `snapshot_cow_rcu` · `snapshot-cow-rcu` | **gap** |  | `c1-cow-snapshots`. close to f6-arcswap-config |

### Concurrency and async / Async Rust

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `async_cancel_safety` · `async-cancel-safety` | **gap** |  | `c4-fix-cancel-safe-reader` **W** |
| `async_patterns` · `async-patterns` | **gap** |  | `c4-join-vs-spawn`. batch one also covers it: `c4-bounded-fan-out` |
| `async_streams_concurrency` · `async-streams-concurrency` | **gap** |  | `c4-ordered-buffered-stream` |
| `async_traits_dyn` · `async-traits-dyn` | gap (partly practised) | `c5-oneshot-channel` | `c5-dyn-async-trait` |
| `futures_by_hand` · `futures-by-hand` | practised | `c5-block-on`, `c5-oneshot-channel` |  |
| `waker_executor` · `waker-executor` | gap (partly practised) | `c5-mini-executor`, `c5-block-on` | `c5-timer-wheel-future` |
| `select_cancel` · `select-cancel` | **gap** |  | closed by batch one (cancel-safe reader, shutdown). batch one also covers it: `c4-fix-cancel-safe-reader`, `c4-drain-with-deadline` |
| `structured_concurrency_cancellation` · `structured-concurrency-cancellation` | **gap** |  | `c4-fail-fast-scope`. batch one also covers it: `c4-drain-with-deadline`, `c4-bounded-fan-out` |
| `graceful_shutdown` · `graceful-shutdown` | **gap** |  | `c4-drain-with-deadline` **W** |
| `bounded_channel` · `bounded-channel` | gap (partly practised) | `c2-bounded-channel` | `c4-fix-unbounded-backlog` |
| `worker_pool` · `worker-pool` | gap (partly practised) | `c2-thread-pool` | `c4-async-worker-pool`. batch one also covers it: `c4-drain-with-deadline` |

### Performance and systems / Measuring and CPU performance

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `perf_checklist` · `perf-checklist` | **gap** |  | `f1-allocation-hunt` |
| `branches_vectorization` · `branches-vectorization` | gap (partly practised) | `f5-swar-memchr` | `f5-vectorisable-filter`. [perf] asm |
| `inlining_codegen` · `inlining-codegen` | gap (partly practised) | `l5-inner-fn-monomorphization` | `f1-cold-path`. [perf] asm |
| `simd_intrinsics` · `simd-intrinsics` | gap (partly practised) | `f5-swar-memchr` | `f5-simd-count`. the owner's machine is aarch64, CI may be x86_64 |
| `matmul_optimization` · `matmul-optimization` | gap (partly practised) | `l4-matrix-operators` | `f5-tiled-matmul`. relative timing can flake |
| `fast_hashing` · `fast-hashing` | practised | `f4-open-addressing-map` | covered by f4-identity-hasher |

### Performance and systems / Memory layout and allocation

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `cache_locality` · `cache-locality` | practised | `f2-particles-as-arrays`, `f2-split-hot-from-cold` |  |
| `data_layout` · `data-layout` | practised | `f2-box-the-large-variant`, `f2-repack-a-submission-entry`, `f2-validity-bitmap-column` |  |
| `struct_packing` · `struct-packing` | practised | `f2-repack-a-submission-entry`, `f2-four-byte-optional-index`, `f2-shrink-a-price-level` |  |
| `small_string_inline` · `small-string-inline` | practised | `f2-inline-small-strings` |  |
| `counting_allocator` · `counting-allocator` | practised | `f2-freeze-shared-strings` | the s11 and f1 proposals add the by-hand version |
| `byte_order_keys` · `byte-order-keys` | gap (partly practised) | `f7-posting-list-codec` | `f7-memcomparable-keys` |
| `bufwriter` · `bufwriter` | **gap** |  | covered by s9-fix-unflushed-writer |

### Performance and systems / Files, processes and the OS

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `file_disk_io_patterns` · `file-disk-io-patterns` | needs runner | `y9-kv-store-wal` | needs: filesystem in tests (probably works in /tmp) |
| `file_locking_atomic_replace` · `file-locking-atomic-replace` | needs runner |  | needs: file locks need libc or std lock APIs and a filesystem. atomic replace is s9-atomic-save |
| `buffered_io_internals` · `buffered-io-internals` | **gap** |  | `s9-bufreader-from-scratch`. batch one also covers it: `s9-bounded-line-reader` |
| `mmap_files` · `mmap-files` | needs runner |  | needs: memmap2 not vendored |
| `io_uring_model` · `io-uring-model` | gap (partly practised) | `f2-repack-a-submission-entry` | `y16-submission-completion-rings` |
| `zero_copy_sendfile_splice` · `zero-copy-sendfile-splice` | needs runner |  | needs: syscalls and libc |
| `process_pipes_signals` · `process-pipes-signals` | needs runner |  | needs: processes and signals |
| `syscalls_portable` · `syscalls-portable` | needs runner |  | needs: libc |
| `virtual_memory_sim` · `virtual-memory-sim` | **gap** |  | `y16-page-table-tlb` |
| `scheduler_policies` · `scheduler-policies` | **gap** |  | `y16-mlfq-scheduler` |
| `disk_scheduling_raid` · `disk-scheduling-raid` | **gap** |  | `y16-raid5-rebuild` |
| `timing_wheel` · `timing-wheel` | **gap** |  | `y16-timing-wheel` |
| `civil_dates_time` · `civil-dates-time` | **gap** |  | `s10-civil-dates` |
| `wal_append_log` · `wal-append-log` | practised | `y9-kv-store-wal` |  |

### Performance and systems / High-performance computing

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `jacobi_stencil_tiling` · `jacobi-stencil-tiling` | **gap** |  | `c6-halo-exchange`. grade results, not speed |
| `loop_optimization_ladder` · `loop-optimization-ladder` | **gap** |  | `f5-eight-accumulators`. [perf] asm |
| `mpi_collectives_simulation` · `mpi-collectives-simulation` | **gap** |  | `c6-ring-allreduce`. count messages |
| `parallel_prefix_scan_reduction` · `parallel-prefix-scan-reduction` | **gap** |  | `c6-parallel-prefix-sum` |
| `parallel_sample_sort_radix` · `parallel-sample-sort-radix` | **gap** |  | `c6-parallel-radix-sort` |
| `scaling_laws_amdahl_gustafson_roofline` · `scaling-laws-amdahl-gustafson-roofline` | not suited |  | arithmetic and measurement, not a coding skill |
| `sparse_kernels_spmv_csr` · `sparse-kernels-spmv-csr` | **gap** |  | `f2-spmv-csr`. pairs with f2-csr-graph |

### Databases / Foundations and SQL

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `relational_algebra_engine` · `relational-algebra-engine` | **gap** |  | `y12-three-valued-filter` |
| `sql_window_functions_ctes` · `sql-window-functions-ctes` | **gap** |  | `y12-window-functions` |

### Databases / Storage and buffer management

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `slotted_pages_heap_file` · `slotted-pages-heap-file` | practised | `f2-slotted-page` |  |
| `storage_models_nsm_dsm_pax` · `storage-models-nsm-dsm-pax` | gap (partly practised) | `f2-particles-as-arrays` | `y12-pax-page` |
| `columnar_encodings` · `columnar-encodings` | gap (partly practised) | `f2-validity-bitmap-column`, `f5-bit-packed-vec`, `f7-posting-list-codec` | `y12-dictionary-rle-scan` |
| `buffer_pool_clock` · `buffer-pool-clock` | **gap** |  | `y12-buffer-pool-guards`. also in the BusTub course |
| `lru_k_replacer_disk_scheduler` · `lru-k-replacer-disk-scheduler` | **gap** |  | f4-lru-k above; disk scheduler is in the BusTub course |
| `buffer_management_vmcache_swizzling` · `buffer-management-vmcache-swizzling` | not suited |  | depends on virtual memory tricks and measurement |
| `lsm_compaction_strategies` · `lsm-compaction-strategies` | **gap** |  | `y12-compaction-picker` |

### Databases / Indexes

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `hash_and_bitmap_indexes` · `hash-and-bitmap-indexes` | **gap** |  | `y12-extendible-hashing` |
| `succinct_wavelet_fm_index` · `succinct-wavelet-fm-index` | **gap** |  | `f5-rank-select` |
| `database_cracking_zone_maps_imprints` · `database-cracking-zone-maps-imprints` | **gap** |  | `y12-zone-maps` |
| `bitweaving_bit_sliced_scans` · `bitweaving-bit-sliced-scans` | gap (partly practised) | `f5-swar-memchr` | `f5-bit-sliced-compare` |

### Databases / Query execution

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `volcano_vectorized_push` · `volcano-vectorized-push` | **gap** |  | `y12-volcano-operators` |
| `join_algorithms` · `join-algorithms-code` | **gap** |  | `y12-hash-and-merge-join` |
| `radix_partitioned_hash_join` · `radix-partitioned-hash-join` | **gap** |  | `y12-radix-partition`. counter-graded |
| `external_sort_hash_aggregation` · `external-sort-hash-aggregation` | **gap** |  | `y12-spilling-aggregation`. disk simulated by a trait |
| `parallel_query_morsel_exchange` · `parallel-query-morsel-exchange` | **gap** |  | `y12-morsel-driven-aggregate` |
| `query_compilation_data_centric_codegen` · `query-compilation-data-centric-codegen` | **gap** |  | `y12-compile-expr-to-closures` |
| `leapfrog_triejoin_worst_case_optimal` · `leapfrog-triejoin-worst-case-optimal` | **gap** |  | `y12-leapfrog-intersect` |
| `dremel_nested_columnar_shredding` · `dremel-nested-columnar-shredding` | **gap** |  | `y12-dremel-levels` |
| `distributed_query_shuffle_broadcast_join` · `distributed-query-shuffle-broadcast-join` | **gap** |  | `y12-choose-join-distribution` |

### Databases / Query optimization

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `cascades_memo_optimizer` · `cascades-memo-optimizer` | **gap** |  | `y12-memo-groups` |
| `join_enumeration_dpccp_dphyp` · `join-enumeration-dpccp-dphyp` | **gap** |  | `y12-join-order-dp` |
| `cardinality_estimation_lab` · `cardinality-estimation-lab` | not suited |  | estimator quality has no exact pass or fail |
| `statistics_histograms_sketches` · `statistics-histograms-sketches` | gap (partly practised) | `f4-count-min-sketch` | `y12-equi-depth-histogram` |
| `subquery_unnesting_decorrelation` · `subquery-unnesting-decorrelation` | not suited |  | too large for one exercise |
| `query_rewrite_rules_magic_sets` · `query-rewrite-rules-magic-sets` | **gap** |  | `y12-predicate-pushdown`. differential check against an evaluator |
| `parametric_query_optimization` · `parametric-query-optimization` | not suited |  | measurement study |
| `adaptive_feedback_reoptimization` · `adaptive-feedback-reoptimization` | not suited |  | measurement study |
| `adaptive_query_processing_eddies_sip` · `adaptive-query-processing-eddies-sip` | not suited |  | measurement study |
| `plan_cache_multiplan_trial` · `plan-cache-multiplan-trial` | **gap** |  | `y12-plan-cache-key` |

### Databases / Transactions and concurrency control

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `transaction_theory_serializability` · `transaction-theory-serializability` | **gap** |  | `y12-conflict-serializable` |
| `lock_manager_deadlock` · `lock-manager-deadlock` | **gap** |  | `y12-lock-manager-wait-die`. also in the BusTub course |
| `timestamp_ordering_occ` · `timestamp-ordering-occ` | **gap** |  | `y12-occ-validation` |
| `mvcc_snapshot_isolation` · `mvcc-snapshot-isolation` | **gap** |  | `y12-mvcc-snapshot-isolation` **W** |
| `mvcc_version_storage_gc` · `mvcc-version-storage-gc` | **gap** |  | `y12-delta-version-chains`. batch one also covers it: `y12-mvcc-snapshot-isolation` |
| `hybrid_logical_clock_snapshots_percolator` · `hybrid-logical-clock-snapshots-percolator` | **gap** |  | `y13-percolator-commit` |

### Databases / Logging and recovery

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `aries_recovery` · `aries-recovery` | gap (partly practised) | `y9-kv-store-wal` | `y12-aries-lite-recovery`. also in the BusTub course |
| `logging_schemes_group_commit` · `logging-schemes-group-commit` | **gap** |  | `y12-group-commit`. fsync injected as a trait |

### Databases / Analytics, time series and streaming

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `tsm_time_series_engine` · `tsm-time-series-engine` | **gap** |  | `y17-series-head-block` |
| `hypertables_continuous_aggregates` · `hypertables-continuous-aggregates` | **gap** |  | `y17-continuous-aggregate` |
| `asof_joins_gap_filling` · `asof-joins-gap-filling` | **gap** |  | `y17-asof-join` |
| `timeseries_sketches_downsampling_anomaly` · `timeseries-sketches-downsampling-anomaly` | **gap** |  | `y17-lttb-downsample` |
| `incremental_view_maintenance_dbsp` · `incremental-view-maintenance-dbsp` | **gap** |  | `y17-zset-incremental-join` |
| `datalog_engine_semi_naive` · `datalog-engine-semi-naive` | **gap** |  | `y17-semi-naive-datalog` |
| `delta_lake_transaction_log` · `delta-lake-transaction-log` | **gap** |  | `y17-table-log-occ` |
| `cloud_warehouse_micro_partitions` · `cloud-warehouse-micro-partitions` | **gap** |  | zone maps above cover the gradeable part |
| `cdc_logical_replication_log` · `cdc-logical-replication-log` | **gap** |  | `y17-cdc-commit-order` |
| `online_schema_change` · `online-schema-change` | not suited |  | an operational procedure |

### Databases / Building complete engines

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| (whole chapter) | needs runner |  | needs: multi-file or staged projects. the Courses engine (BusTub) already builds engines |

### Search and vectors / Text search and autocomplete

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `trigram_index` · `trigram-index` | practised | `y10-trigram-index` |  |
| `autocomplete_engine` · `autocomplete-engine` | **gap** |  | `y10-autocomplete-topk` |

### Search and vectors / Vector search

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `knn_bruteforce` · `knn-bruteforce` | **gap** |  | `y16-knn-topk` |
| `ivf_flat` · `ivf-flat` | **gap** |  | `y16-ivf-search`. grade recall against exact search with fixed data |
| `hnsw_lite` · `hnsw-lite` | **gap** |  | `y16-greedy-graph-search`. give the graph; grade the search |
| `vamana_diskann_lite` · `vamana-diskann-lite` | **gap** |  | `y16-robust-prune` |
| `spann_disk_ivf_lite` · `spann-disk-ivf-lite` | not suited |  | the disk model and its measurements are the point |
| `product_quantization` · `product-quantization` | **gap** |  | `y16-product-quantization` |
| `scalar_binary_quantization_rescoring` · `scalar-binary-quantization-rescoring` | **gap** |  | `y16-scalar-quantize-rescore` |
| `vector_distance_kernels_simd` · `vector-distance-kernels-simd` | **gap** |  | `y16-distance-kernels`. [perf] asm |
| `dimensionality_reduction_jl_pca` · `dimensionality-reduction-jl-pca` | not suited |  | statistical quality only |

### Search and vectors / Running a vector database

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `filtered_ann_hybrid_search` · `filtered-ann-hybrid-search` | gap (partly practised) | `y10-bm25-ranking` | `y16-rank-fusion` |
| `vector_index_updates_persistence` · `vector-index-updates-persistence` | **gap** |  | `y16-tombstones-and-rebuild` |
| `ann_evaluation_harness` · `ann-evaluation-harness` | **gap** |  | `y16-recall-at-k` |
| `mini_vector_database` · `mini-vector-database` | needs runner |  | needs: multi-file or staged project |

### Distributed systems / Time, partitioning and replication

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `consistent_hashing` · `consistent-hashing` | practised | `f4-consistent-hash-ring` |  |
| `hash_partitioning` · `hash-partitioning` | gap (partly practised) | `f4-consistent-hash-ring` | `y13-jump-and-rendezvous` |
| `vector_clocks_hlc` · `vector-clocks-hlc` | **gap** |  | `y13-vector-clocks`, `y13-hybrid-logical-clock` |
| `clock_sync_and_trace_context` · `clock-sync-and-trace-context` | **gap** |  | `b6-traceparent` |
| `quorum_replication_dynamo` · `quorum-replication-dynamo` | **gap** |  | `y13-quorum-read-repair` |
| `swim_gossip` · `swim-gossip` | **gap** |  | `y13-swim-membership` |
| `chandy_lamport_snapshot` · `chandy-lamport-snapshot` | **gap** |  | `y13-chandy-lamport` |

### Distributed systems / Consensus and distributed transactions

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `raft_consensus` · `raft-consensus` | practised | `y7-raft-vote-rules`, `y7-raft-append-entries`, `y7-raft-leader-commit` |  |
| `paxos_basic_and_multi` · `paxos-basic-and-multi` | **gap** |  | `y13-paxos-acceptor` |
| `two_phase_commit` · `two-phase-commit` | **gap** |  | `y13-two-phase-commit` |
| `saga_orchestrator` · `saga-orchestrator` | **gap** |  | `y13-saga-compensation` |
| `spanner_truetime_commit_wait` · `spanner-truetime-commit-wait` | **gap** |  | `y13-commit-wait`. fake clock |

### Distributed systems / Collaboration and CRDTs

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `crdts` · `crdts-code` | **gap** |  | `y13-or-set-crdt` **W**, `y13-pn-counter-lww` |
| `crdt_todo_app_no_db` · `crdt-todo-app-no-db` | **gap** |  | `y13-fractional-positions`. batch one also covers it: `y13-or-set-crdt` |
| `crdt_fugue_text_editor` · `crdt-fugue-text-editor` | **gap** |  | `y13-rga-sequence` |
| `crdt_block_editor_notion_like` · `crdt-block-editor-notion-like` | **gap** |  | `y13-movable-tree` |
| `operational_transform` · `operational-transform` | practised | `y9-ot-collaborative-text` |  |

### Distributed systems / Streams, messaging and reliability

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `kafka_log_model` · `kafka-log-model` | **gap** |  | `y13-consumer-group-offsets` |
| `record_batch_codec` · `record-batch-codec` | gap (partly practised) | `f5-varint-codec` | `f7-record-batch` |
| `stream_processing_windows` · `stream-processing-windows` | **gap** |  | `y17-watermark-windows` |
| `retry_backoff_jitter` · `retry-backoff-jitter` | **gap** |  | `b5-retry-with-budget` |
| `token_bucket` · `token-bucket` | **gap** |  | `b5-token-bucket` |

### Distributed systems / Microservice patterns

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `api_gateway_service_discovery` · `api-gateway-service-discovery` | **gap** |  | `b5-circuit-breaker` |
| `idempotency_dedup_leader_fencing_store` · `idempotency-dedup-leader-fencing-store` | **gap** |  | `b5-fencing-token-store`, `b5-idempotency-keys` |
| `strangler_fig_shadow_traffic` · `strangler-fig-shadow-traffic` | not suited |  | a migration procedure |
| `transactional_outbox_inbox_sqlite` · `transactional-outbox-inbox-sqlite` | needs runner |  | needs: SQLite not vendored. an in-memory version would lose the point |

### Web, networking and security / Web services

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `axum_errors` · `axum-errors` | **gap** |  | `b1-api-error-response` **W** |
| `axum_idempotency` · `axum-idempotency` | **gap** |  | covered by b5-idempotency-keys |
| `axum_middleware` · `axum-middleware` | **gap** |  | `b2-request-id-middleware`. batch one also covers it: `b2-fix-poll-ready` |
| `axum_router_state` · `axum-router-state` | **gap** |  | `b1-crud-with-state`. batch one also covers it: `b1-api-error-response` |
| `axum_sse_replay` · `axum-sse-replay` | **gap** |  | `b1-sse-replay` |
| `rest_api_patterns` · `rest-api-patterns` | **gap** |  | `b1-cursor-pagination` |
| `backend_request_validation_extractors` · `backend-request-validation-extractors` | **gap** |  | `b1-custom-extractor`. batch one also covers it: `b1-api-error-response` |
| `jsonrpc_graphql_dataloader` · `jsonrpc-graphql-dataloader` | **gap** |  | `b1-dataloader-batching` |
| `multipart_forms_uploads` · `multipart-forms-uploads` | **gap** |  | `b1-multipart-parser` |
| `webhooks_signatures` · `webhooks-signatures` | **gap** |  | `b5-fix-webhook-verify` **W** |
| `http_caching_conditional` · `http-caching-conditional` | **gap** |  | `b1-conditional-requests` |
| `http_client_resilience` · `http-client-resilience` | **gap** |  | covered by the retry and breaker proposals |
| `http_client_server_loopback` · `http-client-server-loopback` | needs runner | `b4-http1-request-parser` | needs: real sockets |
| `reverse_proxy_load_balancer` · `reverse-proxy-load-balancer` | **gap** |  | `b4-load-balancer-picker`. the proxying itself needs sockets |

### Web, networking and security / Authentication and web security

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `auth_basic_bearer_jwt` · `auth-basic-bearer-jwt` | **gap** |  | `b7-verify-jwt-hs256` |
| `oauth2_pkce_flow` · `oauth2-pkce-flow` | **gap** |  | `b7-pkce-code-exchange` |
| `cookies_sessions_csrf` · `cookies-sessions-csrf` | **gap** |  | `b7-session-cookie-csrf` |
| `cors_security_headers` · `cors-security-headers` | **gap** |  | `b7-fix-cors-reflection` |
| `backend_authz_rbac_abac_policies` · `backend-authz-rbac-abac-policies` | **gap** |  | `b7-authz-policy` |
| `backend_password_accounts_totp_mfa` · `backend-password-accounts-totp-mfa` | **gap** |  | `b7-totp`. TOTP's usual SHA-1 is not vendored; use the SHA-256 variant |
| `security_pitfalls_secure_coding` · `security-pitfalls-secure-coding` | **gap** |  | `b7-fix-injection-trio` |

### Web, networking and security / Networking and protocols

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `dns_message_codec` · `dns-message-codec` | **gap** |  | `b4-dns-name-compression` |
| `http1_parser` · `http1-parser` | practised | `b4-http1-request-parser` |  |
| `kqueue_epoll_raw` · `kqueue-epoll-raw` | needs runner |  | needs: libc and real sockets |
| `mio_event_loop` · `mio-event-loop` | needs runner |  | needs: mio not vendored |
| `packet_parsing_checksums` · `packet-parsing-checksums` | gap (partly practised) | `b4-tcp-reassembler` | `b4-ipv4-checksum` |
| `protobuf_grpc_wire_format` · `protobuf-grpc-wire-format` | gap (partly practised) | `f5-varint-codec` | `b4-protobuf-wire` |
| `resp_protocol` · `resp-protocol` | gap (partly practised) | `b4-length-prefixed-codec` | `b4-resp-incremental` |
| `tcp_congestion_control_sim` · `tcp-congestion-control-sim` | **gap** |  | `b4-reno-window` |
| `tcp_framed_server` · `tcp-framed-server` | needs runner | `b4-length-prefixed-codec` | needs: real sockets (or tokio duplex). `b4-framed-echo-over-duplex` |
| `tls_handshake_x509_der` · `tls-handshake-x509-der` | **gap** |  | `b7-der-reader` |
| `udp_sockets_reliable` · `udp-sockets-reliable` | **gap** |  | `b4-selective-repeat`. simulated channel |
| `websocket_frames` · `websocket-frames` | **gap** |  | `b4-websocket-frames` |

### Web, networking and security / Cryptography

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `aes_modes` · `aes-modes` | not suited |  | implementing ciphers teaches little Rust and invites misuse |
| `chacha20_poly1305` · `chacha20-poly1305` | not suited |  | same reason as aes_modes |
| `constant_time_side_channels` · `constant-time-side-channels` | needs runner |  | needs: timing and assembly guarantees cannot be graded. batch one also covers it: `b5-fix-webhook-verify` |
| `hash_based_signatures` · `hash-based-signatures` | not suited |  | niche |
| `hmac_hkdf_pbkdf2` · `hmac-hkdf-pbkdf2` | **gap** |  | `b7-hkdf-derive`. batch one also covers it: `b5-fix-webhook-verify` |
| `merkle_tree` · `merkle-tree` | **gap** |  | `b7-merkle-proofs` |
| `modular_arithmetic_dh_rsa` · `modular-arithmetic-dh-rsa` | not suited |  | mathematics more than Rust |
| `secure_random_and_passwords` · `secure-random-and-passwords` | **gap** |  | `b7-password-record`. argon2 is not vendored |

### Web, networking and security / Encodings and compression

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `compression_lz77_huffman` · `compression-lz77-huffman` | **gap** |  | `f7-lz77-roundtrip` |
| `encodings_codecs` · `encodings-codecs` | gap (partly practised) | `f5-varint-codec`, `f7-posting-list-codec` | `f7-fix-utf8-validator` |
| `timeseries_gorilla` · `timeseries-gorilla` | gap (partly practised) | `f5-bit-packed-vec` | `f7-gorilla-timestamps` |
| `binary_format_parsing` · `binary-format-parsing` | practised | `l3-binary-frame-parser`, `y6-wasm-binary-decoder` |  |
| `ndjson_serde` · `ndjson-serde` | **gap** |  | `f7-ndjson-tolerant` |

### Web, networking and security / Multi-file projects

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| crate `cb-backend-reference` | needs runner |  | needs: multi-file grading |

### Compilers and languages / Lexing and parsing

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `zero_copy_lexer` · `zero-copy-lexer` | practised | `l3-zero-copy-tokenizer` |  |
| `pratt_parser` · `pratt-parser` | **gap** |  | `y14-pratt-parser` **W** |
| `expression_parsing_three_ways` · `expression-parsing-three-ways` | **gap** |  | `y14-shunting-yard`. batch one also covers it: `y14-pratt-parser` |
| `parser_combinators` · `parser-combinators` | **gap** |  | `y14-parser-combinators` |
| `parser_generators_lr_peg_earley` · `parser-generators-lr-peg-earley` | **gap** |  | `y14-packrat-peg` |
| `edge_dsl_parser` · `edge-dsl-parser` | gap (partly practised) | `y9-shell-line-parser` | `y14-recovering-parser` |
| `json_parser` · `json-parser` | **gap** |  | `y14-json-parser` |
| `csv_state_machine` · `csv-state-machine` | **gap** |  | `y14-fix-csv-states` |
| `markdown_block_parser` · `markdown-block-parser` | **gap** |  | `y14-markdown-blocks` |

### Compilers and languages / A tree-walking interpreter

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| (whole chapter) | **gap** |  | `y14-closures-interpreter` |

### Compilers and languages / Bytecode and virtual machines

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `bytecode_vm` · `bytecode-vm` | gap (partly practised) | `f2-eight-byte-instructions`, `y6-wasm-stack-interpreter` | `y14-expr-to-bytecode` |
| `wasm_binary_interpreter` · `wasm-binary-interpreter` | gap (partly practised) | `y6-wasm-binary-decoder`, `y6-wasm-stack-interpreter` | `y6-wasm-validator` |
| crate `cb-compiler` | needs runner |  | needs: multi-file grading |
| crate `cb-compiler-tools` | needs runner |  | needs: multi-file grading |

### Compilers and languages / Types and semantics

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `lambda_calculus_toolkit` · `lambda-calculus-toolkit` | **gap** |  | `y14-de-bruijn-substitution` |
| `hindley_milner_inference` · `hindley-milner-inference` | **gap** |  | `y14-unify-types` |
| `system_f_bidirectional_typing` · `system-f-bidirectional-typing` | **gap** |  | `y14-bidirectional-check` |
| `dependent_types_nbe` · `dependent-types-nbe` | not suited |  | too large and too niche for one exercise |
| `pattern_match_compilation_decision_trees` · `pattern-match-compilation-decision-trees` | **gap** |  | `y14-exhaustiveness` |
| `algebraic_effects_handlers` · `algebraic-effects-handlers` | not suited |  | language research |
| `term_rewriting_unification_logic_programming` · `term-rewriting-unification-logic-programming` | **gap** |  | y14-unify-types covers unification |

### Compilers and languages / Middle end and analysis

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `dataflow_analysis_framework` · `dataflow-analysis-framework` | **gap** |  | `y15-liveness-worklist` |
| `abstract_interpretation_intervals` · `abstract-interpretation-intervals` | **gap** |  | `y15-interval-domain` |
| `alias_analysis_points_to` · `alias-analysis-points-to` | **gap** |  | `y15-andersen-points-to` |
| `egraph_equality_saturation` · `egraph-equality-saturation` | **gap** |  | `y15-congruence-closure` |
| `loop_optimizations` · `loop-optimizations` | **gap** |  | `y15-natural-loops-licm` |
| `symbolic_execution_smt_lite` · `symbolic-execution-smt-lite` | not suited |  | too large |
| `cps_closure_conversion_lambda_lifting` · `cps-closure-conversion-lambda-lifting` | **gap** |  | `y15-closure-conversion` |

### Compilers and languages / Back ends

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `ssa_register_allocation` · `ssa-register-allocation` | **gap** |  | `y15-linear-scan` |

### Compilers and languages / Language tooling and production

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| (whole chapter) | **gap** |  | `y15-pretty-printer` |

### Compilers and languages / Multi-file projects

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| (whole chapter) | not suited |  | no recipes of its own |

### Complete projects / Capstones

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `event_sourced_ledger` · `event-sourced-ledger` | needs runner |  | needs: staged or multi-file project. the core slice is m1-replay-events |
| `job_service_axum` · `job-service-axum` | needs runner |  | needs: staged or multi-file project. batch one also covers it: `b1-api-error-response`, `c4-drain-with-deadline` |
| `lease_fencing_sim` · `lease-fencing-sim` | needs runner |  | needs: staged project; the core is b5-fencing-token-store |
| `mini_async_runtime` · `mini-async-runtime` | needs runner | `c5-mini-executor` | needs: mio and multi-file |
| `mini_http_server_thread_pool` · `mini-http-server-thread-pool` | needs runner | `c2-thread-pool`, `b4-http1-request-parser` | needs: real sockets |
| `mini_kv_store` · `mini-kv-store-code` | needs runner | `y9-kv-store-wal` | needs: staged project (docs/RUST_PRACTICE.md 7.3) |
| `mini_redis` · `mini-redis` | needs runner | `b4-length-prefixed-codec` | needs: real sockets, multi-file |
| `query_optimizer` · `query-optimizer` | needs runner |  | needs: staged project |
| `rate_limiter_lab` · `rate-limiter-lab` | **gap** |  | `b5-gcra-limiter`. the lab as a whole is a project |
| `ray_tracer` · `ray-tracer` | not suited |  | output is judged by eye |
| `regex_nfa_engine` · `regex-nfa-engine` | practised | `y8-regex-nfa-matcher` |  |
| `search_engine` · `search-engine` | gap (partly practised) | `y10-trigram-index`, `y10-bm25-ranking` | `y10-phrase-query` |
| `tiny_sql_engine` · `tiny-sql-engine` | needs runner |  | needs: staged project. batch one also covers it: `y14-pratt-parser` |
| `toy_filesystem` · `toy-filesystem` | **gap** |  | `y16-inode-block-map`. the whole filesystem is a project |
| `toy_language_interpreter` · `toy-language-interpreter` | needs runner |  | needs: staged project |
| `typeahead_service` · `typeahead-service` | **gap** |  | y10-autocomplete-topk covers the gradeable core |

### Complete projects / Graphics

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `fractals_noise_automata` · `fractals-noise-automata` | not suited | `l2-collect-then-mutate` | output is judged by eye |
| `image_processing` · `image-processing` | **gap** |  | `f5-convolution-kernel` |
| `lines_curves_fills` · `lines-curves-fills` | **gap** |  | `s3-flood-fill-and-lines` |
| `png_ppm_writer` · `png-ppm-writer` | **gap** |  | `f7-png-chunks-crc` |
| `software_rasterizer` · `software-rasterizer` | not suited |  | output is judged by eye |
| `vec3_matrix_quaternion` · `vec3-matrix-quaternion` | practised | `l4-vec2-operators`, `l4-matrix-operators` |  |

### Worked questions / Rust concept answers

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `rust_async_qa` · `rust-async-qa` | not suited | `c5-block-on` | question-and-answer page; its topics are rows above. batch one also covers it: `c4-fix-cancel-safe-reader` |
| `rust_errors_iterators_closures_qa` · `rust-errors-iterators-closures-qa` | not suited |  | question-and-answer page; topics are rows above |
| `rust_ownership_borrowing_qa` · `rust-ownership-borrowing-qa` | not suited |  | question-and-answer page; L1-L3 practise it |
| `rust_smart_pointers_send_sync_qa` · `rust-smart-pointers-send-sync-qa` | not suited |  | question-and-answer page; topics are rows above |
| `rust_traits_generics_dispatch_qa` · `rust-traits-generics-dispatch-qa` | not suited |  | question-and-answer page; L4, L5 practise it |
| `rust_unsafe_memory_layout_qa` · `rust-unsafe-memory-layout-qa` | not suited |  | question-and-answer page; Y2, Y3, F2 practise it |

### Worked questions / Coding and design questions

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| `arrays_strings_hashmap_questions` · `arrays-strings-hashmap-questions` | not suited |  | DSA area |
| `concurrency_interview_questions` · `concurrency-interview-questions` | practised | `c1-blocking-queue`, `c1-rwlock-writer-preferring`, `c2-thread-pool` | single flight and dining philosophers are rows above |
| `graphs_and_union_find_questions` · `graphs-and-union-find-questions` | not suited |  | DSA area |
| `intervals_greedy_bit_questions` · `intervals-greedy-bit-questions` | not suited |  | DSA area |
| `stacks_queues_heaps_questions` · `stacks-queues-heaps-questions` | not suited |  | DSA area |
| `tries_advanced_questions` · `tries-advanced-questions` | not suited |  | DSA area |
| `tries_core_questions` · `tries-core-questions` | not suited |  | DSA area |
| `linked_lists_in_rust` · `linked-lists-in-rust` | practised | `l1-box-linked-stack`, `l2-mut-cursor-loop`, `y2-unsafe-linked-list` |  |
| `binary_trees_in_rust` · `binary-trees-in-rust` | **gap** |  | `s7-tree-with-weak-parents` |
| `implement_std_from_scratch` · `implement-std-from-scratch` | practised | `s1-my-option`, `s3-minivec-core`, `s3-minivec-deref`, `y2-mini-vec`, `c3-mini-arc` |  |
| `systems_design_in_code` · `systems-design-in-code` | not suited |  | sixteen designs; the gradeable cores are rows above |

### Appendices / Courses, reading and glossary

| Concept file · page | Status | Practised by | Proposed / why |
|---|---|---|---|
| (whole chapter) | not suited |  | reading lists and glossaries |

## Guide pages (the concepts that have no example file)

Judged by title and headings. P practised, R runnable gap, N needs runner or a staged project, X not suited.

### Rust language / Everyday language

| Page | | Practice |
|---|---|---|
| `language-features-from-rust` | X | survey page; its topics are recipe rows |

### Rust language / Memory and allocators

| Page | | Practice |
|---|---|---|
| `concurrent-memory` | R | `c3-seqlock`, `f3-bump-allocator`; epochs need Miri |
| `garbage-collection` | R | `f3-mark-sweep-heap` |
| `graph-algorithms-gc-and-life-without-gc` | R | `f3-mark-sweep-heap`, `y15-liveness-worklist` |

### Rust language / Unsafe and FFI

| Page | | Practice |
|---|---|---|
| `rust-for-systems-code` | N | Miri and hardware measurements |

### Testing and benchmarking / Methods: test design and TDD

| Page | | Practice |
|---|---|---|
| `toolchain` | N | loom, nightly and workspace setup; its model-based tests are `y5-catch-the-broken-queue` |

### Testing and benchmarking / Methods: fuzzing

| Page | | Practice |
|---|---|---|
| `tooling` | R | `y5-fuzz-by-hand` |

### Testing and benchmarking / Methods: benchmarking and profiling

| Page | | Practice |
|---|---|---|
| `benchmarking` | R | `f1-benchmark-that-measures-nothing` |
| `profiling-and-observability` | R | `f1-allocation-hunt` |

### Testing and benchmarking / Systems: distributed systems

| Page | | Practice |
|---|---|---|
| `deterministic-simulation` | R | `y5-simulate-the-network` |

### Testing and benchmarking / Systems: databases

| Page | | Practice |
|---|---|---|
| `testing-a-database` | R | `y5-differential-query-test`, `y5-linearizability-check` |

### Testing and benchmarking / Systems: compilers and languages

| Page | | Practice |
|---|---|---|
| `testing-and-debugging-compilers` | R | `y5-reduce-a-crashing-program` |
| `benchmarking-compilers` | X | tools and methodology |

### Data structures and algorithms / Hashing and probabilistic structures

| Page | | Practice |
|---|---|---|
| `count-min-sketch` | P | `f4-count-min-sketch` |
| `more-primers` | R | `f4-hyperloglog`, `f4-skiplist-memtable`, `y13-or-set-crdt` (batch one); copy-on-write trie: `y12-cow-trie` |
| `hash-tables` | R | `y12-extendible-hashing`; open addressing: `f4-open-addressing-map` |

### Data structures and algorithms / Dynamic programming, greedy and search

| Page | | Practice |
|---|---|---|
| `dp-trees-and-specialized-algorithms` | X | DSA area |
| `algorithms-and-concurrency-in-code` | R | `y12-join-order-dp`, `l8-did-you-mean` |

### Concurrency and async / Atomics and memory ordering

| Page | | Practice |
|---|---|---|
| `effects-async-and-memory-models` | X | theory |

### Performance and systems / Measuring and CPU performance

| Page | | Practice |
|---|---|---|
| `performance-data-layout-simd-threads-and-lock-free-structure` | X | survey; topics are recipe rows |

### Performance and systems / Files, processes and the OS

| Page | | Practice |
|---|---|---|
| `disk-i-o-fsync-the-page-cache-and-queues` | N | real disk measurements |
| `databases-on-new-hardware` | X | measurements |

### Databases / Foundations and SQL

| Page | | Practice |
|---|---|---|
| `relational-model-algebra` | R | `y12-three-valued-filter` |
| `modern-sql` | R | `y12-window-functions` (SQL itself is the SQL course) |
| `the-dbms-stack` | X | overview |
| `cairn-layout` | X | project layout |

### Databases / Storage and buffer management

| Page | | Practice |
|---|---|---|
| `buffer-pools` | R | `y12-buffer-pool-guards`, `f4-clock-eviction`; BusTub course |
| `arc-replacer` | R | `y12-arc-replacer`; BusTub course |
| `disk-i-o` | N | real files; BusTub course |
| `disk-scheduler` | N | threads over real files; BusTub course |
| `buffer-pool-manager` | R | `y12-buffer-pool-guards`; BusTub course |
| `pages-and-tuples` | P | `f2-slotted-page` |
| `table-heap` | R | `y12-halloween-update`; BusTub course |
| `log-structured-storage` | R | `y12-lsm-compaction-merge` |
| `storage-models-and-compression` | R | `y12-pax-page`, `y12-dictionary-rle-scan` |
| `in-memory-dbms-design` | X | design discussion |
| `larger-than-memory-databases` | X | design discussion |
| `columnar-formats-and-nested-data` | R | `y12-dremel-levels`, `y12-pax-page` |
| `modern-encodings-btrblocks-fastlanes-bitweaving` | R | `f5-bit-sliced-compare`, `y12-dictionary-rle-scan` |

### Databases / Indexes

| Page | | Practice |
|---|---|---|
| `b-trees` | R | `y12-bplus-tree-insert`; BusTub course |
| `b-tree-pages` | P | `f2-slotted-page` (leaf page) |
| `search-and-insert` | R | `y12-bplus-tree-insert` |
| `delete-and-tombstones` | R | `y12-bplus-tree-delete` |
| `index-iterator` | R | `y12-bplus-range-iterator` |
| `filters-tries-inverted-and-vector-indexes` | P | `f4-bloom-filter`, `y10-trigram-index` |
| `index-concurrency-control` | N | latch protocols need loom to grade; BusTub course |
| `b-tree-concurrency` | N | same |
| `latch-free-indexes-and-the-bw-tree` | N | same |
| `tries-and-the-adaptive-radix-tree` | R | `f4-adaptive-radix-nodes`, `f7-memcomparable-keys` |

### Databases / Query execution

| Page | | Practice |
|---|---|---|
| `sorting-and-aggregation` | R | `y12-external-merge-sort`, `y12-spilling-aggregation` |
| `join-algorithms` | R | `y12-hash-and-merge-join` |
| `query-execution` | R | `y12-volcano-operators` |
| `catalog-types-expressions` | R | `y12-three-valued-filter` |
| `sql-front-end` | R | `y14-pratt-parser` (batch one) covers the expression part; `y12-sql-binder` |
| `access-method-executors` | R | `y12-volcano-operators`; BusTub course |
| `aggregation-and-nested-loop-joins` | R | `y12-volcano-operators`; BusTub course |
| `grace-hash-join-and-spilling` | R | `y12-grace-hash-join` |
| `sort-top-n-limit-window` | R | `y12-window-functions`, `y12-top-n` |
| `parallel-execution` | R | `y12-morsel-driven-aggregate` |
| `vectorized-execution-with-simd` | R | `f5-vectorisable-filter` |
| `query-compilation-and-code-generation` | R | `y12-compile-expr-to-closures` |
| `parallel-hash-joins` | R | `y12-radix-partition` |
| `parallel-sort-merge-joins` | R | `c6-parallel-radix-sort` |
| `multi-way-and-worst-case-optimal-joins` | R | `y12-leapfrog-intersect` |
| `server-side-logic-and-udfs` | X | measurement study |
| `execution-pipelines` | X | measurement study |
| `query-scheduling` | R | `y16-mlfq-scheduler` (related) |
| `dsls-and-sql-parsing` | R | `y14-pratt-parser` (batch one), `y14-parser-combinators` |

### Databases / Query optimization

| Page | | Practice |
|---|---|---|
| `query-planning-and-rewriting` | R | `y12-predicate-pushdown` |
| `cost-models-and-join-ordering` | R | `y12-join-order-dp`, `y12-equi-depth-histogram` |
| `optimizer-rules-and-the-engine` | R | `y12-predicate-pushdown` |
| `optimizer-architectures` | X | survey |
| `a-cascades-memo-in-rust` | R | `y12-memo-groups` |
| `plan-transformations-and-rule-soundness` | R | `y12-predicate-pushdown` |
| `join-ordering` | R | `y12-join-order-dp` |
| `parallel-search` | X | measurement study |
| `unnesting-subqueries` | X | too large for one exercise |
| `statistics` | R | `y12-equi-depth-histogram` |
| `cardinality-estimation` | X | no exact pass or fail |
| `cost-models` | X | calibration study |
| `adaptive-and-robust` | X | measurement study |
| `production-optimizers` | X | survey |

### Databases / Transactions and concurrency control

| Page | | Practice |
|---|---|---|
| `concurrency-control-theory` | R | `y12-conflict-serializable` |
| `two-phase-locking` | R | `y12-lock-manager-wait-die` |
| `timestamp-ordering-and-occ` | R | `y12-occ-validation` |
| `multi-version-concurrency-control` | P | `y12-mvcc-snapshot-isolation` (batch one) |
| `project-4-mvocc` | R | `y12-delta-version-chains`; BusTub course |
| `mvcc-protocols-compared` | R | `y12-mvto` |
| `mvcc-garbage-collection` | P | `y12-mvcc-snapshot-isolation` (watermark vacuum, batch one) |

### Databases / Logging and recovery

| Page | | Practice |
|---|---|---|
| `logging` | R | `y12-group-commit` |
| `crash-recovery-aries` | R | `y12-aries-lite-recovery` |
| `build-wal-and-aries` | R | `y12-aries-lite-recovery` |
| `recovery-for-main-memory-systems` | X | design comparison |

### Databases / Analytics, time series and streaming

| Page | | Practice |
|---|---|---|
| `modern-olap-systems` | X | survey |
| `time-series-databases` | R | `y17-series-head-block`, `y17-asof-join` |
| `streaming-and-incremental-views` | R | `y17-zset-incremental-join` |
| `lakehouses-and-table-formats` | R | `y17-table-log-occ` |
| `cloud-native-oltp` | X | model and measurements |
| `building-blocks` | R | `y12-dictionary-rle-scan` (related) |
| `learned-tuning-and-in-database-ml` | X | measurement study |
| `hardware-accelerated-databases` | X | measurement study |
| `query-languages` | R | `y14-pipe-compiler` |
| `embedded-engines` | R | `y12-cow-trie` |
| `industry-systems-and-where-next` | X | survey |
| `history-of-databases` | X | history |

### Databases / Building complete engines

| Page | | Practice |
|---|---|---|
| `one-key-space` | R | `f7-memcomparable-keys` |
| `transactions-on-the-key-space` | P | `y12-mvcc-snapshot-isolation` (batch one) |
| `a-column-store` | N | staged project |
| `a-vectorized-engine` | N | staged project |
| `a-bytecode-virtual-machine` | R | `y14-expr-to-bytecode` |
| `htap-one-table-two-engines` | N | staged project |
| `a-durable-lsm-tree` | N | staged project (core: `y12-lsm-compaction-merge`) |
| `the-graph-store` | N | staged project |
| `the-document-store` | N | staged project |
| `the-time-series-engine` | N | staged project |
| `full-text-search` | P | `y10-trigram-index`, `y10-bm25-ranking` |
| `streams-and-windows` | R | `y17-watermark-windows` |
| `one-planner-for-many-models` | N | staged project |
| `the-postgresql-wire-protocol` | R | `b4-pg-messages` |
| `the-engine-in-webassembly` | N | wasm target |
| `passwords-and-scram` | R | `b7-scram-exchange` |
| `roles-privileges-and-row-level-security` | R | `b7-authz-policy` |
| `passing-bustub-s-tests` | X | course notes |
| `networking-protocols` | R | `b4-pg-messages` |

### Search and vectors / Vector search

| Page | | Practice |
|---|---|---|
| `vector-databases` | R | `y16-knn-topk`, `y16-ivf-search` |

### Search and vectors / Running a vector database

| Page | | Practice |
|---|---|---|
| `the-vector-store` | N | staged project |

### Distributed systems / Time, partitioning and replication

| Page | | Practice |
|---|---|---|
| `distributed-databases-i` | R | `y13-jump-and-rendezvous` |
| `sharding` | R | `y13-jump-and-rendezvous` |

### Distributed systems / Consensus and distributed transactions

| Page | | Practice |
|---|---|---|
| `distributed-databases-ii` | R | `y13-two-phase-commit`, `y13-paxos-acceptor` |
| `raft` | P | `y7-raft-vote-rules`, `y7-raft-append-entries`, `y7-raft-leader-commit` |
| `distributed-transactions` | R | `y13-two-phase-commit`, `y13-percolator-commit` |

### Distributed systems / Collaboration and CRDTs

| Page | | Practice |
|---|---|---|
| `crdts` | P | `y13-or-set-crdt` (batch one) |

### Compilers and languages / Lexing and parsing

| Page | | Practice |
|---|---|---|
| `the-pipeline-and-where-tests-attach` | X | overview |
| `parsing-in-depth` | R | `y14-packrat-peg` |
| `fully-recoverable-parsers` | R | `y14-recovering-parser` |
| `recoverable-parsing-and-advanced-compiler-theory` | X | theory map |
| `rust-techniques` | R | `y14-json-parser`; zero-copy lexing: `l3-zero-copy-tokenizer` |
| `how-rustc-swc-and-oxc-handle-the-same-problems` | X | survey |

### Compilers and languages / A tree-walking interpreter

| Page | | Practice |
|---|---|---|
| `the-slice-loop` | X | course process |
| `fix-these-first-in-rlox` | X | notes on the owner's project |
| `project-layout` | N | multi-file |
| `test-harness` | R | `y5-golden-test-runner` |
| `arithmetic` | R | `y14-closures-interpreter` |
| `values-and-comparison` | R | `y14-closures-interpreter` |
| `statements-and-globals` | R | `y14-closures-interpreter` |
| `blocks-and-scope` | R | `y14-closures-interpreter` |
| `control-flow` | R | `y14-closures-interpreter` |
| `functions-and-closures` | R | `y14-closures-interpreter` |
| `resolver` | R | `y14-resolver` |
| `classes-and-inheritance` | N | staged project |

### Compilers and languages / Bytecode and virtual machines

| Page | | Practice |
|---|---|---|
| `from-jlox-to-clox` | X | overview |
| `workspace-and-shared-tests` | N | multi-file |
| `chunks-and-the-disassembler` | R | `y14-expr-to-bytecode` |
| `vm-and-the-pratt-compiler` | R | `y14-expr-to-bytecode` |
| `types-strings-and-interning` | P | `f3-string-interner` |
| `globals-locals-and-statements` | N | staged project |
| `jumps-and-control-flow` | R | `y14-jump-patching` |
| `calls-and-functions` | N | staged project |
| `closures-and-upvalues` | R | `y14-upvalues` |
| `classes-methods-and-super` | N | staged project |
| `optimization` | P | `f2-eight-byte-instructions` (related) |
| `vm-techniques-in-rust` | P | `f2-eight-byte-instructions`, `y6-wasm-stack-interpreter` |
| `clox-tooling` | N | multi-file |
| `webassembly-as-a-compilation-target` | N | needs a wasm runtime in tests |
| `jit-compilation` | N | Cranelift not vendored; executable memory |
| `runtime-systems-and-gc-implementation` | R | `f3-mark-sweep-heap` |

### Compilers and languages / Types and semantics

| Page | | Practice |
|---|---|---|
| `semantics-and-the-lambda-calculus` | R | `y14-de-bruijn-substitution` |
| `hindley-milner-type-inference` | R | `y14-unify-types` |
| `bidirectional-typing-and-dependent-types` | R | `y14-bidirectional-check` |
| `pattern-matching-compilation-and-exhaustiveness` | R | `y14-exhaustiveness` |
| `name-resolution-hygiene-and-modules` | R | `y14-resolver` |
| `implementing-rust-s-hardest-features` | X | survey |
| `formal-verification-and-mechanized-semantics` | X | needs verification tools |
| `compiling-functional-languages` | R | `y15-closure-conversion` |
| `metaprogramming-macros-and-staging` | P | `l10-fix-macro-hygiene` (batch one) |
| `designing-a-language` | X | design discussion |

### Compilers and languages / Middle end and analysis

| Page | | Practice |
|---|---|---|
| `designing-an-ir-in-rust` | P | `f2-eight-byte-instructions`, `f2-inline-edge-lists` |
| `dataflow-analysis-and-abstract-interpretation` | R | `y15-liveness-worklist`, `y15-interval-domain` |
| `dominators-loops-and-control-flow-structure` | R | `y15-natural-loops-licm` |
| `ssa-construction-and-destruction` | R | `y15-ssa-construction` |
| `scalar-optimizations` | R | `y15-constant-propagation` |
| `e-graphs-and-equality-saturation` | R | `y15-congruence-closure` |
| `loop-optimization-and-vectorization` | R | `y15-natural-loops-licm` |

### Compilers and languages / Back ends

| Page | | Practice |
|---|---|---|
| `backends-hir-mir-llvm-and-cranelift` | X | survey |
| `instruction-selection-scheduling-and-emission` | R | `y15-list-scheduling` |
| `register-allocation` | R | `y15-linear-scan` |
| `abis-object-files-linking-and-unwinding` | X | platform tools |
| `gpu-and-accelerator-compilation` | X | needs hardware |
| `security-hardening-and-sanitizers` | N | nightly sanitizers |

### Compilers and languages / Language tooling and production

| Page | | Practice |
|---|---|---|
| `rust-quality-error-messages` | R | `l8-caret-diagnostics` |
| `tooling-around-a-new-language` | X | survey |
| `tooling-in-code` | R | `y15-pretty-printer`; the LSP server needs processes |
| `production-compiler-architecture` | R | `y15-incremental-queries` |
| `reading-rustc-and-contributing-to-it` | X | contribution guide |

### Appendices / Courses, reading and glossary

| Page | | Practice |
|---|---|---|
| `how-to-use-this-guide` | X | appendix |
| `courses-and-books` | X | appendix |
| `tech-talks` | X | appendix |
| `reading-database-research` | X | appendix |
| `contributing-to-a-database` | X | appendix |
| `the-job-landscape` | X | appendix |
| `interviews-and-a-portfolio` | X | appendix |
| `reading-map` | X | appendix |
| `glossary` | X | appendix |
| `university-courses` | X | appendix |
| `reading-for-parts-i-and-ii` | X | appendix |
| `what-to-build-next` | X | appendix |
| `reading-map-from-front-end-to-back-end` | X | appendix |
| `how-to-use-part-iv` | X | appendix |
| `doing-pl-research` | X | appendix |
| `glossary-compilers` | X | appendix |

## Proposed problems

Source is the file under `cookbook/` (or the guide page) the idea comes from. All new statements, tests and code are to be written fresh.

### B1 HTTP services with axum

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `b1-api-error-response` | One error shape for the whole API | implement | medium | IntoResponse and rejections | `examples/web/axum_errors.rs` | axum, tokio, serde, serde_json, tower | **written (batch one)** |
| `b1-conditional-requests` | ETag, If-None-Match and If-Match | implement | medium | HTTP preconditions | `examples/web/http_caching_conditional.rs` | - | proposed |
| `b1-crud-with-state` | CRUD with shared state | implement | easy | extractors and State | `examples/web/axum_router_state.rs` | axum, tokio, serde, serde_json, tower | proposed |
| `b1-cursor-pagination` | Cursor pagination that survives inserts | implement | medium | pagination | `examples/web/rest_api_patterns.rs` | - | proposed |
| `b1-custom-extractor` | A validating extractor that reports every field | implement | hard | FromRequest | `examples/web/backend_request_validation_extractors.rs` | axum, tokio, serde, serde_json, tower | proposed |
| `b1-dataloader-batching` | Fix N+1 with a batching loader | implement | hard | request batching | `examples/web/jsonrpc_graphql_dataloader.rs` | tokio | proposed |
| `b1-multipart-parser` | Parse multipart/form-data in chunks | implement | hard | boundary parsing | `examples/web/multipart_forms_uploads.rs` | - | proposed |
| `b1-sse-replay` | Resume a stream from Last-Event-ID | implement | hard | SSE | `examples/web/axum_sse_replay.rs` | axum, tokio, tower, futures | proposed |

### B2 Tower & middleware

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `b2-fix-poll-ready` | Fix: the middleware that broke backpressure | fix the bug | hard | tower Service contract | `examples/patterns/tower_service_layer.rs` | tower, tokio | **written (batch one)** |
| `b2-request-id-middleware` | A request id on every response, errors too | implement | medium | axum from_fn | `examples/web/axum_middleware.rs` | axum, tokio, tower | proposed |
| `b2-timeout-retry-layer` | A retry layer with one overall budget | implement | hard | Layer and Service | `examples/patterns/tower_service_layer.rs` | tower, tokio | proposed |

### B4 Networking & protocols

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `b4-dns-name-compression` | DNS names with compression, loop-proof | implement | hard | binary parsing | `examples/networking/dns_message_codec.rs` | - | proposed |
| `b4-framed-echo-over-duplex` | A framed server over an in-memory stream | implement | medium | per-connection tasks | `examples/networking/tcp_framed_server.rs` | tokio, tokio-util | blocked by the runner |
| `b4-ipv4-checksum` | IPv4 and TCP headers with checksums | implement | medium | packet parsing | `examples/networking/packet_parsing_checksums.rs` | - | proposed |
| `b4-load-balancer-picker` | Power of two choices with health | implement | medium | load balancing | `examples/web/reverse_proxy_load_balancer.rs` | - | proposed |
| `b4-pg-messages` | Frame PostgreSQL wire messages | implement | medium | protocol framing | `page the-postgresql-wire-protocol` | - | proposed |
| `b4-protobuf-wire` | Protobuf wire format with unknown fields | implement | medium | schema evolution | `examples/networking/protobuf_grpc_wire_format.rs` | - | proposed |
| `b4-reno-window` | Reno's congestion window | implement | medium | congestion control | `examples/networking/tcp_congestion_control_sim.rs` | - | proposed |
| `b4-resp-incremental` | An incremental RESP parser | implement | medium | recursive framing | `examples/networking/resp_protocol.rs` | - | proposed |
| `b4-selective-repeat` | Reliable delivery over a lossy channel | implement | hard | ARQ | `examples/networking/udp_sockets_reliable.rs` | - | proposed |
| `b4-websocket-frames` | WebSocket frames: masking and fragments | implement | medium | framing | `examples/networking/websocket_frames.rs` | - | proposed |

### B5 Resilience

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `b5-circuit-breaker` | A circuit breaker with a half-open probe | implement | medium | circuit breaking | `examples/microservices/api_gateway_service_discovery.rs` | - | proposed |
| `b5-fencing-token-store` | A store that rejects a stale leader | implement | medium | fencing tokens | `examples/microservices/idempotency_dedup_leader_fencing_store.rs` | - | proposed |
| `b5-fix-webhook-verify` | Fix: a webhook check that lets forgeries in | fix the bug | medium | HMAC and replay | `examples/web/webhooks_signatures.rs` | sha2, hex | **written (batch one)** |
| `b5-gcra-limiter` | GCRA: one timestamp per key | implement | medium | rate limiting | `examples/capstones/rate_limiter_lab.rs` | - | proposed |
| `b5-idempotency-keys` | Idempotency keys under concurrent retries | implement | hard | idempotency | `examples/microservices/idempotency_dedup_leader_fencing_store.rs` | - | proposed |
| `b5-retry-with-budget` | Retries with jitter and a retry budget | implement | medium | backoff | `examples/distributed/retry_backoff_jitter.rs` | tokio | proposed |
| `b5-token-bucket` | A token bucket with an injected clock | implement | medium | rate limiting | `examples/distributed/token_bucket.rs` | - | proposed |

### B6 Observability & production

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `b6-capture-spans` | Prove your spans: a recording layer | implement | hard | tracing layers | `crates/cb-crate-tour/examples/tracing_spans_subscriber_layers.rs` | tracing, tracing-subscriber | proposed |
| `b6-fix-span-across-await` | Fix: logs filed under the wrong request | fix the bug | medium | instrument across await | `examples/dev_tools/tracing_logs.rs` | tracing, tracing-subscriber, tokio | proposed |
| `b6-latency-histogram` | A log-bucketed histogram with honest percentiles | implement | medium | histograms | `examples/dev_tools/metrics_prometheus_histograms_load_testing.rs` | - | proposed |
| `b6-traceparent` | Parse and propagate traceparent | implement | easy | W3C Trace Context | `examples/distributed/clock_sync_and_trace_context.rs` | - | proposed |

### B7 Web security

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `b7-authz-policy` | A policy that fails closed | implement | medium | authorisation | `examples/web/backend_authz_rbac_abac_policies.rs` | - | proposed |
| `b7-der-reader` | A strict DER reader | implement | hard | TLV parsing | `examples/networking/tls_handshake_x509_der.rs` | - | proposed |
| `b7-fix-cors-reflection` | Fix: CORS that reflects any origin | fix the bug | easy | CORS | `examples/web/cors_security_headers.rs` | - | proposed |
| `b7-fix-injection-trio` | Fix: traversal, injection and SSRF | fix the bug | medium | input trust | `examples/web/security_pitfalls_secure_coding.rs` | - | proposed |
| `b7-hkdf-derive` | Derive keys with HKDF | implement | medium | key derivation | `examples/cryptography/hmac_hkdf_pbkdf2.rs` | sha2 | proposed |
| `b7-merkle-proofs` | Merkle inclusion proofs | implement | medium | hash trees | `examples/cryptography/merkle_tree.rs` | sha2 | proposed |
| `b7-password-record` | A password record with salt and upgrade | implement | medium | credential storage | `examples/cryptography/secure_random_and_passwords.rs` | sha2, rand | proposed |
| `b7-pkce-code-exchange` | PKCE: bind the code to its requester | implement | medium | OAuth | `examples/web/oauth2_pkce_flow.rs` | sha2, base64 | proposed |
| `b7-scram-exchange` | Verify a SCRAM-SHA-256 exchange | implement | hard | challenge and response | `page passwords-and-scram` | sha2, base64 | proposed |
| `b7-session-cookie-csrf` | Sessions, cookie flags and CSRF tokens | implement | medium | web sessions | `examples/web/cookies_sessions_csrf.rs` | sha2, hex | proposed |
| `b7-totp` | TOTP with a clock window and no reuse | implement | medium | TOTP | `examples/web/backend_password_accounts_totp_mfa.rs` | sha2 | proposed |
| `b7-verify-jwt-hs256` | Verify a JWT without trusting its header | fix the bug | hard | token verification | `examples/web/auth_basic_bearer_jwt.rs` | sha2, base64, serde_json | proposed |

### C1 Threads & shared state

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `c1-cow-snapshots` | Readers that never wait: Arc snapshots | implement | medium | copy-on-write | `examples/concurrency/snapshot_cow_rcu.rs` | - | proposed |
| `c1-fix-lost-wakeup` | Fix: the worker that sleeps for ever | fix the bug | hard | Condvar protocol | `examples/concurrency/mutex_condvar_channel.rs` | - | proposed |
| `c1-fix-poisoned-registry` | Fix: one panic took the whole registry down | fix the bug | medium | poisoning | `examples/std_tour/std_sync_primitives_use_cases.rs` | - | proposed |
| `c1-fix-transfer-deadlock` | Fix: two locks, two orders | fix the bug | medium | lock ordering | `examples/concurrency/locks_deadlocks.rs` | - | **written (batch one)** |
| `c1-once-per-key` | Single flight: one load per key however many ask | implement | hard | Condvar, per-key state | `examples/std_tour/std_sync_primitives_use_cases.rs` | - | proposed |
| `c1-park-unpark-parker` | A one-token parker | implement | medium | thread::park | `examples/std_tour/std_thread_scope_local_park.rs` | - | proposed |
| `c1-sharded-map` | A sharded map whose shards do not block each other | implement | medium | lock striping | `examples/concurrency/sharded_map.rs` | - | proposed |

### C2 Message passing

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `c2-actor-owns-state` | Replace a shared mutex with an actor | implement | medium | actor with oneshot replies | `examples/concurrency/shared_state.rs` | - | proposed |
| `c2-select-pipeline` | Cancel, timeout and data in one select | implement | medium | crossbeam select | `examples/concurrency/crossbeam_select_channels.rs` | crossbeam | proposed |
| `c2-supervised-actor` | An actor that restarts after a panic | implement | hard | supervision | `examples/concurrency/actor_model.rs` | tokio | proposed |
| `c2-worker-pipeline` | A producer, workers and an ordered collector | implement | easy | mpsc, shutdown by drop | `examples/concurrency/threads_channels.rs` | - | proposed |

### C3 Atomics & lock-free

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `c3-arc-with-weak` | MiniArc plus Weak: upgrade with CAS | implement | hard | two counts | `examples/concurrency/own_arc_refcount.rs` | - | proposed |
| `c3-fix-relaxed-publish` | Fix: a Relaxed flag publishes stale data | fix the bug | hard | Release and Acquire | `examples/concurrency/atomics_ordering.rs` | - | proposed |
| `c3-seqlock` | A seqlock for tear-free snapshots | implement | hard | seqlock | `examples/concurrency/seqlock_and_priority_inversion.rs` | - | blocked by the runner |
| `c3-two-lock-queue` | The two-lock queue | implement | hard | head and tail locks | `examples/concurrency/mpmc_queues_michael_scott_and_combining.rs` | - | proposed |

### C4 Async & Tokio

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `c4-async-worker-pool` | An async worker pool that survives panics | implement | medium | JoinSet and bounded queue | `examples/async/worker_pool.rs` | tokio | proposed |
| `c4-bounded-fan-out` | At most N in flight | implement | medium | JoinSet bounded concurrency | `examples/concurrency/joinset_semaphore.rs` | tokio | **written (batch one)** |
| `c4-drain-with-deadline` | Shut down without losing work | implement | hard | graceful shutdown | `examples/async/graceful_shutdown.rs` | tokio, tokio-util | **written (batch one)** |
| `c4-fail-fast-scope` | A scope where one failure cancels the siblings | implement | hard | CancellationToken tree | `examples/concurrency/structured_concurrency_cancellation.rs` | tokio, tokio-util | proposed |
| `c4-fix-cancel-safe-reader` | Fix: select! is eating bytes | fix the bug | medium | cancellation safety | `examples/async/async_cancel_safety.rs` | tokio | **written (batch one)** |
| `c4-fix-non-send-future` | Fix: a guard held across await | fix the bug | medium | Send futures | `crates/cb-crate-tour/examples/tokio_runtime_tasks_channels_timers.rs` | tokio | proposed |
| `c4-fix-stalled-runtime` | Fix: who blocked the executor? | fix the bug | medium | blocking and guards across await | `crates/cb-crate-tour/examples/tokio_runtime_tasks_channels_timers.rs` | tokio | proposed |
| `c4-fix-unbounded-backlog` | Fix: the queue that ate the memory | fix the bug | medium | async backpressure | `examples/async/bounded_channel.rs` | tokio | proposed |
| `c4-join-vs-spawn` | join, spawn, timeouts and panic isolation | implement | medium | task structure | `examples/async/async_patterns.rs` | tokio | proposed |
| `c4-ordered-buffered-stream` | At most K in flight, results in order | implement | medium | streams | `examples/async/async_streams_concurrency.rs` | futures, tokio | proposed |

### C5 Async internals

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `c5-dyn-async-trait` | An async trait you can box | implement | hard | async fn in traits and dyn | `examples/async/async_traits_dyn.rs` | tokio | proposed |
| `c5-hand-written-timeout` | A Timeout future with pin projection | implement | hard | Pin and structural projection | `examples/unsafe_ffi/pin_projection.rs` | tokio, pin-project-lite | proposed |
| `c5-timer-wheel-future` | A timer future woken by one thread | implement | hard | wakers | `examples/async/waker_executor.rs` | - | proposed |

### C6 Data parallelism

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `c6-halo-exchange` | A stencil split across threads with halo exchange | implement | hard | domain decomposition | `examples/hpc/jacobi_stencil_tiling.rs` | - | proposed |
| `c6-parallel-prefix-sum` | A parallel prefix sum | implement | medium | two-pass scan | `examples/hpc/parallel_prefix_scan_reduction.rs` | rayon | proposed |
| `c6-parallel-radix-sort` | A parallel radix sort | implement | hard | histogram and scatter | `examples/hpc/parallel_sample_sort_radix.rs` | rayon | proposed |
| `c6-rayon-reduce` | Parallel reduce with a non-commutative combine | fix the bug | medium | rayon fold and reduce | `examples/concurrency/rayon_data_parallelism.rs` | rayon | proposed |
| `c6-ring-allreduce` | Ring all-reduce over channels | implement | hard | collectives | `examples/hpc/mpi_collectives_simulation.rs` | - | proposed |
| `c6-work-stealing-deques` | Work stealing with crossbeam deques | implement | hard | work stealing | `examples/concurrency/thread_pool_work_stealing.rs` | crossbeam | proposed |

### F1 Measure & read the machine

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `f1-allocation-hunt` | Many allocations per line, down to none | make it fit a budget | medium | allocation profiling | `examples/perf/perf_checklist.rs` | - | proposed |
| `f1-benchmark-that-measures-nothing` | Fix: a benchmark the optimiser deleted | fix the bug | medium | black_box, medians | `examples/perf/bench_harness.rs` | - | proposed |
| `f1-cold-path` | Move the error path out of the hot loop | make it fit a budget | hard | #[cold] and inlining | `examples/perf/inlining_codegen.rs` | - | proposed |
| `f1-hidden-quadratic` | Fix: fast on ten, stuck on a million | fix the bug | medium | complexity in real code | `examples/algorithms/algorithmic_optimization_toolbox.rs` | - | proposed |

### F2 Data layout

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `f2-csr-graph` | Compressed sparse rows: a graph in two arrays | make it fit a budget | medium | CSR layout | `examples/algorithms/csr_graph.rs` | - | proposed |
| `f2-spmv-csr` | Sparse matrix times vector | implement | medium | CSR kernels | `examples/hpc/sparse_kernels_spmv_csr.rs` | - | proposed |

### F3 Memory & allocation

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `f3-bump-allocator` | A bump allocator with alignment and checkpoints | implement | hard | Layout and alignment | `examples/memory/bump_arena_allocator.rs` | - | proposed |
| `f3-free-list-allocator` | Size classes, splitting and coalescing | implement | hard | free lists | `examples/memory/size_class_allocator.rs` | - | proposed |
| `f3-mark-sweep-heap` | A mark-and-sweep heap, checked by reachability | implement | hard | tracing GC | `examples/memory/garbage_collectors.rs` | - | proposed |
| `f3-object-pool` | A buffer pool that gives buffers back on drop | implement | medium | object pool and RAII | `examples/patterns/design_patterns_low.rs` | - | proposed |

### F4 Hashing & purpose-built structures

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `f4-adaptive-radix-nodes` | Node4 to Node256: an adaptive radix tree | implement | hard | ART | `examples/data_structures/adaptive_radix_tree.rs` | - | proposed |
| `f4-clock-eviction` | CLOCK eviction with pin counts | implement | medium | CLOCK | `examples/data_structures/cache_policy_zoo.rs` | - | proposed |
| `f4-compressed-radix-tree` | A compressed radix tree for routes | implement | medium | Patricia tree | `examples/data_structures/trie_radix.rs` | - | proposed |
| `f4-cuckoo-filter` | A cuckoo filter that can delete | implement | hard | cuckoo hashing | `examples/data_structures/cuckoo_hashing.rs` | - | proposed |
| `f4-hyperloglog` | Count distinct in 4 KB | implement | hard | HyperLogLog | `examples/data_structures/hyperloglog.rs` | - | proposed |
| `f4-identity-hasher` | A BuildHasher for integer keys, counter-graded | implement | medium | Hasher and BuildHasher | `examples/data_structures/hash_functions.rs` | - | proposed |
| `f4-lru-cache-arena` | An O(1) LRU cache without Rc or unsafe | implement | hard | index-linked list | `examples/data_structures/lru_cache.rs` | - | **written (batch one)** |
| `f4-lru-k` | LRU-K: resist a sequential scan | implement | hard | LRU-K | `examples/data_structures/cache_policy_zoo.rs` | - | proposed |
| `f4-minhash-near-duplicates` | Find near-duplicates with MinHash and bands | implement | medium | MinHash and LSH | `examples/data_structures/minhash_simhash_lsh.rs` | - | proposed |
| `f4-order-statistics-treap` | Rank and select in a treap | implement | hard | augmented tree | `examples/data_structures/treap_interval.rs` | - | proposed |
| `f4-ordered-index-map` | Insertion order with O(1) lookup | implement | medium | order-preserving map | `crates/cb-crate-tour/examples/indexmap_smallvec_arrayvec_collections.rs` | hashbrown | proposed |
| `f4-reservoir-sample` | Sample a stream of unknown length | implement | medium | reservoir sampling | `examples/data_structures/sampling.rs` | - | proposed |
| `f4-robin-hood-map` | Robin Hood probing with backward shift | implement | hard | probe distance | `examples/data_structures/open_addressing_map.rs` | - | proposed |
| `f4-skiplist-memtable` | A skip list memtable in an arena | implement | hard | skip list | `examples/data_structures/skiplist.rs` | - | proposed |

### F5 CPU-level tricks

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `f5-bit-sliced-compare` | Compare 64 values per word | implement | hard | bit slicing | `examples/database/bitweaving_bit_sliced_scans.rs` | - | proposed |
| `f5-convolution-kernel` | Convolution with edges handled | implement | medium | 2-D indexing | `examples/graphics/image_processing.rs` | - | proposed |
| `f5-eight-accumulators` | Why the float sum does not vectorise | make it fit a budget | hard | dependency chains | `examples/hpc/loop_optimization_ladder.rs` | - | proposed |
| `f5-rank-select` | Rank and select over a bit vector | implement | hard | succinct structures | `examples/database/succinct_wavelet_fm_index.rs` | - | proposed |
| `f5-roaring-containers` | Array or bitmap containers, chosen by density | implement | hard | Roaring | `examples/data_structures/bitset_rank_select_roaring.rs` | - | proposed |
| `f5-simd-count` | std::arch with a scalar fallback | implement | hard | intrinsics behind cfg | `examples/perf/simd_intrinsics.rs` | - | proposed |
| `f5-snowflake-ids` | Sortable ids: time, node and sequence in 64 bits | implement | medium | bit packing | `examples/algorithms/bit_manipulation_real_world.rs` | - | proposed |
| `f5-tiled-matmul` | Cache-blocked matrix multiply | make it fit a budget | hard | loop order and tiling | `examples/perf/matmul_optimization.rs` | - | proposed |
| `f5-vectorisable-filter` | A filter the compiler can vectorise | make it fit a budget | hard | bounds checks and SIMD | `examples/perf/branches_vectorization.rs` | - | proposed |

### F6 Concurrency performance

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `f6-arcswap-config` | Read-mostly config with arc-swap | implement | medium | RCU style | `examples/concurrency/read_copy_update_and_hazard_patterns.rs` | arc-swap | proposed |
| `f6-cache-padded-counters` | Counters that do not share a cache line | make it fit a budget | medium | repr(align) | `examples/perf/false_sharing.rs` | - | proposed |
| `f6-sharded-counter` | A sharded counter that scales | implement | medium | per-shard atomics | `examples/concurrency/concurrent_counters_and_statistics.rs` | - | proposed |

### F7 I/O & serialization

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `f7-custom-visitor` | A hand-written Deserialize with a Visitor | implement | hard | serde data model | `examples/encoding_compression/serde_custom.rs` | serde, serde_json | proposed |
| `f7-fix-utf8-validator` | Fix: a UTF-8 validator that accepts overlongs | fix the bug | hard | UTF-8 rules | `examples/encoding_compression/encodings_codecs.rs` | - | proposed |
| `f7-gorilla-timestamps` | Delta-of-delta timestamps | implement | hard | bit streams | `examples/encoding_compression/timeseries_gorilla.rs` | - | proposed |
| `f7-lz77-roundtrip` | LZ77 with a window, hostile input included | implement | hard | compression | `examples/encoding_compression/compression_lz77_huffman.rs` | - | proposed |
| `f7-memcomparable-keys` | Keys whose bytes sort like their values | implement | medium | order-preserving encoding | `examples/perf/byte_order_keys.rs` | - | proposed |
| `f7-ndjson-tolerant` | NDJSON: skip bad lines, report them | implement | medium | streaming serde | `examples/text_and_parsing/ndjson_serde.rs` | serde, serde_json | proposed |
| `f7-percent-and-base64` | Percent-encoding and base64 at a boundary | implement | easy | encoding rules | `crates/cb-crate-tour/examples/url_base64_hex_percent_encoding.rs` | base64, hex | proposed |
| `f7-png-chunks-crc` | Write PNG chunks with CRC32 | implement | medium | binary formats | `examples/graphics/png_ppm_writer.rs` | - | proposed |
| `f7-record-batch` | A record batch with deltas and a checksum | implement | medium | batch codec | `examples/distributed/record_batch_codec.rs` | - | proposed |
| `f7-strict-config` | Strict config: defaults and typos rejected | implement | easy | default, deny_unknown_fields | `examples/encoding_compression/serde_defaults.rs` | serde, serde_json | proposed |
| `f7-toml-json-one-model` | One model, two formats | implement | medium | format differences | `crates/cb-crate-tour/examples/serde_formats_json_toml_yaml_bincode.rs` | serde, serde_json, toml | proposed |
| `f7-versioned-record-codec` | A versioned record format with checksums | implement | medium | Read/Write serialisation | `examples/std_tour/std_hash_collections_io_serialization_formats.rs` | - | proposed |
| `f7-wire-format-json` | JSON you do not control | implement | medium | serde attributes | `crates/cb-crate-tour/examples/serde_attributes_enum_representations.rs` | serde, serde_json | **written (batch one)** |
| `f7-zero-copy-frames-bytes` | Frames that share one buffer | make it fit a budget | medium | Bytes and BytesMut | `crates/cb-crate-tour/examples/bytes_buf_buf_mut_framing.rs` | bytes | proposed |

### L6 Closures & functional Rust

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `l6-fix-fnonce-twice` | Fix: a callback that can only run once | fix the bug | easy | FnOnce, FnMut, Fn | `examples/language/closures_hof.rs` | - | proposed |
| `l6-memoize-any-fn` | Memoise any function, recursion included | implement | medium | Fn traits, returning closures | `examples/language/closures_hof.rs` | - | proposed |

### L7 Enums & pattern matching

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `l7-fix-binding-modes` | Fix: patterns that move by accident | fix the bug | medium | match ergonomics, ref and @ bindings | `examples/language/ref_patterns.rs` | - | proposed |
| `l7-fix-wildcard-arm` | Fix: the wildcard that hid a new variant | fix the bug | medium | exhaustiveness | `examples/language/pattern_matching.rs` | - | proposed |
| `l7-slice-pattern-commands` | Parse commands with slice patterns and let-else | implement | easy | slice patterns, guards, let-else | `examples/language/pattern_matching.rs` | - | proposed |

### L8 Error design

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `l8-caret-diagnostics` | Point at the exact bytes | implement | medium | spans, line and column, carets | `examples/errors/diagnostics_spans.rs` | - | proposed |
| `l8-did-you-mean` | "Did you mean ...?" in an error message | implement | easy | bounded edit distance | `examples/algorithms/dp_edit_distance.rs` | - | proposed |
| `l8-fix-lost-context` | Fix: the report that forgot why | fix the bug | medium | anyhow context and downcasting | `examples/errors/anyhow_context.rs` | anyhow | **written (batch one)** |
| `l8-fix-stringly-errors` | Fix: Result<_, String> that callers cannot act on | fix the bug | medium | typed errors | `examples/errors/error_enum.rs` | thiserror | proposed |
| `l8-import-errors` | A library error that keeps its cause | implement | medium | thiserror, source chains | `examples/errors/error_chain_retryable.rs` | thiserror | **written (batch one)** |

### L9 Modules, crates & Cargo

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `l9-private-invariants` | Modules that make the invariant unbreakable | fix the bug | easy | visibility, re-exports | `examples/language/modules_visibility.rs` | - | proposed |
| `l9-semver-safe-api` | An API you can extend without breaking callers | implement | hard | non_exhaustive, sealed traits | `examples/language/library_api_polish.rs` | - | proposed |

### L10 Macros

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `l10-fix-macro-hygiene` | Fix: macros that surprise their caller | fix the bug | medium | repetition, hygiene, $crate | `examples/language/macro_rules.rs` | - | **written (batch one)** |
| `l10-tt-muncher` | A query! macro that munches tokens | implement | hard | recursive macros | `examples/language/macro_rules.rs` | - | proposed |

### M1 Idioms & patterns

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `m1-command-with-undo` | Commands that can undo themselves | implement | medium | command pattern | `examples/patterns/design_patterns_high.rs` | - | proposed |
| `m1-injected-strategy` | Swap the policy without touching the caller | implement | easy | strategy and dependency injection | `examples/patterns/design_patterns_high.rs` | - | proposed |
| `m1-order-state-machine` | Illegal transitions, rejected and tested | implement | medium | enum state machines | `examples/patterns/state_machine.rs` | - | proposed |
| `m1-piece-table-undo` | A piece table with undo | implement | hard | persistent edits | `examples/patterns/undo_redo_editors.rs` | - | proposed |
| `m1-replay-events` | Rebuild state by replaying a log | implement | medium | event sourcing with a state trait | `examples/patterns/generic_state_machine.rs` | - | proposed |
| `m1-typed-event-bus` | An event bus keyed by TypeId | implement | medium | dyn Any dispatch | `examples/patterns/any_event_bus.rs` | - | proposed |
| `m1-typestate-protocol` | A protocol client whose misuse does not type-check | implement | hard | typestate | `examples/patterns/typestate_protocol.rs` | - | proposed |

### S2 Strings & text

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s2-regex-log-parser` | Parse logs with named captures, compiled once | implement | medium | regex | `crates/cb-crate-tour/examples/regex_text_processing.rs` | regex | proposed |

### S3 Vec & slices

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s3-flood-fill-and-lines` | Bresenham lines and flood fill | implement | easy | grid algorithms | `examples/graphics/lines_curves_fills.rs` | - | proposed |
| `s3-select-nth-percentiles` | Percentiles without sorting everything | make it fit a budget | medium | select_nth_unstable, binary_search_by_key | `examples/std_tour/std_slice_search_sort_binary.rs` | - | proposed |

### S5 Queues & heaps

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s5-binary-heap-from-scratch` | A binary heap: sift up, sift down, heapify | implement | medium | heap invariants | `examples/data_structures/heap_patterns.rs` | - | proposed |
| `s5-heap-with-reverse` | A scheduler on BinaryHeap with a deterministic tie-break | implement | medium | Ord for heap entries, Reverse | `examples/std_tour/std_cmp_hash_default_traits.rs` | - | proposed |
| `s5-indexed-heap` | An indexed heap with decrease-key | implement | hard | sift up and down with a position map | `examples/data_structures/priority_queue_zoo.rs` | - | proposed |
| `s5-lazy-deletion-heap` | Cancel jobs in a heap without decrease-key | implement | medium | BinaryHeap, lazy deletion | `examples/std_tour/std_btree_binaryheap_ordered.rs` | - | proposed |
| `s5-pick-the-collection` | Fix: the right collection for the workload | fix the bug | medium | cost model | `examples/std_tour/std_collections_choice_guide.rs` | - | proposed |
| `s5-sliding-window-max` | Sliding window maximum with a VecDeque | implement | medium | VecDeque as a deque | `examples/std_tour/std_vec_vecdeque_methods.rs` | - | proposed |

### S6 Iterators

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s6-fix-side-effects-in-map` | Fix: the map that never ran | fix the bug | easy | laziness | `examples/std_tour/std_iterator_adapters_lazy.rs` | - | proposed |
| `s6-flatten-adapter` | Your own Flatten, from both ends | implement | hard | DoubleEndedIterator | `examples/language/iterator_adapters.rs` | - | proposed |
| `s6-itertools-grouping` | Group, merge and chunk with itertools | implement | easy | itertools | `crates/cb-crate-tour/examples/itertools_adaptors_cookbook.rs` | itertools | proposed |
| `s6-lazy-permutations` | Permutations as a lazy iterator | implement | medium | generator state machine | `examples/algorithms/combinatorial_generation.rs` | - | proposed |
| `s6-merge-sorted-streams` | A lazy k-way merge | implement | hard | writing an Iterator | `examples/language/iterator_adapters.rs` | - | **written (batch one)** |
| `s6-paginated-source` | Turn a paged API into an iterator | implement | medium | from_fn, successors, FusedIterator | `examples/std_tour/std_iterator_sources_custom.rs` | - | proposed |
| `s6-pipeline-no-collect` | One pass, no intermediate Vec | make it fit a budget | medium | iterator pipelines | `examples/language/iterators.rs` | - | proposed |
| `s6-single-pass-stats` | try_fold: stats that stop at the first bad record | implement | medium | consumers and short-circuiting | `examples/std_tour/std_iterator_consumers.rs` | - | proposed |
| `s6-tree-iter-no-alloc` | Walk a tree without recursion | implement | hard | stateful iterators | `examples/std_tour/std_iterator_sources_custom.rs` | - | proposed |

### S7 Smart pointers & interior mutability

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s7-copy-on-write-config` | Snapshot a config with Arc::make_mut | implement | medium | clone on write | `examples/std_tour/std_smart_pointers_use_cases.rs` | - | proposed |
| `s7-fix-observer-leak` | Fix: dropped, but still listening | fix the bug | medium | Rc cycles and Weak | `examples/language/smart_pointers.rs` | - | **written (batch one)** |
| `s7-pick-the-pointer` | Fix: Arc<Mutex<..>> everywhere | fix the bug | medium | cheapest correct pointer | `examples/std_tour/std_smart_pointers_use_cases.rs` | - | proposed |
| `s7-tree-with-weak-parents` | A tree with parent links that does not leak | implement | medium | Rc, RefCell, Weak | `examples/interview/binary_trees_in_rust.rs` | - | proposed |

### S8 The core traits

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s8-biguint-add-mul` | Big integers: carries in u64 | implement | medium | limb arithmetic | `examples/math_science/bigint_arithmetic.rs` | - | proposed |
| `s8-checked-conversions` | TryFrom at the boundary: no silent truncation | implement | medium | From, TryFrom, AsRef, Borrow, as casts | `examples/language/conversions.rs` | - | proposed |
| `s8-fix-silent-overflow` | Fix: sizes that wrap in release | fix the bug | medium | checked, saturating, wrapping, as | `examples/std_tour/std_integer_methods_overflow.rs` | - | proposed |
| `s8-fix-sort-floats` | Fix: NaN broke the sort | fix the bug | easy | total_cmp | `examples/std_tour/std_float_methods_total_order.rs` | - | proposed |
| `s8-human-sizes` | Parse and print 1.5GiB without losing bytes | implement | medium | checked parsing | `examples/std_tour/std_num_parsing_formatting.rs` | - | proposed |
| `s8-lazy-registry` | A registry built once on first use | implement | medium | OnceLock, LazyLock, thread_local | `examples/language/static_const_lazy.rs` | - | proposed |
| `s8-ord-eq-hash-key` | A map key whose Eq, Ord, Hash and Borrow agree | implement | medium | core trait consistency | `examples/language/common_traits.rs` | - | proposed |
| `s8-seedable-prng` | A seedable generator behind a trait | implement | easy | wrapping arithmetic | `examples/math_science/prng_zoo.rs` | - | proposed |
| `s8-seeded-sampling` | Reproducible randomness behind a trait | implement | medium | rand with a seed | `crates/cb-crate-tour/examples/uuid_rand_ids_tokens.rs` | rand, rand_chacha, uuid | proposed |
| `s8-split-the-bill` | Money without floats: rounding and allocation | implement | medium | fixed point | `examples/math_science/decimal_fixed_point_money.rs` | - | proposed |
| `s8-stable-float-sum` | A sum that does not depend on the order | implement | medium | compensated summation | `examples/std_tour/std_float_methods_total_order.rs` | - | proposed |

### S9 I/O & filesystem

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s9-atomic-save` | Save a file so a crash never leaves half of it | implement | medium | temp file, fsync, rename | `examples/language/io_files_paths.rs` | - | proposed |
| `s9-bounded-line-reader` | Lines from a stranger, in bounded memory | implement | medium | BufRead, bounded memory | `examples/std_tour/std_io_read_write_seek_traits.rs` | - | **written (batch one)** |
| `s9-bufreader-from-scratch` | Write BufReader | implement | medium | fill_buf and consume | `examples/os_and_storage/buffered_io_internals.rs` | - | proposed |
| `s9-filter-over-write` | A grep-like filter over Read and Write | implement | easy | generic IO, broken pipe | `examples/std_tour/std_io_stdio_terminal_text_processing.rs` | - | proposed |
| `s9-fix-path-traversal` | Fix: a join that escapes the root | fix the bug | medium | Path components | `examples/std_tour/std_path_pathbuf_ffi_strings.rs` | - | proposed |
| `s9-fix-unflushed-writer` | Fix: the report that lost its last lines | fix the bug | easy | BufWriter drop and flush | `examples/std_tour/std_bufreader_bufwriter_patterns.rs` | - | proposed |
| `s9-implement-read` | Chain, Take and a short-reading Read | implement | medium | implementing Read | `examples/std_tour/std_io_read_write_seek_traits.rs` | - | proposed |

### S10 Time, env & processes

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s10-catch-unwind-boundary` | Keep a panic inside its job | implement | medium | catch_unwind, payloads | `examples/std_tour/std_panic_process_exit_catch.rs` | - | proposed |
| `s10-civil-dates` | Days to dates and back | implement | medium | calendar arithmetic | `examples/os_and_storage/civil_dates_time.rs` | - | proposed |
| `s10-clap-subcommands` | A CLI surface tested with try_parse_from | implement | medium | clap derive | `crates/cb-crate-tour/examples/clap_derive_cli_tool.rs` | clap | proposed |
| `s10-injectable-clock-backoff` | Deadlines and backoff with a testable clock | implement | medium | Instant, Duration, injected time | `examples/std_tour/std_time_instant_duration_systemtime.rs` | - | proposed |
| `s10-layered-config` | Flags over environment over file | implement | easy | config precedence | `examples/std_tour/std_env_args_vars.rs` | - | proposed |
| `s10-recurring-schedule` | The next run, across month ends | implement | medium | calendar arithmetic | `crates/cb-crate-tour/examples/chrono_dates_times_zones.rs` | chrono | proposed |

### S11 mem, ptr & alloc

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `s11-counting-allocator` | A counting GlobalAlloc | implement | hard | GlobalAlloc and Layout | `examples/std_tour/std_alloc_layout_global_allocator.rs` | - | proposed |
| `s11-split-two-mut-raw` | Write split_at_mut yourself | implement | medium | raw slices and the safety contract | `examples/std_tour/std_ptr_nonnull_slices_raw.rs` | - | proposed |

### Y2 Unsafe Rust

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y2-hand-written-vtable` | Box<dyn Fn> by hand | implement | hard | vtables and raw pointers | `examples/patterns/type_erasure_vtable.rs` | - | proposed |
| `y2-my-refcell` | Your own RefCell on UnsafeCell | implement | hard | interior mutability | `examples/unsafe_ffi/unsafecell_cells.rs` | - | proposed |
| `y2-review-panic-safety` | Fix: this unsafe drops twice | fix the bug | hard | panic safety | `examples/unsafe_ffi/unsafe_basics.rs` | - | proposed |

### Y5 Testing & verification

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y5-catch-the-broken-queue` | Write the test that catches every broken queue | write the tests | medium | model-based testing | `examples/dev_tools/testing_patterns.rs` | - | **written (batch one)** |
| `y5-codec-properties` | Properties that break six codecs | write the tests | medium | proptest | `crates/cb-crate-tour/examples/criterion_proptest_insta_testing.rs` | proptest | proposed |
| `y5-differential-query-test` | Differential testing of a tiny query engine | write the tests | hard | oracles for databases | `examples/database/consistent_snapshots_sql_logic_tests.rs` | - | proposed |
| `y5-fake-the-clock` | Make it testable: inject the clock and the mailer | implement | medium | traits for test fakes | `examples/language/dyn_vs_generic.rs` | - | proposed |
| `y5-find-the-missed-bug` | Write the test the unit tests missed | write the tests | medium | boundary cases | `examples/dev_tools/testing_patterns.rs` | - | proposed |
| `y5-fuzz-by-hand` | Write a mutation fuzzer that finds the planted crash | write the tests | hard | fuzz loop with a corpus | `(chapter without example files)` | - | blocked by the runner |
| `y5-golden-test-runner` | A golden-file test runner that explains differences | implement | medium | golden tests | `page test-harness` | - | proposed |
| `y5-interleaving-explorer` | Explore every interleaving of two threads | implement | hard | stateless model checking | `examples/concurrency/model_checker_for_concurrent_code.rs` | - | proposed |
| `y5-linearizability-check` | Is this history linearizable? | implement | hard | Wing and Gong search | `examples/distributed/linearizability_checker.rs` | - | proposed |
| `y5-reduce-a-crashing-program` | Reduce a program that crashes the interpreter | implement | medium | test-case reduction | `(chapter without example files)` | - | proposed |
| `y5-shrink-failing-input` | Shrink a failing input | implement | medium | delta debugging | `examples/dev_tools/shrinking.rs` | - | **written (batch one)** |
| `y5-simulate-the-network` | Find the bug with a seeded network simulator | write the tests | hard | deterministic simulation | `examples/distributed/deterministic_simulation_testing.rs` | - | proposed |

### Y6 WebAssembly from scratch

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y6-wasm-validator` | Type-check a function body | implement | hard | stack validation | `examples/compilers_runtimes/wasm_binary_interpreter.rs` | - | proposed |

### Y10 Search engine core

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y10-aho-corasick` | Scan a log for a thousand patterns at once | implement | hard | automaton with failure links | `examples/algorithms/aho_corasick.rs` | - | proposed |
| `y10-autocomplete-topk` | Autocomplete with cached top-k | implement | hard | trie with cached results | `examples/search_vectors/autocomplete_engine.rs` | - | proposed |
| `y10-phrase-query` | Phrase queries with positional postings | implement | hard | positional index | `examples/capstones/search_engine.rs` | - | proposed |

### Y12 Database engine parts

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y12-arc-replacer` | The ARC replacement policy | implement | hard | adaptive eviction | `page arc-replacer` | - | proposed |
| `y12-aries-lite-recovery` | Analysis, redo, undo | implement | hard | ARIES | `examples/database/aries_recovery.rs` | - | proposed |
| `y12-bplus-range-iterator` | A range iterator that holds no latch between calls | implement | medium | batch per leaf | `page index-iterator` | - | proposed |
| `y12-bplus-tree-delete` | Delete from a B+ tree: borrow or merge | implement | hard | rebalancing | `page delete-and-tombstones` | - | proposed |
| `y12-bplus-tree-insert` | A B+ tree with splits and a leaf chain | implement | hard | B+ tree | `examples/data_structures/bplus_tree.rs` | - | proposed |
| `y12-buffer-pool-guards` | A buffer pool with pin guards | implement | hard | RAII pins and dirty write-back | `examples/database/buffer_pool_clock.rs` | - | proposed |
| `y12-bulk-load-and-separators` | Bulk load with shortest separators | implement | medium | bulk loading | `examples/data_structures/btree_bulk_load_prefix_compression.rs` | - | proposed |
| `y12-choose-join-distribution` | Shuffle or broadcast? | implement | medium | cost of data movement | `examples/database/distributed_query_shuffle_broadcast_join.rs` | - | proposed |
| `y12-compaction-picker` | Pick what to compact: tiered against levelled | implement | hard | amplification accounting | `examples/database/lsm_compaction_strategies.rs` | - | proposed |
| `y12-compile-expr-to-closures` | Compile a predicate to closures | implement | medium | closure compilation | `examples/database/query_compilation_data_centric_codegen.rs` | - | proposed |
| `y12-conflict-serializable` | Is this schedule conflict serializable? | implement | medium | precedence graph | `examples/database/transaction_theory_serializability.rs` | - | proposed |
| `y12-cow-trie` | A copy-on-write trie with cheap snapshots | implement | medium | structural sharing with Arc | `page more-primers` | - | proposed |
| `y12-delta-version-chains` | Version chains as deltas | implement | hard | version storage | `examples/database/mvcc_version_storage_gc.rs` | - | proposed |
| `y12-dictionary-rle-scan` | Scan dictionary and run-length data without decoding | implement | medium | encoded execution | `examples/database/columnar_encodings.rs` | - | proposed |
| `y12-dremel-levels` | Shred nested records into levels | implement | hard | repetition and definition levels | `examples/database/dremel_nested_columnar_shredding.rs` | - | proposed |
| `y12-equi-depth-histogram` | Equi-depth histograms and selectivity | implement | medium | statistics | `examples/database/statistics_histograms_sketches.rs` | - | proposed |
| `y12-extendible-hashing` | Extendible hashing: split one bucket at a time | implement | hard | directory doubling | `examples/database/hash_and_bitmap_indexes.rs` | - | proposed |
| `y12-external-merge-sort` | External merge sort with a page budget | implement | hard | runs and k-way merge | `examples/algorithms/sorting_networks_external_sort.rs` | - | proposed |
| `y12-gist-trait` | One tree, two index types through a trait | implement | hard | trait-generic index | `examples/data_structures/gist_generalized_search_tree.rs` | - | proposed |
| `y12-grace-hash-join` | Grace hash join with a memory budget | implement | hard | partition and recurse | `page grace-hash-join-and-spilling` | - | proposed |
| `y12-group-commit` | Group commit: one fsync for many | implement | hard | Condvar batching | `examples/database/logging_schemes_group_commit.rs` | - | proposed |
| `y12-halloween-update` | Fix: an update that visits its own output | fix the bug | medium | iterator stability | `page table-heap` | - | proposed |
| `y12-hash-and-merge-join` | Hash join and merge join, with duplicates | implement | medium | joins | `examples/database/join_algorithms.rs` | - | proposed |
| `y12-join-order-dp` | Join ordering by subset DP | implement | hard | DP over subsets | `examples/database/join_enumeration_dpccp_dphyp.rs` | - | proposed |
| `y12-leapfrog-intersect` | Leapfrog intersection of sorted lists | implement | hard | worst-case optimal joins | `examples/database/leapfrog_triejoin_worst_case_optimal.rs` | - | proposed |
| `y12-learned-index` | A piecewise-linear index with an error bound | implement | hard | learned index | `examples/data_structures/learned_index_rmi.rs` | - | proposed |
| `y12-lock-manager-wait-die` | A lock table with wait-die | implement | hard | deadlock prevention | `examples/database/lock_manager_deadlock.rs` | - | proposed |
| `y12-lsm-compaction-merge` | Compaction: newest wins, tombstones go at the bottom | implement | medium | LSM merge | `examples/algorithms/kway_merge_family.rs` | - | proposed |
| `y12-memo-groups` | A memo that shares subplans | implement | hard | memoisation in optimisers | `examples/database/cascades_memo_optimizer.rs` | - | proposed |
| `y12-morsel-driven-aggregate` | Morsels: parallel aggregation that balances itself | implement | hard | morsel parallelism | `examples/database/parallel_query_morsel_exchange.rs` | - | proposed |
| `y12-mvcc-snapshot-isolation` | Snapshot isolation over versioned keys | implement | hard | MVCC | `examples/database/mvcc_snapshot_isolation.rs` | - | **written (batch one)** |
| `y12-mvto` | Multi-version timestamp ordering | implement | hard | read timestamps | `page mvcc-protocols-compared` | - | proposed |
| `y12-occ-validation` | Optimistic concurrency: validate at commit | implement | hard | OCC | `examples/database/timestamp_ordering_occ.rs` | - | proposed |
| `y12-pax-page` | A PAX page: columns inside a page | implement | medium | storage layout | `examples/database/storage_models_nsm_dsm_pax.rs` | - | proposed |
| `y12-plan-cache-key` | Normalise SQL into a plan-cache key | implement | medium | normalisation | `examples/database/plan_cache_multiplan_trial.rs` | - | proposed |
| `y12-predicate-pushdown` | Push filters through joins without changing results | implement | hard | rewrite soundness | `examples/database/query_rewrite_rules_magic_sets.rs` | - | proposed |
| `y12-radix-partition` | Partition before you join | make it fit a budget | hard | cache-sized partitions | `examples/database/radix_partitioned_hash_join.rs` | - | proposed |
| `y12-spilling-aggregation` | Hash aggregation with a memory budget | implement | hard | spilling | `examples/database/external_sort_hash_aggregation.rs` | - | proposed |
| `y12-sql-binder` | Bind names to columns with precise errors | implement | medium | binding and scopes | `page sql-front-end` | - | proposed |
| `y12-three-valued-filter` | NULL logic in a filter evaluator | implement | medium | three-valued logic | `examples/database/relational_algebra_engine.rs` | - | proposed |
| `y12-top-n` | Top-N with a bounded heap | implement | easy | bounded heaps | `page sort-top-n-limit-window` | - | proposed |
| `y12-volcano-operators` | Pull-based operators over a trait | implement | medium | iterator model | `examples/database/volcano_vectorized_push.rs` | - | proposed |
| `y12-window-functions` | ROW_NUMBER, RANK and running sums | implement | medium | window frames | `examples/database/sql_window_functions_ctes.rs` | - | proposed |
| `y12-z-order-keys` | Z-order keys and range skipping | implement | hard | Morton codes | `examples/math_science/space_filling_curves_zorder_hilbert.rs` | - | proposed |
| `y12-zone-maps` | Skip blocks with min and max | implement | easy | zone maps | `examples/database/database_cracking_zone_maps_imprints.rs` | - | proposed |

### Y13 Distributed systems parts

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y13-chandy-lamport` | A consistent cut on FIFO channels | implement | hard | snapshots | `examples/distributed/chandy_lamport_snapshot.rs` | - | proposed |
| `y13-commit-wait` | Commit wait with an uncertain clock | implement | medium | TrueTime | `examples/distributed/spanner_truetime_commit_wait.rs` | - | proposed |
| `y13-consumer-group-offsets` | Offsets, rebalances and at-least-once | implement | hard | log consumption | `examples/distributed/kafka_log_model.rs` | - | proposed |
| `y13-fractional-positions` | List order that merges: fractional positions | implement | hard | list CRDT | `examples/distributed/crdt_todo_app_no_db.rs` | - | proposed |
| `y13-hybrid-logical-clock` | A hybrid logical clock | implement | medium | HLC | `examples/distributed/vector_clocks_hlc.rs` | - | proposed |
| `y13-jump-and-rendezvous` | Rendezvous and jump hashing | implement | medium | partitioning | `examples/distributed/hash_partitioning.rs` | - | proposed |
| `y13-movable-tree` | Moving nodes in a replicated tree without cycles | implement | hard | tree move CRDT | `examples/distributed/crdt_block_editor_notion_like.rs` | - | proposed |
| `y13-or-set-crdt` | A set that replicas can edit offline | implement | medium | OR-Set | `examples/distributed/crdts.rs` | - | **written (batch one)** |
| `y13-paxos-acceptor` | The Paxos acceptor and proposer rules | implement | hard | Paxos | `examples/distributed/paxos_basic_and_multi.rs` | - | proposed |
| `y13-percolator-commit` | Percolator: a primary lock decides | implement | hard | client-driven commit | `examples/database/hybrid_logical_clock_snapshots_percolator.rs` | - | proposed |
| `y13-pn-counter-lww` | Counters and last-writer-wins registers | implement | easy | merge laws | `examples/distributed/crdts.rs` | - | proposed |
| `y13-quorum-read-repair` | Quorum reads with read repair | implement | hard | N, R, W | `examples/distributed/quorum_replication_dynamo.rs` | - | proposed |
| `y13-rga-sequence` | An RGA text CRDT | implement | hard | sequence CRDT | `examples/distributed/crdt_fugue_text_editor.rs` | - | proposed |
| `y13-saga-compensation` | A saga that compensates after a crash | implement | medium | sagas | `examples/distributed/saga_orchestrator.rs` | - | proposed |
| `y13-swim-membership` | SWIM member states and incarnations | implement | hard | failure detection | `examples/distributed/swim_gossip.rs` | - | proposed |
| `y13-two-phase-commit` | 2PC with a log and crash recovery | implement | hard | atomic commit | `examples/distributed/two_phase_commit.rs` | - | proposed |
| `y13-vector-clocks` | Happened-before, or concurrent? | implement | medium | vector clocks | `examples/distributed/vector_clocks_hlc.rs` | - | proposed |

### Y14 Parsers & languages

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y14-bidirectional-check` | Bidirectional type checking | implement | hard | checking and synthesis | `examples/compilers_runtimes/system_f_bidirectional_typing.rs` | - | proposed |
| `y14-closures-interpreter` | An interpreter with lexical scope and closures | implement | hard | environments | `(chapter without example files)` | - | proposed |
| `y14-de-bruijn-substitution` | Substitution without capture | implement | hard | de Bruijn indices | `examples/compilers_runtimes/lambda_calculus_toolkit.rs` | - | proposed |
| `y14-dpll-sat` | DPLL with unit propagation | implement | hard | backtracking solver | `examples/algorithms/sat_solver_cdcl.rs` | - | proposed |
| `y14-exhaustiveness` | Which patterns are missing? | implement | hard | usefulness check | `examples/compilers_runtimes/pattern_match_compilation_decision_trees.rs` | - | proposed |
| `y14-expr-to-bytecode` | Compile expressions to a stack machine | implement | medium | code generation | `examples/text_and_parsing/bytecode_vm.rs` | - | proposed |
| `y14-fix-csv-states` | Fix: a CSV state machine that loses quotes | fix the bug | medium | state machines | `examples/text_and_parsing/csv_state_machine.rs` | - | proposed |
| `y14-json-parser` | A strict JSON parser with a depth limit | implement | medium | recursive descent | `examples/text_and_parsing/json_parser.rs` | - | proposed |
| `y14-jump-patching` | Compile if and while with backpatched jumps | implement | medium | backpatching | `page jumps-and-control-flow` | - | proposed |
| `y14-markdown-blocks` | A two-phase block parser | implement | hard | block structure | `examples/text_and_parsing/markdown_block_parser.rs` | - | proposed |
| `y14-packrat-peg` | A packrat PEG matcher | implement | hard | memoised parsing | `examples/compilers_runtimes/parser_generators_lr_peg_earley.rs` | - | proposed |
| `y14-parser-combinators` | Parser combinators from closures | implement | hard | combinators and lifetimes | `examples/text_and_parsing/parser_combinators.rs` | - | proposed |
| `y14-pipe-compiler` | Compile a pipe query to operators | implement | medium | lowering | `page query-languages` | - | proposed |
| `y14-pratt-parser` | A Pratt parser for expressions | implement | medium | binding powers | `examples/text_and_parsing/pratt_parser.rs` | - | **written (batch one)** |
| `y14-recovering-parser` | Report every error, not just the first | implement | hard | error recovery | `examples/text_and_parsing/edge_dsl_parser.rs` | - | proposed |
| `y14-resolver` | Resolve variables to scope depths | implement | medium | static resolution | `page resolver` | - | proposed |
| `y14-shunting-yard` | Shunting-yard without recursion | implement | medium | operator stacks | `examples/text_and_parsing/expression_parsing_three_ways.rs` | - | proposed |
| `y14-unify-types` | Unification with an occurs check | implement | hard | type inference | `examples/compilers_runtimes/hindley_milner_inference.rs` | - | proposed |
| `y14-upvalues` | Closures with upvalues in a tiny VM | implement | hard | captured variables | `page closures-and-upvalues` | - | proposed |

### Y15 Compiler analyses and back ends

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y15-andersen-points-to` | Andersen's points-to analysis | implement | hard | constraint solving | `examples/compilers_runtimes/alias_analysis_points_to.rs` | - | proposed |
| `y15-closure-conversion` | Closure conversion | implement | hard | free variables | `examples/compilers_runtimes/cps_closure_conversion_lambda_lifting.rs` | - | proposed |
| `y15-congruence-closure` | Congruence closure on an e-graph | implement | hard | union-find with hash-consing | `examples/compilers_runtimes/egraph_equality_saturation.rs` | - | proposed |
| `y15-constant-propagation` | Constant folding and dead code removal | implement | medium | scalar optimisation | `page scalar-optimizations` | - | proposed |
| `y15-incremental-queries` | A tiny incremental query system | implement | hard | memoised queries with revisions | `page production-compiler-architecture` | - | proposed |
| `y15-interval-domain` | Interval analysis with widening | implement | hard | abstract interpretation | `examples/compilers_runtimes/abstract_interpretation_intervals.rs` | - | proposed |
| `y15-linear-scan` | Linear-scan register allocation | implement | hard | live intervals | `examples/compilers_runtimes/ssa_register_allocation.rs` | - | proposed |
| `y15-list-scheduling` | List scheduling of instructions | implement | medium | dependence DAG | `page instruction-selection-scheduling-and-emission` | - | proposed |
| `y15-liveness-worklist` | Liveness with a worklist | implement | hard | dataflow | `examples/compilers_runtimes/dataflow_analysis_framework.rs` | - | proposed |
| `y15-natural-loops-licm` | Find loops and hoist invariants | implement | hard | dominators | `examples/compilers_runtimes/loop_optimizations.rs` | - | proposed |
| `y15-pretty-printer` | A width-aware pretty printer | implement | hard | layout algorithm | `(chapter without example files)` | - | proposed |
| `y15-ssa-construction` | Place phi nodes and rename | implement | hard | SSA | `page ssa-construction-and-destruction` | - | proposed |

### Y16 Vectors, search and OS models

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y16-distance-kernels` | Distance kernels the compiler vectorises | make it fit a budget | hard | autovectorisation | `examples/search_vectors/vector_distance_kernels_simd.rs` | - | proposed |
| `y16-greedy-graph-search` | Best-first search on a proximity graph | implement | hard | HNSW search | `examples/search_vectors/hnsw_lite.rs` | - | proposed |
| `y16-inode-block-map` | Direct and indirect blocks | implement | medium | file mapping | `examples/capstones/toy_filesystem.rs` | - | proposed |
| `y16-ivf-search` | IVF: probe the nearest lists | implement | hard | inverted file | `examples/search_vectors/ivf_flat.rs` | - | proposed |
| `y16-kd-tree-nearest` | Nearest neighbour with a k-d tree | implement | hard | pruned search | `examples/math_science/spatial_search_structures.rs` | - | proposed |
| `y16-knn-topk` | Exact nearest neighbours with a bounded heap | implement | medium | top-k | `examples/search_vectors/knn_bruteforce.rs` | - | proposed |
| `y16-mlfq-scheduler` | A multi-level feedback queue | implement | medium | scheduling | `examples/os_and_storage/scheduler_policies.rs` | - | proposed |
| `y16-page-table-tlb` | A page table with a TLB | implement | medium | address translation | `examples/os_and_storage/virtual_memory_sim.rs` | - | proposed |
| `y16-product-quantization` | Encode and score with lookup tables | implement | hard | PQ | `examples/search_vectors/product_quantization.rs` | - | proposed |
| `y16-raid5-rebuild` | RAID 5 parity: rebuild a lost disk | implement | easy | XOR parity | `examples/os_and_storage/disk_scheduling_raid.rs` | - | proposed |
| `y16-rank-fusion` | Reciprocal rank fusion | implement | easy | hybrid search | `examples/search_vectors/filtered_ann_hybrid_search.rs` | - | proposed |
| `y16-recall-at-k` | Recall@k that handles ties | implement | easy | evaluation | `examples/search_vectors/ann_evaluation_harness.rs` | - | proposed |
| `y16-robust-prune` | Robust pruning | implement | hard | graph construction | `examples/search_vectors/vamana_diskann_lite.rs` | - | proposed |
| `y16-scalar-quantize-rescore` | Quantise, oversample, rescore | implement | medium | quantisation | `examples/search_vectors/scalar_binary_quantization_rescoring.rs` | - | proposed |
| `y16-submission-completion-rings` | Submission and completion queues in safe Rust | implement | hard | ring protocol | `examples/os_and_storage/io_uring_model.rs` | - | proposed |
| `y16-timing-wheel` | A hierarchical timing wheel | implement | hard | timers | `examples/os_and_storage/timing_wheel.rs` | - | proposed |
| `y16-tombstones-and-rebuild` | Deletes in a vector index | implement | medium | tombstones | `examples/search_vectors/vector_index_updates_persistence.rs` | - | proposed |

### Y17 Analytics and streaming

| Id | Title | Kind | Level | Concept | Source | Crates | State |
|---|---|---|---|---|---|---|---|
| `y17-asof-join` | AS-OF join and gap filling | implement | medium | time alignment | `examples/database/asof_joins_gap_filling.rs` | - | proposed |
| `y17-cdc-commit-order` | Emit changes in commit order | implement | medium | logical decoding | `examples/database/cdc_logical_replication_log.rs` | - | proposed |
| `y17-continuous-aggregate` | A materialised rollup that stays correct | implement | medium | incremental aggregation | `examples/database/hypertables_continuous_aggregates.rs` | - | proposed |
| `y17-lttb-downsample` | Downsample a chart without losing the spikes | implement | medium | LTTB | `examples/database/timeseries_sketches_downsampling_anomaly.rs` | - | proposed |
| `y17-semi-naive-datalog` | Semi-naive transitive closure | implement | hard | fixpoint evaluation | `examples/database/datalog_engine_semi_naive.rs` | - | proposed |
| `y17-series-head-block` | A time series head block with a tag index | implement | hard | postings and blocks | `examples/database/tsm_time_series_engine.rs` | - | proposed |
| `y17-table-log-occ` | A table format commit with conflict detection | implement | medium | log with optimistic concurrency | `examples/database/delta_lake_transaction_log.rs` | - | proposed |
| `y17-watermark-windows` | Event-time windows with late data | implement | hard | watermarks | `examples/distributed/stream_processing_windows.rs` | - | proposed |
| `y17-zset-incremental-join` | Incremental join with z-sets | implement | hard | IVM | `examples/database/incremental_view_maintenance_dbsp.rs` | - | proposed |

